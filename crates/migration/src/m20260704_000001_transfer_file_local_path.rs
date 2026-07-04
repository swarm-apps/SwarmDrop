use sea_orm_migration::prelude::*;

/// `transfer_files.local_path`：接收方文件的最终落盘位置，由 `finalize_sink`
/// 返回后写入（桌面端绝对路径 / 移动端 file:// 或 SAF document URI）。
/// 历史行为 NULL——消费方（收件箱落库）回退到保存目录 + 相对路径的推导。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files ADD COLUMN local_path TEXT")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files DROP COLUMN local_path")
            .await?;
        Ok(())
    }
}
