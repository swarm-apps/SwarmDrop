# SwarmDrop AI 时代产品方向

> 日期：2026-06-27
> 定位：把 SwarmDrop 从“跨网络文件传输工具”推进为“人、AI agent、应用之间的可信数据通道”。

## 一句话判断

SwarmDrop 不应该追成“带 AI 聊天框的快传工具”，而应该成为本地优先设备群里的可信传输层：

```text
AI / 人 / App
    |
    v
SwarmDrop
  - MCP Tools
  - Drop Inbox
  - 可信设备策略
  - 可恢复传输
  - 可审计自动化
    |
    v
桌面 / 手机 / 协作者设备
```

AI 时代会产生大量需要落地到设备的临时产物：代码 patch、日志包、截图、录屏、构建产物、会议材料、模型输出、调试报告。SwarmDrop 的优势不是理解这些内容，而是把这些内容安全、可靠、可解释地送到正确设备。

## 外部趋势

### MCP 成为 AI 连接工具的标准入口

Model Context Protocol 是用于把 AI 应用连接到外部数据、工具和工作流的开放标准。对 SwarmDrop 来说，这意味着“发送文件/文本/调试包”不再只是 UI 操作，也可以成为 AI agent 的标准 tool。

但 MCP 的敏感 tool 调用必须重视安全：官方工具规范建议应用展示暴露给模型的工具、明确标识 tool 调用，并在敏感操作上保留 human-in-the-loop 确认能力。SwarmDrop 的“发送文件”天然涉及数据外流，所以 MCP 能力必须带授权、确认和审计。

参考：

- <https://modelcontextprotocol.io/docs/getting-started/intro>
- <https://modelcontextprotocol.io/specification/2025-06-18/server/tools>

### Local-first 仍是差异化根基

Local-first 的核心是协作与数据所有权同时成立：本地可用、跨设备协作、隐私、安全、长期可保存、用户掌控。SwarmDrop 的 P2P、E2E、无账号、无云端依赖，正好能成为 local-first 工具链里的传输层。

参考：

- <https://www.inkandswitch.com/essay/local-first/>

## 当前项目基础

SwarmDrop 已经具备几个适合继续放大的基础：

- P2P 网络：mDNS、DHT、Relay、DCUtR、运行时候选节点发现。
- 设备信任：配对码、局域网发现、设备身份、Stronghold/Keychain。
- 传输能力：分块、E2E 加密、BLAKE3 校验、进度事件。
- 可靠性建设：`transfer-history-and-resume`、`redesign-transfer-lifecycle` 已经把断点续传、持久化历史、epoch、状态机推到主线。
- AI 入口：`add-mcp-send-tools` 已经实现 MCP server、`get_network_status`、`list_available_devices`、`send_files` 的雏形。
- 移动端基础：`crates/core` 已可被桌面和 SwarmDrop-RN 复用，移动端是系统级入口的关键。

因此，下一阶段不是“重新找方向”，而是把这些能力收束成可信数据通道。

## 功能方向

### 1. 可靠传输闭环

这是所有上层能力的地基。

应完成：

- 完整传输、暂停恢复、重启恢复的双端手动验证。
- transfer projection 作为前端唯一状态来源，避免前端自行猜测状态。
- interrupted、peer_offline、app_restarted、local_paused、remote_paused 等原因在 UI 上可解释。
- 已完成文件恢复时可跳过，部分文件恢复时只拉缺失 chunk。
- 大文件 hash/prepare 阶段提供明确进度和取消能力。

为什么优先：AI 自动发送会放大不稳定性。如果传输失败、恢复不了、状态说不清，MCP 做得越强，用户越不信任。

### 2. Agent Drop：MCP v2

当前 MCP v1 是“AI 能发文件”。下一步应该升级为“AI 能安全地发交付物”。

应实现的 tools：

| Tool | 作用 |
| --- | --- |
| `get_network_status` | 查看节点、NAT、relay、连接状态 |
| `list_available_devices` | 列出可发送目标 |
| `send_files` | 发送文件/目录 |
| `send_text` | 把文本保存为临时文件并发送 |
| `send_clipboard` | 发送当前剪贴板内容 |
| `create_share_code` | 为文件或文本创建一次性分享码 |
| `get_transfer_status` | 查询传输进度、状态、错误 |
| `list_transfer_history` | 查询最近传输记录 |
| `cancel_transfer` | 取消指定传输 |

安全与体验要求：

- 默认只监听 `127.0.0.1`。
- MCP server 默认关闭，用户显式开启。
- 首次客户端连接需要 UI 确认。
- 支持客户端白名单。
- 敏感 tool 调用需要确认，至少包括目标设备、文件列表、总大小、hash 摘要。
- 所有 MCP 调用写入审计记录。
- 返回结构化结果，便于 AI 读取 transfer id、状态、错误原因。

