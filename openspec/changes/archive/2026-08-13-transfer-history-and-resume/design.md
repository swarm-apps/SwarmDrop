## Context

SwarmDrop 当前已完成 Offer 握手、分块传输（sender/receiver）、加密/解密、进度追踪等核心传输功能。但传输记录仅存在前端内存中（Zustand `transfer-store`），后端不持久化任何状态。

现有代码基础：
- **数据库实体已定义**: `entity` crate 中 `transfer_sessions` + `transfer_files` 两张表，含 bitmap checkpoint 字段
- **Migration 已就绪**: SeaORM Entity First 模式的 `m20260228_000001_init` migration
- **传输协议已实现**: `TransferRequest`（Offer/ChunkRequest/Complete/Cancel）和 `TransferResponse`（OfferResult/Chunk/Ack）
- **传输模块已实现**: `transfer/sender.rs`、`transfer/receiver.rs`、`transfer/offer.rs`、`transfer/crypto.rs`、`transfer/progress.rs`
- **前端 UI 已实现**: 发送页、接收页、传输列表页、transfer-store（内存）

缺失的是 DB 集成层和断点续传协议。

## Goals / Non-Goals

**Goals:**
- 在 Tauri setup() 中初始化 SeaORM + SQLite，执行 migration
- 传输生命周期事件触发 DB 写入（创建记录、更新进度、完成/失败/取消）
- 接收方 bitmap checkpoint 持久化（每 10 chunk 刷写一次）
- 断点续传协议（ResumeRequest/ResumeAccept/ResumeReject）
- 应用启动时的会话状态清理
- 传输历史查询 API（Tauri commands）
- 前端启动时从 DB 加载历史记录

**Non-Goals:**
- 文件级别的选择性恢复（v1 all-or-nothing）
- 传输限速/带宽控制
- 多并发传输会话（v1 收发各一个）
- 传输历史数据导出
- 云端同步传输历史

## Decisions

### 1. 数据库初始化位置：Tauri setup()

**选择**: 在 `lib.rs` 的 `setup()` closure 中初始化 `DatabaseConnection`，注入为 `tauri::State<DatabaseConnection>`。

**理由**: setup() 在所有 command handler 之前执行，保证 DB 在任何传输操作前就绪。Tauri State 是线程安全的（`Arc<T>`），无需手动管理连接生命周期。

**替代方案**: lazy_static / OnceCell 全局变量 — 但不符合 Tauri 依赖注入模式，且测试困难。

### 2. DB 写入策略：传输模块内直接写

**选择**: 在 `transfer/sender.rs`、`transfer/receiver.rs`、`offer.rs` 的关键生命周期点直接调用 SeaORM 写入。

**理由**: 传输模块已持有 `AppHandle`（可提取 State），生命周期事件（accept offer、chunk received、complete、fail、cancel）在这些模块中自然发生。引入独立的 DB 写入层（如 actor/channel）会增加复杂度但无明显收益。

**替代方案**: 独立的 DB actor — 增加代码量，对于 SQLite 单连接场景无并发优势。

### 3. Checkpoint 刷新频率：每 10 chunk

**选择**: 接收方每接收 10 个 chunk 执行一次 `UPDATE transfer_files SET completed_chunks = ?, transferred_bytes = ?`。

**理由**:
- 256KB × 10 = 2.5MB，崩溃最大丢失可接受
- SQLite 单次 UPDATE 开销 ~1ms，每 10 chunk 一次对传输速度影响可忽略
- 暂停/网络断开时立即刷写（不等 10 chunk 周期）

### 4. 断点续传协议设计：接收方主导

**选择**: 接收方检测到 `paused` 会话后发起 `ResumeRequest`，发送方被动响应。

**理由**: 接收方持有 checkpoint 信息，知道哪些 chunk 需要重传。发送方无状态（不持久化 checkpoint），只需验证文件完整性。

### 5. 发送方 `failed` 状态可恢复

**选择**: 发送方 app 重启后 `transferring` → `failed`，但收到 ResumeRequest 时仍可恢复为 `transferring`。仅 `cancelled` 是不可逆终态。

**理由**: 发送方重启导致的 `failed` 是被动状态（连接断开），并非用户意图。接收方可能仍在等待恢复。

### 6. 前端历史加载：启动时一次性加载

**选择**: 前端 transfer-store 在 `setupTransferListeners()` 时调用 Tauri command 从 DB 加载历史记录。

**理由**: 传输历史量通常不大（数百条），全量加载到内存再由前端过滤排序是最简单方案。后续如有性能问题再改为分页查询。

## Risks / Trade-offs

| 风险 | 缓解措施 |
|------|----------|
| SQLite 单连接瓶颈 | 传输操作本身就是串行的（收发各一个），DB 写入频率很低（每 10 chunk 一次），不是瓶颈 |
| Bitmap 刷写中 app 崩溃导致数据不一致 | 崩溃最多丢失 10 chunk（2.5MB）进度。下次启动时从 DB bitmap 恢复，安全但不完美 |
| 协议变更向后不兼容 | ResumeRequest 是新增变体，旧版本收到会返回错误。用户双方升级后即可使用 |
| BLAKE3 校验和计算耗时（大文件） | 恢复时需要重新计算发送方文件的校验和。1GB 文件约需 1-2 秒。可接受，因为恢复不是高频操作 |
| 前端全量加载历史 | 初始方案简单。若历史量增长到数千条，需改为分页 + 虚拟列表 |
