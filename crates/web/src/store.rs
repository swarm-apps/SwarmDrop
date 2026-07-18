//! 持久化端口的**内存**实现（`SessionStore` + `InboxStore`）。
//!
//! Web 壳范围内无配对/历史持久化，会话/文件表存内存即可（页面刷新即丢，符合「传输端」
//! 定位）。entity 的 `Model` 是纯 scalar 结构（关系字段只在 `ModelEx`），可直接手构造——
//! 故本 crate **不直接依赖 sea-orm**（投影也直接构造 `TransferProjection`，绕开 `ModelEx`
//! 的 `HasMany` 关系类型）。`std::sync::Mutex` 在单线程 wasm 下等价无锁，满足端口的
//! Send+Sync（数据都是普通值，无 !Send）。
//!
//! **加分项 IndexedDB 版未做**：范围内内存版足够验证传输端到端；持久化断点续传跨刷新
//! 属后续（storage-abstraction.md 探针已证 `send_wrapper::SendWrapper` 包 JsFuture 的 Send
//! 方案可行）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use swarmdrop_host::{AppResult, CoreSaveLocation};
use swarmdrop_transfer::calc_total_chunks;
use swarmdrop_transfer::coordinator::TransferState;
use swarmdrop_transfer::store::{
    CreateSessionInput, InboxStore, SessionStore, TransferProjection, TransferProjectionFile,
};
use uuid::Uuid;

/// 一个会话的内存记录：会话行 + 其文件行。
struct StoredSession {
    session: entity::transfer_session::Model,
    files: Vec<entity::transfer_file::Model>,
}

