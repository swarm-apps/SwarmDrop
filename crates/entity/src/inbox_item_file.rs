use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_item_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub inbox_item_id: Uuid,
    #[sea_orm(belongs_to, from = "inbox_item_id", to = "id")]
    pub inbox_item: HasOne<super::inbox_item::Entity>,
    /// 关联的过程账本文件记录。活动账本被清理后这里会置空。
    pub transfer_file_id: Option<i32>,
    #[sea_orm(belongs_to, from = "transfer_file_id", to = "id")]
    pub transfer_file: HasOne<super::transfer_file::Entity>,
    pub relative_path: String,
    pub name: String,
    pub size: i64,
    pub checksum: String,
    pub local_path: String,
    /// 本地文件已被外部移动或删除时置为 true，记录本身仍保留。
    pub missing: bool,
}

impl ActiveModelBehavior for ActiveModel {}
