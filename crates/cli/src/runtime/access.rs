//! 两条取数路径。
//!
//! 一条命令要么只读/只改**本机持久化记录**（[`RecordAccess`]，**永不起节点**），
//! 要么需要一个**活着的节点**（[`NodeAccess`]），要么什么都不要。
//!
//! ## 为什么必须收在一处
//!
//! 此前这件事有两套并行的答案：设备列表走 `Session`（无常驻节点时起一个完整 P2P 节点，
//! 连引导节点 + NAT 探测，几秒），收件箱走手写的 `is_alive` + 直连数据库（秒回）。
//! 后者是对的，但它没有被抽象出来——于是**每加一条命令都要重做一次「我该不该起节点」的
//! 判断，而判断错了不报错**，只是白等几秒（或者更糟：把一条其实需要网络的命令标成不需要，
//! 表现为「命令跑完了但包没发出去」）。
//!
//! ## 归属规则是一句可判定的问句
//!
//! **这条命令会不会导致一个数据包离开本机？** 会则 [`NodeAccess`]，不会则 [`RecordAccess`]。
//! 只有三条命令要节点：发送、生成邀请、使用邀请。完整的归属表在
//! `dev-notes/knowledge/cli-host.md`。
//!
//! ## 直连数据库的正确性依据
//!
//! `migration` 的连接不设 `journal_mode`，走 SQLite 的 `delete` 模式——那模式下写事务会
//! 阻塞所有读，而常驻节点接收文件时一直在写。**所以有常驻节点时必须走本地通道**，直连会撞
//! `database is locked`。反过来，没有常驻节点时也就没有并发写者，直连是安全的。

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use swarmdrop_core::transfer::store::TransferStore;
use swarmdrop_host::device::PairedDeviceInfo;
use swarmdrop_host_fs::JsonFileIdentityStore;
use swarmdrop_invite::{InviteRegistry, InviteStore};

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};

use super::boot::{RunningNode, boot};
use super::ipc::{self, Request, Response};
use super::single::{Acquisition, NodeLock, acquire};

/// 把通道应答翻成负载。
///
/// `Ok(None)` = 这条通道给不出答案（连不上、或服务端回了无负载的 `Ok`），调用方应回落到
/// 本地路径。**「连不上」不是错误**：它是「没有常驻节点」这一事实。
fn unpack(response: Option<Response>) -> CliResult<Option<Value>> {
    match response {
        Some(Response::Data { payload }) => Ok(Some(payload)),
        // **按服务端给的分类重建**，不要一律压成「节点不可用」——那会让「没有这条记录」
        // 在有常驻节点时退 3、无节点时退 2。
        Some(Response::Error { code, message }) => Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => Ok(None),
    }
}

/// 本机持久化记录的访问器。
///
/// 构造是零成本的（只存路径），真正的连接在每个方法里按需建立——一条命令只用得上其中
/// 一两个，预先全开等于为「看一眼收件箱」也付出打开身份文件的代价。
#[derive(Clone)]
pub struct Records {
    data_dir: DataDir,
}

impl Records {
    pub fn new(data_dir: DataDir) -> Self {
        Self { data_dir }
    }

    /// 打开数据库（迁移一并跑掉）。
    ///
    /// 每个取数方法各自打开一次而不是共用一个连接：一条命令只用得上其中一两个，
    /// 而 SQLite 的连接建立本身很便宜。
    ///
    /// 返回类型经 `migration` 的重导出拿到，**本 crate 不直接依赖 sea-orm**——
    /// 命令行宿主只认端口（`TransferStore` / `InviteStore`），ORM 是 `storage-sql`
    /// 那一侧的实现细节，这里只是转手一个不透明的连接。
    async fn db(&self) -> CliResult<migration::sea_orm::DatabaseConnection> {
        migration::connect_and_migrate(&self.data_dir.database())
            .await
            .map_err(|err| CliError::NodeUnavailable(format!("打开数据库失败: {err}")))
    }

