# SeaORM 2.0 新 Entity 格式

## 目录

- [新格式概览](#新格式概览)
- [新旧格式对比](#新旧格式对比)
- [Model 宏](#model-宏)
- [compact_model 过渡宏](#compact_model-过渡宏)
- [COLUMN 常量](#column-常量)
- [find_by / delete_by 快捷方法](#快捷方法)
- [ActiveModelBehavior](#activemodelBehavior)
- [codegen 命令](#codegen-命令)

## 新格式概览

SeaORM 2.0 引入 `#[sea_orm::model]` 属性宏，将关系定义内联到 Model 字段，宏自动展开生成 `Relation` 枚举和 `Related` impl。

```rust
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub email: String,
    #[sea_orm(has_one)]
    pub profile: HasOne<super::profile::Entity>,
    #[sea_orm(has_many)]
    pub posts: HasMany<super::post::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
```

宏展开后自动生成：
- `Relation` 枚举（`Profile`, `Post` 变体）
- `Related<profile::Entity>` 和 `Related<post::Entity>` impl
- `ModelEx` 类型（含嵌套关系字段）
- `ActiveModelEx` 类型
- `COLUMN` 强类型常量
- `find_by_email()` 等唯一键快捷方法

## 新旧格式对比

### 1.0 展开格式（Expanded）

```rust
// model.rs
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    #[sea_orm(unique)]
    pub email: String,
}

// relation 和 related 需要手写
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::profile::Entity")]
    Profile,
    #[sea_orm(has_many = "super::post::Entity")]
    Post,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef { Relation::Profile.def() }
}
impl Related<super::post::Entity> for Entity {
    fn to() -> RelationDef { Relation::Post.def() }
}

impl ActiveModelBehavior for ActiveModel {}
```

### 2.0 新格式（Dense）

参见上方"新格式概览"，一个 `#[sea_orm::model]` 宏搞定全部。

## Model 宏

### Entity 级注解（写在 `Model` 结构体上）

| 注解 | 说明 |
|------|------|
| `#[sea_orm(table_name = "user")]` | 表名 |
| `#[sea_orm(schema_name = "public")]` | 数据库 schema 名（PG / SQL Server） |
| `#[sea_orm(rename_all = "camelCase")]` | 批量列名重命名规则。可选：`camelCase`、`kebab-case`、`PascalCase`、`snake_case`、`SCREAMING_SNAKE_CASE`、`lowercase`、`UPPERCASE` |

### 字段级注解

| 注解 | 说明 |
|------|------|
| `#[sea_orm(primary_key)]` | 主键，默认自增 |
| `#[sea_orm(primary_key, auto_increment = false)]` | 主键，不自增（复合主键也用此） |
| `#[sea_orm(unique)]` | 唯一约束 |
| `#[sea_orm(unique_key = "pair")]` | 复合唯一约束，同名的字段构成一组 |
| `#[sea_orm(indexed)]` | 非唯一索引 |
| `#[sea_orm(nullable)]` | 显式可空（`Option<T>` 字段自动推断） |
| `#[sea_orm(column_name = "xxx")]` | 自定义列名 |
| `#[sea_orm(column_type = "Text")]` | 指定列类型，复杂类型如 `"Decimal(Some((16, 4)))"` |
| `#[sea_orm(default_value = 0)]` | 默认值（Entity First 建表时使用） |
| `#[sea_orm(default_expr = "Expr::current_timestamp()")]` | 默认表达式 |
| `#[sea_orm(renamed_from = "old_name")]` | 从旧列名重命名（Schema Sync 检测到则发 `ALTER TABLE RENAME`） |
| `#[sea_orm(select_as = "TEXT")]` | SELECT 时强制类型转换 |
| `#[sea_orm(save_as = "JSONB")]` | INSERT/UPDATE 时强制类型转换 |
| `#[sea_orm(ignore)]` | 该字段不映射到数据库列 |

### 关系字段注解

| 注解 | 说明 |
|------|------|
| `#[sea_orm(has_one)]` | 一对一正向关系 |
| `#[sea_orm(has_many)]` | 一对多正向关系 |
| `#[sea_orm(has_many, via = "junction_table")]` | 多对多关系，指定中间表 |
| `#[sea_orm(belongs_to, from = "fk_col", to = "pk_col")]` | 反向关系（外键方） |
| `#[sea_orm(belongs_to, ..., on_update = "Cascade")]` | 外键的 ON UPDATE 行为 |
| `#[sea_orm(belongs_to, ..., on_delete = "Cascade")]` | 外键的 ON DELETE 行为 |
| `#[sea_orm(self_ref, relation_enum = "...", from = "fk", to = "pk")]` | 自引用关系 |
| `#[sea_orm(self_ref, via = "junction", reverse)]` | 自引用 M-N 反向 |
| 菱形/多路径 | `relation_enum = "..."` + `via_rel = "..."` |

`on_update` / `on_delete` 取值：`Cascade`、`Restrict`、`NoAction`、`SetNull`、`SetDefault`。

### 复合主键限制

最多支持 **12** 个字段作为复合主键。超出需自己设计代理主键 + `unique_key` 复合唯一约束。

### junction 表两种写法

```rust
// 写法 A（quickstart 中用）—— 推荐手写
pub cake: Option<super::cake::Entity>,

// 写法 B（codegen 生成）—— 启用 cascade 行为时清晰
#[sea_orm(belongs_to, from = "cake_id", to = "id",
          on_update = "Cascade", on_delete = "Cascade")]
pub cake: HasOne<super::cake::Entity>,
```

两者等价，`Option<E>` 适合简洁手写，`HasOne<E>` 适合显式标注外键行为。

## compact_model 过渡宏

如果已有 1.0 格式的 Entity，不想完全迁移，可用 `#[sea_orm::compact_model]` 过渡。
只需在 Model 上加关系字段，保留原有的 `Relation` 枚举和 `Related` impl：

```rust
#[sea_orm::compact_model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "post")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub body: String,
    pub author: HasOne<super::user::Entity>,  // 新增关系字段
}

// 保留原有的 Relation 枚举
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(belongs_to = "super::user::Entity", from = "Column::UserId", to = "super::user::Column::Id")]
    Author,
}
```

这样就能使用 Entity Loader：

```rust
post::Entity::load().with(super::user::Entity)..
```

## COLUMN 常量

2.0 新增强类型列常量，替代 CamelCase 枚举：

```rust
// 旧：枚举变体
user::Entity::find().filter(user::Column::Name.contains("Bob"))

// 新：强类型常量
user::Entity::find().filter(user::COLUMN.name.contains("Bob"))

// 编译期类型检查 — 下面会报错
user::Entity::find().filter(user::COLUMN.name.like(2))
// error: the trait `From<{integer}>` is not implemented for `String`
```

类型列表：`BoolColumn`, `NumericColumn`, `StringColumn`, `BytesColumn`, `JsonColumn`, `DateLikeColumn`, `TimeLikeColumn`, `DateTimeLikeColumn`, `UuidColumn`, `ArrayColumn` 等。

每种类型有专属方法：
- `StringColumn`: `contains()`, `like()`, `starts_with()`, `ends_with()`
- `ArrayColumn`: `contains()` — Postgres `@>` 操作符
- `NumericColumn`: `between()` 等

## 快捷方法

对有 `#[sea_orm(unique)]` 的字段，自动生成快捷方法：

```rust
// find_by_{field_name}
user::Entity::find_by_email("bob@sea-ql.org").one(db).await?

// delete_by_{field_name} — 返回 DeleteOne 而非 DeleteMany
user::Entity::delete_by_email("bob@spam.com").exec(db).await?

// Entity Loader 也支持
user::Entity::load().filter_by_email("bob@sea-ql.org").one(db).await?

// 复合唯一键也支持
composite_a::Entity::find_by_pair((1, 2)).one(db).await?
```

## ActiveModelBehavior

可以在 `ActiveModelBehavior` 中定义钩子：

```rust
#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, db: &C, insert: bool) -> Result<Self, DbErr>
    where C: ConnectionTrait {
        // 保存前校验
        Ok(self)
    }

    async fn after_save<C>(model: Model, db: &C, insert: bool) -> Result<Model, DbErr>
    where C: ConnectionTrait {
        // 保存后触发通知等
        Ok(model)
    }
}
```

## codegen 命令

用 sea-orm-cli 从现有数据库生成新格式 Entity：

```bash
sea-orm-cli generate entity \
  --output-dir ./src/entity \
  --entity-format dense          # ← 使用新格式
```
