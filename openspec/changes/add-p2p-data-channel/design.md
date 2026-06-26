## Context

`swarm-p2p-core` 当前已经封装了 libp2p 的 ping、identify、Kademlia、mDNS、relay client、AutoNAT、DCUtR 和 CBOR request-response。上层通过 `NetClient<Req, Resp>` 发送请求，通过 `NodeEvent` 接收连接、发现、入站 request 等事件。

这个模型适合 RPC 风格的控制消息，但文件传输属于持续的数据面工作负载。它需要独立的通道生命周期、背压、关闭原因、错误分类、并发限制和未来 QoS 扩展。由于 `swarm-p2p-core` 是桌面端和后续 host shell 共用的网络底座，它必须保持应用无关：文件、chunk、checkpoint、恢复 epoch 等语义都属于 `crates/core`。

## Goals / Non-Goals

**Goals:**
- 在 `swarm-p2p-core` 中加入生产级、通用的数据通道 primitive。
- 保留 request-response 作为控制面 API，用于小型请求/响应消息。
- 提供 typed network error 和 close reason，供下游状态机准确分类。
- 支持上层用 libp2p `StreamProtocol` 注册应用自定义数据协议。
- 支持 per-peer 和 per-protocol 通道限制。
- 保持边界清晰：`libs/core` 传输字节，`crates/core` 定义 SwarmDrop 文件传输语义。

**Non-Goals:**
- 不在 `libs/core` 中实现文件传输、断点续传、分块、加密或 checkpoint。
- 不暴露 Tauri、React Native、文件系统、数据库或 UI 概念。
- 不替换配对、分享码、控制协议继续使用的 request-response。
- 第一版不做高级 QoS、优先级调度或带宽限速。

## Decisions

### D1: 基于 libp2p::stream::Behaviour 封装通用数据通道

最终方案选择在 `swarm-p2p-core` 中封装 libp2p 官方的 `libp2p::stream::Behaviour`（`libp2p-stream` crate），由 core 持有 `Control` 与 `IncomingStreams`，而**不是**手写自定义 `DataChannelBehaviour` + `ConnectionHandler`。

`libp2p::stream::Behaviour` 正是 rust-libp2p 官方为"应用自定义协议的裸字节流"提供的现代用法：`Control::open_stream(peer, protocol)` 返回实现 `AsyncRead + AsyncWrite` 的 `Stream`，无需实现 `ConnectionHandler`。它"需要应用持续 poll `IncomingStreams`"这一要求，可以由 `swarm-p2p-core` 自身的事件循环吸收——core 在中央 `select!` 循环（或专用 task）中持有并 poll `IncomingStreams`，把 accept 到的 `Stream` 通过 channel 转交核心运行时，对下游 `crates/core` 完全透明。这与现有 `NetClient` + `EventReceiver` 模式一致（`Control` 可 clone、跨应用共享）。

备选方案：手写自定义 `DataChannelBehaviour` + `DataChannelHandler`。拒绝原因是手写 `NetworkBehaviour` + `ConnectionHandler`（substream 状态机、背压、`poll` 契约、连接关闭清理）是 libp2p 中最易出错的部分，而它相对 `libp2p-stream` 唯一的额外收益——在 handler 层更早拒绝超额入站流、最精细的 close reason 分类——对"开流频率极低"（每次传输约 1 条数据流）的文件传输场景并不必要；这些都能在 core 持有 `Control` 的 runtime 层用 `Semaphore` 与 close-reason 归一化近似实现。

备选方案：直接对下游暴露 `libp2p::stream::Control`。拒绝原因是会把 poll 和 stream 生命周期细节泄露给应用，破坏 D2/D5 的封装目标，也让未来替换实现更困难。

备选方案：继续用 request-response 传输数据块。拒绝原因是 `request-response` 每个 substream 只承载一次 req+resp（且有 1MB/10MB 上限），不适合大文件持续数据面；rust-libp2p 曾提议的 streaming-response（issue #2657）已被关闭，官方推荐大数据走 `libp2p-stream`。

### D2: 数据通道保持协议无关

core API 只接受 `StreamProtocol` 和不透明字节流。通道知道 `peer_id`、`protocol`、`channel_id`、方向和关闭状态，但不知道应用消息格式。

SwarmDrop 可以在其上定义 `/swarmdrop/transfer-data/1`，但网络库仍然可复用于其它应用层数据协议。

### D3: 控制面和数据面配置分离

