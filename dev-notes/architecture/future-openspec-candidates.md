# 后续 OpenSpec 候选变更

本文档对应 `extract-core-and-add-rn-mobile` 的任务 12.6，列出 core/RN 抽离落地后下一阶段可以独立推进的 OpenSpec 变更建议。每条只描述目标和边界，不预设实现细节，留待 `/opsx:new` 时再展开 proposal / design。

## 1. mobile-background-transfer

**目标：** 让 RN 移动端支持后台运行时继续接收和（必要时）发送文件。

**触发：** 当前 RN MVP 只承诺前台传输（design.md Decision 7）。日常使用一定会切到后台。

**关键问题：**

- iOS：`UIBackgroundTaskIdentifier`、`URLSession background`、推送唤醒边界。
- Android：`ForegroundService` + 持久通知、Doze / App Standby 例外。
- core 端是否需要新增 `BackgroundLifecycle` host trait 让 host 控制节点暂停/恢复。

**范围排除：** 长时间（小时级）后台发送在第一版仍可不承诺。

## 2. qr-code-pairing

**目标：** 用二维码替代纯 6 位数字配对码，移动端扫码即可发起配对。

**触发：** 数字码体验在 RN/桌面互配场景比较繁琐，二维码可同时携带 peerId、bootstrap 地址、share code。

**关键问题：**

- core 是否扩展 `PairingCode` 让它能编码 listen addr，避免完全依赖 DHT 查询。
- 桌面侧二维码生成（参考 `qrcode` crate）和 RN 侧 `expo-camera` 扫码。
- 与现有数字码流程共存（不替换）。

## 3. mobile-app-store-release

**目标：** 把 swarmdrop-mobile 推送到 Google Play / Huawei AppGallery / App Store 的发布通道。

**触发：** core MVP 完成后，应用商店发布是覆盖普通用户的必要步骤。

**关键问题：**

- Expo EAS Build / Submit 流程，以及对应签名、隐私清单。
- `app.json` bundle ID 切换策略（dev vs prod）。
- 与桌面 GitHub Release / UpgradeLink 并行的发版节奏。

## 4. mobile-share-extension

**目标：** 让用户从系统分享面板直接发送文件到 SwarmDrop。

**触发：** iOS Share Extension / Android `ACTION_SEND` 是移动端的"原生入口"。

**关键问题：**

- Expo Share Intent 插件或自定义 Native Module。
- 分享进来的文件 URI 与现有 `FileAccess` source 的衔接。
- 后台传输能力的依赖（与候选项 1 相关）。

## 5. mcp-cross-host

**目标：** 让 MCP server 不再绑死在桌面端，能在 RN 端或独立 CLI 上以 sidecar 形式运行。

**触发：** 当前 MCP 留在 `src-tauri`（design.md Non-Goals）。若 MCP 客户端要查询移动端传输状态，会需要 host-agnostic 入口。

**关键问题：**

- core 是否暴露稳定查询 API（已有，主要是事件订阅模型）。
- RN 端能否承载 MCP 协议（多半要走桌面/CLI）。
- 与桌面端 MCP server 的协议兼容。

## 6. ios-build-environment

**目标：** 把 iOS 构建从"本地 macOS 才能跑"升级到可在 CI / 远程构建（EAS）执行，并补齐验证（任务 11.9 留白）。

**关键问题：**

- CocoaPods、Swift PM、UniFFI Swift 绑定生成的稳定性。
- macOS-only CI runner 成本，是否切到 EAS Build。

## 7. transfer-history-on-mobile

**目标：** 移动端集成 SeaORM 传输历史，与桌面端保持一致体验。

**触发：** 当前 RN MVP 没有 list/detail 历史 UI；core 和 entity 都已就绪。

**关键问题：**

- 数据库文件在移动 sandbox 中的位置（`AppPaths`）。
- 列表/详情 UI 与桌面端的样式差异。
- 历史断点续传的恢复入口。
