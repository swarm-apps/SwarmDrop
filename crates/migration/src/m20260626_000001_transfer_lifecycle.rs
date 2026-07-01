use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 开发期策略：清空旧传输历史。
        // 旧扁平 SessionStatus（5 态）语义与新 phase + reason 模型不兼容，
        // 本阶段不做一次性数据迁移，正式发布前再决定（见 design.md Migration Plan）。
        db.execute_unprepared("DELETE FROM transfer_files").await?;
        db.execute_unprepared("DELETE FROM transfer_sessions")
            .await?;

        // 新增生命周期列（过渡期与 status 并存）
        for stmt in [
            "ALTER TABLE transfer_sessions ADD COLUMN phase TEXT NOT NULL DEFAULT 'active'",
            "ALTER TABLE transfer_sessions ADD COLUMN suspended_reason TEXT",
            "ALTER TABLE transfer_sessions ADD COLUMN terminal_reason TEXT",
            "ALTER TABLE transfer_sessions ADD COLUMN epoch INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE transfer_sessions ADD COLUMN recoverable INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE transfer_sessions ADD COLUMN source_fingerprint TEXT",
        ] {
            db.execute_unprepared(stmt).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for col in [
            "phase",
            "suspended_reason",
            "terminal_reason",
            "epoch",
            "recoverable",
            "source_fingerprint",
        ] {
            db.execute_unprepared(&format!("ALTER TABLE transfer_sessions DROP COLUMN {col}"))
                .await?;
        }
        Ok(())
    }
}
