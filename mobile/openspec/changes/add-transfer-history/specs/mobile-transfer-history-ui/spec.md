## ADDED Requirements

### Requirement: 传输历史页双 section 布局

`src/app/transfer/index.tsx` 渲染的页面 SHALL 包含两个独立的 section：

1. **活跃传输**：从 store.sessions 派生，按 startedAt 降序，使用 `RecentTransferRow` 组件渲染（展示实时 progress / speed / eta）。仅在 sessions 非空时显示。
2. **传输历史**：从 store.dbHistory 派生，使用新组件 `HistoryTransferRow` 渲染。仅在 dbHistory（按过滤后）非空时显示。

页面 SHALL 在 active 与 history 同时为空时显示空状态：图标 + "暂无传输记录" + 副标 "在主屏选择已配对设备开始传输"。

页面挂载时 SHALL 主动调用 `store.loadHistory()` 以刷新历史。

#### Scenario: 同时有活跃和历史

- **GIVEN** sessions 非空、dbHistory 非空
- **WHEN** 用户进入「传输历史」页
- **THEN** 页面 SHALL 同时展示两个 section，活跃在上、历史在下

#### Scenario: 只有历史

- **GIVEN** sessions 为空、dbHistory 非空
- **WHEN** 用户进入页面
- **THEN** 页面 SHALL 仅展示历史 section + 其工具栏

#### Scenario: 完全为空

- **GIVEN** sessions 与 dbHistory 都为空
- **WHEN** 用户进入页面
- **THEN** 页面 SHALL 显示空状态视图

### Requirement: 历史状态过滤器

「传输历史」section 的标题右侧 SHALL 提供一个状态过滤控件（segmented control / pill / native picker，依平台选择最自然的实现），选项为：

- 全部 (默认)
- 已完成 (Completed)
- 失败 (Failed)
- 已暂停 (Paused)
- 已取消 (Cancelled)

过滤状态 SHALL 仅作用于当前会话（不持久化）。

#### Scenario: 切换过滤

- **GIVEN** dbHistory 包含多种状态
- **WHEN** 用户选择 "已完成"
- **THEN** 历史 section SHALL 仅展示 status === Completed 的项

#### Scenario: 过滤后为空

- **GIVEN** 用户选择 "失败" 但 dbHistory 中没有失败项
- **WHEN** 过滤生效
- **THEN** 历史 section SHALL 隐藏（不出现"该分类下无记录"的小空状态，避免冗余）

### Requirement: 清空全部历史操作

「传输历史」section 工具栏 SHALL 提供「清空」按钮，点击后弹出确认对话框（platform-native 风格），用户确认后调用 `store.clearAllHistory()`。

清空按钮 SHALL 在 dbHistory 为空时隐藏。

#### Scenario: 用户清空成功

- **WHEN** 用户点击「清空」并确认
- **THEN** store 调用 native clear，dbHistory 变空，UI 切回空状态，toast 显示 "已清空传输历史"

#### Scenario: 用户取消清空

- **WHEN** 用户点击「清空」但在对话框中取消
- **THEN** dbHistory 保持不变，无任何 toast

### Requirement: 历史卡片组件 HistoryTransferRow

新增组件 `src/components/history-transfer-row.tsx`，输入 `MobileTransferHistoryItem`，展示：

- 方向图标（send: 蓝色右上箭头 / receive: 绿色左下箭头）
- 对端名 + 状态徽章（颜色按状态分类，与桌面端 `STATUS_CLASSNAMES` 风格一致）
- 文件数 + 总大小（人类可读，如 "3 个文件 · 12.4 MB"）
- 相对时间（如 "5 分钟前"，由 dayjs / date-fns 计算）
- 失败时显示截断的 error_message 副标
- 整行 Pressable，点击导航到详情页 `/transfer/[sessionId]`

#### Scenario: 完成态卡片

- **GIVEN** item.status === Completed
- **THEN** 状态徽章 SHALL 是绿色"已完成"

#### Scenario: 失败态卡片显示原因

- **GIVEN** item.status === Failed 且 item.errorMessage === "app_interrupted"
- **THEN** 卡片 SHALL 显示翻译后的人类可读原因，如 "上次未完成"

#### Scenario: 点击导航

- **WHEN** 用户点击历史卡片
- **THEN** 路由 SHALL 推入 `/transfer/[sessionId]?sessionId=<id>`

### Requirement: 详情页基本信息呈现

`src/app/transfer/[sessionId].tsx` 详情页 SHALL：

1. 先尝试从 `store.sessions[sessionId]` 读取（活跃）
2. 否则从 `store.dbHistory` 找匹配的 `MobileTransferHistoryItem`
3. 都没找到则调用 `MobileCore.getTransferSessionDetail(sessionId)` 兜底
4. 仍没找到则显示"会话不存在"空状态

页面 SHALL 展示：方向、对端名、对端 ID、状态、开始时间、结束时间（如有）、总大小、已传大小、错误原因（如有）、保存路径（如 receive 且 saveLocation 存在）。

