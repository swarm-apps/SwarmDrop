use sea_orm::entity::prelude::*;

use crate::{InboxContentKind, InboxSourceKind, PeerId};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_items")]
pub struct Model {
    /// 收件箱条目 ID。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 关联的传输会话。活动账本被清理后这里会置空，收件箱内容仍保留。
    pub transfer_session_id: Option<Uuid>,
    #[sea_orm(belongs_to, from = "transfer_session_id", to = "session_id")]
    pub transfer_session: HasOne<super::transfer_session::Entity>,
    /// 来源 peer 快照。
    #[sea_orm(column_type = "Text")]
    pub source_peer_id: PeerId,
    pub source_name: String,
    pub source_kind: InboxSourceKind,
    pub content_kind: InboxContentKind,
    /// 条目内第一个文件的文件名；零文件条目为 `None`。
    ///
    /// **不是展示标题。** 三端各自按当前 locale 从它 + `item_count` 生成标题；
    /// 此前这里是 `title: String`，存的是拼好的中文散文
    /// （见 `m20260804_000001_inbox_structural_title`）。
    pub primary_file_name: Option<String>,
    pub item_count: i32,
    pub total_size: i64,
    pub root_path: Option<String>,
    pub content_hash: Option<String>,
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::inbox_item_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
