## Context

后端面向用户的字符串散落三桶，消费者完全不同：

| 桶 | 位置 | 消费者 | 现状 |
|---|---|---|---|
| ① 错误 `message` | `crates/core/src/error.rs`、`src-tauri/src/error.rs` | 前端 UI（跨 IPC `{ kind, message }`） | 大部分英文，夹两句中文（`ExpiredCode` / `InvalidCode`） |
| ② 托盘菜单 | `src-tauri/src/tray.rs` | OS 菜单栏（Rust 直接 `.set_text()`，前端看不到） | 硬编码中文 |
| ③ 系统通知 | `crates/core/src/network/event_loop.rs` | OS 通知中心（core 拼 title/body，desktop notifier 直发） | 硬编码中文，P2P 事件触发的纯后端链路 |

决定性事实：**locale 目前只活在前端**（`preferences-store` → tauri-plugin-store），后端完全不知道当前语言。实际 locale 集是 **3 个**（`zh` / `zh-TW` / `en`，见 `lingui.config.ts` 与 `src/lib/i18n.ts`），不是 CLAUDE.md 里写的 8 个。

## Goals / Non-Goals

**Goals**
- 用户切语言后，错误提示、托盘、系统通知全部跟随当前 locale。
- 后端（尤其 core）不承担语言职责——单一职责、平台中立，SwarmDrop-RN 可复用。

**Non-Goals**
- 不新增 locale。
- 不本地化 tracing 日志、开发者技术细节（自由文本内嵌串仍作详情）。
- 不做复数 / 性别语法（当前无计数文案），不做 macOS Services。

## Decisions

### 决策 1：统一原则——后端发码，边缘翻译

三桶不用同一种解法，但共享一条原则：

> **后端 / core 只发「稳定语义码 + 结构化参数」，永不产出本地化散文。翻译发生在呈现边缘——前端 Lingui 管 App 内文案，host 侧 Rust 目录管原生 OS 表面。**

顺此，① 走前端 Lingui（Track 1）；②③ 走 host 侧 Rust 目录（Track 2），且 ③ 的语义码由 core 发、host 译。错误 `kind` 与通知语义枚举**同构**。

### 决策 2：Track 1 复用既有 `errors.ts` 接缝

前端已有 `src/lib/errors.ts`：`isAppError` / `isErrorKind` / `getErrorMessage`。当前 `getErrorMessage` 直接 `return err.message`（把后端英文/中文混杂原样丢给用户）。改法：

- `getErrorMessage(err)` → 先按 `err.kind` 查一张 `kind → Lingui msg` 映射；命中则返回当前 locale 文案，未命中回退 `err.message`（再回退 `String(err)`）。
- 映射表把 `kind` 分两档：**有用户语义的**（`NodeNotStarted` / `ExpiredCode` / `InvalidCode` / `Network` / `Transfer` / `Identity`）各给专门文案；**内部/技术类**（`Io` / `Serialization` / `Database` / `TaskJoin` / `P2p` / `Tauri`）统一归"出错了，请重试"，技术细节留在 `message` 供日志/详情。
- 后端侧顺手把 `ExpiredCode` / `InvalidCode` 的 `#[error("配对码已过期")]` 中文散文改成语言无关技术描述（如 `"pairing code expired"`）——它们本就不该自带某语言散文，`kind` 已足够表达语义。

好处：几乎零后端改动，App 内用户读到的一切走同一个翻译源（Lingui）。

### 决策 3：Track 2 选 `rust-i18n`

原生字符串总计 ~20 条 × 3 locale：

| 方案 | 评价 |
|---|---|
| **`rust-i18n`** ✅ | `t!("tray.open")`，TOML 目录编译期内嵌，`set_locale()` / `locale()` 全局，DX 最好，恰好够用 |
| `fluent` | 复数/性别很强但过重；无复数需求，杀鸡用牛刀 |
| 手写 `match locale` | 3×20=60 臂，重复 locale 列表，随文案增长劣化 |
| 复用 Lingui `.po` | 想法诱人（与前端同目录），但 `gettext-rs` 带 C 依赖、键集也不同，耦合不值 |

目录放桌面壳（`src-tauri/locales/` 或 `src-tauri/src/i18n/`），只含托盘 + 通知两组键。**只译 3 个 locale**，与前端对齐；缺项回退源 locale `zh`（`rust-i18n` 默认行为）。

### 决策 4：locale 交付——启动读 store + `set_locale` 命令

前端仍是唯一权威源。两个时机：

