## Context

移动端（SwarmDrop-RN）已用 `expo-share-intent` 实现「别的 App 分享 → 用 SwarmDrop 打开 → `/send/share-target` 选设备发送」。桌面端缺三样东西：(1) 让操作系统知道 SwarmDrop 能「打开」文件；(2) 把被打开的文件路径送进进程；(3) 一个「文件已定、只需选设备」的反向发送屏。

现状关键事实（已核实）：
- 传输链 `scan_sources → prepare_send → start_send` 只认本地路径，`FileSource` 仅有 `Path{path}` 一个变体，`source_from_id()` 连裸路径字符串都能还原 → **外部路径无需新数据模型**。
- `tauri-plugin-single-instance` 已注册（`setup.rs:141`），回调 `_args` 当前被丢弃——这是 Windows/Linux「已运行时打开文件」参数的天然落点。
- `lib.rs` 入口是 `build_app().run(generate_context!())`，未匹配任何 `RunEvent` → macOS「Open With」的 `RunEvent::Opened` 无人接收。
- **无锁定屏**：身份由 host keychain（`keyring` crate）静默读取，唯一门禁是首启 `_onboarding/device-name`（`index.tsx`：deviceName 空→起名，否则→/devices）。
- 现有 `/send` 页（`index.lazy.tsx`）是「设备优先」：peerId 从 URL 固定、右栏拖文件；task-surface 组件词汇（`TaskPageShell/TaskHeroPanel/GlassPanel/CommandDock/InfoTile/TaskButton` + `FileTree` + `useFileSelection`）齐备。

## Goals / Non-Goals

**Goals:**
- 在三平台把 SwarmDrop 注册为**非默认**的任意文件/文件夹「打开方式」处理器，绝不抢占默认程序。
- 可靠地把被打开的路径（含冷启动 / 已运行 / 多文件 / 多实例）归一化后送到前端。
- 提供与移动端对等的「文件已定、只需选设备」发送屏，视觉与现有 `/send` 无缝一致。
- 后端传输逻辑与数据模型零改动。

**Non-Goals:**
- 不做发送队列 / 离线暂存后发（对方上线即发是另一条独立 TODO）。
- 不做 macOS Services / Windows Share Contract 等原生 App Extension（重活，Tauri 不脚手架）。
- 不做「未引导时缓冲意图、引导后回放」——首启未设名一律丢弃（对齐移动端 v1）。
- 不改动移动端。

## Decisions

### D1. 复用 `FileSource::Path` + 既有发送链，后端零改动
外部拿到的本地绝对路径直接封装为 `FileSource::Path{path}`，走现有 `scan_sources → prepare_send → start_send`。
- **备选**：为「外部来源」新增 `FileSource` 变体或新命令。**否决**：core 完全不关心 source 编码，新增变体是纯负担；路径就是路径。

### D2. 独立整页反向路由，镜像 `/send` 而非改造它
新增 `/send/share-target`（懒路由），复用 task-surface 组件，把 `/send` 的双栏角色对调（左=文件、右=设备单选）。
- **备选 A**：让 `/send` 支持「无 peerId 先选设备」。**否决**：`/send` 强依赖 URL peerId 是其设备优先语义的核心，塞入反向流会让一个组件承担两种相反的信息架构。
- **备选 B**：弹层覆盖当前页。**否决**：DESIGN.md 排斥 modal-first；被用户主动「打开方式」召唤本就是明确上下文切换，无「原页面」需保留。

### D3. 三平台入口 + 归一化 + 单一事件
- macOS：`lib.rs` 的 `.run(ctx)` 改为 `.build(ctx)?.run(|handle, event| …)`，匹配 `RunEvent::Opened{urls}`。
- Windows/Linux 冷启动：`setup()` 里读 `std::env::args()` 过滤出存在的文件/目录路径。
- Windows/Linux 热启动：`single_instance` 回调的 `_args` 落地（当前丢弃），同时保留「唤出主窗口」。
- 三条路径都归一化为本地绝对路径（`file://` URL 需 percent-decode + Windows UNC 处理），汇成一个 `external-file-open{ paths: string[] }` 事件（tauri-specta `collect_events` 声明，前端类型安全订阅）。

