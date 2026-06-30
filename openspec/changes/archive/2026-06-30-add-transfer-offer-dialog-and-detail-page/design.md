## Context

当前文件传输的接收流程：
1. 收到 `transfer-offer` 事件
2. `_app.tsx` 自动跳转到 `/receive` 页面
3. 用户在 `/receive` 页面确认接收
4. 开始传输，跳转到 `/transfer` 列表页

问题：
- 自动跳转打断用户当前操作
- 传输列表页只展示简要信息，无法查看实时进度和文件树详情
- 缺少专门的传输详情页面

## Goals / Non-Goals

**Goals:**
- 接收传输请求时以弹窗形式展示，不打断用户当前操作
- 弹窗中展示文件树预览、保存路径选择和接收/拒绝按钮
- 创建传输详情页面展示实时进度、文件树、统计信息
- 统一桌面端和移动端的用户体验

**Non-Goals:**
- 不修改传输协议或底层通信机制
- 不改变配对流程
- 不涉及历史记录持久化存储

## Decisions

### 1. 弹窗组件设计

**使用 `ResponsiveDialog` 组件**：复用现有的响应式弹窗组件，适配移动端和桌面端。

**弹窗内容**：
- 头部：图标 + "收到文件" + 发送方设备名
- 文件树区域：使用现有的 `FileTree` 组件（mode="select"）展示预览
- 保存路径：显示默认路径 + "更改"按钮
- 底部按钮："拒绝"（outline）+ "接收"（primary）

### 2. 路由设计

**新增路由**：`/transfer/:sessionId`
- 参数 `sessionId` 为传输会话 ID
- 从 `transfer-store` 中获取会话信息
- 支持三种状态展示：transferring（进度）、completed（完成）、failed（失败）

### 3. 数据流设计

```
收到 transfer-offer
    ↓
pushOffer() 到 pendingOffers
    ↓
TransferOfferDialog 显示（不跳转）
    ↓
用户点击"接收"
    ↓
acceptReceive() → addSession() → navigate(/transfer/:sessionId)
    ↓
传输详情页监听 progress/complete/failed 事件
```

### 4. 移动端 vs 桌面端差异

**移动端**：
- 弹窗使用底部 sheet 形式（ResponsiveDialog 自动适配）
- 传输详情页全屏显示

**桌面端**：
- 弹窗居中显示
- 传输详情页在侧边栏+内容区布局中显示

## Risks / Trade-offs

**[风险] 弹窗可能被用户忽略**
→ 缓解：弹窗有明显的视觉提示（蓝色图标 + 文件信息），且需要用户明确操作

**[风险] 同时收到多个传输请求**
→ 缓解：pendingOffers 队列处理，当前弹窗关闭后才显示下一个

**[权衡] 是否保留 `/receive` 页面**
→ 决定：保留作为备用，但主要流程走弹窗。`/receive` 可作为手动入口查看待处理请求。
