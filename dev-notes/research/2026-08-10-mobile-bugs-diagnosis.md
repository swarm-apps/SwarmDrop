# SwarmDrop v0.15.1 真机实测 —— 修复清单（按严重度）

> 下面每条的 file:line 我都亲自打开核对过；与两个 sub-agent 的结论冲突处已标注「**分歧**」并给出我读到的证据。审查判 NEEDS_REVISION 的四条，均以审查意见为准。

## 0. 先纠正一条被两边都读错的事实

**两个会话是同一个文件。** 桌面库实证：

```
1B2D882B…  DreamShaper_8_pruned.safetensors  2132625894  8136 chunks  completed
ED0E20F7…  DreamShaper_8_pruned.safetensors  2132625894  8136 chunks  pending
```

- `1b2d882b` = 手机 → 桌面，**1.99 GB**。探针里的 `blocks=4206` 是**续传补发的后半段**（首轮 3936 块，checkpoint 落在 3930，8136−3930=4206），不是整个文件。
- `ed0e20f7` = 桌面 → 手机，同一个文件传回来。

所以：`slow-source-read` 原诊断说「1.99 GB 单文件」**是对的**；它的 validation 纠正成「1.05 GB、是另一个文件」**是错的**。但 validation 由此得出的**结论仍然成立**——日志里确实没有任何 source URI，「源是 cache 下的 `file://`」是推断不是事实（见 P1 的前置步骤）。

---

# P0 · 移动端收到的文件发布不出去（Bad file descriptor）

**根因一句话：** 上游 `expo-file-system` 的 `forContentURI` 不持有 `ParcelFileDescriptor`，第一次 GC 的 finalizer 就把 fd 关了；本仓修这条的 pnpm patch **从未进过 Android 构建**，因为 SDK 56 默认吃预编译 AAR。

## 我亲自复核的证据（全部通过）

```
$ javap -p <expo.modules.filesystem-56.0.8.aar>/classes.jar/…/FileSystemFileHandle.class
  private final expo.modules.filesystem.FileMode mode;
  private final java.nio.channels.FileChannel fileChannel;
  private FileSystemFileHandle(FileChannel, FileMode);
  # ← 没有 parcelFileDescriptor 字段，没有 describe()
```

```
$ grep -n 'parcelFileDescriptor' node_modules/expo-file-system/android/src/main/java/…/FileSystemFileHandle.kt
56:  private val parcelFileDescriptor: ParcelFileDescriptor? = null
111:      parcelFileDescriptor?.close()
163:  private fun describe(e: Exception): String {
   # ← node_modules 里的源码**是**打过补丁的
```

```
$ python3 -c "print(json.load(open('mobile/package.json')).get('expo'))"
None                                    # ← 没有 expo.autolinking 键
$ expo-modules-autolinking resolve -p android --json
configuration: None
expo-file-system publication={groupId: host.exp.exponent, …, repository: local-maven-repo}
                shouldUsePublicationScriptPath: None
```

三条闭合：`shouldUsePublication` → `true` → `usePublication` → `linkProject()` 被跳过 → gradle 编 AAR，node_modules 里那份被 patch 的 Kotlin 从来没参与构建。日志 `swarmdrop-mobile…log:166,170` 的 reason 是**裸** `'Bad file descriptor'`（无异常类名），正是上游 `e.message ?: "unknown error"` 的输出；同一 commit 的 TS 侧改动（`→ Caused by`、`（已写 N/M 字节）`）却都在日志里 —— JS 生效、Kotlin 不生效，缝就在这。

**同一根因的第二个表现（两边都没并到一起）：** 读路径一模一样。`mobile/src/core/foreign-file-access.ts:127` 的 `new File(sourceId).open(FileMode.ReadOnly)` 在 Android SAF 源上走同一个 `forContentURI`，`FileInputStream(FileDescriptor)` 是 `isFdOwner=false`，`handle.close()` 只关 channel —— **每个 256 KiB chunk 泄漏一个 fd**，2 GB 发送开约 8000 个句柄，靠 finalizer 回收。今天没报错只是运气。

## 修复

### F0-1（必做，最高优先级）让 patch 真正进 Android 构建

| | |
|---|---|
| 改哪 | `mobile/package.json`（新增顶层 `expo` 键） |
| 怎么改 | `"expo": { "autolinking": { "android": { "buildFromSource": ["expo-file-system"] } } }` |
| 工作量 | S–M（要一次完整 prebuild + 重建 APK；`mobile/.gitignore:13` 忽略了 `/android/`） |
| 风险 | 低。不动 wire、不动契约。expo-file-system 改从源码编译，首次构建变慢。唯一风险是将来 SDK 升级改了选项名会**静默**退回 AAR → 必须配 F0-3 |
| 护栏测试 | 无现成测试会红（这块本来零护栏） |

**这条同时修 publish（write）与发送源（read）两条路径 —— 它是根治，F0-2 不是。** 原诊断把 F2（新建原生模块）标成「根治」、F1 标成「当天可验的止血」，按字面执行会让人最终只留 F2，而 F2 完全不碰读路径。以审查意见为准。

验收三条（前两条不用设备）：
1. gradle 配置阶段日志出现 `:expo-file-system` 这个 project；
2. `javap -p node_modules/expo-file-system/android/build/**/FileSystemFileHandle.class` 断言 `parcelFileDescriptor` 字段与 `describe` 方法存在 —— **这是唯一的机器判据，必须写进 F0-3**；
3. 复现 2 GB SAF 接收应不再失败；若仍失败，reason 会**带类名**（`'IOException: Bad file descriptor'`）。
4. 动手前先抓一次 logcat：未修的构建应能看到 CloseGuard 的 `A ParcelFileDescriptor was not closed` —— 零成本先验证诊断。

