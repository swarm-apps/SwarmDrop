use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // standalone FTS5（trigram）虚拟表：
        // - item_id 仅存储不索引（UNINDEXED），承载 inbox_items 的 UUID-TEXT 主键
        // - title / source_name / files_text / extracted_text 为可子串匹配的文本列
        // - extracted_text 本期留空，预留给未来 OCR / 文本抽取
        // - trigram tokenizer：CJK 友好，且能加速 ≥3 字符的 LIKE 子串查询
        db.execute_unprepared(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS inbox_fts USING fts5(
                item_id UNINDEXED,
                title,
                source_name,
                files_text,
                extracted_text,
                tokenize = 'trigram'
            )
            "#,
        )
        .await?;

        // 幂等：迁移正常只跑一次，这里清空保证重入安全。
        db.execute_unprepared("DELETE FROM inbox_fts").await?;

        // 一次性回填存量：按 item 聚合其所有文件名 + 相对路径写入 files_text。
        // LEFT JOIN 保证无文件的条目也建索引行（files_text 为空）。
        db.execute_unprepared(
            r#"
            INSERT INTO inbox_fts(item_id, title, source_name, files_text, extracted_text)
            SELECT
                i.id,
                i.title,
                i.source_name,
                COALESCE(group_concat(f.name || ' ' || f.relative_path, ' '), ''),
                ''
            FROM inbox_items i
            LEFT JOIN inbox_item_files f ON f.inbox_item_id = i.id
            GROUP BY i.id, i.title, i.source_name
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS inbox_fts")
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
    async fn inbox_fts_created_and_rolled_back() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");

        crate::up_through(&db, "m20260630_000001_inbox_fts").await;
        assert!(fts_exists(&db).await, "inbox_fts 应被创建");

        // 空库回填后仍可正常查询（验证 FTS5 + trigram 编译可用、回填 SQL 合法）。
        db.execute_unprepared("SELECT item_id FROM inbox_fts WHERE files_text LIKE '%x%'")
            .await
            .expect("FTS5 trigram 查询应可执行");

        Migrator::down(&db, Some(1))
            .await
            .expect("rollback inbox_fts migration");
        assert!(!fts_exists(&db).await, "inbox_fts 应被回滚删除");
    }

    async fn fts_exists(db: &sea_orm::DatabaseConnection) -> bool {
        let row = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'inbox_fts'",
                [],
            ))
            .await
            .expect("query sqlite_master")
            .expect("count row");
        row.try_get_by_index::<i64>(0).expect("count value") > 0
    }
}
