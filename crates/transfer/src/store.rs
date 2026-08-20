//! 持久化端口（依赖倒置的核心）。
//!
//! transfer 只依赖这些 trait，不认识具体的 ORM 连接类型。桌面/移动端
//! 由 `swarmdrop-storage-sql` 的 `SqlSessionStore` 用 SeaORM 实现；Web 端是
//! `crates/web` 的 IndexedDB 写穿版（见 `dev-notes/knowledge/storage-abstraction.md`）。
//!
//! 粒度取「用例级」：`create_session` 而非 `insert_session_row`+`insert_file_row`，
//! 事务是实现细节，trait 上不出现 `begin`/`commit`。entity 的 `Model` 是 wasm 可用的
//! 普通数据类型，可直接上签名（第 0 步已实测）；带关系字段的 `ModelEx` 是 sea-orm 的
//! 实现细节，**不许出现在端口签名上**。
//!
//! 端口覆盖三类用例，缺一不可：
//!
//! - **运行时写路径** —— 建会话、写 checkpoint、落 outboard、按状态机转换持久化；
//! - **历史管理** —— 列表投影、删单条、清空、取源路径、过期回收、标记 origin；
//! - **收件箱管理** —— 建条目、补建、列表、检索、详情、标记打开、归档、软删、标记文件缺失。
//!
//! 后两类曾整组留在 trait 之外（宿主直接打 ORM 自由函数），于是「换一个存储实现」
//! 只换掉一部分，Web 端补不齐。现已收口：宿主经 [`crate::manager::TransferManager::store`]
//! 或自己持有的同一个 `Arc` 取回 store，管理类用例一条不落地走端口。

use async_trait::async_trait;
use uuid::Uuid;

use crate::AppResult;
use crate::coordinator::TransferState;
use crate::host::{CoreSaveLocation, HostFileMetadata};
use crate::inbox::{INBOX_SEARCH_LIMIT, InboxItemDetail, InboxItemSummary, InboxSearchHit};
use crate::protocol::{FileInfo, TransferOrigin};
use crate::text_delivery::TextDeliveryRecord;

