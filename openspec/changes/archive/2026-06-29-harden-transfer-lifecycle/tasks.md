## 1. 删除拉取式 req-resp 协议处理方(桌面 core)

- [x] 1.1 删 `sender.rs` 的 `handle_chunk_request`、`handle_complete`
- [x] 1.2 删 `receive.rs` 的 `handle_chunk_request_impl`、`handle_complete_impl`
- [x] 1.3 删 `incoming.rs` 中 `ChunkRequest`/`Complete` 路由分支与 `IncomingTransferRuntime` 对应 trait 方法，删 `manager.rs` 对应 impl（连带删 `TransferCompleteOutcome` 结构体）
- [x] 1.4 `cargo check -p swarmdrop-core`，按编译器提示清理残留引用/导入（sender 清 `info`/`TransferResponse`/`calc_total_chunks`，receive 清 `TransferCompleteOutcome`/`TransferDbErrorEvent`）

## 2. 删除协议变体与死字段(桌面 core)

- [x] 2.1 删 `protocol.rs` 的 `TransferRequest::{ChunkRequest, Complete}`、`TransferResponse::{Chunk, ChunkError}`。**偏离**：`TransferResponse::Ack` 保留——它仍被存活的 `Cancel`/`Pause` 应答使用，非死代码（仅 `Complete` 的 ack 用法随 `Complete` 删除）
- [x] 2.2 删 `TransferRequest::ResumeProbe.local_epoch`，同步 `request_resume_probe`/`handle_resume_probe_impl` 签名与两处调用方
- [x] 2.3 移除 data-channel `TransferDataFrame::Ack`：接收方 `handle_block_data` 不再回发（连带删去其 unused `frame_tx`/`epoch` 参数），发送方 reader 删该 match 臂（并把已退化为单帧的 `loop` 改为 cancel-aware 单次读取，修 `never_loop`），`data_frame.rs` 编解码 + `TAG_ACK` 删除
- [x] 2.4 `cargo check` + `cargo test -p swarmdrop-core --lib transfer::`(54 通过) + `cargo test --test e2e_transfer`(7 通过) + clippy 干净

## 3. 过期回收清理原语(共享 core)

- [x] 3.1 在 `database/ops.rs` 新增 `reap_expired_suspended_receives(db, retention_secs)`：查 recoverable suspended + direction=Receive + updated_at 超期的会话，转 terminal，返回 `ExpiredReceiveSession`（含已带 `save_dir` 的 `HostFileMetadata` 清单）供宿主清 `.part`。配套 `transfer::cleanup_expired_part_files` 共享助手（两端复用，`open_or_create_sink`+`cleanup_sink` 尽力删除）
- [x] 3.2 定义保留期常量 `SUSPENDED_RECEIVE_RETENTION_SECS = 604800`（transfer/mod.rs）。终态 reason 复用 `FatalError`（带"超过 N 天未恢复，已过期回收"说明），避免 entity/migration/镜像/bindings churn
- [x] 3.3 单测 `reaps_only_expired_recoverable_receives`：保留期内不回收 / 超期回收转 terminal+不可恢复 / 发送会话与 terminal 不受影响（通过）

## 4. 两端启动接入

- [x] 4.1 桌面 `src-tauri/src/database.rs::cleanup_stale_sessions`：删掉原 inline 7 天过期实现，改调共享 core 原语；对返回的文件按真实路径 `compute_part_path` 尽力删 `.part`（失败仅告警）。`.part` 删除 + app-restart 两个既有测试仍绿
- [x] 4.2 移动 `mobile-core/src/history.rs::reconcile_stale_sessions`：coordinator 清理之后调用同一原语 + 本端 `FileAccess` 经 `cleanup_expired_part_files` 清理；`network.rs` 调用点传入 `file_access`。本地 path 联编 `cargo check` 通过
- [~] 4.3 真机验证（造 8 天前 suspended 接收会话，重启后回收且 `.part` 删除；3 天前不受影响）——**推迟**：逻辑已由 core 单测 + 桌面 `.part` 删除测试覆盖；真机端到端验证待后续手动跑

## 5. 移动端 re-sync 与 bindings

- [~] 5.1 mobile-core `Cargo.toml` re-pin core git rev → 含本 change 的桌面 commit——**待 push 桌面 develop 后执行**（本轮已用 path 依赖临时联编验证）
- [x] 5.2 mobile-core `cargo check` 通过（本地 path 联编）。**uniffi 导出面未变（`reconcile_stale_sessions` 为 `pub(crate)`，无新增/改动的导出方法），故无需重 build native / 重生成 cpp+src bindings**
- [~] 5.3 `pnpm typecheck` + `pnpm lint:ci` 两端——**待 re-pin 后执行**（mobile-core 改动不触及 TS 接口，预期无影响）
