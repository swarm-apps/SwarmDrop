# Design

## 背景与目标

让 AI Agent 拿到**接收侧决策权**——把入站文件在 `RequireConfirmation` 分支上"接受（含落盘位置）/ 拒绝"的能力从"仅人可做"扩展为"授权后 agent 亦可做"，从而使 agent 成为 SwarmDrop 数据通道上的**对称 peer**（能发、能读、也能收）。目标是**零协议改动、零 core 传输逻辑改动**，全部复用既有接收薄壳，风险控制在"新增一个每设备门控 + 两个触发工具"。

## 入站接收现状（决策依据）

```
对端 TransferRequest::Offer
   │  handle_incoming_transfer_request (crates/core/src/transfer/incoming.rs)
   ├─ 未配对 → OfferResult{accepted:false, NotPaired}
   ├─ is_receiving_paused() → OfferResult{ReceivingPaused}
   ├─ evaluate_receive_policy (crates/core/src/transfer/policy.rs)
   │     ├─ Blocked / 超 max_bytes / 含目录且不允许 / 过期 → Reject（自动婉拒 + 记录）
   │     ├─ 可信 + 配了 default_save_location + (直连或允许中继自动接收) → AutoAccept（自动落盘）
   │     └─ 其余 → RequireConfirmation ───┐
   │                                       ▼
   │   cache_inbound_offer: 落一条 phase=Offered 的 receive 会话
   │        + 内存 pending 挂住 PendingOffer{ pending_id, peer_id, files, ... }
   │        + publish TransferOfferReceived 事件（前端弹窗）
   │                                       │
   │   人在 app 点接受/拒绝 ───────────────┤
   │        accept_and_start_receive(session_id, save_location)  ← 攥着 pending_id 回 OfferResult{accepted,key}
   │        reject_and_respond(session_id)                        ← 回 OfferResult{accepted:false}
```

关键事实：
- 挂起 offer 在内存 `TransferManager.pending: DashMap<Uuid, PendingOffer>` 里，`PendingOffer` 含 `pending_id`（对端那条 request-response 的回复句柄）与 `peer_id`（来源设备）。
- `accept_and_start_receive` / `reject_and_respond` 已封装全部落盘/回复/状态机逻辑，MCP 只需触发。
- 走到 pending `RequireConfirmation` 的 offer，**已经过** `max_transfer_bytes` / `allow_directories` / `Blocked` / 过期 各闸。

## 决策

### 决策 1：新增每设备门控 `allow_mcp_accept_from_device`，与发送侧对称

发送侧已有 `receive_policy.allow_mcp_send_to_device`，`send_files` 在 `tools.rs` 里检查它（false 直接拒发）。接收侧加一个兄弟布尔字段 `allow_mcp_accept_from_device`，语义是"**允许 MCP/AI 代该设备处置入站 offer（接受或拒绝）**"：

- 默认 `false`；**只能人在 app 的设备信任策略里开**，agent 无写权限（配对/策略写操作保持 app-only，防自我提权）。
- 它**同时门控** `accept_transfer` 与 `reject_transfer`——一个开关决定"agent 是否有权对该来源设备的入站 offer 做决定"。flag 关时两个工具都拒绝、offer 留给人。
- 检查发生在**工具层**，不进 `evaluate_receive_policy`：policy 评估在 offer 到达时就跑完了（产出 RequireConfirmation），门控是"事后是否让 agent 接手那个已挂起的决定"，语义上属工具层。

### 决策 2：门控检查需先按 session_id 定位 pending offer 的来源设备

`accept_transfer(session_id)` 只拿到会话 id，但门控要查**来源设备**的 flag。因此工具流程：

1. 由 `session_id` 在 `TransferManager.pending` 里 peek 出 `PendingOffer.peer_id`（**不移除**）；pending 不存在 → `isError`（已被接受/拒绝/过期）。
2. `manager.pairing().get_paired_device(peer_id)` 取来源设备；`receive_policy.allow_mcp_accept_from_device == false` → `isError`，提示"该设备未开启『允许 MCP 代收』，请在 SwarmDrop 设备策略中开启"。
3. 通过则调用 `accept_and_start_receive(session_id, save_location)`（它内部 remove pending）/ `reject_and_respond(session_id)`。

`reject_transfer` 同样先门控再 `reject_and_respond`。

