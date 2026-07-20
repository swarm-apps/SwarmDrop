# core-wasm-target

## ADDED Requirements

### Requirement: core 在 wasm32 双 target 可编译并纳入门禁

`swarmdrop-core`（含 pairing / presence / device / network / protocol 业务域）SHALL 在
`wasm32-unknown-unknown` 下通过 `cargo check` 与 `clippy -D warnings` 双门，并作为第六个
crate 纳入 `scripts/check-wasm.sh` 与 CI 常绿集合。

#### Scenario: check-wasm 六 crate 常绿

- **WHEN** 运行 `bash scripts/check-wasm.sh`
- **THEN** `swarmdrop-net-base` / `swarmdrop-net` / `swarmdrop-host` / `swarmdrop-transfer` / `swarmdrop-web` / `swarmdrop-core` 六 crate 在 native 与 wasm32 双 target 下 check + clippy 全部通过

#### Scenario: core 依赖图零 sea-orm

- **WHEN** 在 `crates/core` 源码与 Cargo.toml 中 grep `sea_orm` / `sea-orm` / `DatabaseConnection`
- **THEN** 零命中（Sql 实现整体住在 `crates/storage-sql`）

### Requirement: 存储注入语义不变

桌面与移动宿主 SHALL 继续以 `Arc<dyn SessionStore>`（`swarmdrop-transfer` 端口）注入
`SqlSessionStore`，存储行为（会话/断点/收件箱/历史查询）与本 change 之前逐字节一致。

#### Scenario: 桌面回归

- **WHEN** 桌面执行传输 → 暂停 → 续传 → 完成，并进行历史查询与收件箱搜索
- **THEN** 行为与 change 前一致，`cargo test --workspace` 全绿
