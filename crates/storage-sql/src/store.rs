//! [`SessionStore`] / [`InboxStore`] 的 SeaORM 实现（桌面 / 移动端）。
//!
//! 端口定义在 [`swarmdrop_transfer::store`]；本类型持 `DatabaseConnection`。
//! **运行时写路径**的方法体委托 [`ops`] 的实现函数（它们还有别的调用方）；
//! **历史管理**六类（列表投影 / 删除 / 清空 / 源路径 / 过期回收 / origin 标记）的实现
//! 本体就在本文件——它们只有端口这一个入口，再委托一层自由函数只是多一处可绕过的旁路。
//! **收件箱**十类委托 [`inbox`]：那边的函数全是 `pub(crate)`，本 impl 是它们唯一的入口。
//! Web 端是 `crates/web` 的 IndexedDB 写穿实现。
//! `TransferStore` 由 blanket impl 覆盖（同时实现两个子端口即得），无需在此显式实现。

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, Set,
};
use uuid::Uuid;

use super::{inbox, ops, text_delivery};
use entity::{SessionStatus, TerminalReason, TransferDirection, TransferPhase};
use swarmdrop_host::AppResult;
use swarmdrop_host::{CoreSaveLocation, HostFileMetadata};
use swarmdrop_transfer::coordinator::TransferState;
use swarmdrop_transfer::expired_receive_reason;
use swarmdrop_transfer::inbox::{InboxItemDetail, InboxItemSummary, InboxSearchHit};
use swarmdrop_transfer::protocol::TransferOrigin;
use swarmdrop_transfer::store::{
    CreateSessionInput, ExpiredReceiverActor, InboxStore, SessionStore, TextDeliveryStore,
    TransferProjection,
};
use swarmdrop_transfer::text_delivery::TextDeliveryRecord;

/// SeaORM 持久化实现，注入 `TransferManager` 作为 `Arc<dyn TransferStore>`。
#[derive(Clone)]
pub struct SqlSessionStore {
    db: Arc<DatabaseConnection>,
}

impl SqlSessionStore {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// 泛型 sea-orm 查询要的是 `&DatabaseConnection`（`&Arc<_>` 不会自动解引用到泛型参数）。
    fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[async_trait]
impl SessionStore for SqlSessionStore {
    async fn create_session(&self, input: CreateSessionInput<'_>) -> AppResult<()> {
        ops::create_session(&self.db, input).await
    }

    async fn update_session_save_path(
        &self,
        session_id: Uuid,
        save_path: CoreSaveLocation,
    ) -> AppResult<()> {
        ops::update_session_save_path(&self.db, session_id, save_path).await
    }

    async fn update_file_checkpoint_ranges(
        &self,
        session_id: Uuid,
        file_id: i32,
        completed_chunks: Vec<u8>,
        completed_ranges: &[(u64, u64)],
        transferred_bytes: i64,
    ) -> AppResult<()> {
        ops::update_file_checkpoint_ranges(
            &self.db,
            session_id,
            file_id,
            completed_chunks,
            completed_ranges,
            transferred_bytes,
        )
        .await
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
        ops::mark_file_completed(
            &self.db,
            session_id,
            file_id,
            completed_chunks,
            transferred_bytes,
            local_path,
            local_dir,
        )
        .await
    }

    async fn save_sender_file_progress(
        &self,
        session_id: Uuid,
        progress: &[(u32, u32, u64)],
    ) -> AppResult<()> {
        ops::save_sender_file_progress(&self.db, session_id, progress).await
    }

    async fn apply_transition(
        &self,
        session: &entity::transfer_session::Model,
        state: &TransferState,
    ) -> AppResult<()> {
        ops::apply_transition(&self.db, session, state).await
    }

    async fn find_active_session_ids(&self) -> AppResult<Vec<Uuid>> {
        ops::find_active_session_ids(&self.db).await
    }

    async fn find_active_session_ids_by_peer(&self, peer_id: &str) -> AppResult<Vec<Uuid>> {
        ops::find_active_session_ids_by_peer(&self.db, peer_id).await
    }

    async fn find_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<entity::transfer_session::Model>> {
        ops::find_session(&self.db, session_id).await
    }