`NodeConfig` 将区分 request-response 控制协议和 data-channel 协议。request-response 保留自己的超时配置；data-channel 拥有独立的 open timeout、idle timeout、max inbound channels per peer、max outbound channels per peer 和 max channels per protocol。

这样配对等用户交互协议可以使用较长超时，而数据通道可以使用更适合传输的超时和资源限制。

### D4: 暴露 typed error 和 close reason

`swarm-p2p-core` 将把字符串式 request-response 失败替换或补充为 typed error，例如 timeout、dial failure、unsupported protocol、connection closed、cancelled、protocol negotiation failure、codec error、resource limit exceeded。

数据通道关闭和错误也应提供 typed reason，便于下游映射为 interrupted、retryable、fatal 或 local cancelled。

### D5: 数据通道纳入现有 command/event loop 模型

打开数据通道是 `NetClient` 发送给 swarm owner 的 command，和现有 `dial`、`disconnect`、`send_request`、Kademlia command 保持一致。

入站通道作为 runtime event 交给核心运行时消费，但通道对象本身不可序列化，不应放进前端 UI event payload。必要时应提供独立的 in-process receiver 或 handle registry。

## Risks / Trade-offs

- [Risk] `libp2p-stream` 仍处于 alpha，官方定性为 escape-hatch。→ Mitigation: 只依赖其稳定的 `Control::open_stream` / `Control::accept` 裸流能力，不依赖高级特性；用双节点集成测试覆盖 open/accept/close/error。
- [Risk] `IncomingStreams` 在消费跟不上时会**静默丢弃**入站 stream-open（0 容量 channel，try_send 失败即 drop），且 yamux 无开流级背压。→ Mitigation: SwarmDrop 每次传输仅开 ~1 条数据流、开流速率极低，几乎不触发；core 转交 accept 到的 stream 时用 per-stream spawn / try_send，**绝不阻塞中央 swarm 循环**（否则拖死 ping/kad/identify）；per-peer/per-protocol 上限用 runtime 层 `Semaphore` 显式拒绝并报 typed error，而非依赖底层静默丢弃。
- [Risk] 公开 API breaking change 会影响下游。→ Mitigation: 在本 change 中明确 breaking，后续用单独 change 适配 SwarmDrop 调用方。
- [Risk] 单流写背压依赖底层 muxer 接收窗口，yamux 默认 256KB 窗口在高 RTT 下严重压制吞吐。→ Mitigation: 数据面优先 QUIC（原生 per-stream 流控）；TCP/yamux 路径确保 `rust-yamux` v0.13+ 自适应窗口、不手动覆盖窗口。窗口/吞吐细节由下游 transfer change 负责，本层只提供能力并保持可配置。
- [Risk] 现有 `NodeEvent` 假设事件可序列化，但 stream handle 不可序列化。→ Mitigation: 新增非序列化 runtime event path 或 in-process channel 转交 `Stream`。
- [Risk] TCP、QUIC、relay 等底层传输的错误表现不同。→ Mitigation: 在 `swarm-p2p-core` 中归一化 close reason，利用 QUIC 的结构化 `ReadError/WriteError::ConnectionLost` 区分 connection-lost（→ 可恢复中断）、reset（→ 对端放弃）与本地 cancel，把传输细节放入 debug 字段。

## Migration Plan

1. 引入对 `libp2p::stream::Behaviour` 的封装：core 持有 `Control` + `IncomingStreams`，新增 command、event、config 和测试。
2. 更新 `NetClient` / `NodeEvent` 调用方，使其能处理 typed request-response error。
3. 保持现有 request-response 行为可用，确保配对和控制消息不受影响。
4. 增加 loopback 或双节点 smoke test：打开通道、交换字节、观察关闭事件。
5. 在后续 SwarmDrop transfer change 中采用新 data-channel API。

如果还没有上层协议依赖数据通道，回滚方式是删除新增 behaviour 和 config，保留现有 request-response 代码。

## Resolved Questions

- **half-close**（已定）：第一版不单独支持 half-close，统一为完整 close event；应用层用自己的 `Finish` 帧表达"发送完毕"。
- **DataChannel surface**（已定）：暴露裸 `AsyncRead + AsyncWrite`（libp2p `Stream`），length-prefixed 帧编解码由下游 `crates/core` 实现，`libs/core` 不引入消息边界语义。
- **per-protocol limits**（已定）：第一版静态写入 `NodeConfig`，runtime 层用 `Semaphore` 执行；运行时动态注册留作后续。