### F0-2（降级为可选优化）SAF 发布下沉到原生模块

**不再叫「根治」**，理由改为「2 GB 拷贝离开 JS 线程 + 少 512 次 JSI 往返」。`foreign-file-access.ts:215-245` 的 `copyIntoTarget` 是 512 轮同步 `readBytes`/`writeBytes`（`FileSystemModule.kt` 里它们是同步 `Function` 不是 `AsyncFunction`），`setTimeout(0)` 只让一次事件循环。

只有在 F0-1 落地后仍需要时才做。落地必须补两点：

- **`dir` 返回值钉死成与现状同形**：现状 `foreign-file-access.ts:273` 存的是 `Directory.uri` = tree URI **+ 尾斜杠**；`DocumentFile.fromTreeUri(...).uri` 是 document URI 且无尾斜杠。静默换掉会踢翻收件箱的「打开文件夹」（`saf-intent.ts`）。
- 另外三条契约照搬：重名**复用覆盖**不生成 `foo (1).txt`（publish 必须可重入）、mime 恒 `application/octet-stream`、失败删半成品（`foreign-file-access.ts:164-172` 已有）。

工作量 M/L（新 expo 模块 + prebuild + 无任何现成测试可验）。风险中。

### F0-3（必做）机器护栏：禁止「patch 打在预编译包上」再次静默失效

新增 `mobile/scripts/check-expo-patches.mjs` + CI。三个洞按审查意见补齐：

- 只对**改动触及 `android/`** 的 patch 做 Android 检查（否则纯 JS patch 会假阳）；
- **Apple 侧同款坑**（`platforms/apple/apple.js:20` 也吐 `buildFromSource`，预编译 XCFramework 会吃掉 `ios/` patch）→ 检查要覆盖 `expo.autolinking.apple.buildFromSource`；
- 从 `mobile/pnpm-workspace.yaml:39-40` 读 `patchedDependencies`（位置在这里，不在 package.json），且 resolve **必须在 `mobile/` 下跑**（loader 向上找 package.json，否则命中仓库根那个）；
- `javap` 符号断言从「可选」提为**必选**。

工作量 S，无功能风险。**这次事故的真正教训是「三个 commit、三次以为修好了，全是空的」——只有机器能守住。**

### F0-4（必做）改掉被固化成「架构事实」的错误归因

改：`mobile/packages/swarmdrop-core/rust/mobile-core/src/file_staging.rs:6-22`、`mobile/src/core/foreign-file-access.ts:18-27,199-214`、`dev-notes/knowledge/rust-backend.md`、`CLAUDE.md` 的「暂存→发布两段」段、`dev-notes/research/2026-08-10-transfer-throughput-diagnosis.md:143`，**外加 `mobile/pnpm-workspace.yaml:52-56`**（那段注释今天就是错的：声称补丁允许 SAF `rw`，而 patch 明确保留了上游的拒绝）。

两处必须留手：

- **不要删「绝不 setOffset」这条规则**，只改归因。独立理由仍成立：部分 DocumentsProvider 返回**不可 seek** 的 fd（管道式 `openDocument`），`position()` 一律失败。
- **「暂存 → 发布」两段本身是对的、要留着**，但理由要换成与 fd bug 无关的那几条（SAF/FUSE 随机写慢、避免用户目录出现半成品、跨重启续传、不可 seek fd）。

工作量 S，纯文档。

## 顺带确认（无需再查）：失败后的续传行为是**对的**

`receiver.rs:674-703` 的 `finalize_sink` 错误一律 `?` 上抛不 reset checkpoint；`file_access.rs:236-237` 失败时 `discard_published_staging` 不执行 ⇒ 2 GB 暂存原封不动。日志实证：恢复会话只补了 `blocks=5`（`swarmdrop-mobile…log:169`），1.25 MB，不是 2 GB。

**但严重度不在重传成本上，而在于这是死循环** —— 每次续传都撞同一个 GC bug，文件永远发布不出去，2 GB 暂存一直占着。所以按 P0 修，不按「偶发失败重试即可」处理。

---

# P1 · 移动端读源文件慢 76 倍（一个根因，三个现象）

**根因一句话：** `MobileFileAccessAdapter::read_source_chunk`（`file_access.rs:373-383`）**无条件**把读源委托给 JS，而同一个文件里 `publish`（`:225`）与 `delete_finalized_file`（`:415-421`）都按 `SAF_SCHEME` 分派 —— 读源是唯一漏掉这条原则的；这一跳还用 `invokeBlocking` **硬阻塞整个单线程 tokio runtime**。

## 这一个根因造成的三个现象（不要当三件事）

| 现象 | 数据 |
|---|---|
| ① 发送吞吐上不去 | `read=38904ms/55%`，9.25 ms/256KiB ≈ 27.6 MB/s，桌面同操作 0.118 ms（≈2170 MB/s）—— 慢 78 倍 |
| ② prepare 等几十秒 | prepare 走同一个端口方法（`bao.rs:209` 每 256 KiB 一次 `read_source_chunk`）。日志侧证：05:15:31 app 启动 → 05:16:37 发 offer，**65.8 s** 的窗口里塞下了整个 prepare |
| ③ 每块冻住网络事件循环 9 ms | `swarmdrop_mobile_core.cpp:2760/2891/3053` 用 `invokeBlocking`；`UniffiCallInvoker.h:58-76` 就是 `invokeAsync(wrapper); future.wait();`。TOKIO1 是 `new_current_thread`（`rust-backend.md:1049-1064`）⇒ 一次 1.05 GB 传输累计冻结 **38.9 s** libp2p/WebRTC 事件循环 |

