# Rust Backend

## 概览

Rust 端的项目特有约束：crates/core 与 src-tauri 边界、specta IPC 类型映射、SeaORM/SQLite、libp2p P2P。常规 Rust 风格查 `/rust-best-practices`，async 模式查 `/rust-async-patterns`，Tauri IPC 查 `/tauri-v2`，SeaORM 查 `/sea-orm-2`。

## 模块边界

### 业务逻辑放 crates/core，src-tauri 是薄壳

`src-tauri/src/lib.rs` 用 `pub use swarmdrop_core::pairing;` 等 alias 把 core 模块路径桥进 crate（所以代码里 `crate::pairing::*` / `crate::protocol::*` 仍然有效）。**桌面壳唯一保留业务逻辑的模块是 `transfer/`**，其它都已迁移到 core。

**正确做法**：
- 加新业务逻辑/类型默认放 `crates/core`，让 SwarmDrop-RN 也能复用
- 桌面特定（keychain / 文件系统路径 / Tauri command 包装）才放 src-tauri
- 改 core 时跑 `cargo check -p swarmdrop-core --features specta`，再跑 `cargo check -p swarmdrop` 确认桌面壳不破

**相关文件**：`crates/core/src/lib.rs`、`src-tauri/src/lib.rs`、`dev-notes/architecture/core-desktop-mobile-boundaries.md`

### `crates/core/src/transfer/` 按功能分三层子目录

传输模块（`cleanup-transfer-tech-debt` 重组）从平铺 17 文件分成三组，加新文件时按职责归位：

- **`actor/`** —— 运行时单会话执行：`sender`(`SenderActor`)/`receiver`(`ReceiverActor`)/`registry`(`ActorRegistry`)/`checkpoint`(bitmap 纯函数)
- **`flow/`** —— `TransferManager` 的生命周期方法（按阶段拆 `impl` 块）：`prepare`/`send`/`receive`/`resume/{mod,validation,plan}`
- **`wire/`** —— 数据面字节层：`data_frame`(帧编解码)/`data_plane`(路由+注册表簿记，纯路由)/`crypto`
- **顶层** —— 跨层核心类型：`manager`(`TransferManager` 结构+trait impl)/`coordinator`(状态机 reducer)/`epoch`(`EpochGuard`)/`progress`/`policy`/`incoming`

**正确做法**：
- 跨层引用一律用绝对路径 `crate::transfer::<组>::<模块>`，不用 `super::`（文件进子目录后 `super` 语义会变）
- 文件进子目录后，被跨组调用的 `pub(super)` 要放宽到 `pub(crate)`（`pub(super)` 只剩组内可见）
- 术语固定：运行时内存对象叫 **actor**（`SenderActor`/`ReceiverActor`），**session** 只指逻辑会话 id / DB 行
- `EpochGuard`（`transfer/epoch.rs`）是 epoch 比较单点：`is_stale`(迟到`<`)/`is_newer`(更新`>`)/`matches`(精确`==`)，不要再散写裸 `<`/`>`/`==`

**相关文件**：`crates/core/src/transfer/mod.rs`、`crates/core/src/transfer/{actor,flow,wire}/mod.rs`、`crates/core/src/transfer/epoch.rs`

## IPC 类型 (specta)

### bindings.ts 是自动生成的

`src-tauri/src/setup.rs:104` 在 debug build 时调用 `specta.export(..., "../src/lib/bindings.ts")`，每次 `pnpm tauri dev` 启动都会重写。

**不要做**：
- 手动改 `src/lib/bindings.ts`——会被下次 dev 启动覆盖
- 把 bindings.ts 当成"前端可改的契约"

**正确做法**：
- 改 IPC 类型 → 改 Rust 端 struct → 运行一次 `pnpm tauri dev`（或 `cargo run`，会在 setup hook 里触发导出）→ bindings.ts 自动更新
- 临时手改 bindings.ts 只用于"先让 tsc 通过、稍后再启 dev 重新导出"

### specta 需要开 chrono feature

`Cargo.toml` 里 `specta` 必须含 `chrono` feature，否则 `chrono::DateTime<Utc>` 无法 `derive(specta::Type)`。SwarmDrop 已配置（见 `crates/core/Cargo.toml` + `src-tauri/Cargo.toml`）。

### 跨 IPC 的时间类型用 DateTime<Utc>

specta + chrono 会把 `DateTime<Utc>` 映射成 ISO 8601 字符串（前端 `string` 类型）。前端 `new Date(isoString)` 自动正确解析。

**不要做**：
- 用 `i64` 当 IPC 时间戳——前端容易把秒当毫秒（`new Date(秒数)` 解析成 1970 年附近），导致 timer 死循环（见配对码每秒重生 bug 的修复 commit `8d298e5`）

**例外**：DHT 跨设备记录（`ShareCodeRecord`）保持 `i64` Unix 秒以稳定线路格式 + 节省 record 体积。From 转换里手写 `.timestamp()`。

**相关文件**：`crates/core/src/pairing/code.rs`

## Clippy / dead_code

### 用 #[expect(...)] 替代 #[allow(...)]

项目里清一色用 `#[expect(clippy::xxx, reason = "...")]` 而非 `#[allow]`。Rust 1.81+ 的语义是：标了 expect 的 lint 一旦"自然消失"会反向报警，避免遗留的过期 allow。

**正确做法**：
```rust
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub fn insert_session(...) { ... }
```

**相关文件**：`crates/core/src/database/ops.rs`、`crates/core/src/transfer/flow/receive.rs`

## P2P / 异步

### 启动顺序：plugin → updater → database → start command

`src-tauri/src/setup.rs` 里 plugin 在 Builder::default() 注册；updater + database 在 setup() hook 里初始化并注入 Tauri state。**P2P 节点不在启动期自动起**——前端调 `commands::start()` 才创建 `NetClient` + `PairingManager`。

**相关文件**：`src-tauri/src/setup.rs`、`src-tauri/src/lib.rs` 的 `start` 命令

### 断点续传恢复走 Probe → Commit → Ack

