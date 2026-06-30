# SwarmDrop MCP 使用指南

## 前置条件

1. SwarmDrop 应用已打开并且 P2P 网络节点已启动（状态为 "running"）
2. 目标设备已与本机完成配对，且当前在线

## Tool 使用顺序

### 1. 检查网络状态

调用 `get_network_status` 确认节点已启动：

- `status: "running"` → 节点正常运行，可以继续
- `status: "stopped"` → 请先在 SwarmDrop 应用中启动网络节点

### 2. 查看可用设备

调用 `list_available_devices` 获取可以发送文件的设备列表：

- 返回已配对且在线的设备
- 每个设备包含 `peer_id`（发送文件时需要）、`hostname`、`os` 等信息
- 如果列表为空，说明没有在线的已配对设备

### 3. 发送文件

调用 `send_files` 向目标设备发送文件：

- `peer_id`：从 list_available_devices 获取的目标设备 ID
- `file_paths`：要发送的文件或目录的绝对路径列表
- 支持发送单文件、多文件、目录（会自动递归遍历）
- 返回 `session_id`，传输为异步模式
- 对方需要在 SwarmDrop 应用中接受传输请求

## 智能处理流程

### 场景一：用户未指定目标设备

当用户说"发送文件"但没有指定发给谁时：

```
1. 调用 get_network_status 确认节点状态
2. 调用 list_available_devices 获取可用设备列表
3. 如果设备数量为 0 → 提示用户没有可用设备
4. 如果设备数量为 1 → 直接使用该设备
5. 如果设备数量 > 1 → 使用 ask tool 让用户选择目标设备
```

**示例对话：**

> 用户：帮我发送文件 /home/user/document.pdf
>
> AI：我发现了以下可用设备，请选择要发送的目标：
>
> - 设备 A (MacBook Pro, online)
> - 设备 B (iPhone 15, online)

### 场景二：用户提供了相对路径

当用户提供的是相对路径（如 `document.pdf`、`./file.txt`、`~/Downloads/movie.mp4`）时：

```
1. 识别路径类型（相对路径/绝对路径）
2. 如果是相对路径，根据规则转换为绝对路径：
   - `./file.txt` 或 `file.txt` → 相对于当前工作目录
   - `~/Documents/file.pdf` → 展开为用户主目录
3. 转换后再调用 send_files
```

**路径转换示例：**

| 用户输入 | 转换后 |
|---------|--------|
| `document.pdf` | `/home/user/document.pdf` |
| `./images/photo.jpg` | `/home/user/project/images/photo.jpg` |
| `~/Downloads/app.dmg` | `/Users/username/Downloads/app.dmg` |

### 场景三：完整对话流程

```
用户：把文件发给另一台电脑

AI：
1. get_network_status → 确认节点运行中
2. list_available_devices → 获取设备列表
   → 发现 2 台设备：MacBook-Pro, iPhone-15
3. 使用 ask tool："请选择目标设备：1) MacBook-Pro  2) iPhone-15"

用户：选第一个

AI：
4. 询问："请提供要发送的文件路径"

用户：~/Documents/report.pdf

AI：
5. 将 ~/Documents/report.pdf 转换为绝对路径
6. send_files(peer_id="MacBook的peer_id", file_paths=["/Users/user/Documents/report.pdf"])
7. 返回 session_id，告知用户等待对方接受
```

## 典型流程

### 标准流程（已知设备和绝对路径）

```
get_network_status
  → 确认 status: "running"

list_available_devices
  → 找到目标设备，记住 peer_id

send_files(peer_id, ["/path/to/file.pdf"])
  → 返回 session_id，等待对方接受
```

### 智能流程（未知设备或相对路径）

```
get_network_status
  → 确认 status: "running"

list_available_devices
  → 设备数量 > 1？使用 ask tool 让用户选择
  → 设备数量 = 1？直接使用
  → 设备数量 = 0？提示无可用设备

处理文件路径
  → 相对路径？转换为绝对路径
  → 绝对路径？直接使用

send_files(peer_id, [转换后的绝对路径])
  → 返回 session_id
```

## 查找已接收的文件（收件箱检索）

