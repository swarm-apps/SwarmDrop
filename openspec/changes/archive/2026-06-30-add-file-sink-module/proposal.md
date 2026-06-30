## Why

接收方 `receiver.rs` 中存在大量直接文件 I/O 操作（创建目录、创建 .part 文件、写入分块、BLAKE3 校验、重命名），与 `file_source/path_ops.rs` 中已有的 `write_chunk`、`verify_hash` 存在代码重复。更关键的是，当前接收端硬编码了桌面端路径操作，Android 端接收文件保存到公共目录（SAF/MediaStore）无法复用现有代码。需要一个对称的写入抽象层来解决这两个问题。

## What Changes

- 新建 `file_sink` 模块（`src-tauri/src/file_sink/`），作为接收端文件写入的抽象层，与 `file_source`（读）形成对称架构
- 定义 `FileSink` 枚举：`Path`（桌面直接写本地路径）+ `AndroidPublicDir`（Android SAF/MediaStore 公共目录发布）
- 提供统一接口：创建 .part 文件、写入分块、BLAKE3 校验并最终化、清理临时文件
- 重构 `receiver.rs`：将所有直接文件 I/O 委托给 `FileSink`，消除重复代码
- 消除 `receiver.rs` 中与 `path_ops` 重复的 `write_chunk_at_offset` 和 `verify_checksum` 函数

## Capabilities

### New Capabilities

- `file-sink`: 接收端文件写入抽象层，统一处理桌面路径写入和 Android 公共目录发布，提供 .part 临时文件管理、分块写入、校验最终化等能力

### Modified Capabilities

<!-- 无现有 spec 需要修改 -->

## Impact

- **新增代码**: `src-tauri/src/file_sink/mod.rs`、`file_sink/path_ops.rs`、`file_sink/android_ops.rs`
- **重构代码**: `src-tauri/src/transfer/receiver.rs` — 移除内联文件 I/O，改为调用 `FileSink`
- **删除代码**: `receiver.rs` 中的 `write_chunk_at_offset` 和 `verify_checksum` 函数
- **依赖**: 无新增外部依赖，复用现有 `blake3`、`tokio`、`tauri-plugin-android-fs`
- **API**: 无前端 API 变更，纯后端内部重构
