# receive-staging-publish 任务分解

> 语言约定：所有新增注释与文档一律中文。
> **全局纪律**：本 change 不写任何迁移 / 回填 / 双写 / 兼容层代码（proposal 非目标）。
> 旧的 SAF 半成品由 `cleanup_expired_part_files` 的既有过期回收自然淘汰。
>
> **阶段顺序不可调换**：2 → 3 是 core 与桌面，可独立验证；4 → 5 是移动端，
> 依赖 2 定下的 publish 时机。1 独立，可最先合。

## 1. expo 补丁修正（`mobile/patches/expo-file-system@56.0.8.patch`）

- [x] 1.1 `FileSystemFileHandle.kt` 的 `forContentURI`：`FileMode.READ_WRITE` 分支退回上游的
      `else -> throw Exceptions.IllegalArgument("Unsupported file mode: '$mode'")`——
      改造后 SAF 不再需要随机读写，留一个 `readable=false` 却自称 `rw` 的 channel 是埋雷（design D0）
- [x] 1.2 `offset` 的 setter（`:160-163`）补 `ensureIsOpen()`，与 `read` / `write` 对齐
- [x] 1.3 保留 `parcelFileDescriptor` 字段与 `close()` 里的 `pfd.close()`——**这两处是对的，
      不要一起回退**（design D0 记录了理由：上游不存 pfd 会被 GC finalize 关掉 fd；
      `FileOutputStream(FileDescriptor)` 的 `isFdOwner=false` 使 `channel.close()` 不关 fd）
- [x] 1.4 `pnpm install` 重新应用补丁，确认 patch 能干净套上

## 2. core：per-file publish（`crates/transfer`）

- [x] 2.1 `actor/receiver.rs` 的 `persist_chunk`：在写完 checkpoint 之后判断该文件是否收齐，
      判据用**已有的** `count_completed_in_bitmap(bitmap, total_chunks)`（`:521`），
      **不用** `ProgressTracker` 的字节计数（design D10）
- [x] 2.2 收齐分支执行：`finalize_sink` → `mark_file_completed(session, file_id, bitmap, size,
      finalized.uri, finalized.dir)` → `remove_created_sink` → 从 `sinks` map 摘除
- [x] 2.3 `handle_block_data` 的签名调整以便 `persist_chunk` 能拿到 `sinks`（现在只传 `sink_id`）
- [x] 2.3b `persist_chunk` 的 checkpoint 刷新条件改为
      `completed.is_multiple_of(CHECKPOINT_INTERVAL) && completed < total_chunks`
      ——**最后一块不刷**，完整 bitmap 只由 publish 成功后的 `mark_file_completed` 写。
      不改这条，2.4 删掉兜底后会静默丢文件（design D10 的修正段落）
- [x] 2.4 **删除** `finish_data_channel` 里 `:564-585` 的兜底段（「本会话未收到任何 block 但
      bitmap 已完整的文件也必须 open_or_create + finalize」）——2.3b 之后该状态不可达
- [x] 2.5 `finish_data_channel` 收缩为只做会话级终态：`dispatch(Actor{Completed})` →
      `ensure_inbox_item_after_completion` → 完成事件；不再遍历文件
- [x] 2.6 保留 `ensure_files_complete`（`checkpoint.rs:13-24`）——它是 Finish 帧的协议级断言，
      与 publish 时机无关
- [x] 2.7 单测：多文件传输中，第一个文件收齐时立即 publish，第二个文件仍在传输中
- [x] 2.8 单测：`size == 0` 的空文件经 publish 落地（design D10 指出这是唯一「没有真实数据
      流过却要落地」的情形，也正是 2.4 删掉那段当初照顾的对象之一）
- [x] 2.9 单测：续传时某文件 bitmap 已完整 → 不再为它 `open_or_create_sink`，也不再 publish
- [x] 2.10 publish 后该文件不再持有写句柄（`created_sinks` 里已摘除）——由 `publish_file`
      末尾的 `remove_created_sink` 保证。**未写自动化测试**：要观察它必须在「第一个文件已发布、
      传输仍在进行」的瞬间调 `cancel_receive`，而中断后 actor 已被 `remove_receive_if_epoch`
      摘出 registry，`cancel_receive` 只会返回 SessionNotFound——e2e 里构造不出确定性时机。
      由真机项 6.7 覆盖

