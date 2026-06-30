## Context

SwarmDrop 的文件传输功能已完成 Offer 握手阶段（文件协商 + 密钥交换），加密基础设施（XChaCha20-Poly1305）和前端 UI 框架也已就绪。但实际的数据传输流（分块读取 → 加密 → 网络传输 → 解密 → 写入磁盘）完全缺失。

现有代码基础：
- `transfer/crypto.rs` — TransferCrypto（encrypt_chunk / decrypt_chunk），确定性 nonce 派生
- `transfer/offer.rs` — OfferManager（prepare / send_offer / accept / reject），DashMap 缓存
- `transfer/fs.rs` — list_files / get_file_meta
- `protocol.rs` — TransferRequest::Offer / TransferResponse::OfferResult（分块变体为注释占位）
- `network/event_loop.rs` — 已处理 Offer 入站事件
- 前端 transfer-store 已监听 transfer-progress/complete/failed 事件但后端从未发射

约束：
- libp2p request-response CBOR 响应上限 10MB，请求上限 1MB
- 复用已有 `AppRequest`/`AppResponse` 枚举，不新增 libp2p 协议通道
- 不引入新的外部 Rust 依赖

## Goals / Non-Goals

**Goals:**
- 实现接收方拉取模式的分块传输（ChunkRequest → Chunk）
- 256KB 分块 + 8 并发拉取
- 端到端加密（复用已有 TransferCrypto）
- 进度追踪 + 速度计算 + ETA，通过 Tauri 事件推送前端
- 双向 Cancel（发送方/接收方均可取消）
- 分块级重试（3 次，指数退避）
- BLAKE3 文件校验

**Non-Goals:**
- 断点续传（需要持久化 Checkpoint，后续迭代）
- 传输历史持久化（需要 SQLite，后续迭代）
- 自动接收 / 多设备同时发送
- 传输限速

## Decisions

### 1. 在 OfferManager 中集成传输，而非新建 TransferManager

**选择**: 扩展现有 `OfferManager` → 重命名为 `TransferManager`，在其中管理发送/接收会话

**替代方案**: 新建独立的 `TransferManager`，与 `OfferManager` 平级

**理由**: OfferManager 已持有 `client: AppNetClient` 和 PreparedTransfer 缓存，Offer 握手和分块传输是同一个传输会话的不同阶段。拆分两个 Manager 会导致 PreparedTransfer 需要在 Manager 间传递，增加复杂度。

### 2. 接收方主动拉取（Receiver-Pull）

**选择**: 接收方发送 ChunkRequest，发送方响应 Chunk

**替代方案**: 发送方主动推送所有分块

**理由**:
- 接收方控制并发度和传输节奏
- 天然支持后续断点续传扩展（接收方知道缺哪些块）
- 避免发送方 overwhelm 接收方（流控由接收方决定）

### 3. 发送方在事件循环中同步响应 ChunkRequest

**选择**: event_loop 收到 ChunkRequest 后，读取文件块 → 加密 → 通过 `send_response(pending_id, ...)` 回复

**替代方案**: event_loop 将 ChunkRequest 转发到独立的发送任务

**理由**: libp2p request-response 模式要求 InboundRequest 通过 `send_response(pending_id, ...)` 回复。pending_id 是一次性的，必须在事件循环中直接处理。文件读取在 spawn_blocking 中，不会阻塞异步运行时。

### 4. 接收方通过独立 tokio 任务驱动拉取

**选择**: accept_receive 时 spawn 一个后台 tokio 任务，用 `tokio::sync::Semaphore` 控制 8 并发 ChunkRequest

**替代方案**: 使用 futures::stream::buffer_unordered

**理由**: Semaphore 模型更直观，支持动态调整并发度，且每个 ChunkRequest 是独立的 `client.send_request()` 调用，天然适合 task-per-request。

### 5. 进度事件节流 200ms

**选择**: 每 200ms 最多推送一次 transfer-progress 事件

**替代方案**: 每个分块完成都推送

**理由**: 8 并发 × 256KB 在局域网下可达 50MB/s 以上，即每秒约 200 个分块完成。不节流会导致前端渲染负担过重。200ms 节流 = 每秒 5 次更新，用户体验流畅且开销可控。

### 6. Cancel 使用 CancellationToken 而非协议消息

**选择**: 本地取消使用 `tokio_util::sync::CancellationToken`，同时发送 Cancel 协议消息通知对端

**理由**: CancellationToken 可以优雅地取消所有并发的 ChunkRequest 任务，对端收到 Cancel 消息后清理自己的会话状态。两种机制配合覆盖所有场景。

### 7. 文件写入使用 .part 临时文件

**选择**: 接收方先写入 `filename.part`，所有块完成后校验通过再重命名为最终文件名

**理由**: 避免传输中断后留下不完整文件，用户只会看到完整的文件或 .part 文件（可在下次启动时清理）。

## Risks / Trade-offs

**[单会话限制]** → 初期仅支持同时一个传输会话，多会话管理复杂度高。若用户需要多个传输，排队等待即可。后续可通过 DashMap 的 session 隔离轻松扩展。

**[大文件内存]** → 8 并发 × 256KB = 2MB 内存缓冲，加密后 ≈ 4MB。对大文件友好，不会 OOM。但 BLAKE3 校验在文件完成后需要重新读取整个文件——可接受，因为是流式 hash 不占内存。

**[网络中断]** → libp2p 的 request-response 在连接断开时会返回 OutboundFailure。当前不做自动重连续传（断点续传为 Non-Goal），直接标记为 Failed。用户需重新发送。

**[平台兼容性]** → 文件路径处理使用 `path-slash` crate 统一为 Unix 风格 `/` 分隔符，跨平台重建目录结构。已在 offer.rs 中验证可用。
