# web-node

## ADDED Requirements

### Requirement: 浏览器节点经共享组合根装配

浏览器传输端 SHALL 通过 `crates/core` 的共享节点装配入口（`start_node`）建立，注入 Browser `EndpointProfile` 与 Web 端口实现（内存/OPFS Store、OPFS FileAccess、Web EventSink），而 **不得** 在 web crate 内独立复制 endpoint/router 装配逻辑。浏览器节点广播的 identify 协议、`agent_version` 前缀（`AGENT_PREFIX`）与协议集 SHALL 与桌面/移动端一致，使其在对端设备列表中可见、可互通。

#### Scenario: 浏览器节点对桌面可见

- **WHEN** 浏览器节点与一台桌面节点建立连接并交换 identify
- **THEN** 桌面 `DeviceManager` 的设备列表包含该浏览器节点（`agent_version` 前缀通过 `AGENT_PREFIX` 过滤），不因前缀不符被静默滤除

#### Scenario: 装配来源单一

- **WHEN** 审阅 `crates/web` 的节点建立路径
- **THEN** endpoint 与 3 协议 router 的装配来自 `core::start_node`（web 依赖 `swarmdrop-core`），web crate 内无手抄的 `build_endpoint`/`build_router` 副本

### Requirement: 浏览器支持完整配对与持久化信任

浏览器节点 SHALL 装载完整 `PairingManager` 与 3 协议 router（含 pairing），支持经 PairInvite capability 握手建立配对，并将信任设备记录（`PairedDeviceInfo`）持久化到浏览器本地存储（IndexedDB/OPFS），刷新页面后 SHALL 可凭 NodeId 重连已配对设备。入站未配对设备 SHALL 走真实 `NotPaired` 决策路径，**不得** 以合成设备目录绕过配对安全边界。

#### Scenario: 配对后刷新重连

- **WHEN** 浏览器与桌面完成 invite 配对、信任记录落库后刷新页面
- **THEN** 浏览器节点重启后从本地存储恢复信任设备，可直接凭 NodeId 重连该桌面，无需重新配对

#### Scenario: 未配对入站不再被合成放行

- **WHEN** 一个未配对设备向浏览器节点发起传输控制请求
- **THEN** 浏览器按真实配对记录判定为未配对并走既有 `NotPaired` 决策（等待用户确认或拒绝），而非因合成 `WebPeerDirectory` 返回 `Some` 被静默放行

### Requirement: 节点装配平台形态可注入

共享装配入口 SHALL 通过可注入的 `EndpointProfile`（preset / address_lookup / infra 注册开关 / relay_server 策略）与显式 `os_info` 入参容纳 Native 与 Browser 两种形态，而 **不得** 在装配函数内以硬编码 preset 或平台分支写死。同一 `build_endpoint` SHALL 同时服务生产装配与 e2e 测试装配，消除测试侧的手抄副本。

#### Scenario: 两种 profile 复用同一装配

- **WHEN** 以 `EndpointProfile::native()` 与 `EndpointProfile::browser()` 分别调用共享装配
- **THEN** 两者复用同一 `build_endpoint`/`start_node` 代码路径，仅 profile 与注入端口不同；Native 装配行为与重构前逐字段等价
