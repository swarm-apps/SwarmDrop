## Why

移动端接收文件后，点击"打开文件夹"报错"找不到文件或文件夹"。根因是 `save_path` 在 Android 端存的是显示字符串 `"Download"`，不是有效的文件系统路径；同时 `fileUris`/`saveDirUri` 等 Android URI 信息在传输完成后被丢弃，未持久化到数据库，导致从历史记录恢复时无法正确打开文件位置。需要从类型层面区分桌面端路径和 Android 端公共目录，彻底解决跨平台保存位置的表示问题。

## What Changes

- **BREAKING** 新增 `SaveLocation` 枚举类型（Rust + TypeScript），用 tagged union 区分桌面端文件系统路径和 Android 公共目录
- 数据库 `transfer_sessions.save_path` 列改为存储 JSON 序列化的 `SaveLocation`（列名不变，内容从裸字符串改为 JSON）
- 新增数据库 migration，将现有 `save_path` 数据迁移为新格式
- Rust 端 `TransferCompleteEvent` 移除独立的 `file_uris`/`save_dir_uri` 字段，统一由 `SaveLocation` 承载
- Rust 端 `TransferHistoryItem` 的 `save_path` 字段类型从 `Option<String>` 改为 `Option<SaveLocation>`
- 前端 `TransferSession` 和相关类型使用 TypeScript 联合类型适配
- 前端 `openTransferResult` 根据 `SaveLocation.type` 分支处理，不再依赖独立的 `fileUris`/`saveDirUri` 字段
- 前端 `completeSession` 不再需要保留运行时 URI（Android 端打开时动态 resolve）

## Capabilities

### New Capabilities

- `save-location`: 跨平台保存位置的统一类型表示，涵盖 Rust 枚举定义、数据库序列化、前端类型适配和打开文件逻辑

### Modified Capabilities

（无现有 spec 需要修改）

## Impact

- **Rust 端**：`entity` crate 新增 `SaveLocation` 类型；`database/ops.rs` 序列化/反序列化逻辑调整；`transfer/progress.rs` 完成事件结构体变更；`transfer/receiver.rs` 和 `transfer/offer.rs` 适配
- **数据库**：新增 migration，SQLite `save_path` 列内容从裸字符串迁移为 JSON
- **前端**：`src/commands/transfer.ts` 类型变更；`src/lib/file-picker.ts` 打开逻辑重写；`src/stores/transfer-store.ts`、`src/components/transfer/transfer-offer-dialog.tsx`、`src/routes/_app/transfer/$sessionId.lazy.tsx` 适配
- **兼容性**：旧版本数据库的 `save_path` 数据需通过 migration 迁移，migration 需处理 NULL 和旧格式字符串
