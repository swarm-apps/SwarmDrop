//! 邀请注册表的持久化端口（openspec: invite-persistence）。
//!
//! TTL 从 5 分钟放到 24 小时后，纯内存的注册表不再够用 —— 「发条链接给同事，自己顺手重启了
//! App」这条最普通的路径原本必然失败。本模块把注册表的落盘抽成端口，native 由
//! `swarmdrop-storage-sql` 用 SeaORM 实现，Web 由 `swarmdrop-web` 用 IndexedDB 写穿实现。
//!
//! 端口只依赖本 crate 自己的类型（`NodeId` 来自 net-base），**不认识 entity、不认识 ORM**
//! —— 这是 `crates/invite` 保持 wasm-clean 的前提。
//!
//! ## 写路径返回「成没成」，读路径不返回错误
//!
//! 内存表是一次性消费语义的**权威判定点**（CAS 在单锁内完成），落盘是它的写穿备份。
//! 但**写穿失败不能被端口吞掉** —— 否则注册表根本不知道自己的状态没落地，
//! 也就无法在需要 fail-closed 的路径上收手。
//!
//! 具体错在哪：`register` 已经把 `Pending` 写进库了，此后任何一次写穿失败（无论是
//! `upsert` 还是 `remove` 失败）都留下**同一个**结果 —— 库里仍是 `Pending`。
//! 于是重启 `load` 把它放回内存，那份邀请在剩余 TTL 内**重新可用**：
//!
//! | 失败的操作 | 库里留下 | 重启后 |
//! |---|---|---|
//! | `register` 的 upsert | 没有这行 | 「不认识」→ 拒绝（benign） |
//! | `try_consume` 的 upsert | `Pending` | **可再消费一次**（一次性语义破了） |
//! | `revoke` 的 upsert | `Pending` | **撤销失效，邀请复活** |
//!
//! 所以写方法返回 `bool`：调用方（注册表）据此决定是否让操作 fail-closed。
//! 用 `bool` 而不是 `Result` 是因为端口层没有统一错误类型（`crates/invite` 不依赖 host），
//! 而调用方只需要「成没成」这一位信息 —— 错误详情归实现层的日志。
//!
//! 读路径相反：[`InviteStore::load_all`] 失败按「无记录」处理。邀请是短时凭证，
//! 最坏结果是用户重新生成一次，比启动失败好得多。
//!
//! 完整推导见 `openspec/changes/invite-persistence/design.md` D2。

use async_trait::async_trait;
use swarmdrop_net_base::NodeId;

/// 一条落盘的邀请记录。
///
/// **不含 capability 明文，也不含邀请全串** —— 只有 capability 的 sha256。库被读到也拿不到
/// 能用的凭证；代价是重启后邀请列表只能显示元数据，显示不出原始链接（design D4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRecord {
    /// `sha256(capability)`，查找与消费都按它定位。
    pub capability_hash: [u8; 32],
    /// 发起方身份（当前恒为本机）。
    pub inviter_id: NodeId,
    /// 过期时刻（Unix 秒）。
    pub expires_at: u64,
    pub state: PersistedInviteState,
    /// 创建时刻（Unix 秒），邀请列表按它倒序。
    pub created_at: u64,
}

/// 落盘的邀请状态。与内存态（`invite.rs` 的 `InviteState`）一一对应。
///
/// **`Revoked` 独立于 `Consumed` 是 UX 需要**：撤销与「被对方用掉」在列表里要能区分，
/// 否则用户会看到自己撤销的那条显示成「已被对方使用」。
///
/// ⚠️ **它不是安全措施。** 早期注释写过「撤销走写状态而非删行，所以对写穿失败 fail-closed」
/// —— 那是错的，由审查用探针实测推翻：`register` 已经把 `Pending` 写进库了，此后 UPDATE
/// 失败与 DELETE 失败留下的是**同一个**结果（库里仍是 `Pending`），两者都 fail-open。
/// 真正的防线是写方法返回 `bool` + 调用方在关键路径上 fail-closed（见模块文档）。
///
/// `Consumed` / `Revoked` 都保留到过期之后才清，同样是 **UX 而非安全**：让发起方看到
/// 这条的去向，而不是让它从列表里凭空消失。安全侧不依赖它 —— 注册表查不到直接返回
/// `InviteRejectReason::Unknown`（拒绝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistedInviteState {
    Pending,
    Consumed,
    Revoked,
}

/// 邀请注册表的落盘端口。
#[async_trait]
pub trait InviteStore: Send + Sync {
    /// 启动时全量读回（含 `Consumed` / `Revoked` 但未过期的记录 —— 它们既要挡住再次消费，
    /// 也要让邀请列表显示这条的去向）。
    ///
    /// 读失败返回空 Vec 并由实现记日志，不 panic、不上报。
    async fn load_all(&self) -> Vec<InviteRecord>;

    /// 写穿一条记录（新增或状态变更）。以 `capability_hash` 为键 upsert。
    ///
    /// **返回是否写入成功**（见模块文档）。实现负责记日志，调用方只看这一位。
    #[must_use = "写穿失败时调用方需要决定是否 fail-closed"]
    async fn upsert(&self, record: InviteRecord) -> bool;

    /// 物理删除一条。**返回是否成功。**
    ///
    /// **注册表不用它做撤销** —— 撤销写 [`PersistedInviteState::Revoked`]，
    /// 因为「已撤销」与「已被消费」在列表里要能区分开。保留这个方法是因为实现内部会用它
    /// 拼 [`Self::prune_expired`]（IndexedDB 没有条件批删，只能读回来逐条删）。
    #[must_use = "删除失败时调用方需要决定是否 fail-closed"]
    async fn remove(&self, capability_hash: [u8; 32]) -> bool;

    /// 删除所有 `expires_at <= now` 的记录。
    ///
    /// 不返回结果：清理失败只让表暂时大一点，下次启动会再试，没有安全后果。
    async fn prune_expired(&self, now: u64);
}

/// 不落盘的实现 —— 保持旧的「重启丢邀请」语义。
///
/// 给两类调用方用：不需要持久化的宿主（测试、CLI 工具），以及落盘实现尚未就绪时的占位。
/// 生产路径上桌面/移动走 SQL、Web 走 IndexedDB。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopInviteStore;

#[async_trait]
impl InviteStore for NoopInviteStore {
    async fn load_all(&self) -> Vec<InviteRecord> {
        Vec::new()
    }
    // 报 true：不落盘是这个实现的**契约**，不是失败 —— 报 false 会让 try_consume
    // 在每次消费时都 fail-closed，那等于把「不需要持久化」变成「配对不可用」。
    async fn upsert(&self, _record: InviteRecord) -> bool {
        true
    }
    async fn remove(&self, _capability_hash: [u8; 32]) -> bool {
        true
    }
    async fn prune_expired(&self, _now: u64) {}
}