    /// 传输与收件箱的存储。
    pub async fn transfers(&self) -> CliResult<Arc<dyn TransferStore>> {
        Ok(Arc::new(swarmdrop_storage_sql::SqlSessionStore::new(
            Arc::new(self.db().await?),
        )))
    }

    /// 邀请注册表，**已从库读回内存**。
    ///
    /// ⚠️ **必须 `load` 之后才能用**，这也是本方法存在的全部理由。
    /// `InviteRegistry::revoke_by_hash` 查不到内存记录时会直接 no-op 并**报告成功**——
    /// 不 `load` 就撤销，结果是「报告成功但什么都没发生」，最坏的一种失败形态。
    /// 顺带这也让领域规则（未过期、非已撤销、按创建时刻倒序）只有 `list_active` 一份实现，
    /// 不必在本 crate 里照抄一遍 SQL 过滤条件。
    pub async fn invites(&self) -> CliResult<InviteRegistry> {
        let store: Arc<dyn InviteStore> =
            Arc::new(swarmdrop_storage_sql::SqlInviteStore::new(self.db().await?));
        let registry = InviteRegistry::new(store);
        registry.load(now_secs()).await;
        Ok(registry)
    }

    /// 已配对设备表的存储端口。
    pub fn device_store(&self) -> Arc<JsonFileIdentityStore> {
        Arc::new(JsonFileIdentityStore::new(self.data_dir.path()))
    }

    /// 已配对设备。
    pub async fn paired_devices(&self) -> CliResult<Vec<PairedDeviceInfo>> {
        use swarmdrop_core::host::PairedDeviceStore;

        self.device_store()
            .load_paired_devices()
            .await
            .map_err(|err| CliError::NodeUnavailable(format!("读取已配对设备失败: {err}")))
    }
}

/// 当前 Unix 秒。
///
/// 转出自 [`swarmdrop_host::now_secs`]——邀请 TTL 的判定发生在 core 与本 crate 两处，
/// 各自取时钟会让「同一张邀请在两条路径上过期时刻不同」成为可能。
pub use swarmdrop_host::now_secs;

/// 只读或只改本机持久化记录的命令的取数入口。
///
/// 有常驻节点就经本地通道问它，否则直连本机记录——**任何情况下都不启动节点**。
///
/// 归属规则是一句可判定的问句：**这条命令会不会导致一个数据包离开本机？** 不会的都走这里
/// （`device list/forget` · `invite list/revoke` · `inbox list/show` · `transfer list/show`），
/// 会的走 [`NodeAccess`]（`send` · `invite create` · `invite use`）。
///
/// 答错**不报错**：该走 [`NodeAccess`] 的命令走到这里，表现是「跑完了但一个包都没发」；
/// 反过来则让「看一眼本机记录」变成一次连引导节点的几秒等待。看守它的是
/// `tests/without_a_node.rs::record_commands_never_start_a_node`——它从**进程外**判断
/// （身份文件在不在），而不是让代码断言自己的声明。
pub struct RecordAccess {
    records: Records,
    /// 常驻节点的通道。`None` = 没有常驻节点。
    socket: Option<PathBuf>,
}

impl RecordAccess {
    pub async fn open(data_dir: &DataDir) -> Self {
        let socket = data_dir.socket();
        let alive = ipc::is_alive(&socket).await;
        Self {
            records: Records::new(data_dir.clone()),
            socket: alive.then_some(socket),
        }
    }

    /// 取一次数据：有常驻走通道，否则（或通道这一瞬没了）走本地。
    ///
    /// 兜底而不是报错是刻意的：通道刚才还活着、现在没了，意味着节点在这个瞬间关停——
    /// 此刻已经没有并发写者，正是直连安全的时候。这段竞态处理**只存在这一处**；
    /// 它此前摊在每条命令里各写一遍，漏掉的那份会在节点关停的瞬间报一个与真实原因无关的错。
    pub async fn query<F, Fut>(&self, request: Request, local: F) -> CliResult<Value>
    where
        F: FnOnce(Records) -> Fut,
        Fut: Future<Output = CliResult<Value>>,
    {
        if let Some(socket) = &self.socket
            && let Some(payload) = unpack(ipc::request(socket, &request).await?)?
        {
            return Ok(payload);
        }
        local(self.records.clone()).await
    }
}

