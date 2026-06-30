# SeaORM 2.0 查询方式

## 目录

- [Entity Loader（推荐）](#entity-loader)
- [Model Loader](#model-loader)
- [Nested Select（嵌套查询）](#nested-select)
- [Multi Select（多表联查）](#multi-select)
- [Relational Query（关系过滤）](#relational-query)
- [查询方式选择指南](#查询方式选择指南)

## Entity Loader

2.0 新增，智能选择 JOIN / data loader，自动消除 N+1 问题。
仅适用于 `#[sea_orm::model]` 或 `#[sea_orm::compact_model]` 定义的 Entity。

### 基本用法

```rust
let user = user::Entity::load()
    .filter_by_id(12)
    .with(profile::Entity)           // 1-1 用 JOIN
    .with(post::Entity)              // 1-N 用 data loader
    .one(db).await?;
```

### 嵌套加载

```rust
// join 路径：user -> profile, user -> post -> comment
let user = user::Entity::load()
    .filter_by_id(12)
    .with(profile::Entity)
    .with((post::Entity, comment::Entity))  // 嵌套元组
    .one(db).await?;
// 执行 3 条 SQL：
// 1. SELECT user JOIN profile WHERE id = 12
// 2. SELECT post WHERE user_id IN (12)
// 3. SELECT comment WHERE post_id IN (..)
```

### 分页

```rust
let paginator = user::Entity::load()
    .with(profile::Entity)
    .order_by_asc(user::COLUMN.id)
    .paginate(db, 10);
let users: Vec<user::ModelEx> = paginator.fetch().await?;
```

### 自引用关系

```rust
let staff = staff::Entity::load()
    .with(staff::Relation::ReportsTo)
    .with(staff::Relation::Manages)
    .all(db).await?;
```

### M-N 自引用 + 嵌套

```rust
let users = user::Entity::load()
    .with(profile::Entity)
    .with((user_follower::Entity, profile::Entity))           // followers
    .with((user_follower::Entity::REVERSE, profile::Entity))  // following
    .all(db).await?;
```

### M-N 自引用：单层加载（无 profile 嵌套）

```rust
let alice = user::Entity::load()
    .filter_by_email("alice@rust-lang.org")
    .with(profile::Entity)
    .with(user_follower::Entity)            // followers
    .with(user_follower::Entity::REVERSE)   // following
    .one(db).await?
    .unwrap();
assert_eq!(alice.followers.len(), 2);
```

### filter_by_xxx（唯一键快捷查询）

`#[sea_orm::model]` 对 `#[sea_orm(unique)]` 的字段自动生成 `filter_by_xxx`：

```rust
user::Entity::load()
    .filter_by_email("bob@sea-ql.org")   // 由 unique email 自动生成
    .with(profile::Entity)
    .one(db).await?;

user::Entity::load()
    .filter_by_id(42)                    // 由主键自动生成
    .one(db).await?;
```

### 内部机制

- 1-1 关系：使用 LEFT JOIN（单条 SQL）
- 1-N / M-N 关系：使用 data loader（`WHERE id IN (..)`）
- 嵌套关系：逐层 data loader，合并 id 批量查询

## Model Loader

传统方式，基于 `LoaderTrait`，支持过滤条件。

```rust
let cakes: Vec<cake::Model> = Cake::find().all(db).await?;
let fruits: Vec<Vec<fruit::Model>> = cakes.load_many(Fruit, db).await?;
let fillings: Vec<Vec<filling::Model>> = cakes.load_many(Filling, db).await?;
```

### 带过滤条件

```rust
let fruits_in_stock = cakes.load_many(
    fruit::Entity::find().filter(fruit::Column::Stock.gt(0)),
    db
).await?;
```

### 自引用

```rust
let staff = staff::Entity::find().all(db).await?;
let reports_to = staff.load_self(staff::Entity, staff::Relation::ReportsTo, db).await?;
let manages = staff.load_self_many(staff::Entity, staff::Relation::Manages, db).await?;
```

### M-N 自引用

```rust
let users = user::Entity::find().all(db).await?;
let followers = users.load_self_via(user_follower::Entity, db).await?;
let following = users.load_self_via_rev(user_follower::Entity, db).await?;
```

## Nested Select

使用 `DerivePartialModel` + `#[sea_orm(nested)]` 自定义结果形状。

### 基本嵌套

```rust
#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity")]
struct Cake {
    id: i32,
    name: String,
    #[sea_orm(nested)]
    bakery: Option<Bakery>,
}

#[derive(DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity")]
struct Bakery {
    id: i32,
    #[sea_orm(from_col = "name")]
    brand: String,
}

let cake: Cake = cake::Entity::find()
    .left_join(bakery::Entity)
    .into_partial_model()
    .one(db).await?.unwrap();
```

### 嵌套完整 Model

```rust
#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity")]
struct Cake {
    id: i32,
    #[sea_orm(nested)]
    bakery: Option<bakery::Model>,  // 完整 Model 也可以嵌套
}
```

### 三表 JOIN

```rust
// Order -> Customer, Order -> LineItem -> Cake
#[derive(DerivePartialModel)]
#[sea_orm(entity = "order::Entity")]
struct Order {
    id: i32,
    #[sea_orm(nested)]
    customer: Customer,
    #[sea_orm(nested)]
    line: LineItem,  // LineItem 内部再嵌套 Cake
}

let items = order::Entity::find()
    .left_join(customer::Entity)
    .left_join(lineitem::Entity)
    .join(JoinType::LeftJoin, lineitem::Relation::Cake.def())
    .into_partial_model()
    .all(db).await?;
```

### 别名（同表多次 JOIN）

```rust
#[derive(DerivePartialModel)]
#[sea_orm(entity = "bakery::Entity")]
struct Bakery {
    name: String,
    #[sea_orm(nested, alias = "manager")]
    manager: Worker,
    #[sea_orm(nested, alias = "cashier")]
    cashier: Worker,
}

bakery::Entity::find()
    .join_as(JoinType::LeftJoin, bakery::Relation::Manager.def(), "manager")
    .join_as(JoinType::LeftJoin, bakery::Relation::Cashier.def(), "cashier")
    .into_partial_model()
    .all(db).await?
```

## Multi Select

`find_also_related` 系列，最多支持 6 表联查。

### 基本用法

```rust
let items: Vec<(order::Model, Option<lineitem::Model>, Option<cake::Model>)> =
    order::Entity::find()
        .find_also_related(lineitem::Entity)
        .and_also_related(cake::Entity)
        .all(db).await?;
```

### consolidate() — 扁平 → 嵌套

```rust
// Chain 拓扑: Order -> LineItem -> Cake
let items: Vec<(order::Model, Vec<(lineitem::Model, Vec<cake::Model>)>)> =
    order::Entity::find()
        .find_also_related(lineitem::Entity)
        .and_also_related(cake::Entity)
        .consolidate()
        .all(db).await?;

// Star 拓扑: Order -> Customer, Order -> LineItem
let items: Vec<(order::Model, Vec<customer::Model>, Vec<lineitem::Model>)> =
    order::Entity::find()
        .find_also_related(customer::Entity)
        .find_also_related(lineitem::Entity)
        .consolidate()
        .all(db).await?;
```

### 6 表联查

```rust
let (one, two, three, four, five, six) = one::Entity::find()
    .find_also(one::Entity, two::Entity)
    .find_also(two::Entity, three::Entity)
    .find_also(three::Entity, four::Entity)
    .find_also(four::Entity, five::Entity)
    .find_also(one::Entity, six::Entity)
    .one(db).await?.unwrap();
```

## Relational Query

使用 `has_related` 进行关系过滤（WHERE EXISTS 子查询）：

```rust
// 查找 SeaSide Bakery 的所有 cake
let cakes = cake::Entity::find()
    .has_related(bakery::Entity, bakery::Column::Name.eq("SeaSide Bakery"))
    .all(db).await?;
// SQL: SELECT cake WHERE EXISTS(SELECT 1 FROM bakery WHERE name = 'SeaSide' AND cake.bakery_id = bakery.id)

// M-N 同样适用
let cakes = cake::Entity::find()
    .has_related(baker::Entity, baker::Column::Name.eq("Alice"))
    .all(db).await?;
```

## 查询方式选择指南

| 场景 | 推荐方式 |
|------|----------|
| 加载 Model + 所有关联关系 | Entity Loader |
| 批量加载 + 自定义过滤 | Model Loader |
| 自定义结果形状 / 部分字段 | Nested Select (DerivePartialModel) |
| 多表联查 + 结果聚合 | Multi Select + consolidate() |
| 按关联 Entity 属性过滤 | has_related (WHERE EXISTS) |
| 跨多级关系查询 | Linked |
