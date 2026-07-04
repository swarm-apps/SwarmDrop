## Context

移动端文件传输 App(RN + uniffi/ubrn 桥接的 Rust core)。接收方把文件写到 `<saveRoot>/<relativePath>`,`relativePath` 在发送文件夹时形如 `<根目录名>/<子路径>/<文件>`(`src/core/file-access.ts:94-137`)。文件真实落盘位置由 host 的 `finalize_sink` 决定并已落 `transfer_files.local_path`(见 [[saf-local-path-root-fix]] 那轮治本)。

当前「打开文件夹」两处都打开配置的存储根,而非文件真实所在目录:
- 传输详情页 `savePathOf(projection)` = `projection.saveLocation`(存储根)。
- 收件箱页 `rootPath` 在 core 里 = `save_location_root(save_path)` = 存储根(`crates/core/src/database/inbox.rs:184,493-497`),其 `parentDirOf(localPath)` fallback 是死代码。

关键约束:SAF `content://` document URI 的 docid 用 `%2F` 编码路径分隔符,**不能**用字符串 `parentDirOf` 从文件 URI 推导可打开的子目录 URI —— 必须由 host 在创建/最终化文件时给出真实目录 URI。桌面 core 是移动端的 **git rev 锁定依赖**(mobile-core `Cargo.toml` 锁 rev `a012c61`)。无存量用户,不留兼容层。

## Goals / Non-Goals

**Goals:**
- 「打开文件夹」定位到收到内容真实所在的容器目录,SAF 与 file:// 皆正确。
- 事实源单一:文件父目录 URI 由 `finalize_sink` 返回、落库、贯穿到投影/收件箱,消费方不再各算各的、不做拼接推导。
- 传输详情页与收件箱详情页行为一致。

