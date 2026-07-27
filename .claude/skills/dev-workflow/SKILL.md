---
name: dev-workflow
description: |
  SwarmDrop 项目开发工作流技能。在以下场景自动调用：
  (1) 编写或修改任何 src/ / src-tauri/src/ / crates/core/src/ 下的代码
  (2) 添加新依赖或修改配置文件（Cargo.toml / package.json / tauri.conf.json）
  (3) 完成一个 feature 或修复一个 bug
  触发关键词：组件开发、bug 修复、重构、新功能、依赖升级、配置变更、Tauri command、P2P、配对、传输
---

# Dev Workflow — SwarmDrop 开发工作流

## 工作流程

### 1. 开发前：加载相关知识

根据当前任务，读取 `dev-notes/knowledge/` 下的相关主题文件：

| 主题文件 | 适用场景 |
|---|---|
| [theme-and-styling.md](../../../dev-notes/knowledge/theme-and-styling.md) | shadcn/ui 用法、Tailwind v4 token、主题切换、窗口装饰（macOS Overlay / Win 自定义标题栏）、Aurora 背景、**Zustand selector 派生数组陷阱**等 UI 层约束 |
| [rust-backend.md](../../../dev-notes/knowledge/rust-backend.md) | Tauri command 约定、`crates/core` ↔ `src-tauri` 边界、SeaORM/SQLite、async、tracing、specta + chrono 类型映射、外部打开（share-target）反向流 |
| [toolchain.md](../../../dev-notes/knowledge/toolchain.md) | pnpm/Vite/Tauri CLI、Cargo workspace 与 profile、Lingui 提取与实际 locale、CI、版本号三处同步 |
| [net-kernel.md](../../../dev-notes/knowledge/net-kernel.md) | 网络内核 `swarmdrop-net`（2026-07 重构产物，取代已删除的 `libs/` swarm-p2p-core）：架构速览（Endpoint/actor/双轨事件/扩展点四件套）、**libp2p git pin 的 6 条校准坑**（relay HOP status 默认不广告、NoAddressesInReservation、add_peer_address 非地址簿、DialPeerConditionFalse、circuit listen 时序、kad Instant 分叉）、wasm 工程约定（check-wasm.sh / 业务层零 cfg 硬约束）、wire v2 契约点（DhtKey 域分离 / BlockData proof 扩展位 / RPC 帧上限）、已知负债。**碰 crates/net、crates/net-base、协议注册、relay、DHT、升级 libp2p rev 时必读** |
| [libp2p-wasm.md](../../../dev-notes/knowledge/libp2p-wasm.md) | Web 端（wasm）可行性调研（2026-07）。**结论已落地**——`crates/web` + `docs/app/try` 就是它的产物。仍有效的部分：webrtc-direct 与自建 relay、浏览器公网零可达入口、mixed content / Chrome LNA 平台门、tokio → n0-future、getrandom 双版本等 wasm 编译的坑。碰 Web 端、wasm、自建中继、transport 层时先读 |
| [storage-abstraction.md](../../../dev-notes/knowledge/storage-abstraction.md) | 把 sea-orm 从 `crates/core` 摘出去。**已落地**——core 零 sea-orm，SQL 实现独立成 `crates/storage-sql`，端口 trait 在 `crates/transfer/src/store.rs`。文件保留调研过程：切割线在 `DatabaseConnection` 不在 `entity`、SendWrapper 免改 trait 签名、耦合面量化。碰存储端口、entity 依赖或 Web 端持久化时先读 |
| [iroh-migration.md](../../../dev-notes/knowledge/iroh-migration.md) | libp2p → iroh 迁移评估（2026-07）。**已决策：不迁移**——但 iroh 的 API 形态被 `crates/net` 借鉴（Endpoint 门面 + 隐藏事件循环）。能力差、被推翻的旧认知、生态选型否决清单。碰 P2P 选型、n0-* 依赖或有人提「迁 iroh」时先读。**n0-future 替换的落地细节以本文件为准** |
| [demo-recording.md](../../../dev-notes/knowledge/demo-recording.md) · [file-browser.md](../../../dev-notes/knowledge/file-browser.md) | 演示录制流程 / 文件浏览组件（按需） |

**读取方式**：用 Read 工具读取对应文件，遵循其中记录的最佳实践和注意事项。

如果不确定读哪个，先读 `dev-notes/knowledge/` 目录列表按文件名判断。

