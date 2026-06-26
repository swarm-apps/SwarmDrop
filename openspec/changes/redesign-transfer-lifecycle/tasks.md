## 实施阶段（Phase A / B）

本 change 内部分两阶段实施，解耦对 `add-p2p-data-channel` 的依赖（详见 design.md Migration Plan）：

- **Phase A（不依赖 add，可并行）**：第 1–4、6、7 节——Coordinator、phase+reason、epoch、探测式恢复、DB 单源、前端投影。actor 的数据搬运层**暂用现有 req_resp ChunkRequest**，行为与现状一致。
- **Phase B（依赖 add）**：第 5 节——把 actor 搬运层从 req_resp 换成 hybrid 推流 transfer-data 帧协议（over data channel）。Coordinator / epoch / 恢复协议 / DB 投影不变。

## 1. 前置条件与模型设计

- [x] 1.1 仅 Phase B 依赖 `add-p2p-data-channel`；Phase A 在现有 req_resp 数据搬运上完成，可与 add 并行（无需等待 data channel 或 mock）
- [x] 1.2 定义新的 transfer phase、suspended reason、terminal reason、recoverable、epoch 等实体枚举
- [x] 1.3 设计 transfer projection DTO，覆盖列表页、详情页、进度和操作按钮所需字段
- [x] 1.4 设计旧状态迁移策略：开发期清理旧历史或提供一次性 migration

## 2. 数据库与持久化

- [x] 2.1 更新 `crates/entity` 中 transfer session/file 模型，加入 phase、reason、epoch、recoverable、source fingerprint 等字段
- [x] 2.2 新增或更新 migration，完成旧 schema 到新生命周期 schema 的迁移
- [x] 2.3 重构 database ops，提供按 Coordinator event 写入 session、file、checkpoint 和 projection 的 repository API
- [ ] 2.4 可选新增 transfer event log，用于记录状态转换、epoch、错误和恢复协商证据
- [ ] 2.5 更新启动清理逻辑，将遗留 active session 转为 recoverable suspended，而不是 paused/failed 混用

## 3. TransferCoordinator

- [x] 3.1 新建 `TransferCoordinator` 模块，定义用户命令、网络事件、actor 事件和状态 reducer
- [ ] 3.2 实现 actor registry，统一管理 SenderActor / ReceiverActor 的创建、替换、取消和 epoch 校验
- [ ] 3.3 将 pause、cancel、complete、fail、peer disconnected 等路径改为进入 Coordinator
- [x] 3.4 实现前端 projection 事件发布，替换旧的分散 transfer events
- [x] 3.5 添加旧 epoch actor event 被忽略的单元测试

## 4. 恢复协调协议

- [x] 4.1 更新 `crates/core/src/protocol.rs`，新增 `ResumeProbe`、`ResumeStateReport`、`ResumeCommit`、`ResumeAck` 控制消息
- [ ] 4.2 移除或废弃旧 `ResumeRequest` / `ResumeOffer` 双入口恢复路径
- [x] 4.3 实现 ResumeProbe handler，返回本端 phase、epoch、manifest、checkpoint、source fingerprint 和 terminal marker
- [ ] 4.4 实现 ResumeCommit handler，校验新 epoch、manifest、checkpoint 和 transfer key 信息
- [ ] 4.5 实现恢复拒绝原因映射：cancelled、fatal error、source modified、checkpoint invalid、peer unavailable
- [ ] 4.6 添加恢复协议测试：正常恢复、对端已取消、源文件变更、旧 epoch、checkpoint 越界

## 5. 数据面帧协议（fetch_plan 驱动的 hybrid 推流）— Phase B，依赖 add-p2p-data-channel

