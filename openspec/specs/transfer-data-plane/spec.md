# transfer-data-plane Specification

## Purpose
TBD - created by archiving change redesign-transfer-lifecycle. Update Purpose after archive.
## Requirements
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

transfer-data 协议 SHALL 由发送方按协商出的 `fetch_plan` 连续推送 `BlockData`，
推送速率由**应用层 `Window` 停等流控**节制；发送方 SHALL 把「读源 + 建 bao proof」
与「写帧 + 流控」拆成两条并发路径，中间以有界队列相连，使前者的耗时藏在后者背后。

发送方 MUST NOT 对块内容做任何应用层加密——保密由传输层（Noise / QUIC-TLS）承担，
明文随 bao proof 切片一并送出（`BlockData.data` 恒空）。

`Ack` 与 `BlockRequest` 已不存在：逐块确认由 bao 逐块验签取代，缺口由续传的
`fetch_plan` 重新协商覆盖。

#### Scenario: 发送方按 fetch_plan 连续推送

- **WHEN** 数据通道完成 `Hello` 握手且 `fetch_plan` 已确定（来自 Offer/Accept 或 ResumeCommit）
- **THEN** SenderActor MUST 按 `fetch_plan` 列出的 range 顺序读取源文件、为每块生成 bao proof
  并连续发送 `BlockData` 帧，无需逐块等待对端应答；背压由应用层 `Window` 停等承担，
  而 MUST NOT 依赖传输层流控（浏览器接收侧不存在可用的传输层背压）

#### Scenario: 备块与发帧并发推进

- **WHEN** 发送方正在推送数据块
- **THEN** 读源与建 proof MUST 在一条独立的并发路径上进行，与写帧路径以容量有界的队列相连，
  且这两条路径 MUST 在同一个任务内由 `join` 驱动（MUST NOT spawn 新任务、MUST NOT split 流）

#### Scenario: 队列满时的背压

- **WHEN** 发帧路径被传输层背压顶住、备块路径已把队列填满
- **THEN** 备块路径 MUST 阻塞在入队上停止读源，而 MUST NOT 无界地把已读块囤在内存里

#### Scenario: 满窗停等

- **WHEN** 发送方在当前窗口内已推出 `WINDOW_CHUNKS` 块
- **THEN** 它 MUST 发送一帧 `Window` 并等待对端回同款之后才继续推送下一窗

### Requirement: 数据面 checkpoint 由接收方确认

系统 SHALL 由接收方在数据面传输过程中生成 checkpoint，并通过 Coordinator 投影到 DB。

#### Scenario: 接收方写入 block 后推进 checkpoint

- **WHEN** ReceiverActor 成功验签并写入一个 block/range
- **THEN** 它 MUST 报告 checkpoint 供 Coordinator 投影到 DB，且 checkpoint MUST 只计入
  已落盘且逐块验签通过的 range；落库 MUST 按固定块数节流，而 MUST NOT 每块一次

#### Scenario: 数据通道中断

- **WHEN** 数据通道在文件完成前中断
- **THEN** ReceiverActor MUST flush 已完成 checkpoint，并由 `TransferCoordinator` 将 session
  投影为 recoverable suspended

### Requirement: 数据面终止帧可区分完成与中止
transfer-data 协议 SHALL 区分正常完成、主动中止和异常关闭。

#### Scenario: 所有文件传输完成
- **WHEN** 接收方完成所有文件校验和 finalization
- **THEN** 系统 MUST 发送或处理 `Finish`，并由 `TransferCoordinator` 将 session 投影为 terminal completed

#### Scenario: 一方主动中止
- **WHEN** 一方因用户取消或不可恢复错误发送 `Abort`
- **THEN** 接收方 MUST 将 Abort reason 报告给 `TransferCoordinator`，由 Coordinator 决定 cancelled 或 fatal_error

### Requirement: 数据面承载于单条数据通道

transfer-data 协议 SHALL 在单条长生命周期数据通道上承载一次 (session, epoch) 传输的全部
数据帧，MUST NOT 为每个 block 新开数据通道。该条流 MUST 由单一路径独占顺序读写，
MUST NOT 被 split 成读写两半。

#### Scenario: 整个传输复用单条数据通道

- **WHEN** 一次 active 传输开始
- **THEN** 系统 MUST 为该 (session, epoch) 使用单条数据通道承载
  `BlockData` / `Window` / `Finish` / `Abort`，以避免触发 muxer 开流级 silent-drop

#### Scenario: 流不得被 split

- **WHEN** 收发双方需要在同一条数据通道上既读又写
- **THEN** 该流 MUST 由单一循环整流顺序读写；MUST NOT 使用 `AsyncReadExt::split`
  ——其 BiLock reader half 在 wasm 下数据到达 muxer 后不唤醒读端（native 多线程掩盖，
  浏览器单线程显形为「字节已到但读循环不推进」）

### Requirement: Finish 只在全部计划块确实发出之后写

发送方 SHALL 仅在备块路径与发帧路径**双双成功收敛**之后才发送 `Finish`。
备块路径失败时，发帧路径会因队列关闭而正常收敛——此时 MUST NOT 把它当作传输完成。

#### Scenario: 备块路径中途失败

- **WHEN** 发送方读源文件或生成 proof 失败，而此前已入队的块都已成功写出
- **THEN** 发送方 MUST NOT 发送 `Finish`，MUST 把备块路径的错误作为最终错误上抛，
  并按既有 Interrupted 路径转入可恢复态

#### Scenario: 两条路径的错误归因

- **WHEN** 两条路径都返回错误
- **THEN** 系统 MUST 上抛发帧路径的错误——备块路径此时只会报出次生的「发帧端已退出」

### Requirement: 发送端探针可分辨流水线是否满

发送端 SHALL 为两条并发路径各维护一个独立的逐阶段探针，使日志能直接判定瓶颈落在哪一侧。
单个探针 MUST NOT 横跨两条并发路径——那会破坏「各阶段之和 = 壁钟」这个判读前提。

#### Scenario: 备块路径的阶段划分

- **WHEN** 发送端推送数据块
- **THEN** 备块探针 MUST 分别记录 `read`（读源）、`proof`（生成 bao proof）、
  `enqueue`（入队阻塞 = 被发帧端顶住的时间）三段

#### Scenario: 发帧路径的阶段划分

- **WHEN** 发送端推送数据块
- **THEN** 发帧探针 MUST 分别记录 `queue`（等队列出块 = 被备块端饿着的时间）、
  `write`（写帧，含传输层背压）、`ack`（满窗等待确认）、`rest`（进度簿记与事件投递）四段

#### Scenario: 判读瓶颈

- **WHEN** 运维或开发读取真机日志
- **THEN** 备块侧 `enqueue` 占大头 MUST 可判定为「网络顶住了」，发帧侧 `queue` 占大头
  MUST 可判定为「备块跟不上」，两者都小 MUST 可判定为「流水线已满」

