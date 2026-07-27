//! 持久化端口实现（`SessionStore` + `InboxStore`）：**内存读缓存 + IndexedDB 写穿**。
//!
//! 读路径全部走内存 `HashMap`（同步、零 await），写路径在改完内存后把该会话的快照写进
//! IndexedDB 的 `sessions` store（一条会话一个 key）。启动时 [`PersistentSessionStore::load`]
//! 全量读回，于是**刷新页面后收件箱、传输历史与接收侧续传上下文仍在**。
//!
//! entity 的 `Model` 是纯 scalar 结构（关系字段只在 `ModelEx`），可直接手构造——故本 crate
//! **不直接依赖 sea-orm**（投影也直接构造 `TransferProjection`，绕开 `ModelEx` 的 `HasMany`
//! 关系类型）。`Model` 不可直接序列化（`#[sea_orm::model]` 不把用户 derive 转发给它），
//! 落库形态用 serde 的 remote derive 声明在文件末尾——那里就是这份存储的 wire 格式定义点。
//!
//! ## 落库范围（并非所有会话都写）
//!
//! | 会话 | 落库 | 为什么 |
//! |---|---|---|
//! | `phase=Terminal`（收发双向）| ✅ | 收件箱与历史留痕 |
//! | `Active` / `Suspended` 且 `direction=Receive` | ✅ | OPFS 里的 `.part` 与 checkpoint 都还在，刷新后可续传 |
//! | 其余（所有非终态发送 + `Offered`/`WaitingAccept` 接收）| ❌ | **恢复不了，落库只会留幽灵条目** |
//!
//! 最后一行是浏览器的物理约束，不是偷懒：发送侧的文件内容来自用户选中的 `File` 对象
//! （`OpfsFileAccess::register_source`），页面刷新后 JS 上下文销毁、无法在未经用户重新选择
//! 的情况下再读同一个文件，所以非终态发送会话恢复出来也只能给用户一个点了必失败的「续传」
//! 按钮；待决 offer 同理——`pending_offers()` 是 `TransferManager` 的内存态，刷新后对端的
//! offer 已无处应答。
//!
//! 落库的 `Active` 接收会话在 `WebNode::spawn` 里由 `TransferCoordinator::cleanup_recoverable_sessions`
//! 统一转成 recoverable `suspended(AppRestarted)`（本模块不自己特判）；超过保留期仍未恢复的
//! 接收会话在 [`load`](PersistentSessionStore::load) 就地转终态并写明原因——两件事都与桌面的
//! `cleanup_stale_sessions` 对齐，见 [`reap`] 与 [`is_expired_recoverable_receive`]。
//!
//! ## 不落库的两样东西
//!
//! - **bao outboard**（1 GiB 文件 ≈ 4 MiB）：只有发送侧用得上，而发送侧本就不跨刷新恢复。
//! - **`InboxStore`**：Web 壳没有独立收件箱表，收件箱就是「`direction=Receive` 且
//!   `terminal=Completed`」的会话投影（前端据此渲染），故 `ensure_inbox_item_*` 仍是 no-op。
//!
//! `std::sync::Mutex` 在单线程 wasm 下等价无锁，满足端口的 Send+Sync；IndexedDB 的 `JsFuture`
//! 是 `!Send`，用 `SendWrapper` 裹住以满足 `#[async_trait]` 的 Send 约束（见
//! `dev-notes/knowledge/storage-abstraction.md`，wasm 单线程下跨线程 panic 永不触发）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use swarmdrop_host::{AppError, AppResult, CoreSaveLocation};
use swarmdrop_transfer::coordinator::TransferState;
use swarmdrop_transfer::store::{
    CreateSessionInput, InboxStore, SessionStore, TransferProjection, TransferProjectionFile,
};
use swarmdrop_transfer::store::{initial_completed_chunks, prefix_range, ranges_json};
use swarmdrop_transfer::{SUSPENDED_RECEIVE_RETENTION_SECS, calc_total_chunks};
use uuid::Uuid;

use crate::idb;

/// 保留的终态会话条数上限（按 `updated_at` 保新）。浏览器存储配额远小于桌面磁盘，
/// 历史无限增长会挤占 OPFS 里真正的文件数据。
const HISTORY_CAP: usize = 100;

