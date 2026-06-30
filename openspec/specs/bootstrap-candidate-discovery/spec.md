# bootstrap-candidate-discovery Specification

## Purpose
TBD - created by archiving change auto-discover-lan-helper-nodes. Update Purpose after archive.
## Requirements
### Requirement: 系统维护统一的 bootstrap 候选池
系统 SHALL 维护统一的 bootstrap/relay 候选池，合并内置公网节点、用户自定义节点和自动发现的局域网协助节点。

#### Scenario: 启动时加载内置公网节点
- **WHEN** 网络节点以默认自动发现模式启动
- **THEN** 候选池 SHALL 包含内置公网 bootstrap/relay 节点

#### Scenario: 启动时加载用户自定义节点
- **WHEN** 用户配置了自定义引导节点地址
- **THEN** 候选池 SHALL 包含这些自定义节点
- **AND** 自定义节点 SHALL 与内置节点按 PeerId 去重

#### Scenario: 运行时加入局域网协助节点
- **WHEN** 系统通过 mDNS 和 Identify 识别到局域网协助节点
- **THEN** 候选池 SHALL 增加该节点并标记来源为 `MdnsLanHelper`

### Requirement: 自动发现模式控制候选来源
系统 SHALL 提供发现模式，用于控制是否使用公网节点、局域网自动发现节点和自定义节点。

#### Scenario: 自动模式使用所有可用来源
- **WHEN** 发现模式为 `auto`
- **THEN** 系统 SHALL 使用内置公网节点、用户自定义节点和局域网协助节点

#### Scenario: 仅局域网模式不连接公网 bootstrap
- **WHEN** 发现模式为 `lanOnly`
- **THEN** 系统 SHALL 不主动连接内置公网 bootstrap 节点
- **AND** 系统 SHALL 继续使用 mDNS 和自动发现到的局域网协助节点

#### Scenario: 高级自定义节点保留
- **WHEN** 用户在高级设置中添加自定义引导节点
- **THEN** 系统 SHALL 在允许公网或自定义来源的发现模式下使用该节点

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

#### Scenario: 首次连接公网 bootstrap
- **WHEN** 本节点连接到内置公网 bootstrap peer
- **THEN** 系统 SHALL 触发 DHT bootstrap

#### Scenario: 运行时发现 LAN Helper
- **WHEN** 本节点运行时发现并连接到 LAN Helper
- **THEN** 系统 SHALL 触发 DHT bootstrap 或路由刷新
- **AND** 后续 DHT put/get/provider 查询 SHALL 能使用该 helper

#### Scenario: bootstrap 失败
- **WHEN** 某个候选 bootstrap 失败
- **THEN** 系统 SHALL 标记该候选健康状态为失败或降级
- **AND** 系统 SHALL 尝试其他可用候选

### Requirement: 网络状态展示自动候选来源
系统 SHALL 在网络状态中暴露自动发现相关信息，使 UI 能显示当前使用的候选来源和降级原因。

#### Scenario: 已发现局域网协助节点
- **WHEN** 候选池包含一个或多个 LAN Helper
- **THEN** 网络状态 SHALL 包含局域网协助节点数量
- **AND** UI SHALL 能展示“已发现局域网协助节点”

#### Scenario: 当前通过公网 bootstrap 就绪
- **WHEN** 至少一个内置或自定义公网候选已连接
- **THEN** 网络状态 SHALL 显示公网引导已连接

#### Scenario: 当前 relay reservation 就绪
- **WHEN** 任一 relay reservation 被接受
- **THEN** 网络状态 SHALL 显示中继已就绪
- **AND** 网络状态 SHALL 标明 relay peer 来源是内置、自定义或局域网协助

### Requirement: 手动地址设置降级为高级兜底
系统 SHALL 保留自定义引导节点地址能力，但默认体验 SHALL 以自动发现和状态展示为主。

#### Scenario: 用户打开网络设置
- **WHEN** 用户打开设置页网络区域
- **THEN** UI SHALL 优先展示发现模式和局域网协助节点开关
- **AND** 自定义 Multiaddr 列表 SHALL 位于高级设置区域

#### Scenario: 自动发现不可用
- **WHEN** 没有公网 bootstrap 可用且没有发现 LAN Helper
- **THEN** UI SHALL 提供添加自定义引导节点地址的入口
- **AND** UI SHALL 说明该入口用于高级网络环境或自建节点