/// 会话/文件级持久化端口（断点续传的事实来源 + 传输历史的唯一出口）。
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// 创建传输会话 + 关联文件记录（策略快照随建会话一次写入）。
    async fn create_session(&self, input: CreateSessionInput<'_>) -> AppResult<()>;

    /// 接收方选择保存位置后，把 offered session 补齐为可启动接收的事实。
    async fn update_session_save_path(
        &self,
        session_id: Uuid,
        save_path: CoreSaveLocation,
    ) -> AppResult<()>;

    /// 更新文件 range checkpoint 和已传输字节数（数据面）。
    async fn update_file_checkpoint_ranges(
        &self,
        session_id: Uuid,
        file_id: i32,
        completed_chunks: Vec<u8>,
        completed_ranges: &[(u64, u64)],
        transferred_bytes: i64,
    ) -> AppResult<()>;

    /// 标记单个文件完成，写入完整 checkpoint 与最终落盘位置。
    async fn mark_file_completed(
        &self,
        session_id: Uuid,
        file_id: i32,
        completed_chunks: Vec<u8>,
        transferred_bytes: i64,
        local_path: String,
        local_dir: String,
    ) -> AppResult<()>;

    /// 批量保存发送方 per-file 进度（`(file_id, chunks_done, transferred_bytes)`）。
    ///
    /// **绝对覆盖语义：传进来的值就是新值，包括 0。** 实现不得过滤零值——续传基线是
    /// 唯一一个真的需要把这一列**改小到 0** 的调用方（对端 checkpoint 表明某文件一块都
    /// 没收到，而本端优雅暂停时留下的旧值非 0）。此前两处实现都写着 `if transferred > 0`，
    /// 于是基线为 0 的文件永远写不下去：`dispatch(ResumeCommitted)` 先按旧值 emit 一份高报
    /// 的 projection，紧接着 tracker 的第一条进度把它拽回真值，进度条当场倒退。
    ///
    /// 零值过滤看起来安全，其实挡的是唯一想写 0 的正确调用。其余四个调用点
    /// （`SenderActor::on_completed` / `on_interrupted`、`pause_send`、`handle_pause_impl`）
    /// 传的都是 `get_file_progress()` 全量快照，它们写 0 的场景只有「该文件一块没发」——
    /// 那时 DB 里本来就是 0（首传）或 tracker 已被基线正确初始化（续传），写 0 是恒等操作。
    async fn save_sender_file_progress(
        &self,
        session_id: Uuid,
        progress: &[(u32, u32, u64)],
    ) -> AppResult<()>;

    /// 按 reducer 输出的新状态写入 session 的 phase/reason/epoch/recoverable。
    async fn apply_transition(
        &self,
        session: &entity::transfer_session::Model,
        state: &TransferState,
    ) -> AppResult<()>;

    /// 启动清理：查所有 `phase=Active` 的遗留会话 id。
    async fn find_active_session_ids(&self) -> AppResult<Vec<Uuid>>;

    /// 对端断连：查某 peer 当前所有 `phase=Active` 的会话 id。
    async fn find_active_session_ids_by_peer(&self, peer_id: &str) -> AppResult<Vec<Uuid>>;

    /// 加载 session 原始 Model（Coordinator 读当前生命周期状态；断点续传取回会话）。
    async fn find_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<entity::transfer_session::Model>>;

    /// 查询单个 session 的投影（状态转换后 emit 给前端）。
    async fn get_transfer_projection(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<TransferProjection>>;

    /// 获取 session 的文件列表（含 bitmap，供断点续传使用）。
    async fn get_session_files(
        &self,
        session_id: Uuid,
    ) -> AppResult<Vec<entity::transfer_file::Model>>;

    /// 持久化发送方某文件的 bao outboard（逐块验签 Merkle 树）。prepare 建好后随会话落库，
    /// resume 免重算。
    async fn save_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
        outboard: Vec<u8>,
    ) -> AppResult<()>;

    /// 载入发送方某文件的 bao outboard；无记录（旧会话 / 接收方）返回 `None`（发送端重算回存）。
    async fn load_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
    ) -> AppResult<Option<Vec<u8>>>;

    /// 查询全部传输投影（前端列表的唯一数据源）。
    ///
    /// **契约：按 `started_at` 倒序**。这不是「某个实现碰巧的顺序」——端口保证的是
    /// 确定性：同一份数据两次调用必须给出同一序，否则「取最近 N 条」这类消费方
    /// 在不同实现上会拿到不同的 N 条，测试也只能退化成断言集合相等。
    /// 前端各面板按自己的维度（结束时间 / 更新时间）重排是预期行为，与本契约不冲突。
    async fn list_transfer_projections(&self) -> AppResult<Vec<TransferProjection>>;

    /// 查询**未完成**的传输投影（`phase != Terminal`），同样按 `started_at` 倒序。
    ///
    /// **与「取全部再过滤」不是同一件事**，所以它是端口的一等方法而不是调用方的一行
    /// `filter`：历史记录只增不减，而消费它的是「实时看现在在传什么」这类每秒重取一次
    /// 的面板（命令行宿主的 `transfer watch`）。在应用层过滤意味着每一次刷新都要把
    /// 整张表连同全部文件行读回内存，读的行数随使用时长线性增长——而真正要的那几条
    /// 通常是个位数。
    ///
    /// 包含 `Suspended`：暂停与中断的会话仍是「未完成」，恢复入口正是从这份清单里选。
    async fn list_unfinished_projections(&self) -> AppResult<Vec<TransferProjection>>;

    /// 删除单条传输记录：会话行 + 其文件行（级联）。会话不存在时静默成功。
    ///
    /// 级联边界（三端一致，不因平台分叉）：
    /// - **不删收件箱条目** —— `inbox_items.transfer_session_id` 是 `ON DELETE SET NULL`，
    ///   条目留下、外键置空。收件箱是「结果」，传输记录是「过程」，删过程不动结果。
    /// - **不删已落盘文件** —— 文件的生命周期归收件箱侧管，本方法只清账本。
    ///
    /// 本方法**不判生命周期**：非终态会话不可删是域不变量，守卫在
    /// [`crate::manager::TransferManager::delete_session`]，宿主删除一律走那条域方法。
    async fn delete_session(&self, session_id: Uuid) -> AppResult<()>;

    /// 清空传输历史：删除**所有 `phase = Terminal` 的会话**及其文件行。
    ///
    /// 非终态会话（offered / waiting_accept / active / suspended）一律保留——它们
    /// 还不是「历史」，且删掉行会留下一个仍在写 checkpoint 的孤儿 actor。
    /// 与 [`Self::delete_session`] 同样不碰收件箱条目与已落盘文件。
    async fn clear_all_history(&self) -> AppResult<()>;

    /// 取会话内有源路径的文件绝对路径（发送方向；「从历史重新发送」重建载荷用）。
    ///
    /// 无源路径的文件（接收方向 / 历史会话）不出现在结果里，故返回空 Vec 是正常情形。
    async fn get_session_source_paths(&self, session_id: Uuid) -> AppResult<Vec<String>>;

    /// 启动清理：回收超过保留期仍未恢复的 recoverable suspended **接收**会话。
    ///
    /// 命中判据：`phase = Suspended && recoverable && direction = Receive`
    /// 且 `updated_at < now - retention_secs`。命中会话转 `Terminal`/`FatalError`
    /// （带 [`crate::expired_receive_reason`] 说明），并返回其文件元数据，
    /// 供调用方用本端 `FileAccess` 尽力清理 `.part`。
    ///
    /// **调用点必须排在 [`crate::coordinator::TransferCoordinator::cleanup_recoverable_sessions`]
    /// 之后**：上次运行被强杀留下的 `Active` 会话要先由清理转成 `Suspended`，
    /// 本方法才看得见它们。顺序反了判据就会漏掉一整类会话（Web 端曾因回收跑在
    /// 节点起来之前而不得不改判据，导致两端契约分叉）。
    ///
    /// 保留期内的会话、发送会话、已 terminal 的会话都不受影响——正常断点续传不被打断。
    async fn reap_expired_suspended_receives(
        &self,
        retention_secs: u64,
    ) -> AppResult<Vec<ExpiredReceiverActor>>;

    /// 更新会话的 `origin`（provenance）。
    ///
    /// agent 代收接受入站 offer 后把 origin 标成 `Mcp`，使完成后建的收件箱条目
    /// `source_kind=mcp`（UI 显示「AI 代理」）——与落盘位置无关，让代收文件既落在
    /// 与手动一致的接收文件夹、又能在收件箱区分来源。
    async fn update_session_origin(
        &self,
        session_id: Uuid,
        origin: TransferOrigin,
    ) -> AppResult<()>;
}

