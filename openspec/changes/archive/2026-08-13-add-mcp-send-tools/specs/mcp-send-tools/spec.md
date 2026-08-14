## ADDED Requirements

### Requirement: get_network_status Tool

MCP Server SHALL 提供 `get_network_status` Tool，无参数，返回 P2P 节点的运行状态。返回值 SHALL 包含 status（running/stopped）、peerId、listenAddresses、connectedPeers、discoveredPeers 字段。

#### Scenario: 节点运行中

- **WHEN** AI 调用 `get_network_status` 且 P2P 节点已启动
- **THEN** 返回 `{ status: "running", peerId: "12D3...", listenAddresses: [...], connectedPeers: 3, discoveredPeers: 5 }`

#### Scenario: 节点未启动

- **WHEN** AI 调用 `get_network_status` 且 P2P 节点未启动
- **THEN** 返回 `{ status: "stopped" }`，其他字段为空或零值。不返回 isError（这是合法查询）。

### Requirement: list_available_devices Tool

MCP Server SHALL 提供 `list_available_devices` Tool，无必填参数，返回已配对且在线的设备列表。每个设备 SHALL 包含 peerId、name、os、rttMs 字段。

#### Scenario: 有在线已配对设备

- **WHEN** AI 调用 `list_available_devices` 且存在已配对且已连接的设备
- **THEN** 返回过滤后的设备列表，仅包含 `is_paired && is_connected` 的设备

#### Scenario: 无在线已配对设备

- **WHEN** AI 调用 `list_available_devices` 且没有满足条件的设备
- **THEN** 返回空列表 `{ devices: [], total: 0 }`

#### Scenario: 节点未启动

- **WHEN** AI 调用 `list_available_devices` 且 P2P 节点未启动
- **THEN** 返回 `isError: true`，消息为 "P2P 网络节点未启动"

### Requirement: send_files Tool

MCP Server SHALL 提供 `send_files` Tool，接收 `peer_id`（字符串）和 `file_paths`（字符串数组）两个必填参数。Tool SHALL 内部完成文件枚举、BLAKE3 hash 计算、Offer 发送的完整流程，返回 `session_id` 后立即完成（fire-and-forget）。

#### Scenario: 成功发送 Offer

- **WHEN** AI 调用 `send_files` 传入有效的 peer_id 和存在的文件路径
- **THEN** 系统 SHALL 枚举文件、计算 hash、发送 Offer 到目标设备，返回 `{ sessionId: "...", filesCount: N, totalSize: M, message: "已发送传输请求，等待对方接受" }`

#### Scenario: 文件路径不存在

- **WHEN** AI 调用 `send_files` 传入的某个文件路径不存在
- **THEN** 返回 `isError: true`，消息说明哪个文件不存在

#### Scenario: peer_id 无效

- **WHEN** AI 调用 `send_files` 传入无法解析的 peer_id
- **THEN** 返回 `isError: true`，消息为 "无效的 PeerId"

#### Scenario: 目标设备不在线

- **WHEN** AI 调用 `send_files` 传入的 peer_id 对应设备不在线
- **THEN** 返回 `isError: true`，消息说明设备不在线或无法连接

#### Scenario: 节点未启动

- **WHEN** AI 调用 `send_files` 且 P2P 节点未启动
- **THEN** 返回 `isError: true`，消息为 "P2P 网络节点未启动"

#### Scenario: 支持目录路径

- **WHEN** AI 调用 `send_files` 传入的路径是一个目录
- **THEN** 系统 SHALL 递归枚举目录中的所有文件并一并发送
