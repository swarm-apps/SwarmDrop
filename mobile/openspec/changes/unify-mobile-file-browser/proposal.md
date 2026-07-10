## Why

移动端已经具备 core projection、文件树、系统分享发送、接收 Offer、传输详情和收件箱详情，但这些入口仍分别维护文件行、状态映射和滚动结构。当前实现会按 `relativePath` 去重同名文件，并在大文件集合中回退为全量 `map`，既可能静默漏选文件，也无法稳定满足长列表、终态回看和主要操作始终可达的产品契约。

## What Changes

- 建立移动端统一 `FileBrowser`：由标准 item、adapter 和显式 actions 驱动，同一文件集合可在 tree/grid 视图间切换。
- 将文件身份与展示路径分离：发送来源使用稳定 `sourceId` 派生身份，Offer/projection 使用 `fileId`，`relativePath` 只承担目录结构和路径展示。
- 复用现有 Rust/mobile-core `MobileTransferProjection` 作为传输事实源，不增加 JS 自有 SessionProjection，也不改变共享传输协议。
- 统一发送选择、系统分享发送、接收 Offer、传输详情和收件箱详情的文件展示，移除页面私有的 `SharedFileRow`、`FileRow` 和重复状态映射。
- 让 tree/grid 都使用非嵌套虚拟列表，支持 1、100、1,000、10,000 个文件，并为视图切换、目录展开和滚动重置定义稳定行为。
- 按 `send`、`transfer`、`inbox` scope 持久化视图偏好；偏好只属于展示层。
- 统一任务页布局为固定 header、唯一中间滚动区和固定 footer，并适配 Safe Area、小屏、横屏和大字体。
- 将接收 Offer 改为手机近全高 Bottom Sheet/全屏 modal、平板宽版 Dialog；文件区独立虚拟滚动，接收/拒绝始终可达。
- 保持预览权限最小化：只有 adapter 明确提供的可访问 URI 才能渲染缩略图，Offer 接收前和权限失效时回退文件类型图标。

## Capabilities

### New Capabilities

- `mobile-file-browser`: 统一移动端文件集合模型、稳定身份、adapter、tree/grid 视图、显式操作能力、场景偏好、状态语义、虚拟化与预览权限边界。
- `mobile-transfer-task-layout`: 发送、Offer、传输详情和收件箱详情的固定导航/操作区、唯一主滚动区、响应式容器与 Safe Area 契约。

### Modified Capabilities

无。既有收件箱打开/分享与媒体预览需求保持不变，本 change 只统一承载这些行为的文件浏览和任务布局能力。

## Impact

- `src/components/file-tree/` 将迁移为 `src/components/file-browser/`；保留现有 `@shopify/flash-list`，以纯树构建/拍平逻辑替代当前仅在此处使用且需要强制刷新的 `@headless-tree/*`，不新增列表库。
- `src/app/send/select-device.tsx`、`src/app/send/share-target.tsx`、`src/components/transfer-offer-host.tsx`、`src/app/transfer/[sessionId].tsx`、`src/app/inbox/[itemId].tsx` 改为消费统一 FileBrowser。
- `src/stores/mobile-core-store.ts` 的选择去重/移除逻辑改用稳定身份和路径边界；`src/stores/preferences-store.ts` 增加分 scope 的视图偏好。
- `src/stores/transfer-store.ts` 继续消费 core projection，仅保留实时 progress overlay，不新增或复制持久会话状态。
- 测试继续使用现有 `e2e/webdriver` 的 WebdriverIO + Appium XCUITest 基线，不新增 Jest/Testing Library；需要为文件身份、终态状态、视图切换和大集合建立可驱动的 fixture 与稳定 accessibility id。
- 需要补充 iOS/Android 真实设备上的 Safe Area、横屏、大字体和滚动验证。
