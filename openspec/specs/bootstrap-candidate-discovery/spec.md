# bootstrap-candidate-discovery Specification

## Purpose
TBD - created by archiving change auto-discover-lan-helper-nodes. Update Purpose after archive.
## Requirements
### Requirement: 系统维护统一的 bootstrap 候选池
系统 SHALL 维护统一的 bootstrap/relay 候选池，合并内置公网节点、用户自定义节点和自动发现的局域网协助节点。

#### Scenario: 启动时加载内置公网节点
- **WHEN** 网络节点启动
- **THEN** 候选池 SHALL 包含内置公网 bootstrap/relay 节点

#### Scenario: 启动时加载用户自定义节点
- **WHEN** 用户配置了自定义引导节点地址
- **THEN** 候选池 SHALL 包含这些自定义节点
- **AND** 自定义节点 SHALL 与内置节点按 PeerId 去重

#### Scenario: 运行时加入局域网协助节点
- **WHEN** 系统通过 mDNS 和 Identify 识别到局域网协助节点
- **THEN** 候选池 SHALL 增加该节点并标记来源为 `MdnsLanHelper`

### Requirement: 公网可达性开关控制 relay 收敛
系统 SHALL 提供「公网可达性」开关，用于控制本机是否在**持有公网地址**的候选上建立
relay reservation。该开关**只管 relay 一档**：DHT 种子（kad）角色不受它约束，否则关掉
它的用户会连「跨网还能不能找到人」一起失去。

判据是候选的 `CandidateScope`（「是否持有公网地址」），**不是**「是否不含私网地址」——
后者在混合地址候选（自建 bootstrap 跑在同一局域网、用户按内网地址加进来、identify 又并入
它的公网地址）上会把一台真·公网中继判成局域网节点，开关静默失效。

#### Scenario: 关闭后不在公网中继上建 reservation
- **WHEN** 用户关闭公网可达性
- **THEN** 系统 SHALL 不对任何持有公网地址的候选建立 relay reservation
- **AND** 系统 SHALL 仍把这些候选注册为 kad 种子
- **AND** 读模型 SHALL 对这些候选给出 `PublicReachabilityDisabled` 的排除原因，
  使 UI 能说出「是你关的开关」而不是一句无归因的「连不上」

#### Scenario: 纯局域网候选不受该开关约束
- **WHEN** 用户关闭公网可达性，且某候选只持有私网/回环地址
- **THEN** 系统 SHALL 照常对它收敛 relay reservation

#### Scenario: 高级自定义节点保留
- **WHEN** 用户在高级设置中添加自定义引导节点
- **THEN** 系统 SHALL 把它纳入候选池，并按上述闸门决定其 relay 角色

### Requirement: mDNS 发现的节点必须通过 Identify 确认
系统 SHALL 只在 mDNS 发现的 peer 通过 SwarmDrop Identify 校验后，才将其作为局域网协助节点候选使用。

#### Scenario: mDNS 发现普通 SwarmDrop 设备
- **WHEN** mDNS 发现一个普通 SwarmDrop 设备但其 Identify 未声明 `lan-helper`
- **THEN** 系统 SHALL 连接并展示该设备
- **AND** 系统 SHALL 不把该设备加入 bootstrap/relay 候选池

#### Scenario: mDNS 发现 LAN Helper
- **WHEN** mDNS 发现一个 SwarmDrop 设备且 Identify 声明 `lan-helper`
- **THEN** 系统 SHALL 把该 peer 的可用地址加入候选池
- **AND** 系统 SHALL 标记该候选具备 KadServer 和 RelayServer 角色

#### Scenario: Identify 信息协议不匹配
- **WHEN** mDNS 发现 peer 但 Identify protocol version 与本应用不匹配
- **THEN** 系统 SHALL 忽略该 peer 的 infrastructure 能力

### Requirement: 运行时注册 infrastructure peer
系统 SHALL 支持在网络节点运行期间动态注册 infrastructure peer，并触发 Kad 地址注册、连接和 relay reservation。

#### Scenario: 注册具备 KadServer 的候选
- **WHEN** 候选池新增具备 KadServer 角色的 peer
- **THEN** 系统 SHALL 将其地址加入 Kad 路由表
- **AND** 系统 SHALL 将其地址加入 Swarm 地址表

#### Scenario: 注册具备 RelayServer 的候选
- **WHEN** 候选池新增具备 RelayServer 角色的 peer 且 relay client 已启用
- **THEN** 系统 SHALL dial 该 peer
- **AND** 连接建立后 SHALL 申请 relay reservation

#### Scenario: 动态注册不阻塞事件循环
- **WHEN** 系统注册 infrastructure peer
- **THEN** 注册过程 SHALL 通过 core command/event-loop 路径执行
- **AND** 不得阻塞 ping、identify、kad 和 data-channel 入站流处理

### Requirement: 自动候选触发 DHT bootstrap

系统 SHALL 在发现新的可用 bootstrap 候选后触发 DHT bootstrap 或等价的路由刷新，使本节点加入可用 DHT 网络。

候选的健康状态 SHALL NOT 存储在候选表中。候选表 SHALL 只承载**意图**（谁该在、什么地址、什么角色、什么来源）；连接与 reservation 的事实 SHALL 分别由 `Endpoint::watch_conns` 与 `Endpoint::watch_relays` 承载，并由 `InfraLink` 读模型现场合成。

#### Scenario: 首次连接公网 bootstrap

- **WHEN** 本节点连接到内置公网 bootstrap peer
- **THEN** 系统 SHALL 触发 DHT bootstrap