### 决策 3：保存位置——缺省与手动接收共用同一「接收文件夹」，不搞特殊；来源靠收件箱标记区分

`accept_transfer(session_id, save_path?: string)`：

- 传了 `save_path`（绝对路径）→ 用它，`CoreSaveLocation::Path { path }`。
- 没传 → 用**与手动接收一致的接收文件夹**：优先读用户在设置里配的 `preferences.transfer.savePath`，未配则回退 `<下载目录>/SwarmDrop`（与前端 `getDefaultSavePath` 对齐）。手动接受对话框同样改读这个 pref，二者默认落同一处。
- **不搞独立 agent 子目录**（早期设计曾用 `agent-inbox`，已推翻）：agent 代收本质就是"agent 替人在接受对话框做决定"，落盘位置不该分叉。要区分"谁收的"用**来源标记**而非目录隔离——见决策 3b。

### 决策 3b：agent 代收在收件箱标记为「AI 代理」来源（复用既有 provenance，零新 UI）

收件箱已有 `source_kind`（由会话 `origin` 派生：`Mcp`→"AI 代理"、否则"来自设备"），且 UI 已渲染该徽章。agent 代收接受成功后，`accept_transfer` 把该会话 `origin` 更新为 `TransferOrigin::Mcp { client }`（尽力带 initialize 握手报告的客户端名，如 `mcp:claude-desktop`），使完成后建的收件箱条目 `source_kind=mcp` → 自动显示「AI 代理」。**不新增字段、不改收件箱 UI**——落盘同处、来源可辨。

### 决策 4：发现走既有 list_transfers，本期不加专用工具、不主动推送

- 挂起 offer 已以 `phase=Offered / direction=receive` 落库并出现在 `list_transfers`（`get_transfer_projections`）里——agent **轮询 list_transfers 过滤 `phase=Offered`** 即可发现待审 offer。本期**不新增** `list_pending_offers`（避免与 list_transfers 重复；如实际手感需要再加，见 Open Questions）。
- **不做 server 主动 notification 戳 agent**：MCP 单向通知虽全 client 可用，但多数 client 收到通知**不会自动重新唤起模型**去调工具，可靠性不足以作为唤醒机制。发现依赖 agent 处于活跃轮询循环。

### 决策 5：把决策窗口对齐到「协议真实上限 ≈180s」，并写进 spec 与 guide

> **实现/审查阶段修正**：本决策原写「对齐到 300s / 5 分钟」，但已证伪——`RequestOptions::with_timeout`
> 只是 client 侧 `tokio::timeout` 包装，只能让调用方比协议**更早**放弃，**无法突破** libp2p
> 全局协议超时 `req_resp_timeout`。而该全局值在 `crates/core/src/network/config.rs` 实为 **180s**
> （非早先文档误写的 120s）。故 Offer 的**有效窗口封顶 ≈ 180s（约 3 分钟）**，`with_timeout(300s)`
> 是 no-op。以下为修正后的实际方案。

发送端那条 Offer 请求受全局 `req_resp_timeout=180s` 约束（`behaviour.rs` 把它设为 libp2p
`request_response` 的 `request_timeout`）；接收端 `PENDING_OFFER_TIMEOUT_SECS` 回收内存 pending。
真实有效窗口 = 二者取小。**不提升全局超时**（那会拖慢 pairing / resume probe 等所有请求的失败检测）。对齐后：

- agent（和人）有 **≈180s（约 3 分钟）**窗口决定接受/拒绝——受协议上限封顶，无法更长（要更长需协议层改造：Offer 先 ack + 决策走独立异步通道，另立 change）。
- 为避免"接收端刚接受、发送端恰好超时 → 回复通道已关"的边界竞态，接收端 `PENDING_OFFER_TIMEOUT_SECS` SHALL **小于**真实协议窗口（取 **170s** < 180s），保证 pending 先于发送端放弃被回收。`OFFER_RESPONSE_TIMEOUT_SECS` 取与协议上限一致的 180s、不再做无谓的"更长"承诺。
- 含义仍成立：**"agent 自主代收"是常驻/循环 agent 能力**——一次性 agent 不在循环里，窗口过后 `accept_transfer` 拿到"pending 不存在"仍如实 `isError`。这是模型边界，显式写进 spec（`accept_transfer` 会话已过期场景）与 `swarmdrop://guide`（"要代收就进 watch 循环，别指望被动唤醒；窗口约 3 分钟"）。