- [ ] 5.1 定义 transfer-data frame 类型：`Hello`、`BlockData{range,ciphertext}`、`Ack{checkpoint_offset}`、`BlockRequest{range}`（仅补洞）、`Abort`、`Finish`；在应用层用 length-prefix 加帧（复用 unsigned-varint / asynchronous-codec，不写进 libs/core）
- [ ] 5.2 实现 frame 编解码，包含长度限制、错误处理和协议版本字段
- [ ] 5.3 实现数据通道 Hello 握手，校验 session_id、epoch、role、manifest_digest，并确定本 epoch 的 `fetch_plan`（来自 Offer/Accept 或 ResumeCommit）
- [ ] 5.4 将 SenderActor 改为按 `fetch_plan` 顺序连续推送加密 `BlockData`，不逐块等待请求；写 `BlockData` 与读 `Ack`/`BlockRequest` 走独立 task（读写分离）
- [ ] 5.5 将 ReceiverActor 改为读 `BlockData` 落盘校验、按稀疏策略（每 N 块 / T 秒）发 `Ack` 推进 checkpoint；仅乱序 / 校验失败 / 缺口时在同一条流上发 `BlockRequest` 补洞
- [ ] 5.6 实现整个传输复用单条数据通道（规避 muxer 开流 silent-drop）；checkpoint 以 ranges 存储，只计入已落盘且整帧校验通过的 range
- [ ] 5.7 实现 `Abort` / `Finish` 与 Coordinator 状态转换的集成；数据通道因 `ConnectionLost` 中断时 flush checkpoint 并投影为 recoverable suspended
- [ ] 5.8 配置数据面传输：QUIC 优先路径；TCP/yamux 路径确保 rust-yamux v0.13+ 自适应窗口、不覆盖窗口

## 6. 命令、事件与前端适配

- [ ] 6.1 重构 Tauri transfer commands：pause/resume/cancel/start 只调用 Coordinator，不直接操作 sender/receiver
- [ ] 6.2 更新 specta bindings，导出新的 projection、phase、reason、resume result 类型
- [ ] 6.3 重构 `transfer-store`，使用后端 projection 作为唯一状态来源
- [ ] 6.4 更新传输列表和详情页文案：已暂停、对方暂停、已中断、对方离线、可恢复失败、已取消、不可恢复失败
- [ ] 6.5 移除前端对 active sessions 与 DB history 的自行合并逻辑

## 7. 验证

- [ ] 7.1 添加 Coordinator reducer 单元测试，覆盖所有 phase/reason 转换
- [~] 7.2 添加双端集成测试：正常传输（✅ `crates/core/tests/e2e_transfer.rs`：连通性 + 单文件传输 happy path 已落地通过）、用户暂停、网络中断、应用重启、恢复成功、取消后拒绝恢复（剩余场景待补，harness 已就绪）
- [ ] 7.3 添加数据面测试：旧 epoch Hello 被拒绝、数据通道中断后 checkpoint 保留、Finish 后 completed
- [ ] 7.4 运行 `cargo test` 覆盖 `crates/core`、`crates/entity`、`migration`、`src-tauri`
- [ ] 7.5 运行前端类型检查和构建，确认新 bindings 与 UI 投影一致

---

## 实施分轮路线图（增量过渡，后续接着做）

> 后端状态机核心骨架已完成（commit `4935cb6` / `2ce4440` + simplify 审查）。剩余按下列分轮推进，
> 每轮一个「编译 + 测试」checkpoint，参考 `add-p2p-data-channel` 的节奏：一轮一个 task 组、绿了再 commit。
> Phase A（轮 4-6 + 8 大部分）不依赖数据通道；Phase B（轮 7）依赖已完成的 `add-p2p-data-channel`。

### ✅ 已完成（轮 1-3）

- **轮 1 — DB 层**：entity `phase/reason/epoch/recoverable/source_fingerprint` + 3 枚举 + `TransferPhase::legacy_status` 桥接 + migration（开发期清空旧史）。`create_session` 适配。→ task 1.1-1.4 / 2.1-2.2
- **轮 2 — 状态机核心**：`coordinator.rs` 纯函数 reducer（epoch 校验 + terminal 不可逆，10 单元测试）+ `TransferCoordinator::dispatch` + `TransferProjection` DTO + repository（`apply_transition` / `get_transfer_projection(s)` / `load_sessions_with_files`）。→ task 3.1 / 3.5 / 1.3 / 2.3
- **轮 3 — DB 接线准备**：`mark_session_*` 经 `set_session_lifecycle` 双写 phase/reason，DB 两种表示一致（不改调用点）。simplify 4-agent 审查应用 6 项修复。

### 轮 4 — Coordinator 完整接线（后端）→ task 3.2 / 3.3 / 3.4 / 2.5

