## 1. 共享 core：暂停接收钩子

- [x] 1.1 `OfferRejectReason` 新增 `ReceivingPaused` 变体（与 `NotPaired` / `PolicyRejected` 同处定义）
- [x] 1.2 `IncomingTransferRuntime` trait 新增 `fn is_receiving_paused(&self) -> bool { false }`（默认实现返回 false，保证未实现平台行为不变）；`TransferManager` 加 `receiving_paused: AtomicBool` 字段 + `set_receiving_paused(bool)` + override `is_receiving_paused`（flag 落在 trait 实现者 core `TransferManager`，非 src-tauri `NetManager`）
- [x] 1.3 `crates/core/src/transfer/incoming.rs` 的 `TransferRequest::Offer` 分支：在 `paired_device.is_none()` 校验之后、`evaluate_receive_policy` 之前，插入「若 `runtime.is_receiving_paused()` 则回 `OfferResult{accepted:false, key:None, reason: Some(ReceivingPaused)}` 并 `return Handled`」——不缓存、不落盘、不发 `TransferOfferReceived`

## 2. 共享 core：单元测试

- [x] 2.1 暂停时 offer 被婉拒：`e2e_paused_offer_declined_then_resumes_on_resume`（双真实节点 harness）——B `set_receiving_paused(true)`，A `send_offer` → A 收 `TransferRejected{reason: ReceivingPaused}`、B 不收 offer 事件、B 无 projection（未缓存/未落盘）
- [x] 2.2 恢复后正常：同一测试后半段 `set_receiving_paused(false)` → 新 offer 照常到达 B（`received_offer` 为真）
- [x] 2.3 默认实现不破坏既有：`TransferManager` 未暂停时所有现有 e2e（单文件/多文件/恢复/拒绝/取消等）全绿即证

## 3. 桌面壳：依赖与暂停态

- [x] 3.1 `src-tauri/Cargo.toml`：`tauri` 开 `tray-icon` + `image-png` feature；新增 `tauri-plugin-single-instance`
- [x] 3.2 ~~`NetManager` 持暂停态~~ → 已在 1.2 落在 core `TransferManager`；桌面壳经托管 `NetManager<TransferManager>::transfer()` 够到 `set_receiving_paused`/`is_receiving_paused`
- [x] 3.3 新增命令 `set_receiving_paused(bool)` / `is_receiving_paused`（transfer.rs，经 `tray::apply_receiving_paused`/`current_receiving_paused`；审查 R4 合并原 pause/resume 两命令为一个 bool 命令），暂停态变更后 emit 类型化事件 `ReceivingPausedChanged(bool)` 供 UI 与托盘同步
- [x] 3.4 新增命令 `quit_app`（lifecycle.rs，`app.exit(0)`，供对话框「退出」与 `closeBehavior=quit` 路径调用）
- [x] 3.5 以上命令注册进 `setup.rs` 的 `collect_commands!`；`ReceivingPausedChanged` 注册进 `collect_events!`

## 4. 桌面壳：系统托盘

- [x] 4.1 `setup.rs` 注册 `tauri-plugin-single-instance`（最先注册；回调里 `tray::show_main_window` 唤出既有窗口）
- [x] 4.2 在 `setup` 用 `TrayIconBuilder` 建托盘（`tray.rs::build_tray`）：6 项菜单（状态行禁用项 / 打开 / 暂停⇄恢复 / 打开文件夹 / 设置 / 退出）+ 分隔线；`MenuItem` 可变句柄存入 `TrayState`（`app.manage`）长存
- [x] 4.3 菜单事件：`打开`→show+focus；`退出`→`app.exit(0)`；`暂停/恢复`→spawn 异步翻暂停态并 `set_text` 切文案 + 发事件；`打开文件夹` / `设置`→唤窗 + emit `tray://*` 事件给 webview（依赖前端状态，决策 2）
- [x] 4.4 左右键按平台分化：**macOS** `show_menu_on_left_click(true)`（左键即弹菜单，原生惯例，与 Tailscale 一致）；**Windows/Linux** `show_menu_on_left_click(false)` + `on_tray_icon_event` 左键 `Up`→show+focus。右键全平台弹菜单
- [~] 4.5 状态首行文字（在线/暂停/离线三态）+ 暂停项文案 + tooltip：由 `start`/`shutdown`/暂停切换驱动 `refresh_tray` 更新；macOS `icon_as_template(true)`。**三态独立图标资源（不同形状/角标）待 7.1 补**——当前三态共用应用图标，状态靠文字层表达（已满足 spec 的文字层与无障碍要求）

