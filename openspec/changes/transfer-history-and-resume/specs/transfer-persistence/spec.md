## ADDED Requirements

### Requirement: Database initialization
系统 SHALL 在 Tauri setup() 阶段初始化 SeaORM DatabaseConnection 连接到 SQLite，执行所有待处理的 migration，并将 DatabaseConnection 注入为 Tauri managed state。

#### Scenario: First app launch creates database
- **WHEN** 应用首次启动
- **THEN** 系统在 app data 目录创建 SQLite 数据库文件，执行 migration 创建 `transfer_sessions` 和 `transfer_files` 表

#### Scenario: Database available before any command
- **WHEN** 任何 Tauri command handler 执行时
- **THEN** 系统 MUST 能通过 `tauri::State<DatabaseConnection>` 获取到已初始化的数据库连接

### Requirement: Transfer session record creation
系统 SHALL 在 Offer 被接受时创建传输会话记录和关联的文件记录。

#### Scenario: Receiver accepts offer
- **WHEN** 接收方用户接受 Offer
- **THEN** 系统创建 `transfer_sessions` 记录（status=transferring, direction=receive）和所有 `transfer_files` 记录（含 total_chunks 和全零 completed_chunks bitmap）

#### Scenario: Sender offer accepted
- **WHEN** 发送方收到 OfferResult(accepted=true)
- **THEN** 系统创建 `transfer_sessions` 记录（status=transferring, direction=send）和所有 `transfer_files` 记录（completed_chunks 为空 vec）

#### Scenario: Rejected offer not recorded
- **WHEN** 接收方拒绝 Offer 或 Offer 超时
- **THEN** 系统 SHALL NOT 创建任何数据库记录

### Requirement: Bitmap checkpoint persistence
接收方 SHALL 周期性将 completed_chunks bitmap 刷写到数据库，以支持断点续传。

#### Scenario: Periodic bitmap flush during transfer
- **WHEN** 接收方每接收 10 个 chunk
- **THEN** 系统执行 UPDATE 将内存中的 bitmap 和 transferred_bytes 写入对应 `transfer_files` 行，同时更新 session 的 `updated_at`

#### Scenario: Immediate flush on pause
- **WHEN** 用户主动暂停传输或网络断开
- **THEN** 系统 MUST 立即将内存中的 bitmap 刷写到数据库，不等待 10 chunk 周期

#### Scenario: Immediate flush on cancel
- **WHEN** 传输被取消
- **THEN** 系统清空 bitmap（全零）并更新文件 status

### Requirement: Transfer completion persistence
系统 SHALL 在传输完成时更新数据库记录。

#### Scenario: Successful transfer completion
- **WHEN** 所有文件传输完成且校验通过
- **THEN** 系统更新 session status=completed、finished_at=当前时间戳，所有文件 status=completed

#### Scenario: Transfer failure
- **WHEN** 传输因不可恢复错误失败
- **THEN** 系统更新 session status=failed、error_message=错误描述、finished_at=当前时间戳。接收方清空 bitmap 并删除 .part 临时文件

#### Scenario: Transfer cancellation
- **WHEN** 任一方取消传输
- **THEN** 双方更新 session status=cancelled、finished_at=当前时间戳。接收方清空 bitmap 并删除 .part 临时文件

### Requirement: Startup session cleanup
系统 SHALL 在应用启动时清理处于不一致状态的传输记录。

#### Scenario: Sender transferring sessions on restart
- **WHEN** 应用启动时发现 direction=send, status=transferring 的记录
- **THEN** 系统批量更新这些记录为 status=failed, error_message="应用重启，连接已断"

#### Scenario: Receiver transferring sessions with progress
- **WHEN** 应用启动时发现 direction=receive, status=transferring 的记录，且存在任何文件的 completed_chunks 非全零
- **THEN** 系统更新该 session 为 status=paused

#### Scenario: Receiver transferring sessions without progress
- **WHEN** 应用启动时发现 direction=receive, status=transferring 的记录，且所有文件的 completed_chunks 全零
- **THEN** 系统更新该 session 为 status=failed

#### Scenario: Receiver transferring sessions fully completed
- **WHEN** 应用启动时发现 direction=receive, status=transferring 的记录，且所有文件 status=completed
- **THEN** 系统更新该 session 为 status=completed

#### Scenario: Expired paused sessions
- **WHEN** 应用启动时发现 direction=receive, status=paused 且 updated_at 超过 7 天的记录
- **THEN** 系统清空对应文件的 completed_chunks bitmap，删除 .part 临时文件，更新 session status=failed, error_message="传输已过期（超过 7 天）"
