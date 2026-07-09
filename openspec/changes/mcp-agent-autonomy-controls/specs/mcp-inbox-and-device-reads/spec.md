## ADDED Requirements

### Requirement: list_paired_devices MCP 工具（只读）

MCP Server SHALL 新增 `list_paired_devices` Tool，只读列出**全部**已配对设备（含离线），用于让 agent 解释设备可达性。每个设备 SHALL 至少含 peer_id、设备名、在线态、信任级别，以及策略标志 `allow_mcp_send_to_device`、`allow_mcp_accept_from_device`、`auto_accept`。该 Tool SHALL NOT 提供任何写路径（配对与策略写操作保持 app-only），SHALL 标注 `readOnlyHint`，输出 camelCase JSON。它与 `list_available_devices`（仅 Paired+Online）互补：后者发送前选目标用，前者全量解释用。

#### Scenario: 列出全部配对设备含策略标志

- **WHEN** agent 调用 `list_paired_devices`
- **THEN** 系统 SHALL 返回全部已配对设备（含离线）及其在线态、信任级别与 MCP 相关策略标志

#### Scenario: 解释设备为何不可发/不可收

- **WHEN** 某设备在线但 `allow_mcp_send_to_device` 为 false
- **THEN** 该设备 SHALL 在结果中带出该标志，使 agent 能提示用户到设备策略中开启对应权限

#### Scenario: 无任何写副作用

- **WHEN** agent 调用 `list_paired_devices`
- **THEN** 系统 SHALL NOT 改变任何配对或策略状态

### Requirement: 收件箱整理与投递 MCP 工具

MCP Server SHALL 新增 `archive_inbox_item` 与 `export_inbox_item` Tool，包裹既有收件箱能力。`archive_inbox_item(item_id, archived)` SHALL 归档 / 取消归档条目（可逆）。`export_inbox_item(item_id, destination_dir)` SHALL 把条目文件复制导出到指定目录，行为与既有命令一致。系统 SHALL NOT 提供经 MCP 删除收件箱条目的能力（`delete_inbox_item` 保持 app-only）。

#### Scenario: 归档与取消归档

- **WHEN** agent 调用 `archive_inbox_item(item_id, true)` 后再调用 `archive_inbox_item(item_id, false)`
- **THEN** 系统 SHALL 先归档该条目、随后取消归档，条目状态可逆恢复

#### Scenario: 导出条目到目标目录

- **WHEN** agent 用有效 item_id 与可写 `destination_dir` 调用 `export_inbox_item`
- **THEN** 系统 SHALL 把该条目的文件复制到目标目录

#### Scenario: 条目不存在或目标不可写

- **WHEN** item_id 不存在或 `destination_dir` 不可写
- **THEN** Tool SHALL 返回 `isError: true`，SHALL NOT 使服务崩溃

#### Scenario: 删除保持 app-only

- **WHEN** agent 尝试删除收件箱条目
- **THEN** 系统 SHALL NOT 提供该能力（`delete_inbox_item` 仅限 app 内人工操作）
