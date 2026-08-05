use sea_orm_migration::prelude::*;

/// 收件箱条目不再存拼好的展示标题：`inbox_items.title` → `primary_file_name`，
/// 且 `inbox_fts` 去掉冗余的 `title` 列。
///
/// 旧的 `title` 由已删除的 `swarmdrop_transfer::inbox::inbox_title` 拼出中文散文
/// （「空传输」/ 文件名 /「X 等 N 个文件」）并落库，于是三端的 i18n 都对它无从下手，
/// 且存量条目的语言被写入时刻固定。
///
/// **回填不解析旧标题。** 首文件名是结构数据，从 `inbox_item_files` 直接重建即可；
/// 反解析「a.pdf 等 3 个文件」还得处理文件名本身含「 等 」的情况，凭空引入歧义。
///
/// **索引那一列为什么直接删而不是换成 `search_text`**：展示标题是文件列表的纯函数、
/// 且是有损投影——单文件条目的标题就是文件名（已被 `files_text` 覆盖），多文件条目
/// 只多出「等 N 个文件」。判据见 `swarmdrop_transfer::inbox::inbox_matches` 的文档。
///
/// **两件事合在一条迁移里**，因为 `inbox_fts.title` 是 `inbox_items.title` 的镜像：
/// 拆成两条时回滚按注册逆序执行，删列的那条先跑、重建索引的那条读不到 `i.title`，
/// 「索引镜像该列」这条不变量就落到了一个不实现它的人身上。同一条迁移里 `down()`
/// 能先恢复列、再用真实值重灌索引。
#[derive(DeriveMigrationName)]
pub struct Migration;

