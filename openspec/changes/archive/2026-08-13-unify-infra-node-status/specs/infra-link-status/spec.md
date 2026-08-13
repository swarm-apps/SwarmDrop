## ADDED Requirements

### Requirement: InfraLink 是基础设施关系的唯一读模型

系统 SHALL 提供 `InfraLink` 读模型，表示**本机与某个远端之间的一段基础设施关系**（而非「一个引导节点」）。它 SHALL 是零存储投影，由 `build_infra_links` 在每次调用时现场 join 三个权威源：候选表（意图侧）、`Endpoint::watch_conns`（连接事实）、`Endpoint::watch_relays`（reservation 事实）。

`InfraLink` SHALL 包含意图侧字段（`peer_id` / `addrs` / `sources` / `roles` / `scope` / `first_seen` / `last_seen` / `removable`）、观测侧字段（`connected` / `rtt_ms` / `relay`）与策略侧字段（`ever_active` / `excluded`）。

任何观测值 SHALL NOT 被写回候选表或任何持久结构——「状态粘死」必须由结构保证，而非由调用纪律保证。

#### Scenario: 观测值不落库

- **WHEN** 某条 link 的 relay reservation 从 `Active` 变为 `Failed`
- **THEN** 候选表中该候选的任何字段不因此改变
- **AND** 下一次 `build_infra_links` 返回的 `relay` 反映 `Failed`

#### Scenario: 关系而非节点

- **WHEN** 同一个 `NodeId` 既是已配对设备又承担中继角色
- **THEN** 它同时出现在设备读模型与 `InfraLink` 列表中
- **AND** 两个列表都不因对方的存在而排除它

### Requirement: 角色正交由类型表达

`InfraLink.relay` SHALL 为 `Option<RelayLinkState>`。`None` SHALL 读作「这段关系不承担中继角色，或承担但被 `excluded` 拦下从未登记」，SHALL NOT 读作「状态未知」。

`RelayLinkState` SHALL 有且仅有三个变体：`Connecting`、`Active { circuit_addr }`、`Failed { last_error }`，与内核 `RelayState` 一一对应。`last_error` SHALL 原样保留内核下发的字符串。

系统 SHALL NOT 为纯 DHT 种子角色（`roles.relay_server == false`）合成任何失败态——内核对该角色无状态轨道。

#### Scenario: 纯 kad 种子无失败态

- **WHEN** 一条 link 的 `roles.relay_server` 为 false
- **THEN** `relay` 为 `None`
- **AND** `excluded` 为 `Some(NotARelay)`

#### Scenario: 中继失败携带原因

- **WHEN** 某中继的 reservation 建立失败
- **THEN** `relay` 为 `Some(Failed { last_error })` 且 `last_error` 是内核原文

### Requirement: 「被设置拦下」与「故障」必须可区分

系统 SHALL 提供 `InfraExclusion` 显式表达「该 link 当前不参与 relay 收敛」的原因，且 SHALL 有且仅有两个变体：`NotARelay`、`PublicReachabilityDisabled`。

`InfraExclusion` SHALL NOT 包含 `NodeNotRunning`（节点运行态由 `NetworkStatus.status` 表达，`build_network_status` 中该值恒为 `Running`）。
`InfraExclusion` SHALL NOT 包含任何基于 `DiscoveryMode` 的变体（该轴当前零行为效果，见 `bootstrap-candidate-discovery`）。

被 `excluded` 的 link SHALL 在 UI 上以中性色呈现，且其操作入口 SHALL 是「改设置」而非「重试」。

#### Scenario: 关闭公网可达性后的公网候选

- **WHEN** `public_reachability` 为 false 且某候选 `scope` 为 `Public`
- **THEN** 该 link 的 `excluded` 为 `Some(PublicReachabilityDisabled)`
- **AND** UI 呈现为中性态并提供指向设置的入口，不提供「重试」

### Requirement: 重试记账不跨 IPC

系统 SHALL NOT 通过任何 IPC 边界下发重试轮数（`attempts`）或下次重试时刻（`next_attempt_at`）。退避等待期 SHALL 在 UI 上与「连接中」合并表达。

宽限期状态机 SHALL 仅使用两个锚：候选的 `first_seen`（不可变，首次 upsert 时写）与 `InfraSupervisor` 的 `ever_active` 单调位。

#### Scenario: 状态快照不含轮数

- **WHEN** 某 link 处于第 5 轮退避重试中
- **THEN** 其 `InfraLink` 快照不含任何轮数或倒计时字段

### Requirement: ever_active 是宽限期的唯一开关

`InfraSupervisor` SHALL 为每条 relay link 维护 `ever_active: bool`：`RelayReservationAccepted` 时置位；候选被移除或节点停止时清零；用户手动重启节点时清零。

单条 link SHALL 仅在同时满足三个条件时进入「连不上」态：`ever_active == false` **且** 已观测到至少一次 `RelayLinkState::Failed` **且** `now - first_seen >= GRACE`（GRACE = 10 秒）。

已成功过的 link（`ever_active == true`）掉出 `Active` 时 SHALL NOT 吃宽限期，SHALL 立刻呈现警示与失败原因。

#### Scenario: 启动握手期不闪红

- **WHEN** 节点刚启动、某中继正在首次拨号且尚未产生 `Failed`
- **THEN** 该 link 呈现为「正在连接…」的中性态，不呈现成功色也不呈现警示色

#### Scenario: 首次拨号超时长于 GRACE 时不误报

- **WHEN** native 端首次拨号耗时 25 秒后才失败（拨号超时 30 秒）
- **THEN** 在收到 `Failed` 之前该 link 保持「正在连接…」，不因 `now - first_seen >= 10s` 单独转为「连不上」

#### Scenario: 已连上过的中继掉线立刻报警

