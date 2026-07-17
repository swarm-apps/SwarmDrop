# SwarmDrop 项目指南

> 本文件为 AI Coding Agent 提供项目背景、架构说明和开发规范。

## 项目概述

**SwarmDrop** 是一款去中心化、跨网络、端到端加密的文件传输工具，定位为"跨网络版的 LocalSend"。无需账号、无需服务器，支持局域网和跨网络点对点文件传输。

- **当前阶段**: Phase 2 (设备配对系统) —— 网络层已完成，配对系统进行中
- **应用标识**: `com.yexiyue.swarmdrop`
- **主语言**: 简体中文（所有注释和文档）

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端框架 | React 19 + TypeScript 5.8 + Vite 7 |
| 样式 | Tailwind CSS 4 |
| 路由 | TanStack Router（文件系统路由，自动代码分割） |
| 状态管理 | Zustand 5（4 个 Store：auth、network、preferences、secret） |
| UI 组件 | shadcn/ui（new-york 风格）+ Radix UI + Lucide 图标 |
| 国际化 | Lingui 5（8 语言：zh, zh-TW, en, ja, ko, es, fr, de） |
| 后端 | Rust 2021 + Tauri 2 |
| P2P 网络 | libp2p 0.56（通过 `swarm-p2p-core` 子模块） |
| 安全 | Stronghold（加密密钥库）+ Biometry（FaceID/TouchID/Windows Hello） |

## 项目结构

```
swarmdrop/
├── src/                          # 前端源码
│   ├── commands/                 # Tauri IPC 调用封装
│   ├── components/               # React 组件
│   │   ├── ui/                   # shadcn/ui 组件
│   │   ├── layout/               # 布局组件
│   │   ├── devices/              # 设备相关组件
│   │   ├── network/              # 网络状态组件
│   │   └── pairing/              # 配对流程组件
│   ├── hooks/                    # 自定义 React Hooks
│   ├── lib/                      # 工具函数和库封装
│   ├── locales/                  # 国际化翻译文件 (.po)
│   ├── routes/                   # TanStack Router 路由页面
│   │   ├── __root.tsx            # 根布局
│   │   ├── _auth.tsx             # 未认证布局（Aurora 背景）
│   │   ├── _auth/                # 认证流程页面
│   │   ├── _app.tsx              # 已认证布局（侧边栏/底部导航）
│   │   └── _app/                 # 主应用页面
│   ├── stores/                   # Zustand 状态管理
│   └── main.tsx                  # 应用入口
├── src-tauri/                    # Tauri Rust 后端
│   ├── src/
│   │   ├── lib.rs                # 主入口，插件注册，命令处理器
│   │   ├── commands/             # Tauri 命令处理器
│   │   ├── device/               # 设备管理
│   │   ├── network/              # 网络管理（P2P 节点）
│   │   ├── pairing/              # 配对系统
│   │   ├── protocol.rs           # P2P 协议定义
│   │   └── error.rs              # 错误类型定义
│   ├── capabilities/             # Tauri 权限配置
│   └── Cargo.toml                # Rust 依赖
├── libs/                         # Git 子模块：P2P 核心库
│   └── core/                     # swarm-p2p-core crate
├── docs/                         # Astro + Starlight 文档站点
├── dev-notes/                    # 开发文档
└── skills/                       # Claude Code 技能文件
```

## 构建和开发命令

```bash
# 包管理器：pnpm（不要使用 npm 或 yarn）

# 完整应用开发（Vite 前端 + Tauri Rust 后端）
pnpm tauri dev

# 仅前端开发（Vite dev server，端口 1420）
pnpm dev

# 生产构建
pnpm build              # 前端构建（tsc + vite build）
pnpm tauri build        # 完整应用构建

# 国际化
pnpm i18n:extract       # 提取翻译字符串到 .po 文件

# Rust 命令（需在 src-tauri/ 目录下执行）
cargo build
cargo test
cargo clippy
cargo fmt
```

## 架构详情

### 前端 ↔ 后端通信

前端通过 Tauri IPC 调用 Rust 后端。TypeScript 封装位于 `src/commands/`：

