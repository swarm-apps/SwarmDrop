## MODIFIED Requirements

### Requirement: 在线宣告周期刷新
系统 SHALL 发布结构化的在线记录（直连地址集：剔除 loopback/unspecified/多跳 circuit 等不可拨地址；合法一跳 circuit 地址集；以及至多 3 个 relay 提示——每个含 relay PeerId 与其可达地址，供对端先修复 relay 直连再拨 circuit）。刷新 SHALL 为事件驱动 + 周期兜底：本机地址集或 reservation 集变化时 MUST 立即重发；节点运行期间以不超过记录 TTL 一半的周期（TTL 300s → ≈150s）兜底重发；首次发布失败 SHALL 短退避重试而非静默等待下一周期。节点正常停止时 SHALL 保持现有 announce_offline 行为（移除在线记录）。

#### Scenario: 长时间运行记录不过期
- **WHEN** 节点连续运行 20 分钟且无任何用户操作
- **THEN** DHT 中本机在线记录始终有效，其他设备任意时刻查询均能命中

#### Scenario: 停止节点即下线
- **WHEN** 用户停止节点
- **THEN** 本机在线记录从 DHT 移除，对端在下一个重探周期后显示本机离线

#### Scenario: reservation 建立即时可见
- **WHEN** 本机新获得一个公网中继 reservation（circuit 地址集变化）
- **THEN** 在线记录立即重发（不等待周期），跨网对端在下一个重探周期即可拿到可拨地址

#### Scenario: 记录不含不可拨地址
- **WHEN** 本机地址集中存在 loopback、unspecified 或多跳 circuit 地址
- **THEN** 发布的在线记录中不包含这些地址

### Requirement: 离线设备低频重探
对处于离线（Unreachable）状态的已配对设备，系统 SHALL 以低频周期（数量级 60~90 秒，带抖动）执行重探：查询其 DHT 在线记录，命中则注册地址并重拨（直连与 circuit 地址并发）；直拨全部失败且记录含 relay 提示时，SHALL 逐个提示先确保与该 relay 的直连（触发打洞升级）再拨目标的 circuit 地址。重探失败原因 SHALL 分级记录（无记录/地址不可拨/relay 不可达），MUST NOT 静默丢弃。重探 MUST 持续到设备恢复在线或节点停止；节点启动后的首轮重探 SHALL 覆盖全部已配对设备（替代原一次性 check_paired_online）。

#### Scenario: 对端回归一分钟级被发现
- **WHEN** 一台离线的已配对设备重新上线并完成在线宣告
- **THEN** 本机在一个重探周期内（≤90 秒）发现并重连，设备状态变为在线

#### Scenario: 持续离线不震荡
- **WHEN** 某已配对设备持续离线 30 分钟
- **THEN** 本机对其状态稳定显示离线，不出现周期性的在线/离线跳变

#### Scenario: 经 relay 提示的多步恢复
- **WHEN** 跨网对端仅可经中继到达，且本机与该中继当前无直连
- **THEN** 重探先与中继建立连接，再经 circuit 地址拨通对端，设备恢复在线
