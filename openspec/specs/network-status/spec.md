# network-status Specification

## Purpose
TBD - created by archiving change add-device-manager. Update Purpose after archive.
## Requirements
### Requirement: NetworkStatus 数据结构

系统 SHALL 在 `crates/core/src/network/mod.rs` 定义 `NetworkStatus` 结构体（**不在** `src-tauri`——业务核心已整体迁入 `crates/core`），包含：

- `status: NodeStatus` — 节点运行状态（Stopped / Running）
- `peer_id: Option<NodeId>` — 本节点 NodeId（运行时才有）
- `listen_addrs: Vec<Addr>` — 监听地址列表
- `nat_status: NatStatus` — NAT 状态。该枚举 SHALL 只有 `Public | Unknown` 两个变体（刻意没有 `Private`：唯一写入点是 native 的 autonat，且只在探测成功时置 `Public`）。它 SHALL 经 serde 序列化为 camelCase 字符串（`"public"` / `"unknown"`），SHALL NOT 经 `format!("{:?}")` 跨任何 IPC 边界
- `public_addr: Option<Addr>` — 公网地址（如有）
- `connected_peers: usize` — 已连接的 SwarmDrop 对端数量
- `discovered_peers: usize` — 已发现的 SwarmDrop 对端数量
- `relay_ready` / `public_reachable` / `public_reachability_enabled` / `relay_peers` / `bootstrap_connected` / `discovery_mode` / `auto_discover_lan_helpers` / `local_lan_helper_enabled` / `local_lan_helper_running` / `relay_server_enabled` / `lan_helper_advertised_addrs` / `lan_helper_count` / `bootstrap_candidate_count` / `candidate_sources` / `relay_source` — 既有聚合字段，全部保留
- `infra_links: Vec<InfraLink>` — **新增**，逐条基础设施关系的完整状态

`relay_ready` / `relay_peers` / `bootstrap_candidate_count` / `candidate_sources` / `lan_helper_count` SHALL 在 core 内部改为从 `infra_links` 派生，使事实源收敛到一处；它们的对外字段与语义 SHALL 保持不变。

`bootstrap_connected` 与 `discovered_peers` SHALL 保持现有实现——它们的口径是扫描全部已连 peer 的 agent 前缀，与候选表是不同集合。

#### Scenario: NetworkStatus 可序列化

- **WHEN** `NetworkStatus` 实例被序列化
- **THEN** 输出 JSON 使用 camelCase 字段命名

#### Scenario: NAT 状态跨边界保持 camelCase

- **WHEN** `NatStatus::Public` 经桌面 tauri-specta 或移动 uniffi 边界下发
- **THEN** 前端收到的值为 `"public"`，而非 Debug 格式的 `"Public"`

#### Scenario: 派生字段与逐条状态一致

- **WHEN** `infra_links` 中至少一条的 `relay` 为 `Active`
- **THEN** `relay_ready` 为 true 且 `relay_peers` 包含该条的 `peer_id`

### Requirement: NetManager 维护网络状态

`NetManager` SHALL 持有网络状态字段，并在事件循环中更新：

- `Listening { addr }` → 追加到 `listen_addrs`
- `NatStatusChanged { old, new }` → 更新 `nat_status`
- `RelayReservationAccepted { relay }` → 经 `watch_relays` 反映到对应 `InfraLink`

事件循环 SHALL 订阅 `Endpoint::watch_relays()`，并在其变更时发布网络状态。没有该订阅，`RelayState::Connecting` 与 `RelayState::Failed { last_error }` 无任何 NetEvent 对应物，原生端将永远观测不到它们。

事件循环 SHALL NOT 在每次 `PingSuccess` 时发布全量网络状态与设备列表——`ping_interval` 为 30 秒，若干已连 peer 即产生持续的全量推送，而 `NetworkStatus` 现已携带 `infra_links` 数组。

#### Scenario: 监听地址更新

- **WHEN** 收到 `Listening { addr: "/ip4/192.168.1.100/tcp/12345" }` 事件
- **THEN** `NetManager` 的 `listen_addrs` 包含该地址

#### Scenario: NAT 状态变更

- **WHEN** 收到 `NatStatusChanged { old: Unknown, new: Public }` 事件
- **THEN** `NetManager` 的 `nat_status` 更新为 `Public`

