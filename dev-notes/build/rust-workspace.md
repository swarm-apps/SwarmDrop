# Rust 工作区验证命令

SwarmDrop Rust 代码现在由仓库根 `Cargo.toml` 管理 workspace。常用验证命令应在仓库根目录执行：

```bash
cargo metadata --no-deps --format-version 1
cargo check -p swarmdrop-core
cargo check -p entity
cargo check -p migration
cargo check -p swarmdrop
cargo check --workspace
```

桌面端仍可在 `src-tauri/` 目录内执行 Tauri 相关命令，但 Cargo 会向上查找并使用仓库根 workspace 与根 `target/` 目录。

