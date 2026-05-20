# Toolchain

## 概览

构建 / 包管理 / lint / CI 的项目特有约束。常规命令参考 CLAUDE.md "Build and Development Commands"；本主题只记非显见的坑。

## 包管理

### pnpm only

项目锁定 pnpm（`packageManager` 字段 + pnpm-lock.yaml）。npm / yarn install 会产生不一致的 lockfile。

## Cargo 配置

### dev profile 所有依赖 opt-level = 3

`Cargo.toml` 根：

```toml
[profile.dev.package."*"]
opt-level = 3
```

**Why**：crypto 依赖（`tauri-plugin-stronghold` / `chacha20poly1305` / `blake3` 等）和 libp2p 不开优化会慢 10-100×，dev 体感卡顿明显。

**不要做**：删除这段配置或把 `*` 改成具体 crate 列表——会漏掉新加的 crypto/网络依赖。

### workspace members 固定 4 个

```toml
members = ["crates/core", "crates/entity", "crates/migration", "src-tauri"]
```

加新 crate 必须显式登记，否则 `cargo check --workspace` 不会覆盖。

## Vite / Tauri 联动

### 端口固定 1420，HMR 走 1421

Tauri dev 期间硬编码连这两个端口。改 `vite.config.ts` 端口会让 `pnpm tauri dev` 白屏。

**相关文件**：`vite.config.ts`

### TAURI_DEV_HOST 用于真机调试

`vite.config.ts` 读取 `TAURI_DEV_HOST` env：设了就把 host / hmr.host 切到该 IP。本地 dev 不需要设。

## Git submodule

### libs/ 是 swarm-p2p submodule

```
[submodule "libs"]
    path = libs
    url = https://github.com/yexiyue/swarm-p2p.git
```

**克隆后必须**：`git submodule update --init --recursive`，否则 `cargo build` 找不到 `swarm-p2p-core`。

**注意**：`libs/core` 使用 **Rust 2024 edition**，与主仓 2021 edition 不同——给 submodule 加新文件时注意 edition 差异（比如 `unsafe` block 在 2024 更严格）。

## Lingui 提取

### sourceLocale 是 zh，实际只有 3 个 locale

```ts
sourceLocale: "zh",
locales: ["zh", "zh-TW", "en"],
```

CLAUDE.md 顶部写"8 locales"——那是规划目标，**当前实际是 3 个**。新增 locale 前先确认设计资源就绪。

### 提取命令必须先于 commit

```bash
pnpm i18n:extract
```

漏跑会导致 `src/locales/*/messages.po` 缺少新加的字符串，运行时降级显示原文。

## 版本号同步

三处版本必须一起改（已经踩过坑）：

| 文件 | 字段 |
|---|---|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `package.version` |
| `src-tauri/tauri.conf.json` | `version` ←  **release 真源**（CI 用这个） |

`crates/core/Cargo.toml` 的 version 也建议同步，方便 SwarmDrop-RN 共享 core 时对版本。

## 提交前 checklist

```bash
pnpm exec tsc --noEmit
cargo check --workspace
cargo fmt --all
cargo clippy --workspace -- -D warnings   # 项目期望零 warning
```
