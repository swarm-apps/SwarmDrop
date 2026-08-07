# receive-staging-publish 设计

## 核实中发现的、与直觉不符的四处

立项直觉来自现场排查。落到代码上有四处偏差，本设计**以代码为准**：

| 直觉 | 代码实际 | 影响 |
|---|---|---|
| 「是我们 patch 的 expo 模块引入的 bug」 | patch 三处改动逐条核对：保留 `pfd` 字段**修好**了上游的 fd 泄漏；`close()` 补 `pfd.close()` 是必要的。**唯一的真 bug 是 `FileMode.READ_WRITE` 分支**，而本次崩溃走的是 `FileMode.Truncate`，与它无关 | 根因在 SAF fd 的所有权，不在 patch。见 D0 |
| 「`sourceId` 用 file:// URI」（`mobile/src/core/file-access.ts:95` 的注释） | `pickTransferFiles` 有 `copyToCacheDirectory: true` 所以是 `file://`；但 `pickTransferDirectory` 走 `Directory.pickDirectoryAsync()`，Android 上是 `PickerType.DIRECTORY` → SAF tree，**子项 `entry.uri` 是 `content://`** | 发送侧下沉必须按 scheme 分派，故排除出本 change。见 D8 |
| 「`openspec/specs/file-sink/spec.md` 是这块能力的现行规格」 | 它描述的是**重构前**的 `src-tauri/src/file_sink/` 模块、`FileSink` 枚举、`write_chunk(part, chunk_index, data)`、`verify_and_finalize` 的 BLAKE3 校验——现在一个都不存在，写入早已按 `offset` 走 `crates/host` 的 `FileAccess` 端口 | 新建 capability，不 MODIFY 它；它的清理单独走 |
| 「71 ms 里 FFI 往返占大头」 | 该估算隐含了「编译是优化过的」这个前提，而 `Cargo.toml:145-150` 的 `mobile-release` 是全局 `opt-level = "z"` 且**无 `package."*"` 豁免** | 不承诺性能收益，见 D9 |

## 实施中偏离本设计的五处（写完代码后补记）

| # | 原设计 | 实际 | 位置 |
|---|---|---|---|
| 1 | 「续传时 bitmap 完整 ⟺ 上次已 publish」，删掉兜底即可 | **不成立**，末块一定会刷 checkpoint。必须同时改刷新条件，否则删掉兜底会**静默丢文件** | D10 修正段 |
| 2 | 收齐即发布只是把 finalize 提前 | 还需要一个 `published` 集合——发布后重复块会新建空暂存并覆盖已落地文件。**这个窗口是本 change 引入的** | D10 后的新段 |
| 3 | `ExpoFileAccess` 名不副实，必须改名 | **保持原名**。名字说的是「宿主提供的文件访问」，收缩后仍准确；边界改为写进模块文档第一屏 | D7 |
| 4 | SAF publish 用 expo 的 `File.copy()` | **两条路径都不可用**（删后 uri 失效 / 拿 hash 当文件名），改为自己顺序搬运 | D4 |
| 5 | — | 顺带发现 `SessionStore::reset_file_checkpoint` 在 3.4 之后生产调用点归零，一并删除 | tasks 5c |

其余 file:line 全部核实无误，包括：`FileSystemFileHandle.kt:59-87`（两条 fd 打开路径）、
`:152-163`（`offset` setter 无 `ensureIsOpen`）、`foreign-file-access.ts:224-238`（`existing`
分支复用）、`receiver.rs:181-249`（Err 分支不 cleanup）、`:309-386`（串行接收循环）、
`:557-636`（`finish_data_channel`）、`path_ops.rs:126-156`（整文件 BLAKE3）、
`app.rs:37` + `utils.rs:27-33`（`data_dir: PathBuf` 与 `parse_host_dir`）、
`crates/transfer/src/bao.rs:17-21`（root == 整文件 blake3 的不变量）。

---

## D0：patch 的两处修正

**保留**（这两处是对的，不动）：

