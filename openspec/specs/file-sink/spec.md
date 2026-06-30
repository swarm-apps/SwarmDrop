# file-sink Specification

## Purpose
TBD - created by archiving change add-file-sink-module. Update Purpose after archive.
## Requirements
### Requirement: FileSink 枚举定义

系统 SHALL 提供 `FileSink` 枚举，包含 `Path { save_dir: PathBuf }` 变体（桌面端）和 `#[cfg(target_os = "android")] AndroidPublicDir` 变体（Android 端），作为接收端文件写入的统一抽象。

#### Scenario: 桌面端创建 FileSink

- **WHEN** 接收方在桌面端选择保存目录 `/home/user/downloads`
- **THEN** 系统创建 `FileSink::Path { save_dir: PathBuf::from("/home/user/downloads") }`

### Requirement: 创建 .part 临时文件

`FileSink` SHALL 提供 `create_part_file(relative_path, file_size)` 异步方法，根据 `relative_path` 创建对应的目录结构和 `.part` 临时文件，预分配文件大小，返回 `PartFile` 句柄。

#### Scenario: 创建嵌套路径的 .part 文件

- **WHEN** 调用 `create_part_file("docs/readme.md", 1024)` 且 `save_dir` 为 `/downloads`
- **THEN** 系统创建 `/downloads/docs/` 目录、创建 `/downloads/docs/readme.md.part` 文件并预分配 1024 字节，返回持有该路径信息的 `PartFile`

#### Scenario: 创建无扩展名文件的 .part 文件

- **WHEN** 调用 `create_part_file("Makefile", 512)`
- **THEN** 系统创建 `Makefile.part` 文件并预分配 512 字节

#### Scenario: 空文件的 .part 创建

- **WHEN** 调用 `create_part_file("empty.txt", 0)`
- **THEN** 系统创建 `empty.txt.part` 文件，大小为 0，不调用 `set_len`

### Requirement: 写入分块数据

`FileSink` SHALL 提供 `write_chunk(part_file, chunk_index, data)` 异步方法，将解密后的数据写入 `.part` 文件的正确偏移位置。偏移量 SHALL 按 `chunk_index * CHUNK_SIZE` 计算。写入操作 SHALL 通过 `spawn_blocking` 避免阻塞异步运行时。

#### Scenario: 写入第一个分块

- **WHEN** 调用 `write_chunk(part, 0, data)` 且 data 长度为 256KB
- **THEN** 数据写入 .part 文件偏移 0 处

#### Scenario: 写入中间分块

- **WHEN** 调用 `write_chunk(part, 3, data)`
- **THEN** 数据写入 .part 文件偏移 `3 * 256KB = 768KB` 处

### Requirement: 校验并最终化文件

`FileSink` SHALL 提供 `verify_and_finalize(part_file, expected_checksum)` 异步方法。该方法 SHALL 计算 .part 文件的 BLAKE3 校验和，与 `expected_checksum` 比较。校验通过时 SHALL 将 .part 文件重命名为最终路径并返回 `Ok(final_path)`；校验失败时 SHALL 删除 .part 文件并返回错误。

#### Scenario: 校验成功并最终化

- **WHEN** 调用 `verify_and_finalize(part, "abc123...")` 且 .part 文件的 BLAKE3 hash 匹配
- **THEN** `.part` 文件重命名为最终路径（去掉 `.part` 后缀），返回最终路径

#### Scenario: 校验失败

- **WHEN** 调用 `verify_and_finalize(part, "wrong_hash")` 且 hash 不匹配
- **THEN** `.part` 文件被删除，返回 `AppError::Transfer` 错误

### Requirement: 清理临时文件

`FileSink` SHALL 提供 `cleanup_part_file(part_file)` 异步方法，删除 `.part` 临时文件。删除失败时 SHALL 静默忽略（日志记录但不返回错误）。

#### Scenario: 清理存在的 .part 文件

- **WHEN** 调用 `cleanup_part_file(part)` 且 .part 文件存在
- **THEN** 文件被删除

#### Scenario: 清理不存在的 .part 文件

- **WHEN** 调用 `cleanup_part_file(part)` 且 .part 文件已不存在
- **THEN** 方法静默返回，不报错

### Requirement: PartFile 结构体

系统 SHALL 定义 `PartFile` 结构体，封装 .part 临时文件的状态信息，至少包含：.part 文件路径、最终文件路径、文件大小。`PartFile` SHALL 作为 `create_part_file` 的返回值和其他方法的输入参数。

#### Scenario: PartFile 提供路径访问

- **WHEN** 通过 `create_part_file("photo.jpg", 2048)` 创建 PartFile
- **THEN** `part_file.part_path()` 返回 .part 文件路径，`part_file.final_path()` 返回最终文件路径

### Requirement: receiver.rs 重构使用 FileSink

`receiver.rs` 中的 `ReceiveSession` SHALL 持有 `FileSink` 实例（替代当前的 `save_path: PathBuf`）。所有文件 I/O 操作（创建 .part、写入分块、校验最终化、清理）SHALL 通过 `FileSink` 方法完成。`receiver.rs` 中的 `write_chunk_at_offset` 和 `verify_checksum` 函数 SHALL 被移除。

#### Scenario: ReceiveSession 使用 FileSink 写入文件

- **WHEN** ReceiveSession 接收到一个文件的所有分块
- **THEN** 通过 `FileSink::create_part_file` 创建临时文件，通过 `FileSink::write_chunk` 写入各分块，通过 `FileSink::verify_and_finalize` 校验并最终化

#### Scenario: ReceiveSession 取消时清理

- **WHEN** 用户取消接收传输
- **THEN** 通过 `FileSink::cleanup_part_file` 清理所有已创建的 .part 文件

### Requirement: 模块结构

`file_sink` 模块 SHALL 位于 `src-tauri/src/file_sink/`，包含 `mod.rs`（`FileSink` 枚举 + `PartFile` 定义）、`path_ops.rs`（桌面端实现）、`android_ops.rs`（Android 端占位）。模块 SHALL 在 `src-tauri/src/lib.rs` 中注册。

#### Scenario: 桌面端编译

- **WHEN** 在桌面平台编译
- **THEN** 仅编译 `mod.rs` 和 `path_ops.rs`，`android_ops.rs` 通过 `#[cfg(target_os = "android")]` 排除

#### Scenario: Android 端编译

- **WHEN** 在 Android 平台编译
- **THEN** 编译 `mod.rs`、`path_ops.rs` 和 `android_ops.rs`

