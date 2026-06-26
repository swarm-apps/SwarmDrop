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

### D1: 实现自定义通用 DataChannelBehaviour

最终方案选择在 `swarm-p2p-core` 中实现自定义 `DataChannelBehaviour` 和 `DataChannelHandler`，而不是把公开 API 直接绑定到 `libp2p::stream::Behaviour`。

`libp2p::stream::Behaviour` 适合探索和 async/await 风格原型，但官方定位更接近 escape hatch，并要求应用持续 poll inbound streams。自定义 behaviour 可以让 core 自己管理入站接收、背压、关闭原因、per-peer 限制，以及与现有 command/event loop 的一致集成。

备选方案：直接暴露 `libp2p::stream::Control`。拒绝原因是会把 poll 和生命周期细节泄露给下游应用，也会让未来替换实现更困难。

备选方案：继续用 request-response 传输数据块。拒绝原因是文件传输是持续数据面负载，而 request-response 是带超时和响应大小约束的 RPC primitive。

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

- [Risk] 自定义 behaviour/handler 比直接使用 `libp2p::stream::Behaviour` 复杂。→ Mitigation: 第一版缩小到 byte-channel surface，先覆盖 open/accept/close/error 测试。
- [Risk] 公开 API breaking change 会影响下游。→ Mitigation: 在本 change 中明确 breaking，后续用单独 change 适配 SwarmDrop 调用方。
- [Risk] 背压和队列设计错误可能导致通道阻塞或丢弃。→ Mitigation: 使用有界队列、显式通道限制和确定性的 close reason。
- [Risk] 现有 `NodeEvent` 假设事件可序列化，但 stream handle 不可序列化。→ Mitigation: 新增非序列化 runtime event path 或 in-process channel。
- [Risk] TCP、QUIC、relay 等底层传输的错误表现不同。→ Mitigation: 在 `swarm-p2p-core` 中归一化 close reason，把传输细节放入 debug 字段。

## Migration Plan

1. 新增 data-channel behaviour、command、event、config 和测试。
2. 更新 `NetClient` / `NodeEvent` 调用方，使其能处理 typed request-response error。
3. 保持现有 request-response 行为可用，确保配对和控制消息不受影响。
4. 增加 loopback 或双节点 smoke test：打开通道、交换字节、观察关闭事件。
5. 在后续 SwarmDrop transfer change 中采用新 data-channel API。

如果还没有上层协议依赖数据通道，回滚方式是删除新增 behaviour 和 config，保留现有 request-response 代码。

## Open Questions

- 第一版是否需要显式支持 half-close，还是统一为完整 close event？
- `DataChannel` 应该暴露原始 `AsyncRead + AsyncWrite`，还是提供 length-prefixed frame API？
- per-protocol limits 应该静态写入 `NodeConfig`，还是允许运行时注册？
