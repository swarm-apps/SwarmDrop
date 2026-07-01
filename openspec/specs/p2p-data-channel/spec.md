# p2p-data-channel Specification

## Purpose
TBD - created by archiving change add-p2p-data-channel. Update Purpose after archive.
## Requirements
### Requirement: 通用数据通道 API
`swarm-p2p-core` SHALL 提供通用数据通道 API，用于按 libp2p protocol name 打开和接受应用定义的字节流。

#### Scenario: 打开出站数据通道
- **WHEN** 调用方使用已注册协议向已连接 peer 打开数据通道
- **THEN** 系统 MUST 返回一个绑定 peer、protocol、channel ID 和 outbound 方向的数据通道 handle

#### Scenario: 拒绝不支持的数据协议
- **WHEN** 调用方使用未注册或不支持的协议打开数据通道
- **THEN** 系统 MUST 返回 typed unsupported-protocol error

### Requirement: 入站数据通道事件
`swarm-p2p-core` SHALL 以 runtime event 形式暴露入站数据通道，并且不要求应用 UI 事件序列化 stream handle。

#### Scenario: 接收入站通道
- **WHEN** 远端 peer 使用已注册 inbound protocol 打开数据通道
- **THEN** 系统 MUST 发出 inbound data-channel event，包含 peer ID、protocol、channel ID 和可消费的 channel handle

#### Scenario: 入站通道不由 UI poll
- **WHEN** 入站数据通道被接受
- **THEN** 系统 MUST 在 runtime event path 中管理 stream handle，并且 MUST NOT 要求前端或序列化事件消费者 poll stream

### Requirement: 数据通道生命周期可观测性
`swarm-p2p-core` SHALL 为数据通道打开、关闭和错误结果发出 typed lifecycle 信息。

#### Scenario: 通道正常关闭
- **WHEN** 数据通道正常到达 end-of-stream
- **THEN** 系统 MUST 为该 channel ID 报告 normal close reason

#### Scenario: 通道因连接丢失失败
- **WHEN** 底层 peer connection 在数据通道完成前关闭
- **THEN** 系统 MUST 为该 channel ID 报告 typed connection-closed channel error

#### Scenario: 通道被本地取消
- **WHEN** 本地调用方取消正在打开或已活跃的数据通道
- **THEN** 系统 MUST 报告 typed local-cancelled outcome，而不是泛化为普通网络失败

### Requirement: 数据通道限制
`swarm-p2p-core` SHALL 对活跃数据通道执行可配置的 per-peer 和 per-protocol 限制。

#### Scenario: Peer 超过入站通道限制
- **WHEN** 某个 peer 尝试打开超过 per-peer 配置上限的入站数据通道
- **THEN** 系统 MUST 拒绝超额通道，并报告 typed resource-limit error

#### Scenario: 协议超过活跃通道限制
- **WHEN** 某个协议的活跃通道数量达到配置上限
- **THEN** 系统 MUST 拒绝该协议的新通道，直到容量恢复

### Requirement: Request-response 调用选项
`swarm-p2p-core` SHALL 支持 request-response 操作的 per-call options，同时保留现有默认行为。

#### Scenario: 请求使用自定义 timeout
- **WHEN** 调用方发送 request-response 消息并提供自定义 timeout
- **THEN** 系统 MUST 对该 outbound request 使用自定义 timeout，而不是只使用全局默认值

#### Scenario: 请求失败返回 typed error
- **WHEN** outbound request-response 操作失败
- **THEN** 系统 MUST 返回 typed network failure kind，例如 timeout、dial failure、unsupported protocol、connection closed、cancelled、codec error 或 unknown

### Requirement: 应用无关的数据通道边界
`swarm-p2p-core` SHALL 保持数据通道 primitive 独立于 SwarmDrop transfer 语义。

#### Scenario: 数据通道只暴露传输元信息
- **WHEN** 数据通道 handle 被创建
- **THEN** 它 MUST 只暴露 peer ID、protocol、channel ID、方向等传输元信息，并且 MUST NOT 暴露 file、transfer session、chunk、checkpoint 或 UI status 字段