/// 一个会话的内存记录：会话行 + 其文件行。
struct StoredSession {
    session: entity::transfer_session::Model,
    files: Vec<entity::transfer_file::Model>,
}

/// IndexedDB 写穿的 `SessionStore`/`InboxStore`。
#[derive(Default)]
pub struct PersistentSessionStore {
    sessions: Mutex<HashMap<Uuid, StoredSession>>,
}

impl PersistentSessionStore {
    /// 从 IndexedDB 恢复。单条记录损坏只跳过它（warn）而非让整个节点起不来；
    /// 读不到库（隐私模式 / 配额拒绝）时退化成纯内存 store，节点照常可用。
    pub async fn load() -> Self {
        let store = Self::default();
        let records = match SendWrapper::new(idb::get_all(idb::SESSION_STORE)).await {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!("读取传输会话持久化失败，本次以内存态运行: {e:?}");
                return store;
            }
        };

        let cutoff = Self::now_ms() - (SUSPENDED_RECEIVE_RETENTION_SECS as i64) * 1000;
        let mut reaped = Vec::new();
        {
            let mut map = store.sessions.lock().unwrap();
            for record in records {
                let Some(json) = record.as_string() else {
                    tracing::warn!("跳过一条非字符串的会话记录");
                    continue;
                };
                let persisted: PersistedSession = match serde_json::from_str(&json) {
                    Ok(value) => value,
                    Err(e) => {
                        tracing::warn!("跳过一条无法解析的会话记录: {e}");
                        continue;
                    }
                };
                let mut stored = StoredSession::from(persisted);
                if is_expired_recoverable_receive(&stored.session, cutoff) {
                    reap(&mut stored.session);
                    reaped.push(stored.session.session_id);
                }
                map.insert(stored.session.session_id, stored);
            }
        }

        // 回写被回收的终态（记录留在历史里、写明原因，不让它凭空消失——与桌面
        // `reap_expired_suspended_receives` 同语义）。
        for session_id in reaped {
            if let Err(e) = store.persist(session_id).await {
                tracing::warn!("回写过期回收的会话 {session_id} 失败: {e}");
            }
        }
        store.prune().await;
        store
    }

    /// 全部会话的投影（前端刷新后回补收件箱与活动视图）。
    ///
    /// 不排序：两个消费面板（收件箱按结束时间、活动视图按更新时间）排法本就不同，
    /// 且投影经 JS 对象汇入 store 后顺序不再可依赖——排序职责单点留在前端。
    pub fn all_projections(&self) -> Vec<TransferProjection> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .map(build_projection)
            .collect()
    }

    fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }

    /// 把某会话的当前内存态写进 IndexedDB；不属于落库范围的会话直接跳过（见模块注释）。
    async fn persist(&self, session_id: Uuid) -> AppResult<()> {
        let Some(persisted) = self.snapshot(session_id) else {
            return Ok(());
        };
        let json = serde_json::to_string(&persisted)
            .map_err(|e| AppError::Transfer(format!("序列化传输会话失败: {e}")))?;
        let key = session_id.to_string();
        SendWrapper::new(idb::put_string(idb::SESSION_STORE, &key, &json))
            .await
            .map_err(AppError::from)
    }

    /// 取快照：会话存在且值得落库时返回 DTO，否则 `None`。持锁期间不 await。
    fn snapshot(&self, session_id: Uuid) -> Option<PersistedSession> {
        let map = self.sessions.lock().unwrap();
        let stored = map.get(&session_id)?;
        worth_persisting(&stored.session).then(|| PersistedSession::from(stored))
    }

    /// 从内存与 IndexedDB 一并移除（过期/超额淘汰）。失败只 warn——淘汰不是关键路径。
    async fn forget(&self, session_id: Uuid) {
        self.sessions.lock().unwrap().remove(&session_id);
        if let Err(e) =
            SendWrapper::new(idb::delete(idb::SESSION_STORE, &session_id.to_string())).await
        {
            tracing::warn!("删除过期传输会话 {session_id} 失败: {e:?}");
        }
    }

    /// 终态会话超过 [`HISTORY_CAP`] 时淘汰最旧的几条（非终态不参与淘汰——它们还要续传）。
    async fn prune(&self) {
        let victims = {
            let map = self.sessions.lock().unwrap();
            let mut terminal: Vec<(i64, Uuid)> = map
                .values()
                .filter(|s| s.session.phase == entity::TransferPhase::Terminal)
                .map(|s| (s.session.updated_at, s.session.session_id))
                .collect();
            if terminal.len() <= HISTORY_CAP {
                return;
            }
            terminal.sort_unstable_by_key(|(updated_at, _)| std::cmp::Reverse(*updated_at));
            terminal
                .split_off(HISTORY_CAP)
                .into_iter()
                .map(|(_, id)| id)
                .collect::<Vec<_>>()
        };
        for session_id in victims {
            self.forget(session_id).await;
        }
    }

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

