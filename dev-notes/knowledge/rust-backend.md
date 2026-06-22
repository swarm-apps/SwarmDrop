# Rust Backend

## 概览

Rust 端的项目特有约束：crates/core 与 src-tauri 边界、specta IPC 类型映射、SeaORM/SQLite、libp2p P2P。常规 Rust 风格查 `/rust-best-practices`，async 模式查 `/rust-async-patterns`，Tauri IPC 查 `/tauri-v2`，SeaORM 查 `/sea-orm-2`。

## 模块边界

### 业务逻辑放 crates/core，src-tauri 是薄壳

`src-tauri/src/lib.rs` 用 `pub use swarmdrop_core::pairing;` 等 alias 把 core 模块路径桥进 crate（所以代码里 `crate::pairing::*` / `crate::protocol::*` 仍然有效）。**桌面壳唯一保留业务逻辑的模块是 `transfer/`**，其它都已迁移到 core。

**正确做法**：
- 加新业务逻辑/类型默认放 `crates/core`，让 SwarmDrop-RN 也能复用
- 桌面特定（keychain / 文件系统路径 / Tauri command 包装）才放 src-tauri
- 改 core 时跑 `cargo check -p swarmdrop-core --features specta`，再跑 `cargo check -p swarmdrop` 确认桌面壳不破

**相关文件**：`crates/core/src/lib.rs`、`src-tauri/src/lib.rs`、`dev-notes/architecture/core-desktop-mobile-boundaries.md`

## IPC 类型 (specta)

### bindings.ts 是自动生成的

`src-tauri/src/setup.rs:104` 在 debug build 时调用 `specta.export(..., "../src/lib/bindings.ts")`，每次 `pnpm tauri dev` 启动都会重写。

**不要做**：
- 手动改 `src/lib/bindings.ts`——会被下次 dev 启动覆盖
- 把 bindings.ts 当成"前端可改的契约"

**正确做法**：
- 改 IPC 类型 → 改 Rust 端 struct → 运行一次 `pnpm tauri dev`（或 `cargo run`，会在 setup hook 里触发导出）→ bindings.ts 自动更新
- 临时手改 bindings.ts 只用于"先让 tsc 通过、稍后再启 dev 重新导出"

### specta 需要开 chrono feature

`Cargo.toml` 里 `specta` 必须含 `chrono` feature，否则 `chrono::DateTime<Utc>` 无法 `derive(specta::Type)`。SwarmDrop 已配置（见 `crates/core/Cargo.toml` + `src-tauri/Cargo.toml`）。

### 跨 IPC 的时间类型用 DateTime<Utc>

specta + chrono 会把 `DateTime<Utc>` 映射成 ISO 8601 字符串（前端 `string` 类型）。前端 `new Date(isoString)` 自动正确解析。

**不要做**：
- 用 `i64` 当 IPC 时间戳——前端容易把秒当毫秒（`new Date(秒数)` 解析成 1970 年附近），导致 timer 死循环（见配对码每秒重生 bug 的修复 commit `8d298e5`）

**例外**：DHT 跨设备记录（`ShareCodeRecord`）保持 `i64` Unix 秒以稳定线路格式 + 节省 record 体积。From 转换里手写 `.timestamp()`。

**相关文件**：`crates/core/src/pairing/code.rs`

## Clippy / dead_code

### 用 #[expect(...)] 替代 #[allow(...)]

项目里清一色用 `#[expect(clippy::xxx, reason = "...")]` 而非 `#[allow]`。Rust 1.81+ 的语义是：标了 expect 的 lint 一旦"自然消失"会反向报警，避免遗留的过期 allow。

**正确做法**：
```rust
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub fn insert_session(...) { ... }
```

**相关文件**：`crates/core/src/database/ops.rs`、`crates/core/src/transfer/receive.rs`

## P2P / 异步

### 启动顺序：plugin → updater → database → start command

`src-tauri/src/setup.rs` 里 plugin 在 Builder::default() 注册；updater + database 在 setup() hook 里初始化并注入 Tauri state。**P2P 节点不在启动期自动起**——前端调 `commands::start()` 才创建 `NetClient` + `PairingManager`。

**相关文件**：`src-tauri/src/setup.rs`、`src-tauri/src/lib.rs` 的 `start` 命令