## 3. finalize 语义统一（`src-tauri` + `crates/transfer`）

- [x] 3.1 `src-tauri/src/host/file_sink/path_ops.rs`：`verify_and_finalize` 删掉校验分支，
      只留 `rename`；函数改名以反映新职责（不再 verify）
- [x] 3.2 删除 `verify_checksum_sync`（`:150-156`）及其测试
- [x] 3.3 `src-tauri/src/host/file_source.rs:245-264` 的 `finalize_sink` 不再取 `checksum`；
      `ActiveSink.checksum` 字段随之退役（确认 `create_sink` / `open_or_create_sink`
      两处构造点一并清理）
- [x] 3.4 `crates/transfer/src/actor/receiver.rs` 的 publish 失败分支：**删掉**
      `reset_file_checkpoint` 与 `fail_session` 调用，改为直接 `return Err(...)`
      让它冒泡到 `start_data_channel` 的 Err 分支（既有的 Interrupted 路径，design D5）
- [x] 3.5 确认 `FailureCode::FileFinalizeFailed`（`crates/transfer/src/failure.rs`）在 3.4 之后
      是否还有引用；零引用则一并退役
- [x] 3.6 单测：publish 失败后该文件的 checkpoint **未被重置**，会话转入 suspended/recoverable
      而非 terminal/failed
- [x] 3.7 桌面接收不再包含一遍全文件读——`path_ops::publish` 现在只有 `rename`，
      `verify_checksum_sync` 已删除（代码层面确定）。实测归真机项 6.10

## 4. 移动端 staging 下沉到 Rust（`mobile-core`）

- [x] 4.1 新增 staging 模块：`<data_dir>/staging/` 目录管理（`data_dir` 取自 `app.rs:37`
      已解析的 `PathBuf`，design D1）
- [x] 4.2 staging 路径 = `blake3(save_dir ‖ 0x00 ‖ relative_path)` 的 hex 文件名，扁平不建子目录
      （design D3）。**分隔符 `0x00` 不能省**，否则 `("/a/b","c.txt")` 与 `("/a","b/c.txt")` 撞车
- [x] 4.3 `MobileFileAccessAdapter` 的 `create_sink` / `open_or_create_sink` 改为 Rust 实现：
      建/开 staging 文件，维护 `HashMap<FileSinkId, File>`；`create_sink` 截断，
      `open_or_create_sink` 保留已有内容
- [x] 4.4 `write_sink_chunk` 改为 Rust `FileExt::write_at`，**不再经 JS**
- [x] 4.5 `cleanup_sink` 改为 Rust：关句柄 + 删 staging 文件
- [x] 4.6 `finalize_sink` 改为 Rust 编排：关句柄 → 按目标 scheme 分派（design D4）
      - `file://` 目标：建父目录 → **路径逃逸实地校验**（`canonicalize` 断言仍在 save_dir 内，
        与桌面 `path_ops::ensure_within` 同一条防线；移动端此前完全没有）→ `rename`
      - rename 失败**一律退回 copy**，不判 `EXDEV`：跨设备只是失败原因之一，其余原因
        （目标被占、权限）在 copy 上会以更贴切的错误失败，为此引入 `libc` 读 errno 不划算
      - `content://` 目标：调 JS 的 publish 方法，成功后 Rust 删 staging
- [x] 4.7 publish 失败时删除目标位置的半成品（design D6 兜底 1），staging 保留
- [x] 4.8 单测：staging 路径的确定性与抗撞车（4.2 那两组输入必须产出不同 hex）
- [x] 4.9 单测：`create_sink` 截断 / `open_or_create_sink` 保留 的行为差异
- [x] 4.10 单测：`file://` 同卷目标走 rename（不产生拷贝）

## 5. 移动端 port 收缩与 JS 侧改造

- [x] 5.1 `mobile-core/src/file_access.rs` 的 `ForeignFileAccess` trait：删
      `create_sink` / `open_or_create_sink` / `write_sink_chunk` / `finalize_sink` / `cleanup_sink`，
      加 publish 方法（入参：staging 绝对路径 + `MobileFileMetadata`；返回 `MobileFinalizedSink`）
- [x] 5.2 `mobile/src/core/foreign-file-access.ts`：删除 `sinks` Map、`OpenSink` / `SinkTarget`
      类型、`openSink`、`ensureLocalSinkFile` 及被删的 5 个方法
