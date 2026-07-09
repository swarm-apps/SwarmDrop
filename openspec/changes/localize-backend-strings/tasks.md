## 1. Track 1 · 前端错误本地化（后端错误结构零改）

- [x] 1.1 后端把 `crates/core/src/error.rs` 的 `ExpiredCode` / `InvalidCode` 的 `#[error("配对码已过期")]` / `#[error("无效的配对码")]` 中文散文改为语言无关技术描述（`kind` 不变，前端据 `kind` 渲染）
- [x] 1.2 前端新增 `kind → Lingui msg` 映射：有用户语义档（`NodeNotStarted` / `ExpiredCode` / `InvalidCode` / `Network` / `Transfer` / `Identity`）各给专门 `msg`；内部/技术档（`Io` / `Serialization` / `Database` / `TaskJoin` / `P2p` / `Tauri`）统一「出错了，请重试」
- [x] 1.3 改 `src/lib/errors.ts` 的 `getErrorMessage`：先按 `err.kind` 查映射（命中→当前 locale 文案），未命中回退通用提示（不再把后端英文原文丢给用户）；`isAppError` / `isErrorKind` 不变
- [x] 1.4 审查现有 `getErrorMessage(err)` / `toast.error(...)` 调用点：绝大多数已走 `getErrorMessage`（自动受益）；修 `-mcp-section.tsx` 的 `toast.error(String(e))` 绕过 → `getErrorMessage(e)`。⚠️ **follow-up（本 change 范围外）**：`transfer-notifications.ts` 的 `transferFailed.error` / `transferDbError.message` 是传输生命周期事件 payload（core 直发中文串，如 `"对方取消"`），需另立「传输 reason 语义化」才能本地化，属另一套机制、动 core+RN，未在本轮
- [x] 1.5 `pnpm i18n:extract`（源 590 条）；本轮新增错误文案 en / zh-TW 各补 5 条到位（`节点未启动` / `配对码已过期` 已有译文）

## 2. Track 2a · rust-i18n 目录 + locale 交付基建

- [x] 2.1 `src-tauri/Cargo.toml` 加 `rust-i18n = "4"`；建桌面壳 locale 目录 `src-tauri/locales/{zh,zh-TW,en}.toml`（`tray.*` 组，含状态行/静态项，缺项回退 zh）；`lib.rs` 加 `rust_i18n::i18n!("locales", fallback = "zh")`。notif.* 组延到 Track 2c
- [x] 2.2 新增 `src-tauri/src/i18n.rs`：`init_locale_from_store` 读 `preferences.json` → `"preferences-store"`（zustand 双层编码，取字符串再 parse）→ `state.locale` → `rust_i18n::set_locale`；读失败保持 fallback zh。`setup.rs` 在 `build_tray` **之前**调用
- [x] 2.3 新增 `set_locale` Tauri 命令（`commands/i18n.rs` 薄壳）：`crate::i18n::set_locale` + `crate::tray::relocalize_tray`；注册进 `collect_commands![]` + `commands.rs` mod/re-export
- [x] 2.4 `cargo test -p swarmdrop --test specta_export` 重新生成 `src/lib/bindings.ts`，已确认含 `setLocale`
- [x] 2.5 前端 `preferences-store.ts` 的 `setLocale()` 在 `dynamicActivate` 后 `commands.setLocale(locale)`（try/catch best-effort，失败不影响前端切换）

## 3. Track 2b · 托盘本地化 + 即时重绘

- [x] 3.1 `TrayState` 新增 `open_item` / `open_folder_item` / `settings_item` / `quit_item` 句柄 + `online`/`paused` AtomicBool 状态缓存；`build_tray` 创建后一并存入
- [x] 3.2 `TrayStatus::text()` / `pause_label()` / 各菜单标签改为按当前 locale 取 `t!(...)`（返回 `String`）；新增 `relocalize_tray(app)`：从缓存状态重新派生状态行/暂停项 + 对静态项 `set_text(t!(...))`，`set_locale` 命令调用它
- [ ] 3.3 验证：托盘首帧语言 = 持久化 locale；运行中切语言 → 状态行/打开/暂停·恢复/打开接收文件夹/设置/退出 全部即时重绘（⏳ 需运行应用手测）

## 4. Track 2c · core 语义通知枚举 + host 翻译

- [x] 4.1 `crates/core/src/host.rs`：`NotificationRequest{title,body}` 结构替换为语义枚举 `Notification`（`PairingRequest { hostname }` / `IncomingTransfer { device_name }`）；`Notifier::notify` / `notify_if_unfocused` 签名改收 `Notification`
- [x] 4.2 `crates/core/src/network/event_loop.rs`：2 个通知点（配对/传输）把 `format!` 拼 title/body 改为构造 `Notification::*` 变体（零散文）
- [x] 4.3 `MemoryHost`（core 测试替身）的 `notifications: Vec<Notification>` 字段 + `Notifier` impl 同步改；无其它 `Notifier` 实现、无读 notifications 的测试断言需改。core `cargo test --no-run` 通过（含用 MemoryHost 的 e2e_transfer）
- [x] 4.4 `src-tauri/src/host/notifier.rs`：`DesktopNotifier::notify` 经 `localize()` `match Notification` + `t!("notif.*.title")` / `t!("notif.*.body", hostname = ...)` → `.title().body().show()`；`notif.*` 词条补进三 locale TOML
- [x] 4.5 ✅ **跨仓核实：RN 不受影响、无需改**——RN `events.rs:314` 对 `run_event_loop` 传 `None`（「移动端无窗口聚焦概念，不需要 Notifier」），**根本不实现 `Notifier`、不引用 `NotificationRequest`**；`Notifier` trait 名与 `run_event_loop` 签名（`Option<Arc<dyn Notifier>>`）都没变，`None` 调用不受影响。且 RN 当前 pin 在 git rev（非本地 path），本地改动零即时影响。design 决策 6 的「破 RN」假设**已被证伪**（已回填 design）
- [x] 4.6 `cargo check`（core + 桌面壳）+ core `cargo test --no-run` 通过；clippy `--no-deps` 桌面壳零告警、core 我改的文件（host.rs/event_loop.rs）零告警；fmt 干净

## 5. 收尾验证与知识库

- [x] 5.1 `pnpm build`（tsc + vite）✓；`cargo clippy --no-deps`（桌面壳零告警、core 我改的文件零告警——core 全量的 too_many_arguments 在 receive.rs/incoming.rs 是并行改动/clippy1.95 基线漂移，非本轮）；`cargo fmt` 桌面壳 + core 我改文件干净
- [ ] 5.2 端到端手测：设置里切 zh↔en → ①错误 toast（如"节点未启动"）②托盘全部菜单项 ③系统通知（配对请求/传输请求）三者语言一致跟随（⏳ 需运行应用手测）
- [ ] 5.3 冷启动手测：以持久化为 en 冷启 → 托盘首帧即 English（不闪中文）（⏳ 需运行应用手测）
- [x] 5.4 更新知识库 `dev-notes/knowledge/rust-backend.md`：新增「国际化 (i18n)」段——后端发码/边缘翻译原则、rust-i18n 集成（i18n! 位置/per-locale TOML/%{var} 插值）、locale 交付（tauri-store 双层编码 + build_tray 前时序）、通知语义枚举 + RN 传 None 证伪跨仓破坏
