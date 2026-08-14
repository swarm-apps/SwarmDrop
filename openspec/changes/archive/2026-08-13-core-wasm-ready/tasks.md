# core-wasm-ready 任务分解

## Phase 1 — storage-sql 脚手架与搬移

- [x] 新建 `crates/storage-sql`（`swarmdrop-storage-sql`），workspace members 登记
- [x] 搬移 `crates/core/src/database/{store,ops,inbox}.rs`，`crate::transfer::*` 引用改直指 `swarmdrop_transfer`
- [x] Cargo.toml 依赖面收敛：`swarmdrop-transfer` + `entity`（开 sqlite feature）+ `swarmdrop-host` + `sea-orm`；确认零 core 依赖
- [x] `cargo check -p swarmdrop-storage-sql` 绿

## Phase 2 — core 摘除 sea-orm

- [x] `crates/core` 删 `database/` 模块与 Cargo.toml 的 sea-orm 依赖
- [x] `lib.rs` 模块表调整；grep 证 core 零 `sea_orm`/`DatabaseConnection`
- [x] `cargo check -p swarmdrop-core` 绿（native）

## Phase 3 — 宿主接线

- [x] src-tauri：`src/database/mod.rs` 初始化/`SqlSessionStore` 构造/`database::ops` re-export 改指 storage-sql；Cargo.toml 加依赖
- [x] src-tauri：MCP 收件箱查询、启动清理等消费点路径核对（逻辑零改动）
- [x] mobile-core：import/注入同步调整，`cargo check -p mobile-core` 绿
- [x] `cargo test --workspace` 全绿

## Phase 4 — core tokio → n0-future（24 处 / 4 文件）

- [x] `infra/supervisor.rs`
- [x] `pairing/manager.rs`
- [x] `network/event_loop.rs`
- [x] `presence/supervisor.rs`
- [x] 口径：spawn/time/Instant 换 n0-future；`tokio::sync`/`select!` 保留；`Instant - Duration` 模式一并 `checked_sub` 化（门 5 自查）
- [x] `cargo tree -e features` 核对 test-util 无意外传染面变化

## Phase 5 — wasm 门禁闭环

- [x] core 补 wasm 侧依赖调整（rand/getrandom js backend 等，照 transfer 先例）
- [x] `scripts/check-wasm.sh` 加 `-p swarmdrop-core`（六 crate），双门全绿
- [x] CI（rust.yml）确认覆盖

## Phase 6 — 回归与收尾

- [ ] 桌面冒烟：`pnpm tauri dev` 传输 + 历史查询 + 收件箱搜索
- [x] 知识库更新：storage-abstraction.md 状态块（trait 层→摘出完成）、CLAUDE.md 目录表（storage-sql 登记）
- [ ] （若有新坑）libp2p-wasm.md / rust-backend.md 补条目