现象 ③ 值得单独提醒：它是本次两条 `connection lost` / `io: timed out` 的**嫌疑诱因**，而不只是性能问题。这把 P1 从「慢」推到了「可能导致传不完」的边缘。

**base64 假设已被证伪**：`ffi-converters.ts:340-353` 是 `Uint8Array.set()` 原始字节；`swarmdrop_mobile_core.cpp:6664` 明说 `rustbuffer_alloc` 是「Uint8Array view over Rust-owned memory」。真正的代价是线程跳转 + 4 次 256 KiB 拷贝 + 每块 2 个 expo SharedObject。

**「≥85% 是桥不是磁盘」有同机硬证据**：同一台手机纯 Rust POSIX pwrite 到同一 `/data` 分区是 `write=11729ms/8135 = 1.44 ms`（`…log:165`）。闪存读不会比写慢 6.4 倍。

## F1-0（**动手前必做**，S）先证明分派前提

**这是本条的最大未知数，两个 agent 谁也没证明它。** 日志里没有任何 source URI。在 `MobileFileAccessAdapter` 加一条**每个 source 只打一次**（`Mutex<HashSet<FileSourceId>>` 去重，别每块打）的 DEBUG 日志，记录 scheme 与走了哪条分支。

我能给的侧证只有一条，且**指向不利方向**：05:15:31 重启 → 05:16:37 发 offer 只有 65.8 s，装不下「DocumentPicker `copyToCacheDirectory: true` 复制 2 GB + prepare 8136 块」。所以本次的源**大概率不是 cache 下的 `file://`**，更可能是 `content://`（Android 选目录发送，`file-access.ts:95-97,108`）或**收件箱转发**（`inbox-file-availability.ts:80` 的 `sourceId: file.localPath`，Android 上落点是 SAF ⇒ 也是 `content://`）。

**如果确实是 `content://`，F1-1 对本次实测场景一个字节都不改善。** 这条日志决定后面做哪一个，不能跳。

## F1-1（核心，M+/L）读源按 scheme + 归属分派

| | |
|---|---|
| 改哪 | `mobile/.../mobile-core/src/file_access.rs:373-383`（分派）、`:198-209`（加 `owned_roots`）、`app.rs:63-87`（constructor 加参）、`mobile/src/core/mobile-core.ts:48-54`（传入） |
| 工作量 | **M+/L**（不是 M）—— 改 uniffi 导出签名要 `ubrn build android --and-generate` **和** `ubrn build ios --and-generate` 各跑一遍并提交重生成的 `cpp/generated/*` + `src/generated/*`；iOS 无 CI、要 Mac |

关键改动（以审查后的版本为准）：

```rust
// 白名单**由宿主给出，不在 Rust 侧推导**——两个平台沙箱布局不同，推导必然漏。
pub(crate) fn new(foreign: Arc<dyn ForeignFileAccess>, data_dir: &Path,
                  owned_source_roots: &[String]) -> Self { … }

fn owned_source_path(&self, source: &FileSourceId) -> Option<PathBuf> {
    if source.0.starts_with(SAF_SCHEME) { return None; }
    let path = crate::utils::parse_host_dir(&source.0);
    // Path::starts_with 按 component 比较但**不规范化**，`<cache>/../../..` 会通过。
    // 与本文件发布侧的 ensure_within 保持同一条纪律。
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) { return None; }
    self.owned_roots.iter().any(|r| path.starts_with(r)).then_some(path)
}

async fn read_source_chunk(&self, source: &FileSourceId, offset: u64, length: usize) -> AppResult<Vec<u8>> {
    if let Some(path) = self.owned_source_path(source) {
        // 必须 spawn_blocking：runtime 是 new_current_thread（理由见 file_staging.rs:130-133）
        return tokio::task::spawn_blocking(move || read_at_sync(&path, offset, length)).await
            .map_err(|e| AppError::StorageFailed(format!("读源任务失败: {e}")))?;
    }
    self.foreign.read_source_chunk(source.0.clone(), offset, length as u64).await.map_err(to_app_error)
}
```

`read_at_sync` 照抄 `src-tauri/src/host/file_source/path_ops.rs:60-79`。

JS 侧传入的根目录列表（**一次性做成 `Vec<String>`，别做成单个 `cache_dir`**，uniffi 签名只付一次代价）：

```ts
const ownedSourceRoots = [
  Paths.cache.uri,      // DocumentPicker / ImagePicker / Android share-intent
  Paths.document.uri,   // iOS = 收件箱落点（转发复用它作 sourceId）；Android = 私有数据区
  ...(Platform.OS === "ios" ? [shareExtensionContainerUri()] : []),  // App Group 容器在 app 容器**之外**
];
```

**⚠️ iOS 安全作用域 URL**：`pickDirectoryAsync` 给的 URL 被 expo 包了 `startAccessingSecurityScopedResource()`，Rust 裸 `open()` 会 EPERM。**白名单正是为此存在，绝不能放宽成「任何 `file://` 都直读」。**

**风险**：中。不动 wire、不动 `crates/*`（`check-wasm.sh` 不受影响）、不动接收侧任何不变量（改的是 source 方向，「暂存→发布」「bitmap 完整⟺已发布」「finalize 失败不 reset checkpoint」「随机写只用自有 fd」全在接收侧）。