### 3. Drop Inbox：统一收件箱

SwarmDrop 的接收端不应该只是一条传输历史，而应该是每台设备的 Drop Inbox。

Inbox item 类型：

- 文件和文件夹。
- 文本片段。
- 剪贴板内容。
- 图片、截图、录屏。
- 代码 patch。
- 日志包、测试报告、构建产物。
- 来自分享码的一次性投递。

每条 item 至少包含：

- 来源设备 / 来源客户端 / 是否由 MCP 发起。
- 接收时间、文件数量、总大小。
- BLAKE3 hash 或 bundle hash。
- 传输路径：LAN / holepunch / relay。
- 操作：打开、保存到、复制文本、转发、删除、显示来源。

核心价值：让“收到东西”成为稳定的信息流，而不是一次性弹窗。

### 4. 可信设备策略

设备关系需要比“已配对/未配对”更细。

建议分层：

| 设备类型 | 默认策略 | 适合场景 |
| --- | --- | --- |
| 自有设备 | 可自动接收，进入默认 inbox | 手机、个人笔记本、台式机 |
| 协作者设备 | 每次确认，显示来源和大小 | 同事、朋友 |
| 临时设备 | 分享码 TTL、一次性权限 | 临时传文件 |
| 高风险来源 | 只进隔离 inbox，不自动打开 | 未知网络、临时会话 |

策略能力：

- 自动接收开关。
- 默认保存位置。
- 单次传输大小限制。
- 是否允许目录。
- 是否允许 MCP 自动发送到该设备。
- 是否要求接收端确认。
- 是否允许 relay 传输。

### 5. Artifact Bundle：AI 调试/交付包

AI 工作流经常不是发一个文件，而是发一组相关证据。

建议新增 bundle 概念：

```text
artifact bundle
  - manifest.json
  - files/
  - logs/
  - screenshots/
  - recordings/
  - patches/
  - reports/
```

典型场景：

- “把这次失败测试的截图、日志和 HTML report 发到我的手机。”
- “把当前构建产物和 release note 发到另一台电脑。”
- “把这段 patch 和相关文件发给协作者。”

实现边界：

- v1 可以只是临时目录 + manifest + 压缩/目录传输。
- 不需要理解内容语义。
- bundle hash 作为审计和完整性校验依据。

### 6. 移动端系统级入口

移动端的价值不是“也有一个 App”，而是成为系统分享链路的一部分。

应实现：

- Android share sheet / iOS share extension。
- 从相册、文件、浏览器、聊天 App 直接发送到 SwarmDrop。
- Drop Inbox 移动端列表。
- 通知操作：接受、保存、打开、拒绝。
- 自有设备自动接收。
- 前台优先，后台传输作为后续增强。

移动端优先级应该跟 Drop Inbox 绑定，否则移动端只是另一个手动打开的传输页面，频率起不来。

### 7. 网络可解释性和自愈

P2P 产品不怕复杂，怕用户不知道发生了什么。

应展示：

- 当前传输路径：LAN / holepunch / relay。
- NAT 状态。
- relay reservation 是否可用。
- 使用了哪个 bootstrap/helper 节点。
- 连接失败原因：设备离线、DHT 未找到、打洞失败、relay 不可用、对方拒绝。
- 一键诊断：复制诊断报告、发送诊断包。

和现有 `auto-discover-lan-helper-nodes` 的关系：

- 局域网协助节点是降低配置成本的方向。
- 下一步要把“为什么网络可用/不可用”解释给普通用户。

## 推荐实施顺序

### P0：收尾可靠传输

对应现有：

- `transfer-history-and-resume`
- `redesign-transfer-lifecycle`
- `add-p2p-data-channel`

验收：

- 完整发送接收后历史可见。
- 暂停后恢复继续传输。
- app 重启后可恢复 paused/interrupted 会话。
- 双端状态一致。
- 跨桌面/RN 的共享 core 语义清楚。

### P1：MCP Agent Drop v2

对应新变更候选：

- `mcp-agent-drop-v2`

范围：

- 扩展 MCP tools。
- 增加 transfer status。
- 增加审计记录。
- 增加客户端授权和敏感操作确认。
- 优化 MCP guide。

### P2：Drop Inbox v1

对应新变更候选：

- `drop-inbox-v1`

范围：

- 统一收件箱数据模型。
- 文件/文本/剪贴板都进入 inbox。
- 接收端操作和来源信息。
- 与传输历史的边界：history 记录传输过程，inbox 记录收到的内容。

### P3：可信设备策略

对应新变更候选：

- `trusted-device-policies`

范围：