/// 收件箱端口（「已接收内容」这本账的唯一出口）。
///
/// transfer 自己只用「完成接收后幂等建条目」一个用例，其余九个是**宿主的**用例
/// （列表 / 检索 / 详情 / 归档 / 软删 / 标记）。它们同样在端口上，因为收件箱与
/// 传输历史一样必须「换一个存储实现就整体换掉」——留在 ORM 自由函数上就意味着
/// Web 端天然缺一半能力。
///
/// DTO 与共享的领域规则（标题 / 内容指纹 / 聚合文本 / 来源分类 / 片段 / 命中判据）
/// 在 [`crate::inbox`]，各实现一律调那一份。
#[async_trait]
pub trait InboxStore: Send + Sync {
    /// 从已完成接收会话幂等创建收件箱条目。
    ///
    /// 「已完成接收」的判据是 [`crate::inbox::is_completed_receive`]，不合判据的会话
    /// 返回 `Ok(None)`——**这不是错误**。
    /// 返回详情而非 `()`，是为了让 [`Self::repair_missing_inbox_items_for_completed_receives`]
    /// 能直接用本方法实现，而不必在每个存储实现里各写一遍「查重 + 构造条目」。
    /// 接收热路径（`actor::receiver`）不消费返回值，只在 `Err` 分支记日志并上报。
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>>;

