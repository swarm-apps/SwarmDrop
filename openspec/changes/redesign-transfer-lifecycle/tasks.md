## 1. 前置条件与模型设计

- [ ] 1.1 确认 `add-p2p-data-channel` 已完成或至少提供可用的 DataChannel API mock
- [ ] 1.2 定义新的 transfer phase、suspended reason、terminal reason、recoverable、epoch 等实体枚举
- [ ] 1.3 设计 transfer projection DTO，覆盖列表页、详情页、进度和操作按钮所需字段
- [ ] 1.4 设计旧状态迁移策略：开发期清理旧历史或提供一次性 migration

## 2. 数据库与持久化

- [ ] 2.1 更新 `crates/entity` 中 transfer session/file 模型，加入 phase、reason、epoch、recoverable、source fingerprint 等字段
- [ ] 2.2 新增或更新 migration，完成旧 schema 到新生命周期 schema 的迁移
- [ ] 2.3 重构 database ops，提供按 Coordinator event 写入 session、file、checkpoint 和 projection 的 repository API
- [ ] 2.4 可选新增 transfer event log，用于记录状态转换、epoch、错误和恢复协商证据
- [ ] 2.5 更新启动清理逻辑，将遗留 active session 转为 recoverable suspended，而不是 paused/failed 混用

## 3. TransferCoordinator

- [ ] 3.1 新建 `TransferCoordinator` 模块，定义用户命令、网络事件、actor 事件和状态 reducer
- [ ] 3.2 实现 actor registry，统一管理 SenderActor / ReceiverActor 的创建、替换、取消和 epoch 校验
- [ ] 3.3 将 pause、cancel、complete、fail、peer disconnected 等路径改为进入 Coordinator
- [ ] 3.4 实现前端 projection 事件发布，替换旧的分散 transfer events
- [ ] 3.5 添加旧 epoch actor event 被忽略的单元测试

## 4. 恢复协调协议

- [ ] 4.1 更新 `crates/core/src/protocol.rs`，新增 `ResumeProbe`、`ResumeStateReport`、`ResumeCommit`、`ResumeAck` 控制消息
- [ ] 4.2 移除或废弃旧 `ResumeRequest` / `ResumeOffer` 双入口恢复路径
- [ ] 4.3 实现 ResumeProbe handler，返回本端 phase、epoch、manifest、checkpoint、source fingerprint 和 terminal marker
- [ ] 4.4 实现 ResumeCommit handler，校验新 epoch、manifest、checkpoint 和 transfer key 信息
- [ ] 4.5 实现恢复拒绝原因映射：cancelled、fatal error、source modified、checkpoint invalid、peer unavailable
- [ ] 4.6 添加恢复协议测试：正常恢复、对端已取消、源文件变更、旧 epoch、checkpoint 越界

## 5. 数据面帧协议

- [ ] 5.1 定义 transfer-data frame 类型：`Hello`、`BlockRequest`、`BlockData`、`Ack`、`Abort`、`Finish`
- [ ] 5.2 实现 frame 编解码，包含长度限制、错误处理和协议版本字段
- [ ] 5.3 实现数据通道 Hello 握手，校验 session_id、epoch、role 和 manifest_digest
- [ ] 5.4 将 ReceiverActor 改为根据 checkpoint 发送缺失 range 的 `BlockRequest`
- [ ] 5.5 将 SenderActor 改为在数据通道上响应 `BlockRequest` 并发送加密 `BlockData`
- [ ] 5.6 实现 `Ack` / checkpoint event 写入路径，确保接收方中断前 flush 已完成进度
- [ ] 5.7 实现 `Abort` / `Finish` 与 Coordinator 状态转换的集成

## 6. 命令、事件与前端适配

- [ ] 6.1 重构 Tauri transfer commands：pause/resume/cancel/start 只调用 Coordinator，不直接操作 sender/receiver
- [ ] 6.2 更新 specta bindings，导出新的 projection、phase、reason、resume result 类型
- [ ] 6.3 重构 `transfer-store`，使用后端 projection 作为唯一状态来源
- [ ] 6.4 更新传输列表和详情页文案：已暂停、对方暂停、已中断、对方离线、可恢复失败、已取消、不可恢复失败
- [ ] 6.5 移除前端对 active sessions 与 DB history 的自行合并逻辑

## 7. 验证

- [ ] 7.1 添加 Coordinator reducer 单元测试，覆盖所有 phase/reason 转换
- [ ] 7.2 添加双端集成测试：正常传输、用户暂停、网络中断、应用重启、恢复成功、取消后拒绝恢复
- [ ] 7.3 添加数据面测试：旧 epoch Hello 被拒绝、数据通道中断后 checkpoint 保留、Finish 后 completed
- [ ] 7.4 运行 `cargo test` 覆盖 `crates/core`、`crates/entity`、`migration`、`src-tauri`
- [ ] 7.5 运行前端类型检查和构建，确认新 bindings 与 UI 投影一致
