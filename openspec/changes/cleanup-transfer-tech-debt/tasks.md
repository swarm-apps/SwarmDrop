# 实施分轮（每轮一个 编译+测试+clippy(+tsc) checkpoint，绿了再 commit）

> 安全网：`crates/core/tests/e2e_transfer.rs`（连通/单文件传输/重启清理/reject/remote-reason/peer-disconnect 共 6 个）+ 38 单元测试。
> 节奏参考已归档的 `harden-transfer-lifecycle`。轮 1-2 是 P0（病根 + 死代码），轮 3-5 是 P1（重复/拆分/一致性/前端）。

## 1. 病根：终态写入收口到 Coordinator（P0，含竞态 bug 修复）✅

- [x] 1.1 收发完成改 dispatch：`data_plane.rs`（发送）与 `receiver.rs::finish_data_channel`（接收）的 `mark_session_completed` → `dispatch(Actor{epoch, Completed})`；文件级 `mark_file_completed`/finalize 保留在 actor 内、在 dispatch 之前；完成事件 + 收件箱索引 gate 在 dispatch 返回 Some（被取消/旧 epoch 抢先则不发）
- [x] 1.2 接收方失败改 dispatch：`receiver.rs::fail_session` 的 `mark_session_failed` → `dispatch(Actor{epoch, FatalError(msg)})`（与发送方对称）；`finish_data_channel`/`fail_session` 线入 epoch
- [x] 1.3 策略拒绝改 dispatch：`receive.rs::record_rejected_inbound_offer` 的 `mark_session_rejected` → `dispatch(User{Reject})`（policy reason 已由 `set_session_policy_metadata` 持久化）
- [x] 1.4 终态 backfill 的 `publish_projection`（receiver.rs ×2 / data_plane / receive.rs reject）随 1.1-1.3 消失；`publish_projection` 保留并 **rescope** 为「新建会话首投影」（offered/waiting_accept，创建非 reduce 输入，合法不走 dispatch），doc 去掉逃生口表述。剩 2 个合法 creation 调用点
- [x] 1.5 **竞态回归测试** `e2e_terminal_irreversible_under_concurrent_complete_cancel`：两种到达顺序（cancel→complete / complete→cancel）下迟到的终态都被 `is_terminal` 守卫拒绝（reduce 返回 None），终态不被覆盖
- [x] 1.6 `cargo test -p swarmdrop-core`（78 单元 + 12 E2E）+ clippy（core+桌面壳）全绿；收发对称 Terminal/Completed projection 断言仍通过；`ActorReport::Completed` 分支由此变活

## 2. 清除迁移残留死代码（P0，低风险）

- [ ] 2.1 删 `ops.rs` 死函数：`mark_session_cancelled`(372)、`mark_session_transferring`(453)、`pause_session`(447)、旧版 `update_file_checkpoint`(131，非 ranges)；测试里仍用的 `mark_session_paused` 改为构造 suspended 经 apply_transition / 测试 helper，删之；`set_session_lifecycle`(随轮1完成后)仅余 mark_completed/failed/rejected 若已删则一并删
- [ ] 2.2 收敛 `CoordinatorInput`：`ActorReport::Completed` 经轮1变活；评估 `Progress`/`CheckpointFlushed`（无 caller）、`NetworkSignal::PeerOffline`（用 Interrupted 代替）、`UserCommand::Resume`（恢复走 ResumeCommitted）——接通或删除，不留无 caller 分支
- [ ] 2.3 删半成品死协议 `TransferDataFrame::BlockRequest`（`data_frame.rs:69`）+ `TAG_BLOCK_REQUEST`(25) + encode/decode/match 臂 + `sender.rs:229` 拒绝分支；TAG=3 空洞处加注释或紧凑重编号
- [ ] 2.4 改误导性注释：`receiver.rs:66/490`、`receive.rs:371`、`resume.rs:801`、`network/event_loop.rs:276` 引用已删的 `run_transfer`/`start_pulling`/`pull`/`chunk_request` → 改为推送式语义（`run_data_channel`/push/Finish）
- [ ] 2.5 前端死代码：删 `DeviceCard variant="list"` 整段（`device-card.tsx:112-216`，~104 行 + 重复 Unpair/Policy 弹窗挂载）；删 `StatusBadge` labels Record（`$sessionId.lazy.tsx:158`）+ `:175/207/299` 三元死兜底（projection 必填）；删 `statusTone.online/offline`（`device-card.tsx:46`）；删 `projectionToSession` 6 个死字段（`transfer-projection.ts:131`）
- [ ] 2.6 `cargo test` + clippy + `pnpm exec tsc --noEmit` 全绿

## 3. 后端抽公共 + 拆 god-module（P1）