> **注意时效**：`libp2p-wasm.md` / `storage-abstraction.md` / `iroh-migration.md` 是**调研快照**，
> 记录的是当时的判断与实测。其结论已分别落地或被否决（见上表标注），读时以「为什么这么选」
> 为主，不要把里面的待办清单当作未完成工作。当前架构以 `CLAUDE.md` 为准。

### 2. 开发中：遵循最佳实践

同时参考以下通用 skill（如果与当前任务相关，自动调用）：

- `/tauri-v2` — Tauri v2 IPC、capabilities、配置（**桌面壳的所有改动都先查**）
- `/tauri-specta` — IPC 类型与 typed events（**改跨 IPC 边界类型时必查**：`collect_commands!` /
  `collect_events!` 在 `src-tauri/src/setup.rs`，bindings 自动导出）
- `/sea-orm-2` — SeaORM 2.0 entity / migration / 嵌套 ActiveModel（数据库相关都查）
- `/vercel-react-best-practices` — React 性能（re-render、bundle、waterfalls）
- `/rust-best-practices` — Rust 通用规范（写新 Rust 代码或审查时）
- `/rust-async-patterns` — Tokio、异步事件循环、取消/并发
- `/frontend` — 项目内置：TanStack Router / Zustand / Lingui 用法
- `/uniffi-bindgen-react-native` — `mobile/` 经 uniffi 共享 `crates/*` 的桥接约束（仅相关时）
- `/ui-ux-pro-max`、`/impeccable` — UI/视觉工作（先读 `PRODUCT.md` + `DESIGN.md`）
- `/iroh` — 仅在评估 P2P 选型时参考；**本项目未使用 iroh**，网络栈是自研 `crates/net`

**优先级**：项目知识库 > 项目级 skill（`.claude/skills/*`）> 通用 skill > Claude 自身知识。当项目知识库中有明确记录时，以项目知识库为准。

### 3. 开发后：更新知识库

完成代码修改后，**检查是否产生了新的项目知识**：

**需要记录的内容**：
- 新引入的依赖及其正确用法
- 发现的配置坑和 workaround（尤其是 `crates/core` ↔ `src-tauri` 边界、`crates/net` 内核、Tauri plugin 交互）
- 做出的架构决策及原因（含桌面 / `mobile/` 两端的取舍）
- 与通用最佳实践不同的项目特定做法
- 解决的非显见 bug 的根因

**不需要记录的内容**：
- 代码本身能表达的东西
- 通用编程知识（不特定于本项目）
- 临时性的调试信息
- `git log` / `git blame` 能查到的

**更新方式**：
1. 判断属于哪个主题文件
2. 追加新条目到对应文件的合适分类下
3. 如果现有主题都不合适，再创建新的主题文件
4. 如果发现已有条目过时，更新或删除它

**条目格式**：

```markdown
### 条目标题

简短描述做了什么、为什么这样做。

**正确做法**：
- 具体的代码模式或配置

**不要做**（如果有）：
- 错误的做法及原因

**相关文件**：`path/to/file`
```

### 4. 代码质量检查

开发完成后，运行 `/simplify` 检查代码质量。lint / format / typecheck 命令：

```bash
# 前端
pnpm exec tsc --noEmit
pnpm test                  # vitest
pnpm check:zustand-access  # selector 派生数组检查（防无限重渲染）
pnpm i18n:extract          # 新增/修改翻译字符串后

# Rust（在仓库根目录跑即可，workspace 会一并 check）
cargo fmt --all
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace

# wasm 双 target 门禁 —— 改 net / net-base / host / transfer / invite / core / web 后必跑
./scripts/check-wasm.sh
./scripts/check-wasm.sh --clippy

# 单 crate
cargo check -p swarmdrop-core --features specta
cargo check -p swarmdrop            # 桌面壳（package 名 swarmdrop，lib 名 swarmdrop_lib）
```

**约束**：

1. 提交前 `cargo check --workspace` + `tsc --noEmit` 必须通过。
2. 动了 wasm 侧 7 个 crate 中任何一个，`./scripts/check-wasm.sh` 也必须过——CI 会拦。
3. `pnpm tauri dev` 启动时 specta 自动重新导出 `src/lib/bindings.ts`——改了 Rust 端 IPC 类型
   **不要**手动改 bindings.ts，让它自动生成。
4. `cargo clippy` 当前在 CI 里是 `continue-on-error`（存量 warning 基线未清），
   所以**别指望 CI 帮你拦 clippy 问题**，新代码自己保证干净。
