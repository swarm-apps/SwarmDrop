## ADDED Requirements

### Requirement: Query transfer history
系统 SHALL 提供 Tauri command 查询传输历史记录列表。

#### Scenario: Load all history on startup
- **WHEN** 前端应用启动时调用 `get_transfer_history` command
- **THEN** 系统从数据库查询所有 transfer_sessions（按 started_at 降序），关联查询 transfer_files，返回完整历史列表

#### Scenario: Filter history by status
- **WHEN** 前端调用 `get_transfer_history` 并传入 status 过滤参数（如 "completed"）
- **THEN** 系统仅返回匹配该 status 的 session 记录

### Requirement: Get transfer session detail
系统 SHALL 提供 Tauri command 查询单个传输会话的详细信息。

#### Scenario: Query session with files
- **WHEN** 前端调用 `get_transfer_session` 并传入 session_id
- **THEN** 系统返回该 session 的完整信息，包括关联的所有 transfer_files

#### Scenario: Session not found
- **WHEN** 前端调用 `get_transfer_session` 传入不存在的 session_id
- **THEN** 系统返回错误

### Requirement: Delete transfer history
系统 SHALL 提供 Tauri command 删除传输历史记录。

#### Scenario: Delete single session
- **WHEN** 前端调用 `delete_transfer_session` 传入 session_id
- **THEN** 系统删除该 session 及其关联的所有 transfer_files 记录。若 session 处于 paused 状态，还需清理对应 .part 临时文件

#### Scenario: Clear all history
- **WHEN** 前端调用 `clear_transfer_history`
- **THEN** 系统删除所有 transfer_sessions 和 transfer_files 记录。清理所有 paused 会话的 .part 临时文件

### Requirement: Frontend history integration
前端 transfer-store SHALL 在启动时从数据库加载传输历史，并在传输过程中保持与后端状态同步。

#### Scenario: Load history on app startup
- **WHEN** transfer-store 初始化（setupTransferListeners）
- **THEN** 调用 `get_transfer_history` command 加载历史到 store 的 history 数组

#### Scenario: Sync active transfer to history
- **WHEN** 活跃传输完成/失败/取消
- **THEN** 前端通过 transfer-complete/transfer-failed 事件更新 store 中的 history 对应条目

#### Scenario: Paused session shows in history
- **WHEN** 用户查看传输历史列表
- **THEN** paused 状态的会话显示"恢复"按钮，用户可点击触发断点续传

### Requirement: Resume transfer from frontend
前端 SHALL 提供从传输历史中恢复暂停传输的能力。

#### Scenario: Resume paused transfer
- **WHEN** 用户在历史列表中点击 paused 会话的"恢复"按钮
- **THEN** 前端调用 `resume_transfer` Tauri command，后端发起 ResumeRequest 协议握手

#### Scenario: Resume target offline
- **WHEN** 用户尝试恢复传输但对端不在线
- **THEN** 系统返回错误提示"对端设备不在线"