    async fn get_transfer_projection(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<TransferProjection>> {
        ops::get_transfer_projection(&self.db, session_id).await
    }

    async fn get_session_files(
        &self,
        session_id: Uuid,
    ) -> AppResult<Vec<entity::transfer_file::Model>> {
        ops::get_session_files(&self.db, session_id).await
    }

    async fn save_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
        outboard: Vec<u8>,
    ) -> AppResult<()> {
        ops::save_file_outboard(&self.db, session_id, file_id, outboard).await
    }

    async fn load_file_outboard(
        &self,
        session_id: Uuid,
        file_id: i32,
    ) -> AppResult<Option<Vec<u8>>> {
        ops::load_file_outboard(&self.db, session_id, file_id).await
    }

    async fn list_transfer_projections(&self) -> AppResult<Vec<TransferProjection>> {
        // `order_by_desc(StartedAt)` 是端口契约（确定性顺序），不是可有可无的便利。
        let sessions = entity::TransferSession::load()
            .with(entity::TransferFile)
            .order_by_desc(entity::transfer_session::Column::StartedAt)
            .all(self.db())
            .await?;
        // `ModelEx` → `TransferProjection` 的转换收在方法体内：端口签名上只见纯 DTO。
        Ok(sessions.into_iter().map(Into::into).collect())
    }

    async fn delete_session(&self, session_id: Uuid) -> AppResult<()> {
        if let Some(session) = entity::TransferSession::find_by_id(session_id)
            .one(self.db())
            .await?
        {
            // cascade_delete 一并删文件行；收件箱条目由外键 ON DELETE SET NULL 留下。
            session.cascade_delete(self.db()).await?;
        }
        Ok(())
    }

    async fn clear_all_history(&self) -> AppResult<()> {
        // 终态会话 id 的子查询：文件行按它删，避免先把 id 全物化到内存再回填 IN 列表。
        let terminal_sessions = entity::TransferSession::find()
            .select_only()
            .column(entity::transfer_session::Column::SessionId)
            .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Terminal))
            .into_query();
        entity::TransferFile::delete_many()
            .filter(entity::transfer_file::Column::SessionId.in_subquery(terminal_sessions))
            .exec(self.db())
            .await?;
        entity::TransferSession::delete_many()
            .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Terminal))
            .exec(self.db())
            .await?;
        Ok(())
    }

    async fn get_session_source_paths(&self, session_id: Uuid) -> AppResult<Vec<String>> {
        // 只查 source_path 一列并把 NULL 过滤下推到 SQL，避免物化 bitmap BLOB 等无关列。
        Ok(entity::TransferFile::find()
            .select_only()
            .column(entity::transfer_file::Column::SourcePath)
            .filter(entity::transfer_file::Column::SessionId.eq(session_id))
            .filter(entity::transfer_file::Column::SourcePath.is_not_null())
            .into_tuple::<String>()
            .all(self.db())
            .await?)
    }

    async fn reap_expired_suspended_receives(
        &self,
        retention_secs: u64,
    ) -> AppResult<Vec<ExpiredReceiverActor>> {
        let threshold = ops::now_ms() - (retention_secs as i64) * 1000;
        let sessions = entity::TransferSession::find()
            .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Suspended))
            .filter(entity::transfer_session::Column::Recoverable.eq(true))
            .filter(entity::transfer_session::Column::Direction.eq(TransferDirection::Receive))
            .filter(entity::transfer_session::Column::UpdatedAt.lt(threshold))
            .all(self.db())
            .await?;

        let mut reaped = Vec::with_capacity(sessions.len());
        for session in sessions {
            let session_id = session.session_id;
            let save_dir = session.save_path.clone().map(CoreSaveLocation::from);
            let files = ops::get_session_files(&self.db, session_id)
                .await?
                .into_iter()
                .map(|f| HostFileMetadata {
                    name: f.name,
                    relative_path: f.relative_path,
                    size: f.size as u64,
                    modified_at: None,
                    checksum: Some(f.checksum),
                    save_dir: save_dir.clone(),
                })
                .collect();

            let now = ops::now_ms();
            let mut model = session.into_active_model();
            model.status = Set(SessionStatus::Failed);
            model.phase = Set(TransferPhase::Terminal);
            model.suspended_reason = Set(None);
            model.terminal_reason = Set(Some(TerminalReason::FatalError));
            model.recoverable = Set(false);
            model.error_message = Set(Some(expired_receive_reason(retention_secs).to_column()));
            model.finished_at = Set(Some(now));
            model.updated_at = Set(now);
            model.update(self.db()).await?;

            reaped.push(ExpiredReceiverActor { session_id, files });
        }
        Ok(reaped)
    }

    async fn update_session_origin(
        &self,
        session_id: Uuid,
        origin: TransferOrigin,
    ) -> AppResult<()> {
        let Some(session) = entity::TransferSession::find_by_id(session_id)
            .one(self.db())
            .await?
        else {
            return Err(swarmdrop_host::AppError::SessionNotFound(
                "会话不存在".into(),
            ));
        };

        let mut model = session.into_active_model();
        model.origin = Set(Some(origin.to_db_string()));
        model.updated_at = Set(ops::now_ms());
        model.update(self.db()).await?;
        Ok(())
    }
}

