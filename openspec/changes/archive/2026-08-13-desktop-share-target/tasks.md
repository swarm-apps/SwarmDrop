## 1. Rust 事件与 pending 缓冲基建

- [x] 1.1 在 `src-tauri/src/events.rs` 定义 `ExternalFileOpen { paths: Vec<String> }` 事件类型（specta `Event` 派生），并加入 `setup.rs` 的 `collect_events![]`
- [x] 1.2 新增 pending 缓冲：`Mutex<Vec<PathBuf>>` state，在 `setup()` 中 `app.manage(...)` 注入
- [x] 1.3 新增归一化工具函数：`file://` URL → 本地绝对路径（percent-decode + Windows UNC/盘符处理），过滤不存在的路径
- [x] 1.4 新增 `take_pending_external_open()` 命令（取走即清空），注册进 `collect_commands![]`
- [x] 1.5 新增「收到一批路径」的统一入口函数：约 200ms 去抖合并 → 存入 pending 缓冲 → emit `ExternalFileOpen`

## 2. 三平台捕获入口

- [x] 2.1 macOS：将 `lib.rs` 的 `.run(generate_context!())` 改为 `.build(...)?.run(|handle, event| ...)`，匹配 `RunEvent::Opened { urls }` → 归一化 → 走 1.5 入口（`#[cfg(target_os = "macos")]`）
- [x] 2.2 Windows/Linux 冷启动：`setup()` 中读 `std::env::args()`，解析出存在的文件/文件夹路径 → 走 1.5 入口
- [x] 2.3 Windows/Linux 热启动：`setup.rs` single-instance 回调里落地当前被丢弃的 `_args`（解析路径 → 走 1.5 入口），同时保留 `show_main_window`
- [ ] 2.4 `cargo build` 通过（✓ 已验证）；手动验证 macOS「Open With」冷/热启动都能进入口打日志（⏳ 需打包安装后在设备上验证）

## 3. 文件关联注册（非默认，三平台各走原生机制）

> 决策：`fileAssociations`（按 `ext`）表达不了「任意文件+文件夹」，故三平台各走原生机制。代码已写，⏳ 需逐平台打包安装后 QA。

- [x] 3.1 macOS：自定义 `src-tauri/Info.plist`（Tauri 按文件名约定 merge）—— `CFBundleDocumentTypes` + `LSItemContentTypes=public.data`(任意文件)`+public.folder`(文件夹)，`Role=Viewer`+`Rank=Alternate`（更正：`Role=None` 会不出现在「打开方式」）
- [x] 3.2 Linux：运行时写 `~/.local/share/applications/swarmdrop-open-with.desktop`（`MimeType=application/octet-stream;inode/directory;`）+ `update-desktop-database`（`external_open::register_platform` linux 分支）
- [x] 3.3 Windows：运行时自注册注册表 shell verb —— `HKCU\Software\Classes\*\shell\SwarmDrop`（任意文件）与 `Directory\shell\SwarmDrop`（文件夹），command = `"<exe>" "%1"`；首启幂等写入（`winreg`，`external_open::register_platform` windows 分支）
- [x] 3.4 `capabilities/default.json`：经查 app 自定义命令/事件走 `core:default`，**无需**额外权限（no-op，已核实）
- [ ] 3.5 三平台各自验证：右键任意文件/文件夹能看到 SwarmDrop，且该类型默认程序不变（⏳ 需逐平台真机；Windows/Linux 代码在 mac 上因 cfg 无法编译验证）

## 4. 前端在途分享状态与入站处理器

- [x] 4.1 新增非持久 `src/stores/share-store.ts`（zustand，`sources: FileSource[]` + set/clear），对标 RN `src/stores/share-store.ts`
- [x] 4.2 `cargo test --test specta_export` 重新生成 `src/lib/bindings.ts`，确认含 `ExternalFileOpen` 事件与 `takePendingExternalOpen` 命令
- [x] 4.3 新增根级 `ExternalOpenHandler`（空渲染），挂在 `_app` 布局：先订阅 `ExternalFileOpen` 事件、再 `takePendingExternalOpen()` 拉取一次（顺序避免竞态丢失）
- [x] 4.4 处理器逻辑：`deviceName` 为空 → toast「请先完成 SwarmDrop 设置」并丢弃；否则包装 `FileSource[]` 灌入 share-store → `navigate('/send/share-target')`

## 5. share-target 发送屏（镜像 /send 双栏）

- [x] 5.1 新增懒路由 `src/routes/_app/send/share-target.lazy.tsx`，用 `TaskPageShell/TaskToolbar/TaskContent` 骨架，`lg:grid-cols-[360px_minmax(0,1fr)]`
- [x] 5.2 左栏 `TaskHeroPanel`「待发送」：项数 + 总大小（tabular-nums）+ `InfoTile` + 紧凑 `FileTree`（复用 `useFileSelection` 的 dataLoader / removeFile，folder 级联移除）
- [x] 5.3 右栏 `GlassPanel`「选择设备」：在线可发送已配对设备单选列表（name + 平台图标 + 在线点 + 连接徽章），三态——节点启动中占位 / 无设备 EmptyState / 列表
- [x] 5.4 底部 `CommandDock`：「选择一个设备」↔「发送给 X」↔ 准备进度条
- [x] 5.5 行为：进屏节点未起自动启动一次；选中设备掉线自动取消选中；发送 = `prepareSend`(进度)→`startSend`→`navigate('/transfer/$sessionId')`；离屏清空 share-store
- [x] 5.6 全部文件移除 → 左栏提示返回、发送禁用；发送失败 → toast 报错并留屏
- [x] 5.7 响应式：`lg` 断点以下双栏塌缩为竖排（文件在上、设备在下），minWidth 360 兜底

## 6. i18n 与收尾验证

- [x] 6.1 所有新增文案用 Lingui（`Trans` / `t`），源 locale `zh`；`pnpm i18n:extract`（✓ 源 catalog 583 条）
- [x] 6.2 `pnpm build`（tsc + vite build）+ `cargo clippy`（新代码无警告）+ `cargo fmt` 通过
- [ ] 6.3 端到端手测：macOS「Open With 文件/文件夹」→ 选设备 → 发送成功；未命名首启丢弃提示；无在线设备空状态；多文件一次打开合并为一屏（⏳ 需运行打包应用）
- [x] 6.4 更新知识库 `dev-notes/knowledge/rust-backend.md`：记录 fileAssociations 三平台差异、Info.plist Role 坑、Windows 注册表 verb、冷启动竞态、验证盲区
