# Core 抽离盘点

本清单对应 OpenSpec 变更 `extract-core-and-add-rn-mobile` 的任务 1.1，用于标记 `src-tauri/src` 当前模块在迁移后的归属。

## 可复用业务模块

这些模块应逐步迁移到 `crates/core`，只保留宿主适配点：

| 模块 | 迁移归属 | 说明 |
| --- | --- | --- |
| `protocol.rs` | `crates/core` | P2P request/response wire types，桌面和 RN 必须共享同一套定义。 |
| `device/*` | `crates/core` + host adapter | 设备模型、在线状态、连接类型和 agent_version 解析可复用；本机 OS 信息采集应由 host 提供。 |
| `pairing/*` | `crates/core` | 配对码、DHT key、配对请求/响应和已配对设备状态属于共享业务逻辑。 |
| `network/config.rs` | `crates/core` | 节点配置可迁移，但平台差异如 DNS feature 和启动参数由 host 传入。 |
| `network/manager.rs` | `crates/core` | 网络生命周期和 manager 聚合应成为 core runtime 的主体。 |
| `network/event_loop.rs` | `crates/core` + `EventBus` | libp2p 事件处理可复用，Tauri emit/通知/文件访问需要转为 host trait。 |
| `transfer/crypto.rs` | `crates/core` | 分块加密和校验逻辑平台无关。 |
| `transfer/progress.rs` | `crates/core` + `EventBus` | 进度计算可复用，事件发送由 host adapter 负责。 |
| `transfer/sender.rs` | `crates/core` + `FileAccess` | 发送状态机可复用，文件读取和 Tauri 事件必须抽象。 |
| `transfer/receiver.rs` | `crates/core` + `FileAccess` | 接收状态机可复用，保存位置和通知必须抽象。 |
| `transfer/offer.rs` | `crates/core` + host adapter | 传输 offer、resume、session 管理可复用，Tauri Channel 和 AppHandle 需要外移。 |
| `database/ops.rs` | `crates/core` | 传输历史和 session 数据操作平台无关。 |

## Tauri 桌面 Host 专属模块

这些模块应留在 `src-tauri`，作为桌面 host adapter：

| 模块 | 归属 | 说明 |
| --- | --- | --- |
| `lib.rs` / `main.rs` | `src-tauri` | Tauri builder、插件注册、IPC handler 和 managed state。 |
| `commands/*` | `src-tauri` | Tauri IPC 边界，后续应变成 core runtime 的薄封装。 |
| `mobile.rs` | `src-tauri` | Tauri mobile/updater 插件兼容层，不进入共享 core。 |
| `mcp/*` | `src-tauri` | MCP server 依赖 Tauri AppHandle 和桌面能力，RN MVP 不迁移。 |
| `database/mod.rs` | `src-tauri` + `AppPaths` | 数据库连接路径来自 Tauri app data，连接初始化后续可拆成 host path + core migration。 |

## 需要拆成 Host Trait 的平台能力

| 当前模块 | 目标 trait | 说明 |
| --- | --- | --- |
| `commands/identity.rs` | `KeychainProvider` | Stronghold/密码流程应迁移为 legacy adapter，新默认路径使用 host 安全存储。 |
| `file_source/*` | `FileAccess` | 桌面路径、Tauri Android URI、RN DocumentPicker 都应投影成统一 source。 |
| `file_sink/*` | `FileAccess` | 保存路径、Android 公共目录、RN 私有目录都应投影成统一 sink。 |
| `events.rs` | `EventBus` | 事件名称可保留给桌面 adapter，core 只发布 typed event。 |
| `tauri-plugin-notification` 调用 | `Notifier` | 桌面通知和移动端通知分别实现。 |
| `commands::install_update` / `mobile.rs` | `UpdateInstaller` | 桌面 updater 和移动端 no-op/后续 store 逻辑分开。 |

## 移动端暂不迁移能力

第一阶段 RN MVP 不迁移以下能力：

- MCP server 和 MCP tools。
- Tauri updater / Android APK 安装流程。
- Tauri mobile 生成目录 `src-tauri/gen/android`。
- 长时间后台传输、系统分享扩展、公共下载目录高级权限流。
- Expo Go 运行支持。
