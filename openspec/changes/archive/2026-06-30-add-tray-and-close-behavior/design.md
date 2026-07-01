## Context

桌面壳 `src-tauri` 现状：`tauri.conf.json` 单窗口 `main`，macOS 用 Overlay 标题栏（`trafficLightPosition` 控制红绿灯），Windows/Linux 在 `setup.rs` 关装饰、由前端 `app-topbar.tsx` 自绘最小化/最大化/关闭三键（关闭键调 `appWindow.close()`，见 `app-topbar.tsx:251`）。**当前没有任何窗口关闭事件处理，也没有托盘**——✕ 走 Tauri 默认行为：窗口销毁 → 无窗口 → 进程退出。

关键事实：macOS 红绿灯 ✕、Windows/Linux 自绘 ✕、`Alt+F4`，**三条路径最终都触发同一个 `CloseRequested` 事件**（`appWindow.close()` 与 WM_CLOSE 均如此）；唯独 macOS `Cmd+Q` / 应用菜单退出是**应用级**退出（`RunEvent::ExitRequested`），不经过单个窗口的 `CloseRequested`。

偏好持久化：前端 `preferences-store`（Zustand → `tauri-plugin-store` → `preferences.json`），是所有用户偏好的唯一落点；接收保存路径 `transfer.savePath` 也在此（单一、稳定的默认收件目录已存在）。

被动接收链路：传输 offer 在共享 core `crates/core/src/transfer/incoming.rs` 的 `TransferRequest::Offer` 分支处理，已有 `OfferRejectReason::{NotPaired, PolicyRejected}` 的「构造 `OfferResult{accepted:false, reason}` 并 return」婉拒范式；`TransferRuntime` trait 由 `src-tauri` 的 `NetManager` 实现。

约束：不污染共享 core 的平台无关性（托盘 / 窗口属桌面壳，绝不进 core）；尽量零新业务 crate（仅 `tauri-plugin-single-instance` 一个官方插件）。

## Goals / Non-Goals

**Goals:**
- 关窗不再等于杀进程：✕ 可「最小化到托盘」继续后台接收，托盘是后台在线的唯一可见锚点与唯一可靠退出口。
- 首次明确把选择权交给用户（询问 + 记住），之后零打扰；设置页可随时改。
- 新增一个轻量「暂停接收」能力，让用户临时停手而无需退出。
- 三平台行为一致、各自尊重平台习惯；对共享 core 的侵入降到最小（一个默认 `false` 的 trait 钩子）。

**Non-Goals:**
- 托盘内传输进度行 / 「最近传输」子菜单 / MCP 开关行 / 开机自启（v2）。
- 纯菜单栏（Accessory）应用模式——macOS 仍是带 Dock 的常规应用。
- 暂停态持久化——运行时态，重启回到「接收中」。
- 移动端托盘 / RN 侧暂停 UI——core 钩子默认关闭，RN 行为不变。

## Decisions

### 决策 1：前端权威拦截 ✕，Rust 只负责建托盘
- **选择**：用前端 `getCurrentWindow().onCloseRequested` 作为 ✕ 的**唯一**拦截点，按 `closeBehavior` 分支（隐藏 / 放行 / 弹询问）；Rust 只在 `setup` 建托盘、并提供托盘菜单动作（打开 / 退出 / 切暂停）。
- **理由**：(a) 三平台 ✕ 都汇成同一个 `CloseRequested`，一处 `onCloseRequested` 即全覆盖；(b) 能点 ✕ 的前提是窗口开着、webview 活着，所以「偏好未 hydrate」不成立；(c) 偏好留在 `preferences-store` 唯一一处，无需 Rust 去解析 Zustand 序列化的 blob；(d) 单一职责清爽：**窗口/进程归 Rust、关闭策略+UI 归前端**。
- **更正一个早先假设**：Tauri v2 其实**有** JS 托盘 API（`@tauri-apps/api/tray`/`menu`）。仍选择用 Rust 建托盘，原因是托盘要脱离任一窗口生命周期长存、icon template 与菜单事件在 Rust 端更稳，而**非**「没有 JS API」。

