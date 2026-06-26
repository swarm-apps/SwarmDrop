## Why

当前文件传输仅完成了 Offer 握手阶段（文件协商 + 密钥交换），实际的数据传输流程（分块读取 → 加密 → 发送 → 解密 → 写入 → 校验）完全缺失。前端已监听 `transfer-progress`/`transfer-complete`/`transfer-failed` 事件，但后端从未发射。本次变更实现分块传输核心数据流，使 SwarmDrop 成为真正可用的桌面端 MVP。

## What Changes

- **扩展协议类型**：在 `protocol.rs` 的 `TransferRequest`/`TransferResponse` 中添加 `ChunkRequest`、`Chunk`、`Complete`、`Cancel`、`Ack` 变体
- **新建发送方模块** (`transfer/sender.rs`)：响应 ChunkRequest，从磁盘读取文件分块 → 加密 → 返回 Chunk 响应
- **新建接收方模块** (`transfer/receiver.rs`)：主动发送 ChunkRequest 拉取分块（8 并发） → 解密 → 写入磁盘 → BLAKE3 校验
- **新建会话管理** (`transfer/session.rs`)：TransferSession 状态机，管理发送/接收方的传输生命周期
- **进度事件**：后端通过 `app.emit` 发射 `transfer-progress`、`transfer-complete`、`transfer-failed` 事件
- **双向 Cancel**：发送方和接收方均可取消传输，对端收到通知后清理资源
- **注册缺失命令**：将 `cancel_send`/`cancel_receive` 注册到 `lib.rs` 的 `generate_handler!`
- **启用 save_path**：`accept_receive` 命令的 `save_path` 参数生效，接收方在该路径下重建目录结构

## Capabilities

### New Capabilities
- `chunk-transfer`: 分块传输核心数据流，包括协议扩展、发送/接收状态机、加解密管线、进度追踪和取消机制

### Modified Capabilities

## Impact

- **协议层** (`protocol.rs`)：新增 4 个协议变体，CBOR 序列化格式变更（仅影响新版本间通信）
- **事件循环** (`network/event_loop.rs`)：新增 ChunkRequest/Complete/Cancel 消息的处理分支
- **命令层** (`commands/transfer.rs`)：新增/注册 cancel 命令，修改 accept_receive 实现
- **入口注册** (`lib.rs`)：补充 `cancel_send`/`cancel_receive` 到 `generate_handler!`
- **依赖**：无新增外部依赖，复用已有的 `chacha20poly1305`、`blake3`、`tokio`