/// 需要活节点的命令的取数入口。
///
/// 有常驻节点就复用它，否则起一个**临时节点**——命令结束即销毁，不改变「用户是否希望节点
/// 常驻」的意图。
pub enum NodeAccess {
    /// 复用正在运行的常驻节点。
    Daemon { socket: PathBuf },
    /// 本进程持有的临时节点。
    Owned {
        /// 装箱：两个变体的体量差得远，不装箱会让每个值都按大的那个占位。
        node: Box<RunningNode>,
        /// 持有权。**必须活到节点关停之后**才 drop，否则下一个进程会在旧节点还在时拿到锁。
        lock: NodeLock,
    },
}

impl NodeAccess {
    pub async fn open(data_dir: &DataDir, json: bool) -> CliResult<Self> {
        match acquire(data_dir).await? {
            Acquisition::Existing => Ok(Self::Daemon {
                socket: data_dir.socket(),
            }),
            Acquisition::Owner(lock) => Ok(Self::Owned {
                node: Box::new(boot(data_dir, json).await?),
                lock,
            }),
        }
    }

    /// 本进程自持的节点（复用常驻节点时为 `None`）。
    pub fn local(&self) -> Option<&RunningNode> {
        match self {
            Self::Owned { node, .. } => Some(node),
            Self::Daemon { .. } => None,
        }
    }

    /// 取本进程自持的节点，没有则报「节点不可用」。
    ///
    /// 每条命令的本地回落分支都要这一句。摊在各命令里各写一遍时它们迟早会各说各的措辞，
    /// 而这句话正是用户在通道意外断开时看到的唯一解释。
    pub fn require_local(&self) -> CliResult<&RunningNode> {
        self.local()
            .ok_or_else(|| CliError::NodeUnavailable("节点不可用".into()))
    }

    /// 向常驻节点发一条请求；本进程自持节点时返回 `None`，由调用方走本地路径。
    pub async fn ask(&self, request: &Request) -> CliResult<Option<Response>> {
        match self {
            Self::Owned { .. } => Ok(None),
            Self::Daemon { socket } => ipc::request(socket, request).await,
        }
    }

    /// 命令收尾。
    ///
    /// 临时节点在此关停；复用常驻节点时什么都不做——**绝不能顺手把别人的节点关了**。
    pub async fn close(self) {
        if let Self::Owned { node, lock } = self {
            node.manager.shutdown().await;
            drop(lock); // 显式：锁必须在关停之后释放
        }
    }
}