**护栏测试**：现有的都不会红（`file_access.rs:441,474` 两条测的是自由函数 `publish_to_local`）。要新加 7 条，照 `path_ops.rs:139-182`：精确等长读 / 尾部截断到 EOF / offset 越界返回空 / **`length == 0` 返回空且不报错**（`sender.rs:528-532` 对空文件会发一次 `(0,0)` 读，桌面那份测试没覆盖，现在的 JS 路径是碰巧对的）/ `content://` 仍走 foreign / 白名单外 `file://` 仍走 foreign / 含 `..` 仍走 foreign。

## F1-2（S）删掉 JS 读路径上那次多余的 256 KiB 拷贝

`mobile/src/core/foreign-file-access.ts:132-135` 的 `bytes.buffer.slice(...)` 是白拷 —— `JNIToJSIConverter.cpp:16-25` 交回来的就是刚 `new ArrayBuffer(size)` 的独占 buffer。改成 `byteOffset === 0 && byteLength === buffer.byteLength` 时直接返回。**保留 guard**（iOS 侧的 `readBytes` 实现未核实）。

**定位是删死代码，不是提速** —— memcpy 在 9.25 ms 里占比远小于线程跳转，别写进 changelog 的性能条目。

## F1-3（**不做**）`content://` 缓存 handle

原提案的前提被同一份日志证伪：`…log:166,170` 显示 SAF fd 在**纯顺序 `writeBytes`**（`copyIntoTarget` 刻意不 `setOffset`）中就死了两次。「不 lseek 就安全」不成立。且它要给 `crates/host/src/ports.rs` 加 `close_source`（波及 10 个 `impl FileAccess`），而 **`Drop` 调不了 async 端口方法**。

重启条件：① F0-1 落地、② F1-0 证明源确实是 `content://` 且读仍 >5 ms。届时正确的做法是**跟着 F0-2 一起下沉到原生模块**，而不是在 JS 侧缓存句柄。

## F1-4（S，**独立 PR**）blake3 不吃 `-Oz`

`Cargo.lock` 锁 blake3 1.8.5，`build.rs:366-370` 在 aarch64 上默认开 NEON C intrinsics，cc 把 `OPT_LEVEL` 原样透传 ⇒ 整份 `[profile.mobile-release]` 的 `opt-level = "z"` 变成给 `blake3_neon.c` 的 `-Oz`。实测：移动端 bao+blake3 218 MB/s vs 桌面 1082 MB/s。

在 **workspace root** `Cargo.toml` 加 `[profile.mobile-release.package.blake3] opt-level = 3`（member 自己的 profile 会被 Cargo 静默忽略）。注释别只怪那个 `.c` 文件 —— `opt-level="z"` 同样压着 blake3 的 Rust 侧与 `bao-tree` 遍历。必须用 `cargo build -v | grep blake3_neon` 验 clang 拿到的是 `-O3`。

**必须与 F1-1 分开发** —— 同批会让前后对比无法归因。这是 prepare 修完读之后的**新地板**，不是当前的 9.25 ms。

## 预期收益（用下一次探针验收）

- read 9.25 ms → <1.5 ms/块（上限锚点是同机 pwrite 的 1.44 ms）；发送吞吐 14.8 → 20–28 MB/s（届时 `ack` 的 6.9 ms/块成为新的墙，且它本身有一部分是被阻塞读推高的）。
- prepare(1.99 GB)：~60 s → ~15 s → 加 F1-4 后 ~9 s。
- 副产品：不再每块冻住网络事件循环。

---

# P2 · 续传时发送端进度从 0% 开始

**根因一句话：** 发送端续传的进度基线读的是 `transfer_file.transferred_bytes`（`plan.rs:94-118`），而那一列只在**优雅**路径写；进程被杀（Android 后台回收是常态）就是 0，于是条子从 0 爬到 51.7% 就宣告完成。

## 先证伪「续传真的重传了整个文件」

算术闭合，两端探针独立给出同一个数：DB `total_chunks=8136`，首轮落库到第 3930 块（`receiver.rs:39` `CHECKPOINT_INTERVAL=10` + `:639`），续传推 4206 块，**8136 − 3930 = 4206，一块不多**。`fetch_plan` 只含未完成 range。**协议是对的，坏的只有 UI。**

## 我核实的代码（现状）

```rust
// crates/transfer/src/flow/resume/plan.rs:94-118
pub(crate) fn build_sender_resume_state(files: &[entity::transfer_file::Model]) -> HashMap<u32,(u32,u64)> {
    files.iter().filter_map(|f| {
        let transferred = f.transferred_bytes as u64;
        if transferred == 0 { return None; }        // ← 被杀 ⇒ 0 ⇒ 整条被丢掉
        …
        let chunks_done = if transferred >= file_size { total_chunks }
                          else { (transferred.div_ceil(chunk_size)) as u32 };
```

写入该列的路径共 **4 条**（原诊断说 2 条）：`sender.rs:334` on_completed、`sender.rs:391` on_interrupted、`flow/send.rs:315` pause_send、`flow/receive.rs:470` handle_pause_impl —— 全是优雅路径。启动清理只 dispatch `Startup(FoundActiveSession)`，不补进度。对照组：接收侧 `receiver.rs:305-311` 从每 10 块落库的 bitmap 现算，掉电也在。

**这是三端共享代码，修一次修三端**（桌面很少被杀所以没暴露；Web 端因「非终态发送会话不落库」没有发送续传，但同页面生命周期内有）。

## F2-1（主修，S）基线改从 `fetch_plan` 推导

删掉 `build_sender_resume_state`（`pub(crate)`，只有 `mod.rs:378` 一个调用点），换成 `build_sender_resume_state_from_plan(files, fetch_plan)`。

