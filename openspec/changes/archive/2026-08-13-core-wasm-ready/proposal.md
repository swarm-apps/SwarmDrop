## Why

Web 端已实证可行（swarmdrop-web 浏览器传输端：offer/accept/续传/bao 验证全量复用 `swarmdrop-transfer`，同机基准 31 MB/s、主线程零卡顿），但它**绕过了 `crates/core`**——core 的 pairing / presence / share-code / device 业务域 Web 无法复用，已经出现重复实现（`crates/web/src/share_code.rs` 是 core 分享码逻辑的 Web 侧重写、`WebPeerDirectory` 是配对的合成桩）。后续每个落在 core 的功能，Web 都得再抄一遍。

core 走不进 wasm 的阻碍经实测盘点只剩两项：

1. **sea-orm**（`runtime-tokio-rustls` + `sqlx-sqlite`，撞 mio/ring 硬墙）——载体是 `crates/core/src/database/{store,ops,inbox}.rs` 三个 **Sql 实现**文件。存储 trait 层（`SessionStore`/`InboxStore`，在 `swarmdrop-transfer`）已于网络内核重构时落地，`DatabaseConnection` 已收敛到这三个文件内，切割线现成。
2. **tokio** 运行时原语 24 处（`spawn`/`time`/`select` 相关），集中在 4 个文件（`infra/supervisor.rs`、`pairing/manager.rs`、`network/event_loop.rs`、`presence/supervisor.rs`）。n0-future 迁移方案知识库已有定案（transfer/net 已走通同一条路）。

两项清掉后 core 成为**三端统一业务层**：桌面（src-tauri）、移动（mobile-core）、Web（crates/web）都是「host 端口实现 + 薄暴露壳」，新功能一处实现、三端各一行暴露。

## What Changes

- **新 crate `crates/storage-sql`（swarmdrop-storage-sql）**：`SqlSessionStore` + `ops.rs` + `inbox.rs` 整体搬出 core。依赖面收敛为 `swarmdrop-transfer`（SessionStore/InboxStore trait 与投影类型）+ `entity` + `swarmdrop-host`（AppResult）+ `sea-orm`——**不依赖 core**，依赖方向干净。
- **core 摘除 sea-orm**：`crates/core` 的 Cargo.toml 删 `sea-orm` 依赖，`database/` 模块移除；`database::ops` 的兼容 re-export 路径随宿主接线调整。
- **core tokio → n0-future**：24 处运行时原语迁移（`tokio::sync` 与 `select!` 是纯用户态原语、wasm 可用，保持原样——与 transfer/net 迁移口径一致）。
- **宿主接线**：src-tauri（`database/mod.rs` 的初始化/注入/re-export 改指 storage-sql）与 mobile-core 同步改引用；`SqlSessionStore` 注入路径不变（`Arc<dyn SessionStore>`）。
- **门禁**：`scripts/check-wasm.sh` 加 `-p swarmdrop-core`（六 crate 双 target），CI 随之覆盖。
- **非目标（明确排除）**：Web 侧消费 core（删 share_code 重复、接 core 配对、IndexedDB SessionStore）——另立 change，与 React UI 工程衔接；`InboxStore` 的 Web 实现与 FTS（硬编码 SQLite 裸 SQL，知识库已记录不可移植）；FFI/IPC 面变更（桌面 commands、mobile uniffi 接口不动）；wire 协议不动（`TerminalReason` 地雷已随 wire v2 拆除）。

## Capabilities

### New Capabilities
- `core-wasm-target`: `swarmdrop-core`（含 pairing / presence / device / network / protocol 域）在 `wasm32-unknown-unknown` 下可编译并通过 clippy 门禁，进入 `check-wasm.sh` 与 CI 常绿集合——Web 宿主具备消费 core 业务域的编译前提。

### Modified Capabilities
<!-- 无 spec 级行为变化：纯内部分层重构。桌面/移动的存储行为、传输语义、FFI 面均不变，回归以既有测试与冒烟兜底。 -->

## Impact

- **新增**：`crates/storage-sql/`（三文件搬移 + Cargo.toml）。
- **crates/core**：Cargo.toml（删 sea-orm）、`src/database/` 移除、`lib.rs` 模块表调整、4 个文件的 tokio 原语替换（`infra/supervisor.rs`、`pairing/manager.rs`、`network/event_loop.rs`、`presence/supervisor.rs`）。
- **src-tauri**：`src/database/mod.rs`（SeaORM 连接初始化 + SqlSessionStore 构造改 import storage-sql；`database::ops` re-export 路径同步）、Cargo.toml 加一条依赖。MCP 收件箱查询等桌面专属消费经此路径，不动逻辑。
- **mobile-core**：同类 import/注入调整 + `cargo check -p mobile-core` 回归。
- **workspace**：根 Cargo.toml members 登记；`scripts/check-wasm.sh`、`.github/workflows/rust.yml`。
- **回归面**：`cargo test --workspace` 全绿、桌面 `pnpm tauri dev` 冒烟（传输 + 历史查询 + 收件箱搜索）、`cargo check -p mobile-core`；wasm 双门六 crate 常绿。
- **风险**：n0-future 的 `test-util` feature 会经 feature unification 传染给全构建的 tokio（知识库已警告，迁移时核对构建面）；`SendWrapper` 单线程假设随 core 进 wasm 面扩大（CI 已钉 target 无 `+atomics`，惯例延续）。