- [x] 5.3 实现 publish：先 `createFile(name, "application/octet-stream")` 拿到具体目标文件，
      再**自己顺序搬运**（`readBytes`/`writeBytes` 推进偏移，绝不 `setOffset`）。
      **不用 expo 的 `File.copy()`**——实现时核实其两条路径都不可用：copy 到具体文件
      （`isContainer=false`）会先 `deleteRecursively()` 再写，而 SAF document 删掉后 uri 失效；
      copy 到目录（`isContainer=true`）会拿 **staging 的 hash 文件名**当目标名。
      mimeType 仍必须是 `application/octet-stream`，否则 `.md → .md.txt`（design D4）
- [x] 5.4 publish 可重入：目标已存在时复用而非 delete+重建（沿用 `ensureSafSinkFile:295-302`
      的现有判断与它记录的 SAF delete race 理由，design D6）
- [x] 5.5 `ensureSafSinkFile` / `findChildDirectory` / `findChildFile` / `saveLocationUri` 保留，
      职责从「建 sink」改为「建 publish 目标」，注释同步改写
- [x] 5.6 `errorDetail`（`:51-57`）截断到首行——expo 的异常 message 带整段 Java stacktrace，
      现在被原样塞进 UI toast
- [x] 5.7 ~~文件与类改名~~ → **实现时改了主意，保持 `ForeignFileAccess` / `ExpoFileAccess`**：
      名字说的是「宿主提供的文件访问」，收缩后仍然准确；改 trait 名要连带改 uniffi 生成的
      TS 类型与全部引用，换不到语义收益。边界改为写在模块文档第一屏（design D7 已更新）
- [x] 5.8 改正 `mobile/src/core/file-access.ts:95` 的注释——`pickTransferDirectory` 在 Android 上
      产出的是 `content://` 而非 `file://`（design D8 的核实结果）
- [x] 5.9 重跑 `pnpm --filter react-native-swarmdrop-core build:ios` 与 `build:android`
      重新生成 uniffi 绑定（**签名破坏性变更，不重跑就是运行时 Rust panic**，风险 1）

## 5b. `FailureCode` 变体删除的连带面（实施中发现的遗漏）

> 删 `FileFinalizeFailed` 时只 grep 了 `crates/` 与 `mobile/`，**漏了两个前端和两份
> 生成产物**。本仓「三份自动生成的 bindings 会静默漂移且没有门禁拦」的老问题
> （toolchain.md）在这里又踩了一次。

- [x] 5b.1 `src/lib/errors.ts` 删 `fileFinalizeFailed` 分支（桌面前端）
- [x] 5b.2 `docs/app/app/_lib/view-types.ts` 删同名分支（Web 前端）
- [x] 5b.3 `src/lib/bindings.ts` —— 无需手动，`cargo test --workspace` 里的
      `export_ts_bindings` 已自动重导出（已确认 union 里不再有该变体）
- [x] 5b.4 重生成 `crates/web/bindings/bindings.ts`
      （`cargo test -p swarmdrop-web --features specta --test specta_export`）
- [x] 5b.5 重新 `pnpm build:wasm` 更新 `packages/swarmdrop-web/swarmdrop_web.d.ts`
- [x] 5b.6 `pnpm exec tsc --noEmit`（桌面）+ `docs/` 下 `pnpm typecheck`
- [x] 5b.7 三端各删了一条文案（`FileFinalizeFailed` 的用户串），按 dev-workflow 重跑
      `i18n:extract`：桌面 `src/locales/{zh,zh-TW,en}`、移动 `mobile/src/locales/{zh-Hans,en}`、
      Web `docs/app/app/_locales/{zh,zh-TW,en}`。**不跑会在 catalog 里留孤儿条目**

## 5c. 顺带清掉的死代码

- [x] 5c.1 删 `SessionStore::reset_file_checkpoint` 及其两个实现
      （`storage-sql` 的 trait impl + `ops.rs` 自由函数、`crates/web/src/store.rs`）。
      3.4 之后它**生产调用点归零**；留着不只是死代码，更会误导——下一个人会以为
      checkpoint 有重置场景，而本 change 刚论证过重置是错的

## 5d. 自审出的两处（写完代码后回看）