- `parcelFileDescriptor` 存成字段——上游不存它，pfd 无强引用，GC 时 `finalize()` 会
  `closeWithStatus(LEAKED)` 关掉 fd，而 `FileChannel` 毫不知情。patch 堵的正是这个洞。
- `close()` 里 `fileChannel.close()` 后补 `pfd.close()`——`FileOutputStream(FileDescriptor)`
  在 Android 上 `isFdOwner = false`，`channel.close()` 委托给它时**不关 fd**，不补就是纯泄漏。

**修正一**：`FileMode.READ_WRITE -> FileOutputStream(pfd.fileDescriptor).channel`
（`FileSystemFileHandle.kt:83`）。`FileOutputStream.getChannel()` 返回的是
`readable = false` 的 channel，于是 SAF 续传拿到的 handle 名义 `rw`、实际 write-only，
任何 `readBytes` 会抛 `NonReadableChannelException`。上游把这里写死成
`else -> throw Exceptions.IllegalArgument` 很可能正是为了躲它。

改造后接收路径不再用 SAF 的 `READ_WRITE`（staging 恒为私有目录），**这条分支退回上游的
`throw`** 是最简洁的选择：不留一个语义不实的能力等着别人踩。若将来确有 SAF 随机读写的需求，
再按需求重新设计。

**修正二**：`offset` 的 setter（`:160-163`）补 `ensureIsOpen()`。它是 `read`/`write`
都有、唯独 setter 没有的保护。补上之后 fd 失效会报「file handle is closed」而不是裸
`EBADF` + 一整段 Java 栈。这不是修 bug，是让**下一次**同类问题可诊断。

## D1：staging 落在 `<data_dir>/staging/`

候选：

- **(a) `Paths.cache` 下** —— 系统在存储紧张时会清理它。传输中断后用户过一天再点恢复，
  staging 已经没了，等于整个文件重传。**否决**。
- **(b) `<data_dir>/staging/`**（`data_dir` = `Paths.document.uri`，`app.rs:37` 已持有解析后的
  `PathBuf`，SQLite 也落在同一目录）。**选它**。

它与 SQLite 同寿命：用户「清除数据」时一起没，符合直觉；不进 iCloud 备份的问题不存在
（iOS 上 Documents 会被备份，但 staging 是短命的中间态，且 `cleanup_expired_part_files`
的既有过期回收会淘汰它——如果实测证明备份体积是问题，再单独加 `NSURLIsExcludedFromBackupKey`）。

## D2：下沉边界——写侧全下沉，读侧不动

`MobileFileAccessAdapter`（`mobile-core/src/file_access.rs:174-260`）现在是纯转发层。改造后按
「这件事**只有** JS 能做吗」划线：

| `FileAccess` 方法 | 改造后 | 依据 |
|---|---|---|
| `create_sink` / `open_or_create_sink` | Rust `std::fs` | staging 恒为私有目录 |
| `write_sink_chunk` | Rust `File::write_at`（Unix `FileExt`） | 同上，且它是热路径 |
| `cleanup_sink` | Rust `std::fs::remove_file` | 删的是 staging |
| `finalize_sink` | **Rust 编排 + 按目标 scheme 分派**（见 D4） | `file://` 目标 Rust 全包；`content://` 委托 JS |
| `source_metadata` / `read_source_chunk` | 保持 JS | 源可能是 `content://`，见 D8 |
| `delete_finalized_file` | 保持 JS | 参数是 publish 后的 URI，可能是 SAF document URI |

`ForeignFileAccess` trait 因此**删 5 加 1**：删 `create_sink` / `open_or_create_sink` /
`write_sink_chunk` / `finalize_sink` / `cleanup_sink`，加一个 publish 方法（D4）。

## D3：staging 路径 = `blake3(save_dir ‖ 0x00 ‖ relative_path)` 的 hex

**硬约束**：续传靠 `open_or_create_sink(metadata)` 重建句柄，而 `HostFileMetadata`
（`crates/host/src/ports.rs`）里**没有 session_id**。所以 staging 路径必须是 metadata 的
确定性函数，不能带会话维度。

