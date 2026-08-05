use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_item_files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 按条目取文件行是最热的读法，故建索引。
    #[sea_orm(indexed)]
    pub inbox_item_id: Uuid,
    /// 文件行依附于条目，条目没了就该跟着走。
    #[sea_orm(belongs_to, from = "inbox_item_id", to = "id", on_delete = "Cascade")]
    pub inbox_item: HasOne<super::inbox_item::Entity>,
    /// 关联的过程账本文件记录。活动账本被清理后这里会置空。
    #[sea_orm(indexed)]
    pub transfer_file_id: Option<i32>,
    /// 与 [`super::inbox_item::Model::transfer_session`] 同一条理由：删过程不动结果。
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
    /// 本地文件已被外部移动或删除时置为 true，记录本身仍保留。
    pub missing: bool,
}

impl ActiveModelBehavior for ActiveModel {}