- [x] 5d.1 提取 `discard_published_staging`——「删已发布的暂存、失败只 warn」在
      SAF 分支与本地 copy 回退里各写了一份，逐字相同
- [x] 5d.2 **跨卷 copy 回退前先 `remove_file(target)`**。`rename` 替换的是符号链接本身，
      而 `copy` 会**跟随**它写进被指向的位置——回退路径会把 `ensure_within` 刚挡住的东西
      又放进去。桌面只有 rename、没有这个面，是移动端引入 copy 回退时带进来的

## 5e. `/simplify` 四路审查的处置（reuse / simplification / efficiency / altitude）

### 已修

- [x] 5e.1 **空文件在续传中丢失的回归**（审查副产物，最严重的一条）。「收齐即发布」由
      数据块触发，而空文件的块只在**首次** `full_fetch_plan` 里；续传的 `build_fetch_plan`
      按字节 range 推导，对 `size == 0` 产生不出 range，`ensure_files_complete` 又对它放行
      ——中断后重来的空文件会静默丢失。新增 `publish_pending_empty_files` 在 Finish 处补住，
      并加两条守卫钉死：e2e `e2e_empty_file_is_published_even_when_interrupted_before_its_block`
      （构造「中断时空文件一次都没被碰过」并断言恢复后它有 `local_path`），
      以及 `checkpoint::empty_file_is_byte_complete_but_not_yet_published`
      （钉住 `file_is_complete` 与 `ensure_files_complete` 在空文件上**故意不同**——
      把前者当后者用正是这次回归的根源）
- [x] 5e.2 删掉 `published: HashSet`，判据改问位图（`checkpoint::file_is_complete`）。
      三个 agent 从复用/简化/层次三个角度独立指向同一处：一个事实两份表示，靠约定同步
- [x] 5e.3 `write_all_at` 的 Windows 分支补 `WriteZero` 护栏——从桌面复制时漏了，
      `seek_write` 返回 0 会死循环
- [x] 5e.4 **端口契约本体**（`crates/host/src/ports.rs`）：`finalize_sink` 的文档还写着
      「校验并最终化」而新 spec 说 SHALL NOT verify；四条跨端不变量只写在三个实现文件里。
      现已写进 trait 文档，并**显式声明 Web 实现的缺口**（它没有 staging 可保留）
- [x] 5e.5 删 `open_or_create_sink` 的默认实现——它转调 `create_sink` 会在续传时截断暂存，
      而整文件校验已删、core 也不再 reset checkpoint，继承默认实现会静默产出有洞的文件。
      删掉后**立刻在编译期抓到 4 个未 override 的测试替身**
- [x] 5e.6 `publish_to_local` 的 copy 失败路径补「删目标半成品」——spec 与 design D6 都要求，
      此前只有 SAF 分支做了
- [x] 5e.7 `delete_finalized_file` 接入 scheme 分派：`file://` 走 Rust，不再绕 JS
- [x] 5e.8 `StagingArea`：`create_dir_all` 收进 `new()`（每文件一次白跑的 syscall），
      `open` / `discard` 改 `spawn_blocking`——模块文档自己写了「不能在单线程 runtime 上
      同步 IO」，却只有 `write_at` 遵守了
- [x] 5e.9 `persist_chunk` 返回 `Option<Vec<u8>>` 而非 `bool`，`publish_file` 不再重新查表
      + 克隆同一份位图；顺带删掉它那条不可达的「sink 缺失则静默成功」分支
- [x] 5e.10 `SinkOp` 改元组变体（`offset` 从未被读），测试里两个近乎相同的闭包合成一个
- [x] 5e.11 注释考古：`failure.rs` 的模块文档是 `//!`，会**逐字导出进两份 bindings.ts**——
      前端开发者读到的是一段解释「某个他们从没见过的变体为何被删」的讣告。精简为仍然
      生效的那条约束。`finish_data_channel` / `file_source.rs` 同类
- [x] 5e.12 `errorDetail` 拆成 `rawMessage` + 截首行两层，去掉三层嵌套三元
- [x] 5e.13 `publish` 里 `save_dir.clone()` 改借用；`CoreSaveLocation::Path` 的文档补上
      「移动端可能是 SAF `content://`」——Rust 侧已经在嗅探这个前缀，而类型文档还在否认

### 已跳过（附理由，避免下次重复评估）