    /// 补建所有已完成 receive 会话缺失的收件箱条目，返回本次补出的条目。
    ///
    /// 它修的是「建条目那一刻写失败」（`ensure_*` 失败只记日志不阻断传输，这个洞长期存在），
    /// **不是**存储格式迁移。单个会话失败只跳过、不掐断整批。
    async fn repair_missing_inbox_items_for_completed_receives(
        &self,
    ) -> AppResult<Vec<InboxItemDetail>>;

    /// 列出收件箱条目：排除软删项，按 `received_at` 倒序；`include_archived=false` 时排除已归档项。
    async fn list_inbox_items(&self, include_archived: bool) -> AppResult<Vec<InboxItemSummary>>;

    /// 子串检索：命中判据是 [`crate::inbox::inbox_matches`]（大小写不敏感，覆盖
    /// 标题 / 来源名 / 文件聚合文本 / 抽取正文），片段由 [`crate::inbox::inbox_snippet`]
    /// 生成。各实现的一致性由共享语料 [`crate::inbox::INBOX_MATCH_CASES`] 钉住。
    /// 空查询返回空列表；结果按 `received_at` 倒序并截断到 `limit`。
    async fn search_inbox(
        &self,
        query: &str,
        limit: usize,
        include_archived: bool,
    ) -> AppResult<Vec<InboxSearchHit>>;

    /// **宿主该调的入口**：`limit` 缺省取 [`INBOX_SEARCH_LIMIT`]，且不允许超过它。
    ///
    /// 上面那个 `search_inbox` 收的是确定的 `usize`，于是每个宿主都得自己想「不传时用几」
    /// ——#111 之前四个宿主想出了四个答案（Tauri 命令 20、MCP 20、移动 100、Web 50），
    /// 而内核的截断是「按 `received_at` 倒序之后截断」，掉的永远是最早收到的那批：
    /// 同一批数据、同一个词，老条目在一端搜不到、在另一端搜得到。
    ///
    /// 把兜底放在这里而不是各宿主，是为了让 `Option` 成为宿主面对的类型——「自带一个默认值」
    /// 那条路在类型上就不存在了。**上限只能收窄不能放宽**：它是三端共享的展示契约，
    /// 宿主传更大的数等于把分叉从常量搬到参数上。
    async fn search_inbox_capped(
        &self,
        query: &str,
        limit: Option<u32>,
        include_archived: bool,
    ) -> AppResult<Vec<InboxSearchHit>> {
        let limit = limit.map_or(INBOX_SEARCH_LIMIT, |n| (n as usize).min(INBOX_SEARCH_LIMIT));
        self.search_inbox(query, limit, include_archived).await
    }

    /// 加载收件箱详情。软删除后的条目对普通详情不可见。
    async fn get_inbox_item_detail(&self, item_id: Uuid) -> AppResult<Option<InboxItemDetail>>;

    /// 加载与指定传输会话关联的可见收件箱详情（同样排除软删项）。
    async fn get_inbox_item_by_transfer_session_id(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>>;

    /// 标记收件箱条目最近打开时间。条目不存在时静默成功。
    async fn mark_inbox_item_opened(&self, item_id: Uuid) -> AppResult<()>;

    /// 归档或取消归档收件箱条目。条目不存在时静默成功。
    async fn archive_inbox_item(&self, item_id: Uuid, archived: bool) -> AppResult<()>;

    /// 软删除收件箱记录；是否删除本地文件由宿主在调用前完成。条目不存在时静默成功。
    async fn delete_inbox_item_record(&self, item_id: Uuid) -> AppResult<()>;

    /// 标记收件箱文件的缺失状态。
    ///
    /// `item_id` 不是冗余参数：**归属校验是端口的义务**，不是各宿主自理的事。
    /// 实现必须断言 `file_id` 确实属于 `item_id` 指向的条目，否则返回错误——
    /// 否则任意 `file_id` 都能改到别的条目下的文件（该检查此前只有移动端桥接层做了，
    /// 桌面没做）。另一条理由是 Web 端没有全局自增主键，`file_id` 只是条目内序号，
    /// 单靠它根本定位不到文件。
    async fn mark_inbox_item_file_missing(
        &self,
        item_id: Uuid,
        file_id: i32,
        missing: bool,
    ) -> AppResult<()>;
}

/// 文本收发账本的唯一持久化出口。
///
/// 发送记录和接收记录共享同一个稳定 delivery_id，但只接收方向投影到 Inbox。实现必须让
/// 接收记录与 Inbox 投影处于同一原子提交中；只有该方法成功后协议层才能确认已送达。
#[async_trait]
pub trait TextDeliveryStore: Send + Sync {
    /// 在网络调用前创建发送记录；重复 ID 不得覆盖既有正文。
    async fn create_outgoing_text_delivery(&self, record: TextDeliveryRecord) -> AppResult<()>;