**⚠️ 必须数块，不能 `bytes / CHUNK_SIZE`。** 提案的 floor 版本在「短尾块已收到而中间有洞」时会**少算一块**，`update_file_chunk` 就永远到不了 `chunks_done >= total_chunks`，该文件**永不转 Completed**、`completed_files` 卡在 n−1。正确式子（对齐接收侧的 `count_completed_in_bitmap`）：

```rust
// missing[file_id] = (缺的块数, 缺的字节数)，块数用 floor(first)…ceil(last) 兜住非对齐输入
let chunks_done = calc_total_chunks(size).saturating_sub(missing_chunks);
let bytes_done  = size.saturating_sub(missing_bytes);
(chunks_done > 0 || bytes_done > 0).then_some((file_id, (chunks_done, bytes_done)))
```

**⚠️ 注释里别写「plan 已由 `validate_fetch_plan` 保证对齐」——发起侧不成立。** `build_fetch_plan` 只调 `validate_checkpoint`；对齐是**对端**在 `validate_resume_commit` 里查的，而 `register_resume_actor` 在 `mod.rs:83`、`request_resume_commit` 在 `mod.rs:91` —— 算基线时对齐还没被任何人校验过。

穿参（我已核对两个调用点都持有 plan，且借用时序没问题）：
- `mod.rs:350` `build_sender_actor_for_resume` 加 `fetch_plan: &[FileRange]`；
- `mod.rs:395` `register_resume_actor` 同加，Send 分支透传（Receive 分支忽略，它有自己的 bitmap 源）；
- `mod.rs:83`（在 `:89` clone、`:92` move 之前，借用 OK）与 `mod.rs:460`（在 `:463` move 之前）。

## F2-2（补主修，S）基线落库一次 —— **折进 `build_sender_actor_for_resume`，不要放在 dispatch 之后**

提案放在 `dispatch(ResumeCommitted)` 之后是错的：`dispatch` 自己会重读 DB 再 emit 一份 `TransferProjection`（`coordinator.rs:412-417`），写在后面那份仍是 0；而移动端 `resume_transfer` 在 `initiate_resume` 返回后又重读一次（`mobile-core/src/history.rs:373-378`）→ 两个来源打架，看谁后到。**写在算完基线的地方，两份都对**，而且发起侧与被动发送侧（`mod.rs:460`）共用一条路，不会漏。

已知残留（写进 PR 说明，别让下一个人当 bug 查）：`ResumeInfo.transferred_bytes`（`mod.rs:122` 的 `build_resume_file_infos`）读的是更早载入的内存快照，回写后仍是 0；前端只取 `direction`/`sessionId`，纯装饰。

**一个用户可见的行为变化要写明**：优雅暂停后 DB 可能比对端 checkpoint 高几块（3936 vs 3930），回写会把数字**调小** —— 诚实，但会看到进度轻微倒退。

## F2-3（呈现，S/S/M）prepare 条与传输条必须能区分

**截图上那条 18% 不是续传、也不是传输进度。** 它是 `PrepareProgressBar`（`share-target.tsx:236-238`），续传路径根本不渲染它。而它与传输条**共用同一个视觉原语**（`shared.tsx:199` 默认 `fillClass="bg-primary"`），且背靠背出现（prepare 完 → `router.replace` 到详情页 → 又一条同色条从 0 起）。在 1.99 GB / 60 s 的量级上，这足以让用户把 prepare 读成传输、把「新发送」读成「续传从 0 开始」。

- 移动端 `prepare-progress-bar.tsx:47`：`fillClass="bg-muted-foreground/60"` + 文案带 `{percent}%`。**S**
- 桌面 `src/routes/_app/send/-components/prepare-progress-bar.tsx`：`[&>div]:bg-…` 能盖过 Indicator，但 **track 还是 `bg-primary/20`**（`ui/progress.tsx:16`）→ 会得到「灰填充 + teal 底槽」。要么 root 同改，要么给 `Progress` 加 `indicatorClassName`。**S**
- Web `docs/app/app/_components/progress-bar.tsx`：填充色**硬编码** `bg-[var(--brand-solid)]` 且与传输条共用、文案是另一句、组件自己从 store 读不收 props ——「三端同一份改法」**不成立**。**M**

改 msgid 会让三份 catalog 的 en 译文作废，需要重译。

语义定死：**灰 = 本机在准备，还没上网；teal = 真的在传。**

## F2-4（可选，M）续传详情页标出基线

`TransferResumedEvent` 加 `resumed_from_bytes`，条上画基线刻度 + 「已续传 · 从 X 继续」。要重生成三端 bindings，不碰传输帧。主修不依赖它。

## F2-5（顺手拆雷，M）outboard 失效会**静默**全量重读源文件

`mod.rs:364-377` 那条路径本次没触发（resume 发起到首个窗口只隔 76 ms），但一旦命中，移动端 1.99 GB 会静默卡几十秒，症状与「续传挂了」无法区分 —— 而这正是用户会再点一次、再杀一次进程的地方。改用 `build_outboard_from_source_with_progress` 复用 prepare 的进度管道（需把 `PrepareReporter` 提可见性）。

## F2-6（S）DESIGN.md 加第八条 cross-platform 契约

已经是第三个「同一件事三端各实现一份」的东西，而这次的判据从来没写下来过。`### Transfer Progress Contract`，至少四句：prepare/transfer 视觉必须可区分 · 续传基线来自 fetch_plan 不来自 `transferred_bytes` · 续传必须显式呈现基线 · 任何 >3 s 的本机阶段要有百分比或 ETA。同时改掉 `flow/prepare.rs:31-34` 那句「两者在 UI 上是同一条进度条的前后两段」—— 它现在是错的设计意图，留着会被照抄。**契约与实现同 PR 合入。**

