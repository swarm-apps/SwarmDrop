use sea_orm::entity::prelude::*;

use super::types::{PeerId, SessionStatus, TransferDirection};

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
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
    pub save_path: Option<String>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::transfer_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
