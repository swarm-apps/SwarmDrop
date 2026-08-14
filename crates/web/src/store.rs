//! 持久化端口实现（`SessionStore` + `InboxStore`）：**内存读缓存 + IndexedDB 写穿**。
//!
//! 读路径全部走内存 `HashMap`（同步、零 await），写路径在改完内存后把该会话的快照写进
//! IndexedDB 的 `sessions` store（一条会话一个 key）。启动时 [`WebTransferStore::load`]
//! 全量读回，于是**刷新页面后收件箱、传输历史与接收侧续传上下文仍在**。
//!
//! `InboxStore` 那一半整体委托给 [`WebInboxTable`](crate::inbox)：它是另一张表、另一个
//! object store，本文件只负责在 `ensure_*` / `repair_*` 时把会话行与文件行**传给它**
//! （依赖方向单向，收件箱表不认识会话表）。
//!
//! entity 的 `Model` 是纯 scalar 结构（关系字段只在 `ModelEx`），可直接手构造——故本 crate
//! **不直接依赖 sea-orm**。投影统一走共享的
//! [`projection_of`](swarmdrop_transfer::store::projection_of)：它吃纯 `Model`、不碰
//! `ModelEx` 的 `HasMany`，Web 侧因此不需要（也不许）再有第二份构造器——
//! `transferred_bytes` 由文件级求和派生是**语义规则**，抄一份就等着两端进度显示不一致。
//! `Model` 不可直接序列化（`#[sea_orm::model]` 不把用户 derive 转发给它），
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
//! 统一转成 recoverable `suspended(AppRestarted)`（本模块不自己特判）；紧随其后的
//! [`reap_expired_suspended_receives`](SessionStore::reap_expired_suspended_receives) 把超过保留期
//! 仍未恢复的接收会话就地转终态并写明原因。两步的**顺序**与桌面 `cleanup_stale_sessions` 逐条对齐
//! ——回收曾跑在 `load()` 里（节点起来之前），那时遗留会话还没转 `Suspended`，判据只好分叉成
//! 「非终态」；调用点挪到清理之后，两端就共用同一条 `phase = Suspended` 判据了。
//!
//! ## 不落库的一样东西
//!
//! - **bao outboard**（1 GiB 文件 ≈ 256 KiB；chunk group 与 `CHUNK_SIZE` 对齐前是 4 MiB）：
//!   只有发送侧用得上，而发送侧本就不跨刷新恢复。
//!
//! 收件箱**不在**上面这张表里：它是独立的 `inbox` object store（见 [`crate::inbox`]），
//! 因而不参与下面的 [`HISTORY_CAP`] 淘汰——「清空传输历史不动收件箱」这条三端不变量
//! 在浏览器上才真正成立（投影方案做不到）。
//!
//! `std::sync::Mutex` 在单线程 wasm 下等价无锁，满足端口的 Send+Sync；IndexedDB 的 `JsFuture`
//! 是 `!Send`，用 `SendWrapper` 裹住以满足 `#[async_trait]` 的 Send 约束（见
//! `dev-notes/knowledge/storage-abstraction.md`，wasm 单线程下跨线程 panic 永不触发）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use swarmdrop_host::{AppError, AppResult, CoreSaveLocation, HostFileMetadata};
use swarmdrop_transfer::coordinator::TransferState;
use swarmdrop_transfer::inbox::{
    InboxItemContent, InboxItemDetail, InboxItemSummary, InboxSearchHit, is_completed_receive,
};
use swarmdrop_transfer::protocol::TransferOrigin;
use swarmdrop_transfer::store::{
    CreateSessionInput, ExpiredReceiverActor, InboxStore, SessionStore, TextDeliveryStore,
    TransferProjection, TransferStore,
};
use swarmdrop_transfer::store::{
    initial_completed_chunks, prefix_range, projection_of, ranges_json,
};
use swarmdrop_transfer::text_delivery::{TextDeliveryRecord, validate_text_body};
use swarmdrop_transfer::{calc_total_chunks, expired_receive_reason};
use uuid::Uuid;

use crate::idb;
use crate::inbox::WebInboxTable;

/// 保留的终态会话条数上限（按 `updated_at` 保新）。浏览器存储配额远小于桌面磁盘，
/// 历史无限增长会挤占 OPFS 里真正的文件数据。
const HISTORY_CAP: usize = 100;

/// 一个会话的内存记录：会话行 + 其文件行。
struct StoredSession {
    session: entity::transfer_session::Model,
    files: Vec<entity::transfer_file::Model>,
}

/// IndexedDB 写穿的 `SessionStore` + `InboxStore`（即合并端口 `TransferStore`）。
///
/// 名字对齐它满足的合并端口而不是其中一半：真收件箱表落地后，叫「SessionStore」
/// 就是个谎——它同时是两本账的持有者。
#[derive(Default)]
pub struct WebTransferStore {
    sessions: Mutex<HashMap<Uuid, StoredSession>>,
    /// 发送方向的文本账本。接收方向与 Inbox 共存于 `WebInboxTable`，见其单记录原子写入。
    text_outbox: Mutex<HashMap<Uuid, TextDeliveryRecord>>,
    /// 收件箱表（另一个 object store）。`InboxStore` 的 10 个方法全部委托给它，
    /// 只有 `repair_*` 留在本类型上——那条要同时看见会话与收件箱两边。
    inbox: WebInboxTable,
}

