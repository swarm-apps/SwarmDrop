# 错误处理

## 两种模式

```rust
.error_handling(ErrorHandlingMode::Throw)
// 或
.error_handling(ErrorHandlingMode::Result)
```

默认是 `Result`。

### Throw 模式

前端按 `try/catch` 处理：

```ts
try {
    const user = await commands.getUser(42);
    // user: User
} catch (e) {
    // e: AppError（按你 Rust 端的 specta::Type 投影）
    console.error(e);
}
```

生成的 TS 大致：

```ts
getUser: (id: number) => __TAURI_INVOKE<User>("get_user", { id }),
//                       ^ 直接返回 Promise<User>，错误走 throw
```

**推荐场景**：

- 从原生 `invoke` 迁移过来的项目（原代码大量 `try/catch`，无缝兼容）
- 团队倾向 throw 风格的错误处理
- 全局 ErrorBoundary 统一捕获

### Result 模式

前端拿 tuple 风格的 `{ status, ... }`：

```ts
const r = await commands.getUser(42);
if (r.status === "error") {
    r.error;  // AppError
} else {
    r.data;   // User
}
```

生成的 TS：

```ts
getUser: (id: number) => __TAURI_INVOKE<...>("get_user", { id }).then(typedError),
//                       ^ typedError 把 throw 转成 { status: "ok" | "error", ... }
```

**推荐场景**：

- 想强制前端处理每个错误（编译器会因为 `r.status` 没分支检查而 narrow）
- 喜欢 Rust-style 错误处理
- 用 Effect / Neverthrow 等 Result 库

### 自定义 typedError 实现

如果想接 Effect，可以替换前端 `typedError` 的实现：

```rust
const TYPED_ERROR_IMPL: &str = r#"
async function typedError<T, E>(
    result: Promise<T>,
): Promise<Effect.Effect<T, E>> {
    try {
        const v = await result;
        return Effect.succeed(v);
    } catch (e) {
        return Effect.fail(e as E);
    }
}
"#;

SpectaBuilder::new()
    .error_handling(ErrorHandlingMode::Result)
    .typed_error_impl(TYPED_ERROR_IMPL);
```

## Rust 端的错误类型

### 简单情况：`thiserror`

```rust
use thiserror::Error;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Error, Serialize, Type)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("database error: {0}")]
    Database(String),
}
```

```rust
#[tauri::command]
#[specta::specta]
async fn get_user(id: u32) -> Result<User, AppError> {
    db.find(id).await.ok_or_else(|| AppError::NotFound(format!("user {id}")))
}
```

生成的 TS：

```ts
export type AppError =
    | { type: "notFound"; data: string }
    | { type: "invalid"; data: string }
    | { type: "database"; data: string };
```

### 复杂情况：内部 enum + 外部投影

如果错误内部包了不可 specta 的字段（比如 `tauri::Error` / `std::io::Error`），手写 `specta::Type` impl 投影到一个对前端友好的 payload struct：

```rust
use serde::Serialize;
use thiserror::Error;

/// 前端可见的错误结构
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorPayload {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not found: {0}")]
    NotFound(String),
}

// 手写 Type impl：AppError 在 specta schema 里被映射成 AppErrorPayload
impl specta::Type for AppError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <AppErrorPayload as specta::Type>::definition(types)
    }
}

// 实际序列化：写出 { kind, message }
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (kind, msg) = match self {
            AppError::Tauri(e) => ("Tauri", e.to_string()),
            AppError::Io(e) => ("Io", e.to_string()),
            AppError::NotFound(s) => ("NotFound", s.clone()),
        };
        let mut state = ser.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", kind)?;
        state.serialize_field("message", &msg)?;
        state.end()
    }
}
```

→ Rust 内部保留完整结构化错误（含 source、`From` 链），前端永远只看到 `{ kind, message }` 的扁平 payload。

```ts
try {
    await commands.getUser(42);
} catch (e: AppErrorPayload) {
    if (e.kind === "NotFound") { ... }
}
```

### 自动 `From` 链

让常见错误源直接 `?`：

```rust
impl From<std::io::Error> for AppError { ... }
impl From<serde_json::Error> for AppError { ... }
impl From<sea_orm::DbErr> for AppError { ... }

// 用法
async fn save(data: Vec<u8>, path: PathBuf) -> Result<(), AppError> {
    std::fs::write(&path, &data)?;          // io::Error → AppError
    serde_json::to_writer(...)?;            // serde_json::Error → AppError
    Ok(())
}
```

写一次 `From`，所有命令都能 `?` 收益。

## 类型化错误（typesafe errors）

specta v2 支持把 thiserror 的多个变体当成 discriminated union 给前端：

```rust
#[derive(Error, Debug, Serialize, Type)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum UploadError {
    #[error("io error: {0}")]
    IoError(
        #[serde(skip)]           // io::Error 自己不 Type
        #[from]
        std::io::Error,
    ),

    #[error("too large: {0}")]
    TooLarge(u64),

    #[error("invalid format")]
    InvalidFormat,
}

#[tauri::command]
#[specta::specta]
fn upload(data: Vec<u8>) -> Result<(), UploadError> { ... }
```

生成：

```ts
export type UploadError =
    | { type: "ioError" }                       // skip 的字段被剔除
    | { type: "tooLarge"; data: number }
    | { type: "invalidFormat" };

// throw 模式下
try {
    await commands.upload(data);
} catch (e: UploadError) {
    if (e.type === "ioError") { ... }
    else if (e.type === "tooLarge") { console.log(e.data); }
}
```

→ 前端能 narrow 到具体变体，**比 `{ kind, message }` 投影 strict 得多**。建议在新项目用这种风格。

## 选择决策树

```mermaid
flowchart TD
  A{需要前端 narrow 到具体错误变体?}
  A -- 是 --> B[用 typed enum<br/>tag + content + serialize]
  A -- 否 --> C[投影到 {kind, message} 扁平 payload]

  B --> D{已有大量 try/catch?}
  C --> D
  D -- 是 --> E[ErrorHandlingMode::Throw]
  D -- 否 --> F{想要编译期强制处理错误?}
  F -- 是 --> G[ErrorHandlingMode::Result]
  F -- 否 --> E
```

## 与 anyhow 的兼容

anyhow `Error` 没有 `specta::Type`，**不能**直接出现在命令签名里。两种做法：

```rust
// 方案 1：手动转 String
fn risky() -> anyhow::Result<User> { ... }

#[tauri::command]
#[specta::specta]
async fn cmd() -> Result<User, String> {
    risky().map_err(|e| e.to_string())
}
```

```rust
// 方案 2：包成 AppError
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
```

→ 推荐方案 2，统一所有错误进口。

## 相关
- [types.md](types.md) — `specta::Type` 手写 impl 的细节
- [commands.md](commands.md) — async / Result 命令的签名
- [pitfalls.md](pitfalls.md) — 错误类型链路的常见坑