`redesign-transfer-lifecycle` 已废弃旧的 `ResumeRequest` / `ResumeOffer` 双入口恢复路径。恢复统一走：
`ResumeProbe` 获取对端 phase / epoch / manifest / checkpoint / source fingerprint，再由发起方发送
`ResumeCommit { new_epoch, key, fetch_plan }`，对端校验后返回 `ResumeAck`。

**正确做法**：
- `new_epoch = max(local_epoch, peer_epoch) + 1`；Coordinator 只接受 `new_epoch > current_epoch`
- `ResumeReport` 必须携带 manifest 与 terminal_reason，才能区分 cancelled / fatal_error / source_modified
- 被动端 `ResumeCommit` 校验通过后按本端 direction 重建 `SenderActor` 或 `ReceiverActor`，再经 `NetworkSignal::ResumeCommitted` 转 active
- `ResumeRejectReason::PeerUnavailable` 不改本地状态，保留 suspended 供稍后重试；cancelled/source/checkpoint/session 缺失按语义转 terminal

**不要做**：
- 不要再新增 `ResumeRequest` / `ResumeOffer` 分支，旧路径会绕过 probe 阶段导致两端恢复事实不一致。
- 不要直接调用 `mark_session_transferring` 恢复；phase/epoch 必须由 Coordinator 写入并发布 projection。

**相关文件**：`crates/core/src/transfer/flow/resume/mod.rs`、`crates/core/src/protocol.rs`、`crates/core/tests/e2e_transfer.rs`

### 主动取消必须通知对端并写 cancelled

取消不是本地停止任务：本端要取消 live session、通知对端 `TransferRequest::Cancel`、写入 DB `Cancelled`，对端收到后也要标记 cancelled 并发出友好的 UI 提示。

**正确做法**：
- 发送方 `cancel_send` 也要像接收方一样发送 `Cancel`，不能只 `session.cancel()`
- 发送方 `waiting_accept` 还没有 `SenderActor`，必须通过 `outbound_offers` 记录并在 Offer 异步返回后撤回，避免对端已接受后继续隐藏传输
- 取消状态写入放在 `crates/core`，Tauri / RN host 只做薄命令封装
- 前端收到 `TransferFailedEvent` 中的 `对方取消` 时按 info toast 展示，不按错误处理

**相关文件**：`crates/core/src/transfer/flow/send.rs`、`crates/core/src/transfer/flow/receive.rs`、`src/stores/transfer-store.ts`

### libp2p-stream 数据通道：不在 stable facade，需直接依赖

`swarm-p2p-core` 用 `libp2p::stream` 承载文件传输等数据面字节流，但 **libp2p 0.56 stable facade 没有 `stream` feature**（libp2p-stream 仍是 `0.4.0-alpha`）。必须直接依赖 `libp2p-stream = "0.4.0-alpha"`（与 libp2p 0.56 同期，对齐 libp2p-swarm 0.47.x，无 multiple-versions 冲突）。

**正确做法**：
- `Behaviour::new_control(&self)` 返回 `Control`（可 clone、跨任务共享）；`Control::accept(proto)` 返回 `IncomingStreams`（生命周期独立于临时 control），`Control::open_stream(peer, proto).await` 打开出站流。
- `Stream` 是 `libp2p_swarm::Stream` re-export（`libp2p::Stream`，**非 feature-gated**），impl `futures::AsyncRead + AsyncWrite`；`DataChannel` 用 `stream_mut()` / `into_stream()` 暴露它，避免 Pin 投影。
- `IncomingStreams` 必须持续 poll：放进 core 中央 `select!`（多协议用 `futures::stream::select_all` 合并 + protocol 标签 + `if !is_empty()` 守卫防 busy-loop），accept 出的流用 `try_send` 转交，**绝不阻塞 swarm 循环**（否则拖死 ping / kad）。
- 开流级背压破损（yamux 静默丢流）：用 runtime 层计数登记表（`ChannelRegistry` + drop guard）显式 limit + 报 typed error，而非依赖底层丢弃。
- `OpenStreamError` 是 `#[non_exhaustive]`（`UnsupportedProtocol(_)` / `Io(_)`），match 必须带 `_`。

**不要做**：
- 不要手写自定义 `NetworkBehaviour + ConnectionHandler`——薄封装 `libp2p-stream` 即可，poll 负担由 core event loop 吸收，对下游透明。
- 不要把帧编解码放进 `libs/core`——它只传裸字节，帧协议在 `crates/core`（应用层）。

**相关文件**：`libs/core/src/data_channel.rs`、`libs/core/src/runtime/{node,event_loop}.rs`、`libs/core/src/client/mod.rs`

### transfer-data Finish 只是信号，完成事实必须由接收端证明

`TransferDataFrame::Finish` 只能表示发送端认为 fetch_plan 已写完，不能直接驱动本地
`Completed`。接收端必须用初始 checkpoint bitmap + 本次收到的 `BlockData` bitmap 证明所有非零文件
都完整后，才允许 finalize sink、`mark_file_completed`、`mark_session_completed` 并发布 projection。

**正确做法**：
- 首次传输调用处显式传 `full_fetch_plan(...)`；`SenderActor::run_data_channel` 把传入的 `fetch_plan` 当精确计划，不把空计划隐式扩展成全量。
- data-channel 接收端收到 `Finish` 后先跑完整性校验；缺块/缺 bitmap 走 `Interrupted`，不能把 bitmap 补成全完成。
- 零字节文件可在完成阶段创建空 sink 并标记完成；非零文件没有 live sink 时只能依赖已完整的恢复 checkpoint，不能创建空文件冒充完成。
- 当前 libp2p stream 是可靠有序流，`BlockRequest` 只保留协议帧位；生产路径不要假装支持同流重传，解密/范围错误应中断。

**相关文件**：`crates/core/src/transfer/{actor/sender.rs,actor/receiver.rs,wire/data_frame.rs}`

### swarm-p2p-core 测试需显式声明 tokio rt-multi-thread

