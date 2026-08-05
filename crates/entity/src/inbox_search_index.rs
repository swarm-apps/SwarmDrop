use sea_orm::entity::prelude::*;

/// 收件箱检索索引：每条 `inbox_items` 一行的预聚合文本。
///
/// **没有 `title` 列，这是刻意的。** `inbox_items.title` 是首个文件名，而它必然是
/// `files_text`（全部文件的 `name` + `relative_path` 拼接）的子串 —— 两列做同一个
/// `LIKE '%needle%'`，前者能命中的后者一个不落。见
/// `m20260807_000001_drop_search_index_title` 的模块文档。
///
/// **曾是 FTS5 虚表 `inbox_fts`，现在是普通表。** 检索从来没用过 FTS5 的 `MATCH` / bm25
/// —— trigram 分词器对 <3 字的查询无法命中（「合同」这类 2 字中文词会返回空），所以命中
/// 判据一直是对下面四个文本列做 `LIKE` 子串匹配（规范定义在
/// `swarmdrop_transfer::inbox::inbox_matches`，Web 端直接调它）。既然虚表的检索能力一个
/// 都没用上，它就只是一张预聚合文本表；改回普通表之后建表得以全部走 sea-orm schema
/// builder，`CREATE VIRTUAL TABLE ... USING fts5(...)` 是迁移里最后一处无法用 DSL 表达的
/// 语句，它一走迁移就零裸 SQL 了。
///
/// **代价是失去 trigram 对 `LIKE '%xxx%'` 的索引加速**，≥3 字的查询退化为全表扫描。
/// 规模上站得住：收件箱条目数与用户实际接收次数同阶（几十到几百），线性扫描比一次
/// IndexedDB 往返还便宜 —— Web 端本来就是纯内存线性扫描。
///
/// **顺带补上一条虚表拿不到的完整性**：虚表不支持外键，索引行的生命周期只能靠调用方自觉；
/// 现在 `item_id` 是指向 `inbox_items.id` 的外键且 `ON DELETE CASCADE`，条目行一旦真被删除
/// 索引行随之消失。当前删除是软删（写 `deleted_at`，查询侧过滤），所以这条约束今天不会
/// 被触发 —— 它是为「将来真要硬删」准备的，不是在修一个现存的泄漏。
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "inbox_search_index")]
pub struct Model {
    /// 与 `inbox_items.id` 一一对应，既是主键也是外键。
    #[sea_orm(primary_key, auto_increment = false)]
    pub item_id: Uuid,
    #[sea_orm(belongs_to, from = "item_id", to = "id", on_delete = "Cascade")]
    pub item: HasOne<super::inbox_item::Entity>,
    pub source_name: String,
    /// 该条目全部文件的 `name` + `relative_path` 聚合文本（空格分隔）。
    pub files_text: String,
    /// 预留给未来的 OCR / 文本抽取，当前恒为空串。
    ///
    /// **不是 `Option<String>`**：空串是「抽过、没抽到」，而这一端根本没有抽取能力 ——
    /// 两者的区别在 `inbox_matches` 的签名上（它收 `Option<&str>`）已经表达过一次，
    /// 存储层再表达一遍只会让「NULL 还是空串」变成又一处要对齐的约定。
    pub extracted_text: String,
}

impl ActiveModelBehavior for ActiveModel {}
