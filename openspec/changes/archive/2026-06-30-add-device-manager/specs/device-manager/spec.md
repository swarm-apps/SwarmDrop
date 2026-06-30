## ADDED Requirements

### Requirement: DeviceManager 模块结构

系统 SHALL 提供 `DeviceManager` 结构体，位于 `src-tauri/src/device/manager.rs`，包含以下字段：
- `peers: Arc<DashMap<PeerId, PeerInfo>>` — 运行时发现的 peer
- `paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>` — 已配对设备

`DeviceManager` SHALL 在 `NetManager` 中作为 `devices` 字段持有。

#### Scenario: DeviceManager 初始化

- **WHEN** 调用 `DeviceManager::new(paired_devices: Vec<PairedDeviceInfo>)`
- **THEN** `peers` DashMap 为空，`paired_devices` DashMap 包含所有传入的已配对设备（以 PeerId 为 key）

#### Scenario: NetManager 持有 DeviceManager

- **WHEN** `NetManager::new()` 被调用
- **THEN** `NetManager` 包含一个已初始化的 `DeviceManager` 实例，可通过 `net_manager.devices()` 访问

### Requirement: PeerInfo 运行时数据结构

系统 SHALL 定义 `PeerInfo` 结构体（位于 `src-tauri/src/device/types.rs`），包含以下字段：
- `peer_id: PeerId`
- `addrs: Vec<Multiaddr>` — 多地址列表
- `agent_version: Option<String>` — 原始 agent version 字符串
- `rtt_ms: Option<u64>` — 往返延迟（毫秒）
- `is_connected: bool` — 连接状态
- `discovered_at: i64` — 发现时间（Unix 时间戳毫秒）
- `connected_at: Option<i64>` — 连接时间

#### Scenario: PeerInfo 可序列化

- **WHEN** `PeerInfo` 实例被序列化
- **THEN** 输出 JSON 使用 camelCase 字段命名（`serde(rename_all = "camelCase")`）

### Requirement: PairedDeviceInfo 持久化数据结构

系统 SHALL 定义 `PairedDeviceInfo` 结构体（位于 `src-tauri/src/device/types.rs`），包含以下字段：
- `id: String` — PeerId 字符串
- `name: String` — 设备名称
- `os: String` — 操作系统类型
- `platform: Option<String>` — 平台
- `arch: Option<String>` — 架构
- `paired_at: i64` — 配对时间戳（毫秒）

此结构 SHALL 与前端 `PairedDevice` TypeScript 类型兼容（camelCase 序列化）。

#### Scenario: 从前端数据反序列化

- **WHEN** 前端传入 `{ "id": "12D3KooW...", "name": "MacBook", "os": "macos", "pairedAt": 1700000000000 }` 的 JSON
- **THEN** 系统成功反序列化为 `PairedDeviceInfo` 实例

### Requirement: Device 统一输出类型

系统 SHALL 定义 `Device` 结构体作为对外输出的统一设备类型，包含：
- `id: String` — PeerId
- `name: String` — 设备名称（从 agent_version 解析 hostname）
- `device_type: DeviceType` — 设备类型（desktop/laptop/smartphone/tablet）
- `os: String` — 操作系统
- `status: DeviceStatus` — online/offline
- `connection: Option<ConnectionType>` — lan/dcutr/relay
- `latency: Option<u64>` — 延迟（ms）
- `is_paired: bool` — 是否已配对

#### Scenario: 从 PeerInfo 推断 Device（LAN 连接）

- **WHEN** 一个 `PeerInfo` 的 `agent_version` 为 `"swarmdrop/1.0.0; os=windows; platform=windows; arch=x86_64; host=MY-PC"`，`is_connected=true`，`addrs` 包含 `"/ip4/192.168.1.50/tcp/12345"`
- **THEN** 转换后的 `Device` 的 `name` 为 `"MY-PC"`，`device_type` 为 `Desktop`，`os` 为 `"windows"`，`status` 为 `Online`，`connection` 为 `Some(Lan)`

#### Scenario: 从 PeerInfo 推断 Device（Relay 连接）

- **WHEN** 一个 `PeerInfo` 的 `addrs` 包含 `"/ip4/47.115.172.218/tcp/4001/p2p/.../p2p-circuit/p2p/12D3KooW..."`，`is_connected=true`
- **THEN** 转换后的 `Device` 的 `connection` 为 `Some(Relay)`

#### Scenario: 从 PeerInfo 推断 Device（Dcutr 连接）

- **WHEN** 一个 `PeerInfo` 的 `addrs` 包含公网 IP 地址（非本地、不含 `/p2p-circuit/`），`is_connected=true`
- **THEN** 转换后的 `Device` 的 `connection` 为 `Some(Dcutr)`

### Requirement: 连接类型基于 Multiaddr 分析推断

连接类型推断 SHALL 基于 `PeerInfo.addrs` 中的 Multiaddr 分析，而非 RTT 启发式。推断规则：

1. 如果任意地址包含 `/p2p-circuit/` 协议段 → `Relay`
2. 如果任意地址的 IP 为私有地址（10.x.x.x、172.16-31.x.x、192.168.x.x、链路本地） → `Lan`
3. 其余情况（公网 IP 直连，通常由 DCUtR 打洞建立） → `Dcutr`

优先级：如果同时存在多种类型的地址，按 `Lan > Dcutr > Relay` 优先级取最优连接类型。

RTT（`rtt_ms`）仅作为延迟展示字段，不参与连接类型推断。

#### Scenario: 含 p2p-circuit 地址推断为 Relay

- **WHEN** `PeerInfo.addrs` 中有地址 `"/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWXxx/p2p-circuit/p2p/12D3KooWYyy"`
- **THEN** 连接类型推断为 `Relay`

