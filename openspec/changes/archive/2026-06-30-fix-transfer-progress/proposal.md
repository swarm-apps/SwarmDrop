## Why

当前 `transfer-progress` 事件存在两个缺陷：(1) 发送方（SendSession）从未发射进度事件，UI 只能看到"传输中"无进度直到突然完成；(2) 事件只追踪 `current_file` 和 `completed_files` 计数，前端无法展示每个文件各自的状态（待传输/传输中/已完成）。

## What Changes

- **发送方进度追踪**：给 `SendSession` 增加 `ProgressTracker`，在 `handle_chunk_request` 处理完每个分块后上报进度，包含速度、ETA、当前文件等信息
- **每文件独立状态**：在 `TransferProgressEvent` 中增加 `files` 字段（`Vec<FileProgressInfo>`），每个文件携带 `status`（pending/transferring/completed）和 `transferred` 字节数
- **前端同步更新**：`TransferProgressEvent` TypeScript 类型定义和 `transfer-store` 的 `TransferSession` 补充 per-file 进度字段
- **传输详情页增强**：在传输详情页的文件列表中展示每个文件的独立状态和进度

## Capabilities

### New Capabilities

- `send-progress`: 发送方进度追踪——`SendSession` 集成 `ProgressTracker`，响应 ChunkRequest 时上报分块进度、文件完成状态
- `per-file-progress`: 每文件独立进度——`TransferProgressEvent` 扩展 per-file 状态字段，前后端类型同步，详情页 UI 展示

### Modified Capabilities


## Impact

- **Rust 后端**：`src-tauri/src/transfer/sender.rs`（增加 ProgressTracker）、`src-tauri/src/transfer/progress.rs`（扩展事件结构体）、`src-tauri/src/network/event_loop.rs`（发送方 ChunkRequest 处理后触发进度上报）
- **前端类型**：`src/commands/transfer.ts`（TransferProgressEvent 类型扩展）
- **前端状态**：`src/stores/transfer-store.ts`（TransferSession 补充 per-file 字段）
- **前端 UI**：`src/routes/_app/transfer/$sessionId.lazy.tsx`（传输详情页文件列表增强）