`#[tokio::test(flavor = "multi_thread")]` 需要 tokio `rt-multi-thread`，而 `swarm-p2p-core` 的 `[dependencies] tokio` 只有 `rt`。测试一直靠 workspace feature unification（其他成员带进来）才能编译——**单独 `cargo clippy -p swarm-p2p-core --all-targets` 或单独构建会报 `runtime flavor multi_thread requires rt-multi-thread`**。已在 `[dev-dependencies]` 显式声明 `rt-multi-thread + time`。RN 端单独复用 core 时同理。

**相关文件**：`libs/core/Cargo.toml`

### 传输生命周期：Coordinator reducer + 增量过渡（phase/reason 与旧 SessionStatus 并存）

`redesign-transfer-lifecycle` 把传输状态从扁平 `SessionStatus`（5 态）重构为 `phase`（offered/waiting_accept/active/suspended/terminal）+ `suspended_reason`/`terminal_reason` + `epoch` + `recoverable`。采用**增量过渡**：新字段与旧 `SessionStatus` 列并存、逐步迁移、最后删旧——每步编译通过、不破坏现有传输系统。

**正确做法**：
- 状态机核心是纯函数 reducer（`transfer/coordinator.rs::reduce`）：`(state, input) → Some(new)/None`，无 DB/网络依赖，可独立单元测试（epoch 校验、terminal 不可逆都 hoist 到这一层）。`TransferCoordinator::dispatch` 才做 I/O（load→reduce→persist）。
- **过渡期 status 与 phase 必须同步**：`apply_transition` 写 phase 时经 `TransferPhase::legacy_status(terminal_reason)`（entity 单一映射来源）一并写旧 `status`，否则 coordinator 转换后前端旧路径读到滞留状态。这是 simplify altitude review 抓到的漂移坑。
- `dispatch` 已 load 的 Model 直接传给 `apply_transition(&Model, ...)` 用 `into_active_model` 更新，**不要**在 apply 里二次 `find_by_id`（省一次 SELECT）。
- migration 加列用 `ALTER TABLE ... ADD COLUMN ... NOT NULL DEFAULT ...`；开发期 `DELETE FROM transfer_files/transfer_sessions` 清空旧历史（design 允许），避免处理旧行默认值。
- sea-orm 2.0 entity 用 `ActiveModel::builder().set_xxx()`；加 NOT NULL 字段后在 `create_session` 补 `.set_phase/.set_epoch/.set_recoverable`，未 set 字段走 DB default（builder 不强制）。

**相关文件**：`crates/core/src/transfer/coordinator.rs`、`crates/core/src/database/ops.rs`（apply_transition/projection）、`crates/entity/src/lib.rs`（`TransferPhase::legacy_status`）

### 接线 mark_session_* → Coordinator：本地/对端 reason 区分 + 取消优先于 error

**所有 session 级 phase 转换一律走 `coordinator.dispatch` → `reduce` → `apply_transition`，禁止 `mark_session_*` 直写终态**（`cleanup-transfer-tech-debt` 轮1 收口）。文件级副作用（`mark_file_completed`/finalize/checkpoint/inbox 索引）在 dispatch **之前**完成，但 session 终态由 reducer 统一写。早期「complete/fail 走 mark_* + `publish_projection`」是迁移没收口的过渡写法，已废弃。

**正确做法**：
- **完成/失败/拒绝走 dispatch**：收发完成→`dispatch(Actor{epoch, Completed})`；接收方校验失败→`dispatch(Actor{epoch, FatalError(msg)})`；策略拒绝→`create_session(offered)` 后 `dispatch(User{Reject})`。`epoch` 用 actor 自己的 epoch——旧 epoch actor 在 resume 后才完成会被 epoch 守卫忽略。
- **完成事件/收件箱索引 gate 在 dispatch 返回 `Some`**：`is_terminal` 守卫让先到的终态获胜、迟到的被拒（reduce 返回 None），所以被取消/旧 epoch 抢先时不发 `TransferCompleted`/不建 inbox。这修了「取消后并发完成把 cancelled 覆盖成 completed」的竞态 bug（回归测试 `e2e_terminal_irreversible_under_concurrent_complete_cancel`）。
- **本地 vs 对端 reason 必须区分**：本地操作→`User{Pause/Cancel}`（写 LocalPaused/Cancelled）；对端发来的 Pause/Cancel→`Network{RemotePaused/RemoteCancelled}`（写 RemotePaused）。
- 入站控制消息（req_resp 的 Cancel/Pause）当前**不携带 epoch**，用 `dispatch_network_current`（读 session 当前 epoch 再 dispatch，等价无 stale 保护）；待数据面帧协议带 epoch 后收紧。
- **`publish_projection` 只剩「新建会话首投影」一个合法用途**（offered/waiting_accept，创建不是 reduce 输入、没有 from-state）；状态转换的 projection 由 dispatch 在 reduce 成功后自动发，别再用它给终态补投影。
- **对端断连 → `Network{Interrupted}`**：event_loop 在 `NodeEvent::PeerDisconnected` 调 `IncomingTransferRuntime::handle_peer_disconnected(peer)`（新 trait 方法，默认 no-op），impl 里 `find_active_session_ids_by_peer` + 取消内存会话 + dispatch。发送端会话本就 idle（只应答 ChunkRequest），**只能靠这个 hook 感知断连**。
- **取消优先于 error**（`actor/receiver.rs::run_data_channel` 收尾）：被 `cancel_token` 取消的传输即使有 in-flight chunk 错误也返回 `Ok(false)`（取消），**不能**先判 error 返回 Err——否则断连/取消 teardown 时的 chunk 错误会触发 `fail_session` 写 terminal/failed，盖掉 Interrupted/Cancelled。这也修了「主动取消时若 chunk 报错会变 failed」的潜在 bug。
- **`transferred_bytes` 是 projection 派生、不在 session 列维护**（`cleanup-transfer-tech-debt` 轮4）：`get_transfer_projection` 直接 `SUM(files.transferred_bytes)`；文件级进度由 `persist_chunk`（接收）/ `save_sender_file_progress`（发送）增量落库。**不要**在 pause/cancel/disconnect 前手工把文件进度 sync 回 session 列（已删 `sync_session_transferred_bytes`），那是会漂移的二次写。
- **发送终态副作用归 actor**：发送完成/中断的 dispatch + 落库 + 完成事件下沉到 `SenderActor::on_completed`/`on_interrupted`（与接收 `finish_data_channel`/`fail_session` 对称），`wire/data_plane` 只做纯路由 + `remove_send_if_epoch` 注册表簿记（按 epoch 移除防旧任务误删 resume 后新 actor）。

