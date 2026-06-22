use sea_orm::entity::prelude::*;

use super::types::FileStatus;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub session_id: Uuid,
    #[sea_orm(belongs_to, from = "session_id", to = "session_id")]
    pub session: HasOne<super::transfer_session::Entity>,
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    pub checksum: String,
    pub status: FileStatus,
    pub transferred_bytes: i64,
    pub total_chunks: i32,
    pub completed_chunks: Vec<u8>,
    pub source_path: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
