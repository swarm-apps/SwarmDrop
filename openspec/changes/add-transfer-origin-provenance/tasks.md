## 1. Core 来源模型（`TransferOrigin`）

- [ ] 1.1 在 `crates/core/src/protocol.rs`（与 `Offer` 同处）定义 `TransferOrigin { Human, Mcp { client: Option<String> } }`，含 serde + `#[cfg_attr(feature="specta")]`，并提供 `TransferOrigin <-> String` 转换（供 DB 列存储）
- [ ] 1.2 `TransferRequest::Offer`（`protocol.rs:111`）新增**必填** `origin: TransferOrigin`
- [ ] 1.3 `TransferOfferEvent`（`transfer/incoming.rs:24`）新增 `origin: TransferOrigin`

## 2. 持久化（迁移 + 写入）

- [ ] 2.1 `crates/migration` 新增迁移：`transfer_sessions` 加 `origin` 列（TEXT），注册到 `crates/migration/src/lib.rs`
- [ ] 2.2 `crates/entity/src/transfer_session.rs` Model 加 `origin` 字段
- [ ] 2.3 `CreateSessionInput`（`crates/core/src/database/ops.rs:25`）新增 `origin` 字段；`create_session` 写入 `origin` 列（**独立于** `policy_action`/`policy_reason`）

## 3. 发送端：标注来源 + 门控

- [ ] 3.1 `send_offer`（`transfer/flow/send.rs:28`）新增 `origin: TransferOrigin` 参数，下传到 `create_session(origin)` 与 wire `Offer { origin }`
- [ ] 3.2 发送端门控：`send_offer` 在 `origin` 为 `Mcp` 时查目标设备 `PairedDeviceInfo.receive_policy.allow_mcp_send_to_device`（`device.rs`），为 false 直接 `Err`（清晰文案，提示去设备策略开启）
- [ ] 3.3 `src-tauri/src/commands/transfer.rs` 的 `start_send` 传 `origin = Human`
- [ ] 3.4 `src-tauri/src/mcp/tools.rs` 的 `send_files` 传 `origin = Mcp { client }`

## 4. MCP 客户端身份捕获

- [ ] 4.1 `McpHandler`（`src-tauri/src/mcp.rs`）捕获连接客户端 `Implementation`：实现 `ServerHandler::initialize` 存入 handler，或工具调用时从 rmcp `RequestContext` 的 peer info 读取（择 rmcp 2.0 可行者；拿不到则 `client: None`）
- [ ] 4.2 `send_files` 据此构造 `Mcp { client }`

## 5. 接收端：透传 origin

- [ ] 5.1 `transfer/incoming.rs` 入站处理（`handle_incoming_transfer_request`，~176-292）从 `Offer.origin` 取出，传入 `cache_inbound_offer` 并赋值到 `TransferOfferEvent`（~271-288）
- [ ] 5.2 接收侧 `create_offered_inbound_session`（`transfer/flow/receive.rs`）把 origin 写入接收会话的 `origin` 列

## 6. inbox 来源派生与展示

- [ ] 6.1 `ensure_inbox_item_for_completed_receive_session`（`crates/core/src/database/inbox.rs`）按接收会话 `origin` 派生 `source_kind`：`Mcp{..}` → `InboxSourceKind::Mcp`，否则 `PairedDevice`
- [ ] 6.2 桌面 inbox UI（`src/routes/_app/inbox/index.lazy.tsx`）对 `source_kind=Mcp` 显示来源标记（如"🤖 AI 代理"徽章）

## 7. 桌面前端：offer 对话框

- [ ] 7.1 `src/components/transfer/transfer-offer-dialog.tsx` 在设备名附近展示 origin：`Mcp` 时显示"🤖 由 AI 代理发起（client）"，`Human` 不额外标注
- [ ] 7.2 确认 `src/lib/bindings.ts` 经 `pnpm tauri dev` 自动重生成含 `origin`（不要手改）

## 8. 移动端（SwarmDrop-RN 仓库）

- [ ] 8.1 `mobile-core` 重建（bump core rev + 重编原生产物 + `ubrn:fix` + `bob prepare`），使 `MobileTransferOffer` 带 `origin`
- [ ] 8.2 `src/components/transfer-offer-host.tsx` offer 对话框展示"AI 代理发起"标记
- [ ] 8.3 inbox 展示 `source_kind=Mcp` 标记

## 9. 验证

- [ ] 9.1 `cargo check -p swarmdrop-core --features specta` + `cargo check -p swarmdrop` 通过；`cargo clippy` 无新警告；`cargo fmt`
- [ ] 9.2 单测：`TransferOrigin <-> String` 往返；`source_kind` 由 origin 正确派生
- [ ] 9.3 e2e（`crates/core/tests/e2e_transfer.rs` 风格）：Mcp origin 的 offer 在接收会话与 inbox 正确落 `source_kind=Mcp`；`allow_mcp_send_to_device=false` 时 MCP 发送被发送端拒绝
- [ ] 9.4 手动冒烟：MCP `send_files` → 对端 offer 对话框出现"AI 代理发起"；接收后 inbox 显示来源标记；目标设备关闭 MCP 允许后 `send_files` 返回明确错误
