## 1. 模块骨架

- [x] 1.1 创建 `src-tauri/src/file_sink/mod.rs`：定义 `FileSink` 枚举（`Path` + `#[cfg] AndroidPublicDir`）、`PartFile` 结构体、公共方法签名（`create_part_file`、`write_chunk`、`verify_and_finalize`、`cleanup_part_file`）
- [x] 1.2 创建 `src-tauri/src/file_sink/path_ops.rs`：桌面端各方法的具体实现（创建目录 + .part 文件、按 offset 写入、BLAKE3 校验 + 重命名、删除 .part）
- [x] 1.3 创建 `src-tauri/src/file_sink/android_ops.rs`：Android 端占位（`todo!()` 或 `unimplemented!`），确保编译通过
- [x] 1.4 在 `src-tauri/src/lib.rs` 中注册 `pub mod file_sink;`

## 2. 重构 receiver.rs

- [x] 2.1 修改 `ReceiveSession` 结构体：将 `save_path: PathBuf` 替换为 `sink: FileSink`
- [x] 2.2 修改 `ReceiveSession::new()` 参数：接收 `FileSink` 而非 `PathBuf`
- [x] 2.3 重构 `run_transfer` 中的文件创建逻辑：使用 `sink.create_part_file()` 替代内联的 `create_dir_all` + `File::create` + `set_len`
- [x] 2.4 重构 `pull_single_chunk` 中的写入逻辑：使用 `sink.write_chunk()` 替代内联的 `write_chunk_at_offset`
- [x] 2.5 重构 `run_transfer` 中的校验 + 最终化逻辑：使用 `sink.verify_and_finalize()` 替代内联的 `verify_checksum` + `rename`
- [x] 2.6 重构 `cleanup_part_files`：使用 `sink.cleanup_part_file()` 替代内联的 `remove_file`
- [x] 2.7 删除 `receiver.rs` 中的 `write_chunk_at_offset` 和 `verify_checksum` 函数

## 3. 上游调用适配

- [x] 3.1 修改 `offer.rs` 中 `accept_and_start_receive`：构造 `FileSink::Path { save_dir }` 传给 `ReceiveSession::new()`
- [x] 3.2 确认 `path_ops.rs` 中的 `write_chunk` 和 `verify_hash`（`file_source` 模块中的）不再被 receiver 直接调用后可保留或标记为仅供 source 使用

## 4. 验证

- [x] 4.1 `cargo build` 桌面端编译通过
- [x] 4.2 `cargo test` 现有测试通过
- [x] 4.3 为 `file_sink/path_ops.rs` 的核心方法编写单元测试（create_part_file、write_chunk、verify_and_finalize、cleanup）
