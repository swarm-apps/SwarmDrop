use sea_orm::entity::prelude::*;

use super::types::FileStatus;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "transfer_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 与 `file_id` 组成复合唯一键（旧库里是手写的
    /// `idx_transfer_files_session_file`，现在由 `unique_key` 表达）。
    #[sea_orm(unique_key = "session_file")]
    pub session_id: Uuid,
    // 与主 entity 一致地**不加** `on_delete`：文件行的删除由应用层负责。
    #[sea_orm(belongs_to, from = "session_id", to = "session_id")]
    pub session: HasOne<super::transfer_session::Entity>,
    #[sea_orm(unique_key = "session_file")]
    pub file_id: i32,
    pub name: String,
    pub relative_path: String,
    pub size: i64,
    pub checksum: String,
    pub status: FileStatus,
    pub transferred_bytes: i64,
    pub total_chunks: i32,
    pub completed_chunks: Vec<u8>,
    pub completed_ranges: String,
    pub source_path: Option<String>,
    pub local_path: Option<String>,
    pub local_dir: Option<String>,
    pub outboard: Option<Vec<u8>>,
}

impl ActiveModelBehavior for ActiveModel {}
