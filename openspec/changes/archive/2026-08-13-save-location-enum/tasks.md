## 1. Rust 端 SaveLocation 类型定义

- [ ] 1.1 在 `entity` crate (`src-tauri/entity/src/lib.rs`) 中定义 `SaveLocation` 枚举，实现 `Serialize`/`Deserialize`/`DeriveValueType`，`serde(tag = "type")` 序列化为 JSON
- [ ] 1.2 修改 `transfer_session.rs` 实体，`save_path` 字段类型从 `Option<String>` 改为 `Option<SaveLocation>`

## 2. 数据库 Migration

- [ ] 2.1 新增 migration `m20260310_000001_save_location_enum`，将现有 `save_path` 裸字符串数据转换为 JSON 格式（`"Download"` → `{"type":"androidPublicDir","subdir":"SwarmDrop"}`，绝对路径 → `{"type":"path","path":"..."}`，NULL 保持不变）
- [ ] 2.2 在 `migration/src/lib.rs` 中注册新 migration

## 3. Rust 端 TransferCompleteEvent 简化

- [ ] 3.1 修改 `TransferCompleteEvent` (`src-tauri/src/transfer/progress.rs`)：移除 `save_path`/`file_uris`/`save_dir_uri` 三个字段，替换为 `save_location: Option<SaveLocation>`
- [ ] 3.2 修改 `ProgressTracker::emit_complete` 方法签名，接收 `Option<SaveLocation>` 参数
- [ ] 3.3 修改 `receiver.rs` 中 `emit_complete` 调用处，根据 `FileSink` variant 构造对应 `SaveLocation`

## 4. Rust 端 accept_receive 命令适配

- [ ] 4.1 修改 `commands/transfer.rs` 的 `accept_receive` 命令，参数从 `save_path: String` 改为 `save_location: SaveLocation`
- [ ] 4.2 修改 `transfer/offer.rs` 的 `accept_and_start_receive` 方法，接收 `SaveLocation` 参数，根据 variant 构造 `FileSink`（替代 `build_file_sink` 中的 `#[cfg]` 判断）
- [ ] 4.3 修改 `database/ops.rs` 的 `create_session`，`save_path` 参数类型改为 `Option<SaveLocation>`

## 5. Rust 端 TransferHistoryItem 适配

- [ ] 5.1 修改 `database/ops.rs` 的 `TransferHistoryItem`，`save_path` 字段类型从 `Option<String>` 改为 `Option<SaveLocation>`（SeaORM 会自动通过 JSON 反序列化）

## 6. 前端类型定义更新

- [ ] 6.1 在 `src/commands/transfer.ts` 中定义 `SaveLocation` TypeScript 联合类型
- [ ] 6.2 修改 `TransferSession` 接口：`savePath` 替换为 `saveLocation?: SaveLocation`，移除 `fileUris`/`saveDirUri` 字段
- [ ] 6.3 修改 `TransferCompleteEvent` 接口：`savePath`/`fileUris`/`saveDirUri` 替换为 `saveLocation?: SaveLocation`
- [ ] 6.4 修改 `TransferHistoryItem` 接口：`savePath` 替换为 `saveLocation`
- [ ] 6.5 修改 `acceptReceive` 函数签名：`savePath: string` 改为 `saveLocation: SaveLocation`

## 7. 前端 transfer-store 适配

- [ ] 7.1 修改 `completeSession`：在 `removeAndRefresh` 前，将 `event.saveLocation` 更新到内存 session 中
- [ ] 7.2 确认 `loadHistory` 从数据库加载的 `saveLocation` 能正确映射

## 8. 前端 UI 组件适配

- [ ] 8.1 修改 `transfer-offer-dialog.tsx`：`addSession` 时传 `saveLocation` 而非 `savePath`；桌面端构造 `{ type: "path", path }` ，Android 端构造 `{ type: "androidPublicDir", subdir: "SwarmDrop" }`
- [ ] 8.2 修改 `$sessionId.lazy.tsx` 的 `historyToSession`：映射 `saveLocation` 字段
- [ ] 8.3 修改 `-transfer-item.tsx` 和 `$sessionId.lazy.tsx` 中"打开文件夹"按钮的显示条件（从检查 `savePath` 改为检查 `saveLocation`）

## 9. 前端 openTransferResult 重写

- [ ] 9.1 重写 `src/lib/file-picker.ts` 的 `openTransferResult`：根据 `saveLocation.type` 分支处理
- [ ] 9.2 `path` 分支：保持现有桌面端逻辑（`revealFile` 单文件，`openFolder` 多文件）
- [ ] 9.3 `androidPublicDir` 分支：动态 resolve `Download/{subdir}` 目录 URI，调用 `showViewDirDialog`；失败时回退到 `showViewFileDialog` 打开第一个文件

## 10. 前端设置页适配

- [ ] 10.1 修改 `-transfer-settings-section.tsx` 的保存路径显示逻辑，适配 `SaveLocation` 类型

## 11. 验证

- [ ] 11.1 `cargo build` 编译通过（desktop target）
- [ ] 11.2 `cargo test` 测试通过
- [ ] 11.3 `pnpm build` 前端构建通过（TypeScript 类型检查）