候选：

- **(a) `<staging>/<relative_path>`** —— 不同 `save_dir` 的同名文件会撞车；且要递归建目录，
  还要处理 SAF 上合法而本地文件系统非法的文件名字符。
- **(b) `<staging>/<hex>`，`hex = blake3(save_dir ‖ 0x00 ‖ relative_path)`。选它。**

理由：staging 是**不透明的暂存**，目录结构的信息在 `relative_path` 里、由 publish 时重建，
staging 自己不需要保留它。扁平化换来：无需递归建目录、天然唯一、不受字符集与路径长度限制、
从 metadata 可零成本重算。

这与现有语义一致——「同一个目标路径同时只能有一个传输在写」，现在由 `openSink` 的
`"receive file is already open by another transfer"` 兜住，改造后由同一把 staging 路径兜住。

**分隔符不能省**：`blake3(save_dir ‖ relative_path)` 会让
`("/a/b", "c.txt")` 与 `("/a", "b/c.txt")` 撞成同一个 staging。

## D4：publish 按目标 scheme 分派

```
finalize_sink(sink_id):
  staging = <staging>/<hex>
  ├─ 目标是 file:// ────► Rust: 建目标父目录 → 逃逸实地校验 → rename(staging, dst)
  │                              └ rename 失败一律退回 copy（先删目标再 copy，
  │                                 因为 copy 会跟随符号链接而 rename 不会）
  └─ 目标是 content:// ─► JS: 自己 createFile(octet-stream) + **顺序搬运** → Rust 删 staging
                              （**不用 expo 的 File.copy()**，两条路径都不可用，见偏离表 #4）
```

**`file://` 目标同盘时是 rename，零拷贝。** 这是默认配置下的路径（`resolveReceiveLocation()`
在用户没设 `receivePath` 时回退到 `<data_dir>/transfers`，与 staging 同分区），
所以**默认接收路径上，改造后的 publish 几乎零成本**。真正付拷贝代价的只有 SAF 目标。

**SAF 分支的 mimeType 陷阱必须避开。** `foreign-file-access.ts:304-312` 记录过：
`createFile(name, null)` 会被 expo 兜底成 `text/plain`，然后 `DocumentsContract.createDocument`
发现 `.md` 与 `text/plain` 不匹配、按 `FileUtils#splitFileName` 强制追加 `.txt`，
于是 `.md` 落盘变成 `.md.txt`。而 expo 的 `copy(dest = 目录)` 路径
（`DestinationSink.SAF.receiveFrom`，`isContainer = true`）用的是 `source.type ?: "*/*"`
自己 createFile——**那个坑会原地复活**。

所以 JS 侧的 publish 必须：**先自己 `createFile(name, "application/octet-stream")` 拿到具体
目标文件，再 `copy` 到它**（`isContainer = false`）。现有的 `ensureSafSinkFile`
（`foreign-file-access.ts:278-317`）已经把这套逻辑写对了，改造后它从「建 sink」变成
「建 publish 目标」，逻辑整体保留。

**返回值不变**：publish 仍返回 `{ uri, dir }`。SAF 下 `uri` 必须是 `createFile` 实际返回的
document URI（系统可能把重名改写成 `foo (1).txt`），`dir` 必须是从 `Directory` 拿到的
父目录 URI——`crates/host/src/ports.rs:219-226` 已经把「这是 host 唯一诚实的事实源」写死了。

## D5：publish 失败 → Interrupted，且不动 checkpoint

现状（`receiver.rs:587-619`）：`finalize_sink` 失败 → `remove_created_sink` +
`reset_file_checkpoint` + `fail_session(FatalError(FileFinalizeFailed))` → terminal/failed。
那套语义配的是「finalize 会做整文件 BLAKE3 校验，失败意味着数据坏了」。

删掉校验（proposal 第四节）之后，publish 失败**只可能**是「数据是好的，只是搬不过去」。
于是 `reset_file_checkpoint` 变成有害的——它会让对端重传整个文件，而数据完好躺在 staging 里。

