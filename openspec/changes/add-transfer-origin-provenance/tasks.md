## 1. Core 来源模型（`TransferOrigin`）

- [x] 1.1 在 `crates/core/src/protocol.rs`（与 `Offer` 同处）定义 `TransferOrigin { Human, Mcp { client: Option<String> } }`，含 serde + `#[cfg_attr(feature="specta")]`，并提供 `to_db_string`/`from_db_string` 转换（供 DB 列存储）
- [x] 1.2 `TransferRequest::Offer` 新增**必填** `origin: TransferOrigin`
- [x] 1.3 `TransferOfferEvent` 新增 `origin: TransferOrigin`

## 2. 持久化（迁移 + 写入）

- [x] 2.1 `crates/migration` 新增迁移 `m20260630_000002_add_transfer_origin`：`transfer_sessions` 加 `origin` 列（TEXT），已注册到 `lib.rs`
- [x] 2.2 `crates/entity/src/transfer_session.rs` Model 加 `origin: Option<String>` 字段
- [x] 2.3 `CreateSessionInput` 新增 `origin: Option<TransferOrigin>`；`create_session` 经 `set_origin(o.to_db_string())` 写入（**独立于** `policy_action`/`policy_reason`）

## 3. 发送端：标注来源 + 门控

- [x] 3.1 `send_offer` 新增 `origin: TransferOrigin` 参数，下传到 `create_session(origin)` 与 wire `Offer { origin }`
- [x] 3.2 发送端门控：在 MCP 工具层 `send_files` 前置校验 `manager.pairing().get_paired_device(peer).receive_policy.allow_mcp_send_to_device`，为 false 直接返回清晰错误（放在 MCP 边界——core `send_offer` 无设备策略访问；比塞进 core 更干净）
- [x] 3.3 `src-tauri/src/commands/transfer.rs` 的 `start_send` 传 `origin = Human`
- [x] 3.4 `src-tauri/src/mcp/tools.rs` 的 `send_files` 传 `origin = Mcp { client: None }`（client 名捕获见第 4 组，暂 None）

## 4. MCP 客户端身份捕获

- [x] 4.1 客户端身份捕获：`send_files` 注入 `rmcp::service::RequestContext<RoleServer>` 参数，从 `context.peer.peer_info().client_info.name` 取连接客户端名（rmcp 默认 `initialize` 已把握手 client_info 存到 Peer 上，无需自定义 handler 状态——更干净）；拿不到则 `None`
- [x] 4.2 `send_files` 据此构造 `Mcp { client }`（不再硬编码 None）

## 5. 接收端：透传 origin

- [x] 5.1 `transfer/incoming.rs` 入站处理从 `Offer.origin` 取出，透传 `cache_inbound_offer` / `record_rejected_inbound_offer`（trait + manager 委托同步加参）并赋值到 `TransferOfferEvent`
- [x] 5.2 接收侧 `create_offered_inbound_session`（`transfer/flow/receive.rs`）把 origin 写入接收会话的 `origin` 列

## 6. inbox 来源派生与展示

- [x] 6.1 `ensure_inbox_item_for_completed_receive_session`（`crates/core/src/database/inbox.rs`）按接收会话 `origin` 派生 `source_kind`：`Mcp{..}` → `InboxSourceKind::Mcp`，否则 `PairedDevice`（接通死变体）
- [x] 6.2 桌面 inbox UI（`src/routes/_app/inbox/index.lazy.tsx`）：列表项对 `sourceKind==="mcp"` 显示「AI 代理」徽章；详情「来源类型」标签改为「AI 代理 (MCP)」

## 7. 桌面前端：offer 对话框

- [x] 7.1 `src/components/transfer/transfer-offer-dialog.tsx` 在设备名下方展示 origin：`mcp` 时显示 Bot 图标 + 「由 AI 代理发起（client）」徽章，`human` 不额外标注
- [x] 7.2 `src/lib/bindings.ts` 已补 `TransferOrigin` 类型 + `TransferOfferEvent.origin`（手动临时 patch 以过 tsc，下次 `pnpm tauri dev` 由 specta 自动重生成等价类型覆盖）。tsc 0 错误

## 8. 移动端（SwarmDrop-RN 仓库）

- [ ] 8.1 `mobile-core` 重建（bump core rev + 重编原生产物 + `ubrn:fix` + `bob prepare`），使 `MobileTransferOffer` 带 `origin`
- [ ] 8.2 `src/components/transfer-offer-host.tsx` offer 对话框展示"AI 代理发起"标记
- [ ] 8.3 inbox 展示 `source_kind=Mcp` 标记

## 9. 验证

- [x] 9.1 `cargo check -p swarmdrop-core --features specta --tests` + `cargo check -p swarmdrop --tests` 通过；`cargo clippy` 改动文件 0 新警告；`cargo fmt` 干净
- [x] 9.2 单测：`TransferOrigin <-> String` 往返（protocol.rs）；`source_kind_for_origin` 由 origin 正确派生（inbox.rs，已把内联 match 抽成可测纯函数）。3 测全过
- [x] 9.3 e2e（`crates/core/tests/e2e_transfer.rs`）：`e2e_mcp_origin_lands_as_mcp_inbox_source_kind` —— 两节点真实传输，Mcp origin 经 wire→接收会话→inbox 正确落 `source_kind=Mcp`，已通过。门控（`allow_mcp_send_to_device`）在 MCP 工具层（src-tauri），非 core e2e 可达；其行为由实现 + 设备策略默认值保证，留待真机冒烟（9.4）覆盖
- [ ] 9.4 手动冒烟：MCP `send_files` → 对端 offer 对话框出现"AI 代理发起"；接收后 inbox 显示来源标记；目标设备关闭 MCP 允许后 `send_files` 返回明确错误
