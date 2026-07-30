use sea_orm_migration::prelude::*;

/// `pair_invites`：本机发出的配对邀请状态表（openspec: invite-persistence）。
///
/// 让邀请跨重启存活 —— TTL 从 5 分钟放到 24 小时后，内存态注册表（重启即丢）不再够用：
/// 用户「发条链接给同事，自己顺手重启了 App」这条最普通的路径原本必然失败。
///
/// **表里没有 capability 明文，也没有邀请全串**，只有它的 sha256。
/// `state` 见 `entity::pair_invite::InviteState`（`consumed` 要留到过期后才可删）。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE pair_invites (
                capability_hash TEXT NOT NULL PRIMARY KEY,
                inviter_id      TEXT NOT NULL,
                expires_at      INTEGER NOT NULL,
                state           TEXT NOT NULL,
                created_at      INTEGER NOT NULL
            )",
        )
        .await?;
        // 清理与「列出未过期邀请」都按 expires_at 过滤
        db.execute_unprepared(
            "CREATE INDEX idx_pair_invites_expires_at ON pair_invites (expires_at)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE pair_invites").await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use sea_orm_migration::MigratorTrait;

    use crate::Migrator;

    #[tokio::test]
    async fn pair_invites_table_is_created_and_rolled_back() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");

        crate::up_through(&db, "m20260730_000001_pair_invites").await;
        assert!(table_exists(&db, "pair_invites").await);
        assert!(index_exists(&db, "idx_pair_invites_expires_at").await);

        Migrator::down(&db, Some(1))
            .await
            .expect("rollback pair_invites migration");
        assert!(!table_exists(&db, "pair_invites").await);
        // DROP TABLE 会连带删掉表上的索引
        assert!(!index_exists(&db, "idx_pair_invites_expires_at").await);
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