#### Scenario: 从活跃 session 进入

- **GIVEN** sessionId 在 sessions 中
- **WHEN** 用户从主屏的"活跃传输"卡片点进来
- **THEN** 详情页 SHALL 渲染实时数据（含 speed / eta）

#### Scenario: 从历史进入

- **GIVEN** sessionId 仅在 dbHistory 中
- **WHEN** 用户从历史卡片点进来
- **THEN** 详情页 SHALL 渲染历史快照（无 speed/eta，但有最终态信息）

#### Scenario: 直接 deep link 进入

- **WHEN** 用户通过外部 deep link 进入一个既不在 sessions 也不在 dbHistory 的 sessionId
- **THEN** 详情页 SHALL 先显示 loading，再调用 native 兜底查询；成功则展示，失败则展示"会话不存在"空状态

### Requirement: 详情页文件列表与分享

详情页 SHALL 在「文件」section 展示该会话的所有文件，每一行展示文件名 + 路径 + 大小，并提供以下交互：

- **点击文件行**：检查 `Sharing.isAvailableAsync()`；可用则 `Sharing.shareAsync(file_uri, { dialogTitle: file.name })` 拉起系统分享菜单；不可用则降级为「复制路径」
- **长按文件行**：`Clipboard.setStringAsync(file_uri)` + toast "已复制路径"

分享 SHALL 仅对方向为 receive、status 为 Completed 且 save_path 存在的会话开放（即文件确实落地了）。其他情况下文件行展示为静态信息，不可点击。

#### Scenario: 已完成接收的文件分享

- **GIVEN** item.direction === "receive" && item.status === Completed && item.savePath 存在
- **WHEN** 用户点击文件行
- **THEN** 系统分享菜单 SHALL 拉起，包含「用 X 打开」「保存到照片」「AirDrop / 蓝牙」等条目

#### Scenario: 发送端文件不能分享

- **GIVEN** item.direction === "send"
- **THEN** 文件行 SHALL 不可点击，长按依然可以复制源路径

#### Scenario: Sharing 不可用降级

- **WHEN** `Sharing.isAvailableAsync()` 返回 false
- **THEN** 点击行为 SHALL 降级为复制路径 + toast 提示

### Requirement: 详情页操作按钮

详情页底部 SHALL 根据会话状态展示一组操作按钮：

| 状态 | 主操作 | 次操作 |
|------|--------|--------|
| Transferring（活跃） | 暂停 | 取消 |
| Paused | 恢复 | 删除 |
| Failed | 重新发送 (仅 direction=send) / 恢复 / 删除 | — |
| Completed | 重新发送 (仅 direction=send) | 删除 |
| Cancelled | 重新发送 (仅 direction=send) | 删除 |

按钮触发的 store action：

- 暂停 → `MobileCore.pauseTransfer(sessionId)`
- 取消 → `MobileCore.cancelTransfer(sessionId)`
- 恢复 → `store.resumeHistoryItem(sessionId)`
- 删除 → `store.deleteHistoryItem(sessionId)` + 弹确认 + 删除成功后 `router.back()`
- 重新发送 → 复用历史 item 的 files 调用 `prepareSend` + `sendPrepared`

#### Scenario: 暂停活跃传输

- **GIVEN** session 状态为 Transferring
- **WHEN** 用户点击"暂停"
- **THEN** native `pauseTransfer` SHALL 被调用，事件 `TransferPaused` 到达后 UI SHALL 反映 paused 状态

#### Scenario: 恢复 paused 接收

- **GIVEN** item.status === Paused
- **WHEN** 用户点击"恢复"
- **THEN** store.resumeHistoryItem SHALL 被调用，成功后页面 SHALL 显示新的 transferring session

#### Scenario: 重新发送 completed 历史

- **GIVEN** item.direction === "send" && item.status === Completed
- **WHEN** 用户点击"重新发送"
- **THEN** UI SHALL 调用 prepareSend + sendPrepared，导航到新 session 的详情页

#### Scenario: 删除单条历史

- **WHEN** 用户点击"删除"并确认
- **THEN** native `deleteTransferSession` SHALL 被调用，dbHistory 刷新后该项消失，路由回退

### Requirement: 国际化

所有新增 UI 文案 SHALL 通过 `@lingui/react/macro` 的 `<Trans>` 与 `useLingui().t` 标记，新增的 message 同步进 `src/locales/*.po`，并提供至少 zh-CN / en 两个 locale 的翻译。

特殊文案约定：

- `error_message === "app_interrupted"` SHALL 翻译为 zh "上次未完成" / en "Interrupted"
- 状态徽章文案 SHALL 与桌面端术语保持一致（已完成 / 失败 / 已暂停 / 已取消 / 传输中 / 等待响应 / 准备中）

#### Scenario: 切换语言

- **GIVEN** 用户把系统语言切到 en
- **WHEN** 用户进入「传输历史」页
- **THEN** 所有 UI 文案 SHALL 显示英文版本，包括状态徽章、错误原因映射、按钮文字
