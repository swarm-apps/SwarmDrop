use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("ALTER TABLE transfer_sessions ADD COLUMN policy_action TEXT")
            .await?;
        db.execute_unprepared("ALTER TABLE transfer_sessions ADD COLUMN policy_reason TEXT")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE transfer_sessions DROP COLUMN policy_reason")
            .await?;
        db.execute_unprepared("ALTER TABLE transfer_sessions DROP COLUMN policy_action")
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Migration;
    use sea_orm::{ConnectionTrait, Database, Statement};
    use sea_orm_migration::prelude::*;

    #[tokio::test]
    async fn trusted_policy_columns_are_created_and_rolled_back() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let backend = db.get_database_backend();
        db.execute_unprepared(
            r#"
            CREATE TABLE transfer_sessions (
                session_id TEXT PRIMARY KEY NOT NULL,
                direction TEXT NOT NULL,
                peer_id TEXT NOT NULL,
                peer_name TEXT NOT NULL,
                total_size INTEGER NOT NULL,
                transferred_bytes INTEGER NOT NULL,
                status TEXT NOT NULL,
                phase TEXT NOT NULL,
                suspended_reason TEXT,
                terminal_reason TEXT,
                epoch INTEGER NOT NULL,
                recoverable INTEGER NOT NULL,
                source_fingerprint TEXT,
                started_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                finished_at INTEGER,
                error_message TEXT,
                save_path TEXT
            )
            "#,
        )
        .await
        .unwrap();

        Migration.up(&SchemaManager::new(&db)).await.unwrap();

        let policy_action = db
            .query_one_raw(Statement::from_sql_and_values(
                backend,
                "SELECT policy_action, policy_reason FROM transfer_sessions LIMIT 0",
                [],
            ))
            .await;
        assert!(policy_action.is_ok());

        Migration.down(&SchemaManager::new(&db)).await.unwrap();
        let removed = db
            .query_one_raw(Statement::from_sql_and_values(
                backend,
                "SELECT policy_action FROM transfer_sessions LIMIT 0",
                [],
            ))
            .await;
        assert!(removed.is_err());
    }
}
