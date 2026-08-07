//! 全量初始化迁移 —— 取代此前 12 个增量迁移。
//!
//! ## 为什么推倒重来
//!
//! 那 12 个迁移里有 11 个是裸 SQL（45 处 `execute_unprepared`），大半是 SQLite 的
//! 「建新表 → 拷数据 → 删旧表 → 改名」表重建（当年那个 SQLite 既不支持 `DROP COLUMN`
//! 也不支持改列类型；前者已在 3.35 补上，见 [`crate`] 的模块文档，后者至今没有）。
//! 那些语句表达的是**历史演进过程**，而不是当前 schema —— 从零建库时它们只是把一张表
//! 建了又拆、拆了又建。squash 成一份之后，DDL 只剩「最终形态是什么」，且能全部交给
//! sea-orm 的 schema builder 从 entity 生成：**本迁移零 `execute_unprepared`**。
//!
//! 最后一处挡路的是 `inbox_fts` 的 `CREATE VIRTUAL TABLE ... USING fts5(...)`
//! —— DSL 无法表达。但检索从来没用过 FTS5 的 `MATCH`，虚表只当预聚合文本表使，
//! 于是它改成了普通表 `inbox_search_index`（理由写在
//! [`entity::inbox_search_index`](../../../entity/src/inbox_search_index.rs) 的文档里）。
//!
//! ## 时间胶囊
//!
//! `entity/` 子目录是**冻结的** entity 快照，不是 `use entity::...`。主 crate 的 entity
//! 之后怎么改，本迁移建出来的表都不变 —— 这是 sea-orm 官方对「migration 里用
//! SchemaBuilder」的既定要求，也是让迁移保持确定性的唯一办法。
//!
//! ## 存量库
//!
//! squash 让所有已存在的库的 `seaql_migrations` 里出现 11 条本代码不认识的版本，
//! `Migrator::up` 会直接 `DbErr::Custom("... is missing ...")`。宿主侧据此**删库重建**
//! （`src-tauri/src/database.rs` 与 mobile-core 的 `app.rs`）。丢的是传输历史 / 收件箱 /
//! 邀请注册表；设备身份与已配对设备存在 keychain，不在这个库里，配对关系不受影响。

// `pub(crate)`：`m20260806_000001_inbox_title_to_file_name` 复用这份快照。两个迁移之间
// **没有任何 DDL**（那条是纯数据回填），所以它看到的 schema 与本迁移建出来的逐字相同，
// 再复制一份只会多出一处要同步的副本。**将来有迁移改了这三张表的结构，就不能再复用了**
// —— 那时给新迁移单独冻结一份。
pub(crate) mod entity;

// `EntityName` 提供 `table_ref()`：`drop_table` 那段要把六个不同的 Entity 类型
// 收进同一个数组，只能先统一成 `TableRef`。
use sea_orm::EntityName;
use sea_orm_migration::prelude::*;

pub struct Migration;