**新语义**：publish 失败 → 保留 staging、不动 checkpoint、`return Err(...)`。

这里有一个漂亮的简化：per-file publish 发生在 `handle_block_data` 的调用链内，它的 `Err`
会自然冒泡到 `run_data_channel` → `start_data_channel` 的 `Err` 分支
（`receiver.rs:181-249`）——**那条路径已经是 Interrupted（可恢复）了**。所以不需要为
publish 失败写任何特殊处理，只要不再调 `fail_session` / `reset_file_checkpoint` 即可。
错误原因经既有的 `TransferFailed` 事件呈现。

`FailureCode::FileFinalizeFailed` 的去留待实现时确认：若删掉 `fail_session` 调用后它零引用，
应一并退役（`crates/transfer/src/failure.rs` 是判别码的唯一来源，留着零引用的变体是债）。

## D6：publish 的原子性——`file://` 有，SAF 没有，且不假装有

- **`file://` 目标**：`rename` 是原子的，天然没有中间态。
- **SAF 目标**：copy 期间进程被杀，目标位置会留下一个长度不足的文件。

SAF 上没有干净的原子方案：`DocumentsContract.renameDocument` 存在，但 provider 可选实现，
且「先 createFile 临时名 → copy → rename」在 rename 不被支持的 provider 上会卡在临时名上，
比不原子更糟。

**决定：接受 SAF publish 非原子，靠两条兜底而不是靠假装原子。**

1. publish 失败时（含异常路径）**立即删除目标位置的半成品**，再返回错误。staging 仍在，可重试。
2. 进程被杀这种无法执行清理的情况，由 staging 的存在来标识「这个文件没搬完」——
   续传时 `open_or_create_sink` 找到 staging、bitmap 完整，重新 publish 一次（覆盖半成品）。

第 2 条要求 **publish 必须可重入**：目标已存在时覆盖，而不是生成 `foo (1).txt`。
SAF 下需要先 `findChildFile` 命中就复用（`ensureSafSinkFile:295-302` 现有逻辑已经是这样，
且它的注释解释了为什么不能 delete + 重建：SAF 的异步 delete 会与 createFile 撞 race）。

## D7：JS 侧收缩后的形状

`mobile/src/core/foreign-file-access.ts` 改造后只剩：

- `sourceMetadata` / `readSourceChunk`（读源，D8 不动）
- `publishToTarget`（新，SAF 分支专用）
- `deleteFinalizedFile`

整块删除的是：`sinks: Map<string, OpenSink>`、`OpenSink` / `SinkTarget` 类型、
`createSink` / `openOrCreateSink` / `writeSinkChunk` / `finalizeSink` / `cleanupSink`、
`openSink`、`ensureLocalSinkFile`。`ensureSafSinkFile` / `findChildDirectory` /
`findChildFile` / `saveLocationUri` 保留，服务于 publish。

**命名：实现时改了主意，`ForeignFileAccess` / `ExpoFileAccess` 保持不变。**

原本计划改名，理由是「名字描述了旧职责」。落到代码上这条不成立：`ForeignFileAccess`
说的是「宿主提供的文件访问能力」，收缩之后它提供的**仍然是**文件访问，只是范围小了——
名字没有变得不准确。而改 trait 名要连带改 uniffi 生成的 TS 类型名与全部引用，
diff 变大却换不到任何语义收益。

真正防住「下一个人往里加东西」的不是名字，是**边界写在模块文档的第一屏**：
新文件开头就是「这一层只做 Rust 做不到的事」+ 三条能力清单 + 为什么接收不再直接写 SAF。
一个想加 `writeSinkChunk` 回来的人会先撞上它。

**iOS 上这个 port 近乎空转**：iOS 没有 SAF，`resolveReceiveLocation()` 无论默认还是自定义
都是 `file://`，publish 走 Rust 分支。这是正确的结果，不是遗漏。

## D8：发送侧读取为什么不在本 change

