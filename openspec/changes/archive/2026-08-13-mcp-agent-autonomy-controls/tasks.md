## 1. `ensure_node_running`（节点自足上线，`mcp-node-controls`）

- [x] 1.1 门控定为**身份已解锁 + 幂等**，不引入 `allow_mcp_start_node` preference（见 design 决策 3；preference + 设置 UI 为未来增强）
- [x] 1.2 启动参数复用既有 `commands::start`——内部已从 keychain 自取已配对设备，`network_options=None` 走默认（自定义网络设置透传留 Open Question）
- [x] 1.3 实现 `ensure_node_running` `#[tool]`：已运行 → 幂等返回网络状态（`alreadyRunning: true`）；未运行 → 调 `crate::commands::start(app, keypair, vec![], None)`
- [x] 1.4 门控：`try_state::<Keypair>()` 为 None（身份未解锁）→ `isError` 提示到 app 解锁，不触发生物识别/密码 UI
- [x] 1.5 **不**实现 `shutdown` 工具（保持 app-only）
- [x] 1.6 返回启动后 `get_network_status` 同款状态摘要（status / peerId / connectedPeers）

## 2. 接收暂停查询/开关（`mcp-node-controls`）

- [x] 2.1 `get_receiving_paused` `#[tool]` 包裹 `crate::tray::current_receiving_paused`，标 `readOnlyHint`
- [x] 2.2 `set_receiving_paused(paused)` `#[tool]` 包裹 `crate::tray::apply_receiving_paused`（不标 destructive，可逆）
- [x] 2.3 节点未启动时 `current_receiving_paused` 视为未暂停（与既有命令行为一致）

## 3. `list_paired_devices`（只读设备解释，`mcp-inbox-and-device-reads`）

- [x] 3.1 `list_paired_devices` `#[tool]`：读 `manager.devices().get_devices(DeviceFilter::Paired)`（Device 已含 status + receive_policy）
- [x] 3.2 返回 peer_id、设备名、在线态、信任级别、`allowMcpSendToDevice` / `allowMcpAcceptFromDevice` / `autoAccept` 策略标志；输出 camelCase，标 `readOnlyHint`
- [x] 3.3 无任何写路径；节点未启动时经 `get_net_manager!` 返回 `isError`（与既有工具风格一致）

## 4. 收件箱整理/投递（`mcp-inbox-and-device-reads`）

- [x] 4.1 `archive_inbox_item(item_id, archived)` `#[tool]` 包裹 core `crate::database::inbox::archive_inbox_item`（可逆）
- [x] 4.2 `export_inbox_item(item_id, destination_dir)` `#[tool]`：core `get_inbox_item_detail` + 逐文件复制到目标目录（不存在则创建）
- [x] 4.3 **不**实现 `delete_inbox_item`（保持 app-only）
- [x] 4.4 条目不存在 / 文件缺失 / 复制失败 → `isError`，不崩

## 5. 指南与发现

- [x] 5.1 更新 `src-tauri/docs/mcp-guide.md`：新增"让本机上线（ensure_node_running）""暂停/恢复接收""解释设备可达性（list_paired_devices）""整理与导出收件箱"小节
- [x] 5.2 更新 `get_info` server instructions 与工具总表（扩到 20 个），补新工具与 hint 标注
- [x] 5.3 确认 guide Resource（`include_str!`）随文档更新生效

## 6. 验证

- [x] 6.1 `cargo check -p swarmdrop` 通过 + `cargo clippy`（swarmdrop lib 0 警告）+ `cargo fmt`
- [ ] 6.2 `ensure_node_running` 运行时验证（需实机）：身份未解锁时报错；已运行时幂等；未运行 + 已解锁时能拉起节点
- [ ] 6.3 `set_receiving_paused` 运行时验证（需实机）：暂停后新入站 offer 被自动婉拒；`get_receiving_paused` 如实反映
- [ ] 6.4 `list_paired_devices` 运行时验证（需实机）：含离线设备与策略标志，且无写副作用
- [ ] 6.5 `archive_inbox_item` / `export_inbox_item` 运行时验证（需实机）：归档可逆；导出把条目文件复制到目标目录
