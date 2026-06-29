## Why

v0.6.0 审查发现:传输数据面切到 data-channel 推送模型后,旧的拉取式请求-响应协议(ChunkRequest/Complete 及其响应)已无任何发送方,但协议变体、sender 应答、incoming 路由仍成片留存,误导维护者并维持着一套不再使用的 trait 表面积。同时,进入 `suspended/recoverable` 的接收会话没有过期回收策略:用户若一直不恢复,这些会话连同其 `.part` 临时文件会无限期留存,活动列表越积越多。

## What Changes

- **删除已死的拉取式请求-响应协议**(data-channel 完成已确认走 `Finish` 帧,不依赖以下任何一项):
  - `TransferRequest::{ChunkRequest, Complete}` 与 `TransferResponse::{Chunk, ChunkError, Ack}` 变体
  - `SendSession::{handle_chunk_request, handle_complete}`、`receive.rs` 的 `handle_chunk_request_impl`/`handle_complete_impl`
  - `incoming.rs` 中对应的路由分支与 `IncomingTransferRuntime` trait 方法
- **删除 `TransferRequest::ResumeProbe.local_epoch`**:应答侧 `handle_resume_probe_impl` 完全未读取该字段。
- **评估并移除 data-channel `Ack` 帧**:接收方每块回发、发送方完全忽略,纯开销(若移除影响续传节流则保留并注明)。
- **新增:遗留 suspended 接收会话 7 天过期回收**(新能力 `stale-receive-session-expiry`):共享 core 提供清理原语,把超过保留期仍未恢复的 `recoverable suspended` 接收会话转 `terminal` 并清理其 `.part` 文件;桌面 `setup` 启动清理与移动端 `reconcile_stale_sessions` 都接入。正常断点续传(保留期内)不受影响。
- **BREAKING**(线路协议):删除上述协议变体改变 req-resp wire 格式;本项目不考虑跨版本兼容,新旧端不混用。

## Capabilities

### New Capabilities
- `stale-receive-session-expiry`: 启动清理时回收超过保留期(默认 7 天)未恢复的 recoverable suspended 接收会话,转 terminal 并清理临时文件,防止活动列表与磁盘 `.part` 无限堆积。

### Modified Capabilities
<!-- 协议删除是纯死代码移除,data-channel 可观测行为不变,无 spec 级需求变更。 -->

## Impact

- 桌面 `crates/core`:`protocol.rs`、`transfer/{sender,receive,incoming,manager,resume}.rs`、`transfer/data_frame.rs`(Ack)、新增清理原语于 `database/ops.rs` + 启动接入 `src-tauri/src/setup.rs`。
- 移动 `SwarmDrop-RN/packages/swarmdrop-core`:`reconcile_stale_sessions` 接入新清理原语;协议变体删除后 re-pin core git rev 并重 build native + 重生成 bindings。
- 回归:每步 `cargo check` + `cargo test --test e2e_transfer`;新增过期回收的单测(保留期内不回收 / 超期回收 + .part 清理)。
