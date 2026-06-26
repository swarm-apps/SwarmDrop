## Context

SwarmDrop 的 P2P 通信基于 libp2p request-response 协议（CBOR 编解码），当前仅实现了 Pairing 变体。文件传输需要复用同一通道，扩展 `AppRequest`/`AppResponse` 枚举。

已完成的基础设施：
- `TransferCrypto`：XChaCha20-Poly1305 加密，BLAKE3 nonce 派生（已测试）
- `list_files` / `get_file_meta`：文件系统查询命令（已注册）
- 前端 UI：发送页、接收页、进度页、transfer-store（已实现，调用 stub 命令）
- 前端 `FileTree` 组件：headless-tree + 虚拟滚动，支持 `select`/`transfer` 两种模式

配对流程已建立的模式：入站请求到达 → 事件循环缓存 `pending_id` → emit 事件到前端 → 用户操作后通过 Tauri 命令回调 → 后端用 `pending_id` 回复。Transfer Offer 将沿用此模式。

约束：
- libp2p request-response 全局超时 `req_resp_timeout = 180s`
- CBOR 请求体上限 1MB，响应体上限 10MB
- 本次不实现分块传输（ChunkRequest/Chunk），仅打通 Offer → OfferResult 流程

## Goals / Non-Goals

**Goals:**
- 打通发送方 prepare → offer → 接收方确认 → 密钥交换的完整流程
- 扩展协议层支持 Transfer 变体，为后续分块传输预留结构
- 接收页复用 FileTree 组件展示文件树
- 发送方实现 `prepare_send`（扫描+SHA256+分配fileId）和 `start_send`（发送Offer）
- 接收方实现 `accept_receive`（生成密钥+回复）和 `reject_receive`（拒绝+原因）

**Non-Goals:**
- 分块传输（ChunkRequest/Chunk/Complete/Cancel）— 下一个 change
- 断点续传
- 多会话并发管理（本次只支持单个 Offer 流程，但数据结构预留并发能力）
- 传输进度事件推送（依赖分块传输）

## Decisions

### 1. 协议扩展方式：扩展现有枚举 vs 独立协议

**决策：扩展现有 `AppRequest`/`AppResponse` 枚举。**

理由：
- 复用同一 request-response 通道，无需额外的协议协商
- CBOR tagged enum 自动区分 Pairing 和 Transfer，零额外开销
- `swarm-p2p-core` 的 `InboundRequest` 事件已经是泛型的，无需改动 P2P 库
- 替代方案（独立 libp2p 协议 ID）增加复杂度但没有实际收益

### 2. Offer 文件列表格式：Flat list vs 嵌套树

**决策：Flat list + relativePath。**

每个文件条目包含 `fileId`、`name`、`relativePath`、`size`、`checksum`。目录不作为独立条目——前端从 relativePath 中的 `/` 分隔符重建树结构。

理由：
- 协议更紧凑，序列化/反序列化更简单
- 前端已有 `buildTreeData()` 的树构建逻辑可复用
- 接收方只需要一个新的 `buildTreeDataFromOffer()` 函数即可

### 3. 密钥交换机制：OfferResult 内嵌 vs 单独握手

**决策：密钥由接收方生成，通过 OfferResult 响应内嵌传递。**

流程：
1. 发送方发出 `TransferRequest::Offer`
2. 接收方确认后调用 `generate_key()` 生成 256-bit 随机密钥
3. 接收方通过 `TransferResponse::OfferResult { accepted: true, key: Some([u8; 32]) }` 回复
4. 双方各自用该密钥构造 `TransferCrypto`

安全性：libp2p 传输层使用 Noise 协议端到端加密，即使经过 Relay 中继，中继节点也无法看到 CBOR 内容。密钥在响应中明文传递是安全的。

替代方案考虑：
- ECDH 密钥协商：更强的前向安全性，但复杂度高，且 Noise 已提供传输层安全
- 发送方生成密钥：需要额外一轮 "密钥确认"，增加延迟

### 4. Pending Offer 管理：复用 PairingManager 模式

**决策：在 `transfer/` 模块中新建 `OfferManager`，管理 pending offer 缓存。**

结构：
- `DashMap<u64, PendingOffer>`：key 为 `pending_id`，value 含 `peer_id`、`session_id`、`files` 信息
- 发送方侧：`DashMap<String, PreparedTransfer>`：key 为 `prepared_id`，缓存 prepare_send 的结果

与 `PairingManager` 模式一致：事件循环写入缓存 → 前端操作后通过 Tauri 命令消费缓存。

### 5. prepare_send 的 SHA256 计算策略

**决策：prepare_send 同步计算所有文件的 SHA256，在 tokio blocking task 中执行。**

- 使用 `tokio::task::spawn_blocking` 避免阻塞异步运行时
- 为每个文件分配递增 `fileId`（u32）
- 返回 `PreparedTransfer { prepared_id, files, total_size }`
- `prepared_id` 用 UUID v4 生成

对大文件（>1GB），SHA256 计算可能需要数秒。前端发送页已有 loading 状态处理。

### 6. 前端接收页改造策略

**决策：新增 `buildTreeDataFromOffer()` 函数，不改动现有 `buildTreeData()`。**

- 在 `src/routes/_app/send/-file-tree.ts` 中新增函数
- 输入：`TransferFileInfo[]`，直接使用 `relativePath` 构建树
- 输出：与 `buildTreeData()` 相同的 `TreeData { dataLoader, rootChildren }`
- 接收页用 `<FileTree mode="select">` 渲染，不传 `onRemoveFile`（接收方不能删除文件）

## Risks / Trade-offs

**[180s 超时限制]** → 用户如果长时间不操作，发送方会超时。缓解：前端可在接收页显示倒计时提示；超时后发送方显示明确的 "对方未回应" 错误，可重新发送。

**[Offer 体积受 1MB 请求限制]** → 如果文件列表极长（数万个文件），CBOR 序列化可能接近 1MB。缓解：单个 `TransferFileInfo` 约 200 字节，1MB 可容纳约 5000 个文件。超大目录场景暂不处理，后续可考虑分页 Offer。

**[SHA256 计算性能]** → 大文件的 SHA256 计算耗时。缓解：`spawn_blocking` 不阻塞主线程；前端有 loading 状态。后续可考虑增量计算或仅对小文件计算校验和。

**[密钥仅存内存]** → 如果应用崩溃，密钥丢失，传输会话不可恢复。可接受：传输是短生命周期操作，用户重新发送即可。