    /// 按稳定投递标识读取本地账本记录。
    async fn get_text_delivery(&self, delivery_id: Uuid) -> AppResult<Option<TextDeliveryRecord>>;

    /// 按目标设备列出本机发送记录，最新更新的排在前面。
    async fn list_outgoing_text_deliveries(
        &self,
        peer_id: &str,
    ) -> AppResult<Vec<TextDeliveryRecord>>;

    /// 原子更新发送记录的状态及安全失败分类。
    async fn update_outgoing_text_delivery(
        &self,
        delivery_id: Uuid,
        status: entity::TextDeliveryStatus,
        failure: Option<entity::TextDeliveryFailure>,
        attempt_count: Option<i32>,
        updated_at: i64,
    ) -> AppResult<()>;

    /// 持久化尚待用户确认的接收文本，但不得创建或暴露 Inbox 投影。
    ///
    /// 同一投递标识只能对应相同的来源和正文；重放必须幂等，篡改必须拒绝。
    async fn create_pending_incoming_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<()>;

    /// 读取尚待处理的接收文本，供宿主重新挂载或事件遗漏后重建确认队列。
    async fn list_pending_incoming_text_deliveries(&self) -> AppResult<Vec<TextDeliveryRecord>>;

    /// 将待确认接收文本写入拒绝或过期终态；终态不得再次回到确认队列。
    async fn finalize_pending_incoming_text_delivery(
        &self,
        delivery_id: Uuid,
        status: entity::TextDeliveryStatus,
        updated_at: i64,
    ) -> AppResult<()>;

    /// 幂等持久化接收文本，并在同一事务内创建对应的 Inbox 投影。
    ///
    /// 既有相同 ID 必须同时校验来源和正文：相同内容返回原详情；不同内容返回协议冲突，
    /// 绝不覆盖正文或创建第二个收件箱条目。
    async fn persist_incoming_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<InboxItemDetail>;

    /// 删除发送方向的记录及正文。接收方向应经 Inbox 删除路径处理。
    async fn delete_outgoing_text_delivery(&self, delivery_id: Uuid) -> AppResult<()>;

    /// 将进程退出前遗留的在途发送状态恢复为显式可重试状态，绝不触发网络请求。
    async fn recover_interrupted_text_deliveries(&self, now_ms: i64) -> AppResult<u64>;
}

/// 传输域持久化的合并端口——便于 TransferManager / actor 单 `Arc` 注入。
///
/// blanket impl 覆盖任何同时实现两个子端口的类型，故实现方（`SqlSessionStore`）
/// 只需分别 impl [`SessionStore`] 与 [`InboxStore`]。
pub trait TransferStore: SessionStore + InboxStore + TextDeliveryStore {}
impl<T: SessionStore + InboxStore + TextDeliveryStore + ?Sized> TransferStore for T {}

/// 新建传输会话所需的完整事实。
pub struct CreateSessionInput<'a> {
    pub session_id: Uuid,
    pub direction: entity::TransferDirection,
    pub peer_id: &'a str,
    pub peer_name: &'a str,
    pub files: &'a [FileInfo],
    pub total_size: u64,
    pub save_path: Option<CoreSaveLocation>,
    /// 发送方传入每个文件的绝对路径（与 `files` 一一对应），接收方传 `None`。
    pub source_paths: Option<&'a [String]>,
    pub lifecycle: TransferState,
    /// 入站 Offer 的接收策略快照 `(action_name, reason)`；非策略场景传 `None`。
    pub policy: Option<(&'a str, &'a str)>,
    /// 传输发起来源（人工 / MCP 代理），与 policy 正交；非传输场景传 `None`。
    pub origin: Option<TransferOrigin>,
}

