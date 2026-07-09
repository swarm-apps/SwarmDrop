## Why

在接收决策权（见 `mcp-receive-decision`）之外，让 agent 拥有"和人一样用 app"的体验还差几处**自足性与可解释性**的短板。这些不涉及新的信任模型、多为包裹既有命令的薄壳，但缺了它们 agent 会撞上硬墙或陷入盲区：

1. **节点自足上线（A2）**：节点未启动时，MCP 所有网络/发送/传输/接收工具**直接失败**——这是 agent 遇到的第一堵墙，必须人工去 app 点"启动节点"。与"agent 自足数据通道"定位直接冲突。
2. **接收暂停可见/可控（A3）**：`set_receiving_paused` / `is_receiving_paused` 已存在于 app，但 agent 看不到——它不知道为何自己发现不到入站 offer（可能全局暂停了），也无法在批量处理前先"静音"。
3. **配对设备可解释（A4）**：`list_available_devices` 只回 Paired+Online，agent 无法解释"某设备为何发不出去 / 收不了"（离线、或 `allow_mcp_send_to_device` / `allow_mcp_accept_from_device` 关着）。缺一个**只读**的全量配对设备视图。
4. **收件箱整理与投递（A5）**：agent 能读 inbox 却不能归档、不能把收到的内容导出到指定目录——数据通道叙事缺"整理 + 投递"的最后一步。

补齐后，agent 无需人工前置即可上线工作、能解释自己的能力边界、能整理与投递收件箱内容。全部复用既有 Tauri 命令 / core，不新增业务逻辑，不触碰配对/身份/破坏性删除等安全边界。

## What Changes

- **`mcp-node-controls` 新能力**：
  - `ensure_node_running` — 节点未启动时**受控**启动 P2P 节点（自取已配对设备作为启动参数，agent 无需/不能提供；网络设置走默认）；门控为**身份已解锁**（keypair 已就绪于运行时）+ 幂等（已运行则返回状态）；**不暴露 `shutdown`**（让设备下线是用户级运维决定）。
  - `get_receiving_paused` / `set_receiving_paused` — 只读查询 + 受控开关全局"暂停接收"，包裹既有 `is_receiving_paused` / `set_receiving_paused`。
- **`mcp-inbox-and-device-reads` 新能力**：
  - `list_paired_devices` — **只读**列出全部已配对设备（含在线态、信任级别，以及 `allow_mcp_send_to_device` / `allow_mcp_accept_from_device` / `auto_accept` 等策略标志），让 agent 解释"为何发不出/收不了"。**不含任何写权限**。
  - `archive_inbox_item` — 归档 / 取消归档收件箱条目，包裹既有 `archive_inbox_item`（可逆、低风险）。
  - `export_inbox_item` — 把收件箱条目文件复制导出到指定目录，包裹既有 `export_inbox_item`。
- **更新 `swarmdrop://guide`**：补充节点自足上线、接收暂停、设备解释、inbox 整理的用途与顺序。

## Capabilities

### New Capabilities

- `mcp-node-controls`: Agent 的节点与接收生命周期自足工具（受控 `ensure_node_running` + 接收暂停查询/开关）。
- `mcp-inbox-and-device-reads`: Agent 的只读配对设备视图（含策略标志）+ 收件箱整理/投递工具（归档、导出）。

## Impact

- **后端代码**：`src-tauri/src/mcp/tools.rs` 新增若干 `#[tool]`，包裹既有 `commands/lifecycle.rs`（`start`）、`commands/transfer.rs`（`set_receiving_paused` / `is_receiving_paused`）、`commands/pairing.rs`（配对设备读取）、`commands/inbox.rs`（`archive_inbox_item` / `export_inbox_item`）与 core。无新增业务逻辑。
- **`ensure_node_running` 的启动参数**：agent 不提供 `pairedDevices` / `networkOptions`；后端复用既有 `commands::start`——它内部已从 keychain 自取已配对设备，`network_options=None` 走默认网络设置。
- **门控**：`ensure_node_running` 以**设备身份已解锁**（keypair 已 manage 进运行时状态）为门控 + 幂等；身份未就绪时如实 `isError` 提示到 app 解锁。不引入独立 preference（见 design 决策 3）。`set_receiving_paused` 是全局接收行为、可逆，直接包裹。
- **Rust 依赖**：无新增 crate。
- **共享 core**：MCP 工具桌面专属；被包裹的命令/能力已存在，不改 `crates/core` 业务逻辑。
- **明确不在本期范围（Non-goals）**：`shutdown` / `quit_app`（让设备下线/退出，app-only）；配对写操作（生成码/发起/审批/解绑/改策略，security-gated）；`delete_inbox_item` / `delete_transfer_session` / `clear_transfer_history`（破坏性删除，app-only）；`install_update`、identity/keypair、MCP 自身启停、设备名/偏好设置（app 管理面）；`open_inbox_item` / `show_inbox_item_in_folder`（桌面 GUI 动作，agent 已有本地路径）。
