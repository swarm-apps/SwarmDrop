# Design — 传输子系统清理

## 审查结论（7 路并行架构审查综合）

对 `crates/core/src/transfer/`（6494 行 / 17 文件）+ `database/ops.rs` + 前端 `transfer`/`inbox`/`devices` 做了 7 路并行审查（state-machine / data-plane / resume / lifecycle-persistence / fe-transfer / fe-features / cross-cutting）。

**总判断**：架构内核健康，问题是「一次没收口的迁移 + 多人接力的清理债」，不是地基问题。

健康的部分（不要动）：
- `coordinator.rs` reduce 是纯函数 `(state, input) → Option<state>`，无 I/O，单测覆盖好。
- 平台中立守得干净：core 内无 `tauri`/`std::fs`/`PathBuf` 泄漏，全走 `FileAccess`/`EventBus`/`Notifier`/`TransferRuntime` trait。
- 协议单一无双数据通路：旧 req_resp 拉取式 `ChunkRequest`/`run_transfer` 已被 `harden-transfer-lifecycle` 删除，data channel + `BlockData` 推送是唯一数据面。
- 旧 `ResumeRequest`/`ResumeOffer` 双入口已删，resume 只走新 `ResumeProbe`/`ResumeCommit`。
- 前端 store 已以 `TransferProjection` 为唯一权威源，无「active + history 前端合并」旧逻辑。

## 病根（4 路独立指向）：终态写入双路径

`coordinator.dispatch`（reduce + apply_transition，带 epoch 守卫 `coordinator.rs:195`、terminal 不可逆守卫 `coordinator.rs:203`、自动发 projection）被声明为「唯一持久化入口」。但实际分裂：

| 转换 | 路径 | 守卫 |
|---|---|---|
| pause / cancel / 对端 cancel/pause / interrupt / accept / resume | `coordinator.dispatch` | ✅ epoch + terminal |
| **complete（收发）/ 接收方 fail / 策略 reject** | `mark_session_*` 直写 + 手动 `publish_projection` | ❌ 无 |

证据与后果：
- **真实竞态 bug**：`handle_cancel_impl`（`receive.rs:344`）异步 `dispatch(RemoteCancelled)` 写 terminal/cancelled；并发的接收后台若已进 `finish_data_channel` 会 `mark_session_completed`（`receiver.rs:525`）无条件写 terminal/completed，**把 cancelled 覆盖成 completed**。dispatch 路径本会因 `is_terminal` 返回 None 拒绝——这正是状态机要消灭的 bug。
- **死状态机分支**：`ActorReport::Completed/Progress/CheckpointFlushed`（`coordinator.rs:256/258`）生产从不 dispatch，只活在单测。
- **逃生口**：`coordinator.publish_projection`（`coordinator.rs:423`）存在的唯一理由是给 mark_* 旁路补发投影，让「唯一入口」名存实亡。
- **死函数**：`mark_session_cancelled`/`mark_session_transferring`/`pause_session`/旧版 `update_file_checkpoint` 零生产调用。
- 代码里反复出现的「消除收发不对称 / 与发送方对称」补丁注释，都是在用补丁追平一个本该由状态机统一的问题。

### 决策 D1：终态收口到 dispatch

**所有 session 级 phase 转换一律经 `coordinator.dispatch` → `reduce` → `apply_transition`**。文件级副作用（`mark_file_completed`、sink finalize、`reset_file_checkpoint`、inbox 索引）在 dispatch **之前**完成，不构成阻碍。

- 收发完成 → `dispatch(Actor{epoch, Completed})`（reduce 写 terminal/Completed + finished_at + 发 projection）。
- 接收方校验失败 → `dispatch(Actor{epoch, FatalError(msg)})`（与发送方 `send.rs:404` 对称）。
- 策略拒绝 → `create_session(offered)` 后 `dispatch(User{Reject})`。
- 删除 `publish_projection` 公开入口；projection 仅作为 `apply_input` 内 reduce 成功后的副作用发出。