## 护栏测试

现有全部不会红（`build_sender_resume_state` 零测试引用；`e2e_transfer.rs:988/1084` 的 `transferred_bytes == 0` 因 fixture 进度本为 0 且 `ResumeInfo` 读内存快照）。新加：
- 两条纯函数单测（CHUNK_SIZE 量级，覆盖「有洞」与「文件完全不在计划里 ⇒ 判 Completed」）；提案里 `offset: 7` 那条会被 `validate_fetch_plan` 直接拒，别照抄。
- **一条 e2e 才是真护栏**（纯函数单测钉不住穿参）：种 Send 方向 suspended 会话、`transferred_bytes=0` 但对端 checkpoint 在第 N 块，断言首个 `TransferProgress` 的 `transferred_bytes >= N*CHUNK_SIZE`。现有 `seed_suspended_session`（`e2e_transfer.rs:288-325`）不支持种进度，要先加参数。**S→M。**

---

# P3 · 两个发送页底部按钮被屏幕底边裁掉

> 这一条的原始诊断在输入里被截断、且**没有走审查**。下面只写我亲自读到的部分，机制的后半段需要一次真机确认。

**先排除一个方向：不是「各写各的数值、需要抽新组件」。** 项目已有唯一规范组件 `BottomActionBar`（`mobile/src/components/mobile/screen.tsx:373-391`），它自己吃安全区 `Math.max(insets.bottom, 12)`，两个发送页都在用（`share-target.tsx:234`、`select-device.tsx:259`）。**新代码复用它，不要手写 `pb-*`，也不要在它外面再套 `SafeAreaView edges={["bottom"]}`**（`screen.tsx:82-84` 已写明会空两遍）。

## 我核实到的两个确定缺陷

**① `share-target.tsx:156` 的 `FlatList` 没有 `flex`。**
`AppScreen ... bare` 的非 scroll 分支把 children 放进 `<View className="flex-1">`（`screen.tsx:128`）。列表在 column 里没有 `flex-1` ⇒ 按内容撑高 ⇒ 内容一长就把 `BottomActionBar`（连同它的 `paddingBottom`）整体顶到屏幕外。而 `select-device.tsx:238` 用的 `FileBrowser` 根节点是 `style={{ flex: 1 }}`（`file-browser.tsx:61,90`）—— **两页在这一点上不对称**，share-target 是坏的那个。

**② `prepare-progress-bar.tsx:32` 根节点是 `flex-1`，而两个调用点都把它放进 column。**
`share-target.tsx:235` 与 `select-device.tsx:260` 都是 `<View className="flex-1 gap-2">`（column）。在 column 里 `flex-1` 作用在**纵向主轴**上（`flexGrow:1 / flexShrink:1 / flexBasis:0%`），这不是它想要的语义 —— 它只是想占满宽度。这解释了「只在**准备中**状态下才没有距底留白」：prepare 条一出现，栏高度变化，把已经贴边的按钮推出可视区。

## 修法

| | |
|---|---|
| 改哪 | `mobile/src/app/send/share-target.tsx:156`（`<FlatList className="flex-1" …>` 或 `style={{flex:1}}`）；`mobile/src/components/transfer/prepare-progress-bar.tsx:32`（`flex-1 gap-2` → `gap-2`，两个调用点都是 column） |
| 工作量 | S |
| 风险 | 低。`flex-1` 是 share-target 相对其他列表页缺的那一项，补齐即与 `inbox/[itemId]`、`transfer/[sessionId]`、`send/shared-files` 一致；`prepare-progress-bar` 去掉 `flex-1` 后仍由 column 的宽度撑满 |
| 护栏测试 | 无（`mobile/scripts/` 只有 `check-zustand-store-access.mjs`） |

**待确认**：修完要在真机上看一眼 `select-device`（它没有缺陷①，如果那页也裁，说明还有第三个机制）。同时确认 `device/[peerId].tsx:424` 用的是 `BottomActionArea`（`screen.tsx:359-368`，`py-4` 无 inset）—— Android 手势区确实没留白，属于同一族问题，建议一并统一。

---

# P4 · relay 重试风暴 + 空 error 字段（可观测性）

**根因一句话：** `ensure_relay_reservation` 拿 `address_book` 的第一条地址（`crates/net/src/actor.rs:789-792`），而那条地址本身就是一条 circuit 地址，`circuit_base`（`:1439-1449`）见到已有 `P2p` 就直接追加 `/p2p-circuit` ⇒ **双层 circuit** multiaddr ⇒ `listen_on` 立即失败 ⇒ 无退避地重试几十次，且 `warn!(error = %e)` 的 Display 渲染为**空**。

日志原文（地址是双层的）：
```
relay circuit listen failed relay_addr=/ip4/192.168.50.105/udp/4001/quic-v1/p2p/12D3KooWCkaj…/p2p-circuit/p2p/12D3KooWMSUf…/p2p-circuit error=
```

修法：
1. `actor.rs:810` 把 `error = %e` 改成 `error = ?e`（`TransportError` 的 Display 在这个变体上是空的）—— **S，一行**；
2. `circuit_base` 或它的调用点拒绝「已经含 `P2pCircuit` 的候选地址」，并给失败路径加退避 —— **S/M**；
3. 加一条单测：`circuit_base` 对含 `p2p-circuit` 的输入不得产出双层。

风险低，不碰 wire。**优先级不高但值得顺手做** —— 它今天正在掩盖一个真实故障，也会污染后续所有诊断日志。

