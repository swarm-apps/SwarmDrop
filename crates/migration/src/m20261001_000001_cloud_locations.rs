//! 位置改为本地/云端联合体，保留存量本地文件的实际 URI。
use sea_orm::{ConnectionTrait, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;
#[derive(DeriveIden)]
enum TransferFiles {
    Table,
    Id,
    Location,
    StagedComplete,
    LocalPath,
    LocalDir,
}
#[derive(DeriveIden)]
enum InboxItemFiles {
    Table,
    Id,
    Location,
    LocalPath,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // SeaORM 默认不为 SQLite 迁移开事务；回填和删旧列必须与迁移记录一起原子提交。
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(TransferFiles::Table)
                    .add_column(ColumnDef::new(TransferFiles::Location).json())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(InboxItemFiles::Table)
                    .add_column(ColumnDef::new(InboxItemFiles::Location).json())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TransferFiles::Table)
                    .add_column(
                        ColumnDef::new(TransferFiles::StagedComplete)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        let db = manager.get_connection();
        let backend = db.get_database_backend();
        let rows = db
            .query_all_raw(Statement::from_string(
                backend,
                "SELECT id, local_path, local_dir FROM transfer_files WHERE local_path IS NOT NULL",
            ))
            .await?;
        for row in rows {
            let id: i32 = row.try_get("", "id")?;
            let uri: String = row.try_get("", "local_path")?;
            let dir: Option<String> = row.try_get("", "local_dir")?;
            let value =
                serde_json::json!({"type":"local", "uri":uri, "dir":dir.unwrap_or_default()});
            db.execute(
                Query::update()
                    .table(TransferFiles::Table)
                    .value(TransferFiles::Location, value)
                    .and_where(Expr::col(TransferFiles::Id).eq(id)),
            )
            .await?;
        }
        let rows = db.query_all_raw(Statement::from_string(backend, "SELECT f.id, f.local_path, COALESCE(t.local_dir, i.root_path, '') AS dir FROM inbox_item_files f LEFT JOIN transfer_files t ON t.id = f.transfer_file_id LEFT JOIN inbox_items i ON i.id = f.inbox_item_id")).await?;
        for row in rows {
            let id: i32 = row.try_get("", "id")?;
            let uri: String = row.try_get("", "local_path")?;
            let dir: String = row.try_get("", "dir")?;
            let value = serde_json::json!({"type":"local", "uri":uri, "dir":dir});
            db.execute(
                Query::update()
                    .table(InboxItemFiles::Table)
                    .value(InboxItemFiles::Location, value)
                    .and_where(Expr::col(InboxItemFiles::Id).eq(id)),
            )
            .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(TransferFiles::Table)
                    .drop_column(TransferFiles::LocalPath)
                    .to_owned(),
            )
            .await?;
        // SeaQuery 的 SQLite 构造器会在同一 ALTER 中含多个操作时 panic，必须逐列删除。
        manager
            .alter_table(
                Table::alter()
                    .table(TransferFiles::Table)
                    .drop_column(TransferFiles::LocalDir)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(InboxItemFiles::Table)
                    .drop_column(InboxItemFiles::LocalPath)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        // 云对象没有可恢复的本地路径；拒绝会造成历史位置丢失的降级。
        Err(DbErr::Custom(
            "云位置迁移不能自动降级；请恢复迁移前备份".into(),
        ))
    }
}
