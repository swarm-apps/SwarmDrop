use sea_orm_migration::prelude::*;

/// `transfer_files.outboard`：发送方 bao-tree post-order outboard（BLOB，可空）。
///
/// 逐块验签的 Merkle 树，prepare 阶段与 checksum 同一遍构建、随会话落库，供 resume 免重算
/// （1GiB 文件 ≈ 4MiB，约 0.4%）。历史/接收方行 NULL——发送端载入缺失时按源文件重算并回存。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files ADD COLUMN outboard BLOB")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_files DROP COLUMN outboard")
            .await?;
        Ok(())
    }
}