```typescript
// src/commands/network.ts
import { invoke } from "@tauri-apps/api/core";

export async function startNode(keypair: Keypair, devices: PairedDeviceInfo[]) {
  return await invoke<void>("start", { keypair, pairedDevices: devices });
}
```

Rust 命令处理器位于 `src-tauri/src/commands/`，在 `lib.rs` 中注册：

```rust
.invoke_handler(tauri::generate_handler![
    commands::start,
    commands::shutdown,
    commands::generate_keypair,
    // ...
])
```

### 路由系统

使用 TanStack Router 文件系统路由：

| 文件模式 | 含义 |
|----------|------|
| `__root.tsx` | 根布局 |
| `_layout.tsx` | 无路径布局（路径中不包含 `_layout`） |
| `page.lazy.tsx` | 懒加载路由（代码分割） |
| `index.tsx` | 目录索引路由 |

**当前路由结构：**
- `__root.tsx` — 根布局
- `_auth.tsx` — 未认证布局（Aurora 背景），守卫重定向到 `/devices`
- `_auth/welcome.lazy.tsx` — 欢迎页
- `_auth/setup-password.lazy.tsx` — 设置密码
- `_auth/unlock.lazy.tsx` — 解锁页
- `_auth/enable-biometric.lazy.tsx` — 启用生物识别
- `_app.tsx` — 已认证布局（侧边栏/底部导航）
- `_app/devices.lazy.tsx` — 设备列表
- `_app/settings.lazy.tsx` — 设置页
- `index.tsx` — 重定向到 `/devices`

### 状态管理

4 个 Zustand Store，不同持久化策略：

| Store | 用途 | 持久化 |
|-------|------|--------|
| `auth-store` | 认证流程状态 | `localStorage`（仅 `isSetupComplete` + `biometricEnabled`） |
| `preferences-store` | 主题、语言、设备名称 | `tauri-plugin-store` |
| `secret-store` | Ed25519 密钥对 | Stronghold 加密密钥库 |
| `network-store` | P2P 节点状态、对等节点列表 | 仅运行时（不持久化） |

### 响应式设计

3 个断点（`use-breakpoint` hook）：
- **mobile** (<768px): 底部导航
- **tablet** (768–1023px): 图标-only 侧边栏
- **desktop** (≥1024px): 展开侧边栏

### 国际化

使用 Lingui 框架，Babel macro 提取翻译：

```tsx
import { msg, Trans } from "@lingui/macro";

// JSX 中使用
<Trans>欢迎使用 SwarmDrop</Trans>

// 代码中使用
const message = i18n.t(msg`设备已连接`);
```

源语言为简体中文 (`zh`)。提取命令：`pnpm i18n:extract`

翻译文件位置：`src/locales/{locale}/messages.po`

### P2P 网络架构

**启动流程：**
1. `commands::start()` 创建 `NodeConfig`（启用 mDNS、Relay、DCUtR、autonat、引导节点）
2. 调用 `swarm_p2p_core::start::<AppRequest, AppResponse>()` → 返回 `(NetClient, Receiver<NodeEvent>)`
3. 生成 tokio 任务执行 DHT bootstrap
4. 创建 `NetManager`，存入 Tauri state
5. 启动事件循环，通过 Tauri Channel 转发事件到前端

**引导节点**: `47.115.172.218:4001`（TCP + QUIC）

**分享码系统**: 6 位数字，DHT key = SHA256(code)，记录包含 OS 信息 + 时间戳，默认 TTL 300 秒

> 移动端（iOS / Android，React Native + Expo + uniffi）在 `mobile/`，与桌面共用
> `crates/core` 与 `libs/core`。它曾是独立的 SwarmDrop-RN 仓，2026-07-16 并入本仓（该仓已归档）。
> Rust 桥接 `mobile/packages/swarmdrop-core/rust/mobile-core` 是根 Cargo workspace 的 member，
> 走 path 依赖；JS 侧则是独立 pnpm workspace（在 `mobile/` 下跑 `pnpm install`）。
> 发版是两条独立版本线：桌面 `v*`、移动 `mobile-v*`（仅 Android，iOS 不发版）。

## 开发规范

### 代码风格

- **注释**: 使用简体中文
- **命名**:
  - Rust：模块使用中文文档注释，代码使用英文命名
  - TypeScript：同样规范