**相关文件**：`crates/core/src/transfer/{coordinator.rs,flow/receive.rs,actor/receiver.rs,actor/sender.rs,flow/send.rs,wire/data_plane.rs}`、`crates/core/src/network/event_loop.rs`、`crates/core/src/database/ops.rs`、`crates/core/tests/e2e_transfer.rs`（remote-reason / peer-disconnect / sender-resume 确定性测试）

### 桌面启动清理只保留过期文件清理，active 统一交给 Coordinator

`src-tauri` 启动时不再把 sender/receiver transferring 会话直接改成 failed/paused。`setup.rs` 先注入
`TauriEventBus`，再调用 `cleanup_stale_sessions(db, event_bus)`；该函数用
`TransferCoordinator::cleanup_recoverable_sessions` 把所有 `phase=active` 会话转为
`suspended/app_restarted` 并发布 projection。

**正确做法**：
- `cleanup_stale_sessions` 的 active 清理只经 Coordinator，不直接写 `status`
- 桌面特有的 `.part` 文件清理仅用于过期 receiver suspended 会话，清理后调用 `mark_session_failed`
- `start` 命令复用 managed `TauriEventBus`，不要在 setup 已注入后再创建一个不共享 prepare channel 的 bus

**相关文件**：`src-tauri/src/{setup.rs,database.rs,commands/lifecycle.rs}`

### 前端传输列表只消费 TransferProjection

`redesign-transfer-lifecycle` 中，前端传输列表、详情页和设备页不再读取
`get_transfer_history` / `get_transfer_session`，也不再维护 `sessions + dbHistory`
双状态源。后端提供 `get_transfer_projections` 和 `"transfer-projection-update"` 事件，
前端 store 只保存 `projections`、`progressBySession`、`pendingOffers`。

**正确做法**：
- 发送方 `start_send` 时就在 core 内创建 `phase=WaitingAccept` projection；接收方收到 Offer 时创建 `phase=Offered` projection
- 用户接受/拒绝、对端接受/拒绝、暂停/取消/恢复都经 Coordinator 转换 phase/reason 并发布 projection
- `transfer-progress` 事件只允许更新 projection 的进度字段，不允许前端据此推断 completed/failed/suspended
- UI 文案统一从 `TransferProjection.phase + reason` 派生，避免列表、详情和两端设备各自解释旧 `SessionStatus`

**不要做**：
- 不要在前端重新拼接 active sessions 与 DB history
- 不要在发送/接收页面手工构造 transient transfer session；如果缺等待态，优先让后端补 projection

**相关文件**：`crates/core/src/transfer/{flow/send.rs,flow/receive.rs,coordinator.rs}`、`crates/core/src/database/ops.rs`、`src/stores/transfer-store.ts`、`src/lib/transfer-projection.ts`

### ActorRegistry 是运行时 actor 唯一入口

SenderActor / ReceiverActor 的内存生命周期不再散落在 `TransferManager` 的裸
DashMap 操作里。`ActorRegistry` 统一管理创建、替换、移除、取消和 epoch 准入；
Coordinator 负责 DB 状态，ActorRegistry 负责内存 actor 唯一性。

**正确做法**：
- 插入 actor 必须带 session epoch：首传 epoch=0，ResumeCommit 后使用 `new_epoch`
- 同 epoch 或旧 epoch 的 actor 插入会被拒绝并取消；更高 epoch actor 会取消并替换旧 actor
- ReceiverActor 后台任务结束时按 `(session_id, epoch)` 移除，不能只按 session_id 移除，避免旧任务结束误删恢复后的新 actor
- 业务代码通过 `TransferManager::{get,insert,remove}_{send,receive}_session` helper 访问 actor，不直接碰 registry 内部 map

**不要做**：
- 不要新增 `send_sessions` / `receive_sessions` 裸 map 操作
- 不要在 resume/data-channel 路径创建 actor 时漏传 new_epoch

**相关文件**：`crates/core/src/transfer/actor/registry.rs`、`crates/core/src/transfer/{manager.rs,flow/send.rs,flow/receive.rs,flow/resume/mod.rs}`

### crates/core 端到端集成测试：两个真实节点 + MemoryHost + sqlite::memory（不需要 Tauri/真机）

完整传输链路（offer→transfer→pause→resume→cancel）可在纯 `cargo test` 里跑通，**零生产代码改动**。调研结论：`libp2p-swarm-test` **不适用**——它测 raw `Swarm` + 自定义 `NetworkBehaviour`，和 SwarmDrop 的 `NetClient`/`EventReceiver` 封装层级对不上，且 `CoreBehaviour` 含只能 `with_relay_client` 造的 `relay::client::Behaviour`，传不进 swarm-test。正解是「两个真实 `start()` 节点 + 关 mDNS + 显式 dial」。

**✅ 已落地实证**：`crates/core/tests/e2e_transfer.rs` 已实现并通过——连通性 smoke（`e2e_two_nodes_connect`）+ 完整单文件传输（`e2e_single_file_transfer`：prepare→send_offer→accept→拉取落盘→Complete→Ack→两侧 DB 都到 `Terminal/Completed` + 接收方 `sink_bytes` 等于源 + 两侧都发 `TransferCompleted`）。dev-deps 只需补 `tokio`（macros+rt-multi-thread）+ `migration`（其余 sea-orm/entity/uuid/swarm-p2p-core 已是普通 dep）。`tempfile` 暂未用到（内存库单连接钉死即可跨重启保活）。