- [ ] 5e.14 **`ensure_within` / `write_all_at` / sink 注册表下沉到共享位置**（reuse #1/#2/#3、
      altitude #7）。`ensure_within` 是安全边界、两份已经开始漂，确实该收口；但 `crates/host`
      必须 wasm-clean，放不下 `tokio::fs`，要么新建 native-only crate、要么给 host 加
      `cfg(not(wasm))` 模块。属于独立的结构调整，不塞进这个以修 EBADF 为核心的 change
- [ ] 5e.15 **发布失败的原因退回了无类型 `error: String` 通道**（altitude #3）。见 proposal
      Impact 里改写过的那段——需要让可恢复中断也能携带判别码，动状态机 + 三端 UI，单独立项
- [ ] 5e.16 `Legacy` 被复用来装退役的结构化变体（altitude #4）：存量行会显示成一段 JSON。
      与 5e.15 同源，一并处理
- [ ] 5e.17 `MemoryHost` 未把 staging 与已发布建模成两份（altitude #9），所以没有测试能抓住
      「发布失败却丢了 staging」的 host。要改的是共享测试替身，影响面广于本 change
- [ ] 5e.18 **SAF 发布串在接收读循环里**（efficiency #1）：多文件传到 SAF 时，每个文件的
      全量拷贝期间读循环停摆、连 `Window` 都不回，发送端一起等。改成后台 task 需要重新安排
      「finalize 与 mark_file_completed 之间不插 await」那条不变量，风险高于收益
- [ ] 5e.19 `copyIntoTarget` 每 4 MiB 跨 JSI 搬两遍（efficiency #4，300 MB ≈ 600 MB memcpy）。
      根治要在 expo patch 里加原生 `copyFrom`（Kotlin `transferTo`），扩补丁范围
- [ ] 5e.20 `readSourceChunk` 每次 open/close + `slice()` 冗余拷贝（efficiency 顺带）：
      bao outboard 按 16 KiB 读，300 MB 的 prepare 就是 ~19200 次 JSI 往返。**既有问题**，
      且属 proposal 已列的非目标「发送侧下沉」，单独立项
- [ ] 5e.21 把四个 per-file map 收进 `ChannelState` 结构体（simplification #2）。删掉
      `published` 之后参数已降到 7 个，收益变小；`started_files` 也可推导，但那是既有代码

## 5f. `/code-review high` 的处置（第三道关，15 条）

> 这一关抓到的东西比前两关更硬：**其中一条让整个 change 的目标场景完全失效**。

### 已修

- [x] 5f.1 **SAF 发布路径根本跑不通**（最严重）。我在 port 文档里写死了「`staging_path`
      不带 `file://`」，而 expo 的 `JavaFile` 构造是 `File(URI.create(uri))`——无 scheme
      的裸路径抛 `URI is not absolute`。**用户把接收目录设成系统文件夹时，每个文件的发布
      都会失败**，正是这个 change 要修的那个配置。改为传 `file://` URI（参数更名
      `staging_uri`，两端同步 + 重生成绑定）
- [x] 5f.2 **percent-encoding 全线缺失**（3 条合一）。`to_host_uri` 直接拼原始路径、
      `parse_host_dir` 不解码，而旧的 JS 实现返回的是 expo 编码过的 `file.uri`。后果：
      含空格/中文的文件名在 iOS `Sharing.shareAsync`（`URL(string:)` 返回 nil）、
      Android `new File(uri)`（`URI.create` 抛异常）、`<Image source={{uri}}>` 上全线失效；
      iOS 用户选一个名字带空格的接收目录会导致文件全落进一个字面叫 `%20` 的新目录。
      两个函数都补上编解码 + 三条往返测试（含字面 `%`）
- [x] 5f.3 **升级路径的静默数据丢失**。旧实现在每个文件末块就刷完整位图、发布推迟到会话
      结束，所以库里存在「位图完整 + `local_path` 为空」的行。新代码把「位图完整」当作
      「已发布」，续传会跳过它们 → 永不落地 → 会话报成功。修在唯一的载入点
      `build_file_infos_and_bitmaps`：未发布的行清掉最后一块，让它重走「收齐 → 发布」。
      **这条同时推翻了 `/simplify` 的一个建议**——它论证「`published` 集合可从位图推导」，
      那只在新代码内部成立，对存量数据不成立