/// 内存 `SessionStore`/`InboxStore`。
#[derive(Default)]
pub struct MemorySessionStore {
    sessions: Mutex<HashMap<Uuid, StoredSession>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn create_session(&self, input: CreateSessionInput<'_>) -> AppResult<()> {
        let now = Self::now_ms();
        let CreateSessionInput {
            session_id,
            direction,
            peer_id,
            peer_name,
            files,
            total_size,
            save_path,
            source_paths,
            lifecycle,
            policy,
            origin,
        } = input;
        let (policy_action, policy_reason) = match policy {
            Some((a, r)) => (Some(a.to_string()), Some(r.to_string())),
            None => (None, None),
        };

        let session = entity::transfer_session::Model {
            session_id,
            direction: direction.clone(),
            peer_id: entity::PeerId(peer_id.to_string()),
            peer_name: peer_name.to_string(),
            total_size: total_size as i64,
            transferred_bytes: 0,
            status: lifecycle
                .phase
                .legacy_status(lifecycle.terminal_reason.as_ref()),
            phase: lifecycle.phase.clone(),
            suspended_reason: lifecycle.suspended_reason.clone(),
            terminal_reason: lifecycle.terminal_reason.clone(),
            epoch: lifecycle.epoch,
            recoverable: lifecycle.recoverable,
            source_fingerprint: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            error_message: lifecycle.error_message.clone(),
            policy_action,
            policy_reason,
            origin: origin.map(|o| o.to_db_string()),
            save_path: save_path.map(Into::into),
        };

        let file_models = files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let total_chunks = calc_total_chunks(file.size) as i32;
                let bitmap_len = (total_chunks as usize).div_ceil(8);
                let completed_chunks = if direction == entity::TransferDirection::Receive {
                    vec![0u8; bitmap_len]
                } else {
                    vec![]
                };
                entity::transfer_file::Model {
                    id: idx as i32,
                    session_id,
                    file_id: file.file_id as i32,
                    name: file.name.clone(),
                    relative_path: file.relative_path.clone(),
                    size: file.size as i64,
                    checksum: file.checksum.clone(),
                    status: entity::FileStatus::Pending,
                    transferred_bytes: 0,
                    total_chunks,
                    completed_chunks,
                    completed_ranges: "[]".to_string(),
                    source_path: source_paths.and_then(|p| p.get(idx).cloned()),
                    local_path: None,
                    local_dir: None,
                    outboard: None,
                }
            })
            .collect();

        self.sessions.lock().unwrap().insert(
            session_id,
            StoredSession {
                session,
                files: file_models,
            },
        );
        Ok(())
    }

    async fn update_session_save_path(
        &self,
        session_id: Uuid,
        save_path: CoreSaveLocation,
    ) -> AppResult<()> {
        let mut map = self.sessions.lock().unwrap();
        if let Some(s) = map.get_mut(&session_id) {
            s.session.save_path = Some(save_path.into());
            s.session.updated_at = Self::now_ms();
        }
        Ok(())
    }

    async fn update_file_checkpoint_ranges(
        &self,
        session_id: Uuid,
        file_id: i32,
        completed_chunks: Vec<u8>,
        completed_ranges: &[(u64, u64)],
        transferred_bytes: i64,
    ) -> AppResult<()> {
        self.mutate_file(session_id, file_id, |f| {
            f.completed_chunks = completed_chunks;
            f.completed_ranges = ranges_json(completed_ranges);
            f.transferred_bytes = transferred_bytes;
        });
        Ok(())
    }

    async fn mark_file_completed(
        &self,
        session_id: Uuid,
        file_id: i32,
        completed_chunks: Vec<u8>,
        transferred_bytes: i64,
        local_path: String,
        local_dir: String,
    ) -> AppResult<()> {
        self.mutate_file(session_id, file_id, |f| {
            f.status = entity::FileStatus::Completed;
            f.completed_chunks = completed_chunks;
            f.transferred_bytes = transferred_bytes;
            f.completed_ranges = ranges_json(&prefix_range(transferred_bytes));
            f.local_path = Some(local_path);
            f.local_dir = Some(local_dir);
        });
        Ok(())
    }

    async fn reset_file_checkpoint(&self, session_id: Uuid, file_id: i32) -> AppResult<()> {
        self.mutate_file(session_id, file_id, |f| {
            f.completed_chunks = vec![];
            f.completed_ranges = "[]".to_string();
            f.transferred_bytes = 0;
        });
        Ok(())
    }

    async fn save_sender_file_progress(
        &self,
        session_id: Uuid,
        progress: &[(u32, u32, u64)],
    ) -> AppResult<()> {
        for &(file_id, _chunks_done, transferred) in progress {
            if transferred > 0 {
                self.mutate_file(session_id, file_id as i32, |f| {
                    f.transferred_bytes = transferred as i64;
                });
            }
        }
        Ok(())
    }

    async fn apply_transition(
        &self,
        session: &entity::transfer_session::Model,
        state: &TransferState,
    ) -> AppResult<()> {
        let mut map = self.sessions.lock().unwrap();
        if let Some(s) = map.get_mut(&session.session_id) {
            s.session.phase = state.phase.clone();
            s.session.suspended_reason = state.suspended_reason.clone();
            s.session.terminal_reason = state.terminal_reason.clone();
            s.session.epoch = state.epoch;
            s.session.recoverable = state.recoverable;
            s.session.status = state.phase.legacy_status(state.terminal_reason.as_ref());
            s.session.updated_at = Self::now_ms();
            if state.is_terminal() {
                s.session.finished_at = Some(Self::now_ms());
            }
            if let Some(msg) = &state.error_message {
                s.session.error_message = Some(msg.clone());
            }
        }
        Ok(())
    }

    async fn find_active_session_ids(&self) -> AppResult<Vec<Uuid>> {
        Ok(self.active_ids(None))
    }

    async fn find_active_session_ids_by_peer(&self, peer_id: &str) -> AppResult<Vec<Uuid>> {
        Ok(self.active_ids(Some(peer_id)))
    }

    async fn find_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<entity::transfer_session::Model>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(&session_id)
            .map(|s| s.session.clone()))
    }

    async fn get_transfer_projection(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<TransferProjection>> {
        let map = self.sessions.lock().unwrap();
        Ok(map.get(&session_id).map(build_projection))
    }

    async fn get_session_files(
        &self,
        session_id: Uuid,
    ) -> AppResult<Vec<entity::transfer_file::Model>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(&session_id)
            .map(|s| s.files.clone())
            .unwrap_or_default())
    }

    async fn save_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
        outboard: Vec<u8>,
    ) -> AppResult<()> {
        self.mutate_file(session_id, file_id, |f| f.outboard = Some(outboard));
        Ok(())
    }

    async fn load_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
    ) -> AppResult<Option<Vec<u8>>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(&session_id)
            .and_then(|s| s.files.iter().find(|f| f.file_id == file_id))
            .and_then(|f| f.outboard.clone()))
    }
}

