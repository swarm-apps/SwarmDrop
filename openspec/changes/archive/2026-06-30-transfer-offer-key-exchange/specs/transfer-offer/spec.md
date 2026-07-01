## ADDED Requirements

### Requirement: 发送方准备文件（prepare_send）
系统 SHALL 提供 `prepare_send` Tauri 命令，接收文件路径列表，扫描并收集文件元信息，返回 `PreparedTransfer`。

#### Scenario: 扫描文件并分配 fileId
- **WHEN** 前端调用 `prepare_send(filePaths: Vec<String>)`
- **THEN** 后端 MUST 递归扫描所有路径（文件直接收集，目录递归遍历），为每个文件分配递增的 `file_id: u32`（从 0 开始），计算 SHA256 校验和，返回 `PreparedTransfer { prepared_id, files, total_size }`

#### Scenario: prepared_id 唯一标识
- **WHEN** `prepare_send` 成功返回
- **THEN** `prepared_id` MUST 为 UUID v4 格式字符串，用于后续 `start_send` 引用

#### Scenario: SHA256 计算不阻塞异步运行时
- **WHEN** 对大文件计算 SHA256
- **THEN** MUST 在 `tokio::task::spawn_blocking` 中执行，避免阻塞 tokio runtime

#### Scenario: 空文件列表
- **WHEN** `filePaths` 为空或所有路径无效
- **THEN** MUST 返回错误

### Requirement: 发送方发起 Offer（start_send）
系统 SHALL 提供 `start_send` Tauri 命令，从 `PreparedTransfer` 中选择文件，向指定 peer 发送 Offer。

#### Scenario: 发送 Offer 并等待响应
- **WHEN** 前端调用 `start_send(preparedId, peerId, selectedFileIds)`
- **THEN** 后端 MUST 从缓存中取出 `PreparedTransfer`，筛选 `selectedFileIds` 对应的文件，生成 `session_id`，通过 `client.send_request()` 发送 `TransferRequest::Offer` 到目标 peer，返回 `session_id` 给前端

#### Scenario: Offer 被接受
- **WHEN** 接收方回复 `OfferResult { accepted: true, key: Some(key) }`
- **THEN** 后端 MUST 使用 `key` 构造 `TransferCrypto` 实例，通过 Tauri event 通知前端 Offer 已被接受

#### Scenario: Offer 被拒绝
- **WHEN** 接收方回复 `OfferResult { accepted: false, reason }`
- **THEN** 后端 MUST 通过 Tauri event 通知前端 Offer 被拒绝，包含拒绝原因

#### Scenario: Offer 超时
- **WHEN** 180 秒内未收到接收方回复
- **THEN** libp2p request-response 超时机制自动触发，后端 MUST 将错误传递给前端显示 "对方未回应"

#### Scenario: preparedId 无效
- **WHEN** `preparedId` 在缓存中不存在
- **THEN** MUST 返回错误

### Requirement: 接收方处理入站 Offer
事件循环 SHALL 识别 `AppRequest::Transfer(Offer)` 入站请求，缓存 pending 信息并通知前端。

#### Scenario: 缓存入站 Offer
- **WHEN** 事件循环收到 `InboundRequest` 且 request 为 `AppRequest::Transfer(TransferRequest::Offer { .. })`
- **THEN** MUST 将 `pending_id`、`peer_id`、`session_id`、`files` 信息缓存到 `OfferManager`

#### Scenario: 通知前端并发送系统通知
- **WHEN** 入站 Offer 缓存完成
- **THEN** MUST emit `transfer-offer` Tauri 事件，payload 包含 `sessionId`、`peerId`、`deviceName`、`files`（TransferFileInfo[]）、`totalSize`；若窗口未聚焦，MUST 发送系统通知

#### Scenario: 仅已配对设备的 Offer 被接受
- **WHEN** 收到来自未配对设备的 Offer
- **THEN** MUST 自动回复拒绝的 OfferResult，reason 为 "未配对设备"

### Requirement: 接收方确认接收（accept_receive）
系统 SHALL 提供 `accept_receive` Tauri 命令，确认接收文件并回复密钥。

#### Scenario: 生成密钥并回复 OfferResult
- **WHEN** 前端调用 `accept_receive(sessionId, savePath)`
- **THEN** 后端 MUST 调用 `generate_key()` 生成 256-bit 随机密钥，通过 `client.send_response(pending_id, TransferResponse::OfferResult { accepted: true, key: Some(key) })` 回复发送方，本地构造 `TransferCrypto` 实例并缓存

#### Scenario: sessionId 无效
- **WHEN** `sessionId` 对应的 pending offer 不存在
- **THEN** MUST 返回错误

### Requirement: 接收方拒绝接收（reject_receive）
系统 SHALL 提供 `reject_receive` Tauri 命令，拒绝文件传输。

#### Scenario: 回复拒绝的 OfferResult
- **WHEN** 前端调用 `reject_receive(sessionId)`
- **THEN** 后端 MUST 通过 `client.send_response(pending_id, TransferResponse::OfferResult { accepted: false, reason: Some("用户拒绝") })` 回复发送方，并清理 pending offer 缓存

### Requirement: 前端接收页展示文件树
接收页 SHALL 使用 `FileTree` 组件展示 Offer 中的文件列表，替换现有的 `OfferFileList` 组件。

#### Scenario: 从 TransferFileInfo 构建树数据
- **WHEN** 接收页需要展示文件列表
- **THEN** MUST 调用 `buildTreeDataFromOffer(files: TransferFileInfo[])` 将 flat list 转为 `TreeData { dataLoader, rootChildren }`，使用 `relativePath` 重建目录层级

#### Scenario: 复用 FileTree 组件
- **WHEN** 渲染文件树
- **THEN** MUST 使用 `<FileTree mode="select">` 组件，传入 `dataLoader`、`rootChildren`、`totalCount`、`totalSize`，不传 `onRemoveFile`（接收方不能删除文件）

#### Scenario: 保留保存路径选择功能
- **WHEN** 用户在接收页查看文件树
- **THEN** 保存路径选择器（SavePathPicker）MUST 保持不变，用户可更改保存目录

### Requirement: PreparedTransfer 缓存管理
后端 SHALL 使用 `DashMap<String, PreparedTransfer>` 缓存 `prepare_send` 的结果，供 `start_send` 消费。

#### Scenario: prepare_send 存入缓存
- **WHEN** `prepare_send` 成功完成
- **THEN** MUST 将结果以 `prepared_id` 为 key 存入 `DashMap`

#### Scenario: start_send 消费缓存
- **WHEN** `start_send` 被调用
- **THEN** MUST 从 `DashMap` 中移除对应的 `PreparedTransfer`（一次性消费）

### Requirement: 命令注册
所有新增的 Tauri 命令 SHALL 在 `lib.rs` 的 `invoke_handler!` 中注册。

#### Scenario: 四个命令全部可调用
- **WHEN** 前端调用 `prepare_send`、`start_send`、`accept_receive`、`reject_receive`
- **THEN** 所有命令 MUST 在 `invoke_handler!` 宏中注册，不出现 "Command not found" 错误
