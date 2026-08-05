//! 回填：`inbox_items.title` 从「渲染好的标题」改为「首个文件名」。
//!
//! ## 为什么必须回填
//!
//! 旧写入侧存的是 `"报告.pdf 等 3 个文件"` —— 一句**已经渲染好的中文**。新的渲染在前端
//! 完成（`title` + `item_count` → 「X 等 N 个文件」），于是存量行会被显示成
//! 「报告.pdf 等 3 个文件 **等 3 个文件**」。
//!
//! 不回填的话唯一的出路是让前端**识别旧格式**再决定要不要拼后缀 —— 那等于为了消掉一处
//! 自由文本匹配，又引入一处自由文本匹配（而且这次要匹配的是六种语言里的「等 N 个文件」）。
//!
//! 与失败原因那条通道的处置刚好相反（那边保留 `FailureCode::Legacy` 不回填），区别是
//! **标题能重算、失败原因不能**：文件行还在库里，首个文件名是确定的；而失败原因的原始
//! 错误早已不存在，猜不出来。
//!
//! ## 为什么用 ORM 循环而不是一条 UPDATE
//!
//! 相关子查询（`SET title = (SELECT name FROM inbox_item_files WHERE … LIMIT 1)`）在
//! sea-query 里能写，但要手工拼 `SubQuery` + 外层列引用，可读性远不如直接迭代；而收件箱
//! 条目数与用户实际接收次数同阶（几十到几百，见 `entity::inbox_search_index` 的文档），
//! 一次全表遍历的代价可以忽略。**本迁移同样零 `execute_unprepared`。**

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};

// 复用 init 的冻结快照：两个迁移之间没有 DDL，schema 逐字相同。自己再冻一份只是多一处
// 要同步的副本。**改了这三张表结构的迁移不能再这么做** —— 那时单独冻结。
use crate::m20260805_000001_init::entity;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260806_000001_inbox_title_to_file_name"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        for item in entity::inbox_item::Entity::find().all(db).await? {
            // 首行 = 插入顺序的第一行。写入侧的 `file_rows` 就是按这个顺序建的，
            // 所以 `ORDER BY id` 与 `inbox_primary_file_name(&facts)` 取到同一个。
            let first = entity::inbox_item_file::Entity::find()
                .filter(entity::inbox_item_file::Column::InboxItemId.eq(item.id))
                .order_by_asc(entity::inbox_item_file::Column::Id)
                .one(db)
                .await?;
            // 没有文件行的条目（历史上的「空传输」）回填成空串 —— 前端按 item_count == 0
            // 出「空传输」那句文案。
            let title = first.map(|file| file.name).unwrap_or_default();

            if title != item.title {
                let mut active: entity::inbox_item::ActiveModel = item.clone().into();
                active.title = Set(title.clone());
                active.update(db).await?;
            }

            // 检索索引的 title 列是同一个值的副本，一起改；漏掉它会让「按标题搜」
            // 在存量条目上继续命中那句渲染好的中文，而列表显示的已经是新值。
            if let Some(index) = entity::inbox_search_index::Entity::find_by_id(item.id)
                .one(db)
                .await?
                && index.title != title
            {
                let mut active: entity::inbox_search_index::ActiveModel = index.into();
                active.title = Set(title);
                active.update(db).await?;
            }
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // 不可逆：旧值是渲染结果，而渲染依赖的语言在这里无从知晓。
        Ok(())
    }
}
