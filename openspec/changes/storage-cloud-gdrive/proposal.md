# Proposal

## Why

issue #132 Part 1 要让接收文件落到用户自有云存储。首期选 Google Drive，依赖
`cloud-oauth-login` 提供的账户与短期 access token。传输仍由现有 P2P 管道完成；
云存储只承担接收端的最终目的地。

原方案把 GDrive 写入、`appProperties` 查询和断点续传都交给 OpenDAL。
2026-10-01 核对上游实现发现：GDrive backend 当前使用 `OneShotWriter` 和普通
multipart 上传，写入请求不接收用户元数据，也不暴露 Drive resumable session。
因此首期 Drive adapter 直接调用 Drive API。OpenDAL 仍可服务满足实际契约的
后端与操作，但不作为所有云盘必须经过的一层。

## What Changes

- 新增 native-only `swarmdrop-storage-cloud`（`crates/storage-cloud`）。宿主侧的
  `CloudPublisher` 端口只表达「把已校验的暂存文件发布到云端、恢复发布、返回远端对象
  身份」；不模拟 POSIX 文件系统，也不把厂商 API 形状带进 `crates/core`。
- 首期 `GoogleDrivePublisher` 使用 Drive API：应用创建的 SwarmDrop 目录、按接收设备分区、
  以接收记录键做幂等、`appProperties` 写入溯源字段、resumable session 分块上传。
  账户管理器逐次提供 access token；adapter 不持有 refresh token / client_secret。
- 接收保留本地随机写暂存；单文件收齐并验签后，按有界块从暂存上传。
  Drive session URI 与暂存文件身份持久化，重试先查询远端确认进度；session
  过期时只重启云上传，不要求发送端重传已收齐的 P2P 数据。
- 将最终文件位置表示为本地 / 云端的显式类型，并使会话记录、收件箱、打开位置、
  删除与恢复逻辑理解云对象。允许修改 `crates/host`、`crates/transfer`、`crates/entity`
  的中立数据类型和编排；这些 crate 不依赖 Drive API、OpenDAL 或 native HTTP 库。
- 云接收进度分别呈现 P2P 接收与云发布；上传完成并记录远端对象 ID 后，文件才算
  完成。UI 明示传输段端到端加密、落云为明文、私有性由云 ACL 负责。
- 本期去重范围限定为**同一账户、同一接收设备、同一目标路径与内容哈希**，并以
  接收记录键消除完成响应丢失后的重复创建。多设备共用账户仍各自拥有可见文件；
  跨设备共用一份物理对象需要各云盘均可实现的引用机制，另行设计。
- 明确不做：send-from-cloud、阿里云盘 / OneDrive / OSS 实现、跨设备物理去重、
  continuous folder sync、公有分享链接、SwarmDrop 托管存储。

## Capabilities

### New Capabilities

- `cloud-storage-adapter`：云发布端口、短期 token 消费、provider 私有上传检查点、
  错误归一与运行时能力边界。首期实现为 Drive API；未来阿里云盘可用其原生 API，
  OSS 可在验证后使用 OpenDAL。
- `cloud-receive-destination`：目的地选择、暂存后发布、同一目的地的幂等、重试和
  终态、云对象位置与收件箱呈现、明文落云提示。

### Modified Capabilities

- `transfer-offer`：旧规格的 `SavePathPicker` 只允许本地目录，与新增云目的地冲突。
  本变更只修正这一条目的地选择场景；旧文档其余已经漂移的协议描述另行整理。

## Impact

- **新 crate**：`crates/storage-cloud`；Drive API HTTP client 在该 crate 的 gdrive
  模块内。OpenDAL 不作为本期 GDrive 写入依赖，也不为它虚设 workspace 版本钉。
- **中立边界**：host/transfer/entity 的目的地、最终文件位置、上传完成确认及进度
  需做小范围扩展；storage-sql 增加位置与发布检查点持久化，提供旧本地数据迁移。
- **桌面壳 / UI**：账户与目的地选择、两段进度、云位置呈现和可用动作；三语文案。
- **门禁**：`./scripts/check-wasm.sh` 保持通过。新 crate 只由桌面宿主依赖。
- **文档**：CLAUDE.md 更新模块边界；用户文档说明 BYO 与 E2E 边界。
- **依赖**：实现顺序在 `cloud-oauth-login` 之后。

## 待验证边界

- Google 的 `drive.file` scope 对应用创建的目录、`appProperties` 查询与恢复查找
  是否足够；若不足，必须先明确所需操作，再决定是否请求更广的 `drive` scope。
- Drive session URI 一周左右会过期，首期必须验证状态查询、重启恢复、完成响应丢失
  与过期重建；不得把单次上传成功误当作断点续传验收。
- 阿里云盘后续接入沿用 `CloudPublisher` 的发布语义与 `AccessTokenLease`，其 OAuth
  轮换、上传 ID/分片、元数据及幂等按阿里云盘开放平台逐项实测，不预设与 Drive 相同。
