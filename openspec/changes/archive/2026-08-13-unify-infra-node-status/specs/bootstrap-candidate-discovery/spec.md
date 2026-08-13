## MODIFIED Requirements

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

## ADDED Requirements

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
