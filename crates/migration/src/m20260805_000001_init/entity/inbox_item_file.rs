use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_item_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(indexed)]
    pub inbox_item_id: Uuid,
    #[sea_orm(belongs_to, from = "inbox_item_id", to = "id", on_delete = "Cascade")]
    pub inbox_item: HasOne<super::inbox_item::Entity>,
    #[sea_orm(indexed)]
    pub transfer_file_id: Option<i32>,
    #[sea_orm(
        belongs_to,
        from = "transfer_file_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub transfer_file: HasOne<super::transfer_file::Entity>,
    pub relative_path: String,
    pub name: String,
    pub size: i64,
    pub checksum: String,
    pub local_path: String,
    pub missing: bool,
}

impl ActiveModelBehavior for ActiveModel {}
