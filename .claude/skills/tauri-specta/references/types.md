# 类型映射

## `#[derive(specta::Type)]` 基础

任何要跨 IPC 边界的自定义类型，**必须** derive `specta::Type`：

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub user_id: u32,
    pub display_name: String,
    pub email: Option<String>,
}
```

生成：

```ts
export type User = {
    userId: number,
    displayName: string,
    email: string | null,
};
```

### 传染性原则

derive 的传染范围很重要：**只要这个类型出现在命令/事件链路中，它内部所有字段类型都必须能被 specta 识别**。

```rust
#[derive(Type)]
pub struct Outer {
    inner: Inner,        // ← Inner 也必须 derive Type
}

#[derive(Type)]
pub struct Inner {
    list: Vec<Item>,     // ← Item 也必须 derive Type
}
```

否则编译报错 `the trait Type is not implemented for ...`。

### `#[serde(rename_all)]` 的影响

specta 会**复用 serde 的 rename 规则**：

| serde 属性 | wire 名 + TS 字段名 |
|-----------|---------------------|
| 默认 | 保持 Rust 原名（`user_id`） |
| `rename_all = "camelCase"` | `userId` |
| `rename_all = "kebab-case"` | `user-id`（TS 字段名带连字符必须加引号：`"user-id": ...`） |
| `rename = "..."`（单字段） | 改单字段名 |

→ 大部分项目推荐全局加 `#[serde(rename_all = "camelCase")]`，让 TS 字段名符合前端习惯。

## 枚举类型

### 单元变体

```rust
#[derive(Type, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Pending,
    Done,
    Failed,
}
```

```ts
export type Status = "pending" | "done" | "failed";
```

### 带数据的变体 + serde tag

```rust
#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum PairingMethod {
    Code { code: String },
    Manual,
    Biometric { token: Vec<u8> },
}
```

```ts
export type PairingMethod =
    | { type: "code"; data: { code: string } }
    | { type: "manual" }
    | { type: "biometric"; data: { token: number[] } };
```

不同的 serde tag 风格：

| serde 属性 | TS 形状 |
|-----------|---------|
| 默认（externally tagged） | `{ "Code": { code: string } }` |
| `tag = "type"` (internally) | `{ type: "code", code: string }` |
| `tag = "type", content = "data"` (adjacently) | `{ type: "code", data: { code: string } }` |
| `untagged` | `{ code: string } \| string` —— 不推荐，前端 narrow 难 |

→ 推荐 `tag + content`，**外部 tag 明确、内部数据集中**，前端 `switch (x.type)` discriminated union 工作得最好。

### `#[serde(transparent)]`

让 wire payload 等于内部字段（常用于 newtype）：

```rust
#[derive(Type, Serialize)]
#[serde(transparent)]
pub struct UserId(pub u32);
```

```ts
export type UserId = number;  // 不是 { 0: number }
```

### `#[serde(flatten)]`

把子结构体的字段摊到当前层：

```rust
#[derive(Type, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Common {
    pub id: u32,
    pub created_at: i64,
}

#[derive(Type, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    #[serde(flatten)]
    pub common: Common,    // id / createdAt 摊在外层
    pub name: String,
}
```

```ts
export type User = {
    id: number,
    createdAt: number,
    name: string,
};
```

## BigInt 处理（关键！）

specta-typescript 默认**禁止**导出以下类型，会编译报错 `BigInt forbidden`：

| Rust | 默认行为 | 原因 |
|------|----------|------|
| `i64` / `u64` | ❌ 报错 | 超过 JS `Number.MAX_SAFE_INTEGER` (2^53) 会丢精度 |
| `i128` / `u128` / `f128` | ❌ 报错 | 同上 |
| `usize` / `isize` | ❌ 报错 | 平台相关，最大 2^64 |

### 方案 1：全局放宽（推荐）

如果你能保证所有 u64/i64 字段都在 JS 安全整数范围内（< 2^53 ≈ 9 PB / 285 年的毫秒数），用：

```rust
SpectaBuilder::<Wry>::new()
    .dangerously_cast_bigints_to_number()    // ← 关键
```

之后 `u64` / `i64` / `usize` 全部映射为 TS `number`：

```ts
// 前端体验回到原生
const size: number = await commands.getFileSize();
const total = size + 1;  // ✅
```

名字里的 `dangerously` 是提醒你：**你需要为自己负责**。文件大小、毫秒时间戳、字节计数基本都安全。

### 方案 2：换更小的整数

如果不需要那么大的范围，直接用 `u32` / `u16` / `f64`：

```rust
pub struct File {
    pub size: u32,                          // ✅ ≤ 4GB
    pub timestamp_secs: u32,                // ✅ 到 2106 年
}
```

### 方案 3：序列化为 string（精度无损）

```rust
pub struct Account {
    #[specta(type = String)]                // 告诉 specta：这个字段当 String
    #[serde(with = "...")]                  // 实际运行时也按 String 编解码
    pub balance_satoshis: i64,
}
```

