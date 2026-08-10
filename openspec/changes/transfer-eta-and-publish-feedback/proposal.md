# 传输进度：补齐剩余时间，并让「正在保存」这一段可见

## Why

两个症状，同一个根源——**进度条讲完了故事的一半就停嘴了**。

### 1. 三端都算得出剩余时间，但几乎没人展示

`TransferProgressEvent` 自诞生起就带 `eta: Option<f64>`（`crates/transfer/src/progress.rs`），
三端 binding、三端 store 全都完整接了下来。可真正渲染它的只有两处：桌面
`src/components/transfer/session-panel.tsx` 与 Web `docs/app/app/_components/transfer-detail.tsx`
的详情主块。列表行、设备页的活跃传输、发送卡、Android 前台通知——用户实际盯着看的那些面
——一律只有「速度 · 百分比」。**移动端一处都没有**，全库 `eta` 只出现在一句注释里。

这不是新需求，是**实现欠着规格**：
`openspec/specs/transfer-detail-page/spec.md:16-22`、`openspec/specs/send-progress/spec.md:14-16`、
`openspec/specs/chunk-transfer/spec.md:65` 三份规格早就要求了剩余时间。

而「速度」单独存在时几乎没有决策价值：`12.4 MB/s` 要用户自己心算「还有多少字节 ÷ 它」才
变成「我还要等多久」。主流传输 UI 一律直接给后者。

### 2. 移动端接收的最后一段：进度条 100%，然后凭空多等几十秒

接收是「暂存 → 发布」两段式。`actor/receiver.rs` 的顺序是
`persist_chunk` → `emit_chunk_progress`（**这一帧把进度打到 100%**）→ `publish_file`（**零事件**），
下一条事件要等整个会话结束。中间这段 `publish_file` 在各端代价天差地别：

| 端 | publish 做什么 | 代价 |
|---|---|---|
| 桌面 | `<dst>/x.part` → 同目录 rename | O(1) |
| Web | OPFS `close()` | O(1) |
| iOS | staging → `Documents` 同容器 rename | O(1) |
| **Android** | **SAF 全量字节拷贝**（6 GB 文件要写 12 GB） | **O(size)** |

于是 Android 上必然出现「进度条满了、界面静止、什么都没说」的一段。用户对静止的解读是
卡死，而对卡死的反应是强杀应用——那恰好又会踩中 `transferred_bytes` 归零那条既有缺陷。

**`DESIGN.md:658-663` 的 Transfer Progress Contract 第 4 条早就写死了判据**——「任何超过
约 3 秒的本机阶段都要带百分比或 ETA」——并把 Android SAF publish copy 列为**尚未关闭的
open bug**（`:676-678`，那段还写着「关闭它的那个 PR 要顺手删掉这些句子」）。本 change 就是
那个 PR。

### 顺带暴露的两条「已有信息就是错的」

调查过程中发现两处比「缺信息」更糟的地方，同 PR 一并修（详见 design D5）：

- `ProgressTracker::speed()` 只读 `samples` 首尾，**不问它们有多旧**。传输停滞（正是 publish
  期间）时它会持续返回一个早已不成立的速率，ETA 跟着一起撒谎。
- `progress_event()` 的 200 ms 节流**无条件生效**，块间隔远小于它，所以「某文件刚好收齐」
  那一帧大概率被丢掉——UI 停在 99.x% 直接跳完成。

## What Changes

- **共享**：`packages/shared-view` 新增唯一的 ETA 格式化入口 `formatEta`（算不出返回 `null`，
  占位文案留给调用点）；移动端那份重复的私有 `formatDuration` 删除。
- **传输域**：新增 `FilePublishEvent` + `FilePublishPhase{Started, Finished}` 事件，在
  `publish_file` 前后各发一次；`speed()` 停滞归零；`progress_event(force)` 让文件边界那帧必达。
- **事件桥**：core / Web 壳 / 桌面壳 / 移动 uniffi 四条链路各加一节。
- **三端 UI**：ETA 进入列表行、设备页活跃区、发送卡、Android 前台通知；新增「正在保存」
  阶段表达，Android 带真百分比（数字取自 `copyIntoTarget` 循环里早就在数的 `written`）。
- **契约**：`DESIGN.md` 的 Transfer Progress Contract 补一张活跃进度信息位表与一张文案表，
  并删掉 Android SAF publish 那条已关闭的 open gap。

## 非目标

- **不给会话 `TransferPhase` 枚举加 `publishing`**（design D2）。
- **不加应用层加密、不碰 wire、不 bump `TRANSFER_DATA_PROTOCOL`**——新增事件全部是本地
  UI 事件，一个字节都不过网。
- **不做发送侧的对称提示**（「对方正在保存」）。发布期间接收循环整条挂起、不回 `Window`
  确认，发送端 `sync_window` 无超时干等，发送侧 UI 同样静止——但要修必须过网，超出本次范围。
  记为已知缺口（design D7）。
- **不给 prepare 阶段加 ETA**（`PrepareProgressEvent` 没有 speed/eta 字段，要动 Rust 事件与
  三端生成物，与本次两件事正交）。
- **不给逐文件行加 ETA**：`FileProgressInfo` 内核层就没有 per-file 速率。
- 不动 outboard rebuild 与 Web 端 zip 取回这两个同类 open gap（DESIGN.md 里保留）。

## 影响面

| 层 | 是否编译期强制 |
|---|---|
| `crates/transfer`（progress / events / receiver / sender） | — |
| `crates/core`（host.rs、event_adapter.rs） | ✅ 穷尽 match 必红 |
| `crates/web`（types.rs 三处） | ✅ 穷尽 match 必红 |
| `src-tauri`（events.rs / host/event_bus.rs / setup.rs） | ❌ **三处静默失败路径** |
| `mobile-core`（events.rs） | ❌ `map_event` 的 `_ => None` 会静默吞掉 |
| 三端前端 + 四份 catalog | ❌ |
| 生成物：桌面 bindings / web bindings + wasm / uniffi TS | ❌ 漏跑则该端拿不到类型 |
