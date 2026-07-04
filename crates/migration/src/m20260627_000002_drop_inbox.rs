use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS inbox_items (
                id uuid_text NOT NULL PRIMARY KEY,
                transfer_session_id uuid_text,
                source_peer_id text NOT NULL,
                source_name varchar NOT NULL,
                source_kind varchar NOT NULL,
                content_kind varchar NOT NULL,
                title varchar NOT NULL,
                item_count integer NOT NULL,
                total_size integer NOT NULL,
                root_path varchar,
                content_hash varchar,
                received_at integer NOT NULL,
                last_opened_at integer,
                archived_at integer,
                deleted_at integer,
                FOREIGN KEY (transfer_session_id)
                    REFERENCES transfer_sessions (session_id)
                    ON DELETE SET NULL
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS inbox_item_files (
                id integer NOT NULL PRIMARY KEY AUTOINCREMENT,
                inbox_item_id uuid_text NOT NULL,
                transfer_file_id integer,
                relative_path varchar NOT NULL,
                name varchar NOT NULL,
                size integer NOT NULL,
                checksum varchar NOT NULL,
                local_path varchar NOT NULL,
                missing integer NOT NULL DEFAULT 0,
                FOREIGN KEY (inbox_item_id)
                    REFERENCES inbox_items (id)
                    ON DELETE CASCADE,
                FOREIGN KEY (transfer_file_id)
                    REFERENCES transfer_files (id)
                    ON DELETE SET NULL
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_inbox_items_transfer_session ON inbox_items (transfer_session_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_inbox_items_received_at ON inbox_items (received_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_inbox_items_deleted_archived ON inbox_items (deleted_at, archived_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_inbox_item_files_item ON inbox_item_files (inbox_item_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_inbox_item_files_transfer_file ON inbox_item_files (transfer_file_id)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_inbox_item_files_transfer_file")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_inbox_item_files_item")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_inbox_items_deleted_archived")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_inbox_items_received_at")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_inbox_items_transfer_session")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS inbox_item_files")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS inbox_items")
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use sea_orm_migration::MigratorTrait;

    use crate::Migrator;

    #[tokio::test]
    async fn drop_inbox_tables_are_created_and_rolled_back() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");

        crate::up_through(&db, "m20260627_000002_drop_inbox").await;
        assert!(table_exists(&db, "inbox_items").await);
        assert!(table_exists(&db, "inbox_item_files").await);
        assert!(index_exists(&db, "idx_inbox_items_transfer_session").await);

        Migrator::down(&db, Some(1))
            .await
            .expect("rollback inbox migration");
        assert!(!table_exists(&db, "inbox_item_files").await);
        assert!(!table_exists(&db, "inbox_items").await);
    }

    async fn table_exists(db: &sea_orm::DatabaseConnection, table: &str) -> bool {
        sqlite_master_count(db, "table", table).await > 0
    }

    async fn index_exists(db: &sea_orm::DatabaseConnection, index: &str) -> bool {
        sqlite_master_count(db, "index", index).await > 0
    }

    async fn sqlite_master_count(db: &sea_orm::DatabaseConnection, kind: &str, name: &str) -> i64 {
        let row = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ? AND name = ?",
                [kind.into(), name.into()],
            ))
            .await
            .expect("query sqlite_master")
            .expect("count row");
        row.try_get_by_index::<i64>(0).expect("count value")
    }
}