收益：竞态 bug、死分支、双路径、逃生口、收发不对称**一并消失**。`ActorReport::Completed` 等死分支由此变活（生产真正 dispatch 它们）。

### 决策 D2：`epoch` 必须能取到

`dispatch(Actor{epoch, ...})` 需要 actor 当前 epoch。`SendSession`/`ReceiveSession` 已持 epoch（resume 装入时带），完成回调处可取。`apply_transition` 已是「session 已加载、不二次 SELECT」，complete 路径同理用已加载的 actor epoch，不退化性能。

## 决策 D3：抽公共，按职责拆 god-module

- `transfer/checkpoint.rs`：把 `receiver.rs:627-785` 的 bitmap/ranges 纯函数（`is_chunk_completed`/`count_completed_in_bitmap`/`ranges_from_bitmap`/`first_missing_range` 等）+ resume 的 checkpoint 计算（`build_fetch_plan`/`validate_checkpoint`）收到一处共用——二者本质同源。
- `range_within_file(file_size, range)` 单点实现边界数学，4 处校验（`receiver.rs:120/668`、`resume.rs:606`、`sender.rs:269`）各映射到自己的错误类型。
- `From<&PreparedFile> for FileInfo` + `From<&transfer_file::Model> for FileInfo`，消 5 处逐字段构造。
- `negotiate_resume(direction)` 模板方法：收 `probe→validate→key/epoch→commit→dispatch→rollback` 公共流程，仅把「fetch_plan 来源 + actor 重建」作为按 direction 分派点，消两个 `initiate_*` 80% 重复 + 与 `start_local_resume_actor` 第二份方向分发。拆 `resume/{validation,plan}.rs`。
- `receiver.rs`：`handle_block_data` 拆 `decrypt_and_validate`/`persist_chunk`/`emit_progress` 三段；writer 的 mpsc 桥简化为与 sender 对称的单 reader/单 writer 直写；发送终态副作用下沉 `SenderActor` 方法（与接收 `finish_data_channel` 对称），`data_plane` 回归纯路由。

## 决策 D4：统一命名与一致性

- `EpochGuard`：单点封装 epoch 比较，显式命名「准入 `>=`」与「迟到 `<`」两种语义，让 reducer/registry/data_plane/帧过滤都引用同一判定。控制消息的 epoch 缺口（`dispatch_network_current`）在协议帧带 epoch 前文档化为已知缺口，不长期默认。
- `SendSession`→`SenderActor`、`ReceiveSession`→`ReceiverActor`（与 registry/coordinator 的 actor 术语一致），访问器 `*_session`→`*_actor`。`session` 一词收敛为「逻辑会话 id」与「DB 行」。
- ActorRegistry 用泛型 `Registered<A: Cancellable>` 消 send/receive 复制，补 `remove_send_if_epoch` 对称；access 入口统一（全经 manager 包装或全直连，二选一）。
- 前端：删 `TransferSession`/`projectionToSession` 适配层，组件直消费 `TransferProjection`；phase/reason→文案只留 `projectionStatusLabel` 一处；`isActiveStatus`→`isProjectionActive`；`inbox` 接 store 消除 `selectedIdRef` 时序 workaround；`section-primitives` 提为通用布局原语供 inbox/devices 复用。

## 风险与边界

- 行为保持：除竞态 bug 修复（正确性增强），无外部行为变化。靠 `e2e_transfer.rs` 安全网逐轮回归，轮 1 新增并发取消-完成竞态测试坐实修复。
- 不动 entity/migration（无 schema 改）、不动 Tauri command 签名、不动 wire 协议（`BlockRequest` 从未上线）。
- 分轮独立可提交：每轮一个编译+测试+clippy(+tsc) checkpoint，参考 `harden-transfer-lifecycle` 节奏。
- 跨仓：core 内部重构（含 `SendSession`→`SenderActor` 重命名）多为 `pub(crate)`/内部，RN 桌面稳定后 re-sync + 按编译器提示修，不在本 change。
