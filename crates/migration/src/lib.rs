//! 数据库迁移。
//!
//! 2026-08-05 把此前的 12 个增量迁移 squash 成了一份全量初始化
//! （成因与存量库的处理见 [`m20260805_000001_init`] 的模块文档），其后的都是增量。
//!
//! ## 加新迁移时
//!
//! 新迁移一律**增量**，不要再改 init：
//!
//! - 加列 / 加索引 / 加表 → `Table::alter()` / `Index::create()` / `Table::create()`，
//!   都是 sea-query DSL。**不要 `execute_unprepared`**，这次清理的就是它。
//! - 删列 / 改列类型 → SQLite 不支持，只能建新表拷数据。这是唯一还需要裸 SQL 的场景；
//!   真遇到时先想想是不是可以再 squash 一次（本仓对存量库的取舍是「删库重建」，
//!   代价只有传输历史与收件箱）。
//! - **加了新 entity 或改了列，init 的冻结快照不要跟着改** —— 它是历史的固定形态，
//!   跟着改会让「从零建库」与「增量升级」建出两种不同的 schema。

pub use sea_orm_migration::prelude::*;

mod m20260805_000001_init;
mod m20260806_000001_inbox_title_to_file_name;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260805_000001_init::Migration),
            Box::new(m20260806_000001_inbox_title_to_file_name::Migration),
        ]
    }
}

/// 打开 SQLite 库并跑迁移；**迁移历史已过时就删库重建**。
///
/// 桌面与移动的启动路径逐字相同（同一种连接串、同一套自愈），所以收在这里而不是各写
/// 一份 —— 这段代码的正确性依赖对 sea-orm 错误语义的理解，它该和
/// [`is_stale_migration_history`] 待在一起。
///
/// 重建丢的是**这个库里的东西**：传输历史、收件箱、邀请注册表。设备身份与已配对设备
/// 存在 keychain（桌面 release）/ `dev-identity.json`（桌面 debug）/ 平台安全存储（移动），
/// 不在这个库里，**配对关系不受影响**；已落盘的文件本身也不动。
pub async fn connect_and_migrate(
    db_path: &std::path::Path,
) -> Result<sea_orm::DatabaseConnection, DbErr> {
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let db = sea_orm::Database::connect(&db_url).await?;

    match Migrator::up(&db, None).await {
        Ok(()) => return Ok(db),
        Err(err) if is_stale_migration_history(&err) => {
            tracing::warn!(
                "数据库的迁移历史已过时，将重建：{err}\n\
                 传输历史 / 收件箱 / 已发出的邀请会清空；\
                 设备身份与已配对设备不在此库中，配对关系与已接收的文件不受影响。"
            );
            // 必须先关连接再删文件：Windows 上打开中的文件删不掉，
            // 而「删了个寂寞然后重连到同一个旧库」会再报一次同样的错、变成启动死循环。
            db.close().await?;
        }
        Err(err) => return Err(err),
    }

    remove_sqlite_files(db_path)?;

    let db = sea_orm::Database::connect(&db_url).await?;
    Migrator::up(&db, None).await?;
    tracing::info!("数据库已重建");
    Ok(db)
}