/// 被过期回收的接收会话及其文件元数据（供 host 尽力清理遗留 `.part`）。
pub struct ExpiredReceiverActor {
    pub session_id: Uuid,
    /// 重建 sink 所需的文件元数据（已带 `save_dir`）。
    pub files: Vec<HostFileMetadata>,
}

/// 传输投影 DTO —— 前端唯一状态源（逐步替代旧的分散事件 + 扁平 `SessionStatus`）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProjection {
    pub session_id: Uuid,
    pub direction: entity::TransferDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub phase: entity::TransferPhase,
    pub suspended_reason: Option<entity::SuspendedReason>,
    pub terminal_reason: Option<entity::TerminalReason>,
    pub recoverable: bool,
    pub epoch: i64,
    pub total_size: i64,
    pub transferred_bytes: i64,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    /// 失败判别码（见 [`crate::failure`]）。曾是直达三端 UI 的自由中文文本。
    pub failure: Option<crate::failure::FailureCode>,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
    pub save_path: Option<CoreSaveLocation>,
    /// 「打开文件夹」应定位的真实容器目录 URI(收到内容实际所在的文件夹),已在 core 解析:
    /// 各文件 `local_dir` 全部同一目录 → 该目录;否则回退存储根 `save_path`。前端直读,
    /// 不再自行兜底(已完成接收必为 `Some`)。
    pub content_root: Option<String>,
    pub files: Vec<TransferProjectionFile>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TransferProjectionFile {
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    pub transferred_bytes: i64,
}

impl From<entity::transfer_file::Model> for TransferProjectionFile {
    fn from(f: entity::transfer_file::Model) -> Self {
        Self {
            file_id: f.file_id,
            name: f.name,
            relative_path: f.relative_path,
            size: f.size,
            transferred_bytes: f.transferred_bytes,
        }
    }
}

/// 「打开文件夹」应定位的真实容器目录(投影与收件箱共用,**兜底收口在此一处**):
/// 所有已完成接收文件的 `local_dir` 若唯一一致 → 该目录;否则(跨多个不同父目录 /
/// 发送会话 / 缺 local_dir 的历史)→ 回退存储根 `save_path`。返回值即「可直接打开的
/// 目录 or None(无 save_path 的边角)」,消费方直读、无需再兜底。绝不做「保存目录 +
/// 相对路径」字符串拼接推导。
///
/// 只吃 `local_dir` 的迭代器，不吃文件行：本函数读的就是这一个字段，收整份文件行会
/// 逼着调用方先摊平一份（`completed_chunks` 位图与 `outboard` BLOB 都在行里，每完成
/// 一次接收就是数百 KB～数 MB 的纯 memcpy）。同样也就不必再规定「必须传纯 scalar 的
/// `Model`」——`ModelEx` 的关系机制根本进不了签名。
pub fn content_root_of<'a>(
    dirs: impl IntoIterator<Item = Option<&'a str>>,
    save_path: Option<&CoreSaveLocation>,
) -> Option<String> {
    let mut dirs = dirs.into_iter().flatten();
    if let Some(first) = dirs.next()
        && dirs.all(|d| d == first)
    {
        return Some(first.to_string());
    }
    save_path.map(|CoreSaveLocation::Path { path }| path.clone())
}

/// 传输记录是否允许删除：仅终态与 suspended。
///
/// suspended 是终态的近亲（没有活 actor），代价只是断点信息一并消失；
/// 而删掉一个进行中的会话会留下仍在写 checkpoint 的孤儿 actor。
/// 三端删除路径共用这一处判据，别在 UI 或存储实现里各抄一遍 phase 语义。
pub fn is_deletable(session: &entity::transfer_session::Model) -> bool {
    matches!(
        session.phase,
        entity::TransferPhase::Terminal | entity::TransferPhase::Suspended
    )
}

