## ⚠️ 更正（2026-08-10）：下面「事故链」一节的根因归结**是误诊**

**先读这一节再读下面。** `CLAUDE.md` 与三份知识库都把本文指为「完整推导」的落点，而本文
2026-08-07 那版给出的根因已被推翻。**原文一字未删**——openspec 是变更提案的历史记录，
要能看出认知是怎么变的，删掉就看不见了。下面只说改了什么、为什么改。

**旧归因（错）**：SAF 的 `ParcelFileDescriptor` 不归本进程所有，fd 在内核层被 provider
侧回收，于是 `lseek` 必 `EBADF`——即「SAF 的 fd 天生不能用来做长时间随机写」。

**真根因（对）**：`expo-file-system` 的 `FileSystemFileHandle.forContentURI` 只从
`ParcelFileDescriptor.fileDescriptor` 建 `FileChannel`，**不持有那个 PFD**，于是 PFD 被 GC
回收时 finalizer 顺手关掉了 fd。fd 是**被自己这一侧的 GC 关掉的**，不是被 provider 收走的。
本仓 `mobile/patches/expo-file-system@56.0.8.patch` 修的正是这条，**但那份补丁从未进过
Android 构建**——expo SDK 53+ 的 autolinking 对已发布 maven 产物的模块默认吃预编译 AAR，
`node_modules` 里被 patch 过的 Kotlin 源码根本没参与编译（机制、判据与机器护栏见
[`toolchain.md`](../../../dev-notes/knowledge/toolchain.md) 的「pnpm patch 打在有预编译产物的
原生依赖上会静默失效」）。

**误诊是怎么发生的**：补丁改了三次、三次自评「已修复」、三次照崩，于是「补丁都打了还是崩」
被反推成「SAF 的 fd 天生不能用」，写进本文并扩散到 `CLAUDE.md` 与三份知识库。
一个从未生效的修复，伪造出了一条架构事实。

**判决性证据**：旧归因解释不了**失败点每次都不同**——311 MB 崩在 45 MB，2 GB 分别崩在
16 MB 和 54 MB。provider 侧回收 fd 没有理由与写入量呈这种随机关系，而 GC 时机有。

### 两处留手：以下两条**继续成立**，只是理由换了

误诊被推翻不等于结论跟着作废。这两条是本 change 的产物，**不要连坐删掉**：

1. **「publish 只做顺序写、绝不 `setOffset`」这条规则要留着。** 它现在的理由与 fd bug 完全
   无关：**部分 DocumentsProvider 返回不可 seek 的管道式 fd**（`openDocument` 给的是 pipe，
   `position()` 一律失败）。这条规则本来就该独立于 fd 生命周期成立。
2. **「暂存 → 发布」两段式要留着。** 理由换成与 fd bug 无关的四条：
   - SAF / FUSE 上随机写慢；
   - 用户可见目录不该出现半成品文件；
   - 暂存要跨「中断 → 过几天再恢复」存活，而目标位置的授权可能已经变了；
   - 部分 DocumentsProvider 返回不可 seek 的 fd（同上），随机写在那里根本不可能。

   反过来说：**如果当初就知道真根因，本 change 依然应该做**——只是「Why」会从
   「修一个崩溃」变成上面这四条。它顺带修好崩溃，是因为两段式让 staging 落回私有目录的
   `RandomAccessFile`，绕开了那个没被打上补丁的 SAF handle。

### 随之失效的具体段落（原文保留，此处标注）

- 「事故链」的对照表里，SAF 那行的「fd 所有者：真正的所有者在 provider 侧 / fd 可被外部
  回收」——**换成**「PFD 无人持有，GC finalizer 关掉了 fd」。
- 「Android 11+ 的 `/storage/emulated/0` 对 app 而言是 FUSE 挂载，SAF 的 PFD 在长时间大文件
  写入期间被回收后……」——FUSE 挂载是事实，但它不是这次崩溃的成因。
- 「**这不是补丁引入的**」这句**仍然对**，而且比原文以为的更强：补丁根本没参与编译，
  它既不可能引入 bug，也不可能修好 bug。
