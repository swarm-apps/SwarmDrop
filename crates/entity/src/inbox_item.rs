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
    ///
    /// `unique` 保证一个会话至多派生一条收件箱条目（`ensure_*` 的幂等性靠它兜底）。
    #[sea_orm(unique)]
    pub transfer_session_id: Option<Uuid>,
    /// **`ON DELETE SET NULL` 是「清空传输历史不动收件箱」这条三端不变量的实现基础**
    /// —— 收件箱是结果账本、传输历史是过程账本，删过程不动结果
    /// （`clear_all_history_keeps_inbox_records` 钉着这条）。它此前只存在于
    /// `m20260627_000002_drop_inbox` 的裸 SQL 里、entity 从未表达过；建表改走 schema
    /// builder 之后不写在这里就是真的丢约束。
    #[sea_orm(
        belongs_to,
        from = "transfer_session_id",
        to = "session_id",
        on_delete = "SetNull"
    )]
    pub transfer_session: HasOne<super::transfer_session::Entity>,
    /// 来源 peer 快照。
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
    /// 列表按它倒序，故建索引。
    #[sea_orm(indexed)]
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    #[sea_orm(has_many)]
    pub files: HasMany<super::inbox_item_file::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
