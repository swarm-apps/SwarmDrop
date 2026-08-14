## Why

移动端已经能在别的 App 里「分享 → 用 SwarmDrop 打开」文件后直接发送；桌面端目前只能先打开 app、进设备页、再拖文件，路径长且断裂。桌面用户（以及 AI agent 产文件后想转投的开发者）期望同样的「在文件管理器里右键『打开方式 → SwarmDrop』就能把这个文件推到另一台设备」的最短路径。这是移动端 share-target 在桌面上的对等能力，缺口是三平台的「文件关联入口」和一个「文件已定、只需选设备」的发送屏。

## What Changes

- 把 SwarmDrop 注册为**非默认**的「打开方式」处理器，覆盖任意文件与文件夹（`role=None`、`rank=Alternate`：出现在系统「打开方式」列表但绝不抢占默认程序）。
- 捕获操作系统「用本应用打开」送达的文件路径并归一化，三平台各自入口：macOS `RunEvent::Opened{urls}`；Windows/Linux 冷启动 `std::env::args()`、热启动复用 single-instance 回调当前被丢弃的 `_args`。向前端发一个统一的 `external-file-open` 事件。
- 新增独立整页路由 `/send/share-target`：镜像现有 `/send` 双栏但角色对调（左=待发文件汇总，右=在线可发送设备单选，底=发送/准备进度），复用现有 `scan_sources → prepare_send → start_send` 链（**后端传输逻辑零改动**）。
- 首次启动尚未设置设备名时用本应用打开文件 → 丢弃并提示（与移动端 v1 一致，不做缓冲回放）。
- 非目标（明确排除）：不做发送队列/离线暂存后发；不做 macOS Services / Windows Share Contract 原生扩展；移动端不改动。

## Capabilities

### New Capabilities
- `file-association-entry`: 桌面端把自己注册为任意文件/文件夹的非默认「打开方式」处理器，并在三平台捕获、归一化被打开的文件路径，向前端发布统一的 `external-file-open` 事件。
- `share-target-send`: 「文件已定、只需选设备」的反向发送屏——从外部打开进入，复用既有发送链，选一台在线已配对设备发出；含未引导丢弃、节点自动启动、无在线设备空状态等门禁与状态。

### Modified Capabilities
<!-- 无。share-target 完全复用现有发送链（scan/prepare/start）与 send-progress / transfer-offer 的既有需求，不改变它们的 spec 级行为，仅新增调用入口。 -->

## Impact

- **Rust / 桌面壳**：`src-tauri/src/lib.rs`（`run` 改为 `.build()?.run(|handle,event| …)` 接 `RunEvent::Opened`）、`src-tauri/src/setup.rs`（single-instance `_args` 落地 + 冷启动 `env::args()` 解析 + 新事件 `collect_events` 声明）、`src-tauri/tauri.conf.json`（`bundle.fileAssociations`）、`src-tauri/capabilities/default.json`（权限）。
- **平台特例**：Windows 通配符 + 文件夹 Tauri `fileAssociations` 覆盖不全，需手动写注册表 shell verb（`HKCU\Software\Classes\*\shell` 与 `Directory\shell`）。macOS 用 `public.data/public.item/public.folder`（`LSItemContentTypes`），Linux 用 `MimeType`。
- **前端**：新增 `src/routes/_app/send/share-target.lazy.tsx`、非持久 `src/stores/share-store.ts`、根级 `ExternalOpenHandler`（挂在 `_app` 布局，对标 RN `ShareIntentHandler`）；复用 `-use-file-selection.ts`、task-surface 组件、`FileTree`；`src/lib/bindings.ts` 由 tauri-specta 重新生成含新事件。
- **i18n**：新增 Lingui 文案（源 locale `zh`），实现后 `pnpm i18n:extract`。
- **零改动确认**：`scan_sources / prepare_send / start_send`、`crates/core` 传输逻辑、`FileSource` 数据模型均不改。