---

# 同一根因的不同表现（不要重复排期）

| 根因 | 表现 |
|---|---|
| **expo patch 没进 Android 构建**（P0） | ① SAF publish 的 `Bad file descriptor`（文件收不到）② 发送侧每 chunk 泄漏一个 SAF fd（尚未爆，2 GB 发送开 ~8000 句柄）③ 日志里 reason 不带异常类名，误导了 2026-08-07 那次归因 |
| **`read_source_chunk` 无条件走 JS 桥**（P1） | ① 发送吞吐 14.8 MB/s 上不去（`read=55%`）② prepare 1.99 GB 要几十秒 ③ 每块冻住 libp2p 事件循环 9 ms、累计 38.9 s（断链嫌疑） |
| **发送端续传基线读 `transferred_bytes`**（P2） | ① 续传进度 0% → 51.7% 就完成 ② 未恢复的会话在列表页显示 0 B ③ 用户误判「重传了整个文件」→ 再杀一次进程 → 更多中断 |
| **prepare 条与传输条同色同形**（P2-3） | ① 截图里被读成「续传进度条」② 用户报的「续传从 0% 开始」有一半其实是这个 |
| **`AppScreen bare` 下列表没 `flex-1`**（P3） | 底部操作栏被顶出可视区，只在 prepare 条出现时可见 |

**跨条目的交叉**：F0-1（`buildFromSource`）同时修 P0 的写路径与 P1 的读路径 fd 泄漏 —— 它是唯一一条同时进两个 P 级的改动，应当最先合。

---

# 本次实测澄清了什么

## ① 症状 B（50 → 7 MB/s 随进度劣化）**没有复现**

两个方向全程平坦，我逐窗口核对过：
- 手机 send（`…mobile.log:87-131`）：14.17 – 15.58 MB/s，32 个窗口无趋势；
- 手机 recv（`…mobile.log:133-164`）：9.23 – 10.38 MB/s，同样平坦。

**这意味着什么**：可以**排除**「传输实现里存在 O(已完成) 的算法、内存/句柄单调增长、bitmap 线性扫描」这一整类猜想 —— 那类东西在 2 GB / 8000 块上一定会显形。

**剩下的候选只有三类**：绑定在**某条特定链路**（浏览器 webrtc-direct / relay circuit，而非本次的 WebRTC 打洞直连，`…mobile.log:15:15:43` 明确 `webrtc hole punching succeeded`）；绑定在**特定端**（Web/OPFS）；或环境性（Wi-Fi 热节流、信道竞争）。

**下一步：不要盲追，但也不要销案。** 把同一套 `swarmdrop_transfer::probe` 探针在**能复现那条链路的场景**上再跑一次即可判定 —— 如果劣化出现在 recv 的 `wait` 上，是链路/对端；出现在 `write`/`ckpt` 上，是本端存储；出现在 send 的 `read` 上，是源。探针已经有了，这次成本是零。

## ② 桌面 → 手机的瓶颈经探针确认是**链路带宽**

桌面 send `ack=204820ms/98%`（25.2 ms/块），手机 recv `wait=172565ms/83%`。两端 CPU/IO 都在打盹：桌面 `read=961ms/0%`，手机 `verify+write+ckpt` 合计 16%。稳定 9.73 MB/s ≈ 78 Mbps。

**含义**：这个方向**再优化两端代码收益为零**。要动的是链路本身 —— 值得查的是「为什么打洞成功后只有 78 Mbps」（是否实际回落到 relay circuit？WebRTC data channel 的 SCTP 窗口？Wi-Fi 上行？），而不是 transfer crate。

顺带一个次级发现：手机 recv 的 `ckpt=12838ms/6%`，而 checkpoint 每 10 块才刷一次 ⇒ **单次 checkpoint 落库 15.8 ms**。目前被 `wait` 掩盖着，但等链路提速后它会浮出水面。现在不动它（`CHECKPOINT_INTERVAL=10` 是「续传最多重拉 9 块」的判据），但记一笔。

## ③ 本次**没有触及症状 A**（Web / webrtc-direct 的 3.3 MB/s），`支路缓冲已满` 零命中**不能**作为 udp_mux 假设被推翻的证据

三条理由，逐条可验：

1. **那条日志根本不在本次的代码路径上。** `支路缓冲已满` 只存在于 `crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs:341` —— 它属于 **`/webrtc-direct` 的 native 监听端 mux**。本次两端是 Android + macOS，走的是 **`/webrtc` 打洞**（`…mobile.log` 05:15:42 `upgrading relayed connection via webrtc hole punch` → 05:15:43 `succeeded`），根本没有实例化那个 mux。**零命中 = 那段代码没跑，不是它没问题。**
2. **本次的瓶颈在更上游，会掩盖任何下游告警。** 桌面→手机每块 25.2 ms 的 ack 意味着发送速率被应用层窗口卡在 ~10 MB/s；即使 mux 缓冲有问题，在这个投喂速率下也不会溢出。要让 mux 承压，必须让它成为最慢的那一环。
3. **症状 A 的现场是浏览器 ↔ 原生**，那条路径上还多出 wasm 侧的 datachannel、OPFS 写、以及本次日志里出现过但**与本次无关**的 `OPFS 错误: exceed its storage quota`（`…mobile.log:34`，那是 02:59 手机 → Web 的会话）。

**要证伪/证实 udp_mux 假设，必须在 Web 端 ↔ 原生 direct 监听端跑一次**，并且同时开 `swarmdrop_transfer::probe` 与 `webrtc_p2p=debug`。在那之前，udp_mux 仍是症状 A 的**未排除**候选。

