## ADDED Requirements

### Requirement: 证书生成约束

系统 SHALL 使用 **ECDSA P-256** 生成自签名证书，有效期 MUST NOT 超过 14 天，且 MUST NOT
使用 RSA 密钥。这三条同时是 libp2p WebTransport spec 的 MUST 与浏览器
`serverCertificateHashes` 的准入条件——任一条不满足，浏览器会在 TLS 阶段直接拒绝，且错误
信息不指向真正的原因。

#### Scenario: 生成的证书满足浏览器准入条件

- **WHEN** 系统生成一张新证书
- **THEN** 该证书 MUST 是 X.509v3、密钥算法 MUST 是 ECDSA P-256、有效期 MUST ≤ 14 天

#### Scenario: 拒绝加载不合规的持久化证书

- **WHEN** 从持久化数据加载到一张 RSA 证书或有效期超过 14 天的证书
- **THEN** 系统 MUST 视其为不可用并重新生成，不得沿用

### Requirement: 两张证书的时间关系

系统 SHALL 始终持有两张证书：`current`（当前生效）与 `next`（未来生效）。`next` 的生效时间
MUST 等于 `current` 的过期时间——两者首尾相接，不留空档也不无谓重叠。

#### Scenario: 首次启动

- **WHEN** 没有任何持久化证书，系统在时刻 T 初始化
- **THEN** 系统 MUST 生成 `current`（T 起 14 天）与 `next`（T+14 天起 14 天）

#### Scenario: 加载时 current 已过期

- **WHEN** 从持久化数据加载，`current` 已过期但 `next` 仍在有效期内
- **THEN** 系统 MUST 将 `next` 提升为 `current`，并生成新的 `next`

#### Scenario: 加载时两张都已过期

- **WHEN** 从持久化数据加载，两张证书都已过期（例如设备关机超过 28 天）
- **THEN** 系统 MUST 丢弃两张并按首次启动的规则重新生成

### Requirement: 轮换判据与幂等

轮换状态机 SHALL 以显式传入的时刻推进，MUST NOT 在内部读取系统时钟。当传入时刻越过
`current` 的过期时间时，状态机 MUST 将 `next` 提升为 `current` 并生成新的 `next`；同一个
时刻重复推进 MUST 只轮换一次。

**时钟注入不是风格选择**：`current` 的有效期是 14 天，若状态机内部读时钟，"跨过期切换"这条
行为就无法在单元测试里验证，而项目要求护栏测试必须能红。

#### Scenario: 未到期不轮换

- **WHEN** 以 `current` 有效期内的任一时刻推进
- **THEN** 状态机 MUST 报告「无事发生」，两张证书 MUST 保持不变

#### Scenario: 跨过期轮换

- **WHEN** 以晚于 `current` 过期时间的时刻推进
- **THEN** 原 `next` MUST 成为新的 `current`、MUST 生成一张新的 `next`，且状态机 MUST 报告
  发生了轮换并给出退役的 certhash

#### Scenario: 重复推进幂等

- **WHEN** 用同一个已越过期限的时刻连续推进两次
- **THEN** 第二次 MUST 报告「无事发生」，MUST NOT 产生第二次轮换

### Requirement: 通告 certhash 集合

通告地址 SHALL 同时携带 `current` 与 `next` 的 certhash，`current` 在前。

这直接决定了旧地址的实际寿命：客户端持有 `[A, B]` 时，服务端在下一轮用的是 `B`，仍在客户端
愿意接受的集合内，故旧地址在**下一整轮**仍可用；再下一轮才失效。因此通告地址的有效期是
两个轮换周期（28 天）而非一个。

#### Scenario: 通告顺序

- **WHEN** 构造通告地址
- **THEN** 地址 MUST 依次含 `current` 与 `next` 的 certhash

#### Scenario: 旧地址在下一轮仍可拨

- **WHEN** 客户端持有轮换前的通告地址（含 `[A, B]`），服务端已轮换到 `current = B`
- **THEN** 连接 MUST 建立成功——服务端实际使用的证书哈希在客户端接受集合内

### Requirement: Noise 扩展的上报与验证

服务端 SHALL 在 Noise 握手的 `webtransport_certhashes` 扩展里带上 `current`、`next` 以及
近期已退役的 certhash。客户端 SHALL 把地址里的全部 certhash 作为期望集合传入，并 MUST 验证
服务端上报的集合覆盖了期望集合中的每一项；任一项缺失时握手 MUST 失败。

