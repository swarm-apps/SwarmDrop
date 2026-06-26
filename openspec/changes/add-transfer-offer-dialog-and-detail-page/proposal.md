## Why

当前收到文件传输请求时，系统直接跳转到 `/receive` 页面，这会打断用户当前的操作流程。同时，传输过程中缺乏一个专门的详情页面来查看实时进度和文件树。本变更将引入弹窗确认机制和传输详情页面，提升用户体验。

## What Changes

- **新增 `TransferOfferDialog` 组件**：收到传输请求时显示弹窗（不跳转页面），包含文件树预览、保存路径选择和接收/拒绝按钮
- **修改 `_app.tsx`**：移除自动跳转到 `/receive` 的逻辑，改为显示弹窗
- **创建传输详情页面** (`/transfer/:sessionId`)：显示传输进度、文件树、统计信息（速度、剩余时间、已传输大小）
- **修改 `TransferItem` 组件**：点击后跳转到传输详情页面
- **可选保留 `/receive` 页面**：作为备用或完全废弃

## Capabilities

### New Capabilities
- `transfer-offer-dialog`: 传输请求弹窗，显示文件预览和确认操作
- `transfer-detail-page`: 传输详情页面，展示进度和文件树

### Modified Capabilities
- `file-transfer`: 修改接收流程，从跳转页面改为弹窗确认后跳转详情页

## Impact

- **新增文件**：
  - `src/components/transfer/transfer-offer-dialog.tsx`
  - `src/routes/_app/transfer/$sessionId.lazy.tsx`
- **修改文件**：
  - `src/routes/_app.tsx`
  - `src/routes/_app/transfer/-transfer-item.tsx`
  - `src/routes/_app/receive/index.lazy.tsx` (可选废弃)
- **路由变更**：新增 `/transfer/:sessionId` 路由
