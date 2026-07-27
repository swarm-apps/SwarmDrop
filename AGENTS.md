# SwarmDrop 项目指南

> 本文件为 AI Coding Agent 提供入口。**架构描述的唯一事实源是根目录的
> [`CLAUDE.md`](CLAUDE.md)** —— 项目结构、技术栈、模块边界、构建命令、开发阶段全在那里，
> 本文件不再重复。
>
> 之所以这样安排：这两份文档过去各写一套架构描述，结果双双漂移出一个大版本
> （AGENTS.md 一度停留在「Phase 2 / Stronghold / 8 语言 / Rust 2021」）。
> 单一事实源是防止复发的唯一办法。**改架构时只需同步 `CLAUDE.md`。**

## 开始之前

1. 读 [`CLAUDE.md`](CLAUDE.md) —— 项目全貌与硬约束
2. 动手写代码前调用 `/dev-workflow`（加载 `dev-notes/knowledge/` 下的踩坑记录）
3. 碰 UI/视觉先读 `PRODUCT.md` + `DESIGN.md`

## 一分钟速览

- **是什么**：去中心化、跨网络、端到端加密的文件传输工具 —— "跨网络版 LocalSend"。
  无账号、无服务器。同时是 AI Agent 的设备间数据通道（内置本地 MCP server）。
- **三端**：桌面（Tauri）· 移动（React Native + uniffi，`mobile/`）· Web（wasm，`crates/web`）
- **应用标识**：`com.yexiyue.swarmdrop`
- **主语言**：简体中文 —— 所有注释、文档、commit message 均用中文

## 开发规范

### 代码风格

- **注释与文档**：简体中文；**代码标识符**：英文
- **注释写「为什么」，不写「是什么」** —— 这是本仓最重要的资产。
  决策要带证据（实测结论、上游 PR 编号、被推翻的假设）。参考根 `Cargo.toml` 里
  libp2p pin 与 `[profile.mobile-release]` 的注释写法。
- **路径别名**：`@/` → `./src/`
- **图表**：一律 Mermaid，不用 ASCII art

### 提交前必须通过

```bash
cargo fmt --all
cargo check --workspace --all-targets
./scripts/check-wasm.sh          # 破坏浏览器 target 的改动 CI 会拦
pnpm exec tsc --noEmit
```

改了 Rust 端 IPC 类型**不要手改** `src/lib/bindings.ts` —— `pnpm tauri dev` 会自动重新导出。

### Commit 规范

Conventional Commits（`feat` / `fix` / `refactor` / `docs` / `chore` / `ci` / `test` / `build` / `perf`），
message 用中文。changelog 由 git-cliff 生成，`pnpm changelog`。

### 错误处理

Rust 端统一 `thiserror`：桌面壳用 `src-tauri/src/error.rs` 的 `AppError` / `AppResult`，
各 crate 有自己的 error 类型并在边界转换。避免在库层 `unwrap()` —— 传输与 supervisor
是常驻 actor，一次 panic 会带走整条链路。

### 变更管理

较大的功能与重构走 `openspec/`：提案落在 `changes/<name>/`，稳定后的规格进 `specs/`。
相关 skill：`/opsx:propose`、`/opsx:apply`、`/opsx:archive`。

## 相关技能

项目内与通用 skill（对话中用 `/skill-name` 调用）：

| Skill | 用途 |
|---|---|
| `/dev-workflow` | **入口** —— 加载项目知识库并在收尾时引导更新 |
| `/frontend` | TanStack Router、Zustand、Lingui 的项目内约定 |
| `/tauri-v2` · `/tauri-specta` | 桌面壳改动、IPC 类型与 typed events |
| `/rust-best-practices` · `/rust-async-patterns` | Rust 通用规范与异步模式 |
| `/sea-orm-2` | entity / migration / 嵌套 ActiveModel |
| `/uniffi-bindgen-react-native` | `mobile/` 的 uniffi 桥接 |
| `/impeccable` · `/ui-ux-pro-max` | UI 设计与审查 |
| `/opsx:*` | OpenSpec 变更流程 |

**优先级**：项目知识库（`dev-notes/knowledge/`）> 项目级 skill > 通用 skill > 模型自身知识。
知识库有明确记录时以知识库为准。