- 3.2 Coordinator 持有 `db + event_bus + actor registry`（管理 Sender/ReceiverActor 创建/替换/取消/epoch 校验）
- 3.3 把 17 个 `mark_session_*` 调用点（send/receiver/receive/resume/incoming）逐点映射为 `dispatch(session_id, CoordinatorInput)`：completed→`Actor{Completed}`、failed→`Actor{FatalError}`、本地 cancel→`User{Cancel}`、对端 cancel→`Network{RemoteCancelled}`、本地 pause→`User{Pause}`、对端 pause→`Network{RemotePaused}`、恢复→`Network{ResumeCommitted}`
- 3.4 dispatch 转换成功后 `get_transfer_projection` + `publish(CoreEvent::TransferProjection)`；新增 `CoreEvent::TransferProjection`（host.rs 已 `#[non_exhaustive]`，桌面 `host/event_bus.rs` 加 emit `"transfer-projection"`）
- 2.5 启动清理：遗留 active → `dispatch(Startup(FoundActiveSession))` → suspended/app_restarted
- **风险**：17 点语义各异，逐点改 + 每点 `cargo test`；本地 vs 对端 pause 的 reason 区分在此落实
- **验证**：`cargo test` + 现有传输流程不回归

### 轮 5 — 恢复协议 → task 4.1-4.6

- protocol.rs 新增 `ResumeProbe`/`ResumeStateReport`/`ResumeCommit`/`ResumeAck` + `FileRange`/`FileCheckpoint`/`ResumePhaseReport`
- incoming.rs match arm + handler（ResumeProbe→报告本端 phase/epoch/checkpoint/fingerprint；ResumeCommit→校验 + `dispatch(ResumeCommitted)` 生成新 epoch）
- 废弃 `ResumeRequest`/`ResumeOffer` 双入口（resume.rs ~580 行）
- **验证**：恢复协议单元测试（正常/对端取消/源变更/旧 epoch/checkpoint 越界，task 4.6）

### 轮 6 — 前端切换（需运行验证）→ task 6.1-6.5

- 6.1 Tauri commands pause/resume/cancel/start 只调 Coordinator
- 6.2 specta bindings 导出 TransferProjection/phase/reason（`pnpm tauri dev` 重新生成 `bindings.ts`，勿手改）
- 6.3-6.5 `transfer-store` 弃 sessions+dbHistory 双源、改消费单一 projection 事件；列表/详情页文案（已暂停/对方暂停/已中断/对方离线/可恢复失败/已取消/不可恢复失败）
- **跨仓**：RN 端 `src/stores/transfer-store.ts` + `mobile-core/transfer.rs` 平行同步（SwarmDrop-RN 仓，桌面稳定后）
- **验证**：`pnpm exec tsc --noEmit` + `pnpm tauri dev` 实跑

### 轮 7 — 数据面 Phase B（依赖 add-p2p-data-channel ✅）→ task 5.1-5.8

- transfer-data 帧（Hello/BlockData/Ack/BlockRequest/Abort/Finish）over data channel（应用层 length-prefix 加帧）
- SenderActor 按 fetch_plan 连续推 + ReceiverActor 稀疏 Ack + 读写分离 + 单流复用；checkpoint 改 ranges
- QUIC 优先 / TCP 路径确保 yamux 自适应窗口
- **验证**：旧 epoch Hello 拒绝、中断后 checkpoint 保留、Finish→completed（task 7.3）

### 轮 8 — 集成验证 + 清理 → task 7.1-7.5 / 2.4

- 双端集成测试：正常/暂停/中断/重启/恢复/取消后拒绝恢复（task 7.2）——**纯 cargo test E2E harness**（两个真实 `start()` 节点 + 关 mDNS + 显式 dial + `MemoryHost` + `sqlite::memory:`，零生产代码改动、不需真机；方案见 dev-notes/knowledge/rust-backend.md「端到端集成测试」。中断=drop event_loop task，重启=同 db 重 spawn）。**✅ harness 已落地** `crates/core/tests/e2e_transfer.rs`（连通性 + 单文件传输 happy path 通过），剩余场景（暂停/中断/重启/恢复/取消后拒绝）顺着接线进度逐个补——这张安全网让轮 4/5 的接线每改一处都能跑 E2E 验证不破坏端到端
- 删除旧 `SessionStatus` 路径 + `legacy_status` 桥接 + `mark_session_*` 双写（迁移完成后回收）
- 可选 2.4 transfer event log
- `cargo test` 全覆盖 + 前端构建（task 7.4 / 7.5）

> 注：task 7.1（reducer 单元测试）已在轮 2 以 10 个测试部分覆盖，轮 4 接线后补全 phase/reason 全转换。
