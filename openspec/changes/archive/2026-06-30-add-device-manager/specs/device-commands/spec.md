## ADDED Requirements

### Requirement: start 命令接收 paired_devices 参数

`start` Tauri Command SHALL 接受额外的 `paired_devices: Vec<PairedDeviceInfo>` 参数，并将其传递给 `DeviceManager::new()`。

#### Scenario: 传入已配对设备启动网络

- **WHEN** 前端调用 `invoke("start", { channel, pairedDevices: [{ id: "12D3KooW...", name: "PC", os: "windows", pairedAt: 1700000000000 }] })`
- **THEN** 后端创建的 `DeviceManager` 包含该已配对设备，`is_paired("12D3KooW...")` 返回 `true`

#### Scenario: 无已配对设备启动

- **WHEN** 前端调用 `invoke("start", { channel, pairedDevices: [] })`
- **THEN** 后端创建的 `DeviceManager` 的 `paired_devices` 为空

### Requirement: start 命令中 NodeEvent 双路处理

`start` 命令的事件循环 SHALL 按以下顺序处理每个 `NodeEvent`：
1. 调用 `NetManager.handle_event(&event)` 更新网络状态（Listening、NatStatusChanged、RelayReservation）
2. 调用 `DeviceManager.handle_event(&event)` 更新设备状态（PeersDiscovered、PeerConnected 等）
3. 根据事件类型发送对应的 Tauri Event（`devices-changed` 或 `network-status-changed`）
4. 通过 Channel 转发原始事件给前端

#### Scenario: 设备事件处理顺序

- **WHEN** 后端收到 `PeerConnected { peer_id: peer_A }` 事件
- **THEN** 先更新 `DeviceManager` 中 `peer_A` 的 `is_connected=true`，然后发送 `devices-changed` Event，最后通过 Channel 转发给前端

#### Scenario: 网络事件处理顺序

- **WHEN** 后端收到 `Listening { addr }` 事件
- **THEN** 先更新 `NetManager` 的 `listen_addrs`，然后发送 `network-status-changed` Event，最后通过 Channel 转发给前端

### Requirement: list_devices Tauri Command

系统 SHALL 提供 `list_devices` Tauri Command，接受 `filter` 参数，返回 `Device` 列表。

```
#[tauri::command]
pub async fn list_devices(
    net: State<'_, NetManagerState>,
    filter: Option<String>,
) -> AppResult<DeviceListResult>
```

`filter` 取值：`"all"` | `"connected"` | `"paired"`，默认 `"connected"`。

返回值 `DeviceListResult` 包含 `devices: Vec<Device>` 和 `total: usize`。

#### Scenario: 获取已连接设备

- **WHEN** 前端调用 `invoke("list_devices", { filter: "connected" })`
- **THEN** 返回所有 `is_connected=true` 的设备列表，包含 `id`、`name`、`deviceType`、`status`、`connection`、`latency`、`isPaired` 字段

#### Scenario: 节点未启动时查询

- **WHEN** 前端调用 `invoke("list_devices")` 但 P2P 节点未启动
- **THEN** 返回错误 `"节点未启动"`

#### Scenario: 无效 filter 值

- **WHEN** 前端调用 `invoke("list_devices", { filter: "invalid" })`
- **THEN** 返回错误 `"filter 必须是 all, connected 或 paired"`

### Requirement: devices-changed Tauri Event

系统 SHALL 在以下情况发送 `devices-changed` Tauri Event：
- 新 peer 被发现
- peer 连接或断开
- peer 的 identify 信息更新
- 已配对设备列表变更

Event payload SHALL 为空对象 `{}`（前端收到后自行调用 `list_devices` 获取最新数据）。

#### Scenario: peer 连接时通知前端

- **WHEN** 后端收到 `PeerConnected` 事件并更新 `DeviceManager`
- **THEN** 发送 `devices-changed` Event，前端监听后调用 `list_devices` 获取最新列表

#### Scenario: 配对设备变更时通知前端

- **WHEN** 调用 `DeviceManager.add_paired_device()` 添加新配对设备
- **THEN** 发送 `devices-changed` Event

### Requirement: get_network_status Tauri Command

系统 SHALL 提供 `get_network_status` Tauri Command，返回当前网络状态。

```
#[tauri::command]
pub async fn get_network_status(
    net: State<'_, NetManagerState>,
) -> AppResult<NetworkStatus>
```

#### Scenario: 节点运行中查询