`read_source_chunk` 同样是热路径，而且 bao outboard 构建
（`crates/transfer/src/bao.rs:64-78` 的 `build_outboard_from_source`）要**完整读一遍整个
文件**——311 MB 的文件在用户选完之后经 JS 读一遍，感知就是「选完卡了很久」。技术上与写侧
是同一件事。

但它有一个写侧没有的约束：**源的 scheme 不受我们控制**。核实结果：

| 入口 | Android | iOS |
|---|---|---|
| `pickTransferFiles`（`file-access.ts:17`，`copyToCacheDirectory: true`） | `file://` | `file://` |
| `pickFromMediaLibrary`（`:55` ImagePicker） | `file://`（expo 复制到 cache） | `file://` |
| `pickTransferDirectory`（`:101` `Directory.pickDirectoryAsync()`） | **`content://`**（SAF tree 的子项） | `file://` |

所以下沉读侧必须写一个 scheme 分派器，并且要保证 `content://` 分支仍走 JS 且满足
`read_source_chunk` 的严格契约（`crates/host/src/ports.rs:190-201`：不取整、不多读、不少读，
且 bao 会按 16 KiB 非对齐 offset 调用）。这是独立的风险面，与「修接收崩溃」没有耦合，
**另开 change**。

顺带：`file-access.ts:95` 那句注释是错的，本 change 顺手改正（它误导的正是「能不能假设
sourceId 是 file://」这个判断）。

## D9：性能与 `opt-level = "z"` 的关系

本 change **不承诺**修复 3.6 MB/s。理由是有一个更便宜、更可能的解释尚未被排除：

`Cargo.toml:133-134` 有 `[profile.dev.package."*"] opt-level = 3`，CLAUDE.md 解释了它存在的
理由——「加密依赖否则慢 10–100 倍」。而 `:145-150` 的 `[profile.mobile-release]`
是后来从 mobile-core 搬进 workspace 的，**没有带上对应的 `package."*"` 豁免**，
于是 `opt-level = "z"` 应用于所有 crate，包括 blake3（bao 逐块验签）、bao-tree、
quinn、snow / chacha20poly1305（TCP 链路的 Noise）。

它是几行 Cargo.toml 的独立修复，与本 change 无代码耦合，**应当先做并单独测量**，
否则本 change 的性能效果无法归因。

本 change 与性能唯一的确定关系是：接收热路径上每 chunk 一次的 uniffi + JSI 往返会消失。
这是**架构上的正确性**（staging 恒为私有目录，普通 POSIX IO 没有理由绕 JS），
收益多少由测量说话。

建议的测量顺序（不属于本 change 的任务，但决定如何解读结果）：

1. 桌面 → 桌面传大文件，得到「core 数据面本身」的 baseline
2. 加 `[profile.mobile-release.package."*"] opt-level = 3` 重编 release 包，得到 profile 的贡献额
3. 本 change 落地后再测，差值即 FFI 的贡献额

## D10：per-file publish 的判据与落点

判据用 **bitmap**，不用 `ProgressTracker` 的字节计数——`receiver.rs:521` 的
`count_completed_in_bitmap(bitmap, total_chunks)` 已经在 `persist_chunk` 里算出来了，
现成可用；而 `ProgressTracker::update_file_chunk`（`progress.rs:240-256`）的
`chunks_done >= total_chunks` 是展示用的累加计数，不是持久化真相。

落点在 `persist_chunk` 之后：收齐 → `finalize_sink` → `mark_file_completed` →
`remove_created_sink` → 从 `sinks` map 摘除。

**但 checkpoint 的刷新条件必须同时改，否则会静默丢文件。**

`persist_chunk` 现在的条件是 `completed.is_multiple_of(CHECKPOINT_INTERVAL) || completed >= total_chunks`
——**最后一块一定会刷**。于是「收齐最后一块 → 完整 bitmap 落库 → publish 失败 → 中断」之后，
续传时 `initial_bitmaps` 里该文件已完整，`first_missing_range` 跳过它、`fetch_plan` 不含它，
**再也不会有 block 到达、也就再也不会触发 publish**；而 `ensure_files_complete` 只看 bitmap，
会让 UI 报完成，文件却永久停在 staging。这正是本 change 计划删掉的那段兜底当初照顾的情形之一
——原设计断言它「不可达」是错的。

