## Why

传输子系统经过 `redesign-transfer-lifecycle`、`add-p2p-data-channel`、`trusted-device-policies`、`harden-transfer-lifecycle` 多个 change 接力实现后，**架构内核是好的**（reducer 纯函数、平台中立守得住、协议单一无双数据通路、前端 projection 单源），但留下了一次**没收口的迁移**和**多人接力的清理债**：

1. **状态机迁移只做了一半（病根）**：`TransferCoordinator.dispatch` 被文档声明为「状态变化的唯一持久化入口」，且现有 spec 明确要求「Actor MUST NOT 绕过 Coordinator 直接决定 session phase」。但 `complete` / 接收方 `fail` / 策略 `reject` 三类终态仍走旧 `mark_session_*` 直写 DB，**绕过 reducer 的 epoch 守卫和 terminal 不可逆守卫**，与 pause/cancel/interrupt/resume 走 dispatch 的路径并存。这不仅是「不统一」，还引入一个**真实竞态 bug**：对端取消（dispatch 写 terminal/cancelled）与并发的接收完成（mark 直写 terminal/completed）会互相覆盖，而走 dispatch 本会被 `is_terminal` 守卫拒绝。

2. 由病根衍生出一批**死代码**（3 个 `ActorReport` reducer 分支、4 个零调用 `mark_session_*` 函数、`publish_projection` 逃生口、`TransferDataFrame::BlockRequest` 半成品协议、前端 `DeviceCard variant="list"` ~104 行、`StatusBadge` labels 死兜底等）。

3. 一批**职责过载与重复**：`receiver.rs` 879 行 god-module（`handle_block_data` 115 行揉 6 件事）、`resume.rs` 1003 行（两个 `initiate_*` 80% 复制）、range 校验 4 处、`FileInfo` 构造 5 处、前端 `TransferSession` 是 `TransferProjection` 的有损镜像逼组件吃双源、状态→文案映射散落 3 处且互相打架。

4. 一批**命名/可见性不统一**：epoch 校验散落 5 处 4 种算子、actor 与 session 双术语指同一对象、pause/suspend/paused 三词混指、ActorRegistry 访问半包装半直连。

诊断依据：7 路并行架构审查（见 design.md「审查结论」），其中 4 路独立指向同一病根。

## What Changes

- **终态写入收归状态机**：`complete` / 接收方 `fail` / 策略 `reject` 改为经 `coordinator.dispatch(Actor{Completed}/Actor{FatalError}/User{Reject})`，文件副作用（落盘、checkpoint、inbox 索引）留在 actor 内，但 session 级 phase 一律经 `apply_transition`。删除 `publish_projection` 旁路逃生口。**修复终态竞态覆盖 bug**。
- **清除迁移残留死代码**：删零调用的 `mark_session_*` 原语、`set_session_lifecycle` 桥接、无 caller 的 `ActorReport`/`NetworkSignal`/`UserCommand` 变体、半成品 `BlockRequest` 帧、引用已删函数的误导性注释；删前端 `DeviceCard` list 死分支、`StatusBadge` 死兜底、`statusTone` 死配置、6 个 projection 死字段。
- **抽公共消除重复**：后端 `transfer/checkpoint.rs`（bitmap/ranges 纯函数，receiver + resume 共用）、`range_within_file` 单点校验、`From<&PreparedFile/Model> for FileInfo`、`negotiate_resume` 模板方法、拆 `receiver.rs`/`resume.rs` god-module；前端删 `TransferSession`/`projectionToSession` 适配层让组件直消费 projection、状态→文案单源、`section-primitives` 提为通用原语、`inbox` 接入 store。
- **统一命名与可见性**：抽 `EpochGuard` 单点；`SendSession`/`ReceiveSession` → `SenderActor`/`ReceiverActor`；ActorRegistry 泛型化消复制 + 对称 API；pause/suspend 术语文档化收敛；`pub(crate)`/私有 fn 可见性按「跨文件才 pub(crate)」对齐。

不引入新的外部行为（除竞态 bug 修复属正确性增强）。无 DB schema 变更、无 Tauri command 签名变更、无 wire 协议变更（`BlockRequest` 从未上线，删除不影响兼容）。

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `transfer-lifecycle-state`：强化「终态写入单一入口」不变量——把现有 spec 已声明但代码未落实的「actor 不绕过 Coordinator 决定终态」从口头要求变成强制约束（禁止 `mark_session_*` 直写终态、终态在并发完成下不可逆）。

## Impact

- 影响 crates：`crates/core`（transfer 全模块 + database/ops）、`src`（前端 transfer/inbox/devices）。`crates/entity` / `crates/migration` 不变（无 schema 改动）。
- 影响模块：`coordinator` / `receiver` / `sender` / `data_plane` / `resume` / `receive` / `send` / `incoming` / `actor_registry` / `data_frame` / `database/ops`；前端 `transfer-store` / `transfer-projection` / `routes/_app/transfer` / `inbox` / `devices/-components`。
- 行为影响：仅竞态 bug 修复改变正确性（取消后并发完成不再覆盖 cancelled）；其余为行为保持的重构 + 死代码删除。
- 验证靠现成的 `crates/core/tests/e2e_transfer.rs` E2E 安全网（连通/单文件传输/重启清理/reject/remote-reason/peer-disconnect），每轮编译 + `cargo test -p swarmdrop-core` + clippy + 前端 `tsc` 兜底；轮 1 新增并发取消-完成竞态测试。
- 跨仓影响（SwarmDrop-RN）：改的是共享 `crates/core` 内部实现（删 `mark_session_*`、`SendSession`→`SenderActor` 重命名、`BlockRequest` 删除），但这些多为 `pub(crate)`/内部细节，uniffi 导出面预计基本不变；移动端按既定策略在桌面稳定后于 SwarmDrop-RN 仓 re-sync core git rev 并按编译器提示修内部引用，不在本 change 范围。
