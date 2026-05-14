mod entity;

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. 通过冻结的 Entity 快照自动建表（含外键）
        db.get_schema_builder()
            .register(entity::transfer_session::Entity)
            .register(entity::transfer_file::Entity)
            .apply(db)
            .await?;

        // 2. 手动创建复合唯一索引（Entity 定义无法表达复合唯一约束）
        manager
            .create_index(
                Index::create()
                    .table(entity::transfer_file::Entity)
                    .name("idx_transfer_files_session_file")
                    .col(entity::transfer_file::Column::SessionId)
                    .col(entity::transfer_file::Column::FileId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 按外键依赖反序删除
        manager
            .drop_table(
                Table::drop()
                    .table(entity::transfer_file::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(entity::transfer_session::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