- 「五、补丁修正与错误呈现」要拆开看：第三条（`errorDetail` 截断）在 TS 侧，一直生效；
  前两条在 Kotlin 侧，**当时全是空转**，真正让它们进构建的是后来给 `mobile/package.json`
  加的 `expo.autolinking.android.buildFromSource`。其中「`FileMode.READ_WRITE` 的
  write-only channel bug」最终**没有按原文的方式修**——补丁改为**保持上游对 SAF `"rw"`
  的拒绝**（`FileOutputStream.getChannel()` 本就 readable=false，冒充 "rw" 只会得到一个
  「签名说能读、实际不能读」的句柄）。两段式落地后接收侧也不再需要它：staging 恒在应用
  私有目录走 `RandomAccessFile`，SAF 只在 publish 时被顺序写一次。补丁的当前内容以
  [`toolchain.md`](../../../dev-notes/knowledge/toolchain.md) 的
  「expo-file-system 56.0.8 的 SAF FileHandle 必须保活 PFD」一节为准。

---

## Why

移动端接收大文件时会在传输中途以 `java.io.IOException: Bad file descriptor` 崩掉，
且**「恢复」按钮永远点不动**——每次恢复都以同一个错误立即失败。2026-08-07 实测：
Web → 移动接收 311.7 MB，45 MB 处中断；换桌面 → 移动同样复现；把接收目录从
系统 `Download`（SAF）换成应用私有目录后不再复现。

### 事故链

崩溃栈落在 `FileSystemFileHandle.setOffset` → `FileChannelImpl.position0` → `EBADF`。
`FileChannelImpl.position(long)` 会**先** `ensureOpen()`，抛出来的却是 `EBADF` 而不是
`ClosedChannelException`——**channel 自己还开着，是底层 fd 在内核层失效了**。

这与「私有目录正常、SAF 报错」的对照完全吻合，因为两条路径的 fd 所有权根本不同
（`mobile/node_modules/expo-file-system/android/src/main/java/expo/modules/filesystem/FileSystemFileHandle.kt:59-87`）：

| 接收目录 | 打开方式 | fd 所有者 | 结果 |
|---|---|---|---|
| 应用私有目录（`file://`） | `RandomAccessFile(file, "rw").channel` | RAF 自己，且 `FileChannelImpl` 强引用 parent | 生命周期自洽，稳 |
| 系统目录（`content://` SAF） | `FileOutputStream(pfd.fileDescriptor).channel` | `ParcelFileDescriptor`，真正的所有者在 provider 侧 | fd 可被外部回收，channel 无从知晓 |

Android 11+ 的 `/storage/emulated/0` 对 app 而言是 FUSE 挂载，SAF 的 `ParcelFileDescriptor`
在长时间大文件写入期间被回收后，`FileChannel` 依然认为自己是打开的，**下一次 `lseek` 就是
`EBADF`**。`FileSystemFileHandle.kt:152-163` 的 `offset` setter **没有 `ensureIsOpen()` 保护**
（`read` / `write` 都有），所以第一个撞上的必然是它。

**这不是补丁引入的。** `mobile/patches/expo-file-system@56.0.8.patch` 的三处改动逐条核对后：
保留 `parcelFileDescriptor` 字段**修好**了上游的 fd 泄漏（原版 pfd 无人引用，GC 时
`finalize()` 就会关掉 fd）；`close()` 里补 `pfd.close()` 是必要的（`FileOutputStream(FileDescriptor)`
的 `isFdOwner=false`，`channel.close()` 根本不关 fd）。补丁里**确实有一个真 bug**——
`FileMode.READ_WRITE` 走 `FileOutputStream(...).channel` 得到的是 `readable=false` 的
write-only channel——但那条分支只在续传时才走，这次崩的是 `FileMode.Truncate`，与它无关。
详见 design D0。

### 三处放大事故的结构问题