/// fts5 虚表改列只能重建。`title_column` 为空即新形状，非空即旧形状。
///
/// 数据源是 `inbox_items` / `inbox_item_files` 两张**真表**而不是旧索引：重灌因此是
/// 自愈的（旧索引若曾漏行不会被继承），也与 `m20260630_000001_inbox_fts` 的首次回填同源。
/// 收件箱条目数与用户实际接收次数同阶（几十到几百），这一次 join 的成本可忽略。
async fn rebuild_fts(
    db: &impl sea_orm::ConnectionTrait,
    title_column: Option<&str>,
) -> Result<(), DbErr> {
    db.execute_unprepared("DROP TABLE IF EXISTS inbox_fts")
        .await?;

    let (create_col, insert_col, select_col, group_col) = match title_column {
        Some(expr) => ("title,", "title,", format!("{expr},"), ", i.title"),
        None => ("", "", String::new(), ""),
    };

    db.execute_unprepared(&format!(
        "CREATE VIRTUAL TABLE inbox_fts USING fts5(\
            item_id UNINDEXED, {create_col} source_name, files_text, extracted_text, \
            tokenize = 'trigram')"
    ))
    .await?;

    // LEFT JOIN 保证无文件的条目也建索引行（files_text 为空）。
    db.execute_unprepared(&format!(
        "INSERT INTO inbox_fts(item_id, {insert_col} source_name, files_text, extracted_text) \
         SELECT i.id, {select_col} i.source_name, \
                COALESCE(group_concat(f.name || ' ' || f.relative_path, ' '), ''), '' \
         FROM inbox_items i \
         LEFT JOIN inbox_item_files f ON f.inbox_item_id = i.id \
         GROUP BY i.id, i.source_name{group_col}"
    ))
    .await?;

    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// **必须开事务，否则中途崩溃会砖化数据库。**
    ///
    /// sea-orm 的默认是「只有 Postgres 开事务」（`migrator/exec.rs` 的
    /// `should_use_transaction`：`use_transaction()` 为 `None` 时回退到
    /// `backend == Postgres`），而本仓是 SQLite。`seaql_migrations` 的记录只在 `up()`
    /// 返回 `Ok` 之后才写，于是无事务 + 多语句 + 非幂等三者一凑：
    ///
    /// 断电或进程被杀落在 `ADD COLUMN` 之后 → 迁移未记录 → 下次启动重跑 →
    /// `duplicate column name: primary_file_name` → **应用再也起不来，且没有恢复路径**。
    /// 落在 `DROP TABLE inbox_fts` 与 `CREATE VIRTUAL TABLE` 之间更糟：库里连索引表都没了，
    /// 此后每完成一次接收都会报 `no such table: inbox_fts`。
    ///
    /// 本仓其余迁移躲过这条，是因为它们要么是单语句、要么全程 `IF NOT EXISTS` 守着重入；
    /// 这是第一条多语句且非幂等的（全表 UPDATE + `DROP COLUMN` 触发的整表重写 +
    /// fts5 重建，窗口也是最长的一条）。SQLite 的 DDL 是事务性的，含 fts5 的 DROP/CREATE。
    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("ALTER TABLE inbox_items ADD COLUMN primary_file_name TEXT")
            .await?;

        // `ORDER BY f.id` 与运行时同源：`inbox_item_files` 是按 `transfer_files` 的顺序
        // 插入的，自增 id 升序即那个顺序（顺序契约见 `InboxFileFacts` 的文档）。
        // 取不到文件行的条目留 NULL——`item_count > 0` 却没有文件行是异常数据，
        // 展示端会退化成「空传输」，不为它发明新的展示态。
        db.execute_unprepared(
            r#"
            UPDATE inbox_items SET primary_file_name = (
                SELECT f.name FROM inbox_item_files f
                WHERE f.inbox_item_id = inbox_items.id
                ORDER BY f.id LIMIT 1
            )
            "#,
        )
        .await?;

        db.execute_unprepared("ALTER TABLE inbox_items DROP COLUMN title")
            .await?;

        rebuild_fts(db, None).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("ALTER TABLE inbox_items ADD COLUMN title TEXT NOT NULL DEFAULT ''")
            .await?;

        // ⚠️ 下面这段中文散文是**旧格式的逐字复刻**，只为让回滚后的旧代码有可显示的标题。
        // 它不是现行规则——现行规则是「领域层不产出展示串」。不要把它当模板抄进任何新代码。
        //
        // 分支顺序与三端前端的判别一致：**先判首文件名缺席**，再判文件数。反过来写
        // （对 `primary_file_name` 用 `COALESCE`）会让「`item_count >= 2` 却没有文件行」
        // 这类异常数据还原出 `" 等 3 个文件"`——一个带前导空格的畸形标题。
        db.execute_unprepared(
            r#"
            UPDATE inbox_items SET title = CASE
                WHEN primary_file_name IS NULL THEN '空传输'
                WHEN item_count <= 1 THEN primary_file_name
                ELSE primary_file_name || ' 等 ' || item_count || ' 个文件'
            END
            "#,
        )
        .await?;

        db.execute_unprepared("ALTER TABLE inbox_items DROP COLUMN primary_file_name")
            .await?;

        // 列已经恢复且填好，所以这里能用**真实值**重灌索引，而不是留一列空串。
        rebuild_fts(db, Some("i.title")).await
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use sea_orm_migration::MigratorTrait;

    use crate::Migrator;

    /// 迁到本条之前，种两个条目：一个三文件、一个零文件。
    async fn seed() -> sea_orm::DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");
        crate::up_through(&db, "m20260730_000001_pair_invites").await;

        db.execute_unprepared(
            r#"
            INSERT INTO inbox_items
                (id, source_peer_id, source_name, source_kind, content_kind,
                 title, item_count, total_size, received_at)
            VALUES
                ('item-multi', 'peer', 'Alice', 'paired_device', 'files',
                 'a.txt 等 3 个文件', 3, 30, 100),
                ('item-empty', 'peer', 'Bob', 'paired_device', 'files',
                 '空传输', 0, 0, 200)
            "#,
        )
        .await
        .expect("seed inbox_items");

        db.execute_unprepared(
            r#"
            INSERT INTO inbox_item_files
                (id, inbox_item_id, relative_path, name, size, checksum, local_path, missing)
            VALUES
                (10, 'item-multi', 'a.txt', 'a.txt', 10, 'sum-a', '/s/a.txt', 0),
                (11, 'item-multi', 'docs/b.txt', 'b.txt', 10, 'sum-b', '/s/b.txt', 0),
                (12, 'item-multi', 'docs/c.txt', 'c.txt', 10, 'sum-c', '/s/c.txt', 0)
            "#,
        )
        .await
        .expect("seed inbox_item_files");

        db
    }

    /// 回填取的是**第一个**文件名（按 id 升序，与 `inbox_content_hash` 的累加顺序同源），
    /// 零文件条目留 NULL 而不是空串。
    #[tokio::test]
    async fn backfills_primary_file_name_and_drops_title() {
        let db = seed().await;
        // 精确再走一条（本迁移），不要用 `up_through`：它的 `Some(position + 1)` 是
        // 「再应用 N 条待应用迁移」，在已经迁到中途的库上会把后续所有迁移一起跑完。
        Migrator::up(&db, Some(1)).await.expect("apply migration");

        assert_eq!(
            primary_file_name(&db, "item-multi").await,
            Some("a.txt".to_string()),
            "多文件条目应回填 id 最小的那个文件名"
        );
        assert_eq!(
            primary_file_name(&db, "item-empty").await,
            None,
            "零文件条目应留 NULL，而不是空串"
        );
        db.execute_unprepared("SELECT title FROM inbox_items")
            .await
            .expect_err("inbox_items.title 应已删除");
    }

    /// 索引重建：title 列消失，其余三列仍能按真表内容命中，模板文字不再命中。
    #[tokio::test]
    async fn fts_reindexed_without_title_column() {
        let db = seed().await;
        Migrator::up(&db, Some(1)).await.expect("apply migration");

        db.execute_unprepared("SELECT title FROM inbox_fts")
            .await
            .expect_err("inbox_fts.title 应已删除");

        assert_eq!(
            hits(&db, "%docs/b%").await,
            1,
            "存量条目应能按文件路径搜到（重灌来自 inbox_item_files）"
        );
        assert_eq!(
            hits(&db, "%个文件%").await,
            0,
            "「个文件」是旧标题模板的固定文字，不该命中任何条目"
        );
    }

    /// 回滚要把两样都还原：`inbox_items.title` 的旧格式散文，以及**镜像它的索引列**。
    ///
    /// 后者是拆成两条迁移时会漏掉的那一半——那时删列的一条先跑、重建索引的一条读不到
    /// `i.title`，只能灌空串，于是降级后的旧代码检索少一列且无人报错。
    #[tokio::test]
    async fn rollback_restores_title_and_its_index() {
        let db = seed().await;
        Migrator::up(&db, Some(1)).await.expect("apply migration");
        Migrator::down(&db, Some(1)).await.expect("rollback");

        assert_eq!(title(&db, "item-multi").await, "a.txt 等 3 个文件");
        assert_eq!(title(&db, "item-empty").await, "空传输");

        let indexed: String = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT title FROM inbox_fts WHERE item_id = 'item-multi'",
                [],
            ))
            .await
            .expect("query inbox_fts")
            .expect("row")
            .try_get_by_index(0)
            .expect("title value");
        assert_eq!(
            indexed, "a.txt 等 3 个文件",
            "回滚后索引必须镜像真实的 title，而不是空串"
        );
    }

    /// 这条迁移**必须**在事务里跑（理由见 `use_transaction` 的文档）。
    ///
    /// 断言一个常量看着没用，但它守的是一件删掉后毫无提示的事：sea-orm 在 SQLite 上
    /// 默认不开事务，而本迁移是多语句非幂等的，裸跑时中途崩溃会让应用永久起不来。
    #[test]
    fn runs_in_a_transaction() {
        use sea_orm_migration::MigrationTrait;
        assert_eq!(
            super::Migration.use_transaction(),
            Some(true),
            "多语句非幂等迁移不开事务 = 中途崩溃即砖化数据库"
        );
    }

    /// 异常数据（`item_count >= 2` 却没有文件行）回滚后不该产出畸形标题。
    #[tokio::test]
    async fn rollback_handles_items_without_file_rows() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");
        crate::up_through(&db, "m20260730_000001_pair_invites").await;
        db.execute_unprepared(
            r#"
            INSERT INTO inbox_items
                (id, source_peer_id, source_name, source_kind, content_kind,
                 title, item_count, total_size, received_at)
            VALUES
                ('item-orphan', 'peer', 'Carol', 'paired_device', 'files',
                 '早已丢失.txt 等 2 个文件', 2, 20, 300)
            "#,
        )
        .await
        .expect("seed orphan item");

        Migrator::up(&db, Some(1)).await.expect("apply migration");
        assert_eq!(primary_file_name(&db, "item-orphan").await, None);

        Migrator::down(&db, Some(1)).await.expect("rollback");
        assert_eq!(
            title(&db, "item-orphan").await,
            "空传输",
            "没有首文件名时不该还原出「 等 2 个文件」这种带前导空格的畸形标题"
        );
    }

    async fn primary_file_name(db: &sea_orm::DatabaseConnection, id: &str) -> Option<String> {
        db.query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT primary_file_name FROM inbox_items WHERE id = ?",
            [id.into()],
        ))
        .await
        .expect("query primary_file_name")
        .expect("row")
        .try_get_by_index::<Option<String>>(0)
        .expect("column value")
    }

    async fn title(db: &sea_orm::DatabaseConnection, id: &str) -> String {
        db.query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT title FROM inbox_items WHERE id = ?",
            [id.into()],
        ))
        .await
        .expect("query title")
        .expect("row")
        .try_get_by_index::<String>(0)
        .expect("column value")
    }

    async fn hits(db: &sea_orm::DatabaseConnection, pattern: &str) -> i64 {
        db.query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) FROM inbox_fts \
             WHERE source_name LIKE ? OR files_text LIKE ? OR extracted_text LIKE ?",
            [pattern.into(), pattern.into(), pattern.into()],
        ))
        .await
        .expect("query inbox_fts")
        .expect("count row")
        .try_get_by_index::<i64>(0)
        .expect("count value")
    }
}