**Non-Goals:**
- 不做「每个文件独立 reveal / 高亮选中文件」的更细粒度定位(只需打开容器目录)。
- 不覆盖 Bug 2(canResume 误显「恢复」)—— 已独立前端落地。
- 不为历史 NULL 数据做推导兼容(直接回退存储根)。
- 不改 Android app 私有目录(file://)下「打开文件夹」入口的可见性策略(`canOpenSaveFolder` 保持不变)。

## Decisions

### D1: `finalize_sink` 返回「文件 URI + 父目录 URI」,而非只返回文件 URI

`FileAccess::finalize_sink` 返回值 `AppResult<String>` → `AppResult<FinalizedSink>`,其中 `FinalizedSink { uri: String, dir: String }`。`dir` 是文件父目录的、host 侧唯一诚实且 SAF 合法可打开的目录 URI。

- **为什么**:父目录 URI 是「只有桥另一侧才知道的真相」——SAF 下无法由文件 URI 字符串推导。沿用 [[saf-local-path-root-fix]] 的模式(把真相作为返回值贯穿回来落库)。
- **备选**:①保留 `finalize_sink -> String`,另加 `sink_dir(sink) -> String` 方法 —— 多一次桥调用、多一处生命周期管理,劣。②只返回文件 URI,core 端推导父目录 —— SAF 下不可行,直接否决。
- **RN 实现**:`ExpoFileAccess.openSink` 时把父目录 URI 存进 `OpenSink`:file:// 用 `file.parentDirectory.uri`;SAF 在 `ensureSafSinkFile` 里返回叶子 `currentDir` 并取 `currentDir.uri`。`finalizeSink` 返回 `{ uri: sinkId, dir }`。

### D2: 每文件落库 `local_dir`,容器目录在读模型计算(而非写时聚合)

新增 `transfer_files.local_dir TEXT`(nullable),存**文件的叶子父目录 URI**(即「文件存储位置的父目录」,字面命中需求)。`content_root`/`root_path` 不新增会话级列,而是在 `TransferProjection::from` 与收件箱建条目时,由所有文件的 `local_dir` 计算。

- **为什么**:`local_dir` 是纯事实(与 `local_path` 同构,finalize 时一次写入,receiver 热路径无额外聚合逻辑);「打开哪个文件夹」是 UX 口径,放读模型里两处复用(投影 From、inbox 创建都本就遍历 files)。比较用字符串相等,SAF 安全(不做 URI 切割)。
- **备选**:会话级 `content_root_path` 列 + receiver 增量维护 —— 需在热路径做「首个/是否一致」的写时聚合与并发处理,复杂且易漂移,劣。

### D2a(explore 轮拍板):`content_root_of` 回**纯事实 Option**,兜底交消费方

`content_root_of(files) -> Option<String>`:所有文件 `local_dir` 唯一一致 → `Some(该目录)`,否则 → `None`。**不把 save_path 兜底烤进 core**。理由:烤进兜底会让 `content_root` 一会儿是真相、一会儿是存储根,并造成「core 兜底 + 前端 `?? saveLocation` 双重兜底(前端那句成死代码)」。改纯事实后:

- 投影 `content_root` = `content_root_of(files)`(纯),**前端** `contentRoot ?? saveLocation` 显式兜底(前端持有 saveLocation)。
- 收件箱 `root_path` = `content_root_of(files) ?? save_path`,**兜底落在 core**——因为收件箱前端只拿得到 `rootPath`、拿不到 saveLocation。

### D2b(explore 轮拍板):容器目录取**叶子父目录**,非顶层容器

`content_root_of` 的聚合是「叶子父目录全同→该目录,否则→None」。选叶子父目录(非「顶层收到的文件夹」):①字面命中 todo「文件父目录」;②三个 host 实现统一取 parent,最简;③嵌套多文件夹(album 内还有子目录)时叶子不一致→None→兜底存储根,是可接受的优雅降级。顶层容器语义能修好嵌套但要三端各算「第一段目录」,复杂度不划算。

### D3: 收件箱 `root_path` 语义从「存储根」改为「真实容器目录」+ core 侧兜底

`inbox.rs` 建条目时的 `root_path = save_location_root(save_path)` 改为 `content_root_of(files) ?? save_path`(此时 files 的 `local_dir` 已由 receiver 写入,建条目在会话完成后)。**兜底放 core**(见 D2a):收件箱前端只有 `rootPath`、无 saveLocation。收件箱页 `folderTarget` 因此自动正确,并把死代码 `?? parentDirOf(localPath)` 收敛为直接用 `rootPath`。

### D4: 移动 core 桥暴露 `content_root`,前端 None 时兜底

`MobileTransferProjection` 新增 `contentRoot?: string`(纯事实,可为 undefined)。传输详情页 `savePathOf` → `projection.contentRoot ?? projection.saveLocation.inner.path`(前端持有 saveLocation,兜底落这里)。`MobileTransferProjectionFile` 不暴露 `local_dir`(RN 无需每文件目录,只需一个打开目标),保持桥面最小。

## Risks / Trade-offs

- **[host trait 契约破坏,双 host 实现都要改]** → Memory host、Tauri host、RN host 三处 `finalize_sink` 同步改;桌面单测(`host.rs` 的 memory finalize 测试)一并更新。
- **[ubrn `--and-generate` 冲刷两端原生脚手架]** → 接口变必须重生成 bindings;按 `dev-notes/knowledge/toolchain.md` 流程执行,完成后核对 iOS(`SwarmdropCore.h/.mm`)/Android(`build.gradle`/`CMakeLists`/`com.swarmdropcore/*.kt`)未被换成 example app;干净构建/CI 而非吃缓存假绿。
- **[SAF `currentDir.uri` 是否为可 ACTION_VIEW 的目录 URI]** → `ensureSafSinkFile` 里 `currentDir` 来自 `new Directory(baseUri)` 逐层 `createDirectory`,其 `.uri` 是 SAF tree 下的目录 document URI;与现有 `openSaveFolder`(对 content:// 走 `vnd.android.document/directory` ACTION_VIEW)兼容。交用户手动验证。
- **[跨仓 rev 协调]** → 联调切 path 依赖(4 member 同切);落地后桌面提交推送 + 移动端 bump rev,顺序不能反(否则拉不到新 core)。
- **[多顶层目录回退到存储根]** → 少数「一次收多个不同文件夹」场景仍开根,可接受(有唯一共同容器才下钻,是安全且符合直觉的口径)。

## Migration Plan

1. 桌面仓(path 依赖联调):加迁移 → 改 trait/host 实现/receiver/ops/inbox → `cargo test` 桌面 core。
2. 移动 core 桥:改 `history.rs`/`file_access.rs` → ubrn 重生成 bindings → arm64 重编 → 核对原生脚手架。
3. RN:`foreign-file-access.ts` finalizeSink 返回 dir → 两页打开目标 → `tsc`/`biome`。
4. 回收:桌面仓提交推送 → 移动 `Cargo.toml` git/path 切回 git 并 bump rev → 干净构建。
5. 验证:**不做真机/模拟器验证,交用户手动验证**(收文件夹→打开文件夹应落在子目录;收平铺单文件→落在存储根;SAF 与 iOS 各验一次)。

回滚:`local_dir` 迁移可 `down`;前端回退到 `saveLocation`;trait 契约回滚需同步三 host —— 故落地按上面顺序、桌面 core 稳定后再 bump rev。

## Open Questions(explore 轮已收敛)

- ~~`content_root` 字段命名~~ → 定:新增 `contentRoot`,`saveLocation` 保留原义(配置存储根,仍用于「保存位置」展示与 None 时兜底)。
- ~~容器目录取叶子 vs 顶层~~ → 定:叶子父目录(见 D2b)。
- ~~content_root 兜底归属~~ → 定:纯事实 Option,兜底交消费方(见 D2a)。
- 收件箱 `root_path` 迁移期已有条目(无 `local_dir`)→ 回退存储根,不回填(无存量用户,可接受)。