## ④ 一条负面澄清：这次也**没能**确定移动端发送源的 scheme

日志里没有任何 source URI（`flow/send.rs:63-65` 落库但不打日志），所以「读源慢是因为它跨了 JS 桥」这个**机制**已经证实，但「F1-1 的 `file://` 快路径能不能命中本次场景」**没有证据**。唯一侧证（65.8 s 的窗口装不下「2 GB copyToCache + prepare」）指向 `content://`，即 **F1-1 可能对本次一点忙都帮不上**。这是整份清单里唯一一个「修完可能测不出提升」的风险点，所以 F1-0 被列为动手前的硬前置。

---

# 建议的下一步

## 现在就该修（不需要更多数据）

| 顺序 | 内容 | 工作量 | 为什么现在 |
|---|---|---|---|
| 1 | **F0-1** `expo.autolinking.android.buildFromSource: ["expo-file-system"]` | S–M | 唯一一条「文件收不到」的修复，且同时堵住发送侧 fd 泄漏。判据链已由我实测闭合，零推测 |
| 2 | **F0-3** patch 护栏脚本（含 `javap` 必选断言 + Apple 分支） | S | 没有它，第 4 次「以为修好了」只是时间问题 |
| 3 | **F0-4** 改掉 5 处错误归因 + `pnpm-workspace.yaml:52-56` 那条自相矛盾的注释 | S | 这份文档已经把一次误诊固化成三处「架构事实」 |
| 4 | **F2-1 + F2-2** 续传基线改从 `fetch_plan` 推 + 就地回写 | S | 纯 Rust、一次修三端、不碰 wire、无护栏会红。用户报的「续传从 0% 开始」的另一半 |
| 5 | **F2-3（移动端那份）+ P3** prepare 条换灰 + 带百分比、`FlatList` 补 `flex-1`、prepare bar 去掉 `flex-1` | S | 同一个 PR 合最省事：P3 不修的话，「准备中」那行文字仍被裁掉，只改颜色等于没改 |
| 6 | **P4** `error = ?e` + 拒绝双层 circuit + 退避 | S | 它正在污染后续所有诊断日志 |

**桌面与 Web 的 F2-3 那两份**（桌面 track 同改 / Web 加 fill 参数 + 三份 catalog 重译）可以跟在后面，M。

## 要先收集数据再动手

| 要修的 | 先收集什么 | 怎么收集 | 判据 |
|---|---|---|---|
| **F1-1**（读源分派） | **本次发送源的 scheme** | 在 `MobileFileAccessAdapter` 加每 source 一次的 DEBUG 分派日志（`Mutex<HashSet>` 去重），随下一个 APK 出；或直接在 `flow/send.rs:63` 的 `info!` 里加 `source_scheme` | `file://` 且在白名单内 ⇒ F1-1 直接做；`content://` ⇒ 跳过 F1-1，把 SAF 读一起下沉到 F0-2 的原生模块 |
| **F1-4**（blake3 `-Oz`） | 该日志到底是 release 还是 debug 构建 | `cargo build -p swarmdrop-mobile-core --profile mobile-release --target aarch64-linux-android -v 2>&1 \| grep blake3_neon`，看 clang 拿的是 `-O3` 还是 `-Oz` | 若是 `-Oz` ⇒ 218 MB/s 归因成立，单独一个 PR 修；若已是 `-O3` ⇒ 另找（CPU 本身） |
| **F0-2**（SAF publish 下沉） | F0-1 落地后 2 GB SAF 接收是否还失败 | 复现同一场景，看 logcat 有没有 CloseGuard 的 `A ParcelFileDescriptor was not closed`；以及 reason 是否变成带类名的 `'IOException: …'` | 不再失败 ⇒ F0-2 降级为纯性能优化，可缓；仍失败 ⇒ 立刻做 F0-2，并回头查 provider 侧 |
| **症状 B（劣化）** | 一次能复现劣化的会话的完整探针 | 在**当初出现 50→7 的那条链路**上重跑（大概率是 Web ↔ 原生或纯 relay circuit），同时开 `swarmdrop_transfer=info` | 劣化落在 `wait` ⇒ 链路/对端；落在 `write`/`ckpt` ⇒ 本端存储；落在 `read` ⇒ 源 |
| **症状 A（Web 3.3 MB/s）+ udp_mux 假设** | Web ↔ 原生 direct 的探针 + mux 日志 | 浏览器端拨 native `direct_listener`，两端同时开 `swarmdrop_transfer` 探针与 `webrtc_p2p=debug`；确认 `支路缓冲已满` 是否命中 | 命中 ⇒ udp_mux 假设成立；不命中且 `wait` 主导 ⇒ 看 SCTP/datachannel 层；不命中且 `write` 主导 ⇒ OPFS |
| **桌面→手机的 78 Mbps 天花板** | 这条连接实际走的是打洞直连还是 relay circuit | `swarmdrop_net=debug` 下看数据面用的 connection 的 endpoint 地址；再做一次纯 iperf 对照 | 若是 relay ⇒ 属于选路问题不是传输问题；若是直连 ⇒ 查 WebRTC data channel 的窗口与 GRO/MTU |

## 一条编排纪律

F0-1 是 native 改动，必须重建 APK；F1-1 要重跑两次 `ubrn build --and-generate`。**不要把 F0-1、F1-1、F1-4 混进同一个 APK** —— 三者都会改吞吐，混在一起下一份探针日志就没法归因了。建议三次构建：`F0-1 + F2 + P3 + P4`（一次）→ 重测 → `F1-1`（一次）→ 重测 → `F1-4`（一次）。