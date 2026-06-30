# SeaORM 2.0 关系定义

## 目录

- [一对一 (One-to-One)](#一对一)
- [一对多 (One-to-Many)](#一对多)
- [多对多 (Many-to-Many)](#多对多)
- [自引用关系 (Self-Referencing)](#自引用关系)
- [菱形关系 (Diamond Relations)](#菱形关系)
- [Linked 链式关系](#linked-链式关系)
- [复合外键](#复合外键)
- [关系注解速查](#关系注解速查)

## 一对一

**经验法则**：在持有 `xxx_id` 外键的 Entity 上定义 `belongs_to`。

正向（拥有方）：

```rust
// cake.rs
#[sea_orm(has_one)]
pub fruit: HasOne<super::fruit::Entity>,
```

反向（外键方，`unique` 保证 1-1）：

```rust
// fruit.rs
#[sea_orm(unique)]
pub cake_id: Option<i32>,
#[sea_orm(belongs_to, from = "cake_id", to = "id")]
pub cake: HasOne<super::cake::Entity>,
```

**关键点**：1-1 和 1-N 的唯一区别是外键字段是否有 `unique` 约束。

## 一对多

正向（父方）：

```rust
// cake.rs
#[sea_orm(has_many)]
pub fruits: HasMany<super::fruit::Entity>,
```

反向（子方，无 `unique`）：

```rust
// fruit.rs
pub cake_id: Option<i32>,  // 没有 unique
#[sea_orm(belongs_to, from = "cake_id", to = "id")]
pub cake: HasOne<super::cake::Entity>,
```

## 多对多

SeaORM 的特色 — M-N 关系是第一等公民，查询时无需提及 junction 表。

### 主表

用 `has_many` + `via` 指定 junction 表：

```rust
// cake.rs
#[sea_orm(has_many, via = "cake_filling")]
pub fillings: HasMany<super::filling::Entity>,

// filling.rs（双向定义）
#[sea_orm(has_many, via = "cake_filling")]
pub cakes: HasMany<super::cake::Entity>,
```

### Junction 表

双向 `belongs_to`，复合主键：

```rust
// cake_filling.rs
#[sea_orm::model]
#[derive(DeriveEntityModel, ..)]
#[sea_orm(table_name = "cake_filling")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub cake_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub filling_id: i32,
    #[sea_orm(belongs_to, from = "cake_id", to = "id")]
    pub cake: Option<super::cake::Entity>,
    #[sea_orm(belongs_to, from = "filling_id", to = "id")]
    pub filling: Option<super::filling::Entity>,
}
```

**注意**：junction 表的 belongs_to 字段类型用 `Option<Entity>`（而非 `HasOne<Entity>`）。

### 也支持代理主键

```rust
// film_actor.rs（使用普通主键 + 唯一约束）
#[sea_orm(primary_key)]
pub id: i32,
#[sea_orm(unique_key = "film_actor")]
pub film_id: i32,
#[sea_orm(unique_key = "film_actor")]
pub actor_id: i32,
```

## 自引用关系

### BelongsTo 自引用（1-1 / 1-N）

```rust
// staff.rs
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub reports_to_id: Option<i32>,
    #[sea_orm(
        self_ref,
        relation_enum = "ReportsTo",
        relation_reverse = "Manages",
        from = "reports_to_id",
        to = "id"
    )]
    pub reports_to: HasOne<Entity>,
    #[sea_orm(self_ref, relation_enum = "Manages", relation_reverse = "ReportsTo")]
    pub manages: HasMany<Entity>,
}
```

### M-N 自引用（via junction 表）

```rust
// user.rs — followers/following
#[sea_orm(self_ref, via = "user_follower", from = "User", to = "Follower")]
pub followers: HasMany<Entity>,
#[sea_orm(self_ref, via = "user_follower", reverse)]
pub following: HasMany<Entity>,
```

junction 表：

```rust
// user_follower.rs
#[sea_orm(primary_key)]
pub user_id: i32,
#[sea_orm(primary_key)]
pub follower_id: i32,
#[sea_orm(belongs_to, from = "user_id", to = "id")]
pub user: Option<super::user::Entity>,
#[sea_orm(belongs_to, relation_enum = "Follower", from = "follower_id", to = "id")]
pub follower: Option<super::user::Entity>,
```

## 菱形关系

同一 Entity 有多个关系指向同一目标时，用 `relation_enum` + `via_rel` 区分：

```rust
// bakery.rs — manager 和 cashier 都指向 worker
#[sea_orm(belongs_to, relation_enum = "Manager", from = "manager_id", to = "id")]
pub manager: HasOne<super::worker::Entity>,
#[sea_orm(belongs_to, relation_enum = "Cashier", from = "cashier_id", to = "id")]
pub cashier: HasOne<super::worker::Entity>,

// worker.rs — 反向定义
#[sea_orm(has_many, relation_enum = "BakeryManager", via_rel = "Manager")]
pub manager_of: HasMany<super::bakery::Entity>,
#[sea_orm(has_many, relation_enum = "BakeryCashier", via_rel = "Cashier")]
pub cashier_of: HasMany<super::bakery::Entity>,
```

**注意**：多关系指向同一 Entity 时，`Related` trait 不会自动生成，需手动实现或使用 Linked。

## Linked 链式关系

当两个 Entity 间存在多条路径，无法用 `Related` 表达时：

```rust
pub struct CakeToFilling;
impl Linked for CakeToFilling {
    type FromEntity = cake::Entity;
    type ToEntity = filling::Entity;
    fn link(&self) -> Vec<RelationDef> {
        vec![
            cake_filling::Relation::Cake.def().rev(),
            cake_filling::Relation::Filling.def(),
        ]
    }
}

// 懒加载
cake_model.find_linked(CakeToFilling).all(db).await?

// 饿加载
cake::Entity::find().find_also_linked(CakeToFilling).all(db).await?
```

## 复合外键

```rust
// composite_a.rs
#[sea_orm(unique_key = "pair")]
pub left_id: i32,
#[sea_orm(unique_key = "pair")]
pub right_id: i32,

// composite_b.rs
#[sea_orm(belongs_to, from = "(left_id, right_id)", to = "(left_id, right_id)")]
pub a: Option<super::composite_a::Entity>,
```

## 外键 ON UPDATE / ON DELETE

`belongs_to` 支持显式指定外键行为（codegen 默认会生成）：

```rust
#[sea_orm(
    belongs_to,
    from = "cake_id",
    to = "id",
    on_update = "Cascade",
    on_delete = "Cascade"
)]
pub cake: HasOne<super::cake::Entity>,
```

取值：`Cascade`、`Restrict`、`NoAction`、`SetNull`、`SetDefault`。SQLite 不支持后加外键，但定义在 CREATE TABLE 时仍生效。

## REVERSE 常量（自引用 M-N）

自引用 M-N 关系会自动生成 `Entity::REVERSE` 常量，用于在 Entity Loader 中加载反向：

```rust
let user = user::Entity::load()
    .filter_by_email("alice@rust-lang.org")
    .with(profile::Entity)
    .with(user_follower::Entity)            // followers（正向）
    .with(user_follower::Entity::REVERSE)   // following（反向）
    .one(db).await?;
```

## 关系生成的便捷方法

`#[sea_orm::model]` 根据关系字段名自动生成 builder 方法：

```rust
// has_one / belongs_to → set_xxx(...)
.set_profile(profile::ActiveModel::builder().set_picture("Tennis"))
.set_user(user::ActiveModel::builder().set_name("Alice"))

// has_many → add_xxx(...) / push(...)
.add_post(post::ActiveModel::builder().set_title("..."))
.add_tag(tag::ActiveModel::builder().set_tag("sunny"))

// 自引用 M-N → add_follower / add_following
alice.add_follower(bob).save(db).await?;
sam.add_following(alice).save(db).await?;
```

## 关系注解速查

| 场景 | 注解 |
|------|------|
| 一对一正向 | `#[sea_orm(has_one)]` |
| 一对多正向 | `#[sea_orm(has_many)]` |
| 多对多 | `#[sea_orm(has_many, via = "junction_table")]` |
| 反向（外键方） | `#[sea_orm(belongs_to, from = "fk", to = "pk")]` |
| 反向+级联 | `belongs_to, ..., on_update = "Cascade", on_delete = "Cascade"` |
| 自引用 | `#[sea_orm(self_ref, relation_enum = "...", from = "fk", to = "pk")]` |
| 自引用反转 | `#[sea_orm(self_ref, via = "junction", reverse)]` |
| 菱形/多路径 | `relation_enum = "..."` + `via_rel = "..."` |
| 复合外键 | `from = "(col1, col2)"`, `to = "(col1, col2)"` |