#### Scenario: 含私有 IP 地址推断为 Lan

- **WHEN** `PeerInfo.addrs` 中有地址 `"/ip4/192.168.1.50/tcp/12345"` 且不含 `/p2p-circuit/`
- **THEN** 连接类型推断为 `Lan`

#### Scenario: 仅公网直连地址推断为 Dcutr

- **WHEN** `PeerInfo.addrs` 中仅有地址 `"/ip4/203.0.113.5/udp/54321/quic-v1"` 且不含 `/p2p-circuit/` 也不含私有 IP
- **THEN** 连接类型推断为 `Dcutr`

#### Scenario: 多种地址取最优连接类型

- **WHEN** `PeerInfo.addrs` 同时包含私有 IP 地址和 `/p2p-circuit/` 中继地址
- **THEN** 连接类型推断为 `Lan`（优先级最高）

### Requirement: NodeEvent 驱动更新 peers

`DeviceManager` SHALL 提供 `handle_event(&self, event: &NodeEvent<AppRequest>)` 方法，处理以下事件：

- `PeersDiscovered` → 新增或更新 peer 地址
- `PeerConnected` → 设置 `is_connected = true`，记录 `connected_at`
- `PeerDisconnected` → 设置 `is_connected = false`，清除 `rtt_ms`
- `IdentifyReceived` → 更新 `agent_version`
- `PingSuccess` → 更新 `rtt_ms`

其他事件 SHALL 被忽略。

#### Scenario: 新 peer 被发现

- **WHEN** 收到 `PeersDiscovered { peers: [(peer_A, addr_1)] }` 且 `peer_A` 不在 DashMap 中
- **THEN** DashMap 中新增 `peer_A` 条目，`is_connected=false`，`addrs=[addr_1]`，`discovered_at` 为当前时间

#### Scenario: 已知 peer 发现新地址

- **WHEN** 收到 `PeersDiscovered { peers: [(peer_A, addr_2)] }` 且 `peer_A` 已存在、`addrs` 中不包含 `addr_2`
- **THEN** `peer_A` 的 `addrs` 追加 `addr_2`

#### Scenario: peer 连接

- **WHEN** 收到 `PeerConnected { peer_id: peer_A }`
- **THEN** `peer_A` 的 `is_connected` 设为 `true`，`connected_at` 设为当前时间

#### Scenario: peer 断开

- **WHEN** 收到 `PeerDisconnected { peer_id: peer_A }`
- **THEN** `peer_A` 的 `is_connected` 设为 `false`，`rtt_ms` 清为 `None`

#### Scenario: 收到 identify 信息

- **WHEN** 收到 `IdentifyReceived { peer_id: peer_A, agent_version: "swarmdrop/1.0.0; ..." }`
- **THEN** `peer_A` 的 `agent_version` 更新为该字符串

#### Scenario: ping 成功

- **WHEN** 收到 `PingSuccess { peer_id: peer_A, rtt: 5ms }`
- **THEN** `peer_A` 的 `rtt_ms` 更新为 `5`

### Requirement: 设备查询接口

`DeviceManager` SHALL 提供 `get_devices(&self, filter: DeviceFilter) -> Vec<Device>` 方法，支持以下过滤模式：

- `All` — 返回所有已发现的 peer，标注 `is_paired` 状态
- `Connected` — 仅返回 `is_connected=true` 的 peer
- `Paired` — 仅返回已配对设备，合并运行时在线状态

#### Scenario: 查询所有设备

- **WHEN** 调用 `get_devices(DeviceFilter::All)` 且有 3 个已发现 peer（1 个已配对）
- **THEN** 返回 3 个 `Device`，其中 1 个 `is_paired=true`

#### Scenario: 查询已连接设备

- **WHEN** 调用 `get_devices(DeviceFilter::Connected)` 且有 3 个已发现 peer、其中 2 个已连接
- **THEN** 返回 2 个 `Device`，均为 `status=Online`

#### Scenario: 查询已配对设备

- **WHEN** 调用 `get_devices(DeviceFilter::Paired)` 且有 2 个已配对设备、其中 1 个在线
- **THEN** 返回 2 个 `Device`，在线的那个 `status=Online`，另一个 `status=Offline`

### Requirement: 已配对设备管理

`DeviceManager` SHALL 提供以下方法管理已配对设备：
- `is_paired(&self, peer_id: &PeerId) -> bool`
- `add_paired_device(&self, info: PairedDeviceInfo)`
- `remove_paired_device(&self, peer_id: &PeerId) -> Option<PairedDeviceInfo>`
- `get_paired_devices(&self) -> Vec<PairedDeviceInfo>`

#### Scenario: 添加已配对设备

- **WHEN** 调用 `add_paired_device(info)` 其中 `info.id = "12D3KooWXxx"`
- **THEN** `is_paired` 对该 PeerId 返回 `true`，`get_paired_devices()` 包含该设备

#### Scenario: 移除已配对设备

- **WHEN** 调用 `remove_paired_device(peer_id)` 且该 peer 已配对
- **THEN** 返回 `Some(PairedDeviceInfo)`，`is_paired` 对该 PeerId 返回 `false`

### Requirement: OsInfo 迁移到 device 模块

现有 `src-tauri/src/device.rs` 中的 `OsInfo` 结构体及其方法 SHALL 迁移到 `src-tauri/src/device/mod.rs`，保持完全相同的公开 API。

#### Scenario: OsInfo 功能不变

- **WHEN** 调用 `OsInfo::new().to_agent_version()`
- **THEN** 返回格式为 `"swarmdrop/{version}; os={os}; platform={platform}; arch={arch}; host={hostname}"` 的字符串