/// 过期回收的命中判定，与桌面 `reap_expired_suspended_receives` 同一组条件：
/// recoverable 的**接收**会话、非终态、且超过保留期未更新。
///
/// 桌面在 `cleanup_recoverable_sessions` 之后 reap，命中的必是 `Suspended`；Web 的回收发生在
/// 加载期（清理还没跑），遗留的 `Active` 尚未转 `Suspended`，故这里按「非终态」判而不按
/// `phase=Suspended` 判——两端命中的是同一批会话。
fn is_expired_recoverable_receive(
    session: &entity::transfer_session::Model,
    cutoff_ms: i64,
) -> bool {
    session.phase != entity::TransferPhase::Terminal
        && session.recoverable
        && session.direction == entity::TransferDirection::Receive
        && session.updated_at < cutoff_ms
}

/// 就地转终态，字段与桌面 reap 逐条对齐（含 `status=Failed` 与那句过期说明）。
///
/// 不经 `TransferCoordinator`：加载期还没有 coordinator，而且这是「载入时发现记录已过期」
/// 而非一次状态**转换**——没有对应的 `StartupSignal`，也不该发 projection（前端此刻还没订阅）。
fn reap(session: &mut entity::transfer_session::Model) {
    let now = js_sys::Date::now() as i64;
    let retention_days = SUSPENDED_RECEIVE_RETENTION_SECS / 86_400;
    session.phase = entity::TransferPhase::Terminal;
    session.suspended_reason = None;
    session.terminal_reason = Some(entity::TerminalReason::FatalError);
    session.status = entity::SessionStatus::Failed;
    session.recoverable = false;
    session.error_message = Some(format!("会话超过 {retention_days} 天未恢复，已过期回收"));
    session.finished_at = Some(now);
    session.updated_at = now;
}

/// 落库判定（模块注释的表格即此函数）。
fn worth_persisting(session: &entity::transfer_session::Model) -> bool {
    match session.phase {
        entity::TransferPhase::Terminal => true,
        entity::TransferPhase::Active | entity::TransferPhase::Suspended => {
            session.direction == entity::TransferDirection::Receive
        }
        entity::TransferPhase::Offered | entity::TransferPhase::WaitingAccept => false,
    }
}

