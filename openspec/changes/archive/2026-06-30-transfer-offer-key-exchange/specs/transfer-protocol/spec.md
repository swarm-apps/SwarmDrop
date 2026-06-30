## ADDED Requirements

### Requirement: Transfer protocol types
`protocol.rs` 中的 `AppRequest` 和 `AppResponse` 枚举 SHALL 扩展 `Transfer` 变体，用于承载文件传输的所有请求/响应消息。

#### Scenario: AppRequest 包含 Transfer 变体
- **WHEN** 编译协议类型
- **THEN** `AppRequest` 枚举 MUST 包含 `Transfer(TransferRequest)` 变体，与现有 `Pairing(PairingRequest)` 并列

#### Scenario: AppResponse 包含 Transfer 变体
- **WHEN** 编译协议类型
- **THEN** `AppResponse` 枚举 MUST 包含 `Transfer(TransferResponse)` 变体，与现有 `Pairing(PairingResponse)` 并列

### Requirement: TransferRequest Offer 变体
`TransferRequest` 枚举 SHALL 包含 `Offer` 变体，用于发送方向接收方提出文件传输请求。

#### Scenario: Offer 包含完整文件元信息
- **WHEN** 发送方构造 `TransferRequest::Offer`
- **THEN** Offer MUST 包含以下字段：`session_id: String`、`files: Vec<FileInfo>`、`total_size: u64`

#### Scenario: FileInfo 结构体字段
- **WHEN** 定义 `FileInfo` 结构体
- **THEN** MUST 包含：`file_id: u32`、`name: String`、`relative_path: String`、`size: u64`、`checksum: String`（SHA256 hex）

### Requirement: TransferResponse OfferResult 变体
`TransferResponse` 枚举 SHALL 包含 `OfferResult` 变体，用于接收方回复 Offer 请求。

#### Scenario: 接受时携带密钥
- **WHEN** 接收方确认接收
- **THEN** `OfferResult` MUST 包含 `accepted: true` 和 `key: Some([u8; 32])`，其中 key 为接收方生成的 256-bit 随机对称密钥

#### Scenario: 拒绝时携带原因
- **WHEN** 接收方拒绝接收
- **THEN** `OfferResult` MUST 包含 `accepted: false`、`key: None`、`reason: Some(String)`

### Requirement: Transfer 变体预留分块传输结构
`TransferRequest` 和 `TransferResponse` 枚举 SHALL 预留 ChunkRequest、Chunk、Complete、Cancel、Ack 变体的位置，但本次仅实现 Offer 和 OfferResult。

#### Scenario: 编译时包含预留变体注释
- **WHEN** 查看 `TransferRequest` 和 `TransferResponse` 定义
- **THEN** MUST 以注释形式标注后续将添加的变体（ChunkRequest、Chunk、Complete、Cancel、Ack），确保枚举结构为后续扩展做好准备

### Requirement: CBOR 序列化兼容性
所有新增的 Transfer 协议类型 SHALL 实现 `Serialize` 和 `Deserialize`，使用 `serde(rename_all = "camelCase")` 命名规范，确保与现有 CBOR 编解码管线兼容。

#### Scenario: Transfer 请求可通过 libp2p request-response 发送
- **WHEN** 发送方调用 `client.send_request(peer_id, AppRequest::Transfer(offer))`
- **THEN** 请求 MUST 被 CBOR 序列化并通过 libp2p 传输，接收方 MUST 能正确反序列化