#### Scenario: 运行时发现 LAN Helper

- **WHEN** 本节点运行时发现并连接到 LAN Helper
- **THEN** 系统 SHALL 触发 DHT bootstrap 或路由刷新
- **AND** 后续 DHT put/get/provider 查询 SHALL 能使用该 helper

#### Scenario: bootstrap 失败

- **WHEN** 某个候选 bootstrap 失败
- **THEN** 该候选的失败经 `watch_relays` 的 `Failed { last_error }` 表达，候选表条目不变
- **AND** 系统 SHALL 尝试其他可用候选

### Requirement: 网络状态展示自动候选来源

系统 SHALL 在网络状态中暴露自动发现相关信息，使 UI 能显示当前使用的候选来源和降级原因。

除既有聚合字段外，网络状态 SHALL 包含 `infraLinks`，使 UI 能逐条回答「这一条来自哪里、承担什么角色、现在是什么状态、为什么」。

#### Scenario: 已发现局域网协助节点

- **WHEN** 候选池包含一个或多个 LAN Helper
- **THEN** 网络状态 SHALL 包含局域网协助节点数量
- **AND** 对应的 `infraLinks` 条目的 `sources` 含 `MdnsLanHelper`

#### Scenario: 当前通过公网 bootstrap 就绪

- **WHEN** 至少一个内置或自定义公网候选已连接
- **THEN** 网络状态 SHALL 显示公网引导已连接

#### Scenario: 当前 relay reservation 就绪

- **WHEN** 任一 relay reservation 被接受
- **THEN** 网络状态 SHALL 显示中继已就绪
- **AND** 对应 `infraLinks` 条目的 `relay` 为 `Active` 并携带 circuit 地址

#### Scenario: 逐条失败可归因

- **WHEN** 某个候选的 reservation 建立失败
- **THEN** 对应 `infraLinks` 条目携带失败态与 `lastError` 原文

### Requirement: 手动地址设置降级为高级兜底
系统 SHALL 保留自定义引导节点地址能力，但默认体验 SHALL 以自动发现和状态展示为主。

#### Scenario: 用户打开网络设置
- **WHEN** 用户打开设置页网络区域
- **THEN** UI SHALL 优先展示公网可达性和局域网协助节点开关
- **AND** 自定义 Multiaddr 列表 SHALL 位于高级设置区域

#### Scenario: 自动发现不可用
- **WHEN** 没有公网 bootstrap 可用且没有发现 LAN Helper
- **THEN** UI SHALL 提供添加自定义引导节点地址的入口
- **AND** UI SHALL 说明该入口用于高级网络环境或自建节点

### Requirement: 自动发现模式控制候选来源

系统 SHALL 提供发现模式，用于控制是否使用公网节点、局域网自动发现节点和自定义节点。

⚠️ **该要求当前未实现。** 经核实，`DiscoveryMode` 在生产代码中零消费：全仓对该枚举无任何 `match` / `matches!`，只有构造与回显；`create_candidate_manager` 无条件收录 host 配置的全部引导节点；`wants_reservation` 不看它；桌面与移动的清单构造函数同样忽略它。两条既有测试互相矛盾，其中断言「LanOnly 不应加载」的那条跑在空清单上，属空跑通过。

该要求 SHALL 由一个独立的变更处理（删除该轴，或补齐实现并定义它与 `public_reachability` 的语义边界）。本轮 SHALL NOT 基于 `DiscoveryMode` 引入任何新逻辑——特别地，`InfraExclusion` 不得包含基于它的变体。

#### Scenario: 自动模式使用所有可用来源

- **WHEN** 发现模式为 `auto`
- **THEN** 系统 SHALL 使用内置公网节点、用户自定义节点和局域网协助节点

#### Scenario: 仅局域网模式的行为待定

- **WHEN** 发现模式为 `lanOnly`
- **THEN** 当前实现与 `auto` 无差别；该行为的最终定义由独立变更给出

### Requirement: 候选 scope 由候选表单点推断

`BootstrapCandidateManager::upsert` SHALL 在内部按**合并后的全部地址**推断 `CandidateScope`，SHALL NOT 接受调用方传入的 scope。

当前三个调用方给出三种 scope（启动路径硬编码 `Public`、运行时意图用地址推断、局域网协助路径硬编码 `Lan`），而 `upsert` 对 scope 是直接覆盖、对 roles 是累加。结果是一个既被用户手填又被 identify 认出的节点，scope 会在 `Lan` / `Public` 之间翻转，而 `wants_reservation` 直接吃 scope，使该候选在收敛环里时进时出。

#### Scenario: 二次 upsert 不翻转 scope

- **WHEN** 一个含私网地址的候选先由用户手动登记、随后经 identify 被再次 upsert
- **THEN** 其 `scope` 保持由合并后地址推断的结果，不被调用方覆盖

#### Scenario: 角色累加而 scope 重算

- **WHEN** 同一候选先以 kad 角色登记、后以 relay 角色登记
- **THEN** `roles` 两者皆为 true，`scope` 按合并后的全部地址重新推断

### Requirement: 候选表记录首次登记时刻

`BootstrapCandidate` SHALL 携带 `first_seen`，在首次 upsert 时写入且此后不可变。它是宽限期状态机的时间锚（见 `infra-link-status`）。

`last_seen` SHALL 保持现有语义（每次 upsert 刷新，用于重置退避）。

#### Scenario: 重新发现不重置 first_seen

- **WHEN** 某候选因 helper 重启而被 mDNS 重新发现并 upsert
- **THEN** `last_seen` 刷新而 `first_seen` 不变

