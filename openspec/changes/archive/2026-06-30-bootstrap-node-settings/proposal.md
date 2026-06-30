## Why

当前引导节点地址硬编码在 Rust 后端 `BOOTSTRAP_NODES` 常量中，用户无法自定义。同时桌面端设备页缺少网络状态信息（引导节点连接、中继状态、NAT 穿透状态），用户无法判断网络是否正常工作。需要将引导节点列表可配置化，并在前端展示关键网络状态。

## What Changes

- 设置页新增「引导节点」管理区域：展示默认节点 + 用户自定义节点列表，支持添加/删除自定义节点
- 引导节点列表持久化到 `preferences-store`，启动节点时传入自定义列表
- 后端 `start` 命令接受可选的自定义引导节点参数，与默认节点合并
- 修改引导节点列表后支持重启节点（停止 + 重新启动）
- `NetworkStatus` 新增引导节点连接状态字段
- 桌面端设备页顶部新增 `NetworkStatusBar`（类似移动端），展示引导节点连接、中继就绪、NAT 穿透状态
- 移动端 `NetworkStatusBar` 同步展示新增的状态信息

## Capabilities

### New Capabilities
- `bootstrap-node-settings`: 引导节点列表配置 — 设置页管理 UI、持久化、传参给后端启动命令、重启节点
- `network-status-display`: 网络状态展示 — 桌面端状态栏、引导节点/中继/NAT 状态指示器

### Modified Capabilities

（无现有 specs 需要修改）

## Impact

- **后端**: `src-tauri/src/commands/mod.rs` (start 命令签名)、`src-tauri/src/network/config.rs` (合并自定义节点)、`src-tauri/src/network/manager.rs` (NetworkStatus 新字段)、`src-tauri/src/network/event_loop.rs` (转发新状态)
- **前端 Store**: `src/stores/preferences-store.ts` (新增 bootstrapNodes 字段)、`src/stores/network-store.ts` (新增状态字段)
- **前端命令**: `src/commands/network.ts` (start 参数、NetworkStatus 类型)
- **前端 UI**: `src/routes/_app/settings/` (引导节点管理组件)、`src/routes/_app/devices/` (桌面端状态栏)、`src/components/network/` (NetworkStatusBar 增强)
