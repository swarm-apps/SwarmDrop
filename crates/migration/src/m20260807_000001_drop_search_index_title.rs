//! 删掉 `inbox_search_index.title` —— 一列**完全冗余**的检索文本。
//!
//! ## 为什么它是冗余的
//!
//! `m20260806_000001_inbox_title_to_file_name` 之后，`title` 存的是**首个文件名**，
//! 而同表的 `files_text` 是「该条目全部文件的 `name` + `relative_path` 空格拼接」——
//! 首文件名必然是 `files_text` 的一个子串。两列做的是同一个 `LIKE '%needle%'`，
//! `title` 能命中的查询 `files_text` 一个不落地也能命中。
//!
//! 这在 title 还是「报告.pdf 等 3 个文件」的年代不成立（那时它多出一截 `files_text`
//! 没有的散文），但那一截恰恰是**噪音**：所有多文件条目都含「个文件」，搜「文件」会
//! 命中全部。上一条迁移把散文换成文件名，噪音随之消失，剩下的就只是冗余了。
//!
//! ## 为什么值得单独一条迁移
//!
//! 留着它不会让检索出错，只是每次写入多存一份首文件名、每次查询多一个 `OR ... LIKE`。
//! 真正的代价在**将来**：两列的更新点已经分开（写入侧一处、`m20260806` 回填一处），
//! 任何一处漏改就会让「按标题搜」与「列表显示」指向不同的值——而这种不一致没有任何
//! 测试能自然发现，因为两列在正常路径上恒等。删掉冗余源是唯一不需要持续维护的做法。
//!
//! ## down 为什么不回填
//!
//! 加回来的是空列。既然 `files_text` 覆盖了同一份文本，空 `title` 与回填过的 `title`
//! 对检索结果**逐条等价**——回填只是把冗余重新写实一遍。

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260807_000001_drop_search_index_title"
    }
}

/// 自己定义 Iden 而不复用 `m20260805_000001_init` 的冻结快照：本迁移**改的正是**
/// 那张表的结构，复用等于让快照同时扮演「建表时的样子」与「改表后的样子」。
/// init 的注释里已经写死了这条界限。
#[derive(DeriveIden)]
enum InboxSearchIndex {
    Table,
    Title,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(InboxSearchIndex::Table)
                    .drop_column(InboxSearchIndex::Title)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(InboxSearchIndex::Table)
                    .add_column(
                        ColumnDef::new(InboxSearchIndex::Title)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, Statement};
    use sea_orm_migration::MigratorTrait;

    use crate::Migrator;

    /// up / down 往返，两半各钉一件事。
    ///
    /// **up 那半钉的是「捆绑的 SQLite 支持 `DROP COLUMN`」**：它要到 3.35（2021）才有，
    /// 而本仓此前所有删列都是「建新表 → 拷数据 → 删旧表 → 改名」的手工重建。
    /// 不支持就在这里红，而不是在用户升级时红。
    ///
    /// **down 那半钉的是回滚真能跑通**：`MigrationTrait::down` 有默认实现（兄弟迁移
    /// `m20260806` 的 down 就是空的），所以「写了 down」不等于「down 是对的」。
    #[tokio::test]
    async fn up_drops_title_and_down_adds_it_back() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        assert!(
            !has_column(&db, "inbox_search_index", "title").await,
            "up 之后 title 列应当已被删除"
        );

        // 末尾新增文本账本迁移后，回滚到本迁移之前需要撤两步：先卸文本账本，再恢复 title。
        Migrator::down(&db, Some(2)).await.unwrap();
        assert!(
            has_column(&db, "inbox_search_index", "title").await,
            "down 之后 title 列应当被加回"
        );
    }

    async fn has_column(db: &sea_orm::DatabaseConnection, table: &str, column: &str) -> bool {
        db.query_all_raw(Statement::from_string(
            db.get_database_backend(),
            format!("PRAGMA table_info({table})"),
        ))
        .await
        .unwrap()
        .iter()
        .any(|row| row.try_get::<String>("", "name").unwrap() == column)
    }
}
