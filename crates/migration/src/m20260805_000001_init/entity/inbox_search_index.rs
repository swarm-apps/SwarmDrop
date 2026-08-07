use sea_orm::entity::prelude::*;

/// 收件箱检索索引 —— 取代旧的 FTS5 虚表 `inbox_fts`（理由见主 entity 的文档注释）。
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_search_index")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub item_id: Uuid,
    #[sea_orm(belongs_to, from = "item_id", to = "id", on_delete = "Cascade")]
    pub item: HasOne<super::inbox_item::Entity>,
    pub title: String,
    pub source_name: String,
    pub files_text: String,
    pub extracted_text: String,
}

impl ActiveModelBehavior for ActiveModel {}