#### Scenario: 验证通过

- **WHEN** 客户端用地址里的 certhash 集合发起握手，服务端上报的集合包含它们全部
- **THEN** 握手 MUST 成功

#### Scenario: 证书哈希不符时握手失败

- **WHEN** 客户端传入的期望集合里有一项不在服务端上报的集合中
- **THEN** 握手 MUST 失败，MUST NOT 建立连接

#### Scenario: 负向路径必须有测试看守

- **WHEN** 为该行为编写护栏测试
- **THEN** MUST 包含「故意给错 hash」的用例，且该用例 MUST 在实现被改坏时变红——只测成功
  路径不满足本项要求

### Requirement: 持久化端口

证书对 SHALL 经一个由本 crate 定义、由宿主实现的持久化端口读写，格式为**多段 PEM**（有效期
本就编码在 X.509 内，不额外定义元数据格式）。轮换发生时系统 MUST 回写持久化数据。

持久化的价值是**降低通告地址变更的频率**：不持久化则每次重启 certhash 都变，持久化后最多
每 14 天变一次。

#### Scenario: 轮换后回写

- **WHEN** 发生一次轮换
- **THEN** 系统 MUST 通过持久化端口写回新的证书对

#### Scenario: 重启后 certhash 不变

- **WHEN** 进程重启且持久化的 `current` 仍在有效期内
- **THEN** 通告地址中的 certhash MUST 与重启前一致

#### Scenario: 持久化失败不中断服务

- **WHEN** 持久化端口的写入失败（磁盘满、权限不足等）
- **THEN** 系统 MUST 仅记录 warn 级日志并继续运行——内存中的证书是好的，连接照常；后果
  退化为「本次未持久化」，MUST NOT 使节点起不来或断开既有连接

#### Scenario: 持久化数据损坏

- **WHEN** 持久化的 PEM 无法解析
- **THEN** 系统 MUST 按首次启动重新生成，并记录 warn 级日志说明 certhash 将会改变

### Requirement: 轮换表达为地址事件

轮换 SHALL 通过 transport 既有的地址事件对外表达：先为旧通告地址发出 `AddressExpired`，
再为新通告地址发出 `NewAddress`。MUST NOT 为此引入额外的通知机制——上层（identify、地址
收集、bootstrap 通告）走的应当是与「网卡插拔」完全相同的路径。

#### Scenario: 轮换产生一对地址事件

- **WHEN** 一个正在监听的 transport 发生证书轮换
- **THEN** 该监听器 MUST 依次产出旧地址的 `AddressExpired` 与新地址的 `NewAddress`

#### Scenario: 既有连接不受影响

- **WHEN** 轮换发生时存在已建立的 WebTransport 连接
- **THEN** 这些连接 MUST 保持可用，MUST NOT 被断开或重建

### Requirement: 轮换时钟的推进者

轮换状态机 SHALL 由 transport 的 poll 循环推进，系统 MUST NOT 为此启动独立的后台定时任务
——多一个任务就多一处生命周期与泄漏风险。

推进 MUST NOT 退化成「每次 poll 都读一次系统时钟」：poll 在空闲连接上每秒可达上千次，而
轮换周期是 14 天。系统 SHALL 在 poll 内驱动一个固定间隔的定时器，只在它到期时读时钟并推进。
该定时器 MUST 注册 waker，使得**完全空闲时也保证被唤醒** —— 否则轮换要等到恰好有别的事情
触发一次 poll 才发生。

代价是证书可能晚换不超过一个间隔，在 14 天的尺度上无实际影响。

#### Scenario: 定时器到期时推进

- **WHEN** transport 被 poll、轮换检查定时器已到期，且当前时间已越过 `current` 的过期时间
- **THEN** 系统 MUST 在该次 poll 内完成轮换并开始产出对应的地址事件

#### Scenario: 定时器未到期不读时钟

- **WHEN** transport 被高频 poll 但检查定时器尚未到期
- **THEN** 系统 MUST NOT 读取系统时钟，也 MUST NOT 产生任何轮换相关的工作

#### Scenario: 空闲时仍会被唤醒

- **WHEN** transport 上没有任何连接活动，长时间无人主动 poll
- **THEN** 定时器 MUST 在到期时唤醒 poll，使轮换不依赖外部事件