/// 由会话行 + 文件行构造投影 —— **投影语义的唯一定义处**。
///
/// 里面藏着一条真规则而不只是字段搬运：`transferred_bytes` 派生自文件级求和（单一事实
/// 来源）。文件进度由 persist_chunk / save_sender_file_progress 增量落库，投影直接 SUM，
/// 省掉各生命周期转换前手工 `sync_session_transferred_bytes` 的二次写与漂移风险。
/// `content_root` 同理收口在 [`content_root_of`]。
///
/// 参数取纯 scalar 的 `Model`：带关系字段的 `ModelEx` 是 sea-orm 的实现细节，
/// 编不进 wasm target —— Web 端此前正因签名收 `ModelEx` 而抄了一份 21 字段的同语义副本。
pub fn projection_of(
    session: &entity::transfer_session::Model,
    files: &[entity::transfer_file::Model],
) -> TransferProjection {
    let transferred_bytes = files.iter().map(|f| f.transferred_bytes).sum();
    let save_path = session.save_path.clone().map(CoreSaveLocation::from);
    let content_root = content_root_of(
        files.iter().map(|f| f.local_dir.as_deref()),
        save_path.as_ref(),
    );
    TransferProjection {
        session_id: session.session_id,
        direction: session.direction.clone(),
        peer_id: session.peer_id.0.clone(),
        peer_name: session.peer_name.clone(),
        phase: session.phase.clone(),
        suspended_reason: session.suspended_reason.clone(),
        terminal_reason: session.terminal_reason.clone(),
        recoverable: session.recoverable,
        epoch: session.epoch,
        total_size: session.total_size,
        transferred_bytes,
        started_at: session.started_at,
        updated_at: session.updated_at,
        finished_at: session.finished_at,
        failure: session
            .error_message
            .as_deref()
            .map(crate::failure::FailureCode::from_column),
        policy_action: session.policy_action.clone(),
        policy_reason: session.policy_reason.clone(),
        save_path,
        content_root,
        files: files.iter().cloned().map(Into::into).collect(),
    }
}

impl From<entity::transfer_session::ModelEx> for TransferProjection {
    fn from(s: entity::transfer_session::ModelEx) -> Self {
        // 关系字段在此收口成纯 scalar：往下（projection_of）只见 `Model`，
        // `ModelEx` 不越过本 impl。
        let files: Vec<entity::transfer_file::Model> =
            s.files.iter().cloned().map(Into::into).collect();
        projection_of(&s.into(), &files)
    }
}

/// 解析文件 `completed_ranges` JSON（数据面/续传计划共用）。
pub fn parse_completed_ranges(value: &str) -> Vec<(u64, u64)> {
    serde_json::from_str(value).unwrap_or_default()
}

/// 序列化 `completed_ranges` JSON（[`parse_completed_ranges`] 的反向；store 实现共用）。
pub fn ranges_json(ranges: &[(u64, u64)]) -> String {
    serde_json::to_string(ranges).unwrap_or_else(|_| "[]".to_string())
}

/// 「前缀已传」形态的 ranges（发送方以 `transferred_bytes` 表示断点）。
pub fn prefix_range(transferred_bytes: i64) -> Vec<(u64, u64)> {
    if transferred_bytes > 0 {
        vec![(0, transferred_bytes as u64)]
    } else {
        Vec::new()
    }
}