**正确做法**：
- 现成资产：`MemoryHost::new(paths)`（`crates/core/src/host.rs`，实现全 6 个 host trait + `with_source()` 预载文件 + `events()` 取回 CoreEvent）；`Database::connect("sqlite::memory:")` + `migration::Migrator::up(&db, None)`；`swarm_p2p_core::start` + `TransferManager::new` + `NetManager::new` + `run_event_loop` 全 public，复刻 `runtime::start_node` 即可。
- **关 mDNS + 显式 dial 消除时序**：`NodeConfig::new(...).with_mdns(false).with_relay_client(false).with_dcutr(false).with_autonat(false).with_listen_addrs(["/ip4/127.0.0.1/tcp/0"])`；建连用 `client.add_peer_addrs(peer, [listen_addr]) + client.dial(peer)`，不靠 mDNS `PeerDiscovered`（这也是 `data_channel.rs` 并行串扰的根治法）。
- 每节点 `tokio::spawn(run_event_loop(receiver, mgr.shared_refs(), host, None))` 驱动接收方协议处理（IncomingTransferRuntime）。
- 断言：`MemoryHost.events()` 查发出的 projection / Transfer* 事件；`db` 查 phase/epoch/checkpoint 验状态机。中断模拟 = drop 一侧 event_loop task；重启 = 用同一 `db` 重新 spawn 节点。
- dev-deps：`migration`（workspace）、`sea-orm`、`tokio`（rt-multi-thread+macros）、`swarm-p2p-core`、`tempfile`。

**不要做**：
- 不要 mock `AppNetClient`（= `NetClient<AppRequest,AppResponse>`，必须两个真实建连节点）。
- 不要忘 `is_paired` 校验：Offer 要求已配对，`NetManager::new` 的 `paired_devices` 要互相塞 `PairedDeviceInfo`（双向），否则 Offer 直接被 `OfferRejectReason::NotPaired` 拒。`is_paired` 唯一运行时依据是 `PairingManager` 的内存 DashMap，不查 DB / keychain。
- 不要等 mDNS 发现事件触发连接——改用 `dial()` 的精确 await。
- **连接判定不要用 `connected_count()` / `get_network_status().connected_peers`**：它额外要求 identify 把 `agent_version` 分类成 SwarmDrop 客户端（`OsInfo::is_swarmdrop_agent`），测试给的 agent_version 不匹配会恒为 0。改用 `manager.devices().is_connected(&peer_id)`（只看裸 `PeerConnected`，与连通性/req_resp/配对都无关）。
- **不要在同步谓词里 `block_on` async DB 查询**：`#[tokio::test]` 已在 runtime 上，再建嵌套 runtime block_on 会 panic（"Cannot start a runtime from within a runtime"）。DB 等待写原生 `async` 轮询循环（`loop { get_transfer_projection().await; sleep().await }`），连接/事件这类同步状态才用同步谓词轮询。
- 端口用 `/ip4/127.0.0.1/tcp/0`（OS 分配），dial 前必须先轮询 `get_network_status().listen_addrs` 拿到实际绑定地址（`run_event_loop` 处理 `NodeEvent::Listening` 时回填）。
- **`client.dial()` 在并行 `cargo test` 下会瞬时失败**：多个 `#[tokio::test(multi_thread)]` + 多组节点同跑抢 CPU 时，到 `127.0.0.1:port` 的连接尝试瞬时失败，`dial().expect()` 会 flaky（串行 `--test-threads=1` 不复现，但 CI 默认并行）。`connect` helper 要**重试 dial 直到 `devices().is_connected(&peer)` 双向为真**、忽略单次 dial 错误（已连接时再 dial 是廉价 no-op 错误）——连接才是目标，不是单次 dial 调用成功。

**相关文件**：`crates/core/tests/e2e_transfer.rs`（**已实现的 harness，直接参照/扩展**）、`crates/core/src/host.rs`（MemoryHost）、`crates/core/src/runtime.rs`（start_node 可复刻）、`crates/core/src/network/event_loop.rs`（run_event_loop）、`libs/core/tests/data_channel.rs`（现有双节点模式参考）

### LAN Helper 三节点测试需要真实私有网卡地址

`auto-discover-lan-helper-nodes` 的三节点集成测试会启动 A/B 普通节点和 C LAN Helper，
用 mDNS + Identify 事件把 C 注册为 infrastructure peer，再通过 Kad record 验证 A 写、B 读。
由于生产逻辑会过滤 loopback 和 link-local，测试不能只监听 `127.0.0.1`，否则无法覆盖真实
LAN Helper 路径。

**正确做法**：
- 测试用 `if-addrs` 枚举 operational up、非 loopback、非 p2p 的私有 IPv4 网卡并绑定 `/ip4/<private>/tcp/0`
- 找不到可绑定私有 IPv4 时打印说明并跳过真实 LAN 流程，避免无网卡 CI 假失败
- LAN Only 测试要额外断言 `NetworkStatus.candidate_sources` 不包含 `BuiltInPublic`

**相关文件**：`crates/core/tests/e2e_lan_helper.rs`、`crates/core/Cargo.toml`

## 身份存储 (keychain)

### dev 用文件后端、release 用系统 keychain（ad-hoc 签名导致 keychain 拒读）

`pnpm tauri dev` 编译的是 **ad-hoc 签名（linker-signed）二进制**——`codesign -dvvv target/debug/swarmdrop` 显示 `flags=0x20002(adhoc,linker-signed)`、`TeamIdentifier=not set`，且 `Identifier` 带内容 hash **每次 rebuild 都变**。macOS login keychain 对 ad-hoc 签名进程访问限制极严，所有 `keyring` 请求（**连查询一个不存在的条目**）都返回 `errSecInteractionNotAllowed`（"Platform secure storage failure: User interaction is not allowed."，不弹授权框直接硬拒）。

