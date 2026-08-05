use sea_orm::entity::prelude::*;

use super::types::{InboxContentKind, InboxSourceKind, PeerId};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub transfer_session_id: Option<Uuid>,
    /// `SetNull` 是「清空传输历史不动收件箱」的实现基础，不是可选装饰。
    #[sea_orm(
        belongs_to,
        from = "transfer_session_id",
        to = "session_id",
        on_delete = "SetNull"
    )]
    pub transfer_session: HasOne<super::transfer_session::Entity>,
    #[sea_orm(column_type = "Text")]
    pub source_peer_id: PeerId,
    pub source_name: String,
    pub source_kind: InboxSourceKind,
    pub content_kind: InboxContentKind,
    pub title: String,
    pub item_count: i32,
    pub total_size: i64,
    pub root_path: Option<String>,
    pub content_hash: Option<String>,
    #[sea_orm(indexed)]
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::inbox_item_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
