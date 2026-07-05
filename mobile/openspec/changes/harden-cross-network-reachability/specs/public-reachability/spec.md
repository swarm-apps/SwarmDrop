## ADDED Requirements

### Requirement: 发现与可达性解耦的设置语义
系统 SHALL 提供独立于发现模式的公网可达性设置 `public_reachability`（默认开启）：开启时允许在已知公网中继上建立 reservation 使本机可被跨网直达；关闭时 MUST NOT 对公网范围的中继发起 reservation。`discovery_mode=LanOnly` SHALL 仅表示不主动连接内置公网引导节点，不限制经局域网协作节点学到的公网中继的使用（除非公网可达性关闭）。

#### Scenario: LanOnly 且默认可达（用户网关拓扑目标态）
- **WHEN** 电脑 B 设置 LanOnly + 公网可达性默认开，同局域网存在连接公网中继的 LAN Helper A
- **THEN** B 经 A 学到公网中继并完成 reservation，跨网设备 C 可一跳直拨 B，无需依赖打洞

#### Scenario: 严格局域网隔离
- **WHEN** 用户设置 LanOnly + 关闭公网可达性
- **THEN** B 不对任何公网中继发起连接或 reservation，跨网可达仅剩 LAN Helper 转发路径，设置文案如实说明该模式下跨网访问可能不可用

### Requirement: 可达状态可见
系统 SHALL 在本机网络状态中暴露公网可达性事实（存在活跃公网 reservation 或已确认的公网直达地址），两端 UI 的网络状态区 SHALL 据此展示"公网可达/仅局域网可达"；设备列表的在线/离线二元展示保持不变。

#### Scenario: 仅局域网可达时用户可见
- **WHEN** B 的所有公网 reservation 丢失且无公网直达地址（如网关 A 长时间离线且未恢复）
- **THEN** B 的网络状态区显示"仅局域网可达"，用户能区分"设备离线"与"跨网不可直达"