前端拿到 string，需要时 `BigInt(s)`。

### 方案 4：per-field `Number` 接受精度损失

```rust
pub struct Foo {
    #[specta(type = specta_typescript::Number)]
    pub big: u64,
}
```

只对单字段生效，类似于"显式 unsafe 块"，便于审计。

## 跨 IPC 边界传第三方类型

很多 crate 的类型没有 `specta::Type` impl（libp2p / 外部 SDK / mmap-related types）。**通用解法**：在边界 toString，进函数后 `parse()`。

```rust
#[tauri::command]
#[specta::specta]
async fn connect(peer_id: String) -> Result<(), AppError> {
    let peer_id: PeerId = peer_id.parse()?;
    // 之后用真实的 PeerId
}
```

### 特殊情况：用 `#[specta(type = ...)]` 重映射

如果第三方类型实际是 string-like，可以让它伪装成 String：

```rust
#[derive(Type, Serialize)]
pub struct Foo {
    #[specta(type = String)]                // specta 当 String 看
    #[serde(serialize_with = "...")]         // serde 也写成 string
    pub peer_id: PeerId,
}
```

## specta 自带 feature 覆盖的常用类型

打开对应 feature 后，下列 crate 类型可直接 derive：

| Crate | Feature | 类型 |
|-------|---------|------|
| `uuid` | `specta = { features = ["uuid"] }` | `Uuid` → TS `string` |
| `chrono` | `chrono` | `DateTime<Utc>` → TS `string` (ISO 8601) |
| `serde_json` | `serde_json` | `serde_json::Value` → TS `JsonValue` |
| `url` | `url` | `Url` → TS `string` |
| `bytes` | `bytes` | `Bytes` → TS `number[]` |

不开 feature 直接用会报 `Type not implemented`。

## phase-aware serde（v2 新增）

specta v2 默认**按 serde 阶段拆分类型**——如果 serialize 和 deserialize 形状不一致（最常见原因是 `#[serde(skip_serializing)]` / `rename(serialize, deserialize)` / `default`），会生成两个别名：

```rust
#[derive(Type, Serialize, Deserialize)]
pub struct Foo {
    pub a: String,
    #[serde(default, skip_serializing)]
    pub b: String,
}
```

生成：

```ts
export type Foo_Serialize = { a: string };
export type Foo_Deserialize = { a: string; b: string };
export type Foo = Foo_Serialize | Foo_Deserialize;
```

specta-tauri 在命令边界会自动用正确的相位：

- 参数：`Foo_Deserialize`（从前端传进来要 deserialize）
- 返回值：`Foo_Serialize`（要 serialize 出去）

如果 serialize/deserialize 形状一致，只生成一个 `Foo` 别名。

### 关掉这个行为

如果你的类型 serde 两端一致、不想看到 `_Serialize` / `_Deserialize` 后缀：

```rust
SpectaBuilder::new()
    .disable_serde_phases()
```

之后只生成单一别名。

> 实战经验：**默认开着 phase-aware 更安全**（能挡住 serde skip/default 的不对称坑）。除非你确定全部类型都对称，否则不要 `disable_serde_phases()`。

## semantic types（v2 新增）

启用后，特定类型会得到**运行时形状不同于 JSON 的强映射**：

```rust
use specta_typescript::semantic;

SpectaBuilder::new()
    .semantic_types(semantic::Configuration::default());
```

默认规则：

| Rust | wire (JSON) | TS 运行时类型 |
|------|-------------|---------------|
| `chrono::DateTime<Utc>` | `string` | `Date` |
| `bytes::Bytes` / `Vec<u8>` | `string` (base64) / `number[]` | `Uint8Array` |
| `url::Url` | `string` | `URL` |

前端拿到的就是真正的 `Date` / `Uint8Array` / `URL` 实例（specta-typescript 注入了自动转换逻辑）。

需要时机：

- 想让前端拿到 `Date` 对象做 `.getTime()` / `Intl.DateTimeFormat`
- 想让前端不必手动 `new URL(str)`
- 想让二进制数据走 `Uint8Array` 而不是 `number[]`

不需要时：保持默认即可，前端用 string 处理。

## 跨 crate 类型重名怎么办？

specta v2 类型全局**只认 type name 不认 module path**。如果项目里两个 crate 各有一个同名 type 都进入了 specta 收集表，会报：

```text
Detected multiple types with the same name: "TransferDirection"
```

`#[serde(rename)]` 只影响字段，**不影响 type name**。`#[specta(rename = ...)]` 在 container 上已被废弃。

**唯一解**：真的把其中一个 Rust struct 改名（比如 `RuntimeTransferDirection` vs `DbTransferDirection`）。这种压力反而是个意外的整理收益。

## 相关
- [error-handling.md](error-handling.md) — `specta::Type` 手写 impl 投影错误
- [export.md](export.md) — `framework_runtime` / semantic types 注入 helpers
- [pitfalls.md](pitfalls.md) — BigInt / DuplicateTypeName / 第三方类型 等坑