**1. 坏 handle 被永久缓存，「可恢复中断」是假的。**
`mobile/src/core/foreign-file-access.ts:224-238` 的 `openSink` 在 `existing` 分支**无条件复用
旧 handle**，而接收中断走的是 Interrupted（可恢复）路径——
`crates/transfer/src/actor/receiver.rs:181-249` 的 `Err` 分支**不调** `cleanup_part_files`，
于是那个 fd 已死的 handle 一直留在 `sinks` Map 里。日志里 11:15:31 首次失败、11:15:38 探测式
恢复**立刻以同一错误失败**，就是这条。UI 上那个「可恢复中断 / 点恢复」按钮因此是空头承诺。

**2. 接收热路径每 256 KB 跨一次 FFI，而它本不需要跨。**
`crates/transfer/src/actor/receiver.rs:309-386` 的接收循环是严格串行的
（`read_frame → decode_and_verify → write_sink_chunk → 下一帧`），其中 `write_sink_chunk`
经 uniffi async callback 调度到 RN 的 JS 线程、再经 JSI 把 256 KB 传给 Kotlin、同步写盘、
Promise resolve 回 Rust。而 JS 线程同时还在跑进度条的 React 重渲染。

关键是：**这一跳没有必要存在**。`mobile-core/src/app.rs:37` 早就持有
`data_dir: PathBuf`（`utils.rs:27` 的 `parse_host_dir` 在构造时就把 `file://` 解析掉了），
私有目录就是普通 POSIX 路径，Rust 侧 `std::fs` 直接可写。JS 侧存在的**唯一**不可替代理由
是 `ContentResolver`/SAF，而那只在 publish 的一瞬间需要。

实测桌面 → 移动局域网 3.6 MB/s（每 256 KB chunk 约 71 ms），Web → 移动 1.2 MB/s。
写 256 KB 到 ext4/f2fs 顶多 1–2 ms，两者之间的差额目前没有测量数据归因，
本 change **不承诺**下沉能解决性能问题（见「非目标」与 design D9）。

**3. `finalize` 时机制造了一个本不该存在的状态。**
`receiver.rs:557-636` 的 `finish_data_channel` 把**所有**文件的 finalize 堆在会话 Finish
之后，于是产生了「bitmap 完整但未 finalize」这个中间态，需要 `:564-585` 那段 30 行的兜底
（「本会话未收到任何 block 但 bitmap 已完整的文件也必须 open_or_create + finalize」）。
同时所有文件的 sink 从首块一直开到会话结束——一批 100 个文件就是 100 个并发 fd。

## What Changes

### 一、接收路径分成 staging 与 publish 两段（移动端）

`FileAccess` 端口的三段式（`create_sink → write_sink_chunk(offset) → finalize_sink`）
本来描述的就是「开一个可随机写的暂存 → 写 → 发布到目标并返回它最终在哪」。三端实现却分化了：

| 端 | staging | publish | 是否退化 |
|---|---|---|---|
| 桌面 | `<dst>/x.part` | `rename`（同盘原子、零拷贝） | 否 |
| Web | `opfs:/x`（写的就是最终路径） | `close` | 是（`crates/web/src/file_access.rs:217-224` 自己记录了后果） |
| 移动 | SAF `content://x` | `close` | **是，且致命**——把 `lseek` 压在不属于自己的 FUSE fd 上 |

移动端回到端口本来的语义：**staging 一律落在应用私有目录**，publish 时才触碰目标位置。
`file://` 目标走 rename/copy，`content://` 目标委托 JS 调 expo 的 `File.copy()`。

**staging 路径必须是 `f(save_dir, relative_path)` 的确定性函数**——`HostFileMetadata`
里没有 session_id，而续传靠 `open_or_create_sink(metadata)` 重建句柄（design D3）。

### 二、接收写盘热路径下沉到 Rust（移动端）

staging 既然在私有目录，就是普通路径，`MobileFileAccessAdapter`
（`mobile-core/src/file_access.rs:174-260`）不再把写侧转发给 JS：