- 设备信任层级。
- 自动接收规则。
- 保存位置规则。
- MCP 是否可自动向某设备发送。
- 临时设备和一次性分享策略。

### 当前落地语义

`drop-inbox-and-transfer-activity` 与 `trusted-device-policies` 落地后，桌面端 v1 采用以下边界：

- **收件箱**记录已经成功接收的内容，面向用户的“我收到了什么”。收件箱条目可打开、显示位置、导出、归档、删除记录；删除本地文件必须单独确认。
- **活动与恢复**记录传输过程，面向“现在发生了什么、什么可以恢复、为什么失败”。清空活动记录只清理过程账本，不删除收件箱条目，也不删除已接收文件。
- **自动接收**只由可信设备策略触发。自动接收仍走标准接收流程，完成后进入收件箱；失败、中断、拒绝只留在活动与恢复。
- **信任等级**分为本人设备、协作者、临时设备和已阻止。本人设备可配置自动接收；协作者默认需要确认；临时设备默认有大小和目录限制；已阻止设备的入站 offer 会被策略拒绝。
- **策略原因**写入活动投影。自动接收和策略拒绝都应在活动与恢复中可解释，而不是混入普通错误文案。

### P4：Artifact Bundle + 移动端入口

对应新变更候选：

- `artifact-bundle-transfer`
- `mobile-share-sheet-and-inbox`

范围：

- AI 调试包/交付包。
- 系统分享菜单。
- 移动端 Drop Inbox。
- 通知操作。

## OpenSpec 候选变更

| Change | 目标 | 依赖 |
| --- | --- | --- |
| `mcp-agent-drop-v2` | 让 AI agent 能安全、可审计地发送文件/文本/剪贴板并查询状态 | `add-mcp-send-tools`、可靠传输 |
| `drop-inbox-v1` | 建立统一收件箱，承接文件、文本、剪贴板、分享码投递 | 传输历史、保存位置策略 |
| `trusted-device-policies` | 自有设备/协作者/临时设备的接收、保存、确认策略 | 设备管理、配对系统 |
| `artifact-bundle-transfer` | 支持 AI 调试包/交付包，带 manifest 和整体 hash | MCP v2、目录传输 |
| `mobile-share-sheet-and-inbox` | 移动端系统分享入口和收件箱体验 | RN core、Drop Inbox |
| `network-diagnostics-v1` | 网络状态解释、一键诊断报告、诊断包发送 | 网络状态投影、helper nodes |

## 不做什么

- 不做云盘：SwarmDrop 是通道，不是长期存储中心。
- 不做聊天：消息流会稀释“可信传输”的定位。
- 不内置大模型：内容理解交给 SwarmNote 或外部 AI。
- 不做泛泛 AI 文件管理：SwarmDrop 不需要替用户理解文件，只要可靠交付。
- 不把 relay 当作中心化服务卖点：relay 是兜底路径，不是产品核心。

## 和 SwarmNote 的关系

SwarmNote 和 SwarmDrop 应该分工清楚：

| 产品 | 时代定位 | 负责 |
| --- | --- | --- |
| SwarmNote | 本地优先知识和长期记忆 | 笔记、memory、检索、上下文 |
| SwarmDrop | 可信数据流动 | 文件、文本、产物、设备间投递 |

组合后的图景：

```text
SwarmNote 负责“我知道什么”
SwarmDrop 负责“我把东西送到哪里”
```

AI 时代真正有价值的是这两个能力一起成立：知识在本地，数据在自己的设备群里流动。

## 成功指标

产品指标：

- 用户可以在 10 秒内把 AI 生成的文本/文件发到另一台设备。
- 大文件中断后无需重来。
- 用户能解释一次失败：对方离线、打洞失败、relay 不可用、接收方拒绝。
- MCP 发起的每次发送都有审计记录。
- 自有设备之间可以低摩擦自动接收。

工程指标：

- 传输状态由 core 投影统一驱动。
- MCP tools 不绕过现有 TransferManager。
- 所有高风险 MCP tool 都有确认或策略 gate。
- Drop Inbox 与 transfer history 边界清楚。
- 桌面/RN 共用 core 语义，不分叉实现。

## 结论

SwarmDrop 的未来不是“更花哨的快传”，而是成为本地优先 AI 工作流的基础设施：

```text
可靠传输是底座
MCP 是 AI 入口
Inbox 是用户体验
设备策略是信任边界
移动端是日常入口
```

先收稳可靠传输，再把 MCP 做成安全可审计的 Agent Drop，随后建设 Drop Inbox 和可信设备策略。这条路线既能紧跟 AI 时代，也不会丢掉 SwarmDrop 最稀缺的东西：无云、E2E、P2P、用户掌控。