/// **不要用 `DeriveMigrationName`。** 它取的是 `get_file_stem(file!())`，而本迁移是
/// 「目录 + `mod.rs`」的形态，于是版本名会变成 **`"mod"`** —— 与被删掉的那个旧 init
/// （`m20260228_000001_init/mod.rs`）**完全撞名**。
///
/// 后果是静默且致命的：停留在 v0.3.3 ~ v0.4.2 的库里 `seaql_migrations` 只有一行 `mod`，
/// 升级后 `migration_in_fs` 与 `migration_in_db` 都是 `{mod}` ⇒ pending 空、missing 也空
/// ⇒ `Migrator::up` 返回 `Ok(())`、[`crate::connect_and_migrate`] 的自愈路径**永不触发**，
/// 应用继续跑在 2026-02 的两表 schema 上，第一次查收件箱就 `no such table: inbox_items`。
///
/// 手写名字之后，那种库的 `mod` 会落进 `missing_migrations_in_fs` → 判定为过时历史 →
/// 删库重建，走的正是设计好的那条路。
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260805_000001_init"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 建表顺序由 schema builder 按外键依赖自动拓扑排序 —— SQLite 不支持后加外键，
        // 顺序错了就是硬错误，所以这件事不该由手写顺序承担。
        db.get_schema_builder()
            .register(entity::transfer_session::Entity)
            .register(entity::transfer_file::Entity)
            .register(entity::inbox_item::Entity)
            .register(entity::inbox_item_file::Entity)
            .register(entity::inbox_search_index::Entity)
            .register(entity::pair_invite::Entity)
            .apply(db)
            .await?;

        // 单列索引（`indexed`）与唯一键（`unique` / `unique_key`）都已在 entity 上声明，
        // 由上面的 `apply` 一并建出。**只有复合非唯一索引 entity 表达不了** ——
        // `indexed` 是列级属性、`unique_key` 只组合唯一键，所以这一条手写。
        // 用的仍是 sea-query DSL，不是裸 SQL。
        manager
            .create_index(
                Index::create()
                    .table(entity::inbox_item::Entity)
                    .name("idx_inbox_items_deleted_archived")
                    .col(entity::inbox_item::Column::DeletedAt)
                    .col(entity::inbox_item::Column::ArchivedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 按外键依赖反序删：引用方先走。
        for table in [
            entity::inbox_search_index::Entity.table_ref(),
            entity::inbox_item_file::Entity.table_ref(),
            entity::inbox_item::Entity.table_ref(),
            entity::transfer_file::Entity.table_ref(),
            entity::transfer_session::Entity.table_ref(),
            entity::pair_invite::Entity.table_ref(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use sea_orm_migration::MigratorTrait;

    use crate::Migrator;

    /// 版本名必须是**目录名**，不是 `mod.rs` 的 stem。
    ///
    /// 这条红了说明有人把 `DeriveMigrationName` 加回来了 —— 那会让版本名退回 `"mod"`，
    /// 与被删的旧 init 撞名，古董库升级时自愈路径会被静默跳过（详见 `Migration` 上的注释）。
    #[test]
    fn migration_name_is_the_directory_not_mod() {
        use sea_orm_migration::MigrationName;
        assert_eq!(super::Migration.name(), "m20260805_000001_init");
    }

    /// 六张表 + 全部索引建出来，且能整体回滚。
    ///
    /// 索引名分两类：`idx-{table}-{key}` 是 schema builder 从 entity 生成的，
    /// `idx_inbox_items_deleted_archived` 是上面手写的那条复合非唯一索引。
    /// **断言索引存在不是形式主义** —— 它们此前散在 5 个迁移的裸 SQL 里，
    /// 这次全部改由 entity 属性声明，漏标一个属性不会有任何编译信号。
    #[tokio::test]
    async fn init_creates_all_tables_and_indexes() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");

        Migrator::up(&db, None).await.expect("run migrations");

        for table in [
            "transfer_sessions",
            "transfer_files",
            "inbox_items",
            "inbox_item_files",
            "inbox_search_index",
            "pair_invites",
        ] {
            assert!(
                object_exists(&db, "table", table).await,
                "表 {table} 应被创建"
            );
        }

        for index in [
            "idx-transfer_files-session_file",
            "idx-inbox_items-received_at",
            "idx-inbox_item_files-inbox_item_id",
            "idx-inbox_item_files-transfer_file_id",
            "idx-pair_invites-expires_at",
            "idx_inbox_items_deleted_archived",
        ] {
            assert!(
                object_exists(&db, "index", index).await,
                "索引 {index} 应被创建"
            );
        }

        Migrator::down(&db, None).await.expect("rollback");
        assert!(!object_exists(&db, "table", "transfer_sessions").await);
        assert!(!object_exists(&db, "table", "inbox_search_index").await);
    }

    /// `inbox_items.transfer_session_id` 必须是 `ON DELETE SET NULL`。
    ///
    /// 这条外键是「清空传输历史不动收件箱」的实现基础，而它此前只存在于
    /// `m20260627_000002_drop_inbox` 的裸 SQL 里 —— entity 从未表达过。改走 schema
    /// builder 之后若忘了在 entity 上写 `on_delete`，约束会静默消失，表现是删会话时
    /// 收件箱条目跟着遭殃或删除直接失败。这条测试直接删一行会话来验行为，
    /// 不解析 DDL 文本。
    #[tokio::test]
    async fn deleting_a_session_nulls_the_inbox_link_instead_of_cascading() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");
        Migrator::up(&db, None).await.expect("run migrations");

        // sqlx 默认 `PRAGMA foreign_keys=ON`，这里显式确认一次：外键若没开，
        // 本测试会「通过」但什么也没验证到。
        assert_eq!(scalar(&db, "PRAGMA foreign_keys").await, 1, "外键必须启用");

        exec(
            &db,
            "INSERT INTO transfer_sessions (session_id, direction, peer_id, peer_name, \
             total_size, transferred_bytes, status, phase, epoch, recoverable, \
             started_at, updated_at) \
             VALUES (x'01', 'receive', 'peer', 'Peer', 0, 0, 'completed', 'terminal', 1, 0, 0, 0)",
        )
        .await;
        exec(
            &db,
            "INSERT INTO inbox_items (id, transfer_session_id, source_peer_id, source_name, \
             source_kind, content_kind, title, item_count, total_size, received_at) \
             VALUES (x'02', x'01', 'peer', 'Peer', 'paired_device', 'files', 'T', 1, 0, 0)",
        )
        .await;

        exec(&db, "DELETE FROM transfer_sessions").await;

        assert_eq!(
            scalar(&db, "SELECT COUNT(*) FROM inbox_items").await,
            1,
            "收件箱条目必须留下（删过程不动结果）"
        );
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM inbox_items WHERE transfer_session_id IS NULL"
            )
            .await,
            1,
            "外键必须被置空，而不是保留悬空引用"
        );
    }

    async fn exec(db: &sea_orm::DatabaseConnection, sql: &str) {
        db.execute_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .unwrap_or_else(|e| panic!("执行失败: {sql}\n{e}"));
    }

    async fn scalar(db: &sea_orm::DatabaseConnection, sql: &str) -> i64 {
        db.query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("query")
            .expect("row")
            .try_get_by_index::<i64>(0)
            .expect("scalar")
    }

    async fn object_exists(db: &sea_orm::DatabaseConnection, kind: &str, name: &str) -> bool {
        let row = db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ? AND name = ?",
                [kind.into(), name.into()],
            ))
            .await
            .expect("query sqlite_master")
            .expect("count row");
        row.try_get_by_index::<i64>(0).expect("count value") > 0
    }
}
