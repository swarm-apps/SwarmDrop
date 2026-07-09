## 1. 接收门控字段 `allow_mcp_accept_from_device`（core + 持久化）

- [x] 1.1 在 `receive_policy` 结构（`crates/core/src/device.rs` 或策略所在处）新增布尔字段 `allow_mcp_accept_from_device`，默认 `false`；补 serde/specta 派生与默认值，确保向后兼容（旧持久化数据缺该字段时反序列化为 false）
- [x] 1.2 `apply_trust_level_defaults` / 各信任级别默认值同步该字段（默认全 false，本人设备也不默认开——代收是显式授权）
- [x] 1.3 确认 `update_paired_device_policy` Tauri 命令与 `PairedDeviceInfo` 序列化把新字段透传到前端 bindings（`pnpm` specta 生成）— bindings.ts 已补 `allowMcpAcceptFromDevice?`（`#[serde(default)]` → 可选），下次 `tauri dev` 由 specta 自动重生成确认

## 2. app 端设备策略 UI 开关

- [x] 2.1 `src/routes/_app/devices/-components/trust-policy-dialog.tsx` 加"允许 MCP/AI 代收此设备的文件"开关，绑定 `allow_mcp_accept_from_device`
- [x] 2.2 文案明确"仅本机 agent 可代为接受/拒绝该设备的入站文件；关闭则入站仍需你手动确认"
- [x] 2.3 i18n：`pnpm i18n:extract` 补齐 8 语言 catalog 的新字符串（源 locale zh 已含新文案，其余 locale 待翻译填充）

## 3. MCP 默认收件目录

- [x] 3.1 新增 MCP 默认收件目录解析（`<下载目录>/SwarmDrop`，`app.path().download_dir()`）；目录不存在则创建 —— `mcp_default_receive_dir`
- [x] 3.2 暴露一个内部 helper 供 `accept_transfer` 缺省 `save_path` 时调用，返回 `CoreSaveLocation::Path`

## 4. `accept_transfer` MCP 工具（`src-tauri/src/mcp/tools.rs`）

- [x] 4.1 定义参数 `AcceptTransferParams { session_id: String, save_path: Option<String> }`
- [x] 4.2 由 `session_id` 在 `TransferManager.pending` peek 出来源 `peer_id`（不移除）；pending 不存在 → `isError`（提示会话已被处理或已过期）—— core 新增 `pending_offer_peer` accessor
- [x] 4.3 门控：`manager.pairing().get_paired_device(peer_id).receive_policy.allow_mcp_accept_from_device == false` → `isError`，提示到设备策略开启（镜像 `send_files` 的门控写法）—— 抽成共用 `gate_pending_offer`
- [x] 4.4 解析 `save_path`：有则 `CoreSaveLocation::Path`，无则用 MCP 默认收件目录
- [x] 4.5 调用 core `accept_and_start_receive(session_id, save_location)`，成功返回 session_id / 保存位置；标 `read_only_hint=false, open_world_hint=true`
- [x] 4.6 节点未启动 / 数据库未就绪 → 明确 `isError`

## 5. `reject_transfer` MCP 工具

- [x] 5.1 定义参数 `TransferSessionParams { session_id }`（复用既有）
- [x] 5.2 同 4.2 / 4.3 的 pending 定位 + `allow_mcp_accept_from_device` 门控（共用 `gate_pending_offer`）
- [x] 5.3 调用 core `reject_and_respond(session_id)`；标 `destructiveHint`
- [x] 5.4 pending 不存在 / 已终态 → `isError`，不重复拒绝、不崩

## 6. 发现契约与 guide

- [x] 6.1 确认挂起 offer 以 `phase=Offered / direction=receive` 出现在 `list_transfers` 输出（`TransferState::offered` → `TransferPhase::Offered`，`McpTransfer` 已含 phase / direction，无需改）
- [x] 6.2 更新 `src-tauri/docs/mcp-guide.md`：新增"代收入站文件"小节——`list_transfers` 滤 `phase=Offered` 发现待审 → `accept_transfer` / `reject_transfer`；**显式写出 ~5 分钟窗口约束**与"要代收需进 watch 循环"
- [x] 6.3 更新 `get_info` server instructions 与 guide 工具总表，扩到 14 个工具，标注 accept/reject 的 hint
- [x] 6.4 确认 guide Resource（`include_str!` 嵌入）随文档更新生效

## 7. 对齐决策窗口到协议真实上限 ~180s

- [x] 7.1 定位 Offer 发送路径（`flow/send.rs` `send_offer` → `client.send_request`），确认可透传 `RequestOptions`（`send_request_with_options` 存在）
- [x] 7.2 `OFFER_RESPONSE_TIMEOUT_SECS` 取与全局协议上限一致的 180s（client 侧 `with_timeout` 无法突破 libp2p `req_resp_timeout`=180s，故不做无谓的"更长"承诺）；**不改**全局超时
- [x] 7.3 接收端 `PENDING_OFFER_TIMEOUT_SECS` 设为 170s（**小于**真实协议窗口 180s），保证 pending 先于发送端放弃被回收，消除竞态
- [x] 7.4 Offer 发送路径已支持 per-request timeout（`send_request_with_options`），无需全局提超时

## 8. 验证

- [x] 8.1 `cargo check -p swarmdrop` 通过 + `cargo clippy`（mcp 模块 / swarmdrop lib 0 警告；剩余 3 条 core `too_many_arguments` 为既有、属别的在飞 change）+ `cargo fmt`
- [x] 8.2 core 策略字段单测通过：`allow_mcp_accept_defaults_false_all_trust_levels` + `legacy_policy_without_allow_mcp_accept_deserializes_false`（含旧数据反序列化兼容）+ 前端 `tsc --noEmit` 通过
- [ ] 8.3 端到端冒烟（需双设备实机）：设备策略关闭时 `accept_transfer` 被门控拒绝；开启后能接受一个 `RequireConfirmation` offer 并落到默认收件目录 / 指定 `save_path`
- [ ] 8.4 窗口验证（需实机）：offer 在 ~180s（约 3 分钟）窗口内可被 `accept_transfer` 接受；超窗后如实回"会话已过期 / pending 不存在"，不崩
- [ ] 8.5 `reject_transfer` 能婉拒挂起 offer（需实机），对端收到 `OfferResult{accepted:false}`