/// 删主库文件连同 `-wal` / `-shm` 副文件。
///
/// 当前连接不设 `journal_mode`（sqlx 0.9 的默认就是不设，走 SQLite 的 `delete` 模式），
/// 所以副文件通常不存在 —— 但 journal 模式是**写在库文件头里的持久设置**，
/// 只删主文件而把某个历史版本留下的 `-wal` 落在原地，新库会读到一段本该消失的旧事务。
/// 三个一起删是廉价的确定性。
fn remove_sqlite_files(db_path: &std::path::Path) -> Result<(), DbErr> {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db_path.as_os_str().to_os_string();
        name.push(suffix);
        let path = std::path::PathBuf::from(name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            // 副文件多半本来就不存在，主文件不存在也说明无事可做。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(DbErr::Custom(format!(
                    "重建数据库失败，无法删除 {}: {e}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

/// `Migrator::up` 是否因「库里记着本代码不存在的迁移」而失败。
///
/// squash 之后**每一个存量库都会撞上它**：sea-orm 的 `get_migration_with_status` 算
/// `已应用 − 代码里有的` 这个差集，非空就为其中每条拼一句
/// `Migration file of version '…' is missing, this migration has been applied but its
/// file is missing`，整体包成 [`DbErr::Custom`]。它发生在任何 DDL 之前，所以库本身
/// 是完好的、只是这份代码认不出它的历史。
///
/// [`connect_and_migrate`] 据此**删库重建**——宿主只调那一个函数，不直接用本判据
/// （所以它是 `pub(crate)`）。不这么处理的话，它就是一个冒泡到 setup 的启动错误，
/// 表现为应用直接打不开。
///
/// **判据故意收得很窄。** sea-orm 没有为这种情况定义专门的错误变体，只能认措辞；
/// 而把任意 `DbErr::Custom` 都当成「该删库」会把真正的迁移失败（写坏的 DDL、磁盘满、
/// 权限问题）也变成一次静默的数据清除 —— 那是本函数最需要避免的事。
pub(crate) fn is_stale_migration_history(err: &DbErr) -> bool {
    matches!(
        err,
        DbErr::Custom(msg) if msg.contains("this migration has been applied but its file is missing")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};

    /// 判据必须认得 sea-orm 真实产出的那条错误 —— 手抄一份措辞去断言等于自己跟自己对暗号，
    /// 所以这里造一个「库里多一条未知迁移」的真实场景让它自己报错。
    #[tokio::test]
    async fn detects_migration_history_the_code_no_longer_knows() {
        let db = Database::connect("sqlite::memory:").await.expect("connect");
        Migrator::up(&db, None).await.expect("first run");

        // 伪造一条来自「未来版本」的已应用记录，模拟降级或 squash 后的存量库。
        db.execute_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO seaql_migrations (version, applied_at) VALUES ('m29990101_000001_x', 0)",
        ))
        .await
        .expect("seed unknown migration");

        let err = Migrator::up(&db, None).await.expect_err("应当报未知迁移");
        assert!(
            is_stale_migration_history(&err),
            "判据没认出 sea-orm 的真实错误: {err}"
        );
    }

    /// 端到端：库里留着本代码不认识的迁移记录时，`connect_and_migrate` 重建它并正常返回。
    ///
    /// 判据测试只证明「认得出那条错误」，这条才证明**自愈真的走得通** ——
    /// 关连接、删文件、重连、重跑迁移，任何一步错了应用都是启动失败。
    #[tokio::test]
    async fn connect_and_migrate_rebuilds_a_database_with_stale_history() {
        // 目录名带 pid：同机并发跑（CI 多 job / 本地与 IDE 同时跑）时两个进程会互删
        // 对方的库，表现为随机失败。本仓没有 `tempfile` 依赖，pid 够用。
        let dir = std::env::temp_dir().join(format!(
            "swarmdrop-migration-test-selfheal-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("swarmdrop.db");

        // 先建一个正常的库，再往里塞一条「未来版本」的迁移记录当作存量库。
        let db = connect_and_migrate(&db_path).await.expect("first open");
        db.execute_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO seaql_migrations (version, applied_at) VALUES ('m29990101_000001_x', 0)",
        ))
        .await
        .expect("seed unknown migration");
        db.execute_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO pair_invites (capability_hash, inviter_id, expires_at, state, created_at) \
             VALUES ('deadbeef', 'peer', 1, 'pending', 0)",
        ))
        .await
        .expect("seed row");
        db.close().await.expect("close");

        let db = connect_and_migrate(&db_path).await.expect("自愈后应能打开");

        let invites = db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM pair_invites",
            ))
            .await
            .expect("query")
            .expect("row")
            .try_get_by_index::<i64>(0)
            .expect("count");
        assert_eq!(invites, 0, "库应被重建（旧数据不保留）");

        db.close().await.expect("close");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **古董库回归**：`seaql_migrations` 里只有一行 `"mod"`。
    ///
    /// 那是 v0.3.3 ~ v0.4.2 的真实形态 —— 当时唯一的迁移是
    /// `m20260228_000001_init/mod.rs`，而 `DeriveMigrationName` 取 `file!()` 的 stem，
    /// 记进库里的版本名就是 `"mod"`。
    ///
    /// **这条测试守的是「本迁移不能也叫 `mod`」。** 一旦撞名，两边名字相同 ⇒ pending 空、
    /// missing 也空 ⇒ `Migrator::up` 返回 `Ok(())` ⇒ 自愈永不触发，应用继续跑在 2026-02
    /// 的两表 schema 上，第一次查收件箱就 `no such table: inbox_items`。
    #[tokio::test]
    async fn connect_and_migrate_rebuilds_ancient_database_named_mod() {
        let dir = std::env::temp_dir().join(format!(
            "swarmdrop-migration-test-ancient-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let db_path = dir.join("swarmdrop.db");
        let url = format!("sqlite:{}?mode=rwc", db_path.display());

        // 造一个「只记着 `mod`、但一张本版表都没有」的库。
        let db = sea_orm::Database::connect(&url).await.expect("open");
        Migrator::install(&db).await.expect("install table");
        db.execute_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO seaql_migrations (version, applied_at) VALUES ('mod', 0)",
        ))
        .await
        .expect("seed ancient row");
        db.close().await.expect("close");

        let db = connect_and_migrate(&db_path)
            .await
            .expect("必须重建，而不是判成「无待处理迁移」直接放行");

        let tables = db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' \
                 AND name IN ('inbox_search_index','pair_invites','inbox_items')",
            ))
            .await
            .expect("query")
            .expect("row")
            .try_get_by_index::<i64>(0)
            .expect("count");
        assert_eq!(tables, 3, "重建后必须建出本版的表");

        db.close().await.expect("close");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 反向守卫：普通迁移失败**不得**被判成「该删库」。
    #[test]
    fn does_not_treat_other_errors_as_stale_history() {
        assert!(!is_stale_migration_history(&DbErr::Custom(
            "disk I/O error".to_string()
        )));
        assert!(!is_stale_migration_history(&DbErr::Custom(
            "no such column: foo".to_string()
        )));
    }
}