- [ ] 3.1 新建 `transfer/checkpoint.rs`：把 `receiver.rs:627-785` 的 bitmap/ranges 纯函数 + resume 的 `build_fetch_plan`/`validate_checkpoint` 收一处共用（消同源重复，纯函数可独立单测）
- [ ] 3.2 `range_within_file(file_size, range) -> Result<(), RangeError>` 单点边界数学，4 处校验（`receiver.rs:120/668`、`resume.rs:606`、`sender.rs:269`）改用，各映射到 AppError/ResumeRejectReason
- [ ] 3.3 `From<&PreparedFile> for FileInfo` + `From<&transfer_file::Model> for FileInfo`，5 处逐字段构造（`sender.rs:246`、`send.rs:52`、`resume.rs:494/789`、incoming）改 `.map(FileInfo::from)`
- [ ] 3.4 拆 `resume.rs`：`negotiate_resume(direction)` 模板方法消两个 `initiate_*` 80% 重复（`resume.rs:30/113`）+ 与 `start_local_resume_actor` 统一 actor 重建；拆 `resume/{validation,plan}.rs`；复用 `parse_peer_id`（删 `send.rs:71` 内联）+ `save_location` helper（消 `resume.rs:51/438` 重复）
- [ ] 3.5 拆 `receiver.rs` god-module：`handle_block_data` 拆 `decrypt_and_validate`/`persist_chunk`/`emit_progress` 三段；bitmap 纯函数移 `checkpoint.rs`（随 3.1）；receiver writer 的 mpsc 桥简化为与 sender 对称的单 reader/单 writer 直写（删 `is_terminal` Abort 死分支）
- [ ] 3.6 `data_plane`↔actor 终态边界归一：发送终态副作用下沉 `SenderActor::finish/on_interrupted`，`data_plane.rs` 回归纯路由（与接收 `finish_data_channel` 对称）
- [ ] 3.7 后端去重收尾：`cache_inbound_offer` vs `record_rejected_inbound_offer` 抽共享落库（`receive.rs:31/83`）；`policy` 纳入 `CreateSessionInput` 删 `set_session_policy_metadata` 二次写（`ops.rs:411`）
- [ ] 3.8 `cargo test` + clippy 全绿

## 4. 后端一致性与命名收口（P1）

- [ ] 4.1 `EpochGuard` 单点封装 epoch 比较，显式命名「准入 `>=`」「迟到 `<`」；reducer/actor_registry/data_plane/帧过滤改引用；`dispatch_network_current` 的 stale 缺口文档化为已知项
- [ ] 4.2 `SendSession`→`SenderActor`、`ReceiveSession`→`ReceiverActor`，访问器 `get/insert/remove_send_session`→`*_actor`；`session` 收敛为「逻辑 id / DB 行」
- [ ] 4.3 ActorRegistry 泛型 `Registered<A: Cancellable>` 消 send/receive 复制（`actor_registry.rs:41/56`），补 `remove_send_if_epoch` 对称；发送完成路径改用它（修 `data_plane.rs:85` 无 epoch 守卫误删风险）；access 入口统一
- [ ] 4.4 `sync_session_transferred_bytes` 下沉为 projection 派生（`get_transfer_projection` 时 SUM files），删 4 处手工 sync（`receive.rs:212/395/439`、`send.rs:295`）+ 雷同 workaround 注释
- [ ] 4.5 命名/可见性收口：pause/suspend/paused 术语在类型注释固定映射；resume.rs 6 个仅本文件用的 `pub(crate)` helper 降私有；删只用一次的 `generate_id()` 包装
- [ ] 4.6 `cargo test` + clippy + workspace check（含桌面壳）全绿

## 5. 前端清理与一致性（P1）

- [ ] 5.1 删 `TransferSession`/`projectionToSession` 适配层，组件直消费 `TransferProjection` + 实时 progress；派生值（百分比/isActive/文案）走以 projection 为入参的纯函数
- [ ] 5.2 状态→文案单源：phase/reason→文案只留 `projectionStatusLabel`（`transfer-projection.ts:50`），删各组件内联文案（`传输完成`/`传输失败`/`已暂停`/`准备中` 等）；`isActiveStatus`→`isProjectionActive`
- [ ] 5.3 `inbox` 接入 store（`inbox-store` 或 `useInboxList/useInboxDetail` hook），消除 `selectedIdRef` + `loadItems` 返回值时序 workaround（`inbox/index.lazy.tsx:52`）
- [ ] 5.4 `section-primitives`（SectionShell/EmptyPanel + 补 CenteredEmptyState）提为通用布局原语（移到 `components/layout` 或 `components/ui`），inbox/add-device-section 复用，删 4+ 处手搓 glass-panel/空态
- [ ] 5.5 共享小工具：`PolicyReasonBadge`（offer 弹窗 + history-item 复用，统一文案/门槛）；`getFileIcon`/`EXT_ICON_MAP` 提 `lib/file-icon.ts`（inbox ItemIcon 复用）；`getDeviceIcon` 统一喂 `device.os`；inbox 详情时间改 `formatRelativeTime`；store 的 event→toast 副作用拆 `transfer-notifications.ts`
- [ ] 5.6 offer 弹窗 `shiftOffer` 移出 finally（accept/reject 成功后再出队，失败保留供重试）
- [ ] 5.7 `pnpm exec tsc --noEmit` + `pnpm i18n:extract`（如改文案）+ 构建通过

## 6. 收尾验证

- [ ] 6.1 `cargo test --workspace` + `cargo clippy --workspace -- -D warnings` 全绿
- [ ] 6.2 `pnpm exec tsc --noEmit` + 前端构建通过
- [ ] 6.3 `/simplify` 复审本次累积 diff，确认无新增重复/死代码
- [ ] 6.4 更新 `dev-notes/knowledge/rust-backend.md`：把「终态统一经 dispatch、禁止 mark_* 直写终态」沉淀为约定，删除已过时的「mark_* 过渡桥接」条目