- **路径别名**: `@/` 映射到 `./src/`（TypeScript 和 Vite 中一致）

### 错误处理

Rust 使用自定义 `AppError` 和 `AppResult`：

```rust
// src-tauri/src/error.rs
#[derive(thiserror::Error, Debug, serde::Serialize)]
pub enum AppError {
    #[error("节点未启动")]
    NodeNotStarted,
    #[error("网络错误: {0}")]
    Network(String),
    // ...
}

pub type AppResult<T> = Result<T, AppError>;
```

### 新增 Tauri 命令流程

1. 在 `src-tauri/src/commands/` 创建或修改模块
2. 在 `mod.rs` 中 pub use 导出
3. 在 `lib.rs` 的 `generate_handler![]` 中注册
4. 在 `src/commands/` 创建 TypeScript 封装
5. 如需新权限，更新 `src-tauri/capabilities/default.json`

### Git 子模块

`libs/` 是 Git 子模块，指向 `https://github.com/swarm-apps/swarm-p2p.git`

```bash
# 克隆时初始化子模块
git submodule update --init --recursive

# 更新子模块
git submodule update --remote
```

## 关键配置

### Vite (vite.config.ts)
- 端口固定为 1420（Tauri 要求）
- HMR 在 1421 端口
- 忽略 `src-tauri/**` 的文件监听

### Cargo.toml 优化

开发模式下为加密依赖开启优化（否则慢 10-100 倍）：

```toml
[profile.dev.package."*"]
opt-level = 3

[profile.dev.package.tauri-plugin-stronghold]
opt-level = 3
```

### shadcn/ui (components.json)

```json
{
  "style": "new-york",
  "rsc": false,
  "tailwind": {
    "baseColor": "neutral",
    "cssVariables": true
  },
  "iconLibrary": "lucide"
}
```

## 文件位置速查

| 用途 | 路径 |
|------|------|
| Tauri 命令 | `src-tauri/src/commands/` |
| 前端命令封装 | `src/commands/` |
| Zustand stores | `src/stores/` |
| 路由页面 | `src/routes/` |
| shadcn/ui 组件 | `src/components/ui/` |
| 布局组件 | `src/components/layout/` |
| 翻译文件 | `src/locales/{locale}/messages.po` |
| Tauri 权限 | `src-tauri/capabilities/default.json` |
| 产品需求 | `dev-notes/product-requirements.md` |
| 实现路线图 | `dev-notes/roadmap/implementation-roadmap.md` |
| UI 设计文件 | `dev-notes/design/design.pen` |

## 开发阶段

| 阶段 | 状态 | 描述 |
|------|------|------|
| Phase 1 — 网络层 | ✅ 完成 | libp2p Swarm、mDNS、DHT、Relay、DCUtR |
| Phase 2 — 配对系统 | 🚧 进行中 | 分享码、设备身份、DHT Provider |
| Phase 3 — 文件传输 | ⏳ 待开始 | Request-Response、E2E 加密、进度显示 |
| Phase 4 — 移动端 | ⏳ 待开始 | HTTP 桥或全平台 libp2p、二维码配对 |

详细阶段规划：`dev-notes/roadmap/phase-*.md`

## 注意事项

1. **Rust 库命名**: `swarmdrop_lib`（非 `swarmdrop`），避免 Windows 上 cargo 命名冲突
2. **移动端不支持 Updater**: 在 `lib.rs` 中容错处理，避免 panic
3. **分享码有效期**: 默认 300 秒，可在 `pairing/code.rs` 中调整
4. **DHT 不用于设备发现**: 仅用于分享码查找 PeerId，避免设备列表过大

## 相关技能

项目包含多个 Claude Code 技能文件（`.claude/skills/`）：

- `frontend` — 前端开发最佳实践（TanStack Router、Zustand、Lingui）
- `tauri-v2` — Tauri v2 开发指南
- `rust-best-practices` — Rust 最佳实践
- `rust-async-patterns` — Rust 异步编程模式
- `ui-ux-pro-max` — UI/UX 设计智能
- `openspec-*` — OpenSpec 变更管理工作流

使用方式：在对话中引用 `/skill-name`