impl WebTransferStore {
    /// 从 IndexedDB 恢复（会话表 + 收件箱表各自全量读回）。单条记录损坏只跳过它（warn）
    /// 而非让整个节点起不来；读不到库（隐私模式 / 配额拒绝）时退化成纯内存 store，节点照常可用。
    ///
    /// **不做任何回填**：老库里已完成的接收会话不会在这里被补出收件箱条目（design D7），
    /// 收件箱从空开始是预期结果。要补是显式调 `repair_*`，那是修复通道不是迁移。
    pub async fn load() -> Self {
        // 两张表的全量读互不依赖，却排在**节点 spawn 的关键路径**上——串行发起就是两次
        // IndexedDB 往返摞在启动时延里。并发跑它们；两个 future 都持 `JsValue`（`!Send`），
        // 故 join 必须发生在同一个 `SendWrapper` 边界**之内**（在外面 join 等于把 !Send
        // 的组合 future 暴露给 Send 约束）。
        let (inbox, records, text_outbox) = SendWrapper::new(async {
            futures::join!(
                WebInboxTable::load(),
                idb::get_all(idb::SESSION_STORE),
                idb::get_all(idb::TEXT_OUTBOX_STORE),
            )
        })
        .await;

        let store = Self {
            inbox,
            ..Self::default()
        };
        if let Ok(records) = text_outbox {
            let mut outbox = store.text_outbox.lock().unwrap();
            for record in records {
                let Some(json) = record.as_string() else {
                    tracing::warn!("跳过一条非字符串的文本发送记录");
                    continue;
                };
                match serde_json::from_str::<TextDeliveryRecord>(&json) {
                    Ok(record) => {
                        outbox.insert(record.delivery_id, record);
                    }
                    Err(error) => tracing::warn!("跳过一条无法解析的文本发送记录: {error}"),
                }
            }
        } else {
            tracing::warn!("读取文本发送账本失败，本次以内存态运行");
        }
        let records = match records {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!("读取传输会话持久化失败，本次以内存态运行: {e:?}");
                return store;
            }
        };

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
                let stored = StoredSession::from(persisted);
                map.insert(stored.session.session_id, stored);
            }
        }
        store.prune().await;
        store
    }

    fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }

    /// 把某会话的当前内存态写进 IndexedDB；不属于落库范围的会话直接跳过（见模块注释）。
    async fn persist(&self, session_id: Uuid) -> AppResult<()> {
        self.persist_many(Some(session_id)).await
    }

    /// 批量写穿：全部快照排进**一个** IndexedDB 事务，只等一次提交。
    ///
    /// 单条路径也走它（一次 `Vec` 分配 vs 一次 IndexedDB 往返，量级不可比），
    /// 于是「取快照 → 序列化 → 落库」只有一份实现。
    async fn persist_many(&self, session_ids: impl IntoIterator<Item = Uuid>) -> AppResult<()> {
        let mut entries = Vec::new();
        for session_id in session_ids {
            let Some(persisted) = self.snapshot(session_id) else {
                continue;
            };
            let json = serde_json::to_string(&persisted)
                .map_err(|e| AppError::Transfer(format!("序列化传输会话失败: {e}")))?;
            entries.push((session_id.to_string(), json));
        }
        SendWrapper::new(idb::put_many(idb::SESSION_STORE, &entries))
            .await
            .map_err(AppError::from)
    }

    /// 取快照：会话存在且值得落库时返回 DTO，否则 `None`。持锁期间不 await。
    fn snapshot(&self, session_id: Uuid) -> Option<PersistedSession> {
        let map = self.sessions.lock().unwrap();
        let stored = map.get(&session_id)?;
        worth_persisting(&stored.session).then(|| PersistedSession::from(stored))
    }

    /// 从内存与 IndexedDB 一并移除，**失败上报**。
    ///
    /// 用户点的删除只删掉内存的话，刷新页面记录就会复活（#104 的验收标准正是这一条），
    /// 所以这条路径必须把 IndexedDB 的失败透出去。
    async fn remove(&self, session_id: Uuid) -> AppResult<()> {
        self.remove_many(&[session_id]).await
    }

    /// 批量移除：内存先删干净，IndexedDB 侧**一个事务删完全部键**。
    ///
    /// 逐条 `remove()` 的代价不是常数——「清空历史」最多 100 条终态会话，逐条就是
    /// 100 个独立事务、100 次串行往返。
    async fn remove_many(&self, session_ids: &[Uuid]) -> AppResult<()> {
        {
            let mut map = self.sessions.lock().unwrap();
            for session_id in session_ids {
                map.remove(session_id);
            }
        }
        let keys: Vec<String> = session_ids.iter().map(Uuid::to_string).collect();
        SendWrapper::new(idb::delete_many(idb::SESSION_STORE, &keys))
            .await
            .map_err(AppError::from)
    }

    /// 淘汰路径的尽力移除（过期/超额）。失败只 warn——淘汰不是关键路径，下次启动还会再扫一遍。
    async fn forget_many(&self, session_ids: &[Uuid]) {
        if let Err(e) = self.remove_many(session_ids).await {
            tracing::warn!("淘汰 {} 条传输会话失败: {e}", session_ids.len());
        }
    }

    /// 终态会话超过 [`HISTORY_CAP`] 时淘汰最旧的几条（非终态不参与淘汰——它们还要续传）。
    ///
    /// **只淘汰 sessions，收件箱条目不在其列**：收件箱是结果账本，被过程账本的容量上限
    /// 挤掉毫无道理。这正是「真表」相对旧的「已完成接收会话投影」方案的实质差异——
    /// 投影方案下会话一被淘汰，用户收到过的东西就凭空消失了。
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
        self.forget_many(&victims).await;
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

    /// 取一条会话的行副本（会话行 + 文件行）。收件箱表按设计不认识会话表，
    /// 建条目要的两样事实由这里摊出来再传给它。
    fn session_rows(
        &self,
        session_id: Uuid,
    ) -> Option<(
        entity::transfer_session::Model,
        Vec<entity::transfer_file::Model>,
    )> {
        let map = self.sessions.lock().unwrap();
        map.get(&session_id)
            .map(|s| (s.session.clone(), s.files.clone()))
    }

    /// 给收件箱详情补上关联传输会话的投影。
    ///
    /// [`WebInboxTable`] 一律返回 `transfer: None`（它看不见会话表），这一格由本类型补——
    /// 与 SQL 实现在 `detail_from_model` 里查一次投影是同一件事，只是这边的会话在内存里。
    /// 会话已被删除或清空时留 `None`，与 SQL 侧外键置空后的表现一致。
    fn attach_transfer(&self, mut detail: InboxItemDetail) -> InboxItemDetail {
        self.attach_transfers(std::slice::from_mut(&mut detail));
        detail
    }

    /// [`Self::attach_transfer`] 的批量版：整批详情共用**一次** sessions 加锁。
    ///
    /// 列表路径逐条补投影会把「一次列表」变成 N 次加锁，而这些详情本就来自同一个快照时刻。
    fn attach_transfers(&self, details: &mut [InboxItemDetail]) {
        let map = self.sessions.lock().unwrap();
        for detail in details {
            if let InboxItemContent::Files { transfer, .. } = &mut detail.content {
                *transfer = detail
                    .item
                    .transfer_session_id
                    .and_then(|session_id| map.get(&session_id))
                    .map(|s| projection_of(&s.session, &s.files));
            }
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

    async fn persist_text_outbox(&self, delivery_id: Uuid) -> AppResult<()> {
        let Some(record) = self.text_outbox.lock().unwrap().get(&delivery_id).cloned() else {
            return Ok(());
        };
        let json = serde_json::to_string(&record)
            .map_err(|error| AppError::Transfer(format!("序列化文本发送记录失败: {error}")))?;
        SendWrapper::new(idb::put_string(
            idb::TEXT_OUTBOX_STORE,
            &delivery_id.to_string(),
            &json,
        ))
        .await
        .map_err(AppError::from)
    }
}

/// Web 壳内部使用的存储面 = 端口 [`TransferStore`] **+ 浏览器特有的批量读**。
///
/// 端口的 `list_inbox_items` 只给 summary（桌面列表要的正是它，且端口不归本层改），
/// 于是浏览器侧要拿详情就只能「列表 + 逐条 detail」——那是 1 + N 次 wasm 调用、N 份
/// 重复克隆，还顺带制造一个竞态（拉详情与拉列表之间条目可能已被删，返回 `null`）。
/// 而 Web 的收件箱本就是全内存表，`list` 与 `detail` 读的是同一份数据，N 次调用买不到
/// 任何新鲜度。
///
/// 做成 supertrait 而不是让 `WebNode` 直接持具体类型：宿主拿到的仍然是**端口**，
/// 只是这个端口在 Web 侧多了一条批量读，而不是退回「宿主自己认得存储实现」。
pub trait WebStore: TransferStore {
    /// 收件箱条目列表，**带文件清单与关联传输投影**。可见性与顺序同
    /// [`InboxStore::list_inbox_items`]。
    fn list_inbox_details(&self, include_archived: bool) -> Vec<InboxItemDetail>;
}

impl WebStore for WebTransferStore {
    fn list_inbox_details(&self, include_archived: bool) -> Vec<InboxItemDetail> {
        let mut details = self.inbox.list(include_archived);
        self.attach_transfers(&mut details);
        details
    }
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
impl SessionStore for WebTransferStore {
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
            error_message: lifecycle.failure.as_ref().map(|f| f.to_column()),
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

    /// **绝对覆盖，零值照写**——判据见端口 trait 文档。浏览器端只有接收方向能续传，
    /// 所以这条路径今天走不到；保持与 native 实现同语义是为了别让端口的两个实现分叉。
    async fn save_sender_file_progress(
        &self,
        session_id: Uuid,
        progress: &[(u32, u32, u64)],
    ) -> AppResult<()> {
        for &(file_id, _chunks_done, transferred) in progress {
            self.mutate_file(session_id, file_id as i32, |f| {
                f.transferred_bytes = transferred as i64;
            });
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
                if let Some(failure) = &state.failure {
                    s.session.error_message = Some(failure.to_column());
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
        Ok(map
            .get(&session_id)
            .map(|s| projection_of(&s.session, &s.files)))
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

    async fn list_transfer_projections(&self) -> AppResult<Vec<TransferProjection>> {
        let mut projections: Vec<TransferProjection> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| projection_of(&s.session, &s.files))
            .collect();
        // `HashMap` 的迭代序不可依赖，而端口契约要求 `started_at` 倒序。
        // 撑着这条契约的是最后这句：**前端各面板照旧按自己的维度重排**（今天三端都按
        // `updatedAt`，见 DESIGN.md 的 Transfer List Order Contract），所以这里保证的
        // 是「确定性」而不是「给谁看的顺序」——`started_at` 不可变，比会被 checkpoint
        // 改写的 `updated_at` 更适合当这个锚。
        projections.sort_unstable_by_key(|p| std::cmp::Reverse(p.started_at));
        Ok(projections)
    }

    /// 会话与其文件行在 Web 侧是同一条 IndexedDB 记录，删掉它即级联。
    /// OPFS 里已落盘的文件**不动**——文件的生命周期归收件箱侧管（三端一致）。
    async fn delete_session(&self, session_id: Uuid) -> AppResult<()> {
        self.remove(session_id).await
    }

    async fn clear_all_history(&self) -> AppResult<()> {
        let terminal: Vec<Uuid> = {
            let map = self.sessions.lock().unwrap();
            map.values()
                .filter(|s| s.session.phase == entity::TransferPhase::Terminal)
                .map(|s| s.session.session_id)
                .collect()
        };
        // 非终态一条不动：它们还不是「历史」，且删掉记录会留下仍在写 checkpoint 的孤儿。
        self.remove_many(&terminal).await
    }

    async fn get_session_source_paths(&self, session_id: Uuid) -> AppResult<Vec<String>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(&session_id)
            .map(|s| {
                s.files
                    .iter()
                    .filter_map(|f| f.source_path.clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn reap_expired_suspended_receives(
        &self,
        retention_secs: u64,
    ) -> AppResult<Vec<ExpiredReceiverActor>> {
        let now = Self::now_ms();
        let cutoff = now - (retention_secs as i64) * 1000;
        let reaped: Vec<ExpiredReceiverActor> = {
            let mut map = self.sessions.lock().unwrap();
            map.values_mut()
                .filter(|s| {
                    s.session.phase == entity::TransferPhase::Suspended
                        && s.session.recoverable
                        && s.session.direction == entity::TransferDirection::Receive
                        && s.session.updated_at < cutoff
                })
                .map(|s| {
                    let save_dir = s.session.save_path.clone().map(CoreSaveLocation::from);
                    let files = s
                        .files
                        .iter()
                        .map(|f| HostFileMetadata {
                            name: f.name.clone(),
                            relative_path: f.relative_path.clone(),
                            size: f.size as u64,
                            modified_at: None,
                            checksum: Some(f.checksum.clone()),
                            save_dir: save_dir.clone(),
                        })
                        .collect();
                    // 就地转终态，字段与 SQL 实现逐条对齐（含 `status=Failed` 与那句过期说明）。
                    // 不经 `TransferCoordinator`：这是「启动时发现记录已过期」而非一次状态
                    // **转换**——没有对应的信号，也不该发 projection（前端此刻还没订阅）。
                    s.session.phase = entity::TransferPhase::Terminal;
                    s.session.suspended_reason = None;
                    s.session.terminal_reason = Some(entity::TerminalReason::FatalError);
                    s.session.status = entity::SessionStatus::Failed;
                    s.session.recoverable = false;
                    s.session.error_message =
                        Some(expired_receive_reason(retention_secs).to_column());
                    s.session.finished_at = Some(now);
                    s.session.updated_at = now;
                    ExpiredReceiverActor {
                        session_id: s.session.session_id,
                        files,
                    }
                })
                .collect()
        };
        // 回写终态：记录留在历史里、写明原因，不让它凭空消失。整批一个事务。
        self.persist_many(reaped.iter().map(|actor| actor.session_id))
            .await?;
        Ok(reaped)
    }

    /// Web 没有 MCP 代收，但端口方法不留平台空洞：照样写内存 + 写穿，
    /// 将来 Web 侧接入任何非人工发起的传输，provenance 直接就是对的。
    async fn update_session_origin(
        &self,
        session_id: Uuid,
        origin: TransferOrigin,
    ) -> AppResult<()> {
        {
            let mut map = self.sessions.lock().unwrap();
            if let Some(s) = map.get_mut(&session_id) {
                s.session.origin = Some(origin.to_db_string());
                s.session.updated_at = Self::now_ms();
            }
        }
        self.persist(session_id).await
    }
}

#[async_trait]
impl InboxStore for WebTransferStore {
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>> {
        let Some((session, files)) = self.session_rows(session_id) else {
            return Err(AppError::SessionNotFound("传输会话不存在".into()));
        };
        let detail = self.inbox.ensure_from_session(&session, &files).await?;
        Ok(detail.map(|detail| self.attach_transfer(detail)))
    }

    /// 补建：**只补缺的**，已有条目的会话不重复补。
    ///
    /// 实现在本类型而非 [`WebInboxTable`] 上，因为它要同时看见两边（收件箱表按设计
    /// 不认识会话表）。构造走同一条 `ensure_from_session`，条目怎么建只有一处定义。
    ///
    /// **不在启动路径上被调用**（design D7）：它修的是「`ensure_*` 当时写失败」这个
    /// 长期存在的洞，不是存储格式迁移。
    async fn repair_missing_inbox_items_for_completed_receives(
        &self,
    ) -> AppResult<Vec<InboxItemDetail>> {
        let candidates: Vec<Uuid> = {
            let map = self.sessions.lock().unwrap();
            // 判据调共享的 `is_completed_receive`，不在此处重写那三段合取：它与
            // `ensure_from_session` 必须始终同义，否则「补建」会补出一批 ensure 根本不认的会话。
            map.values()
                .filter(|s| is_completed_receive(&s.session))
                .map(|s| s.session.session_id)
                .collect()
        };

        let mut repaired = Vec::new();
        for session_id in candidates {
            if self.inbox.has_item_for_session(session_id) {
                continue;
            }
            let Some((session, files)) = self.session_rows(session_id) else {
                continue;
            };
            // 尽力补建：单个会话失败（如缺 local_path 的旧数据）只跳过，不掐断整批——
            // 否则一个坏会话会让其后所有可补会话永远建不出来。
            match self.inbox.ensure_from_session(&session, &files).await {
                Ok(Some(detail)) => repaired.push(self.attach_transfer(detail)),
                Ok(None) => {}
                Err(e) => tracing::warn!("补建收件箱条目失败，跳过: session={session_id}, {e}"),
            }
        }
        Ok(repaired)
    }

    /// 端口契约是 summary（桌面列表要的就是这个），Web 壳自己走
    /// [`WebStore::list_inbox_details`]——同一份内存数据，摘不摘文件行是调用方的事。
    async fn list_inbox_items(&self, include_archived: bool) -> AppResult<Vec<InboxItemSummary>> {
        Ok(self
            .inbox
            .list(include_archived)
            .into_iter()
            .map(|detail| detail.item)
            .collect())
    }

    async fn search_inbox(
        &self,
        query: &str,
        limit: usize,
        include_archived: bool,
    ) -> AppResult<Vec<InboxSearchHit>> {
        Ok(self.inbox.search(query, limit, include_archived))
    }

    async fn get_inbox_item_detail(&self, item_id: Uuid) -> AppResult<Option<InboxItemDetail>> {
        Ok(self
            .inbox
            .detail(item_id)
            .map(|detail| self.attach_transfer(detail)))
    }

    async fn get_inbox_item_by_transfer_session_id(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>> {
        Ok(self
            .inbox
            .detail_by_session(session_id)
            .map(|detail| self.attach_transfer(detail)))
    }

    async fn mark_inbox_item_opened(&self, item_id: Uuid) -> AppResult<()> {
        self.inbox.mark_opened(item_id).await
    }

    async fn archive_inbox_item(&self, item_id: Uuid, archived: bool) -> AppResult<()> {
        self.inbox.archive(item_id, archived).await
    }

    async fn delete_inbox_item_record(&self, item_id: Uuid) -> AppResult<()> {
        self.inbox.delete_record(item_id).await
    }

    async fn mark_inbox_item_file_missing(
        &self,
        item_id: Uuid,
        file_id: i32,
        missing: bool,
    ) -> AppResult<()> {
        self.inbox
            .mark_file_missing(item_id, file_id, missing)
            .await
    }
}

#[async_trait]
impl TextDeliveryStore for WebTransferStore {
    async fn create_outgoing_text_delivery(&self, record: TextDeliveryRecord) -> AppResult<()> {
        if record.direction != entity::TextDeliveryDirection::Send {
            return Err(AppError::Transfer("只能创建发送方向的文本记录".into()));
        }
        validate_text_body(&record.body)?;
        let delivery_id = record.delivery_id;
        {
            let mut outbox = self.text_outbox.lock().unwrap();
            if outbox.contains_key(&delivery_id) {
                return Err(AppError::Transfer("文本投递标识已存在".into()));
            }
            outbox.insert(delivery_id, record);
        }
        self.persist_text_outbox(delivery_id).await
    }

    async fn get_text_delivery(&self, delivery_id: Uuid) -> AppResult<Option<TextDeliveryRecord>> {
        if let Some(record) = self.text_outbox.lock().unwrap().get(&delivery_id).cloned() {
            return Ok(Some(record));
        }
        Ok(self.inbox.text_delivery(delivery_id))
    }

    async fn list_outgoing_text_deliveries(
        &self,
        peer_id: &str,
    ) -> AppResult<Vec<TextDeliveryRecord>> {
        let mut records: Vec<_> = self
            .text_outbox
            .lock()
            .unwrap()
            .values()
            .filter(|record| record.peer_id == peer_id)
            .cloned()
            .collect();
        records.sort_by_key(|record| std::cmp::Reverse(record.updated_at));
        Ok(records)
    }

    async fn update_outgoing_text_delivery(
        &self,
        delivery_id: Uuid,
        status: entity::TextDeliveryStatus,
        failure: Option<entity::TextDeliveryFailure>,
        attempt_count: Option<i32>,
        updated_at: i64,
    ) -> AppResult<()> {
        {
            let mut outbox = self.text_outbox.lock().unwrap();
            let record = outbox
                .get_mut(&delivery_id)
                .ok_or_else(|| AppError::Transfer("文本投递记录不存在".into()))?;
            record.status = status;
            record.failure = failure;
            if let Some(attempt_count) = attempt_count {
                record.attempt_count = attempt_count;
            }
            record.updated_at = updated_at;
        }
        self.persist_text_outbox(delivery_id).await
    }

    async fn persist_incoming_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<InboxItemDetail> {
        if record.direction != entity::TextDeliveryDirection::Receive {
            return Err(AppError::Transfer("只能持久化接收方向的文本记录".into()));
        }
        self.inbox.persist_text_delivery(record).await
    }

    async fn delete_outgoing_text_delivery(&self, delivery_id: Uuid) -> AppResult<()> {
        self.text_outbox.lock().unwrap().remove(&delivery_id);
        SendWrapper::new(idb::delete(
            idb::TEXT_OUTBOX_STORE,
            &delivery_id.to_string(),
        ))
        .await
        .map_err(AppError::from)
    }

    async fn recover_interrupted_text_deliveries(&self, now_ms: i64) -> AppResult<u64> {
        let changed: Vec<Uuid> = {
            let mut outbox = self.text_outbox.lock().unwrap();
            outbox
                .iter_mut()
                .filter_map(|(delivery_id, record)| {
                    record.recover_after_restart(now_ms).then_some(*delivery_id)
                })
                .collect()
        };
        for delivery_id in &changed {
            self.persist_text_outbox(*delivery_id).await?;
        }
        Ok(changed.len() as u64)
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
    /// 不落库（1 GiB 文件 ≈ 256 KiB），载入恒 `None`——见模块注释「不落库的一样东西」。
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

#[cfg(test)]
mod tests {
    use super::*;

    use swarmdrop_transfer::protocol::FileInfo;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    /// 终态发送会话——历史列表里的常客，且落库范围内（见模块注释的表格）。
    fn completed_state() -> TransferState {
        TransferState {
            phase: entity::TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(entity::TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            failure: None,
        }
    }

    async fn seed(store: &WebTransferStore, id: Uuid, lifecycle: TransferState) {
        let files = vec![FileInfo {
            file_id: 0,
            name: "a.bin".into(),
            relative_path: "a.bin".into(),
            size: 1024,
            checksum: "deadbeef".into(),
        }];
        store
            .create_session(CreateSessionInput {
                session_id: id,
                direction: entity::TransferDirection::Send,
                peer_id: "peer",
                peer_name: "name",
                files: &files,
                total_size: 1024,
                save_path: None,
                source_paths: None,
                lifecycle,
                policy: None,
                origin: None,
            })
            .await
            .expect("create_session");
    }

    /// 建会话把 `started_at` 写成 now，测试要的时序只能事后校准（内存 map 即事实来源）。
    fn backdate(store: &WebTransferStore, id: Uuid, started_at: i64) {
        store
            .sessions
            .lock()
            .unwrap()
            .get_mut(&id)
            .expect("session seeded")
            .session
            .started_at = started_at;
    }

    /// 同上，校准淘汰用的 `updated_at`（`prune` 按它保新）。
    fn set_updated_at(store: &WebTransferStore, id: Uuid, updated_at: i64) {
        store
            .sessions
            .lock()
            .unwrap()
            .get_mut(&id)
            .expect("session seeded")
            .session
            .updated_at = updated_at;
    }

    /// 一条接收会话（收件箱的唯一来源）。文件是否已 finalize 由调用方决定：
    /// 只有走过 `mark_files_completed` 的会话才有 `local_path`，也才建得出收件箱条目。
    async fn seed_receive(
        store: &WebTransferStore,
        id: Uuid,
        peer_name: &str,
        lifecycle: TransferState,
    ) -> Vec<FileInfo> {
        let files = vec![FileInfo {
            file_id: 0,
            name: "报告.pdf".into(),
            relative_path: "报告.pdf".into(),
            size: 5,
            checksum: "checksum-0".into(),
        }];
        store
            .create_session(CreateSessionInput {
                session_id: id,
                direction: entity::TransferDirection::Receive,
                peer_id: "peer-a",
                peer_name,
                files: &files,
                total_size: 5,
                save_path: Some(CoreSaveLocation::Path {
                    path: "/inbox-store-test".into(),
                }),
                source_paths: None,
                lifecycle,
                policy: None,
                origin: None,
            })
            .await
            .expect("create receive session");
        files
    }

    /// 模拟 receiver 的文件级完成：真实链路里 `finalize_sink` 的返回值经
    /// `mark_file_completed` 写入 `local_path`，收件箱条目的落盘位置全靠它。
    async fn mark_files_completed(store: &WebTransferStore, id: Uuid, files: &[FileInfo]) {
        for file in files {
            store
                .mark_file_completed(
                    id,
                    file.file_id as i32,
                    vec![],
                    file.size as i64,
                    format!("/inbox-store-test/{}", file.relative_path),
                    "/inbox-store-test".into(),
                )
                .await
                .expect("mark file completed");
        }
    }

    /// 建一条已完成接收会话并落成收件箱条目，返回条目 id。
    async fn seed_inbox_item(store: &WebTransferStore, id: Uuid, peer_name: &str) -> Uuid {
        let files = seed_receive(store, id, peer_name, completed_state()).await;
        mark_files_completed(store, id, &files).await;
        store
            .ensure_inbox_item_for_completed_receive_session(id)
            .await
            .expect("ensure inbox item")
            .expect("已完成接收应产出条目")
            .item
            .id
    }

    /// **本 change 的实质差异**：会话表有 `HISTORY_CAP` 淘汰，收件箱条目不在其列。
    ///
    /// 这条测试是人工验收 10.16 的机器版。它红了意味着收件箱退回成「已完成接收会话的投影」：
    /// 用户收满 100 次之后，早期收到的东西会随着过程账本被挤掉而**凭空消失**——建这张真表
    /// 的全部理由就是这一条。
    ///
    /// 关联会话被淘汰后 `transfer` 这一格转为 `None`（与 SQL 侧外键置空后的表现一致），
    /// 但条目本身与它的文件行必须原样还在。
    #[wasm_bindgen_test]
    async fn prune_evicts_sessions_but_keeps_inbox_items() {
        let store = WebTransferStore::default();
        let receive_id = Uuid::new_v4();
        let item_id = seed_inbox_item(&store, receive_id, "小明的 Mac").await;
        assert!(
            store
                .get_inbox_item_detail(item_id)
                .await
                .unwrap()
                .expect("条目")
                .transfer
                .is_some(),
            "会话还在时详情应带上传输投影"
        );
        // 压成全表最旧的一条终态会话，保证它必是淘汰的那个
        set_updated_at(&store, receive_id, 0);

        // 灌满 HISTORY_CAP 条更新的终态会话；淘汰的在线触发点只有 apply_transition。
        let mut filler = Vec::with_capacity(HISTORY_CAP);
        for i in 0..HISTORY_CAP {
            let id = Uuid::new_v4();
            seed(&store, id, completed_state()).await;
            set_updated_at(&store, id, 1_000 + i as i64);
            filler.push(id);
        }
        let trigger = *filler.last().expect("填充会话");
        let session = store.find_session(trigger).await.unwrap().expect("session");
        store
            .apply_transition(&session, &completed_state())
            .await
            .expect("apply transition");

        assert!(
            store.find_session(receive_id).await.unwrap().is_none(),
            "最旧的终态会话应被淘汰（否则这条测试没测到淘汰本身）"
        );
        let detail = store
            .get_inbox_item_detail(item_id)
            .await
            .unwrap()
            .expect("收件箱条目不该随会话一起被淘汰");
        assert_eq!(detail.item.title, "报告.pdf");
        assert_eq!(detail.files.len(), 1, "条目的文件行也必须还在");
        assert!(
            detail.transfer.is_none(),
            "关联会话已被淘汰，投影这一格留空"
        );
        assert_eq!(store.list_inbox_items(false).await.unwrap().len(), 1);
    }

    /// `repair_*` **只补缺的**：已有条目的会话不重复补，软删过的也不复活。
    ///
    /// 它红了意味着一次修复调用就会把收件箱翻倍（每条已完成接收各多出一份），
    /// 或者把用户明确删掉的记录又变回来——后者是 `has_item_for_session` 刻意不排除软删项的理由。
    #[wasm_bindgen_test]
    async fn repair_only_creates_missing_inbox_items() {
        let store = WebTransferStore::default();
        let with_item = Uuid::new_v4();
        let without_item = Uuid::new_v4();
        let unfinished = Uuid::new_v4();

        let existing_item = seed_inbox_item(&store, with_item, "A").await;
        // 已完成但从没 ensure 过（模拟「ensure 当时写失败」那个洞）
        let files = seed_receive(&store, without_item, "B", completed_state()).await;
        mark_files_completed(&store, without_item, &files).await;
        // 还在传的接收会话：任何时候都不该被补出条目
        seed_receive(&store, unfinished, "C", TransferState::active(0)).await;

        let repaired = store
            .repair_missing_inbox_items_for_completed_receives()
            .await
            .expect("repair");

        assert_eq!(repaired.len(), 1, "只补缺失的那一条");
        assert_eq!(repaired[0].item.transfer_session_id, Some(without_item));
        let all = store.list_inbox_items(false).await.unwrap();
        assert_eq!(all.len(), 2, "已有条目不该被重建成第二份");
        assert!(all.iter().any(|item| item.id == existing_item));

        assert!(
            store
                .repair_missing_inbox_items_for_completed_receives()
                .await
                .expect("repair again")
                .is_empty(),
            "补过之后再调用应是空操作"
        );

        store
            .delete_inbox_item_record(existing_item)
            .await
            .expect("软删");
        assert!(
            store
                .repair_missing_inbox_items_for_completed_receives()
                .await
                .expect("repair after delete")
                .is_empty(),
            "用户删掉的条目不能被 repair 补回来"
        );
        assert_eq!(store.list_inbox_items(false).await.unwrap().len(), 1);
    }

    /// 清空传输历史不动收件箱——与 SQL 侧 `clear_history_should_keep_inbox_records` 同形。
    ///
    /// 与上面的 prune 测试测的不是同一条路径：那条是**自动**淘汰（容量上限），这条是用户
    /// 亲手点的「清空历史」。它红了意味着用户想清掉的是过程账本，实际连收到过的东西
    /// 一起清没了。
    #[wasm_bindgen_test]
    async fn clear_all_history_keeps_inbox_items() {
        let store = WebTransferStore::default();
        let session_id = Uuid::new_v4();
        let item_id = seed_inbox_item(&store, session_id, "小明的 Mac").await;

        store.clear_all_history().await.expect("clear");

        assert!(
            store
                .get_transfer_projection(session_id)
                .await
                .unwrap()
                .is_none(),
            "活动账本这一侧应已清干净"
        );
        let list = store.list_inbox_items(false).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, item_id);
    }

    /// 端口契约（D3）：按 `started_at` **倒序**。
    ///
    /// 与 `crates/storage-sql/src/store.rs` 的 `lists_projections_by_started_at_desc`
    /// 逐条对应——同一组落库顺序、同一组期望顺序。两端跑同一组断言正是 D3 的意义：
    /// 本实现此前直接 `HashMap::values()` 出，顺序随机——端口就没有确定性可言了。
    ///
    /// 跑法：`wasm-pack test --headless --chrome -p swarmdrop-web`。
    #[wasm_bindgen_test]
    async fn lists_projections_by_started_at_desc() {
        let store = WebTransferStore::default();
        let base = WebTransferStore::now_ms();
        let oldest = Uuid::from_u128(1);
        let middle = Uuid::from_u128(2);
        let newest = Uuid::from_u128(3);

        for (id, started_at) in [
            (middle, base - 2_000),
            (oldest, base - 5_000),
            (newest, base - 1_000),
        ] {
            seed(&store, id, completed_state()).await;
            backdate(&store, id, started_at);
        }

        let ids: Vec<Uuid> = store
            .list_transfer_projections()
            .await
            .expect("list")
            .into_iter()
            .map(|p| p.session_id)
            .collect();
        assert_eq!(ids, vec![newest, middle, oldest], "应按 started_at 倒序");
    }

    /// D6：清空只删终态会话，非终态一条不动（与 SQL 侧
    /// `clear_all_history_keeps_unfinished_sessions` 同形）。
    ///
    /// Web 上非终态里唯一跨刷新存活的是接收会话，删掉它等于让用户丢掉 OPFS 里
    /// 已经收了一半的 `.part`——那正是浏览器端续传的全部本钱。
    #[wasm_bindgen_test]
    async fn clear_all_history_keeps_unfinished_sessions() {
        let store = WebTransferStore::default();
        let finished = Uuid::from_u128(4);
        let running = Uuid::from_u128(5);
        seed(&store, finished, completed_state()).await;
        seed(&store, running, TransferState::active(0)).await;

        store.clear_all_history().await.expect("clear");

        assert!(
            store.find_session(finished).await.unwrap().is_none(),
            "终态会话应被清空"
        );
        assert!(
            store.find_session(running).await.unwrap().is_some(),
            "进行中的会话必须保留"
        );
    }

    /// **生产启动路径的唯一覆盖**：`Self::load()` 用 `futures::join!` 并发读回两张表，
    /// 两个 future 各自调一次 `idb::open()`——而本模块其余测试全部从 `default()` 起步
    /// （纯内存 + 写穿），一条都不读库，于是那条并发路径此前零覆盖。
    ///
    /// 它红了意味着刷新页面后收件箱与传输历史一起空掉：数据在 IndexedDB 里躺着，界面上
    /// 什么都没有，而且**没有任何报错**——`load()` 的两处读失败都刻意降级成 `warn` 继续
    /// （隐私模式 / 配额拒绝时节点仍要能用），所以静默是设计的一部分，测试是唯一的哨兵。
    ///
    /// 断言按 id 取而不是比长度：同一个 IndexedDB 库被全部 wasm 测试共用，`load()` 读回
    /// 的是所有测试的记录。末尾清掉自己的两条 key，别给别人留垃圾。
    #[wasm_bindgen_test]
    async fn load_restores_sessions_and_inbox_concurrently() {
        let seeded = WebTransferStore::default();
        let session_id = Uuid::new_v4();
        let item_id = seed_inbox_item(&seeded, session_id, "Reload 验证机").await;

        let reloaded = WebTransferStore::load().await;

        let detail = reloaded
            .list_inbox_details(true)
            .into_iter()
            .find(|d| d.item.id == item_id)
            .expect("收件箱条目必须跨 load() 存活");
        assert_eq!(detail.item.title, "报告.pdf");
        assert_eq!(detail.files.len(), 1, "条目的文件行也要一起回来");
        assert_eq!(detail.item.transfer_session_id, Some(session_id));
        // 这一格由 `attach_transfers` 从**会话表**补——它非空即证明两张表都读回来了，
        // 而不是只有收件箱那一张。
        assert!(
            detail.transfer.is_some(),
            "关联会话也必须跨 load() 回来（否则 join! 里另一半没读到）"
        );
        assert!(
            reloaded.find_session(session_id).await.unwrap().is_some(),
            "会话表自身同样要能查到这条"
        );

        SendWrapper::new(idb::delete(idb::INBOX_STORE, &item_id.to_string()))
            .await
            .expect("清理收件箱测试记录");
        SendWrapper::new(idb::delete(idb::SESSION_STORE, &session_id.to_string()))
            .await
            .expect("清理会话测试记录");
    }
}
