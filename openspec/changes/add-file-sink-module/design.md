## Context

当前接收端 `receiver.rs` 直接操作文件系统：创建目录、创建 .part 临时文件、按 offset 写入分块、BLAKE3 校验、重命名最终化。这些操作与 `file_source/path_ops.rs` 中已有的 `write_chunk`、`verify_hash` 存在重复。

更重要的是，Android 接收端需要将文件保存到公共目录（通过 SAF/MediaStore），当前硬编码的 `PathBuf` 操作无法适配。需要一个写入抽象层，让 `receiver.rs` 无需关心平台差异。

项目已有对称的读取抽象 `file_source` 模块（`FileSource` 枚举 + `path_ops` / `android_ops`），写入端应遵循相同模式。

## Goals / Non-Goals

**Goals:**

- 消除 `receiver.rs` 中与 `path_ops` 重复的文件 I/O 代码
- 提供 `FileSink` 抽象，统一桌面和 Android 接收端的文件写入流程
- 支持 .part 临时文件管理（创建 → 写入 → 校验 → 最终化 / 清理）
- 保持与 `file_source` 对称的模块结构和 API 风格
- 重构 `receiver.rs` 使其只关注网络拉取 + 加解密 + 进度跟踪

**Non-Goals:**

- 不实现 Android `android_ops.rs` 的具体逻辑（留空占位，属于后续任务）
- 不改变传输协议或前端 API
- 不重构 `sender.rs`（已完全委托给 `file_source`，无需改动）
- 不实现文件冲突自动重命名（属于独立功能）

## Decisions

### 1. 新建独立 `file_sink` 模块，不合并到 `file_source`

**选择**: `file_sink/` 独立模块

**备选**: 扩展 `file_source` 为 `file_io`

**理由**:
- 读（source）和写（sink）是本质不同的关注点，`FileSource` 的 `Path`/`AndroidUri` 枚举形态不适用于写入目标
- Android 写入目标可能是 MediaStore 公共目录，与读取时的 SAF URI 完全不同
- 独立模块保持单一职责，避免 `file_source` 膨胀

### 2. `FileSink` 使用枚举 + 关联数据，不使用 trait

**选择**: `enum FileSink { Path { save_dir }, AndroidPublicDir { ... } }`

**备选**: `trait FileSink` + `PathSink` / `AndroidSink` 实现

**理由**:
- 与 `FileSource` 枚举风格保持一致
- 只有两个变体，trait 过于抽象
- 枚举可以 `#[cfg]` 条件编译，桌面端不编译 Android 变体

### 3. `PartFile` 结构体封装临时文件状态

**选择**: 引入 `PartFile` 结构体持有 .part 文件的元数据（路径、最终路径、大小等）

**理由**:
- `receiver.rs` 中 .part 路径计算逻辑出现两次（`run_transfer` 和 `cleanup_part_files`），需要统一
- `PartFile` 作为类型安全的句柄，避免裸 PathBuf 传递
- 未来 Android 端 `PartFile` 可持有不同的内部状态（如 `FileUri`）

### 4. 桌面端 `path_ops` 直接复用 `file_source::CHUNK_SIZE`

**选择**: `file_sink` 导入 `file_source::CHUNK_SIZE`

**理由**:
- 分块大小是全局常量，不应在两个模块中分别定义
- 保持发送端和接收端的分块大小一致

## Risks / Trade-offs

- **[风险] `PartFile` 的生命周期管理** → 如果进程崩溃，.part 文件可能残留。缓解：启动时可扫描清理（不在本次范围）
- **[风险] Android `android_ops.rs` 留空** → 实际 Android 接收功能延后。缓解：桌面端功能不受影响，Android 占位确保接口设计合理
- **[权衡] 引入新模块增加代码量** → 短期增加约 150 行代码，但消除 receiver.rs 中约 80 行重复代码，净增约 70 行。长期 Android 适配时避免大规模重构
