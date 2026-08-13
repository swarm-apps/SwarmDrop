## ADDED Requirements

### Requirement: Resume protocol extension
系统 SHALL 扩展 TransferRequest/TransferResponse 协议，新增 ResumeRequest 和 ResumeResult 变体，支持断点续传协商。

#### Scenario: ResumeRequest message format
- **WHEN** 接收方发起断点续传
- **THEN** 系统发送 `TransferRequest::ResumeRequest { session_id, file_checksums: Vec<FileChecksum> }`，其中 FileChecksum 包含 file_id 和 BLAKE3 checksum

#### Scenario: ResumeResult accept
- **WHEN** 发送方验证所有文件校验和通过
- **THEN** 系统回复 `TransferResponse::ResumeResult { session_id, accepted: true, reason: None }`

#### Scenario: ResumeResult reject
- **WHEN** 发送方验证失败或拒绝恢复
- **THEN** 系统回复 `TransferResponse::ResumeResult { session_id, accepted: false, reason: Some(ResumeRejectReason) }`，reason 为 FileModified / SessionNotFound / SenderCancelled 之一

### Requirement: Receiver resume initiation
接收方 SHALL 能检测未完成的传输会话并发起断点续传。

#### Scenario: User triggers resume from history
- **WHEN** 用户在传输历史中点击 paused 会话的"恢复"按钮
- **THEN** 系统从 DB 读取 transfer_files 的 completed_chunks bitmap，向发送方发送 ResumeRequest

#### Scenario: Resume accepted - continue transfer
- **WHEN** 接收方收到 ResumeResult(accepted=true)
- **THEN** 系统更新 session status=transferring，仅对 bitmap 中值为 0 的 chunk 发送 ChunkRequest

#### Scenario: Resume rejected - file modified
- **WHEN** 接收方收到 ResumeResult(reason=FileModified)
- **THEN** 系统清空所有文件的 completed_chunks bitmap，删除 .part 临时文件，更新 session status=failed, error_message="源文件已被修改，无法恢复传输"

#### Scenario: Resume rejected - sender cancelled
- **WHEN** 接收方收到 ResumeResult(reason=SenderCancelled)
- **THEN** 系统清空 bitmap，删除 .part 文件，更新 session status=cancelled

#### Scenario: Resume rejected - session not found
- **WHEN** 接收方收到 ResumeResult(reason=SessionNotFound)
- **THEN** 系统清空 bitmap，删除 .part 文件，更新 session status=failed, error_message="发送方找不到对应会话"

### Requirement: Sender resume handling
发送方 SHALL 能响应接收方的 ResumeRequest，验证文件完整性后决定接受或拒绝。

#### Scenario: Sender accepts resume with valid files
- **WHEN** 发送方收到 ResumeRequest 且 DB 中存在该 session（status 非 cancelled）且所有文件 BLAKE3 校验和匹配
- **THEN** 系统更新 session status=transferring（若之前是 failed），回复 ResumeAccept，准备响应后续 ChunkRequest

#### Scenario: Sender rejects resume - file modified
- **WHEN** 发送方收到 ResumeRequest 且任意文件的 BLAKE3 校验和不匹配
- **THEN** 系统回复 ResumeReject(reason=FileModified)

#### Scenario: Sender rejects resume - session cancelled
- **WHEN** 发送方收到 ResumeRequest 且 DB 中该 session 的 status=cancelled
- **THEN** 系统回复 ResumeReject(reason=SenderCancelled)

#### Scenario: Sender rejects resume - session not found
- **WHEN** 发送方收到 ResumeRequest 且 DB 中找不到该 session_id
- **THEN** 系统回复 ResumeReject(reason=SessionNotFound)

### Requirement: Sender restart recovery
发送方 SHALL 支持在 app 重启后（DB status=failed）仍能响应接收方的 ResumeRequest。

#### Scenario: Resume after sender restart
- **WHEN** 发送方 app 重启后（session 已降级为 failed），接收方发来 ResumeRequest
- **THEN** 发送方重新读取源文件计算 BLAKE3 校验和。若匹配，更新 session status=transferring 并回复 ResumeAccept

### Requirement: Pause transfer
接收方 SHALL 支持用户主动暂停正在进行的传输。

#### Scenario: User pauses active transfer
- **WHEN** 用户点击"暂停"按钮
- **THEN** 系统停止发送 ChunkRequest，立即将内存 bitmap 刷写到 DB，更新 session status=paused, updated_at=当前时间戳，断开与发送方的传输连接

#### Scenario: Sender detects pause
- **WHEN** 发送方检测到接收方连接断开
- **THEN** 发送方运行时状态切换为 peer_paused（不写 DB），UI 显示"对方已暂停"