表现：设备身份起不来 → `initialize_identity` 抛错 → core `identity.rs` 的 `provider.load_identity().await?` 直接 `?` 传播（`keychain.rs` 只把 `NoEntry` 转 `Ok(None)`，其它错误一律 `Err`，连"生成新身份"退路都没有）→ 前端 `deviceId` 为 null → 点"启动节点"静默无反应。**删 keychain 条目无效**（是签名问题、非条目问题，新签名读旧条目/连查询都被拒）。

**正确做法**：
- 身份存储后端按 build 类型分叉，cfg 边界**唯一集中**在工厂 `crate::host::keychain_provider(&app)`：
  - `#[cfg(debug_assertions)]` → `FileKeychainProvider`（`app_data_dir/dev-identity.json` 明文持久，写后 `chmod 0600`）
  - `#[cfg(not(debug_assertions))]` → `DesktopKeychainProvider`（系统 keychain）
- 工厂返回 `Arc<dyn KeychainProvider>` 统一两分支静态类型（cfg 分支返回不同具体类型，`-> impl Trait` 无法统一）；core 函数签名是 `P: KeychainProvider + ?Sized`，用 `&*provider` 传入。
- 文件后端必须**持久**（keypair 存盘、复用），否则每次重启换 PeerId 破坏配对测试。`load_identity` 在文件缺失/keypair 空时返回 `Ok(None)`（绝不 `Err`），让 core 走"生成新身份并 save"路径。
- 调用 `Arc<dyn KeychainProvider>` 的 trait 方法**不需要** `KeychainProvider` 在 scope（trait object 走 vtable）；从具体 struct 换成 `Arc<dyn>` 后记得删掉原 `use ...::KeychainProvider`，否则 unused import warning。

**不要做**：
- 不要在 `DesktopKeychainProvider` 内部塞 `if-cfg` 降级——release 也可能在 keychain 偶发报错时误把明文私钥落盘；且降级逻辑散落每个方法。独立 provider + cfg 门控 `#[cfg(debug_assertions)] pub mod file_keychain;` 让 release 二进制根本不含文件后端代码。
- 给新增 `#[tauri::command]` 透传 `app: AppHandle` 改变了命令签名（如 `remove_paired_device` 补 app），但 Tauri 按类型注入、不占前端参数位，前端 invoke 不变；改后跑一次 `pnpm tauri dev` 重新导出 bindings 即可。

**相关文件**：`src-tauri/src/host/file_keychain.rs`、`src-tauri/src/host.rs`（`keychain_provider` 工厂）、`crates/core/src/identity.rs`、`src-tauri/src/host/keychain.rs`

## 系统托盘

### 三态托盘图标要拿到 `TrayIconBuilder::build()` 的返回值，且不能用 `icon_as_template`

`TrayIconBuilder::build(app)` 返回 `tauri::Result<TrayIcon<R>>`——早期实现里这个返回值被直接丢弃（`builder.build(app)?;`），导致后续没有句柄可以调 `set_icon` 动态换图标，只能在创建时定死一次。要支持运行时切换图标，必须把这个返回值存进长期持有的状态（本项目是 `TrayState`），否则和 `MenuItem` 句柄一样会因为没人持有导致效果消失。

另外，`icon_as_template(true)`（macOS 的单色模板图标，跟随系统深浅色自动着色）和"用颜色区分状态"是互斥的——template 图标会被系统强制去色成单色轮廓，图标本身的颜色信息不会显示。如果三态要靠颜色区分（而不是纯形状区分），三个平台都不能用 template 模式，直接传全彩 PNG。

**正确做法**：
```rust
let tray_icon = builder.build(app)?;  // 存返回值，不要丢弃
app.manage(TrayState { status_item, pause_item, tray_icon });

// 运行时切换：
match tauri::image::Image::from_bytes(png_bytes) {
    Ok(icon) => { let _ = state.tray_icon.set_icon(Some(icon)); }
    Err(e) => warn!("托盘图标解码失败，保留上一次的图标: {e}"),
}
```
- `Image::from_bytes` 需要 `image-png`（或 `image-ico`）feature，本项目 `Cargo.toml` 已开。
- 三态图标用 `include_bytes!` 编译期嵌入（避免运行时文件路径依赖），放 `src-tauri/icons/tray/`。
- 状态图标设计手法：复用品牌 logo 本身的双色剪影结构（不额外加徽章/角标、不整体去色），每个状态只换配色（比如离线态灰调、正常态品牌色、警示态琥珀色），形状不变——比在图标上叠加小圆点更耐小尺寸（22×22 菜单栏图标叠角标很容易糊）。
- 状态→(文案, 图标) 的派生只写一处：用一个 `TrayStatus` 枚举 + `from_flags(online, paused)` 做唯一的三分支匹配，`text()`/`icon_bytes()` 挂在枚举上；不要在 `status_text` 之外再单独写一个结构相同的 `match (online, paused)` 去选图标字节，两份独立的三分支匹配迟早会在加新状态时漏改一处。
- 图标解码失败的处理分两种场合：`build_tray` 里的初始图标字节是编译期常量，解码失败只可能是资产本身损坏，属于构建期 bug，应该让它经 `?` 直接让托盘创建失败、快速暴露问题；`refresh_tray` 没有 `Result` 可传播，退化成 `warn!` 日志 + 保留上一次图标，不要用 `if let Ok(..)` 静默吞掉（本文件其它地方对真正无害的操作才用 `let _ = ...`，图标解码失败不算无害）。

**相关文件**：`src-tauri/src/tray.rs`、`src-tauri/icons/tray/{online,offline,paused}.png`

## 依赖升级

### 判断"是否真落后"看 Cargo.lock 解析版本，不看 requirement 字面

很多依赖写宽松约束（`tauri = "2"` / `axum = "0.8"` / `tokio = "1.49"`），`cargo update` 早把它们解析到最新。真正需要动手的只有被版本号**卡住**的（major / 0.x 跨段）。审计方法：`Cargo.lock` 实测解析版本 + crates.io max_stable 对比，再用 `cargo tree -i <crate>@<ver>` 看旧版来源。