## 5. 前端：关闭语义

- [x] 5.1 `preferences-store` 加 `closeBehavior: "ask" | "tray" | "quit"`（默认 `ask`，纳入 `partialize` 与类型）+ `setCloseBehavior` + `hasShownTrayHint` + `setHasShownTrayHint`
- [x] 5.2 挂一次性 `getCurrentWindow().onCloseRequested`（`CloseBehaviorManager`，挂 `__root.tsx` 全局）：始终 preventDefault，按 `closeBehavior` → `hide()` / `quit_app` / 弹对话框
- [x] 5.3 首次询问对话框（复用 `alert-dialog.tsx`）：二选一（最小化到托盘[默认] / 退出 SwarmDrop）+「记住我的选择」复选框；按选择执行并按需写 `closeBehavior`；文案按平台分化（`{trayWord}` = 菜单栏 / 托盘）
- [x] 5.4 首次缩盘系统通知（`@tauri-apps/plugin-notification`）：仅首次（`hasShownTrayHint` 守一次），文案告知「后台运行 + 退出在托盘」
- [x] 5.5 设置页加「通用 → 关闭主窗口时」（Select：每次询问 / 最小化到托盘 / 退出应用），读写 `closeBehavior`

## 6. 前端：托盘事件消费

- [x] 6.1 监听 `tray://open-receive-folder` → `openPath(transfer.savePath)`（`CloseBehaviorManager` 内）
- [x] 6.2 监听 `tray://open-settings` → `navigate({ to: "/settings" })`（托盘侧已先唤窗）
- [~] 6.3 （可选，本期不做）窗口内暂停/恢复开关：暂停态已可由托盘切换、设置页有关闭行为；窗口内 UI 开关留待后续，不阻塞本期

## 7. 配置 / 资源 / i18n

- [ ] 7.1 三态托盘图标资源（**需设计资产，本期未做**）：macOS template 单色三态 PNG（在线/暂停/离线）、Windows 多尺寸 `.ico`、Linux PNG，放入 `src-tauri/icons/` 后在 `tray.rs` 接 `set_icon`。当前三态共用应用图标，状态靠文字层表达
- [x] 7.2 `tauri.conf.json`：Linux `bundle.linux.deb.depends` 加 `libayatana-appindicator3-1`（运行时托盘依赖）；`tauri` 已开 `image-png`
- [x] 7.3 `capabilities/default.json` 放行 `core:window:allow-hide`（前端唯一从 JS 调的窗口方法；审查 R1 删去多授权的 `allow-show`/`allow-unminimize`——show/unminimize 仅 Rust 侧 `tray::show_main_window` 直调、不走 IPC）
- [x] 7.4 `pnpm i18n:extract` 后补 en / zh-TW 翻译（14 条新串全部填好；既有积压 165 条不在本期范围）

## 8. 验证

- [x] 8.1 `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings`（零警告）+ core 暂停接收 e2e 通过 + `cargo check -p swarmdrop`；`pnpm exec tsc --noEmit` 通过；`pnpm i18n:extract` 无解析错误
- [x] 8.2 运行实测（`pnpm tauri dev`）：✕ 在 `ask` **弹出关闭确认对话框（用户手测确认）**；设置页「通用 → 关闭主窗口时」Select 正常渲染（MCP DOM 实测）
- [~] 8.3 托盘：`build_tray` 在 setup 成功（app 正常启动无 crash 即证托盘已建）；single-instance 已注册。**左键开窗 / 右键菜单 / 各菜单项 等原生交互建议手点确认**（MCP 不易驱动原生托盘菜单）
- [x] 8.4 macOS `Cmd+Q` 实测：**真正退出（用户手测确认）**。**Open Question 已解**——Cmd+Q 走应用级退出、不经 `onCloseRequested`，无需 `is_quitting` 兜底
- [x] 8.5 暂停接收实测（MCP webview invoke）：`pause_receiving`→`is_receiving_paused()=true` 且广播 `receiving-paused-changed=true`；`resume_receiving`→`false`/事件 `false`。core 侧「暂停时 offer 婉拒、恢复后正常」由 e2e 2.1/2.2 坐实
- [ ] 8.6 跨仓确认：`SwarmDrop-RN` 拉取共享 core 后编译/启动不回归（`is_receiving_paused` 默认 false，offer 处理行为不变）——跨仓手测（待在 RN 仓验证）