| 操作 | 改造后归属 | 理由 |
|---|---|---|
| `create_sink` / `open_or_create_sink` / `write_sink_chunk` / `cleanup_sink` | **Rust**（`std::fs`） | staging 恒为私有目录，POSIX IO |
| publish 到 `content://` | JS | 只有 `ContentResolver` 能做 |
| `delete_finalized_file`（SAF URI） | JS | 同上 |
| `source_metadata` / `read_source_chunk` | **保持 JS**（本 change 不动） | 发送源可能是 `content://`，见 design D8 |

副作用是**删掉一整块最难测的逻辑**：`foreign-file-access.ts` 的 `sinks: Map`、长驻
`FileHandle`、SharedObject 生命周期、`existing` 分支要不要复用——全部消失，换成 Rust 侧一个
`HashMap<FileSinkId, File>`，且可直接写单测（现在那段只能在真机上试）。

### 三、finalize 改为 per-file：收齐即发布（core，三端共享）

文件的 bitmap 收齐时立即 publish，而不是等会话 Finish。

- `receiver.rs:564-585` 的兜底段**整段删除**——「bitmap 完整但未 finalize」这个状态
  从状态空间里消失了（续传时某文件 bitmap 完整 ⟺ 它上次已经 publish 过）
- **但这条等价关系需要同时改 checkpoint 的刷新时机才成立**：末块刻意**不刷**，
  完整 bitmap 只由 publish 成功后的 `mark_file_completed` 写。不改这条，
  「收齐 → 完整 bitmap 落库 → publish 失败」之后续传会永远跳过该文件，
  文件停在 staging 却被判为完成（design D10 的修正段落）
- 收齐即发布还需要一个 `published` 集合：发布后该文件的块若再次到达，
  `ensure_sink` 会为它新建空暂存并再次发布，把已落地的完整文件覆盖成残片。
  这个窗口是本节引入的，必须一起堵上（design 的对应段落）
- fd 峰值从 N（文件数）降到 1
- staging 的磁盘峰值从「整批总大小」降到「单个最大文件」。手机上传 20 个 500 MB 视频，
  前者要 10 GB 临时空间，而私有目录与 `/storage/emulated/0` 本就是同一个存储池
- 传输中断时已收完的文件**已经落到用户目录**，不再整批卡在 staging

### 四、finalize 语义统一为「只发布，不校验」（三端）

`src-tauri/src/host/file_sink/path_ops.rs:126-146` 的 `verify_and_finalize` 至今还在
**把整个文件重读一遍算 BLAKE3**（`:150-156` 的 `verify_checksum_sync`）。bao 逐块验签落地后
这遍校验已是纯冗余（每块收下时已验过，`root == 整文件 blake3` 是 bao 的不变量，
`crates/transfer/src/bao.rs:17-21`），而移动与 Web 的 finalize 都只是 close——
**桌面单方面多付一遍全文件读**，311 MB 就是白读 311 MB。删掉它。

随之而来的是失败语义的更正：finalize 不再校验，那么它失败**只可能**意味着
「数据是好的，只是搬不过去」（空间不足 / 权限被撤 / fd 失效）。于是
`receiver.rs:589-619` 现在做的 `reset_file_checkpoint` 是**错的**——它会让对端把整个文件
重传一遍，而数据完好躺在 staging 里。改为：保留 staging、不动 checkpoint、
走 Interrupted（可恢复）而非 terminal/failed（design D5）。

### 五、补丁修正与错误呈现

- `FileMode.READ_WRITE` 的 write-only channel bug（design D0）
- `offset` setter 补 `ensureIsOpen()`，让 fd 失效报出「file handle is closed」而非裸 `EBADF`
- `foreign-file-access.ts:51-57` 的 `errorDetail` 取 `err.message`，而 expo 的异常 message
  带整段 Java stacktrace，被原样塞进了 UI toast（实测截图可见）。截断到首行

## Capabilities

### New Capabilities

- `receive-file-staging`: 接收写入与最终落地分成 **staging（可随机写的暂存）** 与
  **publish（发布到用户目标位置）** 两个阶段；staging 位置由宿主自选但必须可随机写且
  可由文件元信息确定性重建；publish 在**单个文件收齐时**发生，而非会话结束时；
  finalize 只发布不校验，其失败表示「可重试的落地失败」而非「数据损坏」。

