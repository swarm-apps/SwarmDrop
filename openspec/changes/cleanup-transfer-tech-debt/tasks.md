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

- [x] 2.1 删 `ops.rs` 6 个真死函数：`mark_session_failed`/`cancelled`/`rejected`/`transferring`、`pause_session`、旧版 `update_file_checkpoint`（非 ranges）。保留 `mark_session_completed`/`mark_session_paused`（仅测试 fixture）+ `set_session_lifecycle`（被 fixture + 745 的 reap 用）+ `update_session_terminal`（被 paused 用）。`legacy_status` 仍 load-bearing（apply_transition 写 status 列），随 status 列下线再删
- [ ] 2.2 收敛 `CoordinatorInput`（**待后续**，判断题）：`ActorReport::Completed` 经轮1变活；`Progress`/`CheckpointFlushed`（无 caller）、`NetworkSignal::PeerOffline`、`UserCommand::Resume`——决定接通 actor 进度上报 vs 删除（涉及 reduce arm + 单测，单独处理）
- [x] 2.3 删半成品死协议 `TransferDataFrame::BlockRequest` + `TAG_BLOCK_REQUEST` + encode/decode/2 个 match 臂 + `sender.rs` 拒绝分支；TAG 3/4 空洞处加注释说明已废弃、不复用
- [x] 2.4 改 5 处误导性注释：`receiver.rs`(start_pulling/run_transfer)、`receive.rs`(pull)、`resume.rs`(chunk_request)、`event_loop.rs`(pull) → 推送式语义（run_data_channel/push）
- [ ] 2.5 前端死代码（**待后续**，归入轮5 前端清理一并做）：`DeviceCard variant="list"` ~104 行 + 重复弹窗、`StatusBadge` labels + 三元死兜底、`statusTone.online/offline`、`projectionToSession` 6 死字段
- [x] 2.6 backend 部分：`cargo test`（78 单元 + 12 E2E）+ clippy（core+桌面壳）全绿（前端 2.5 随轮5）

## 3. 后端抽公共 + 拆 god-module（P1）

- [x] 3.1 新建 `transfer/checkpoint.rs`：把 receiver 的 10 个 bitmap/range 纯函数（ensure_files_complete/first_missing_range/chunk_range/validate_block_range/is/mark_chunk/count/bytes/ranges_from_bitmap）+ 其测试移出共用，receiver.rs 903→654 行。注：复核发现 resume 的 sender checkpoint 用 transferred_bytes（非 bitmap），与 bitmap 数学不同源、无重复可消，review「同源」是概念相似
- [ ] 3.2 range 校验 dedup（**精化/降级**）：复审发现 4 处（`receiver.rs` validate_fetch_plan/validate_block_range、`resume.rs` validate_fetch_plan、`sender.rs` write_range）的 length==0/empty-file/chunk 对齐规则**各有意义不同**（resume 无条件拒 length 0、sender 特判 size==0、receiver 叠 chunk 对齐），真正共享的只有「`offset+length` 溢出 + `end>file_size`」薄核心。全量共享 validator 会改行为；价值有限，仅在做 3.5 receiver 拆分时顺手抽 `range_end_within(file_size, range)->Result<u64,_>` 薄核心，各处保留自己的 length-0 规则
- [x] 3.3 `From<&PreparedFile> for FileInfo` + `From<&transfer_file::Model> for FileInfo`（manager.rs，集中 FileInfo 两个唯一构造来源）；4 处生产构造（`send.rs`、`sender.rs::file_manifest`、`resume.rs::build_resume_manifest`、`resume.rs` build_file_infos）改 `.map(FileInfo::from)`。测试 fixture 不动
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