#[async_trait]
impl InboxStore for SqlSessionStore {
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>> {
        inbox::ensure_inbox_item_for_completed_receive_session(&self.db, session_id).await
    }

    async fn repair_missing_inbox_items_for_completed_receives(
        &self,
    ) -> AppResult<Vec<InboxItemDetail>> {
        inbox::repair_missing_inbox_items_for_completed_receives(&self.db).await
    }

    async fn list_inbox_items(&self, include_archived: bool) -> AppResult<Vec<InboxItemSummary>> {
        inbox::list_inbox_items(&self.db, include_archived).await
    }

    async fn search_inbox(
        &self,
        query: &str,
        limit: usize,
        include_archived: bool,
    ) -> AppResult<Vec<InboxSearchHit>> {
        inbox::search_inbox(&self.db, query, limit, include_archived).await
    }

    async fn get_inbox_item_detail(&self, item_id: Uuid) -> AppResult<Option<InboxItemDetail>> {
        inbox::get_inbox_item_detail(&self.db, item_id).await
    }

    async fn get_inbox_item_by_transfer_session_id(
        &self,
        session_id: Uuid,
    ) -> AppResult<Option<InboxItemDetail>> {
        inbox::get_inbox_item_by_transfer_session_id(&self.db, session_id).await
    }

    async fn mark_inbox_item_opened(&self, item_id: Uuid) -> AppResult<()> {
        inbox::mark_inbox_item_opened(&self.db, item_id).await
    }

    async fn archive_inbox_item(&self, item_id: Uuid, archived: bool) -> AppResult<()> {
        inbox::archive_inbox_item(&self.db, item_id, archived).await
    }

    async fn delete_inbox_item_record(&self, item_id: Uuid) -> AppResult<()> {
        inbox::delete_inbox_item_record(&self.db, item_id).await
    }

    async fn mark_inbox_item_file_missing(
        &self,
        item_id: Uuid,
        file_id: i32,
        missing: bool,
    ) -> AppResult<()> {
        inbox::mark_inbox_item_file_missing(&self.db, item_id, file_id, missing).await
    }
}

#[async_trait]
impl TextDeliveryStore for SqlSessionStore {
    async fn create_outgoing_text_delivery(&self, record: TextDeliveryRecord) -> AppResult<()> {
        text_delivery::create_outgoing(&self.db, record).await
    }

    async fn get_text_delivery(&self, delivery_id: Uuid) -> AppResult<Option<TextDeliveryRecord>> {
        text_delivery::get(&self.db, delivery_id).await
    }

    async fn list_outgoing_text_deliveries(
        &self,
        peer_id: &str,
    ) -> AppResult<Vec<TextDeliveryRecord>> {
        text_delivery::list_outgoing(&self.db, peer_id).await
    }