/// 文件的初始 chunk 位图：接收方按 [`calc_total_chunks`](crate::calc_total_chunks) 建零位图，
/// 发送方为空（不使用 bitmap）。接收位图尺寸是续传正确性的域不变量，store 实现共用一份。
pub fn initial_completed_chunks(size: u64, direction: entity::TransferDirection) -> Vec<u8> {
    if direction == entity::TransferDirection::Receive {
        vec![0u8; (crate::calc_total_chunks(size) as usize).div_ceil(8)]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(local_dir: Option<&str>) -> entity::transfer_file::Model {
        entity::transfer_file::Model {
            id: 0,
            session_id: Uuid::nil(),
            file_id: 0,
            name: "a.bin".into(),
            relative_path: "a.bin".into(),
            size: 1024,
            checksum: "deadbeef".into(),
            status: entity::FileStatus::Pending,
            transferred_bytes: 0,
            total_chunks: 1,
            completed_chunks: vec![],
            completed_ranges: "[]".into(),
            source_path: None,
            local_path: None,
            local_dir: local_dir.map(str::to_string),
            outboard: None,
        }
    }

    fn save_path(path: &str) -> CoreSaveLocation {
        CoreSaveLocation::Path {
            path: path.to_string(),
        }
    }

    fn session() -> entity::transfer_session::Model {
        entity::transfer_session::Model {
            session_id: Uuid::nil(),
            direction: entity::TransferDirection::Receive,
            peer_id: entity::PeerId("peer-123".to_string()),
            peer_name: "测试设备".into(),
            total_size: 2048,
            transferred_bytes: 0,
            status: entity::SessionStatus::Completed,
            phase: entity::TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(entity::TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            source_fingerprint: None,
            started_at: 1,
            updated_at: 2,
            finished_at: Some(3),
            error_message: None,
            policy_action: None,
            policy_reason: None,
            origin: None,
            save_path: None,
        }
    }

    /// 从文件行取出 `local_dir` 迭代器 —— 真实调用方的形态。
    fn dirs(files: &[entity::transfer_file::Model]) -> impl Iterator<Item = Option<&str>> {
        files.iter().map(|f| f.local_dir.as_deref())
    }

    /// `content_root_of` 三端合一后的回归（D2：签名不收文件行才让 Web 与 SQL 侧
    /// 都免掉整份克隆）。
    ///
    /// 四条分支各一断言——Web 那份副本正是靠「看起来同语义」活下来的，合一后必须有
    /// 一处把语义钉死，否则下次改动只在其中一端暴露。
    #[test]
    fn content_root_falls_back_to_save_path_unless_local_dirs_agree() {
        let root = save_path("/recv");

        // 全部 local_dir 一致 → 该目录（优先于存储根：那才是内容真正落在的地方）。
        let same = [file(Some("/recv/相册")), file(Some("/recv/相册"))];
        assert_eq!(
            content_root_of(dirs(&same), Some(&root)),
            Some("/recv/相册".to_string())
        );

        // 跨多个不同父目录 → 回退存储根，绝不拼接推导。
        let split = [file(Some("/recv/a")), file(Some("/recv/b"))];
        assert_eq!(
            content_root_of(dirs(&split), Some(&root)),
            Some("/recv".to_string())
        );

        // 全无 local_dir（发送会话 / 未完成接收）→ 同样回退存储根。
        let bare = [file(None), file(None)];
        assert_eq!(
            content_root_of(dirs(&bare), Some(&root)),
            Some("/recv".to_string())
        );

        // 既无一致 local_dir 又无存储根 → `None`，消费方直读、无需再兜底。
        let lone = [file(None)];
        assert_eq!(content_root_of(dirs(&lone), None), None);
    }

    /// `transferred_bytes` 由文件级求和派生 —— 会话行上那一列**不参与**投影。
    ///
    /// 这条钉的是 [`projection_of`] 里唯一的语义规则。Web 端此前自带的第二份构造器
    /// 正是复刻了这条求和（21 个字段逐个搬），两份实现哪天分叉，同一批数据在浏览器与
    /// 桌面就会显示不同的进度。
    #[test]
    fn projection_sums_transferred_bytes_from_files_not_session_column() {
        let mut a = file(Some("/recv"));
        a.transferred_bytes = 300;
        let mut b = file(Some("/recv"));
        b.file_id = 1;
        b.transferred_bytes = 700;

        let mut session = session();
        // 会话行上的值刻意写成一个错的数：投影必须无视它。
        session.transferred_bytes = 42;

        let projection = projection_of(&session, &[a, b]);

        assert_eq!(projection.transferred_bytes, 1000);
        assert_eq!(projection.content_root.as_deref(), Some("/recv"));
        assert_eq!(projection.files.len(), 2);
    }
}