### 决策 2：托盘菜单动作的「自包含 vs 依赖前端状态」二分
- **选择**：托盘菜单项分两类执行：
  - **Rust 自包含**（不依赖前端状态）：`打开 SwarmDrop`（`show()`+`set_focus()`）、`退出 SwarmDrop`（`app.exit(0)`）、`暂停/恢复接收`（翻 Rust 侧暂停态）——直接在 Rust 菜单事件里做。
  - **依赖前端状态**（需要 `savePath` 或前端路由）：`打开接收文件夹`、`设置…`——Rust 仅 `emit` 一个事件，由**缩盘后仍存活的 webview** 消费执行（前端用 `opener` 打开 `transfer.savePath` / 用路由跳 `/settings`）。
- **理由**：`savePath` 与路由是前端拥有的状态，让 Rust 去读 `preferences.json` blob 既脆又越界；`hide()` 不销毁 webview，事件可达，于是「emit 给 webview 执行」是最干净的跨边界方式。

### 决策 3：托盘状态由 Rust 从事件循环直接派生，前端不碰托盘
- **选择**：托盘的三态（在线 / 暂停 / 离线）、状态首行文字、设备数，由 Rust 侧根据「节点是否启动 + peer 连接事件 + 暂停态」直接派生并更新托盘（`set_icon` / `set_text` / `tooltip` / macOS `set_title`）；前端不直接操作托盘句柄。
- **理由**：托盘生命周期归 Rust，状态源（`NetManager` / network event_loop / 暂停态）也在 Rust，自给自足无需前端往返；前端操作托盘会造成生命周期混乱。
- **约束**：状态展现做**两层冗余**——图标形状 + 菜单首行文字；tooltip 仅辅助（**GNOME 不显示 tooltip**，关键信息不得只靠它）。

### 决策 4：暂停接收 = 「在线但婉拒」，且运行时态不持久化
- **选择**：暂停态是 **`TransferManager`（core）** 内一个 `AtomicBool`（实现 `IncomingTransferRuntime` 的就是 `TransferManager`，flag 放它最贴近、且平台中立、RN 也白拿）；override `IncomingTransferRuntime::is_receiving_paused()`（core trait，默认 `false`）读它，并提供 `set_receiving_paused(bool)`。桌面壳经托管的 `NetManager<TransferManager>::transfer()` 够到它来切换。暂停期间：**不动节点、不 `announce_offline`、配对/发现照常**；只在 `TransferRequest::Offer` 入口前置检查，若暂停则回 `OfferResult{accepted:false, reason: ReceivingPaused}` 婉拒，不缓存、不落盘、不打扰用户。重启回到「接收中」（不持久化）。
- **理由**：贴合「临时停手」心智又保持可发现（区别于「下线」会真断节点）；婉拒（decline-with-reason）比「挂起/排队」简单诚实——无需超时与重投状态机，发送方收到明确理由可重试。
- **备选/不取**：(a) 映射为节点 `shutdown`/`announce_offline`——会真下线、语义过重；(b) 挂起并在恢复后重投 offer——要维护待决队列与超时，复杂度不值 v1；(c) 持久化暂停态——有「忘了自己暂停过、长期静默收不到文件」的脚枪，故不持久化。

### 决策 5：✕ 与 Cmd+Q 语义分离
- **选择**：✕（含 Windows `Alt+F4`，本质都是窗口 `CloseRequested`）→ 走 `closeBehavior`；macOS `Cmd+Q` / 应用菜单退出（应用级 `ExitRequested`）→ 始终真正退出。Windows/Linux 无 `Cmd+Q` 等价物，其可靠真退出口是**托盘「退出」**。
- **理由**：mac 用户强预期 `Cmd+Q`=退出；二者走不同的系统事件，天然可分离，实现也最干净。
- **风险与兜底**：~~需实测确认 macOS `Cmd+Q` 不经过单窗口 `onCloseRequested`~~ **已实测确认**：macOS `Cmd+Q` 走应用级退出、不经 `onCloseRequested`，`closeBehavior=tray` 下仍真正退出，**无需 `is_quitting` 兜底**。