    async fn update_outgoing_text_delivery(
        &self,
        delivery_id: Uuid,
        status: entity::TextDeliveryStatus,
        failure: Option<entity::TextDeliveryFailure>,
        attempt_count: Option<i32>,
        updated_at: i64,
    ) -> AppResult<()> {
        text_delivery::update_outgoing(
            &self.db,
            delivery_id,
            status,
            failure,
            attempt_count,
            updated_at,
        )
        .await
    }

    async fn create_pending_incoming_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<()> {
        text_delivery::create_pending_incoming(&self.db, record).await
    }

    async fn list_pending_incoming_text_deliveries(&self) -> AppResult<Vec<TextDeliveryRecord>> {
        text_delivery::list_pending_incoming(&self.db).await
    }

    async fn finalize_pending_incoming_text_delivery(
        &self,
        delivery_id: Uuid,
        status: entity::TextDeliveryStatus,
        updated_at: i64,
    ) -> AppResult<()> {
        text_delivery::finalize_pending_incoming(&self.db, delivery_id, status, updated_at).await
    }

    async fn persist_incoming_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<InboxItemDetail> {
        text_delivery::persist_incoming(&self.db, record).await
    }

    async fn delete_outgoing_text_delivery(&self, delivery_id: Uuid) -> AppResult<()> {
        text_delivery::delete_outgoing(&self.db, delivery_id).await
    }

    async fn recover_interrupted_text_deliveries(&self, now_ms: i64) -> AppResult<u64> {
        text_delivery::recover_interrupted(&self.db, now_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use entity::SuspendedReason;
    use migration::MigratorTrait;
    use sea_orm::{ConnectOptions, Database};
    use swarmdrop_transfer::protocol::FileInfo;

    async fn test_store() -> SqlSessionStore {
        // `:memory:` 每条物理连接是独立空库，钉死单连接保证 migration 与查询同库。
        let mut opt = ConnectOptions::new("sqlite::memory:");
        opt.max_connections(1).min_connections(1);
        let db = Database::connect(opt)
            .await
            .expect("connect sqlite::memory:");
        migration::Migrator::up(&db, None).await.expect("migrate");
        SqlSessionStore::new(Arc::new(db))
    }

    /// 建会话时一次写到目标 phase：状态直写的旁路已删，fixture 走 `lifecycle` 入参。
    fn suspended_state() -> TransferState {
        TransferState {
            phase: TransferPhase::Suspended,
            suspended_reason: Some(SuspendedReason::LocalPaused),
            terminal_reason: None,
            epoch: 0,
            recoverable: true,
            failure: None,
        }
    }

    fn completed_state() -> TransferState {
        TransferState {
            phase: TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            failure: None,
        }
    }

    async fn seed(
        store: &SqlSessionStore,
        id: Uuid,
        direction: TransferDirection,
        lifecycle: TransferState,
    ) {
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
                direction,
                peer_id: "peer",
                peer_name: "name",
                files: &files,
                total_size: 1024,
                save_path: Some(CoreSaveLocation::Path {
                    path: "/recv".into(),
                }),
                source_paths: None,
                lifecycle,
                policy: None,
                origin: None,
            })
            .await
            .expect("create_session");
    }

    /// 建会话把 `started_at` / `updated_at` 一律写成 now，测试要的时序只能事后校准。
    async fn backdate(
        store: &SqlSessionStore,
        id: Uuid,
        apply: impl FnOnce(&mut entity::transfer_session::ActiveModel),
    ) {
        let mut m = store
            .find_session(id)
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
        apply(&mut m);
        m.update(store.db()).await.expect("backdate");
    }

