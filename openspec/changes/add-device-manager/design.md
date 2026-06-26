## Context

SwarmDrop 当前的设备管理架构是"前端驱动"：后端通过 Tauri Channel 将原始 `NodeEvent` 转发给前端，前端 `network-store.ts` 维护 `peers: Map<PeerId, PeerInfo>` 并推断设备类型、连接类型等。已配对设备存储在前端 `secret-store.ts`，持久化到 Stronghold 加密 vault。

后端 `NetManager` 仅持有 `AppNetClient` + `PeerId` + `PairingManager`，不持有任何设备或网络状态。

即将实现的 MCP Server 需要后端直接访问设备列表（`list_devices` Tool）和网络状态（`get_network_status` Tool），如果这些数据全在前端，MCP 就无法独立工作。

## Goals / Non-Goals

**Goals:**

- 后端建立设备数据的单一数据源（`DeviceManager`），MCP 和 Tauri Commands 都能直接访问
- `NetManager` 维护网络状态（监听地址、NAT 状态、公网地址），为 MCP 的 `get_network_status` Tool 提供数据
- 前端 `network-store` 简化为消费后端数据，不再自己维护 peer 状态和网络状态
- 已配对设备持久化保持在前端 Stronghold，启动时注入后端
- 为后续 MCP Server 的 `list_devices` / `get_network_status` Tool 提供直接可用的数据层

**Non-Goals:**

- 不改变已配对设备的持久化位置（保持前端 Stronghold）
- 不引入 SQLite 或其他数据库（Phase 3 再考虑）
- 不实现 MCP Server 本身（这是后续变更）
- 不改变配对流程逻辑（`PairingManager` 保持不变）

## Decisions

### Decision 1: DeviceManager 作为独立模块 `src-tauri/src/device/`

**选择**：将现有 `device.rs` 升级为 `device/` 目录模块，新增 `manager.rs` 和 `types.rs`。

**替代方案**：
- 放在 `commands/` 下：会让 commands 模块过大，且 MCP 引用不方便
- 新建 `src-tauri/src/devices/` 与 `device.rs` 并存：命名混乱

**理由**：`device/` 与 `pairing/` 平级，职责清晰。MCP 模块和 Tauri Commands 都能直接 `use crate::device::DeviceManager`。

### Decision 2: 使用 DashMap 作为运行时存储

**选择**：`peers: Arc<DashMap<PeerId, PeerInfo>>` + `paired_devices: Arc<DashMap<PeerId, PairedDeviceInfo>>`

**替代方案**：
- `RwLock<HashMap>`：需要手动管理读写锁粒度，容易死锁
- `tokio::sync::RwLock<HashMap>`：异步锁在频繁更新场景下性能不如 DashMap

**理由**：DashMap 是分片并发 HashMap，适合高频读写场景（NodeEvent 频繁更新 + MCP/前端频繁查询）。无锁读、分片写，性能最优。

### Decision 3: 后端处理 NodeEvent + 转发前端

**选择**：事件循环中先调用 `DeviceManager.handle_event(&event)` 更新后端状态，然后继续通过 Channel 转发给前端。

**替代方案**：
- 仅后端处理，前端通过 Tauri Event 订阅变更：前端失去事件细节，某些 UI 交互（如 inbound request 弹窗）依赖原始事件
- 仅前端处理：回到原来的问题，MCP 无法访问

**理由**：双路处理保证后端有完整数据，前端仍能接收原始事件用于特殊 UI 逻辑（如配对请求通知）。前端的 peer 状态跟踪逻辑可以移除，但 `inboundRequest` 等事件仍需要前端处理。

### Decision 4: 前端通过 Tauri Command 获取设备列表 + Event 通知刷新

**选择**：新增 `list_devices` Tauri Command + `devices-changed` Event。前端调用 command 拉取数据，event 触发时自动刷新。

**替代方案**：
- 纯轮询：浪费资源，延迟高
- 纯推送（每次变更推送完整列表）：设备多时带宽浪费

**理由**：Command + Event 的 pull-on-notify 模式是 Tauri 推荐模式。Event 轻量（只通知"有变化"），Command 按需获取完整数据。

### Decision 5: start() 接收 paired_devices 参数

**选择**：修改 `start()` Tauri Command 签名，前端调用时传入 `pairedDevices`（从 Stronghold 读取）。

**理由**：保持 Stronghold 在前端管理，后端不直接访问 Stronghold。启动时一次性注入，配对成功后通过 `DeviceManager.add_paired_device()` 同步更新后端状态。

### Decision 6: 网络状态放在 NetManager 而非 DeviceManager

**选择**：`NetManager` 新增 `NetworkStatus` 结构体（`listen_addrs`、`nat_status`、`public_addr`），处理 `Listening`、`NatStatusChanged`、`RelayReservation` 等事件。

**替代方案**：
- 放在 `DeviceManager` 中：网络状态与设备无关，混在一起职责不清
- 保持前端处理：MCP 无法访问

**理由**：网络状态属于网络层，与网络生命周期（start/stop）同一层级。`NetManager` 管网络，`DeviceManager` 管设备，职责清晰。前端同样通过 `get_network_status` Command + `network-status-changed` Event 消费数据。

## Risks / Trade-offs

- **[双数据源不一致]** → 配对成功后前端写 Stronghold + 后端更新 DashMap，两者可能短暂不一致。缓解：配对成功的事件处理中同步更新两端，且后端数据仅用于运行时查询（重启后重新注入）。
- **[前端响应延迟]** → 从前端直接处理事件改为 Command 拉取，可能有轻微延迟。缓解：`devices-changed` Event 实时通知，前端收到后立即拉取，延迟可忽略。
- **[DashMap 内存占用]** → 大量 peer 时 DashMap 内存增长。缓解：SwarmDrop 是 P2P 文件传输工具，同时在线 peer 数量不会很多（几十个量级），内存不是问题。
- **[前端重构范围]** → `network-store.ts` 变动较大，需要仔细处理 peer 相关的组件依赖。缓解：`Device` 类型保持不变，组件层面改动小。