### 与既有 spec 的关系

`openspec/specs/file-sink/spec.md` 描述的是**重构前**的架构（`src-tauri/src/file_sink/`
模块、`create_part_file(relative_path, file_size)`、`write_chunk(part, chunk_index, data)`、
`verify_and_finalize` 的 BLAKE3 校验、`FileSink` 枚举含 `AndroidPublicDir` 变体）。
这些东西**现在一个都不存在**——文件 IO 早已收进 `crates/host` 的 `FileAccess` 端口，
写入按 `offset` 而非 `chunk_index`。它是一份描述已消失架构的存量 spec，本 change
**不与之交互**，但它应当被单独清理（见「非目标」）。

`chunk-transfer` / `transfer-data-plane` / `transfer-resume-coordination` 的要求不受影响：
chunk 尺寸、窗口流控、续传协商都没动，变的只是「收齐之后什么时候落地、落到哪」。

## Impact

- **`crates/transfer`**：`src/actor/receiver.rs` 的 `persist_chunk` 后新增「该文件收齐 →
  publish」分支；`finish_data_channel` 删掉 `:564-585` 的兜底段并改为只做会话级终态；
  `:589-619` 的失败分支改语义（不 reset checkpoint、转 Interrupted）。
  `src/failure.rs` 的 `FileFinalizeFailed` ~~保留~~ → **实施时删除了**（本段原先说保留，
  与最终实现相反，此处以实现为准）：改走 Interrupted 之后它零构造点，而该文件的模块文档
  写死了「变体数量贴着实际构造点，不预留将来可能用到的码」。

  **代价必须记下来**：publish 失败的原因因此退回了无类型的 `error: String` 通道，而那条
  通道至今仍被前端子串匹配（`event-bus.ts` / `transfer-notifications.ts` 的
  `error.startsWith("对方取消")`）——正是判别码当初要终结的反模式；且
  `SuspendedReason::Interrupted` 不携带 payload，重启后「磁盘满了」与「断线了」无法区分。
  这是本 change 留下的**已知缺口**，修法是让可恢复中断也能携带判别码
  （`SuspendedReason` 带 cause，或并列一个 `InterruptCode`），需要动状态机 + 三端 UI，
  单独立项。
- **`src-tauri`**：`host/file_sink/path_ops.rs` 删 `verify_checksum_sync` 与
  `verify_and_finalize` 里的校验分支（只留 rename）；`host/file_source.rs:245-264`
  的 `finalize_sink` 不再需要 `ActiveSink.checksum`，该字段随之退役。
- **`mobile/packages/swarmdrop-core/rust/mobile-core`**：新增 staging 实现（`std::fs`）；
  `ForeignFileAccess` trait（`src/file_access.rs:119-171`）**删 5 个方法、加 1 个 publish 方法**，
  是破坏性的 uniffi 签名变更，**必须重跑 `build:ios` / `build:android`**。
- **`mobile/src/core/foreign-file-access.ts`**：`sinks` Map、`OpenSink`、`openSink`、
  `ensureLocalSinkFile` 整块删除；只保留 SAF publish 与 `deleteFinalizedFile`。
  文件应随之改名以反映新职责（design D7）。
- **`mobile/patches/expo-file-system@56.0.8.patch`**：两处修正（D0）。
- **`crates/web`**：**本 change 不动**。Web 的 `staging == dst` 也是退化，但 OPFS 上没有
  fd 失效问题，独立处理（见「非目标」）。
- **回归**：续传跨进程重启仍能续（staging 路径可由 metadata 重建）；取消传输时 staging
  被真删（`cleanup_sink` 契约）；`cleanup_expired_part_files`
  （`crates/transfer/src/lib.rs:87-89`）按 metadata 重建 sink 再删，改的是它删的位置。

**非目标**：