```
启动: 桌面壳读 tauri-plugin-store 持久化的 preferences.locale → rust_i18n::set_locale()
       → 再 build_tray（首帧即正确语言，不闪）
切换: 前端 setLocale() 里 invoke("set_locale", { locale })
       → rust_i18n::set_locale(locale) → refresh_tray()（重绘全部标签）
       → 后续通知自动用新 locale
```

- **首帧来源选"读 store"而非 `sys-locale`**：与用户显式选择完全一致，首帧不闪。代价仅是 Rust 侧耦合 store 的键路径（`preferences` store 的 `locale` 字段）——用 tauri-plugin-store 的 Rust API `app.store(...)` 读，失败则回退 `defaultLocale = zh`。
- `set_locale` 是新增 Tauri 命令（`commands/` 薄壳），经 tauri-specta 进 `bindings.ts`。

### 决策 5：托盘存全部句柄 + 即时重绘

现状 `TrayState` 只存了 `status_item` + `pause_item` + `tray_icon`；open / open-folder / settings / quit 是局部变量，换不了词。改：

- `TrayState` 新增 `open_item` / `open_folder_item` / `settings_item` / `quit_item` 四个 `MenuItem<Wry>` 句柄。
- `refresh_tray`（或新增 `relocalize_tray`）在 `set_locale` 时对全部项 `set_text(t!(...))`；`TrayStatus::text()` / `pause_label()` 从返回 `&'static str` 改为按当前 locale 取 `t!()`（返回 `String`）。
- 菜单**结构**（项的存在、顺序、行为）不变——那部分仍归 `system-tray` spec，本 change 只把"文案固定中文"改成"文案随 locale"。

### 决策 6：core 语义通知枚举（+ 跨仓协调）

现状 core 里 `NotificationRequest { title, body }` 是拼好的中文散文，`Notifier::notify(NotificationRequest)` 是 core↔host 合同。改成语义枚举：

```
// crates/core：只有码 + 结构化字段，零散文
enum Notification {
    PairingRequest { hostname: String },
    IncomingTransfer { device_name: String },
    // …按现有通知点枚举
}
trait Notifier { async fn notify(&self, n: Notification) -> AppResult<()>; /* + notify_if_unfocused 默认方法同改 */ }

// event_loop.rs：构造 Notification::PairingRequest { hostname } 而非 format!
// src-tauri desktop notifier：match n { PairingRequest{hostname} => t!("notif.pairing.title") / t!("notif.pairing.body", hostname = hostname) } → .title().body().show()
```

与错误 `kind` 同构，core 彻底不碰语言。

✅ **跨仓影响（实现阶段已证伪原假设）**：本决策起初担心 `Notifier` 签名改动会破 SwarmDrop-RN，**该担心不成立**。核实：RN `mobile-core/src/events.rs:314` 对 `run_event_loop` 传 `None`（注释「移动端无窗口聚焦概念，不需要 Notifier」），**RN 根本不实现 `Notifier` trait、也不引用 `NotificationRequest`**（后者无 uniffi 导出，只有 desktop-only 的 specta derive）。而本次改动只动 trait 的**方法签名**，trait 名 `Notifier` 与 `run_event_loop(..., Option<Arc<dyn Notifier>>)` 签名都没变——RN 的 `None` 调用点不受影响。且 RN `Cargo.toml` 当前 pin 在 `swarmdrop-core` 的 git rev（非本地 path），本地改动对 RN 零即时影响；将来 bump rev 重编时也照样通过。故本 change **纯桌面仓内闭环，无需与 RN 配对提交**。

（历史退路，现已不需要）务实降级选项：把翻译目录放桌面壳、core 暂发中文 `NotificationRequest`。既然核实无跨仓破坏，直接走干净的语义枚举方案，符合既定架构口味。

## Risks / Open Questions

- ~~**跨仓编译窗口**~~：原以为 `Notifier` 合同变更需 SwarmDrop-RN 配对更新——**实现阶段已证伪**（见决策 6）：RN 传 `None`、不实现 `Notifier`、不引用 `NotificationRequest`，且 pin 在 git rev，故无跨仓破坏、无需配对提交。这不再是协调点。
- **store 键路径耦合**：决策 4 读 tauri-plugin-store 需知道 `preferences` store 文件名与 `locale` 字段名；这层耦合已被 `try + 回退 zh` 兜底，且前端 mount 后 `set_locale` 会立刻校正。
- **`rust-i18n` 全局 locale**：进程级全局态；本应用单窗口、无多语言并发需求，可接受。
- **Track 1 / Track 2 独立性**：两轨几乎不耦合，可分阶段 apply（先前端错误、后 Rust 原生），互不阻塞。
