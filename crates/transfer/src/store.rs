//! 持久化端口（依赖倒置的核心）。
//!
//! transfer 只依赖这些 trait，不认识具体的 ORM 连接类型。桌面/移动端
//! 由 `swarmdrop-core` 的 `SqlSessionStore` 用 SeaORM 实现；Web 端未来可实现
//! IndexedDB/OPFS 版（见 `dev-notes/knowledge/storage-abstraction.md`）。
//!
//! 粒度取「用例级」：`create_session` 而非 `insert_session_row`+`insert_file_row`，
//! 事务是实现细节，trait 上不出现 `begin`/`commit`。entity 的 `Model` 是 wasm 可用的
//! 普通数据类型，可直接上签名（第 0 步已实测）。

use async_trait::async_trait;
use uuid::Uuid;

use crate::AppResult;
use crate::coordinator::TransferState;
use crate::host::{CoreSaveLocation, HostFileMetadata};
use crate::protocol::{FileInfo, TransferOrigin};

/// 会话/文件级持久化端口（断点续传的事实来源）。
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

    /// 重置文件 checkpoint（bitmap 清零 + transferred_bytes 归零）。
    async fn reset_file_checkpoint(&self, session_id: Uuid, file_id: i32) -> AppResult<()>;

    /// 批量保存发送方 per-file 进度（`(file_id, chunks_done, transferred_bytes)`）。
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
}

/// 收件箱端口（transfer 只用「完成接收后幂等建条目」一个用例）。
#[async_trait]
pub trait InboxStore: Send + Sync {
    /// 从已完成接收会话幂等创建收件箱条目。失败仅作 DB 附加错误上报、不回滚传输，
    /// 故返回 `()`（调用方 receiver 不消费 detail）。
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<()>;
}

/// 传输域持久化的合并端口——便于 TransferManager / actor 单 `Arc` 注入。
///
/// blanket impl 覆盖任何同时实现两个子端口的类型，故实现方（`SqlSessionStore`）
/// 只需分别 impl [`SessionStore`] 与 [`InboxStore`]。
pub trait TransferStore: SessionStore + InboxStore {}
impl<T: SessionStore + InboxStore + ?Sized> TransferStore for T {}

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
    pub error_message: Option<String>,
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

impl From<entity::transfer_file::ModelEx> for TransferProjectionFile {
    fn from(f: entity::transfer_file::ModelEx) -> Self {
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
pub fn content_root_of<'a>(
    files: impl IntoIterator<Item = &'a entity::transfer_file::ModelEx>,
    save_path: Option<&entity::SaveLocation>,
) -> Option<String> {
    let mut dirs = files.into_iter().filter_map(|f| f.local_dir.as_deref());
    if let Some(first) = dirs.next()
        && dirs.all(|d| d == first)
    {
        return Some(first.to_string());
    }
    save_path.map(|entity::SaveLocation::Path { path }| path.clone())
}

impl From<entity::transfer_session::ModelEx> for TransferProjection {
    fn from(s: entity::transfer_session::ModelEx) -> Self {
        // transferred_bytes 派生自文件级求和（单一事实来源）：文件进度由 persist_chunk /
        // save_sender_file_progress 增量落库，projection 直接 SUM，省掉各生命周期转换前
        // 手工 sync_session_transferred_bytes 的二次写与漂移风险。
        let transferred_bytes = s.files.iter().map(|f| f.transferred_bytes).sum();
        let content_root = content_root_of(s.files.iter(), s.save_path.as_ref());
        Self {
            session_id: s.session_id,
            direction: s.direction,
            peer_id: s.peer_id.0,
            peer_name: s.peer_name,
            phase: s.phase,
            suspended_reason: s.suspended_reason,
            terminal_reason: s.terminal_reason,
            recoverable: s.recoverable,
            epoch: s.epoch,
            total_size: s.total_size,
            transferred_bytes,
            started_at: s.started_at,
            updated_at: s.updated_at,
            finished_at: s.finished_at,
            error_message: s.error_message,
            policy_action: s.policy_action,
            policy_reason: s.policy_reason,
            save_path: s.save_path.map(Into::into),
            content_root,
            files: s.files.into_iter().map(Into::into).collect(),
        }
    }
}

/// 解析文件 `completed_ranges` JSON（数据面/续传计划共用）。
pub fn parse_completed_ranges(value: &str) -> Vec<(u64, u64)> {
    serde_json::from_str(value).unwrap_or_default()
}
