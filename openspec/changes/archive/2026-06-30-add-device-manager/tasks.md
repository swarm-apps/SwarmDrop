## 1. 模块结构与依赖

- [x] 1.1 `Cargo.toml` 新增 `dashmap = "6.1"` 和 `chrono = "0.4"` 依赖
- [x] 1.2 将 `src-tauri/src/device.rs` 迁移为 `src-tauri/src/device/mod.rs`（保持 `OsInfo` 及其 API 不变）
- [x] 1.3 创建 `src-tauri/src/device/types.rs`，定义 `PeerInfo`、`PairedDeviceInfo`、`Device`、`DeviceType`、`DeviceStatus`、`ConnectionType`、`DeviceFilter`、`DeviceListResult`
- [x] 1.4 创建 `src-tauri/src/device/manager.rs`，定义 `DeviceManager` 结构体（`peers: Arc<DashMap>`、`paired_devices: Arc<DashMap>`）
- [x] 1.5 更新 `src-tauri/src/lib.rs` 中的模块声明（`mod device;` 保持不变，验证编译通过）

## 2. DeviceManager 核心实现

- [x] 2.1 实现 `DeviceManager::new(paired_devices: Vec<PairedDeviceInfo>)` 构造函数
- [x] 2.2 实现 `handle_event(&self, event: &NodeEvent<AppRequest>)` — 处理 PeersDiscovered、PeerConnected、PeerDisconnected、IdentifyReceived、PingSuccess
- [x] 2.3 实现辅助函数：`extract_hostname()`、`extract_os()`、`infer_device_type()`（解析 agent_version），`infer_connection_type(addrs)` 基于 Multiaddr 分析（p2p-circuit → Relay，私有 IP → Lan，公网直连 → Dcutr，多种地址取最优）
- [x] 2.4 实现 `get_devices(&self, filter: DeviceFilter) -> Vec<Device>` 统一查询接口（All / Connected / Paired 三种过滤模式）
- [x] 2.5 实现已配对设备管理方法：`is_paired()`、`add_paired_device()`、`remove_paired_device()`、`get_paired_devices()`

## 3. NetManager 网络状态管理

- [x] 3.1 定义 `NetworkStatus` 结构体（`status`、`peer_id`、`listen_addrs`、`nat_status`、`public_addr`、`connected_peers`、`discovered_peers`）和相关枚举（`NodeStatus`、`NatStatus`）
- [x] 3.2 修改 `NetManager` 结构体，新增 `devices: DeviceManager` 字段 + 网络状态字段（`listen_addrs: Vec<Multiaddr>`、`nat_status`、`public_addr: Option<Multiaddr>`）
- [x] 3.3 修改 `NetManager::new()`，接收 `paired_devices` 参数并创建 `DeviceManager`
- [x] 3.4 添加 `NetManager::devices(&self)` 和 `NetManager::get_network_status(&self)` 方法
- [x] 3.5 实现 `NetManager::handle_event()` — 处理 Listening（追加 listen_addrs）、NatStatusChanged（更新 nat_status）、RelayReservation（更新 public_addr）

## 4. start() 命令修改

- [x] 4.1 修改 `start()` Tauri Command 签名，新增 `paired_devices: Vec<PairedDeviceInfo>` 参数
- [x] 4.2 修改 `start()` 事件循环：先调用 `NetManager.handle_event()`（网络事件），再调用 `DeviceManager.handle_event()`（设备事件），再根据事件类型 emit 对应 Event（`devices-changed` 或 `network-status-changed`），最后通过 Channel 转发
- [x] 4.3 将 `NetManager` 中的网络状态字段改为可共享（`Arc<RwLock>` 或使用内部可变性），以便事件循环中可修改

## 5. Tauri Command 新增

- [x] 5.1 实现 `list_devices` Tauri Command（接受 `filter` 参数，从 `DeviceManager.get_devices()` 读取）
- [x] 5.2 实现 `get_network_status` Tauri Command（从 `NetManager.get_network_status()` 读取）
- [x] 5.3 在 `src-tauri/src/lib.rs` 的 `invoke_handler` 中注册 `list_devices` 和 `get_network_status`
- [x] 5.4 验证后端编译通过（`cargo build`）

## 6. 前端命令层修改

- [x] 6.1 修改 `src/commands/network.ts` 的 `start()` 函数签名，新增 `pairedDevices` 参数并传递给 `invoke("start", { channel, pairedDevices })`
- [x] 6.2 新增 `src/commands/network.ts` 的 `listDevices(filter?)` 函数封装
- [x] 6.3 新增 `src/commands/network.ts` 的 `getNetworkStatus()` 函数封装

## 7. 前端 network-store 简化

- [x] 7.1 移除 `peers: Map` 字段和 `handleEvent` 中的 peer 状态跟踪逻辑（PeersDiscovered、PeerConnected、PeerDisconnected、IdentifyReceived、PingSuccess）
- [x] 7.2 移除 `listenAddrs`、`natStatus`、`publicAddr` 字段和 `handleEvent` 中对应的事件处理（Listening、NatStatusChanged、RelayReservation）
- [x] 7.3 新增 `devices: Device[]` 字段和 `fetchDevices(filter?)` 方法
- [x] 7.4 新增 `networkStatus: NetworkStatus` 字段和 `fetchNetworkStatus()` 方法
- [x] 7.5 修改 `startNetwork()`：从 `useSecretStore.getState().pairedDevices` 获取已配对设备，传入 `start(handleEvent, pairedDevices)`
- [x] 7.6 添加 `devices-changed` Event 监听，收到后自动调用 `fetchDevices()`
- [x] 7.7 添加 `network-status-changed` Event 监听，收到后自动调用 `fetchNetworkStatus()`
- [x] 7.8 `handleEvent` 仅保留 `inboundRequest` 事件处理（转发给 pairing-store）
- [x] 7.9 迁移或移除前端的设备推断函数（`inferDeviceType`、`inferConnectionType`、`peerToDevice`、`parseAgentVersion`）

## 8. UI 层适配

- [x] 8.1 更新设备列表页面组件（`devices.lazy.tsx`），从新的 `network-store.devices` 读取数据，替换原来的 `selectNearbyDevices` 等选择器
- [x] 8.2 更新网络状态展示组件，从 `network-store.networkStatus` 读取数据，替换原来的 `listenAddrs`、`natStatus` 等字段
- [x] 8.3 验证设备卡片组件 `DeviceCard` 仍然可以正常渲染（`Device` 类型字段对照检查）
- [x] 8.4 验证前端编译通过（`pnpm build`）

## 9. 端到端验证

- [x] 9.1 `cargo build` 后端编译通过
- [x] 9.2 `pnpm build` 前端编译通过
- [x] 9.3 `pnpm tauri dev` 启动应用，验证网络启动、设备发现、设备列表展示、网络状态展示正常
