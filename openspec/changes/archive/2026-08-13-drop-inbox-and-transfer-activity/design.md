## 背景

SwarmDrop 当前的传输页面同时展示活跃传输和基于数据库投影的历史记录。近期传输工作已经把产品推向更明确的生命周期模型：`transfer_sessions` 记录方向、对端快照、总字节数/已传字节数、phase、suspended/terminal reason、epoch、可恢复性、保存位置；`transfer_files` 记录 checksum、checkpoint bitmap/ranges 和 source path。

这套传输账本对恢复、诊断和审计仍然必要。但当 SwarmDrop 有了真正的接收体验后，它不适合作为用户查看“收到内容”的主界面。成功收到文件的用户需要内容视角的收件箱；调试暂停、失败或可恢复传输的用户需要过程视角的活动与恢复。

本变更引入如下拆分：

```text
transfer_sessions / transfer_files
  = 过程账本
  = 活跃进度、恢复、失败原因、诊断

inbox_items / inbox_item_files
  = 已接收内容索引
  = 打开、显示位置、另存/导出、删除、来源详情
```

本变更优先落桌面端。数据模型和 core API 应保持 host-neutral，方便 SwarmDrop-RN 后续复用；移动端 UI 可以在后续变更中补齐。

## 目标 / 非目标

**目标：**

- 为成功接收的文件/目录新增持久化 Drop Inbox。
- 将现有传输历史界面重塑为活动与恢复。
- 保留传输会话作为恢复和诊断的过程账本。
- 避免同一份已接收内容在主界面中出现两条同级记录。
- 明确收件箱内容和传输活动日志之间的保留/删除语义。
- 为未来文本、剪贴板、Artifact Bundle 预留 content kind，但 v1 只实现文件/目录。

**非目标：**

- 本变更不实现文本、剪贴板或 Artifact Bundle 发送。
- 不替换传输生命周期、epoch 或恢复协议。
- 不增加云同步，也不把 SwarmDrop 变成长期内容存储中心。
- 不实现移动端 share sheet 或移动端收件箱 UI。
- 不实现设备信任策略 gate；这属于后续 `trusted-device-policies` 变更。

## 决策

### 1. 收件箱是独立内容表，不是传输历史改名

新增收件箱持久化，而不是复用/重命名 `transfer_sessions`。

建议模型：

```text
inbox_items
  id                  UUID
  transfer_session_id UUID NULL
  source_peer_id      TEXT
  source_name         TEXT
  source_kind         TEXT  -- paired_device | share_code | mcp | unknown
  content_kind        TEXT  -- files | text | clipboard | bundle（v1 仅 files）
  title               TEXT
  item_count          INTEGER
  total_size          INTEGER
  root_path           TEXT NULL
  content_hash        TEXT NULL
  received_at         INTEGER
  last_opened_at      INTEGER NULL
  archived_at         INTEGER NULL
  deleted_at          INTEGER NULL

inbox_item_files
  id                  INTEGER
  inbox_item_id       UUID
  transfer_file_id    INTEGER NULL
  relative_path       TEXT
  name                TEXT
  size                INTEGER
  checksum            TEXT
  local_path          TEXT
```

理由：传输会话回答“发生了什么”，收件箱回答“我收到了什么”。二者分离后，用户清理诊断记录不会丢失已接收内容记录；未来文本、剪贴板和 bundle 也能进入收件箱，而不必伪装成文件传输会话。

备选方案：把已完成接收会话直接当成收件箱记录。拒绝原因：内容操作会和恢复/诊断状态耦合，删除语义会变得混乱。

### 2. 收件箱记录在接收完成时创建

core 接收完成路径应在所有选中文件校验并落盘完成后创建一个收件箱条目。

规则：

- 只有 `direction=receive` 的会话能创建收件箱条目。
- 只有成功完成的会话能创建收件箱条目。
- 暂停、中断、失败、拒绝、取消的会话留在活动与恢复中。
- 收件箱创建必须按 `transfer_session_id` 幂等。
- 如果文件已经完成落盘但收件箱创建失败，后续投影刷新或修复命令应能补建收件箱条目。

### 3. 活动与恢复继续使用现有传输投影

当前传输页应被重塑，而不是删除。它变成过程视角界面：

- 活跃：phase active。
- 可恢复：phase suspended 且 `recoverable=true`。
- 失败 / 已取消：包含原因的 terminal 记录。
- 已完成：可保留为诊断尾部，但不再是查看已接收内容的主入口。

主导航文案应从“传输历史”转向“活动”或“活动与恢复”。收件箱成为已接收内容的主入口。

### 4. 收件箱删除和活动清理必须分开

删除语义必须显式：

| 用户操作 | 效果 |
| --- | --- |
| 删除收件箱条目 | 删除/软删除内容视角记录；可在确认后选择删除本地接收文件 |
| 清空活动与恢复 | 删除可遗忘的过程日志；不得删除收件箱记录或已接收文件 |
| 删除已关联收件箱的 transfer session | 只作为过程日志清理；除非用户单独删除，否则关联收件箱条目仍保留 |
| 用户在 SwarmDrop 外移动/删除接收文件 | 收件箱条目保留，但打开/显示时标记为 missing |

理由：避免“清空历史记录”误删用户刚收到的文件。

### 5. 内容操作由 host command 基于本地路径执行

收件箱操作通过 host commands 暴露：

- 按过滤条件列出收件箱条目。
- 获取收件箱条目详情和文件列表。
- 打开文件或目录。
- 在系统文件管理器中显示条目/文件。
- 另存/导出/复制到目标位置。
- 归档或删除条目。
- 显示来源和传输详情。

core 负责数据和投影；Tauri 负责平台相关的打开、显示、另存操作。

### 6. 文本/剪贴板/bundle 只预留模型，不进入 v1 UI

`content_kind` 包含未来值，但 v1 创建路径只产生 `files`。这样后续 MCP v2 添加 `send_text`、`send_clipboard` 或 Artifact Bundle 时，不需要再次大改 schema。

## 风险 / 权衡

- **[风险] 完成路径重试导致重复记录** -> 对 `transfer_session_id` 建唯一约束，并让收件箱创建幂等。
- **[风险] 用户混淆收件箱和活动** -> 导航、空状态和文案必须明确：收件箱是已接收内容，活动是传输/恢复状态。
- **[风险] 清空活动误删内容** -> command 命名和 DB 操作保持分离，并补测试覆盖保留语义。
- **[风险] 本地文件被外部移动或删除** -> 操作时标记 missing 并给出恢复/定位提示，而不是静默失败。
- **[风险] 移动端分叉** -> core 结构保持 host-neutral，避免在共享类型中写死桌面概念。

## 迁移计划

1. 新增收件箱表和 entity model。
2. 在接收完成路径中加入幂等收件箱创建。
3. 新增收件箱查询/操作 Tauri commands。
4. 新增收件箱 UI 路由和导航。
5. 将传输历史 UI 改名/重塑为活动与恢复，同时保留投影 API。
6. 更新删除/保留语义相关文案和文档。

回滚策略：`transfer_sessions` 仍保留为事实源。如果收件箱创建或 UI 出问题，活动与恢复仍可显示传输投影，收件箱表可暂时不使用。

## 待确认问题

- v1 是否应该把已接收文件移动进受管理的 Inbox 目录，还是只索引用户选择的保存位置？初步建议：原地索引，避免意外移动文件。
- 删除收件箱条目时默认保留文件还是删除文件？初步建议：默认保留文件，删除本地文件必须单独勾选并确认。
- 是否要给已发送内容做 Outbox？初步建议：本变更不做，先聚焦接收内容。
