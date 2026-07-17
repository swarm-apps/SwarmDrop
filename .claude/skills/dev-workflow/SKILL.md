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
| [theme-and-styling.md](../../../dev-notes/knowledge/theme-and-styling.md) | shadcn/ui 用法、Tailwind v4 token、主题切换、窗口装饰（macOS Overlay / Win 自定义标题栏）、Aurora 背景等 UI 层约束 |
| [rust-backend.md](../../../dev-notes/knowledge/rust-backend.md) | Tauri command 约定、`crates/core` ↔ `src-tauri` 边界、SeaORM/SQLite、P2P/async、tracing、specta + chrono 类型映射 |
| [toolchain.md](../../../dev-notes/knowledge/toolchain.md) | pnpm/Vite/Tauri CLI、Biome、Cargo workspace、Lingui 提取、git submodule（libs/）、CI、版本号同步 |
| [iroh-migration.md](../../../dev-notes/knowledge/iroh-migration.md) | libp2p → iroh 迁移评估结论（**评估中，未决策**）：能力差、被推翻的旧认知、relay/配对/Web 端取舍、生态选型否决清单。碰 P2P 选型、Web 端、n0-* 依赖或有人提「迁 iroh」时先读 |

**读取方式**：用 Read 工具读取对应文件，遵循其中记录的最佳实践和注意事项。

如果不确定读哪个，先读 `dev-notes/knowledge/` 目录列表按文件名判断。

### 2. 开发中：遵循最佳实践

同时参考以下通用 skill（如果与当前任务相关，自动调用）：

- `/tauri-v2` — Tauri v2 IPC、capabilities、配置（**桌面壳的所有改动都先查**）
- `/sea-orm-2` — SeaORM 2.0 entity / migration / 嵌套 ActiveModel（数据库相关都查）
- `/vercel-react-best-practices` — React 性能（re-render、bundle、waterfalls）
- `/rust-best-practices` — Rust 通用规范（写新 Rust 代码或审查时）
- `/rust-async-patterns` — Tokio、异步事件循环、取消/并发
- `/frontend` — 项目内置：TanStack Router / Zustand / Lingui 用法
- `/lingui-best-practices` — i18n（新增 `<Trans>` / `t``...```、提取 catalog）
- `/tailwind-css-patterns` / `/tailwind-design-system` — Tailwind v4 utility 和 design token
- `/uniffi-bindgen-rn` — `mobile/` 经 uniffi 共享 `crates/core` 的桥接约束（仅相关时）

**优先级**：项目知识库 > 项目级 skill（`.claude/skills/*`）> 通用 skill > Claude 自身知识。当项目知识库中有明确记录时，以项目知识库为准。

### 3. 开发后：更新知识库

完成代码修改后，**检查是否产生了新的项目知识**：

**需要记录的内容**：
- 新引入的依赖及其正确用法
- 发现的配置坑和 workaround（尤其是 `crates/core` ↔ `src-tauri` / `swarm-p2p-core` / Tauri plugin 交互）
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
pnpm i18n:extract   # 新增/修改翻译字符串后

# Rust（在仓库根目录跑即可，workspace 会一并 check）
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo check --workspace

# 单 crate
cargo check -p swarmdrop-core --features specta
cargo check -p swarmdrop
```

**约束**：提交前 cargo check + tsc 必须通过。`pnpm tauri dev` 启动时 specta 会自动重新导出 `src/lib/bindings.ts`——改了 Rust 端 IPC 类型时**不要**手动改 bindings.ts，让它自动生成。