- **发送侧 `read_source_chunk` / `source_metadata` 的下沉** → 需要按 URI scheme 分派，
  因为 `mobile/src/core/file-access.ts:95` 那句「`sourceId` 用 file:// URI」**在 Android
  目录发送场景下是错的**（`pickDirectoryAsync` 走 `PickerType.DIRECTORY` → SAF tree，
  子项是 `content://`）。它同样是热路径（bao outboard 构建要完整读一遍整个文件），
  但复杂度与风险独立，另开 change。详见 design D8。
- **性能修复的承诺** → 本 change 不声称能解决 3.6 MB/s。
  `Cargo.toml:145-150` 的 `[profile.mobile-release] opt-level = "z"` **没有对应的
  `package."*"` 豁免**（那一条只写在 `:133-134` 的 `[profile.dev]` 下），于是 blake3 /
  bao-tree / quinn / snow / chacha20poly1305 全被压在 `"z"` 下——正是 CLAUDE.md 里
  「加密依赖否则慢 10–100 倍」所指的坑在另一个 profile 上复发。那是几行配置的独立修复，
  与本 change 无耦合。详见 design D9。
  > **2026-08-10 部分落地**：Cargo.toml 加了 `[profile.mobile-release.package.blake3]
  > opt-level = 3`（`"z"` 会穿透到 blake3 build.rs 的 cc 调用，把 aarch64 的 C NEON 实现
  > 按住）。**只做了 blake3 一个包**，不是本段设想的 `package."*"` 全局豁免——判据与
  > 「还有哪些包该例外」见 [`toolchain.md`](../../../dev-notes/knowledge/toolchain.md)。
- **收件箱 per-file 化** → per-file publish 后文件已在用户目录（文件管理器可见），
  但 `ensure_inbox_item_for_completed_receive_session` 是**会话级**的，app 内收件箱仍要等
  会话完成才出现条目。这是已知缺口，属于产品语义变更（「部分完成的会话在收件箱长什么样」
  涉及三端 UI），不塞进本 change。
- **Web 端 `staging == dst` 的修正** → OPFS 上 rename 很便宜，但没有 fd 失效风险，
  不紧急；且本 change 已横跨 core + 桌面 + 移动三处。
- **过时的 `openspec/specs/file-sink/spec.md` 的清理** → 应当单独归档处理，
  混进本 change 会让 delta 读起来像是在改一个还活着的能力。
- **向后兼容与数据迁移** → 不做。staging 位置变化意味着旧的半成品文件不会被新代码认领，
  它们由 `cleanup_expired_part_files` 的既有过期回收自然淘汰。

**风险**：

1. **uniffi 签名破坏性变更**。`ForeignFileAccess` 删 5 加 1，TS 侧不实现新方法就是运行时
   `handle_callback_unexpected_error` → Rust panic（`foreign-file-access.ts:36-38` 的注释
   记录过这个形态：日志只有 "Rust panic" 没有源信息）。必须重跑 ubrn 生成并全量 typecheck。
2. **staging 与 publish 之间的窗口**。publish 是 copy 而非 rename 时（SAF 目标），
   存在「staging 已完整、目标只写了一半」的瞬间。进程此刻被杀，目标位置会留下一个
   长度不足的文件。需要明确 publish 的原子性策略（design D6）。
3. **SAF `createFile` 的 mimeType 陷阱会复活**。`foreign-file-access.ts:304-312` 记录过：
   传 `null` 会被 expo 兜底成 `text/plain`，导致 `.md` 落盘变成 `.md.txt`，必须传
   `application/octet-stream`。而 expo 的 `copy(dest=directory)` 路径用的是
   `source.type ?: "*/*"` 自己 createFile。**publish 必须 copy 到我们自己 createFile 好的
   具体文件**（`isContainer=false`），不能 copy 到目录（design D4）。
4. **per-file publish 改变了取消语义**。已 publish 的文件在会话被取消后**保留**
   （与「中断时已收完的文件立刻可用」一致）。这是有意的行为变更，需要在 UI 文案上对齐。
5. **私有目录空间**。staging 峰值虽降到单个最大文件，但仍是 2× 瞬时占用，
   且与目标目录共享同一存储池。publish 前应检查可用空间并给出可读的失败原因。
