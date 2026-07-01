# SeaORM 2.0 其他新特性

涵盖 SKILL 主线之外、但仍然重要的新增功能：`raw_sql!` 宏、RBAC、多列唯一键、自定义包装主键、灵活 JSON 反序列化、错误处理改进、新数据库后端等。

## 目录

- [raw\_sql! 宏](#raw_sql-宏)
- [RBAC 与 RestrictedConnection](#rbac-与-restrictedconnection)
- [多列唯一键 (Multi-part Unique Key)](#多列唯一键)
- [包装类型作主键 (Wrapper PK)](#包装类型作主键)
- [灵活 JSON 反序列化](#灵活-json-反序列化)
- [改进的错误处理](#改进的错误处理)
- [新增数据库/数据格式支持](#新增数据库数据格式支持)
- [PartialModel → ActiveModel](#partialmodel--activemodel)
- [非均匀批量插入 (Non-uniform Insert)](#非均匀批量插入)
- [Seaography 2.0 / SeaORM Pro 2.0](#seaography-20--sea-orm-pro-20)

## raw_sql! 宏

替代手动拼接 `Statement::from_sql_and_values`，自动处理参数和数组展开。**SQL 注入安全**。

```rust
use sea_orm::{raw_sql, DbBackend, FromQueryResult};

// 1. 找 Entity Model
let cake = cake::Entity::find()
    .from_raw_sql(raw_sql!(
        DbBackend::Postgres,
        "SELECT * FROM cake WHERE id = {id}",
        id = 1_i32
    ))
    .one(db).await?;

// 2. 数组展开 — {..ids} 自动展开成 $1, $2, $3
let ids = vec![1, 2, 3];
let cakes = cake::Entity::find()
    .from_raw_sql(raw_sql!(
        DbBackend::Postgres,
        "SELECT * FROM cake WHERE id IN ({..ids})",
        ids = ids
    ))
    .all(db).await?;

// 3. 自定义结构（DerivePartialModel 或 FromQueryResult）
#[derive(FromQueryResult)]
struct CakeSummary { id: i32, name: String, slices: i64 }

let rows: Vec<CakeSummary> = CakeSummary::find_by_statement(raw_sql!(
    DbBackend::Postgres,
    "SELECT id, name, COUNT(*) AS slices FROM cake GROUP BY id, name"
)).all(db).await?;

// 4. 直接执行 DML/DDL
db.execute_raw(raw_sql!(
    DbBackend::Postgres,
    "UPDATE cake SET name = {name} WHERE id = {id}",
    name = "Pancake", id = 1_i32
)).await?;
```

### 与 execute / query 系列方法的关系

| 方法 | 接受 | 用途 |
|------|------|------|
| `db.execute(query)` | SeaQuery `SelectStatement` 等 | 不再需要手动 `backend.build()` |
| `db.execute_raw(stmt)` | `Statement`（含 raw SQL） | DML / DDL |
| `db.execute_unprepared("...")` | `&str` | `CREATE EXTENSION` 等不可参数化语句 |
| `db.query_one(&query)` / `query_all(&query)` | SeaQuery 语句 | 返回 `QueryResult` |
| `db.query_one_raw(stmt)` / `query_all_raw(stmt)` | raw SQL | 返回 `QueryResult` |

## RBAC 与 RestrictedConnection

内置基于角色的访问控制。`RestrictedConnection` 是一种连接包装类型，**编译期** 阻止越权操作。

### 初始化

```rust
// 1. 创建 RBAC 系统表
sea_orm::rbac::schema::create_tables(db, Default::default()).await?;

// 2. 配置角色和权限
let mut ctx = sea_orm::rbac::RbacContext::load(db).await?;
ctx.add_tables(db, &["baker", "bakery", "cake", "*"]).await?;
ctx.add_crud_permissions(db).await?;
ctx.add_roles(db, &["admin", "manager", "public"]).await?;

// 3. 角色层级（DAG，多继承）
ctx.add_role_hierarchy(db, &[
    sea_orm::rbac::RbacAddRoleHierarchy {
        super_role: "admin",
        role: "manager",
    },
]).await?;

// 4. 给角色发权限（支持通配符 "*"）
ctx.add_role_permissions(db, "public",  &["select"],            &["*"]).await?;
ctx.add_role_permissions(db, "manager", &["insert", "update"],  &["cake"]).await?;

// 5. 分配用户角色（1 用户 ↔ 1 角色）
ctx.assign_user_role(db, &[(1, "admin"), (2, "manager")]).await?;

// 6. 服务启动时加载到内存
db.load_rbac().await?;
```

### 使用

```rust
use sea_orm::rbac::RbacUserId;

let user_id = RbacUserId(1);
let restricted: RestrictedConnection = db.restricted_for(user_id)?;

// 所有查询都会被审计；越权 → DbErr::AccessDenied
async fn create_cake(db: RestrictedConnection) -> Result<(), DbErr> {
    cake::Entity::insert(cake::ActiveModel::default())
        .exec(&db).await?;
    Ok(())
}

// 事务也支持
db.transaction::<_, _, DbErr>(|txn| {
    Box::pin(async move {
        // txn 是 RestrictedTransaction
        Ok(())
    })
}).await?;
```

### 限制

- **DDL 和 raw SQL 不支持** — `RestrictedConnection` 不允许执行
- 权限是累加的（除非用 per-user override 撤销）
- 类型系统阻止把 `RestrictedConnection` 降级为 `DatabaseConnection`

## 多列唯一键

通过同名 `unique_key` 把多列绑成一组唯一约束：

```rust
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "film_actor")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique_key = "film_actor")]
    pub film_id: i32,
    #[sea_orm(unique_key = "film_actor")]
    pub actor_id: i32,
}
```

→ 自动生成 `find_by_film_actor((film_id, actor_id))` 等快捷方法。

## 包装类型作主键

`DeriveValueType` 增强后自动实现 `NotU8`、`IntoActiveValue`、`TryFromU64`，可以做主键：

```rust
#[derive(Clone, Debug, PartialEq, Eq, DeriveValueType)]
pub struct UserId(pub i32);

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: UserId,   // ← 可以了
    pub name: String,
}
```

### UnixTimestamp 类型

`UnixTimestamp` 是内置的包装类型，把 `DateTime` 透明映射成 `i64` 存储：

```rust
pub created_at: UnixTimestamp,    // 数据库存 INTEGER，应用层用 DateTime
```

## 灵活 JSON 反序列化

`ActiveModel::from_json` 容忍字段缺失，缺失字段自动设为 `NotSet`（而非报错）：

```rust
let partial: user::ActiveModel = user::ActiveModel::from_json(
    serde_json::json!({ "name": "Bob" })  // 缺 email 等字段
)?;
// partial.email == NotSet，可以与 DB 中现有记录 merge
```

## 改进的错误处理

新增 `DbErr` 变体，原本 panic 的场景改为返回错误：

| 变体 | 触发场景 |
|------|----------|
| `DbErr::PrimaryKeyNotSet` | save/update 时主键未设置 |
| `DbErr::BackendNotSupported` | 在不支持的数据库后端调用功能 |
| `DbErr::AccessDenied` | `RestrictedConnection` 越权 |

## 新增数据库/数据格式支持

| 后端 | feature flag |
|------|-------------|
| SQL Server | `sqlx-mssql` |
| ClickHouse | `clickhouse` |
| Arrow / Parquet | `with-arrow`、`with-parquet` |
| MariaDB RETURNING | `mariadb-use-returning` |
| PostgreSQL `serial` 旧行为 | `option-postgres-use-serial` |

注意：核心 CRUD 跨后端通用；Arrow / Parquet 主要用于数据科学场景，搭配 `data-science` 文档。

## PartialModel → ActiveModel

`PartialModel` 现可通过 `IntoActiveModel` 转为 `ActiveModel`：

```rust
#[derive(DerivePartialModel)]
#[sea_orm(entity = "user::Entity")]
struct UserPatch {
    id: i32,
    name: String,
}

let patch = UserPatch { id: 1, name: "Bob".into() };
let am: user::ActiveModel = patch.into_active_model();
am.save(db).await?;
```

## 非均匀批量插入

`insert_many` 现支持每行不同的列集合：

```rust
let res = user::Entity::insert_many([
    user::ActiveModel { name: Set("Alice".into()), email: Set("a@x".into()), ..Default::default() },
    user::ActiveModel { name: Set("Bob".into()), ..Default::default() },  // 没设 email
]).exec(db).await?;
// 空集合也安全：返回 InsertManyResult { last_insert_id: None }
```

`exec_with_returning` 现在返回 `Vec<Model>`（`exec_with_returning_many` 已废弃）。

## Seaography 2.0 / SeaORM Pro 2.0

- **Seaography 2.0** — instant GraphQL：Entity 自动映射为 GraphQL 类型，新格式 Entity 无需额外宏
- **SeaORM Pro 2.0** — 官方管理面板，内置 RBAC

启用方式参考 [seaography 文档](https://www.sea-ql.org/SeaORM/zh-CN/docs/graph-ql/seaography-intro/)。

## 版本与生态兼容性

- **Rust** — 2024 edition，MSRV `1.85`
- **async-std** — 已废弃，强制 tokio
- **Loco** 框架 — 官方主线尚未迁移 2.0，需用 SeaQL fork
- 最新稳定版（截至当前文档）：`2.0.0-rc.38`（2026-04），1.1.x 仍维护
