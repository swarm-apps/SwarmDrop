//! [`SessionStore`] / [`InboxStore`] 的 SeaORM 实现（桌面 / 移动端）。
//!
//! 端口定义在 [`swarmdrop_transfer::store`]；本类型持 `DatabaseConnection`，方法体委托
//! [`ops`] / [`inbox`] 的既有实现函数（函数本体不动）。Web 端未来另实现 IndexedDB/OPFS 版。
//! `TransferStore` 由 blanket impl 覆盖（同时实现两个子端口即得），无需在此显式实现。

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use super::{inbox, ops};
use crate::AppResult;
use crate::host::CoreSaveLocation;
use crate::transfer::coordinator::TransferState;
use crate::transfer::store::{CreateSessionInput, InboxStore, SessionStore, TransferProjection};

/// SeaORM 持久化实现，注入 `TransferManager` 作为 `Arc<dyn TransferStore>`。
#[derive(Clone)]
pub struct SqlSessionStore {
    db: Arc<DatabaseConnection>,
}

impl SqlSessionStore {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
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

    async fn reset_file_checkpoint(&self, session_id: Uuid, file_id: i32) -> AppResult<()> {
        ops::reset_file_checkpoint(&self.db, session_id, file_id).await
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
}

#[async_trait]
impl InboxStore for SqlSessionStore {
    async fn ensure_inbox_item_for_completed_receive_session(
        &self,
        session_id: Uuid,
    ) -> AppResult<()> {
        // 返回的 InboxItemDetail 是 core inbox 类型，端口不透出（接收方只关心成功/失败）。
        inbox::ensure_inbox_item_for_completed_receive_session(&self.db, session_id)
            .await
            .map(|_| ())
    }
}
