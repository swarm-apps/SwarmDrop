## 1. 删除拉取式 req-resp 协议处理方(桌面 core)

- [ ] 1.1 删 `sender.rs` 的 `handle_chunk_request`、`handle_complete`
- [ ] 1.2 删 `receive.rs` 的 `handle_chunk_request_impl`、`handle_complete_impl`
- [ ] 1.3 删 `incoming.rs` 中 `ChunkRequest`/`Complete` 路由分支与 `IncomingTransferRuntime` 对应 trait 方法,删 `manager.rs` 对应 impl
- [ ] 1.4 `cargo check -p swarmdrop-core`,按编译器提示清理残留引用/导入

## 2. 删除协议变体与死字段(桌面 core)

- [ ] 2.1 删 `protocol.rs` 的 `TransferRequest::{ChunkRequest, Complete}`、`TransferResponse::{Chunk, ChunkError, Ack}`
- [ ] 2.2 删 `TransferRequest::ResumeProbe.local_epoch`,同步 `request_resume_probe`/`handle_resume_probe_impl` 签名
- [ ] 2.3 移除 data-channel `TransferDataFrame::Ack`:接收方 `handle_block_data` 不再回发,发送方 reader 与 `data_frame.rs` 编解码删除该变体
- [ ] 2.4 `cargo check` + `cargo test -p swarmdrop-core --lib transfer::` + `cargo test --test e2e_transfer` 全绿

## 3. 过期回收清理原语(共享 core)

- [ ] 3.1 在 `database/ops.rs` 新增 `reap_expired_suspended_receives(db, retention_secs)`:查 recoverable suspended + direction=Receive + updated_at 超期的会话,转 terminal,返回被回收会话及其文件清单
- [ ] 3.2 定义保留期常量(默认 7 天 = 604800 秒),决定终态 reason(默认复用 `FatalError`;如需 `Expired` 则同步 entity/migration/镜像)
- [ ] 3.3 单测:保留期内不回收 / 超期回收 / 发送会话与 terminal 不受影响

## 4. 两端启动接入

- [ ] 4.1 桌面 `src-tauri/setup.rs`:`cleanup_recoverable_sessions` 之后调用回收原语,对返回的文件用本端 `FileAccess` 尽力清 `.part`(失败仅告警)
- [ ] 4.2 移动 `packages/swarmdrop-core/.../history.rs::reconcile_stale_sessions`:coordinator 清理之后调用同一原语 + 本端 FileAccess 清理
- [ ] 4.3 手动验证:造一个 8 天前的 suspended 接收会话,重启后被回收且 `.part` 删除;3 天前的不受影响

## 5. 移动端 re-sync 与 bindings

- [ ] 5.1 mobile-core `Cargo.toml` re-pin core git rev 到含本 change 的桌面 release commit
- [ ] 5.2 `pnpm --filter react-native-swarmdrop-core cargo:check`,再 `build:ios`/`build:android --and-generate` 重生成 cpp/src generated bindings
- [ ] 5.3 `pnpm typecheck` + `pnpm lint:ci` 两端绿