#[async_trait]
impl InboxStore for MemorySessionStore {
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        _session_id: Uuid,
    ) -> AppResult<()> {
        // Web 壳无收件箱：no-op（接收方本就只把返回值当「成功/失败」，见端口注释）。
        Ok(())
    }
}

impl MemorySessionStore {
    fn mutate_file(
        &self,
        session_id: Uuid,
        file_id: i32,
        apply: impl FnOnce(&mut entity::transfer_file::Model),
    ) {
        let mut map = self.sessions.lock().unwrap();
        if let Some(s) = map.get_mut(&session_id) {
            if let Some(f) = s.files.iter_mut().find(|f| f.file_id == file_id) {
                apply(f);
            }
            s.session.updated_at = Self::now_ms();
        }
    }

    fn active_ids(&self, peer: Option<&str>) -> Vec<Uuid> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.session.phase == entity::TransferPhase::Active)
            .filter(|s| peer.is_none_or(|p| s.session.peer_id.0 == p))
            .map(|s| s.session.session_id)
            .collect()
    }
}

/// 直接构造 `TransferProjection`（绕开 `ModelEx` 的 `HasMany` 关系类型，避免依赖 sea-orm）。
fn build_projection(s: &StoredSession) -> TransferProjection {
    let transferred_bytes = s.files.iter().map(|f| f.transferred_bytes).sum();
    let save_path = s.session.save_path.clone().map(CoreSaveLocation::from);
    let content_root = content_root_of(&s.files, &save_path);
    TransferProjection {
        session_id: s.session.session_id,
        direction: s.session.direction.clone(),
        peer_id: s.session.peer_id.0.clone(),
        peer_name: s.session.peer_name.clone(),
        phase: s.session.phase.clone(),
        suspended_reason: s.session.suspended_reason.clone(),
        terminal_reason: s.session.terminal_reason.clone(),
        recoverable: s.session.recoverable,
        epoch: s.session.epoch,
        total_size: s.session.total_size,
        transferred_bytes,
        started_at: s.session.started_at,
        updated_at: s.session.updated_at,
        finished_at: s.session.finished_at,
        error_message: s.session.error_message.clone(),
        policy_action: s.session.policy_action.clone(),
        policy_reason: s.session.policy_reason.clone(),
        save_path,
        content_root,
        files: s
            .files
            .iter()
            .map(|f| TransferProjectionFile {
                file_id: f.file_id,
                name: f.name.clone(),
                relative_path: f.relative_path.clone(),
                size: f.size,
                transferred_bytes: f.transferred_bytes,
            })
            .collect(),
    }
}

/// 内联 `content_root_of`（与 transfer 侧同语义）：所有文件 `local_dir` 唯一一致 → 该目录；
/// 否则回退存储根 `save_path`。避免为 `content_root_of` 的 `ModelEx` 签名引入 sea-orm。
fn content_root_of(
    files: &[entity::transfer_file::Model],
    save_path: &Option<CoreSaveLocation>,
) -> Option<String> {
    let mut dirs = files.iter().filter_map(|f| f.local_dir.as_deref());
    if let Some(first) = dirs.next()
        && dirs.all(|d| d == first)
    {
        return Some(first.to_string());
    }
    save_path
        .as_ref()
        .map(|CoreSaveLocation::Path { path }| path.clone())
}

fn ranges_json(ranges: &[(u64, u64)]) -> String {
    serde_json::to_string(ranges).unwrap_or_else(|_| "[]".to_string())
}

fn prefix_range(transferred_bytes: i64) -> Vec<(u64, u64)> {
    if transferred_bytes > 0 {
        vec![(0, transferred_bytes as u64)]
    } else {
        Vec::new()
    }
}