## Risks / Trade-offs

- **macOS `Cmd+Q` 路由不确定**（决策 5）→ 以实测为准；提供 `is_quitting` 标志 + `ExitRequested` 不拦截作兜底。
- **Linux 托盘可见性**：依赖 `libayatana-appindicator3`（`deb.depends` 声明）；**GNOME 默认无托盘**→ 文档提示装 AppIndicator/Status Icons 扩展；GNOME 无 tooltip → 状态靠图标+菜单首行冗余。**主窗口必须始终功能完整，托盘只能是加速入口，不得成为触达任何功能的唯一路径**。
- **托盘句柄被 drop 致图标消失** → 把 `TrayIcon` 与各 `MenuItem` 句柄存入 Tauri state 长存。
- **缩盘后用户找不到 app 还在跑**（尤其 Win11 默认折叠图标）→ 首次缩盘弹一次系统通知告知；设置页永远保留「退出」选项。
- **暂停态婉拒对发送方表现为「被拒」** → `ReceivingPaused` 理由明确，与 `NotPaired`/`PolicyRejected` 区分；发送方可在对方恢复后重试。
- **多开实例致托盘重复 / 状态错乱** → `tauri-plugin-single-instance`，二次启动唤出已有窗口。

## Migration Plan

1. **core**：`OfferRejectReason` 加 `ReceivingPaused`；`IncomingTransferRuntime` trait 加 `fn is_receiving_paused(&self) -> bool { false }`，`TransferManager` 用 `AtomicBool` override 它并加 `set_receiving_paused`；`incoming.rs` 的 `Offer` 分支在 `NotPaired` 检查后、策略评估前插入「若 `runtime.is_receiving_paused()` 则回 `ReceivingPaused` 婉拒并 return」。
2. **桌面壳**：`Cargo.toml` 开 `tauri` 的 `tray-icon`、加 `tauri-plugin-single-instance`；命令 `pause_receiving`/`resume_receiving`/`is_receiving_paused`（经 `NetManager::transfer().set_receiving_paused(..)`）、`quit_app`，注册进 `collect_commands!`；`setup.rs` 建托盘（菜单 + 三态图标 + 左右键 + 句柄留存）、注册 single-instance、从 network 事件派生托盘状态。（core 侧 flag 落在 `TransferManager`，见 1/决策 4。）
3. **前端**：`preferences-store` 加 `closeBehavior`（默认 `ask`，纳入 `partialize`）；挂一次性 `onCloseRequested`；首次询问对话框；设置页「关闭主窗口时」下拉；托盘「打开文件夹/设置」事件消费；首次缩盘通知。
4. **资源/配置**：三平台托盘图标；`tauri.conf.json` Linux `deb.depends`；`capabilities/default.json` 放行 hide/show。
5. **i18n**：`pnpm i18n:extract` 后补 en / zh-TW。
6. **验证**：core 单测（暂停时 offer 婉拒、恢复后正常、默认 `false` 不影响既有）；运行实测（✕ 缩盘 / 首次对话框 / 记住 / 托盘各项 / Cmd+Q 真退出 / 单实例 / 暂停-发送方被婉拒）。
- **回滚**：均为新增；去掉 feature/插件/命令/前端钩子即恢复原行为（✕=退出），core trait 默认 `false` 向后兼容。

## Open Questions

- ~~macOS `Cmd+Q` 是否经过单窗口 `onCloseRequested`？~~ **已解**：不经过，走应用级退出 → 真退出，无需 `is_quitting`（决策 5）。
- `暂停接收` 是否需要在窗口 UI（非仅托盘）也给一个开关？v1 托盘可切 + 设置页有关闭行为；窗口内暂停开关留待后续（tasks 6.3）。
- ~~托盘「设置」是仅 `show()` 还是同时路由到 `/settings`？~~ **已定**：唤窗 + emit `tray://open-settings`，前端 `navigate({to:"/settings"})`（决策 2）。