修法比把兜底加回来更简洁：**最后一块不刷 checkpoint，完整 bitmap 只由 publish 成功后的
`mark_file_completed` 写入**（它本来就写 `completed_chunks`，见 `crates/transfer/src/store.rs:55-63`）。
条件改为 `completed.is_multiple_of(CHECKPOINT_INTERVAL) && completed < total_chunks`。

这样不变量才真正成立：

> **DB 里 bitmap 完整 ⟺ 该文件已 publish**

publish 失败时 DB bitmap 停在上一个节流点，续传重拉最多 `CHECKPOINT_INTERVAL - 1` = 9 个 chunk
（2.25 MB）后再次触发 publish，自愈。代价可接受，且远好过永久卡死。

### 收齐即发布必须配一个「已发布」集合（实现时发现）

发布之后若该文件的块再次到达，`ensure_sink` 会为它**新建一条空暂存**、写入这一块、
再次发布——把已经落到用户目录的完整文件覆盖成残片。

**这个窗口是收齐即发布引入的**：会话末尾统一 finalize 的旧实现里，重复块只是重复写同一个
sink，最终仍只发布一次，不会有事。所以它不是既有缺陷，是本 change 必须一起堵上的口子。

`ReceiverActor::run_data_channel` 因此维护一个 `published: HashSet<u32>`，
`handle_block_data` 开头即拒。续传时 bitmap 已完整的文件**预置进去**——按
「DB bitmap 完整 ⟺ 已发布」那条不变量它们本就不该再有块到达，这样顺带把不变量
变成运行时强制的。

选择**报错断流**而不是静默忽略：走到这里说明对端与本端对「哪些文件已经收齐」的认知
已经分叉，静默吞掉只会让分叉继续扩大。

### 已知限制：publish 与 `mark_file_completed` 之间的窗口

publish 成功后 staging 已经消失（`file://` 是 rename 走的），此时若进程在
`mark_file_completed` 之前被杀，DB 里 bitmap 停在上一个节流点、而 staging 不存在。
续传会新建一个预分配的空 staging，却只重拉最后那几块——**前面的空洞永远不会被填**，
而 `ensure_files_complete` 只看 bitmap，会静默产出一个长度正确、内容有洞的文件。

**这是既有缺陷，不是本 change 引入的**（现状同样存在：`.part` 被 rename 走之后
`mark_file_completed` 失败，续传 `open_or_create_sink` 一样会新建空 `.part`），
窗口是两个语句之间的一个 await。

**本 change 不修**，因为任何修法都比缺陷本身重：hard link + 延后删除会引入文件系统能力假设；
staging 旁挂 `.meta` 记录 bitmap 则是把持久化状态劈成两处、需要自己保证两者一致。
真正的修法是让 `open_or_create_sink` 回报「这是新建的还是续上的」，由 core 在新建时重置 bitmap
——那是端口签名变更，应当单独立项。

实现时的纪律：**publish 与 `mark_file_completed` 之间不得插入任何其他 await**，把窗口压到最小。

**空文件成立**：`calc_total_chunks(0) == 1`（`lib.rs:75-80`），发送端对 `size == 0` 会发一个
空 BlockData（`sender.rs:542-546`），接收端照样走完 `persist_chunk` 并标记 chunk 0，
于是 `1 >= 1` 收齐、正常 publish。这条必须有测试钉住——它是唯一一个「没有真实数据流过
却要落地」的情形，也正是 `finish_data_channel` 那段兜底当初要照顾的对象之一。

`finish_data_channel` 随之收缩为只做会话级终态：`dispatch(Actor{Completed})` →
`ensure_inbox_item_after_completion` → 完成事件。`ensure_files_complete`
（`checkpoint.rs:13-24`）保留——它是「Finish 帧到达时所有文件确实收齐」的协议级断言，
与 publish 时机无关。