**正确做法**：
- 区分"直接依赖解析到最新"（无需动）与"requirement 上限低于最新"（要改 Cargo.toml）。
- 多版本并存常见且无害：`sha2 0.10` / `chacha20poly1305 0.10` 等旧版由 `libp2p → ed25519-dalek` 等传递依赖钉住，**无法与我们直接依赖的 0.11 统一**，只增编译体积、不冲突。`cargo update -p sha2 --precise X` 会因多版本报 `ambiguous`，属正常。

### RustCrypto 0.11 波（chacha20poly1305 / sha2）：aead::OsRng 移除 → Generate trait

升级 chacha20poly1305 0.10→0.11（aead 0.5→0.6）时**唯一硬编译错误**是 `chacha20poly1305::aead::OsRng` 不再 re-export（rand_core 升 0.10，OS 随机改走 getrandom）。

**正确做法**：
- 随机 key 生成改用 `Generate` trait（无 rng 参数、getrandom 后端）：
  ```rust
  use chacha20poly1305::aead::{Generate, Key};
  Key::<XChaCha20Poly1305>::generate().into()  // -> [u8; 32]
  ```
- `XNonce::from_slice(&nonce)` 在 hybrid-array 下已 deprecated，`-D warnings` 会变硬错误 → 改 `&XNonce::from(nonce)`（`[u8;24]` 走 `From<[u8;N]>`）。
- `XChaCha20Poly1305::new(key.into())`、`Sha256::digest(...).to_vec()` **无需改**：hybrid-array `Array` 仍 `Deref<[u8]>` + 提供 `From<&[u8;N]>`。SHA256 摘要值逐字节不变 → DHT key 兼容旧节点。
- 这俩同属 RustCrypto 协调波，**一起升**避免 generic-array/hybrid-array 长期并存。需 edition 2024 / MSRV 1.85（本仓已满足）。

**相关文件**：`crates/core/src/transfer/wire/crypto.rs`、`crates/core/src/pairing/dht_key.rs`

### rmcp 1.x→2.0：类型改名 + streamable-HTTP 新增 Host/Origin 白名单

src-tauri 是 rmcp 唯一直接依赖方（`tauri-plugin-mcp-bridge` **不**依赖 rmcp），升 2.0 无传递冲突。编译期改动仅两处机械改名：

**正确做法**：
- `rmcp::model::Content::text(..)` → `ContentBlock::text(..)`（v2 把 `Content` 改名 `ContentBlock`）。
- `RawResource::new(..).…​.no_annotation()` → 直建 `Resource::new(..).with_description(..).with_mime_type(..)`（v2 删除 `Annotated<T>`/`AnnotateAble`/`RawResource`）。
- 宏 `#[tool]`/`#[tool_router]`/`#[tool_handler]`、`ServerHandler` trait、`StreamableHttpService`、`Parameters<T>`、axum 0.8 兼容性**均不变**。

**注意（运行时、非编译）**：v2 给 streamable-HTTP 加了 DNS-rebinding 防护，`StreamableHttpServerConfig` 新增 `allowed_hosts`（默认含 `127.0.0.1`/`localhost`/`::1`）和 `allowed_origins`。本地绑定通常无碍，但升级后需冒烟连一次 MCP client；被拒就显式放行。v2 还含 streamable-HTTP session leak 安全修复（#934）。

**相关文件**：`src-tauri/src/mcp/{tools,resources,server}.rs`

### keyring 3.x→4.x：feature 体系重构为 v1 facade（旧 feature 全删）+ 仅 release 可验证

keyring 4.x 不是无脑 bump：把后端拆成 `keyring-core` + 各平台独立 store crate，默认 `v1` feature 按 target 自动 set_default_store。**旧 feature（apple-native/windows-native/linux-native-sync-persistent/crypto-rust/vendored）全部移除**，保留会编译失败。

**正确做法**：
- 删掉三个 `[target.'cfg(...)'.dependencies]` keyring 块，合并为 `[dependencies]` 单行 `keyring = "4.1.2"`（不要再写 default-features=false 或旧 feature 名）。
- `keychain.rs` 源码**零改动**：`Entry::new` / `set_secret`/`get_secret` / `set_password`/`get_password` / `delete_credential` / `KeyringError::NoEntry` / `{error}` Display 全兼容。`pub mod keychain` 无条件编译，故 debug `cargo check` 即可覆盖；release cfg 工厂分支用 `cargo check --release` 坐实。
- Linux 后端由 dbus-secret-service 换纯 Rust zbus，不再链接 libdbus/OpenSSL；`release.yml` 无 keyring 专属 apt 依赖、无需改。

**验证盲区（务必真机）**：keyring 仅 release build 生效（debug 走 `file_keychain`）+ macOS ad-hoc 签名进程被 Keychain 拒读 → `cargo test`/`pnpm tauri dev` **覆盖不到真实路径**，编译通过≠功能正确。必须出签名 release 包在三平台手测身份读写 + 重启 PeerId 稳定。跨版本 store 实现全换，老用户旧条目可能读不到 → 走"找不到即重建"（[见上 keychain 段]）→ PeerId 重置需重新配对，release note 要提示。

**相关文件**：`src-tauri/src/host/keychain.rs`、`src-tauri/src/host.rs`

### 桌面「用本应用打开文件」（share-target 入口）：文件用 Tauri fileAssociations（按扩展名），别用 public.data 通配

**macOS「打开方式」只显示声明了「与该文件 UTI 具体匹配」的 app，不显示只声明通用 `public.data`/`public.item` 的 app**（Apple 论坛 + 实测：xlsx/md/sql 都被"归属抑制"压掉，只有无归属的随机文件才偶尔显示）。macOS 15.4+ 里 `public.data` 单条目还行、多加几条 UTI 还会触发 Gatekeeper。所以**「声明 public.data 覆盖任意文件」在 Open With 上根本走不通**——这是 macOS 设计、不是实现问题。真·任意文件只能走原生 Share Extension（Tauri 不脚手架，重活，单独立项）。