/// 把序列化失败翻成统一的错误措辞。
pub fn to_value<T: serde::Serialize>(value: &T, what: &str) -> CliResult<Value> {
    serde_json::to_value(value)
        .map_err(|err| CliError::NodeUnavailable(format!("序列化{what}失败: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 没有常驻节点时，`Persisted` 档必须直接走本地——**不得启动节点**。
    ///
    /// 这条看守的是本模块存在的理由：档位判断错了不会报错，只会让「看一眼本机记录」
    /// 变成一次连引导节点、做 NAT 探测的几秒等待。
    #[tokio::test]
    async fn record_access_without_daemon_goes_local() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");

        let access = RecordAccess::open(&dir).await;
        let payload = access
            .query(Request::DeviceList, |records| async move {
                to_value(&records.paired_devices().await?, "设备列表")
            })
            .await
            .expect("本地路径应当可用");

        // 空目录 ⇒ 空列表，而不是错误。
        assert_eq!(payload.as_array().map(Vec::len), Some(0));
    }

    /// 邀请注册表必须是 `load` 过的。
    ///
    /// 空库上也要能取到一个可用的注册表——`invite list` 在从未发过邀请的机器上
    /// 应当输出空列表，而不是报错。
    #[tokio::test]
    async fn invite_registry_is_loaded_and_usable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");

        let registry = Records::new(dir).invites().await.expect("注册表");
        assert!(registry.list_active(now_secs()).is_empty());
    }

    /// 造一张可注册的邀请。
    fn sample_invite() -> swarmdrop_invite::PairInvite {
        swarmdrop_invite::PairInvite::generate(
            &swarmdrop_net::SecretKey::generate(),
            vec!["/ip4/192.168.1.10/tcp/4001".parse().expect("addr")],
            swarmdrop_invite::TransportPolicy::Auto,
            "测试机".into(),
            "macos".into(),
            now_secs(),
        )
        .expect("生成邀请")
    }

    /// **无节点时撤销必须跨重启生效。**
    ///
    /// 这条端到端地看守本次改动的核心承诺：邀请泄露后的止损不依赖节点在跑。
    /// 它同时钉住了 `Records::invites` 存在的理由——注册表**先 load 再撤销**，
    /// 不 load 的话 `revoke_by_hash` 查不到内存记录会直接 no-op 并**报告成功**，
    /// 表现是「撤销成功」但那张邀请下次启动照样可用。
    #[tokio::test]
    async fn revoking_without_a_node_survives_restart() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        let records = Records::new(dir);

        // 发一张（模拟节点在跑时签发的那张）。
        let invite = sample_invite();
        let issued = records.invites().await.expect("注册表");
        issued.register(&invite, now_secs()).await;
        drop(issued);

        // 另开一个注册表读回来——这一步等价于「重启后再看」。
        let reopened = records.invites().await.expect("注册表");
        let rows = super::super::invites::list(&reopened, now_secs());
        assert_eq!(rows.len(), 1, "重启后应当仍看得见这张邀请");

        let outcome = super::super::invites::revoke(&reopened, &rows[0].id)
            .await
            .expect("撤销");
        assert_eq!(outcome.revoked, 1);
        assert!(outcome.persisted, "撤销必须落盘，否则重启后会复活");
        drop(reopened);

        // 再重启一次：撤掉的那张不该回来。
        let after = records.invites().await.expect("注册表");
        assert!(
            super::super::invites::list(&after, now_secs()).is_empty(),
            "撤销没有跨重启生效——邀请复活了"
        );
    }

    /// 全撤要把每一张都撤掉，且**不因某一张失败而短路**。
    #[tokio::test]
    async fn revoke_all_clears_every_invite() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        let records = Records::new(dir);

        let registry = records.invites().await.expect("注册表");
        for _ in 0..3 {
            registry.register(&sample_invite(), now_secs()).await;
        }
        assert_eq!(super::super::invites::list(&registry, now_secs()).len(), 3);

        let outcome = super::super::invites::revoke_all(&registry, now_secs()).await;
        assert_eq!(outcome.revoked, 3);
        assert!(outcome.persisted);

        let after = records.invites().await.expect("注册表");
        assert!(super::super::invites::list(&after, now_secs()).is_empty());
    }

    /// **无节点时解除配对必须真的改掉本机记录。**
    #[tokio::test]
    async fn forgetting_without_a_node_removes_the_record() {
        use swarmdrop_core::host::PairedDeviceStore;
        use swarmdrop_host::device::{OsInfo, PairedDeviceInfo};

        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        let records = Records::new(dir);

        let peer_id = swarmdrop_net::SecretKey::generate().node_id();
        records
            .device_store()
            .save_paired_devices(&[PairedDeviceInfo::new(peer_id, OsInfo::default(), 1)])
            .await
            .expect("写入");
        assert_eq!(records.paired_devices().await.expect("读取").len(), 1);

        let outcome = super::super::devices::forget(&records, None, &peer_id.to_string())
            .await
            .expect("解除配对");

        assert_eq!(outcome.remaining, 0);
        assert!(
            records.paired_devices().await.expect("读取").is_empty(),
            "解除配对没有落到本机记录上"
        );
    }

    /// 空库上的传输记录查询要给空列表，不是报错——新机器上 `transfer list` 是常见操作。
    #[tokio::test]
    async fn transfer_list_on_a_fresh_machine_is_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        let records = Records::new(dir);

        let store = records.transfers().await.expect("打开存储");
        assert!(
            super::super::transfers::list(&*store)
                .await
                .expect("读取传输记录")
                .is_empty()
        );
    }
}
