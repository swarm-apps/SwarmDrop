# network-status Specification

## Purpose
TBD - created by archiving change add-device-manager. Update Purpose after archive.
## Requirements
### Requirement: NetworkStatus 数据结构

系统 SHALL 定义 `NetworkStatus` 结构体（位于 `src-tauri/src/commands/mod.rs` 或 `NetManager` 内部），包含：
- `status: NodeStatus` — 节点运行状态（Stopped / Running）
- `peer_id: Option<String>` — 本节点 PeerId（运行时才有）
- `listen_addrs: Vec<String>` — 监听地址列表
- `nat_status: NatStatus` — NAT 状态（Unknown / Public / Private）
- `public_addr: Option<String>` — 公网地址（如有）
- `connected_peers: usize` — 已连接 peer 数量
- `discovered_peers: usize` — 已发现 peer 数量

#### Scenario: NetworkStatus 可序列化

- **WHEN** `NetworkStatus` 实例被序列化
- **THEN** 输出 JSON 使用 camelCase 字段命名

### Requirement: NetManager 维护网络状态

`NetManager` SHALL 持有网络状态字段，并在事件循环中更新：
- `Listening { addr }` → 追加到 `listen_addrs`
- `NatStatusChanged { old, new }` → 更新 `nat_status`
- `RelayReservation { addr }` → 更新 `public_addr`（提取中继地址）

#### Scenario: 监听地址更新

- **WHEN** 收到 `Listening { addr: "/ip4/192.168.1.100/tcp/12345" }` 事件
- **THEN** `NetManager` 的 `listen_addrs` 包含该地址

#### Scenario: NAT 状态变更

- **WHEN** 收到 `NatStatusChanged { old: Unknown, new: Private }` 事件
- **THEN** `NetManager` 的 `nat_status` 更新为 `Private`

#### Scenario: 中继预约成功

- **WHEN** 收到 `RelayReservation { addr: "/ip4/47.115.172.218/tcp/4001/p2p/.../p2p-circuit/p2p/12D3KooW..." }` 事件
- **THEN** `NetManager` 的 `public_addr` 更新为该地址

### Requirement: NetManager 提供 get_network_status 方法

`NetManager` SHALL 提供 `get_network_status(&self) -> NetworkStatus` 方法，汇总当前网络状态。

`connected_peers` 和 `discovered_peers` 计数 SHALL 从 `DeviceManager` 的 `peers` DashMap 中计算。

#### Scenario: 节点运行中查询状态

- **WHEN** 节点已启动，有 2 个监听地址、3 个已连接 peer、5 个已发现 peer
- **THEN** `get_network_status()` 返回 `status=Running`、`listen_addrs` 包含 2 个地址、`connected_peers=3`、`discovered_peers=5`

### Requirement: NetManager handle_event 处理网络事件

`NetManager` SHALL 提供 `handle_event(&self, event: &NodeEvent<AppRequest>)` 方法（或在事件循环中直接处理），更新网络状态相关字段。

此方法 SHALL 与 `DeviceManager.handle_event()` 配合使用——设备事件由 `DeviceManager` 处理，网络事件由 `NetManager` 处理。

#### Scenario: 事件分流

- **WHEN** 收到 `Listening` 事件
- **THEN** `NetManager` 处理（更新 `listen_addrs`），`DeviceManager` 忽略

#### Scenario: 设备事件不影响网络状态

- **WHEN** 收到 `PeerConnected` 事件
- **THEN** `DeviceManager` 处理（更新 peer 状态），`NetManager` 的 `listen_addrs`/`nat_status` 不变（但 `connected_peers` 计数会因 DeviceManager 更新而变化）