#### Scenario: 中继进入连接中可被观测

- **WHEN** 某中继开始拨号，内核将其 `RelayState` 置为 `Connecting`
- **THEN** 原生端在无任何 NetEvent 的情况下经 `watch_relays` 订阅收到网络状态更新
- **AND** 对应 `InfraLink.relay` 为 `Connecting`

#### Scenario: 中继失败原因可达前端

- **WHEN** 某中继拨号失败，内核将其置为 `Failed { last_error }`
- **THEN** 前端收到的 `InfraLink.relay` 携带该 `last_error` 原文

### Requirement: NetManager 提供 get_network_status 方法

`NetManager` SHALL 提供 `get_network_status(&self) -> NetworkStatus` 方法，汇总当前网络状态。

`connected_peers` 和 `discovered_peers` 计数 SHALL 从 `DeviceManager` 计算，并 SHALL 继续使用 `is_swarmdrop_agent` 白名单判据。SHALL NOT 改为「排除基础设施节点」的黑名单判据——局域网协助节点本身就是 SwarmDrop 设备，可能同时是已配对设备，把它从设备读模型排除会使用户的另一台设备因为提供协助而消失。

`infra_links` SHALL 由 `build_infra_links` 现场计算，SHALL NOT 缓存。

#### Scenario: 节点运行中查询状态

- **WHEN** 节点已启动，有 2 个监听地址、3 个已连接 peer、5 个已发现 peer
- **THEN** `get_network_status()` 返回 `status=Running`、`listen_addrs` 包含 2 个地址、`connected_peers=3`、`discovered_peers=5`

#### Scenario: 局域网协助节点同时出现在两处

- **WHEN** 某已配对设备同时被识别为局域网协助节点
- **THEN** 它既出现在设备列表中，也出现在 `infra_links` 中
- **AND** `connected_peers` 计入它

### Requirement: NetManager handle_event 处理网络事件

`NetManager` SHALL 提供 `handle_event(&self, event: &NodeEvent<AppRequest>)` 方法（或在事件循环中直接处理），更新网络状态相关字段。

此方法 SHALL 与 `DeviceManager.handle_event()` 配合使用——设备事件由 `DeviceManager` 处理，网络事件由 `NetManager` 处理。

#### Scenario: 事件分流

- **WHEN** 收到 `Listening` 事件
- **THEN** `NetManager` 处理（更新 `listen_addrs`），`DeviceManager` 忽略

#### Scenario: 设备事件不影响网络状态

- **WHEN** 收到 `PeerConnected` 事件
- **THEN** `DeviceManager` 处理（更新 peer 状态），`NetManager` 的 `listen_addrs`/`nat_status` 不变（但 `connected_peers` 计数会因 DeviceManager 更新而变化）

### Requirement: MCP 网络状态与人面同源

桌面 MCP server 的 `get_network_status` 工具 SHALL 从同一份 `NetworkStatus` 投影，SHALL NOT 硬编码任何字段。

具体地：`status` SHALL 反映 `NetworkStatus.status` 的真实值（当前实现硬编码为 `"running"`，使 server instructions 中「先调用本工具确认节点已启动」的指引完全失效）；`nat_status` SHALL 经 serde 序列化而非 `format!("{:?}")`；返回值 SHALL 包含 `infraLinks`。

#### Scenario: 节点未运行时 agent 能判别

- **WHEN** 节点未启动，AI agent 调用 `get_network_status`
- **THEN** 返回的 `status` 为 `"stopped"`

#### Scenario: agent 可读到逐条基础设施状态

- **WHEN** 节点运行且有一条中继处于失败态
- **THEN** `get_network_status` 返回值的 `infraLinks` 中该条携带失败态与 `lastError`

### Requirement: 托盘状态反映网络健康度

桌面托盘 SHALL 反映网络健康度，而非仅反映「`start()` 曾返回成功」。

托盘是应用窗口关闭后唯一可见的状态面，也是用户最信任的一处（系统级）。它 SHALL 在整体健康度进入 `Isolated` 时呈现区别于正常运行的状态。

#### Scenario: 节点在跑但完全不可达

- **WHEN** 节点 `running`、全部中继失败、无已连对端，且应用窗口已关闭
- **THEN** 托盘状态不呈现为正常「在线」

