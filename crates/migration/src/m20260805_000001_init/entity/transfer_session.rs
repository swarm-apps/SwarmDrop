use sea_orm::entity::prelude::*;

use super::types::{
    PeerId, SaveLocation, SessionStatus, SuspendedReason, TerminalReason, TransferDirection,
    TransferPhase,
};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub session_id: Uuid,
    pub direction: TransferDirection,
    #[sea_orm(column_type = "Text")]
    pub peer_id: PeerId,
    pub peer_name: String,
    pub total_size: i64,
    pub transferred_bytes: i64,
    pub status: SessionStatus,
    pub phase: TransferPhase,
    pub suspended_reason: Option<SuspendedReason>,
    pub terminal_reason: Option<TerminalReason>,
    pub epoch: i64,
    pub recoverable: bool,
    pub source_fingerprint: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub policy_action: Option<String>,
    pub policy_reason: Option<String>,
    pub origin: Option<String>,
    pub save_path: Option<SaveLocation>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::transfer_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
