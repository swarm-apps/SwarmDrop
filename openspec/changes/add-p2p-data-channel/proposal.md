## Why

SwarmDrop 当前依赖 `swarm-p2p-core` 的通用 request-response API 承载所有应用层通信。这个模型适合配对、分享码、恢复探测等小型控制消息，但不适合长期、高吞吐、需要背压和关闭语义的数据传输。

现在要重做文件传输的暂停、中断和恢复语义，需要先把 `swarm-p2p-core` 升级为具备“控制面 + 数据面”能力的通用 P2P runtime，并且不能把 SwarmDrop 文件传输业务逻辑写进网络库。

## What Changes

- 在 `swarm-p2p-core` 中新增通用数据通道能力，支持按 `StreamProtocol` 打开和接收入站字节通道。
- 新增 typed network failure / close reason，用于替代部分字符串错误。
- 为 request-response 增加 per-call options，例如单次请求超时和关联元数据。
- 新增数据通道生命周期事件：入站通道、出站打开失败、通道关闭、通道错误。
- 新增 per-peer / per-protocol 通道数量限制，避免入站流失控。
- **BREAKING**：调整 `NetClient` 和 `NodeEvent` 的公开 API，使调用方能处理 typed error 和 data-channel 事件。
- 保持数据通道通用性：`libs/core` 不引入 transfer session、file、chunk、checkpoint 等 SwarmDrop 业务概念。

## Capabilities

### New Capabilities
- `p2p-data-channel`: 通用 P2P 数据通道能力，负责打开、接收、观察和关闭应用定义的字节流。

### Modified Capabilities
- None.

## Impact

- 影响 crate：`libs/core` (`swarm-p2p-core`) 及其公开 API 调用方。
- 影响模块：runtime behaviour 组合、event loop、command system、client API、配置、错误模型、测试和示例。
- 可能新增或调整 libp2p feature，用于自定义数据通道 behaviour / connection handler。
- 下游影响：`crates/core` 和 `src-tauri` 后续需要适配 typed error，并使用新的 data-channel API。
