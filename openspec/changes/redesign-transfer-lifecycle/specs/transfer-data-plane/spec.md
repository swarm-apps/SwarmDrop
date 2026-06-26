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

### Requirement: 数据帧基于 fetch_plan 连续推送，BlockRequest 仅用于补洞
transfer-data 协议 SHALL 由发送方按协商出的 `fetch_plan` 连续推送 `BlockData`，接收方以稀疏 `Ack` 推进 checkpoint，`BlockRequest` 仅用于乱序或校验失败的 gap-fill。

#### Scenario: 发送方按 fetch_plan 连续推送
- **WHEN** 数据通道完成 `Hello` 握手且 `fetch_plan` 已确定（来自 Offer/Accept 或 ResumeCommit）
- **THEN** SenderActor MUST 按 `fetch_plan` 列出的 range 顺序读取源文件、加密并连续发送 `BlockData` 帧，无需逐块等待 `BlockRequest`，背压由传输层（QUIC / yamux 接收窗口）承担

#### Scenario: 接收方稀疏确认推进 checkpoint
- **WHEN** ReceiverActor 成功写入并校验若干 block/range
- **THEN** 它 MUST 每 N 块或每 T 秒发送一次聚合 `Ack`（携带最新已确认 checkpoint offset），而 MUST NOT 对每个 block 都要求一次往返

#### Scenario: 缺口或校验失败时补洞
- **WHEN** ReceiverActor 检测到乱序缺口、校验失败或 `fetch_plan` 未覆盖的缺失 range
- **THEN** 它 MUST 在同一条数据通道上发送 `BlockRequest` 请求该 range，而 MUST NOT 为补洞另开新数据通道

### Requirement: 数据面 checkpoint 由接收方确认
系统 SHALL 由接收方在数据面传输过程中生成 checkpoint，并通过 Coordinator 投影到 DB。

#### Scenario: 接收方写入 block 后推进 checkpoint
- **WHEN** ReceiverActor 成功写入并校验一个 block/range
- **THEN** 它 MUST 报告 checkpoint event 供 Coordinator 投影到 DB；对发送方的确认采用稀疏 `Ack`（每 N 块 / T 秒）而非逐块，且 checkpoint MUST 只计入已落盘且整帧校验通过的 range

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

### Requirement: 数据面承载于单条数据通道并依赖传输层背压
transfer-data 协议 SHALL 在单条长生命周期数据通道上承载一次 (session, epoch) 传输的全部数据帧，并依赖传输层流控做背压，MUST NOT 为每个 block 或每次 `BlockRequest` 新开数据通道。

#### Scenario: 整个传输复用单条数据通道
- **WHEN** 一次 active 传输开始
- **THEN** 系统 MUST 为该 (session, epoch) 使用单条数据通道承载 `BlockData` / `Ack` / `BlockRequest` / `Finish` / `Abort`，以避免触发 muxer 开流级 silent-drop

#### Scenario: 接收方读写分离避免死锁
- **WHEN** 接收方在数据通道上同时读 `BlockData` 和写 `Ack`
- **THEN** 收发 MUST 在独立任务中进行，避免 yamux OnRead 同流双向阻塞死锁
