## 1. 联调准备(依赖切换)

- [x] 1.1 mobile-core `packages/swarmdrop-core/rust/mobile-core/Cargo.toml`:按注释把 `swarmdrop-core`/`entity`/`migration`/`swarm-p2p-core` 四个 git 依赖切回 path(`../../../../../SwarmDrop/crates/{core,entity,migration}` 与 `libs/core`),git/path 不可混
- [x] 1.2 确认桌面仓 `/Volumes/yexiyue/SwarmDrop` 工作区干净、停在 rev `a012c61`

## 2. 桌面 core:DB 迁移与落库

- [x] 2.1 新增迁移 `crates/migration/src/m20260704_000002_transfer_file_local_dir.rs`:`ALTER TABLE transfer_files ADD COLUMN local_dir TEXT`(仿 `m20260704_000001_transfer_file_local_path.rs`),down 为 DROP COLUMN
- [x] 2.2 `crates/migration/src/lib.rs` 注册新迁移;若有硬编码「距末尾步数」的测试,按 `up_through(name)+down(1)` 惯例更新
- [x] 2.3 `entity` 的 `transfer_file` model 增加 `local_dir: Option<String>` 字段

## 3. 桌面 core:host trait 契约

- [x] 3.1 `crates/core/src/host.rs`:定义 `FinalizedSink { uri: String, dir: String }`;`finalize_sink` 返回 `AppResult<String>` → `AppResult<FinalizedSink>`,更新 doc 注释(dir = 父目录事实源,禁止拼接推导)
- [x] 3.2 更新 `host.rs` 内 Memory host / Tauri host 两处 `finalize_sink` 实现返回 `FinalizedSink`(桌面父目录 = 绝对路径 dirname)
- [x] 3.3 更新 `host.rs` 的 memory finalize 单测(`memory_host_file_access_should_read_write_finalize_and_cleanup`)断言新返回结构

## 4. 桌面 core:receiver / ops / inbox 贯穿

- [x] 4.1 `crates/core/src/transfer/actor/receiver.rs:516-555`:接住 `FinalizedSink { uri, dir }`,把 `dir` 传给 `mark_file_completed`
- [x] 4.2 `crates/core/src/database/ops.rs:184` `mark_file_completed` 增参 `local_dir: String` 并 `model.local_dir = Set(Some(local_dir))`
- [x] 4.3 `ops.rs:388-461` `TransferProjection` 增 `content_root: Option<String>`;在 `From<transfer_session::ModelEx>` 里由所有文件 `local_dir` 计算(全同 → 该目录;否则 → `save_path` 的 path);`TransferProjectionFile` 保持不暴露 local_dir
- [x] 4.4 `crates/core/src/database/inbox.rs:184`:`root_path` 改用与 4.3 相同的容器目录计算(取代 `save_location_root(save_path)`),`local_dir` 缺失回退 `save_path`
- [x] 4.5 `cargo test`(桌面 core + migration)通过

## 5. 移动 core 桥 + bindings

- [x] 5.1 `packages/swarmdrop-core/rust/mobile-core/src/file_access.rs`:`ForeignFileAccess.finalize_sink` 桥签名同步为返回 `{ uri, dir }` 结构(uniffi Record),映射到 `FinalizedSink`
- [x] 5.2 `packages/swarmdrop-core/rust/mobile-core/src/history.rs`:`MobileTransferProjection` 增 `content_root: Option<String>`(穷尽解构 drift guard 同步);确认 `MobileTransferProjectionFile` 不加 local_dir
- [x] 5.3 ubrn `--and-generate` 重生成 bindings(`packages/swarmdrop-core/src/generated/`)+ arm64 原生重编;按 `dev-notes/knowledge/toolchain.md` 处理
- [x] 5.4 核对 iOS(`SwarmdropCore.h/.mm`)/Android(`build.gradle`/`CMakeLists`/`cpp-adapter`/`com.swarmdropcore/*.kt`)原生脚手架未被 `--and-generate` 冲刷成 example app

## 6. RN host 实现

- [x] 6.1 `src/core/foreign-file-access.ts`:`OpenSink` 增字段存父目录 URI;`ensureSafSinkFile` 返回叶子 `currentDir` 以取 `currentDir.uri`;`ensureLocalSinkFile` 用 `file.parentDirectory.uri`
- [x] 6.2 `finalizeSink` 返回 `{ uri: sinkId, dir }`,类型对齐重生成后的桥 Record

## 7. RN 前端两页

- [x] 7.1 `src/app/transfer/[sessionId].tsx`:`savePathOf`/openFolder 目标改为 `projection.contentRoot ?? saveLocation.inner.path`;`canOpenSaveFolder` 判定用同一目标;DetailCard「保存位置」展示对齐
- [x] 7.2 `src/app/inbox/[itemId].tsx:240-247`:`folderTarget` 直接用 `detail.item.rootPath`(core 已修正为真实容器目录),收敛死代码 `?? parentDirOf(localPath)` fallback
- [x] 7.3 `tsc --noEmit` 与 `biome check` 通过

## 7b. explore 轮收敛(纯事实 Option)

- [x] 7b.1 `ops.rs content_root_of` 去掉 `save_path` 参数,回纯 `Option<String>`(全同→Some,否则→None);投影 From 用 `content_root_of(files)`
- [x] 7b.2 `inbox.rs root_path = content_root_of(files) ?? save_path`(兜底落 core,收件箱前端无 saveLocation)
- [x] 7b.3 前端无需改;desktop core 测试重跑绿(92+15) + iOS/Android 原生增量重编 exit 0
- [x] 7b.4 design.md(D2a/D2b/D3/D4/Open Questions)+ spec 记录叶子父目录 & 纯事实决策

## 8. 回收与收尾

- [x] 8.1 桌面仓提交并推送 develop(ed49a08)
- [x] 8.2 mobile Cargo.toml 切回 git dep bump 到 ed49a08;定向 cargo update;cargo check 通过(sea-orm 仍 rc.38)
- [x] 8.3 用户选择先发 v0.7.5、发版后手动验证(打开文件夹爆炸半径小:最坏 SAF toast 报错优雅降级)
- [ ] 8.4 `openspec archive fix-open-real-content-folder`(用户验收后)