### D4. `fileAssociations` = 非默认；Windows 通配符+文件夹走手动注册表
- `tauri.conf.json` `bundle.fileAssociations`：`role=None`、`rank=Alternate`。
- macOS：`LSItemContentTypes` 用 `public.data` / `public.item`（含文件夹用 `public.folder`）。
- Linux：`MimeType` 列常见类型 + `inode/directory`（folder），或 `all/all` 约定。
- Windows：Tauri `fileAssociations` 按扩展名注册 ProgID，**无法**表达「任意文件」与「文件夹右键」。需手动写注册表 shell verb：`HKCU\Software\Classes\*\shell\SwarmDrop`（任意文件）与 `HKCU\Software\Classes\Directory\shell\SwarmDrop`（文件夹），命令 `"path\to\swarmdrop.exe" "%1"`。作为独立 task。
- **备选**：设为默认处理器（`rank=Owner`）。**否决**：抢占用户默认程序极具侵入性，且易触发杀软告警。

### D5. 冷启动竞态：Rust 侧 pending 缓冲 + 前端 mount 时拉取
`RunEvent::Opened` / 冷启动 argv 可能在前端订阅事件之前就发生。做法：Rust 侧把归一化路径存进 `Mutex<Vec<PathBuf>>` state 并 emit 事件；同时提供命令 `take_pending_external_open()`，前端根 `ExternalOpenHandler` 在 mount 时先拉一次（类似 deep-link 的 `getCurrent`），之后靠事件增量接收。取走即清空，避免重复处理。

### D6. 未引导丢弃 + 非持久 share-store
根级 `ExternalOpenHandler`（挂 `_app` 布局，空渲染命令式，对标 RN `ShareIntentHandler`）：拿到 paths → 若 `preferences.deviceName` 为空 → toast「请先完成 SwarmDrop 设置」并丢弃；否则 `scanSources(paths)` 灌入非持久 `share-store` → navigate `/send/share-target`。离屏清空 share-store。

### D7. 多文件/多实例合并
同一次「打开多个文件」或 OS 为每个文件各拉一个实例时，Rust 侧用约 200ms 去抖窗口把连续到达的路径合并进同一个 pending 批次，再 emit 一次，避免前端连开多屏。

## Risks / Trade-offs

- **Windows 注册表写入的位置与时机** → 运行时写 `HKCU`（免管理员权限）在首次启动 self-register；生产环境同时用安装器（NSIS/WiX）hook 保证卸载时清理。二者取其一或并存作为 task 决策。
- **`fileAssociations` 通配符跨平台行为不一** → 不依赖单一 `ext:["*"]`；按 D4 分平台落地并各自验证「右键任意文件/文件夹能看到 SwarmDrop、且默认程序不变」。
- **冷启动竞态导致漏处理** → D5 的 pending 缓冲 + mount 拉取双保险；取走即清空保证不重复。
- **「打开方式」列表污染 / 杀软误报** → `role=None`+`rank=Alternate` 只做候选、不抢默认，降低侵入观感。
- **超大文件夹 scan 慢** → 沿用现有 `scan_sources`（walkdir）行为与既有 UI 反馈，不在本期额外优化。
- **选中设备中途掉线** → share-target 订阅设备列表，掉线自动取消选中并禁用发送（移动端已有模式）。

## Migration Plan

- 纯新增，无破坏性变更、无数据迁移。
- 回滚：移除 `fileAssociations` 配置 + `RunEvent::Opened` 分支 + Windows 注册表项（安装器/自注册负责清理）+ 前端路由/handler/store。
- 灰度：dev 阶段可先只接 macOS `RunEvent::Opened` + 前端屏（假 share-store 注入即可 craft/预览），Windows 注册表 verb 作为后续 task。

## Open Questions

- 去抖窗口取值：默认 ~200ms（假设，实现时按实测微调）。
- Windows self-register 走运行时 `HKCU` 还是安装器 hook：默认运行时首启注册（dev 友好），生产再补安装器清理；留作 task 内决策，不阻塞设计。