- **WHEN** 某 link 的 `ever_active` 为 true 且 reservation 丢失
- **THEN** 立刻呈现警示态并展示 `last_error`，不等待宽限期

### Requirement: 整体网络健康是六态，且与节点生命周期正交

系统 SHALL 把「节点生命周期」与「网络健康度」表达为两条正交的轴。常驻状态位 SHALL 显示：节点非 `running` 时为生命周期文案，`running` 时为健康度结论。

健康度 SHALL 有且仅有六态：`NotRunning`、`Starting`、`Reachable`、`LanReachable`、`ConfiguredLanOnly`、`Isolated`。

每一态的常驻文案 SHALL 是**后果句**（说明「谁能不能连到你」），SHALL NOT 是无主语形容词（如「良好」「受限」「可达」）。

`LanReachable` SHALL 以中性色呈现，SHALL NOT 以警示色呈现——只有同一网络里的设备能连到你对多数用户是可用状态。

#### Scenario: 节点在跑但对外完全不可达

- **WHEN** 节点处于 `running`、全部中继处于失败态、且无已连对端
- **THEN** 常驻状态位呈现 `Isolated` 的警示态与后果句，SHALL NOT 呈现「在线」类成功文案

#### Scenario: 仅局域网可用不是故障

- **WHEN** 节点 `running`、无公网可达、但有已连对端
- **THEN** 常驻状态位呈现 `LanReachable` 的中性态

### Requirement: 部分中继失败不构成整体降级

当至少一条中继处于 `Active` 时，系统 SHALL NOT 因其余中继失败而降低整体健康度或改变常驻状态位的颜色。逐条失败 SHALL 只在诊断层呈现。

#### Scenario: 两条中继连上一条

- **WHEN** 配置了两条中继，一条 `Active` 一条 `Failed`
- **THEN** 整体健康度为 `Reachable`，常驻位为成功态
- **AND** 诊断层逐条呈现两者的真实状态

### Requirement: 报警必须同时满足三个条件

系统 SHALL 仅在同时满足以下三条时呈现警示：**非用户配置造成** ∧ **已过宽限期** ∧ **确实挡住了用户此刻的动作**。

可达性缺失 SHALL NOT 产生全局横幅；它 SHALL 在受影响的动作处（生成邀请 / 配对入口）升级为阻断级就地提示。

#### Scenario: 节点未运行时不报「引导未连接」

- **WHEN** 节点未启动
- **THEN** 提示文案说明的是「节点未运行」，SHALL NOT 把原因归给引导节点

#### Scenario: 不可达时阻断邀请生成

- **WHEN** 本机无任何公网可达地址且用户尝试生成邀请
- **THEN** 在邀请入口处就地呈现阻断级提示，说明这份邀请异地设备用不了

### Requirement: 两层信息披露

节点状态面 SHALL 只有两层：**结论层**（常驻可见）与**诊断层**（默认折叠，一次展开即看全）。

结论层 SHALL 包含：状态点 + 状态词、一句可达性后果句、已配对与在线台数、至多一个 CTA。
诊断层 SHALL 包含：逐条 `InfraLink`（状态、来源、原样 `last_error`）与本机真值（节点 ID、可达地址、监听地址、NAT、身份存放位置）。

任何构建 SHALL NOT 因视口尺寸而隐藏信息位；空间不足时 SHALL 折叠或内滚，SHALL NOT 静默丢弃。

`last_error` SHALL 原样呈现且 SHALL 提供复制入口，SHALL NOT 被翻译或改写。

#### Scenario: 矮窗口不丢信息

- **WHEN** 桌面窗口高度小于 700px
- **THEN** 节点状态面的全部信息位仍可经折叠或滚动到达

#### Scenario: 失败原因可复制

- **WHEN** 某条 link 处于失败态且携带 `last_error`
- **THEN** 诊断层展示该字符串原文并提供复制按钮

### Requirement: 只有 HostConfigured 来源的 link 提供移除入口

`InfraLink.removable` SHALL 仅在 `sources` 全部为 `HostConfigured` 时为 true。UI SHALL 仅对 `removable == true` 的 link 提供「移除」入口。

自动来源（`MdnsLanHelper` / `Learned`）SHALL NOT 提供逐条移除入口——`remove_infrastructure_peer` 会断开与该节点的全部连接（含在途传输），而自动候选会在下一次 identify 时被重新登记。关闭自动来源 SHALL 经 `auto_discover_lan_helpers` 总开关。

#### Scenario: 重叠节点不提供移除

- **WHEN** 某 LAN Helper 同时是已配对设备且正在传输文件
- **THEN** 该 link 的 `removable` 为 false，UI 不提供「移除」入口

#### Scenario: 用户添加的节点可移除

- **WHEN** 用户手动添加了一条引导节点
- **THEN** 该 link 的 `removable` 为 true 且提供移除入口

### Requirement: 平台差异经退化规则表达，不经能力字段

系统 SHALL NOT 引入跨 IPC 的「平台能力」类型（如 `NodeCapabilities`）来描述某端是否支持 NAT 探测 / mDNS / socket 监听——mDNS 是**运行时退化**（绑定失败时静默降级为不可用）而非编译期能力，从端点 profile 推断会产生错误的真值。

某端不具备某项能力时，该信息位 SHALL 整格不渲染，SHALL NOT 渲染恒定占位值。

#### Scenario: 浏览器不渲染 NAT

- **WHEN** 在 Web 端渲染节点诊断层
- **THEN** 不渲染 NAT 状态一格（autonat 在 wasm 下编译期不存在）

#### Scenario: 浏览器不渲染已发现数

- **WHEN** 在 Web 端渲染节点状态
- **THEN** 不渲染「已发现节点」计数（无 mDNS，该值恒为 0）