- [x] 5f.4 `create_dir_all` 先于 `ensure_within`，会跟着符号链接把目录建到保存目录外
      （写入被拦住，但目录留下了）。改为 `create_dir_within` 逐层建、逐层验，
      并加多层逃逸测试
- [x] 5f.5 发布失败后暂存永久泄漏：sink 已从 host 表摘除，取消时的 `cleanup_sink`
      成了 no-op。移动端靠「`sink_id` 就是暂存文件名」补删；桌面改为失败时把
      `PartFile` **放回表里**
- [x] 5f.6 知识库两个「相关文件」块相邻错位（`/dev-workflow` 把这个文件当架构事实源加载）

### 已记录未修

- [ ] 5f.7 **删掉整文件 BLAKE3 后失去了最后一道网**（review #3/#4）：暂存被外部删除/截断、
      或「发布成功但 `mark_file_completed` 失败」时，续传只补缺失块，产出长度正确、
      内容有洞的文件而无人察觉。design 里原记为「进程被杀」的窄窗口，review 指出**普通
      DB 错误**同样可达。根治要 `open_or_create_sink` 回报「新建还是续上」，属端口签名变更
- [ ] 5f.8 失败判别码退回无类型通道（review #8/#9，与 5e.15/5e.16 同源）
- [ ] 5f.9 `file_is_complete` 在每块前多一次 O(total_chunks) 位图扫描（review #13）。
      可折进 `persist_chunk` 已有的计算，但那会让「守卫」与「记账」耦合，收益不抵
- [ ] 5f.10 `publish` 里那条自称不可达的 `save_dir` 缺失分支（review #15）——留着它比
      让 `sink_id_for` 与 `publish` 共享一个取值 helper 更省事，但确实是重复校验

## 6. 门禁与真机验证

> **门禁在 `/code-review` 的修复之后完整重跑过一轮**（fmt / test / clippy / wasm 双 target /
> wasm 测试 25 项 / 桌面 tsc + vitest 198 项 / mobile typecheck + lint），全绿。

- [x] 6.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo test --workspace`
- [x] 6.2 `cargo clippy --workspace`
- [x] 6.3 `./scripts/check-wasm.sh`（core 改动必须不破坏浏览器 target）
- [x] 6.4 `mobile/` 下 `pnpm typecheck` + `pnpm lint`
- [x] 6.4b 根目录前端全套（dev-workflow 清单里这几条**没有 CI 兜底**，漏跑无人告知）：
      `pnpm exec tsc --noEmit` / `pnpm test`（29 files·198 tests）/ `check:zustand-access` /
      `check:clipboard` / `check:shared-view` / `check:landing`，全部通过
- [x] 6.4c `./scripts/test-wasm.sh` —— 改了 `crates/web/src/store.rs`（删 `reset_file_checkpoint`），
      `check-wasm.sh` 只保证编得过，这条才是那 20 条 wasm 测试唯一的执行者
- [ ] 6.5 真机：接收目录设为系统 `Download`（SAF），接收 >300 MB 单文件走完全程
- [ ] 6.6 真机：同上场景中途断网 → 恢复 → 续传成功且不从头开始
- [ ] 6.7 真机：多文件接收传到一半取消 → 已收齐的文件在目标目录**保留**，
      未收齐的 staging 被删（spec「取消传输后已发布的文件保留」）
- [ ] 6.8 真机：接收含 `.md` 文件的传输到 SAF 目标，确认落盘文件名**没有**被追加 `.txt`
- [ ] 6.9 真机：杀掉 app 后重启 → 恢复未完成的接收 → 从 staging 续传
- [ ] 6.10 桌面：接收大文件确认功能不回归（3.1–3.3 删掉校验之后）
- [ ] 6.11 记录改造前后的接收吞吐（同一对设备、同一文件），供 design D9 的归因使用——
      **但不以此判定本 change 成败**

## 7. 收尾

- [x] 7.1 更新 `dev-notes/knowledge/rust-backend.md`：记录「接收 staging 恒为应用私有目录、
      热路径不跨 FFI」这条边界，以及 SAF fd 不可用于随机写的根因
- [x] 7.2 `CLAUDE.md` 的 `crates/host` 端口描述若提到文件 IO 归属，同步更新
- [x] 7.3 确认 proposal 非目标里列的四项仍未被偷偷做掉（发送侧下沉 / profile / 收件箱
      per-file / Web staging）