    /// 端口契约（D3）：[`SessionStore::list_transfer_projections`] 按 `started_at` **倒序**。
    ///
    /// 落库顺序刻意与时间序错开——顺序若来自插入序（或 Web 侧的 `HashMap` 迭代序），
    /// 这条就红。`crates/web/src/store.rs` 有一份逐条对应的同形断言：两端跑同一组期望，
    /// 「换一个存储实现、行为不变」才成立。
    #[tokio::test]
    async fn lists_projections_by_started_at_desc() {
        let store = test_store().await;
        let base = ops::now_ms();
        let oldest = Uuid::from_u128(1);
        let middle = Uuid::from_u128(2);
        let newest = Uuid::from_u128(3);

        for (id, started_at) in [
            (middle, base - 2_000),
            (oldest, base - 5_000),
            (newest, base - 1_000),
        ] {
            seed(&store, id, TransferDirection::Send, completed_state()).await;
            backdate(&store, id, |m| m.started_at = Set(started_at)).await;
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

    /// D6：清空只删终态会话，非终态连同其文件行一条不动。
    ///
    /// 无条件 `delete_many` 会把正在传的那条也删掉，留下一个仍在写 checkpoint 的孤儿 actor
    /// ——与「进行中不可删」的域不变量自相矛盾。
    #[tokio::test]
    async fn clear_all_history_keeps_unfinished_sessions() {
        let store = test_store().await;
        let finished = Uuid::from_u128(1);
        let running = Uuid::from_u128(2);
        let paused = Uuid::from_u128(3);
        seed(
            &store,
            finished,
            TransferDirection::Receive,
            completed_state(),
        )
        .await;
        seed(
            &store,
            running,
            TransferDirection::Receive,
            TransferState::active(0),
        )
        .await;
        seed(
            &store,
            paused,
            TransferDirection::Receive,
            suspended_state(),
        )
        .await;

        store.clear_all_history().await.expect("clear");

        assert!(
            store.find_session(finished).await.unwrap().is_none(),
            "终态会话应被清空"
        );
        assert!(
            store.find_session(running).await.unwrap().is_some(),
            "进行中的会话必须保留"
        );
        assert!(
            store.find_session(paused).await.unwrap().is_some(),
            "可续传的挂起会话必须保留"
        );
        // 文件行按「属于终态会话」的子查询删，幸存会话的文件不能被连坐。
        assert_eq!(store.get_session_files(running).await.unwrap().len(), 1);
        assert!(store.get_session_files(finished).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reaps_only_expired_recoverable_receives() {
        let store = test_store().await;
        let now = ops::now_ms();
        let day = 24 * 60 * 60 * 1000;
        let expired_recv = Uuid::from_u128(1);
        let fresh_recv = Uuid::from_u128(2);
        let expired_send = Uuid::from_u128(3);
        let terminal_recv = Uuid::from_u128(4);

        for (id, direction, updated_at, lifecycle) in [
            (
                expired_recv,
                TransferDirection::Receive,
                now - 8 * day,
                suspended_state(),
            ),
            (
                fresh_recv,
                TransferDirection::Receive,
                now - 3 * day,
                suspended_state(),
            ),
            (
                expired_send,
                TransferDirection::Send,
                now - 8 * day,
                suspended_state(),
            ),
            (
                terminal_recv,
                TransferDirection::Receive,
                now - 30 * day,
                completed_state(),
            ),
        ] {
            seed(&store, id, direction, lifecycle).await;
            backdate(&store, id, |m| m.updated_at = Set(updated_at)).await;
        }

        let retention = 7 * 24 * 60 * 60; // 7 天（秒）
        let reaped = store
            .reap_expired_suspended_receives(retention)
            .await
            .expect("reap");

        // 只回收过期的 recoverable suspended receive。
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].session_id, expired_recv);
        assert_eq!(reaped[0].files.len(), 1);
        assert_eq!(reaped[0].files[0].relative_path, "a.bin");
        assert!(reaped[0].files[0].save_dir.is_some());

        // 过期会话转 terminal、不可恢复、带过期 reason。
        let m = store.find_session(expired_recv).await.unwrap().unwrap();
        assert_eq!(m.phase, TransferPhase::Terminal);
        assert!(!m.recoverable);
        assert_eq!(m.terminal_reason, Some(TerminalReason::FatalError));

        // 保留期内 / 发送会话 / 已 terminal 均不受影响。
        let fresh = store.find_session(fresh_recv).await.unwrap().unwrap();
        assert_eq!(fresh.phase, TransferPhase::Suspended);
        assert!(fresh.recoverable);
        let send = store.find_session(expired_send).await.unwrap().unwrap();
        assert_eq!(send.phase, TransferPhase::Suspended);
        let term = store.find_session(terminal_recv).await.unwrap().unwrap();
        assert_eq!(term.terminal_reason, Some(TerminalReason::Completed));
    }
}
