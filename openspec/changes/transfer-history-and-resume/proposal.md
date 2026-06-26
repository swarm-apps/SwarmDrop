## Why

当前传输记录仅存储在前端内存中（Zustand `transfer-store`），应用重启后所有传输历史丢失。此外，传输中断（网络断开、应用退出）后无法恢复，必须重新传输整个文件。这两个问题严重影响用户体验，尤其是大文件传输场景。

数据库实体（`transfer_sessions`、`transfer_files`）和 migration 已就绪，但尚未集成到运行时流程中。需要将 SeaORM + SQLite 接入 Tauri 后端，在传输生命周期中持久化记录，并实现基于 bitmap checkpoint 的断点续传协议。

## What Changes

### 数据库集成
- 在 Tauri `setup()` 中初始化 SeaORM `DatabaseConnection`，执行 migration，注入 Tauri State
- 传输生命周期事件（offer 接受、chunk 完成、传输完成/失败/取消）触发 DB 写入
- 接收方每 N 个 chunk 刷新 `completed_chunks` bitmap 到 DB（断点续传基础）
- 应用启动时清理过期会话（paused 超过 7 天 → 标记 failed）

### 断点续传协议
- **BREAKING**: 扩展 `TransferRequest`/`TransferResponse` 枚举，新增 `ResumeRequest`/`ResumeAccept`/`ResumeReject` 变体
- 接收方检测到未完成会话时发起 `ResumeRequest`（含每个文件的 bitmap checkpoint）
- 发送方校验文件 BLAKE3 checksum，决定接受或拒绝（`FileModified`/`SessionNotFound`/`SenderCancelled`）
- 恢复后仅传输缺失的 chunk（bitmap 为 0 的位）

### 传输历史 API
- 新增 Tauri command：查询历史列表（分页）、删除历史记录、获取会话详情
- 前端启动时从 DB 加载历史到 `transfer-store`
- 前端传输列表页展示持久化历史（含分页、按状态过滤）

### 暂停/恢复 UI 流程
- 前端暂停按钮 → 后端标记 session 为 `paused`，刷新 checkpoint
- 前端恢复按钮 → 后端发起 `ResumeRequest` 协议握手
- 发送方重启后收到 `ResumeRequest` 可从 `failed` 状态恢复

## Capabilities

### New Capabilities
- `transfer-persistence`: 传输生命周期与 SQLite 数据库的集成，包括 DB 初始化、记录写入时机、checkpoint 刷新策略、启动清理逻辑
- `transfer-resume`: 断点续传协议扩展，包括 ResumeRequest/ResumeAccept/ResumeReject 协议、bitmap checkpoint 恢复、文件变更检测、发送方重启恢复
- `transfer-history-api`: 传输历史查询 API，包括 Tauri command 定义、前端 store 集成、历史列表 UI 数据流

### Modified Capabilities
<!-- 无已有 spec 需要修改 -->

## Impact

### 后端（Rust）
- `src-tauri/src/lib.rs` — 添加 SeaORM 初始化、migration 执行、DatabaseConnection 注入
- `src-tauri/src/protocol.rs` — 新增 ResumeRequest/ResumeAccept/ResumeReject 协议变体
- `src-tauri/src/network/event_loop.rs` — 处理 Resume 协议消息、传输事件触发 DB 写入
- `src-tauri/src/transfer/receiver.rs` — checkpoint bitmap 刷新、恢复逻辑
- `src-tauri/src/transfer/sender.rs` — ResumeRequest 处理、文件变更校验
- `src-tauri/src/transfer/session.rs` — 状态机扩展（paused、failed→transferring）
- `src-tauri/src/commands/transfer.rs` — 新增历史查询/删除命令、暂停/恢复命令
- 新增 `src-tauri/src/database/mod.rs` — DB 连接初始化模块

### 前端（TypeScript/React）
- `src/stores/transfer-store.ts` — 启动时从 DB 加载、传输事件同步 DB
- `src/commands/transfer.ts` — 新增历史查询、暂停/恢复 command wrappers
- `src/routes/_app/transfer/` — 历史列表分页、状态过滤

### 依赖
- `sea-orm`（已在 Cargo.toml）、`sea-orm-migration`（已在 migration crate）
- 无新外部依赖