- **WHEN** 前端调用 `invoke("get_network_status")` 且节点已启动
- **THEN** 返回 `{ status: "running", peerId: "12D3KooW...", listenAddrs: [...], natStatus: "private", connectedPeers: 3, discoveredPeers: 5 }`

#### Scenario: 节点未启动时查询

- **WHEN** 前端调用 `invoke("get_network_status")` 且节点未启动
- **THEN** 返回 `{ status: "stopped", peerId: null, listenAddrs: [], natStatus: "unknown", connectedPeers: 0, discoveredPeers: 0 }`

### Requirement: network-status-changed Tauri Event

系统 SHALL 在以下情况发送 `network-status-changed` Tauri Event：

- 新监听地址添加（`Listening`）
- NAT 状态变更（`NatStatusChanged`）
- 中继预约成功（`RelayReservation`）

Event payload SHALL 为空对象 `{}`（前端收到后自行调用 `get_network_status` 获取最新数据）。

#### Scenario: 监听地址变更通知

- **WHEN** 后端收到 `Listening` 事件并更新 `NetManager`
- **THEN** 发送 `network-status-changed` Event

#### Scenario: NAT 状态变更通知

- **WHEN** 后端收到 `NatStatusChanged` 事件
- **THEN** 发送 `network-status-changed` Event

### Requirement: 前端 getNetworkStatus 命令封装

`src/commands/network.ts` SHALL 新增 `getNetworkStatus()` 函数：

```typescript
export async function getNetworkStatus(): Promise<NetworkStatus>
```

#### Scenario: 前端获取网络状态

- **WHEN** 前端调用 `getNetworkStatus()`
- **THEN** 返回后端 `NetManager` 的当前网络状态

### Requirement: 前端 start 函数签名变更

`src/commands/network.ts` 的 `start()` 函数 SHALL 接受 `pairedDevices: PairedDevice[]` 参数，传递给后端。

```typescript
export async function start(
  onEvent: (event: NodeEvent) => void,
  pairedDevices: PairedDevice[]
): Promise<void>
```

#### Scenario: 前端调用 start 传入已配对设备

- **WHEN** `startNetwork()` 被调用
- **THEN** 从 `useSecretStore.getState().pairedDevices` 获取已配对设备列表，传递给 `start(handleEvent, pairedDevices)`

### Requirement: 前端 listDevices 命令封装

`src/commands/network.ts` SHALL 新增 `listDevices()` 函数：

```typescript
export async function listDevices(
  filter?: "all" | "connected" | "paired"
): Promise<DeviceListResult>
```

#### Scenario: 前端获取设备列表

- **WHEN** 前端调用 `listDevices("connected")`
- **THEN** 返回后端 `DeviceManager` 中所有已连接设备的 `Device` 列表

### Requirement: 前端 network-store 简化

`src/stores/network-store.ts` SHALL 移除以下字段及相关的 `handleEvent` 逻辑：

- `peers: Map<PeerId, PeerInfo>` 及 peer 状态跟踪（PeersDiscovered、PeerConnected、PeerDisconnected、IdentifyReceived、PingSuccess）
- `listenAddrs: string[]` 及 Listening 事件处理
- `natStatus: NatStatus` 及 NatStatusChanged 事件处理
- `publicAddr: string | null` 及 RelayReservation 事件处理

替代为：

- `devices: Device[]` — 从后端获取的设备列表
- `networkStatus: NetworkStatus` — 从后端获取的网络状态
- `fetchDevices(filter?)` — 调用 `listDevices()` 更新 `devices`
- `fetchNetworkStatus()` — 调用 `getNetworkStatus()` 更新 `networkStatus`
- 监听 `devices-changed` Event，收到后自动调用 `fetchDevices()`
- 监听 `network-status-changed` Event，收到后自动调用 `fetchNetworkStatus()`

`handleEvent` SHALL 仅保留 `inboundRequest` 事件处理（转发给 pairing-store）。

#### Scenario: 收到 devices-changed 自动刷新

- **WHEN** 前端收到 `devices-changed` Event
- **THEN** 自动调用 `fetchDevices()` 更新 `devices` 列表，UI 响应式更新

#### Scenario: 收到 network-status-changed 自动刷新

- **WHEN** 前端收到 `network-status-changed` Event
- **THEN** 自动调用 `fetchNetworkStatus()` 更新 `networkStatus`，UI 响应式更新

#### Scenario: 保留 inboundRequest 事件处理

- **WHEN** 后端转发 `inboundRequest` 事件给前端
- **THEN** 前端 `handleEvent` 仍然处理该事件（转发给 pairing-store），不受简化影响
