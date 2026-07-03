# Design

## 背景与目标

补齐 agent"和人一样用 app"的几处自足性/可解释性短板——**多为包裹既有命令的薄壳**，唯一带设计分量的是 `ensure_node_running`（让 agent 能替你把设备带上线）。目标：不新增业务逻辑、不碰配对/身份/破坏性删除等安全边界，只把已存在的能力**受控**开放给 agent。

## 决策

### 决策 1：拆成两个内聚能力，而非一个大杂烩

- `mcp-node-controls`：节点与接收的**生命周期**（上线、暂停接收）——agent 自足运行所需。
- `mcp-inbox-and-device-reads`：**只读设备解释** + **收件箱整理/投递**——agent 的可解释性与数据 hygiene。

两组职责不同，分能力符合项目单一职责口味，归档进 `openspec/specs/` 时边界清晰。

### 决策 2：`ensure_node_running` 由后端自取启动参数，agent 不提供

前端 `start(pairedDevices, networkOptions)` 的参数来自前端 store。MCP 场景下 agent 无法（也不该）提供这些。因此 `ensure_node_running` SHALL：

- 后端自行加载已配对设备（keychain/store）与持久化网络设置（发现模式 / 引导节点 / LAN 协助等），复用 app 冷启动同源装配逻辑；
- 已在运行 → 幂等返回当前网络状态，不重复启动；
- 未运行 → 装配参数并启动，返回启动后状态。

### 决策 3：`ensure_node_running` 门控——身份已解锁即可（不引入 preference）

让 agent 把你的设备带上线，是有"在场/存在性"含义的动作，须受控。经权衡（见下）采用**身份已解锁 + 幂等**作为门控，**不**引入独立的 `allow_mcp_start_node` preference：

- **身份已解锁门控**：启动需 keypair 已 `manage` 进 Tauri state（`register_keypair` 之后，即用户本会话已在 app 内解锁）。keypair 不在 state → 直接 `isError` 提示"请先在 SwarmDrop 中解锁"，SHALL NOT 尝试触发生物识别/密码 UI（那是 app 前台交互，MCP 不驱动）。
- **幂等**：节点已运行时直接返回当前网络状态，不重复启动。
- **为何不加 preference 门控**：app 已解锁本身就是"用户在场"的强信号；且节点上线本身低风险——真正的对外副作用（发送 / 代收）仍分别受 `allow_mcp_send_to_device` / `allow_mcp_accept_from_device` 门控。额外的 `allow_mcp_start_node` preference 需要"Rust 读 tauri-plugin-store + 设置 UI + i18n"一整条新基建、且默认关意味着没 UI 就不可用，性价比低。保留为未来增强（见 Open Questions）。

**不暴露 `shutdown`**：上线是"让 agent 能干活"，下线是用户级运维决定，价值低风险高，保持 app-only。

**启动参数**：`ensure_node_running` 复用既有 `commands::start`——它内部已从 keychain 自取已配对设备（前端传参仅作 fallback），故 agent 侧传空；`network_options=None` 走默认网络设置（用户的自定义 bootstrap / 发现模式等暂不透传，见 Open Questions）。

### 决策 4：接收暂停——只读消除盲区，写受控且可逆

- `get_receiving_paused` 包裹 `is_receiving_paused`，纯只读，标 `readOnlyHint`——让 agent 知道"是不是全局暂停了，所以我发现不到入站 offer"。
- `set_receiving_paused(paused)` 包裹既有命令，全局、可逆（暂停只是对新 offer 自动婉拒，不影响在线/配对/发现）。不标 destructive（可逆）。典型用途：agent 批处理前先静音、完事恢复。

### 决策 5：`list_paired_devices` 只读、含策略标志，零写权限

`list_available_devices` 只回 Paired+Online，agent 无法解释"设备 X 为何发不出/收不了"。新增 `list_paired_devices` 只读返回**全部**已配对设备：peer_id、设备名、在线态、信任级别，以及策略标志 `allow_mcp_send_to_device` / `allow_mcp_accept_from_device` / `auto_accept`。agent 据此可提示用户"设备 X 关闭了『允许 MCP 发送』，请在设备策略里开启"。**不含任何写路径**——配对/策略写操作保持 app-only。标 `readOnlyHint`。

### 决策 6：inbox 整理/投递复用既有薄壳，破坏性删除除外

- `archive_inbox_item(item_id, archived)` 包裹既有命令，可逆低风险，做 inbox hygiene。
- `export_inbox_item(item_id, destination_dir)` 包裹既有命令，把收到内容投递到目标目录，契合数据通道叙事。
- **`delete_inbox_item` 不暴露**：破坏性且可删磁盘文件，保持 app-only。

## Open Questions

- **`allow_mcp_start_node` preference（已决定本期不做）**：本期采用身份已解锁门控。若日后需要更保守的"显式授权 agent 上线"，再补 preference + 设置 UI + Rust 侧 store 读取（一整条垂直切片）。
- **`ensure_node_running` 的网络设置来源**：本期启动走默认 `NetworkRuntimeConfig`，不透传用户在前端 preferences store 里的自定义 bootstrap / 发现模式 / LAN 协助。若 agent 启动的节点需要尊重用户自定义网络设置，需让 Rust 侧读取该 store 或把网络设置持久化到 core 可达处——另议。
- **`export_inbox_item` 的冗余度**：agent 拿到 `get_inbox_file` 的本地路径后本可自行 `cp`，`export_inbox_item` 的增量价值主要是"整条条目一次导出 + 复用 app 的命名/去重逻辑"。若价值不足可降级或后置。
- **`list_paired_devices` 与 `list_available_devices` 是否合并**：后者是前者的"在线子集"。是否用一个带 `filter` 参数的工具统一？倾向保留两个（`list_available_devices` 语义窄、发送前用；`list_paired_devices` 全量、解释用），不破坏既有工具。