#[async_trait]
impl SessionStore for PersistentSessionStore {
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
                let completed_chunks = initial_completed_chunks(file.size, direction.clone());
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
        self.persist(session_id).await
    }

    async fn update_session_save_path(
        &self,
        session_id: Uuid,
        save_path: CoreSaveLocation,
    ) -> AppResult<()> {
        {
            let mut map = self.sessions.lock().unwrap();
            if let Some(s) = map.get_mut(&session_id) {
                s.session.save_path = Some(save_path.into());
                s.session.updated_at = Self::now_ms();
            }
        }
        self.persist(session_id).await
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
        self.persist(session_id).await
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
        self.persist(session_id).await
    }

    async fn reset_file_checkpoint(&self, session_id: Uuid, file_id: i32) -> AppResult<()> {
        self.mutate_file(session_id, file_id, |f| {
            f.completed_chunks = vec![];
            f.completed_ranges = "[]".to_string();
            f.transferred_bytes = 0;
        });
        self.persist(session_id).await
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
        self.persist(session_id).await
    }

    async fn apply_transition(
        &self,
        session: &entity::transfer_session::Model,
        state: &TransferState,
    ) -> AppResult<()> {
        {
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
        }
        self.persist(session.session_id).await?;
        // 进终态才可能挤爆历史上限，其余转换不必扫全表。
        if state.is_terminal() {
            self.prune().await;
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

    /// outboard 只留内存：唯一的消费方是发送端，而发送会话本就不跨刷新恢复（模块注释）。
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
impl InboxStore for PersistentSessionStore {
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        _session_id: Uuid,
    ) -> AppResult<()> {
        // Web 壳没有独立收件箱表：收件箱 = 已完成接收会话的投影（模块注释），会话本身
        // 已随 apply_transition 落库，这里无事可做（端口本就只把返回值当成功/失败）。
        Ok(())
    }
}

// ── 持久化 DTO ────────────────────────────────────────────────────────────────
//
// entity 的 `Model` 没有 serde derive（`#[sea_orm::model]` 不把用户 derive 转发给生成的
// `Model`，只到 `ModelEx`），故落库形态用 serde 的 **remote derive** 在此显式声明：字段清单
// 仍然逐个写出来（**这就是存储格式的声明点，改字段即改格式**），但省掉两个方向的手工搬运
// ——那种写法漏一个字段编译照过、只在运行时静默丢数据，而 remote derive 在 entity 加列时
// 直接编译不过。老记录解析失败会被 `load` 跳过（warn，不炸）。

#[derive(Serialize, Deserialize)]
struct PersistedSession {
    #[serde(with = "SessionRowDef")]
    session: entity::transfer_session::Model,
    files: Vec<PersistedFile>,
}

/// newtype 只为给 `Vec` 里的元素挂上 `with`（serde 的 `with` 作用于字段，不能作用于泛型参数）。
#[derive(Serialize, Deserialize)]
struct PersistedFile(#[serde(with = "FileRowDef")] entity::transfer_file::Model);

#[derive(Serialize, Deserialize)]
#[serde(remote = "entity::transfer_session::Model")]
#[allow(dead_code, reason = "字段只被 serde 生成的 remote 读写，本地无读取方")]
struct SessionRowDef {
    session_id: Uuid,
    direction: entity::TransferDirection,
    peer_id: entity::PeerId,
    peer_name: String,
    total_size: i64,
    transferred_bytes: i64,
    status: entity::SessionStatus,
    phase: entity::TransferPhase,
    suspended_reason: Option<entity::SuspendedReason>,
    terminal_reason: Option<entity::TerminalReason>,
    epoch: i64,
    recoverable: bool,
    source_fingerprint: Option<String>,
    started_at: i64,
    updated_at: i64,
    finished_at: Option<i64>,
    error_message: Option<String>,
    policy_action: Option<String>,
    policy_reason: Option<String>,
    origin: Option<String>,
    save_path: Option<entity::SaveLocation>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "entity::transfer_file::Model")]
#[allow(dead_code, reason = "字段只被 serde 生成的 remote 读写，本地无读取方")]
struct FileRowDef {
    id: i32,
    session_id: Uuid,
    file_id: i32,
    name: String,
    relative_path: String,
    size: i64,
    checksum: String,
    status: entity::FileStatus,
    transferred_bytes: i64,
    total_chunks: i32,
    completed_chunks: Vec<u8>,
    completed_ranges: String,
    source_path: Option<String>,
    local_path: Option<String>,
    local_dir: Option<String>,
    /// 不落库（1 GiB 文件 ≈ 4 MiB），载入恒 `None`——见模块注释「不落库的两样东西」。
    #[serde(skip)]
    outboard: Option<Vec<u8>>,
}

impl From<&StoredSession> for PersistedSession {
    fn from(s: &StoredSession) -> Self {
        Self {
            session: s.session.clone(),
            files: s.files.iter().cloned().map(PersistedFile).collect(),
        }
    }
}

impl From<PersistedSession> for StoredSession {
    fn from(p: PersistedSession) -> Self {
        Self {
            session: p.session,
            files: p.files.into_iter().map(|f| f.0).collect(),
        }
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
