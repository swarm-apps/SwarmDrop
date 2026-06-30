## Why

当前 SwarmDrop 的设备管理和网络状态职责全部在前端（`network-store.ts` 维护 `peers` Map、`listenAddrs`、`natStatus` 等，`secret-store.ts` 持久化已配对设备）。后端 `NetManager` 只是 `NetClient` + `PairingManager` 的薄包装，不持有任何设备或网络状态。这导致即将实现的 MCP Server 无法独立访问设备列表和网络状态——必须依赖前端在线。同时 `NetManager` 职责模糊，随着功能扩展会变成大杂烩。

## What Changes

- **新增 `DeviceManager` 模块**（`src-tauri/src/device/`）：后端独立管理运行时 peer 状态和已配对设备，作为设备数据的单一数据源
- **后端接管 `NodeEvent` 中的设备状态更新**：`PeersDiscovered`、`PeerConnected`、`PeerDisconnected`、`IdentifyReceived`、`PingSuccess` 等事件在后端 `DeviceManager` 中处理并更新 `DashMap`
- **`NetManager` 接管网络状态**：`Listening`、`NatStatusChanged`、`RelayReservation` 等事件在 `NetManager` 中处理，维护 `listen_addrs`、`nat_status`、`public_addr` 等状态
- **新增 Tauri Command `list_devices`**：前端通过此命令获取设备列表，替代前端自己维护 `peers` Map
- **新增 Tauri Command `get_network_status`**：前端通过此命令获取网络状态，替代前端自己维护网络状态字段
- **新增 Tauri Event `devices-changed` / `network-status-changed`**：状态变更时通知前端刷新
- **修改 `start()` 命令**：接收 `paired_devices` 参数，注入到 `DeviceManager`
- **修改 `NetManager`**：新增 `devices: DeviceManager` 字段 + 网络状态字段（`listen_addrs`、`nat_status`、`public_addr`）
- **简化前端 `network-store.ts`**：移除 `peers` Map、`listenAddrs`、`natStatus` 等字段及 `handleEvent` 中对应的逻辑，改为调用后端命令获取数据
- **迁移 `device.rs` → `device/` 目录模块**：`OsInfo` 迁入 `device/mod.rs`，新增 `device/manager.rs` 和 `device/types.rs`

## Capabilities

### New Capabilities

- `device-manager`: 后端设备管理器，负责运行时 peer 追踪（DashMap）、已配对设备管理、统一查询接口（DeviceFilter: all/connected/paired）、NodeEvent 驱动的状态更新
- `network-status`: NetManager 维护网络状态（listen_addrs、nat_status、public_addr），处理 Listening/NatStatusChanged/RelayReservation 事件
- `device-commands`: 前端调用的 Tauri Command（`list_devices`、`get_network_status`）和 Event（`devices-changed`、`network-status-changed`），前端 network-store 简化为消费后端数据

### Modified Capabilities

## Impact

- **后端代码**：
  - `src-tauri/src/device.rs` → 迁移为 `src-tauri/src/device/` 模块
  - `src-tauri/src/commands/mod.rs` — `NetManager` 扩展、`start()` 签名变更
  - `src-tauri/Cargo.toml` — 新增 `dashmap`、`chrono` 依赖
- **前端代码**：
  - `src/stores/network-store.ts` — 大幅简化，移除 peer 状态跟踪
  - `src/commands/network.ts` — `start()` 签名变更，新增 `listDevices()`
- **依赖**：新增 `dashmap 6.1`、`chrono 0.4`（Rust crate）
- **后续影响**：MCP Server 的 `list_devices` Tool 可直接调用 `DeviceManager.get_devices()`，不再需要额外的数据层
