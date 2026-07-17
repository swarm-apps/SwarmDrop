## Why

收到「文件夹」类传输后,点「打开文件夹」停在**设置的存储根**,而不是文件真实所在的子目录 —— 用户要多点一层才能看到文件。根因是两个详情页都把「配置的存储根」当作打开目标,而文件真实位置只有 host(finalize_sink)侧知道,当前没有被贯穿回来:

- 传输详情页(`src/app/transfer/[sessionId].tsx`)打开 `projection.saveLocation`(= 存储根)。
- 收件箱详情页(`src/app/inbox/[itemId].tsx`)看似用 `rootPath ?? parentDirOf(localPath)`,但桌面 core 的 `inbox.root_path = save_location_root(save_path) = save_path`(存储根),那条 `parentDirOf(localPath)` fallback 因 rootPath 恒非空而是死代码。→ 两页对文件夹接收都开到根。
- 文件真实位置的唯一事实源是 `finalize_sink` 返回值(已落 `transfer_files.local_path`)。但 SAF `content://` document URI 的 docid 用 `%2F` 编码路径分隔符,**字符串 `parentDirOf` 拼不出可打开的子目录 URI** —— 属于明令禁止的「保存目录 + 相对路径拼接推导」。

## What Changes

- **BREAKING(host trait 契约)**:`FileAccess::finalize_sink` 返回值从 `AppResult<String>` 改为携带「文件最终 URI + 其父目录 URI」的记录。父目录 URI 是 host 侧唯一诚实的、SAF 合法可打开的目录事实源。
- 新增 DB 列 `transfer_files.local_dir`(TEXT, nullable)+ 迁移,存放每个已完成接收文件的真实父目录 URI。
- `TransferProjection` 新增 `content_root: Option<String>`:由所有文件的 `local_dir` 计算(全部同一目录 → 该目录;否则回退 `save_path`)。收件箱条目的 `root_path` 改用同一计算,修正文件夹语义(不再恒等于存储根)。
- 移动 core 桥镜像新字段并重生成 bindings;RN host `finalizeSink` 返回 `{ uri, dir }`(file:// 用 `parentDirectory.uri`,SAF 用创建叶子文件时的 `currentDir.uri`)。
- 前端两页「打开文件夹」改为打开真实容器目录:传输详情页用 `contentRoot ?? saveLocation`;收件箱页因 core `rootPath` 修正而自动正确,并收敛死代码 fallback。
- 无存量用户,不留历史数据兼容层:`local_dir` NULL 时直接回退 `save_path`,不做推导。

## Capabilities

### New Capabilities
- `open-containing-folder`: 「打开文件夹」定位到收到内容真实所在的容器目录(而非配置的存储根),覆盖传输详情页与收件箱详情页;由 core 侧以 finalize_sink 返回的父目录 URI 为事实源贯穿(SAF 与 file:// 皆正确)。

### Modified Capabilities
<!-- 无:mobile-inbox-file-actions 现有需求(open/share)不变;「打开文件夹」此前未被 spec 覆盖,作为新能力引入。 -->

## Impact

- **桌面仓 `/Volumes/yexiyue/SwarmDrop`**:`crates/core/src/host.rs`(trait + Memory/Tauri 实现)、`crates/core/src/transfer/actor/receiver.rs`(finalize 调用)、`crates/core/src/database/ops.rs`(`mark_file_completed`、`TransferProjection`)、`crates/core/src/database/inbox.rs`(`root_path` 计算)、`crates/migration/`(新迁移 + lib.rs 注册)。
- **移动仓 `SwarmDrop-RN`**:`packages/swarmdrop-core/rust/mobile-core/src/{history.rs,file_access.rs}`(桥字段/签名)、ubrn bindings 重生成(`packages/swarmdrop-core/src/generated/`)、`src/core/foreign-file-access.ts`(finalizeSink 返回 dir)、`src/app/transfer/[sessionId].tsx` 与 `src/app/inbox/[itemId].tsx`(打开目标)。
- **依赖协调**:桌面 core 在移动端是 git rev 锁定依赖(mobile-core `Cargo.toml` 锁 rev `a012c61`)。联调期切回 path 依赖(4 个 workspace member 同切,git/path 不可混);落地后桌面仓提交推送 + 移动端 bump rev。
- **构建**:接口变更需 ubrn `--and-generate` 重生成 bindings + arm64 原生重编;该命令会冲刷两端原生脚手架,须按 `dev-notes/knowledge/toolchain.md` 处理并核对脚手架未被换成 example app。
- 不含 Bug 2(canResume 误显「恢复」)—— 已作为独立前端小改直接落地。
