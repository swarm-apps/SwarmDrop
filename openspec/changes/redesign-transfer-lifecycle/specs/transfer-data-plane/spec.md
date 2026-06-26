## ADDED Requirements

### Requirement: 文件数据使用 P2P 数据通道传输
系统 SHALL 使用 `add-p2p-data-channel` 提供的通用数据通道承载文件数据帧，而不是继续用 request-response 传输每个数据块。

#### Scenario: 开始 active 数据传输
- **WHEN** transfer session 进入 active phase
- **THEN** 系统 MUST 为 transfer-data protocol 打开或接受数据通道，并在通道上执行传输帧协议

#### Scenario: 数据通道不可用
- **WHEN** 数据通道打开失败且错误为连接中断或 peer offline
- **THEN** 系统 MUST 将 session 投影为 recoverable suspended，而不是 fatal error

### Requirement: 数据通道第一帧必须是 Hello
transfer-data 数据通道 SHALL 以 `Hello` 帧开始，用于绑定 session、epoch 和 manifest。

#### Scenario: Hello 匹配当前 session
- **WHEN** 数据通道收到 `Hello { session_id, epoch, manifest_digest }` 且与本地 projection 匹配
- **THEN** 系统 MUST 接受该数据通道并继续传输

#### Scenario: Hello epoch 过期
- **WHEN** 数据通道收到的 `Hello` epoch 与当前 session epoch 不匹配
- **THEN** 系统 MUST 拒绝该数据通道，并且 MUST NOT 修改 session 状态

### Requirement: 数据帧支持缺失块拉取
transfer-data 协议 SHALL 支持接收方按缺失 block/range 请求数据，并支持发送方返回对应数据帧。

#### Scenario: 接收方请求缺失 range
- **WHEN** ReceiverActor 根据 checkpoint 计算出缺失 range
- **THEN** 它 MUST 发送 `BlockRequest` 帧请求对应 file/range

#### Scenario: 发送方返回数据
- **WHEN** SenderActor 收到有效 `BlockRequest`
- **THEN** 它 MUST 读取源文件、加密数据，并发送 `BlockData` 帧

### Requirement: 数据面 checkpoint 由接收方确认
系统 SHALL 由接收方在数据面传输过程中生成 checkpoint，并通过 Coordinator 投影到 DB。

#### Scenario: 接收方写入 block 后确认
- **WHEN** ReceiverActor 成功写入并校验一个 block/range
- **THEN** 它 MUST 报告 checkpoint event，并 MAY 发送 `Ack` 帧通知发送方当前已接收范围

#### Scenario: 数据通道中断
- **WHEN** 数据通道在文件完成前中断
- **THEN** ReceiverActor MUST flush 已完成 checkpoint，并由 `TransferCoordinator` 将 session 投影为 recoverable suspended

### Requirement: 数据面终止帧可区分完成与中止
transfer-data 协议 SHALL 区分正常完成、主动中止和异常关闭。

#### Scenario: 所有文件传输完成
- **WHEN** 接收方完成所有文件校验和 finalization
- **THEN** 系统 MUST 发送或处理 `Finish`，并由 `TransferCoordinator` 将 session 投影为 terminal completed

#### Scenario: 一方主动中止
- **WHEN** 一方因用户取消或不可恢复错误发送 `Abort`
- **THEN** 接收方 MUST 将 Abort reason 报告给 `TransferCoordinator`，由 Coordinator 决定 cancelled 或 fatal_error
