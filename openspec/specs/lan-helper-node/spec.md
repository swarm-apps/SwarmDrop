# lan-helper-node Specification

## Purpose
TBD - created by archiving change auto-discover-lan-helper-nodes. Update Purpose after archive.
## Requirements
### Requirement: 用户可显式开启局域网协助节点
系统 SHALL 提供“本设备作为局域网协助节点”的用户开关。该能力默认关闭，只有用户显式开启后，本桌面端才提供 Kad Server 与 Relay Server 能力。

#### Scenario: 默认不提供协助节点能力
- **WHEN** 用户从未开启“本设备作为局域网协助节点”
- **THEN** 节点启动时 SHALL 不启用 Relay Server
- **AND** 节点 SHALL 不声明 `lan-helper` capability

#### Scenario: 开启协助节点能力
- **WHEN** 用户开启“本设备作为局域网协助节点”并启动网络节点
- **THEN** 节点 SHALL 以 Kad Server 模式响应 DHT 查询
- **AND** 节点 SHALL 启用 Relay Server
- **AND** 节点 SHALL 在 SwarmDrop Identify 信息中声明 `lan-helper` capability

#### Scenario: 关闭协助节点能力
- **WHEN** 用户关闭“本设备作为局域网协助节点”
- **THEN** 系统 SHALL 提示该设置需要重启网络节点后生效
- **AND** 下次启动后节点 SHALL 不再提供 Relay Server 能力

### Requirement: 局域网协助节点使用受限 Relay Server 配置
局域网协助节点 SHALL 使用面向桌面端的保守 Relay Server 限额，限制 reservation 数、circuit 数、单 peer 数、转发时长和转发字节数。

#### Scenario: Relay Server 使用默认限额
- **WHEN** 节点以局域网协助模式启动
- **THEN** Relay Server SHALL 使用 `LanHelperConfig.relay_limits` 中的限额
- **AND** 未显式配置时 SHALL 使用保守默认值

#### Scenario: 超过 reservation 限制
- **WHEN** 其他 peer 请求 reservation 且已达到最大 reservation 数
- **THEN** Relay Server SHALL 拒绝新的 reservation
- **AND** 系统 SHALL 保持现有 reservation 不受影响

#### Scenario: 超过 circuit 限制
- **WHEN** 中继 circuit 数或单个 circuit 资源超过限额
- **THEN** Relay Server SHALL 按 libp2p relay 限额拒绝或关闭超额 circuit
- **AND** 网络状态 SHALL 能反映中继服务仍在受限运行

### Requirement: 局域网协助节点声明可用局域网地址
局域网协助节点 SHALL 向 relay reservation 响应提供可用的私有局域网地址，并 SHALL 过滤不可路由地址。

#### Scenario: 监听到私有局域网地址
- **WHEN** 协助节点获得可用的私有 IPv4 局域网监听地址
- **THEN** 节点 SHALL 将该地址注册为可公告地址
- **AND** 其他设备 SHALL 能基于该地址申请 relay reservation

#### Scenario: 监听地址为通配地址
- **WHEN** 协助节点监听地址为 `/ip4/0.0.0.0/...` 或 `/ip6/::/...`
- **THEN** 节点 SHALL 不把通配地址公告给其他设备
- **AND** 系统 SHALL 尝试使用实际网卡私有地址或 mDNS 发现地址作为可用候选

#### Scenario: 监听地址为 loopback
- **WHEN** 协助节点只有 loopback 地址可用
- **THEN** 节点 SHALL 不把该地址作为 LAN Helper 可用地址公告
- **AND** 网络状态 SHALL 显示协助节点不可被局域网使用

### Requirement: 局域网协助节点能力通过 SwarmDrop Identify 声明
当协助节点能力开启时，系统 SHALL 在 SwarmDrop 的 Identify agent 信息中携带能力标记，使其他 SwarmDrop 设备可以识别该节点。

#### Scenario: Identify 包含 lan-helper capability
- **WHEN** 节点以局域网协助模式启动
- **THEN** `OsInfo::to_agent_version()` 生成的 agent 信息 SHALL 包含 `caps=lan-helper`

#### Scenario: 解析 lan-helper capability
- **WHEN** 本机收到对端 Identify 信息且其中包含 `caps=lan-helper`
- **THEN** 系统 SHALL 将该对端识别为局域网协助节点候选

#### Scenario: 非 SwarmDrop 节点不被识别为协助节点
- **WHEN** 本机收到非 SwarmDrop protocol version 或非 SwarmDrop agent 前缀的 Identify 信息
- **THEN** 系统 SHALL 不把该 peer 识别为局域网协助节点

### Requirement: 局域网协助节点状态可见
系统 SHALL 在网络状态中暴露本设备是否正在提供局域网协助能力，以及中继服务的关键运行状态。

#### Scenario: 协助节点运行中
- **WHEN** 本设备已开启局域网协助能力且网络节点正在运行
- **THEN** 网络状态 SHALL 显示本设备正在作为局域网协助节点运行
- **AND** 网络状态 SHALL 显示 relay server 已启用

#### Scenario: 协助节点未运行
- **WHEN** 用户开启了局域网协助能力但网络节点尚未启动
- **THEN** 设置页 SHALL 显示该能力将在下次启动网络节点时生效

#### Scenario: 中继服务资源变化
- **WHEN** relay reservation 或 circuit 状态发生变化
- **THEN** 系统 SHOULD 更新网络状态中的协助节点运行指标

