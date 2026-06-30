# Design

## 目标与原则

一次传输从"谁发起"的角度只有两类：**人在应用里发**，或 **AI 代理经 MCP 发**。把这个事实做成一个干净的 `TransferOrigin` 枚举，从 wire 一路透传到接收端 UI 与 inbox；同时把发送端那道"是否允许 agent 往该设备发"的闸门（既有死字段）接通。**架构上不引入"溯源子系统"，只加一个枚举 + 一道闸门 + UI 顺势展示。**

## 决策

### 决策 1：用枚举 `TransferOrigin`，不用字符串前缀

```rust
// crates/core/src/protocol.rs（与 Offer 同处，需上 wire）
pub enum TransferOrigin {
    Human,
    Mcp { client: Option<String> }, // client = MCP 客户端名（claude-desktop / cursor…），拿不到为 None
}
```

调研初稿曾建议 `origin: String` + `"mcp-"` 前缀判断——**否决**：stringly-typed、易写错、`source_kind` 派生要靠前缀匹配。枚举类型安全、`match` 即派生，契合项目整洁层次的口味。

### 决策 2：必填字段，不做向后兼容

按用户决策，两端同版本发布，wire `Offer.origin` 直接是**必填** `TransferOrigin`，不用 `Option` + `#[serde(default)]` 兼容垫片，也不留"None=老版本人工发"的歧义分支。新旧互联会反序列化失败——这是有意接受的代价，换取干净的数据模型。

### 决策 3：专门的 `origin` 列，不复用 `policy_action`/`policy_reason`

调研初稿曾建议把 origin 塞进 `policy: Some(("mcp_send", ...))` 元组——**否决**：`policy_action`/`policy_reason` 是**接收策略决策**的语义（auto_accept / require_confirmation / reject），与"发起来源"正交，混用会污染两个概念。`transfer_sessions` 新增**独立 `origin` 列**（存 `TransferOrigin` 的序列化值），发送端会话与接收端会话各自记录自己的 origin。

### 决策 4：只设一道发送端闸门，接收端只展示

- **发送端门控（真正的安全控制）**：`send_offer` 在 `origin` 为 `Mcp` 时校验目标设备的 `allow_mcp_send_to_device`（`device.rs` 的 `DeviceReceivePolicy`，字段名即"允许 MCP 发到该设备"；默认 Owned=true、其余=false）。被拒直接 `Err`，`send_files` 回传清晰错误。这是可强制执行的控制——控制 agent 能不能往外发。
- **接收端只展示、不 gate**：origin 是发送方自报、可伪造，接收端再基于它拦截属于冗余且不可靠的复杂度。接收端只把 origin 渲染到 offer 对话框、并据它派生 inbox `source_kind`。

> 即调研里"接收端 `evaluate_receive_policy` 加 `from_mcp` 检查"那条**不采纳**，以保持简洁。

### 决策 5：MCP 客户端身份从握手捕获，best-effort

`McpHandler`（`src-tauri/src/mcp.rs`）每会话创建。实现 `ServerHandler::initialize`（或读取 rmcp 请求上下文的 peer info）捕获 client `Implementation { name, version }`，供 `send_files` 构造 `Mcp { client: Some(name) }`。若 rmcp 版本不便在 handler 存储该信息，则退化为 `Mcp { client: None }`——不阻塞主链路（client 名是锦上添花，origin 的"是不是 MCP"才是关键）。

### 决策 6：`source_kind` 由会话 origin 派生

接收完成落 inbox 的唯一收口 `ensure_inbox_item_for_completed_receive_session`（`inbox.rs`）按接收会话的 `origin` 派生：`Mcp{..}` → `InboxSourceKind::Mcp`，否则 `PairedDevice`。`InboxSourceKind::Mcp` 死变体由此接通。

## 数据流（端到端）

```
发送端: start_send(Human) / send_files(Mcp{client})
          → send_offer(origin)  ──[origin==Mcp 时校验 allow_mcp_send_to_device]
          → create_session(origin) 写 transfer_sessions.origin（发送侧审计）
          → wire: Offer { …, origin }
接收端: incoming Offer.origin
          → TransferOfferEvent { …, origin } → Tauri 事件 → 前端 offer 对话框（标注 AI 代理）
          → create_session(origin) 写接收侧 transfer_sessions.origin
          → 接收完成 ensure_inbox_item: source_kind = match origin
          → inbox UI 显示来源标记
移动端: 同一 core → mobile-core wrapper → RN offer 对话框 + inbox 展示
```

## Open Questions

- rmcp 2.0 捕获 client `Implementation` 的确切钩子（`initialize` 覆写并存入 handler，还是工具调用时从 `RequestContext` 的 peer info 读取）——实现期确认；拿不到就 `client: None`。
- `origin` 列的存储形态（紧凑字符串 `"human"`/`"mcp:claude-desktop"` vs JSON）——实现期定，倾向紧凑字符串 + core 内 `TransferOrigin <-> String` 转换，避免 entity 依赖 serde_json。
