## Why

点窗口的 ✕ 直接退出整个 app —— 对一个「配对 + 被动后台接收」的 P2P 应用是反模式：进程一退，设备就离线、不可被发现、收不到任何文件。用户想「关掉窗口但继续在后台收文件」却无路可走，也没有一个常驻入口去查看在线状态 / 唤回窗口 / 可靠退出。本变更加入系统托盘 + 「关闭语义」选择（首次询问、设置可改），让「关窗 ≠ 杀进程」，把「后台仍在线」这件事变得可见、可控。

## What Changes

- 新增**系统托盘**（macOS / Windows / Linux 常驻）：6 项菜单（状态行 / 打开 SwarmDrop / 暂停接收 / 打开接收文件夹 / 设置 / 退出）、三态图标（在线 / 暂停 / 离线）、左键单击开窗、右键弹菜单。托盘由 Rust 在 `setup` 创建（脱离任一窗口生命周期长存）。
- 新增**窗口关闭语义** `closeBehavior`（`ask` / `tray` / `quit`，默认 `ask`）：前端唯一拦截 ✕（`onCloseRequested`），首次弹二选一对话框（推荐「最小化到托盘」+「记住我的选择」复选框），设置页可随时改。macOS `Cmd+Q` / 应用菜单退出绕过缩盘、真正退出（✕ 与 Cmd+Q 语义分离）。首次缩盘后补一条系统通知，告知 app 仍在后台。
- 新增**全局「暂停接收」开关**（共享 core）：运行时态，暂停期间保持节点在线、可被发现、配对不受影响，但对新到的传输 offer 自动以「对方已暂停接收」理由婉拒；恢复后照常接收。托盘与设置/UI 都能切换并反映其状态。

## Capabilities

### New Capabilities
- `system-tray`: 桌面系统托盘——常驻托盘图标、6 项菜单、三态状态展现、左右键交互、单实例与各平台兼容。
- `window-close-behavior`: 窗口关闭语义——`closeBehavior` 偏好、前端 `onCloseRequested` 拦截、首次询问对话框、缩盘通知、`Cmd+Q` 真退出。
- `pause-receiving`: 共享 core 的全局暂停接收能力——运行时开关、暂停期间在线但婉拒新 offer、命令暴露。

### Modified Capabilities
（无。`pause-receiving` 仅在现有传输 offer 处理路径 `crates/core/src/transfer/incoming.rs` 注入一个前置检查，不改既有 offer 协议或已声明需求。）

## Impact

- **共享 core**（`crates/core`）：在 `TransferRequest::Offer` 处理入口（`incoming.rs:168`）新增「暂停态婉拒」前置分支，与既有 `NotPaired` / `PolicyRejected` 婉拒范式同构；`OfferRejectReason` 新增 `ReceivingPaused`；`IncomingTransferRuntime` trait 新增 `is_receiving_paused()` 查询（默认实现返回 `false`，平台无关），由 `TransferManager` 用 `AtomicBool` override（flag 落在 trait 实现者 `TransferManager` 而非 `NetManager`）。**移动端 `SwarmDrop-RN` 通过 git 依赖共享同一份 core，默认 `false` 即行为不变，不强制 RN 实现暂停 UI**。
- **桌面壳**（`src-tauri`）：Cargo `tauri` 开 `tray-icon` feature；`setup.rs` 建托盘并留存句柄；新增命令 `pause_receiving` / `resume_receiving` / `is_receiving_paused`（经托管 `NetManager<TransferManager>::transfer().set_receiving_paused(..)`）与 `quit_app`；新增 `tauri-plugin-single-instance`（常驻后台必需，二次启动唤出已有窗口）。`capabilities/default.json` 放行 `core:window:allow-hide` / `allow-show`（`allow-set-focus` 已有）。
- **前端**：`preferences-store` 加 `closeBehavior`；挂一次性 `onCloseRequested`；首次询问对话框（复用 `components/ui/alert-dialog.tsx`）；设置页加「关闭主窗口时」一项；托盘里「打开接收文件夹 / 设置」靠 Rust emit 事件、由（缩盘后仍存活的）webview 执行。
- **资源 / 打包**：三平台托盘图标（macOS template 三态单色 PNG / Windows 多尺寸 `.ico` / Linux PNG）；`tauri.conf.json` Linux `bundle.linux.deb.depends` 加 `libayatana-appindicator3-dev`。
- **i18n**：zh 源 + en + zh-TW 三处补串（对话框 / 菜单 / 设置 / 通知文案，按平台分化「菜单栏」与「托盘」用词）。
- **明确不在本期范围（Non-goals）**：托盘内传输进度行 / 「最近传输」子菜单 / MCP 服务开关行 / 开机自启（均留 v2）；纯菜单栏（Accessory）模式（macOS 仍保留 Dock 图标）；暂停态持久化（运行时态，重启回到「接收中」，避免意外长期静默）；移动端托盘（平台无关 core 钩子默认关闭，RN 不受影响）。