除了发送，MCP 还能检索本机**已接收**的文件（收件箱）。推荐顺序：先 `search_inbox` 定位条目，再 `get_inbox_file` 取本地路径。检索仅覆盖**本机已接收内容，不跨设备**。

### 1. 检索条目

调用 `search_inbox` 按关键词查找：

- `query`：关键词，支持中文（含"合同""发票"这类 2 字词）
- `limit`（可选）：返回条数上限，默认 20
- 返回命中条目：`id`（条目 id）、`title`、`sourceName`（来源设备）、`receivedAt`（接收时间）、`snippet`（匹配片段）、`files[]`（含 `name` 与 `relativePath`）
- 按接收时间倒序返回

### 2. 取文件本地路径

拿到命中后，调用 `get_inbox_file` 定位单个文件：

- `item_id`：来自 `search_inbox` 命中的 `id`
- `relative_path` 或 `file_id`：二选一，推荐用命中里的 `files[].relativePath`
- 返回 `localPath`（绝对路径）、`name`、`size`、`missing`
- 若文件已被移动/删除（`missing: true`），返回缺失标识、不返回无效路径

**典型流程：**

```
search_inbox(query="合同")
  → 命中条目，记住 id 与 files[].relativePath

get_inbox_file(item_id=<id>, relative_path=<relativePath>)
  → 返回 localPath，可据此进一步处理（如再用 send_files 转发）
```

## 注意事项

- **文件路径**：最终发送给 `send_files` 的必须是绝对路径
- **路径转换**：`~` 表示用户主目录，`.` 表示当前目录
- **设备选择**：当存在多个可用设备时，必须使用 ask tool 询问用户
- **大文件**：大文件的 hash 计算可能需要等待一段时间
- **传输结果**：对方接受/拒绝会在 SwarmDrop 应用 UI 中显示
- **网络检查**：每次发送前建议先确认网络状态和设备在线状态

## 跟踪与控制传输

发起 `send_files` 后，传输是异步的。用下面的工具跟踪与干预：

- `list_transfers` — 列出进行中与最近的传输会话（按更新时间倒序，可选 `limit`）。每个会话含 `sessionId`、`direction`（send/receive）、对端、`phase`、进度、文件数。
- `get_transfer_status` — 按 `sessionId` 查单个会话的 phase、整体进度与分文件状态。
- `cancel_transfer` — 按 `sessionId` 取消进行中的传输（会通知对端，**破坏性**操作）。
- `pause_transfer` / `resume_transfer` — 按 `sessionId` 暂停 / 恢复。恢复时若对端不在线，会话会保留为 suspended 供稍后重试。

```
send_files → 返回 sessionId
  → list_transfers / get_transfer_status 看进度
  → 需要时 pause_transfer / resume_transfer / cancel_transfer
```

## 可用的 12 个 Tool

| Tool                     | 作用                       | 前置条件                 |
| ------------------------ | -------------------------- | ------------------------ |
| `get_network_status`     | 获取 P2P 节点运行状态      | 无                       |
| `list_available_devices` | 列出已配对且在线的设备     | 节点运行中               |
| `send_files`             | 向指定设备发送文件         | 节点运行中，目标设备在线 |
| `list_transfers`         | 列出进行中/最近的传输会话  | 无                       |
| `get_transfer_status`    | 查单个传输会话的状态       | 无                       |
| `cancel_transfer`        | 取消进行中的传输（破坏性） | 节点运行中               |
| `pause_transfer`         | 暂停进行中的传输           | 节点运行中               |
| `resume_transfer`        | 恢复已暂停的传输           | 节点运行中               |
| `search_inbox`           | 按关键词检索本机收件箱     | 无                       |
| `list_inbox`             | 列出收件箱条目（无需关键词） | 无                       |
| `get_inbox_item`         | 取某收件箱条目的完整详情   | 先检索/列举定位          |
| `get_inbox_file`         | 取收件箱内某文件的本地路径 | 先 `search_inbox` 定位   |

## 安全说明

- MCP Server 仅监听本地（127.0.0.1），不接受外部连接
- 文件传输使用端到端加密（XChaCha20-Poly1305）
- 传输请求需要对方在 SwarmDrop 应用中手动接受
