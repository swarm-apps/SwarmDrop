## 1. 修复发现层缺陷（战略"第一步"，零核心改动）

- [x] 1.1 `src-tauri/src/mcp.rs` 的 `get_info().with_instructions` 从只述发送三件套，补全到枚举完整工具集（含 `search_inbox` / `get_inbox_file`；本期新增传输工具落地后再追加进 instructions）
- [x] 1.2 为现有 5 个工具补 `readOnlyHint` / `destructiveHint` 标注（只读工具标 readOnly；`send_files` 标 `read_only_hint=false, open_world_hint=true`）

## 2. 扩展检索面（`mcp-find-tools` 修改）

- [x] 2.1 `src-tauri/src/mcp/tools.rs` 的 `search_inbox` 把 `include_archived` 从硬编码 `false` 改为可选参数透传给 core `search_inbox`（默认 false）
- [x] 2.2 更新 `search_inbox` 工具 description，说明 `include_archived` 语义

## 3. 新增传输生命周期只读工具（`mcp-transfer-tools`）

- [x] 3.1 实现 `list_transfers`：包裹 core `get_transfer_projections`，返回会话列表（session_id / direction / 对端名 / phase / reason / 进度 / 文件摘要），支持可选 `limit`（按 updated_at 倒序），标 `readOnlyHint`
- [x] 3.2 实现 `get_transfer_status`：按 `session_id` 包裹 core `get_transfer_projection` 返回单会话详情（phase / 进度 / 分文件状态），不存在时 `isError`，标 `readOnlyHint`
- [x] 3.3 返回结构裁剪：`McpTransfer` 不暴露 `epoch` / bitmap / `save_path` 等内部字段，输出 camelCase JSON

## 4. 新增传输控制工具（`mcp-transfer-tools`）

- [x] 4.1 实现 `cancel_transfer`：包裹既有 `cancel_send` / `cancel_receive`（通知对端 + 经 Coordinator 写 Cancelled），标 `destructiveHint`
- [x] 4.2 实现 `pause_transfer`：包裹既有 `pause_send` / `pause_receive`
- [x] 4.3 实现 `resume_transfer`：包裹既有 `initiate_resume` 单入口（走 Probe→Commit→Ack，不新增分支）；对端不可用时如实回传 suspended 结果、不报硬错

## 5. 新增收件箱列举 / 详情工具（`mcp-transfer-tools`）

- [x] 5.1 实现 `list_inbox`：包裹 core `list_inbox_items` 按接收时间倒序列出条目，支持 `limit` 与可选 `include_archived`，标 `readOnlyHint`
- [x] 5.2 实现 `get_inbox_item`：按条目 id 包裹 core `get_inbox_item_detail`，返回完整详情（含文件列表 relativePath / size / missing / localPath），补全 search→detail→get_inbox_file 闭环，标 `readOnlyHint`

## 6. 指南 Resource 与文档

- [x] 6.1 更新 `src-tauri/docs/mcp-guide.md`：新增"跟踪与控制传输"小节 + 工具总表扩到 12 个（含调用顺序）
- [x] 6.2 确认 guide Resource（`include_str!` 嵌入）随文档更新生效（编译期嵌入，无需额外改动）

## 7. 验证

- [x] 7.1 `cargo check -p swarmdrop` 编译通过（clippy mcp 模块 0 警告，cargo fmt 已格式化）
- [ ] 7.2 启动 MCP Server，`tools/list` 确认全部工具已注册且带正确 hint 标注
- [ ] 7.3 `get_info` 返回的 instructions 能看到全部工具（发现层修复生效）
- [ ] 7.4 端到端冒烟：用支持本地 MCP 的客户端走通"发送→list_transfers→get_transfer_status→cancel/pause/resume"与"search_inbox/list_inbox→get_inbox_item→get_inbox_file"两条链路