**正解：文件用 Tauri 官方 `bundle.fileAssociations`（按 `ext` 扩展名列表）**——为每个扩展名生成具体 UTI 声明，Open With 可靠显示，且三平台注册由 Tauri 统一生成（macOS CFBundleDocumentTypes+LSHandlerRank / Windows 注册表 / Linux .desktop MimeType）。用 `role=Viewer`+`rank=Alternate`（出现但不抢默认）。代价：只覆盖列举的扩展名（列一批广的即可：Office/文档/图片/视频/音频/压缩/代码…），极冷门/无扩展名文件不显示。
- ⚠️ Tauri 曾漏生成 `LSHandlerRank`（issue #13159）导致 macOS 不进 Open With，需 **tauri ≥ 2.6 左右**（本仓 2.11.3 已含修复）。
- **别再自定义 `src-tauri/Info.plist` 塞 CFBundleDocumentTypes**：会与 Tauri 生成的合并/覆盖冲突。

**文件夹**（fileAssociations 按扩展名，表达不了目录）走 `external_open::register_open_with` 后台线程单独最小注册：
- **Windows**：HKCU 注册表 `Software\Classes\Directory\shell\<Verb>\command`（`winreg`，`[target.'cfg(windows)'.dependencies]`，幂等短路）。文件不用手写注册表了，交给 Tauri。
- **Linux**：`~/.local/share/applications/*.desktop` 的 `MimeType=inode/directory;` + `update-desktop-database`（best-effort、`.spawn()` 不等子进程）。文件的 MimeType 由 Tauri 生成的 .desktop 承载。
- **macOS**：本轮不做文件夹 Open With（自定义 plist 有合并冲突风险，且 macOS 文件夹 Open With 本就少见）。

**路径送达机制三平台也不同**（`external_open::ingest_paths` 统一入口 + ~200ms 去抖合并）：
- macOS 走 `RunEvent::Opened { urls }`——**必须把 `lib.rs` 的 `.run(generate_context!())` 改成 `.build(ctx)?.run(|handle, event| ...)`** 才能接到；冷启动不经 argv。
- Windows/Linux 冷启动读 `std::env::args()`；热启动读 `single_instance` 回调的 `args`（原本被 `_args` 丢弃，是天然挂载点）。
- **冷启动竞态**：`RunEvent::Opened`/argv 可能早于前端订阅 → Rust 侧缓冲 + 前端 mount 时调 `take_pending_external_open()` 拉取（取走即清空、标记就绪）。前端务必**先挂事件监听、再拉 pending**，否则 take 标记就绪后、订阅前到达的路径会丢。
- **⚠️ 缓冲必须用进程级全局（`OnceLock`），不能用 Tauri 托管 state**：macOS「退出 app 后用『打开方式』打开文件」是冷启动 + 窗口状态恢复，`application:openURLs:`（→`RunEvent::Opened`）可能**早于 `setup()` 的 `app.manage(...)`** 到达。此时若在 Opened handler 里 `app.state::<T>()`，会 panic「state not managed」；而该 handler 在 ObjC `extern "C"` 边界上、panic 不可 unwind → 直接 `SIGABRT`（崩溃栈：`tao::...application_open_urls` → `panic_cannot_unwind` → `abort`，release 下我们的帧被内联，看着像 tao 自崩）。Tauri 官方 Opened 示例用托管 state 只在「app 已运行」场景成立。解法：缓冲放模块内 `OnceLock<Mutex<..>>`，Opened 冷路径完全不碰 `AppHandle`；并给 Opened handler 外包一层 `std::panic::catch_unwind(AssertUnwindSafe(..))` 兜底。
- **唤窗只在前端就绪（热态/托盘隐藏）时做**：常驻托盘、窗口隐藏时来了外部打开要 `show_main_window` 否则用户啥都看不到；但**别在冷启动 Opened 早期路径调 AppKit 窗口操作**（状态恢复中，有风险），且冷启动窗口本就默认显示。用「缓冲里的 `frontend_ready` 标志」区分冷/热，仅热态唤窗。
- app 自定义命令/事件走 `core:default`，**不需要**在 `capabilities/default.json` 加权限（只有 plugin 命令才要）。

**验证盲区（务必真机、逐平台）**：文件关联注册 + 路径送达全部**只能打包安装后在对应系统手测**，`cargo check`/`pnpm tauri dev` 覆盖不到；且 Windows 注册表代码在 mac 上因 `cfg(windows)` **连编译都不过**（Linux 同理）。macOS 侧还要注意 ad-hoc 签名/未公证的 dev 包在 Finder「打开方式」里的行为可能与正式包不同。

**相关文件**：`src-tauri/tauri.conf.json`（`bundle.fileAssociations` 扩展名列表）、`src-tauri/src/external_open.rs`（文件夹注册 + 路径入口/缓冲）、`src-tauri/src/{lib,setup}.rs`、`src-tauri/Cargo.toml`（winreg）、前端 `src/components/external-open-handler.tsx`

### develop 基线可能带 clippy/fmt 漂移（clippy/rustfmt 版本更新所致）

工具链升级（如 clippy/rustfmt 1.95）会新增/收紧 lint，使**之前干净**的已提交代码在全量重建时冒出警告（too_many_arguments、derivable_impls、items_after_test_module、collapsible_if、unused_imports 等）和 fmt 漂移。它们不是本次改动引入的。

**正确做法**：
- 证明"本次改动 0 新警告"：`git stash` 前后各跑一次 `cargo clippy --workspace`，比对计数。
- too_many_arguments 按本仓约定加 `#[expect(clippy::too_many_arguments, reason = "...")]`（async_trait 方法上的 `#[expect]` 会随宏展开保留、能命中）。
- 只在 test 用到的 import 移进 `#[cfg(test)] mod tests` 局部，别留在模块顶层（否则 lib 构建报 unused，即便 test mod 有 `use super::*`）。

**相关文件**：`crates/core/src/{device.rs,network/event_loop.rs,transfer/incoming.rs,transfer/flow/receive.rs}`、`src-tauri/src/database.rs`