### 决策 6：能力提示与不改 core

- `accept_transfer` 有落盘副作用 → `read_only_hint=false`；`reject_transfer` 会婉拒对端 → 标 `destructiveHint`（与 `cancel_transfer` 同类）。
- **不改** core 传输状态机、**不动**协议、**不开** rmcp `elicitation` feature。仅新增策略字段 + 两个工具触发入口。

## Open Questions

- **专用发现工具**：若实际使用中 agent 从 list_transfers 里滤 `phase=Offered` 手感差（噪音多），再加一个 `list_pending_offers` 只回待审 offer——本期不预先复杂化。
- **notification 唤醒**：待主流 client 支持"通知触发工具轮询"或有稳定的 server→client 唤醒语义后，可加一条 `notifications/message` 作为**减少盲轮询**的增强（非替代 watch 循环）。
- **reject 是否需门控**：本设计让 `allow_mcp_accept_from_device` 同时门控 accept 与 reject（一个开关 = "agent 可否代该设备决定"）。若日后想"agent 可拒不可收"，再拆成两个 flag。

## 附：elicitation 推送 vs pull（技术备注）

本期发现走 **pull**（agent 轮询 `list_transfers` 滤 `phase=Offered`），**不用** server 主动把 offer 推给 agent。这里记录被否掉的「推送」方案的机制与否掉原因，供日后（若要支持一次性 agent）参考。

### 推送靠的是 MCP **Elicitation**，不是"推一条 offer 数据"

MCP 是双向 JSON-RPC——server 也能反向对 client 发起 request。所谓"推审批"是 server 发一个 `elicitation/create` request，client 回一个 response：

```
server → client:  {"method":"elicitation/create",
                   "params":{"message":"收到 X 的 foo.zip，接收?",
                             "requestedSchema":{ accept:boolean, savePath:string }}}
client → server:  {"result":{"action":"accept"|"decline"|"cancel", "content":{...}}}
```

### 三层实现

1. **rmcp API**：`context.peer`（`Peer<RoleServer>`）上有 `elicit::<T>(message).await`（`elicitation` feature 门控，本期未开）；返回 `Result<Option<T>, ElicitationError>`，`UserDeclined` / `UserCancelled` 为显式变体。在 tool handler 或后台 task（clone peer）里 `.await` 即阻塞到 client 回复。
2. **传输（关键）**：Streamable HTTP 下 server→client 消息只能走 SSE。server **主动发起**的 request 走 client 用 **HTTP GET** 打开的独立下行 SSE 流（区别于 client POST 请求所回的关联流）。前提：client 先发过 GET 且 server 返回 `text/event-stream`（非 405）+ **stateful session**（`Mcp-Session-Id`）。我们 `server.rs` 的 `stateful_mode = true` + `LocalSessionManager` + `sse_keep_alive` **已满足此前提**。stdio 传输则天然全双工，无需这些。
3. **响应回程**：client 的 response **另发一个 HTTP POST**（JSON-RPC response，server 回 202），按 session-id 路由回挂起的 `elicit().await`。

### 为什么本期不用（三个 catch）

- **推给的是"client 侧的人"，非 agent 的模型**：Claude Code 等会弹交互对话框给用户 → 本质是把人工确认从 SwarmDrop app 挪到 MCP 客户端弹窗，而非"agent 自主决策"。与本 change「agent 成为对称 peer、自主代收」的目标不符。
- **client 支持面窄**：Claude Code / Cursor 支持 elicitation，**Claude Desktop 不支持**——不能当唯一路径。
- **架构坎**：入站 offer 在**网络事件循环**里处理，那里没有 `context.peer`（`RequestContext` 只在 tool handler 内）。要推，需把活跃 MCP 会话的 `Peer<RoleServer>` 句柄捕获 / 注册出来交给传输子系统，还要决定"多个 agent 会话推给谁"。

### 结论

pull 全 client 可用、且真正让 agent 做决策，代价是 agent 需在活跃 watch 循环里、有约 3 分钟窗口（决策 4 / 5）。elicitation 推送留作**未来支持"一次性 / 人在环"agent** 的可选增强，届时单独立 change（含 peer 句柄捕获、多会话路由、feature 开关与 client capability 探测）。
