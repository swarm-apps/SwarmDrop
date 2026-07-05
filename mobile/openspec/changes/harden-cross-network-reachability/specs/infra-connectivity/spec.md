## ADDED Requirements

### Requirement: 基础设施链路收敛维持
系统 SHALL 对候选表中的每个基础设施节点（内置/自定义引导、LAN Helper、学习型中继）持续维持期望状态：连接存活、Kademlia 已接线、以及（该候选具备 relay 角色且公网可达性允许时）relay reservation 存活。任一环断开时系统 MUST 以退避策略自动重建，不得依赖一次性注册流程；重复的维持动作 MUST 幂等（已达期望状态时无副作用）。

#### Scenario: 网关挂起恢复后自动重建
- **WHEN** LAN Helper（手机 A）挂起导致与本机（电脑 B）的连接及 B 在 A 上的 reservation 丢失，随后 A 恢复且地址未变化
- **THEN** B 在退避周期内自动重连 A 并重建 reservation，B 的跨网可达地址恢复发布，全程无需重启任何节点

#### Scenario: 候选不可达时退避不风暴
- **WHEN** 某基础设施候选持续不可达 10 分钟
- **THEN** 重建尝试按退避策略降频进行，不产生高频连接风暴，候选恢复后仍能在低频周期内被重新收敛

### Requirement: reservation 丢失可感知
relay reservation 对应的 circuit listener 关闭时，系统 SHALL 上抛显式事件（与既有 reservation 建立事件成对），供收敛层触发重建；MUST NOT 仅以日志记录该状态变化。

#### Scenario: listener 关闭触发事件
- **WHEN** 与 relay 的连接断开导致 circuit listener 永久关闭
- **THEN** 业务层收到 reservation 丢失事件，其中包含对应 relay 的 PeerId

### Requirement: 学习型中继候选
系统 SHALL 在收到 identify 信息且对端 agent 标识为引导/中继基础设施节点时，将其自动纳入候选表（来源标记为 Learned，公网范围）。LanOnly 模式下经 LAN Helper 间接学到的公网中继同样 SHALL 被纳管（受公网可达性设置约束）。

#### Scenario: LanOnly 设备经网关学到公网中继
- **WHEN** LanOnly 的电脑 B 经 LAN Helper A 的 DHT/identify 认识公网中继 BOOT，且公网可达性设置为开
- **THEN** BOOT 进入 B 的候选表，B 与 BOOT 建立连接并完成 reservation，B 的在线记录获得可被跨网设备直拨的一跳 circuit 地址

### Requirement: 中继限额适配文件传输
LAN Helper 的 relay server 限额 SHALL 不因流量截断已配对设备间的传输（circuit 字节数不设上限）；电路时长上限 SHALL 仅用于回收僵尸电路（数量级半天），被时长回收的连接 MUST 能被断连宽限机制无感重建。

#### Scenario: 大文件走中继不被掐断
- **WHEN** 两台跨网设备经 LAN Helper 中继传输一个大于 64MiB 的文件
- **THEN** 传输不因中继流量限额被中断

#### Scenario: 时长回收无感重建
- **WHEN** 一条走中继的已配对设备连接达到电路时长上限被 relay server 回收
- **THEN** 双方在断连宽限期内重建连接，设备列表全程不出现离线闪烁
