## Why

后端面向用户的字符串目前"单一语言、且不一致"：错误 `message` 大多是英文（`"Node not started"`），却夹着两句硬编码中文（`"配对码已过期"`、`"无效的配对码"`）；系统托盘菜单（`"打开 SwarmDrop"`、`"暂停接收"`、`"退出 SwarmDrop"`…）与系统通知（`"配对请求"`、`"收到文件传输请求"`…）全是硬编码中文。前端已用 Lingui 支持 zh / zh-TW / en 三语，但后端字符串**绕过了它**——用户切到 English 后，托盘、通知、以及大量报错仍是中文/英文混杂。

根因是"翻译发生在错误的层"：后端在自己那层拼了本地化散文，而它既不知道当前语言、又把责任揽错了地方。

## What Changes

统一到一条原则：**后端只发「稳定语义码 + 结构化参数」，翻译发生在呈现边缘。** 分两条几乎独立的轨：

- **Track 1 · 错误 → 前端 Lingui**：后端错误已带稳定 `kind`（`src/lib/errors.ts` 已有 `isErrorKind` / `getErrorMessage` 接缝）。把 `getErrorMessage` 从"直接返回后端 `message`"改成"按 `kind` 查 Lingui 目录"；后端 `message` 降级为开发者/日志用技术细节；移除越界的中文错误变体（`ExpiredCode` / `InvalidCode` 只靠 `kind`，不再自带中文散文）。**后端错误结构零改动**（`kind` 合同已在），主要是前端接缝与文案。

- **Track 2 · 托盘 + 通知 → Rust 侧 i18n**：这两处由 Rust/OS 直接渲染，前端够不着，必须 Rust 译。引入 `rust-i18n` 目录（3 locale × ~20 条）；前端为语言权威源，Rust 启动读持久化 locale、切换时经新 `set_locale` 命令收到更新；托盘全部菜单项句柄纳入 `TrayState`，切语言即时重绘；**core 保持语言中立**——`NotificationRequest{title,body}` 改为语义枚举 `Notification::PairingRequest{hostname}`，由 desktop host `match + t!()` 翻译（与错误 `kind` 同构，SwarmDrop-RN 可复用）。

- **非目标**（明确排除）：不新增 locale（维持 zh / zh-TW / en，与 `lingui.config.ts` 对齐）；不本地化 tracing 日志与自由文本技术细节（`Network(String)` / `Transfer(String)` 等内嵌串仍作技术详情，只本地化 `kind` 级标题）；不做 macOS Services / 复数语法（当前无计数文案）。

## Capabilities

### New Capabilities

- `error-localization`: 后端错误以稳定 `kind` + 结构化信息暴露，用户可读文案由前端 Lingui 按 `kind` 生成；后端不再返回预翻译的用户文案。
- `native-string-localization`: 托盘与系统通知等原生 OS 表面由 Rust 侧目录按当前 locale 本地化；前端为 locale 权威源并在启动 / 切换时同步给后端；core 通过语义通知类型保持语言中立，交由 host 翻译。

### Modified Capabilities

- `system-tray`: 托盘菜单项与状态首行文案改为按当前 locale 本地化，并在语言切换时即时重绘（不再固定中文字面量）。

## Impact

- **后端 core**：`crates/core/src/host.rs`（`NotificationRequest` 结构替换为语义 `Notification` 枚举 + `Notifier` trait 签名改收它）、`crates/core/src/network/event_loop.rs`（构造语义枚举而非 `format!` 拼串）。✅ **无跨仓破坏**（实现阶段核实）：SwarmDrop-RN 对 `run_event_loop` 传 `None`、不实现 `Notifier`、不引用 `NotificationRequest`，且 pin 在 git rev；trait 名与 `run_event_loop` 签名未变——RN 不受影响，无需配对提交（见 design「决策 6」）。
- **桌面壳**：`src-tauri/Cargo.toml`（+`rust-i18n`）、新增 locale 目录（TOML）、`src/host/notifier.rs`（`match` + `t!()`）、`src/tray.rs`（`TrayState` 存全部菜单句柄 + `refresh_tray` 重设全标签 + locale 感知）、新增 `set_locale` 命令（`commands/` 薄壳）+ 启动读 tauri-plugin-store 的持久化 locale。
- **前端**：`src/lib/errors.ts`（`getErrorMessage` 改按 `kind` 查 Lingui）、新增 `kind → msg` 映射、`src/stores/preferences-store.ts`（`setLocale` 里 `invoke("set_locale")`）、新增 Lingui 文案、`src/lib/bindings.ts` 由 tauri-specta 重新生成含 `set_locale`。
- **i18n**：源 locale `zh`；实现后 `pnpm i18n:extract`。
- **零改动确认**：后端错误结构（`kind` / `AppErrorPayload`）、传输链、`FileSource` 均不改；仅通知构造点与 `Notifier` 合同变。
