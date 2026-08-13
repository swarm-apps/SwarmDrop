# transfer-eta-and-publish-feedback 任务分解

> 语言约定：所有新增注释与文档一律中文。
> **阶段顺序不可调换**：1（共享格式化）与 2（传输域）可并行；3（事件桥）依赖 2；
> 4/5/6（三端 UI）依赖 3，彼此零文件重叠、可并行；7 收尾必须最后跑。

## 1. 共享格式化（`packages/shared-view`）

- [x] 1.1 `src/format/time.ts` 新增 `formatEta(seconds: number | null | undefined): string | null`：
      null / 非有限 / 负数一律返回 `null`；否则向上取整到 5 秒（<60s）或 10 秒（≥60s）
      的整数倍后交给 `formatDuration`（design D1）
- [x] 1.2 `src/format/index.ts` 导出（`src/index.ts` 已整体再导出，不动）
- [x] 1.3 `src/format/time.test.ts` 补用例：null / undefined / NaN / Infinity / 负数 / 粗化边界
      （59→60s、61→70s）/ 0
- [x] 1.4 **只用三端 lib 的交集**（`Number.isFinite` / `Math.ceil`），不用 `replaceAll` 一类
      只有部分端有的 API；改完跑 `pnpm check:shared-view`
- [x] 1.5 删除 `mobile/src/app/transfer/[sessionId].tsx` 里的私有 `formatDuration`，改从
      `@swarmdrop/shared-view` 导入。⚠️ 坏数据分支的显示从 `-` 变 `0s`，扫一遍同文件里
      完成态「用时」的调用点

## 2. 传输域（`crates/transfer`）

- [x] 2.1 `src/progress.rs` 新增 `FilePublishPhase { Started, Finished }`（unit 变体，
      三端 codegen 都稳）与 `FilePublishEvent { session_id, file_id, name, relative_path,
      total_bytes, phase }`，derive 组合照 `TransferProgressEvent`
- [x] 2.2 `src/progress.rs` 的 `speed()`：末样本年龄 ≥ `SPEED_WINDOW` 时返回 `0.0`
      （design D5-1）。**取「判末样本年龄」而非「修剪 samples」**——`speed()` 是 `&self`
      且被 `eta()` 复用
- [x] 2.3 `src/progress.rs` 的 `progress_event()` 加 `force: bool` 逃逸口；`force` 时跳过
      200ms 节流但仍刷新 `last_emit`
- [x] 2.4 单测：停滞超过窗口后 `speed() == 0.0` 且 `eta().is_none()`
- [x] 2.5 单测：节流窗口内 `progress_event(false)` 返回 None、`progress_event(true)` 返回 Some
- [x] 2.6 `src/events.rs` 的 `TransferEvent` 加 `FilePublish { event: FilePublishEvent }`
- [x] 2.7 `src/actor/receiver.rs` 的 `publish_file`：`finalize_sink` **之前**发 `Started`、
      `mark_file_completed` **之后**发 `Finished`。⚠️ **绝不夹在两者之间**（design D2）。
      零签名改动——`self.events` / `self.session_id` 已在 actor 上
- [x] 2.8 `src/actor/receiver.rs` 的 `emit_chunk_progress` 用 `update_file_chunk` 的返回值当 force。
      ⚠️ **落地时改了判据**：返回值语义是「整个**会话**收齐」而不是「该文件收齐」——按文件判
      会退化成 O(N²)（任何 ≤`CHUNK_SIZE` 的文件都只有一块 ⇒ 每个小文件强制一帧 × 每帧克隆整个
      `files` 向量）。护栏是 `mid_session_file_completion_should_not_force_a_frame`
- [x] 2.9 `publish_pending_empty_files` **不发**发布事件。落地判据是尺寸
      （`PUBLISH_ANNOUNCE_MIN_BYTES = 1 MiB`）而非 `size == 0`：小文件的发布远快于 300ms 的
      揭示阈值，事件必然被前端丢弃，发了只是白推几万条 IPC 消息（已知限制见 design D8.1）
- [x] 2.10 `src/actor/sender.rs`：文件边界同样 force 一帧（同因同果）
- [x] 2.11 `src/probe.rs` 的 `RECV_LABELS` 从 `rest` 拆出 `publish` 桶，让真机日志能把发布
      耗时与簿记耗时分开

## 3. 事件桥（四条链路）

- [x] 3.1 `crates/core/src/host.rs`：`CoreEvent` 加 `FilePublish { event: FilePublishEvent }`
- [x] 3.2 `crates/core/src/event_adapter.rs`：加映射臂（穷尽 match，漏了必红）
- [x] 3.3 `crates/web/src/types.rs` **三处**：`WebTransferEvent` 变体 + `type_name()` +
      `From<TransferEvent>`（后两处穷尽 match，漏了必红）
- [x] 3.4 `src-tauri/src/events.rs`：新增 `FilePublish` newtype（照 `TransferProgress` 的
      `#[serde(transparent)]` 写法）
- [x] 3.5 `src-tauri/src/host/event_bus.rs`：加转发臂。⚠️ 该 match 有 `_ => {}` 兜底，
      **漏改静默丢事件**
- [x] 3.6 `src-tauri/src/setup.rs` 的 `collect_events![]` 注册。⚠️ **漏改不报错，emit 时才炸**
- [x] 3.7 `mobile-core/src/events.rs`：`MobileFilePublish` uniffi Record + `MobileCoreEvent`
      变体 + `map_event` 臂。⚠️ 该 match 有 `_ => return None`，**漏改静默吞掉**
- [x] 3.8 护栏：两条兜底分支从 `_ => {}` / `_ => None` 改成带 `warn!` 的具名臂，另加一条
      `file_publish_should_survive_the_mobile_mirror` 镜像测试。
      ⚠️ **表驱动的全变体护栏没做**——审查建议的更深解法是删掉 `CoreEvent` 的
      `#[non_exhaustive]`（本仓没有外部 host，三个消费者都是 path 依赖），那会让两条桥变成
      编译期穷尽、`warn!` 与这条手写测试一起消失。属独立改动，未在本次范围内

## 4. 桌面 UI（`src/`）

- [x] 4.1 `src/stores/transfer-store.ts`：加 `publishingBySession` 域；`Started` 写入、
      `Finished` 删除；收到该会话的 Failed / Paused / 非 active Projection 时清空
- [x] 4.2 `src/components/transfer/session-panel.tsx` 的 active 分支：ETA 改走 `formatEta`，
      算不出显示「计算中」占位（**不再整块消失**）；补 `completedFiles/totalFiles`；
      加 `aria-live="polite"`
- [x] 4.3 同文件新增 publishing 分支：「正在保存 {文件名}」+ `tone="local"` 进度条
- [x] 4.4 `src/routes/_app/transfer/-session-row.tsx` 的 active 分支右列改为
      「速度（仅 ≥920px） · 剩余 X · N%」；publishing 时状态词换成「正在保存…」。
      改这一处同时覆盖设备页「正在传输」区（同一个 `SessionRow`）
- [x] 4.5 ⚠️ `SessionRow` 是 `memo` 且 200ms 一帧，新增 prop 必须是稳定引用；
      `pnpm check:zustand-access` 规则 B 禁止 selector 派生新对象
- [x] 4.6 `src/locales/{zh,zh-TW,en}/messages.po`：新增「计算中」「正在保存 {0}」「正在保存…」；
      **「剩余 {0}」复用既有条目，不要新造**

## 5. Web UI（`docs/app/app/`）

- [x] 5.1 `_lib/store.ts`：消费新事件 + `publishingBySession` 域（TS 侧不会因缺失而编译失败，
      人为核对）；「内容没变」要 `return s` 不是 `return {}`
- [x] 5.2 `_lib/view-types.ts`：再导出新事件类型
- [x] 5.3 `_components/transfer-detail.tsx`：ETA 改走 `formatEta` + 占位，**同时改写**
      「算不出就整格省掉」那段注释；新增 publishing 分支
- [x] 5.4 `_components/active-transfers-section.tsx`：阶段行右端空位挂 ETA
- [x] 5.5 `_components/transfer-activity-panel.tsx`：右列改判为「active → 剩余 X，其余 →
      相对时间」，**并改写那段取舍注释**
- [x] 5.6 `_components/send-panel.tsx` 的 `SentSessionCard`：补「N% · 剩余 X」
- [x] 5.7 ⚠️ 所有新增渲染点必须走 `_lib/format.ts` 的 `transferSample` 取 live，
      **不许直读 `s.progress[id]`**——Web 的 progress 域只增不清，直读会在已完成会话上
      显示陈旧的「剩余 3m 20s」
- [x] 5.8 `_locales/{zh,zh-TW,en}/messages.po`：同 4.6

## 6. 移动 UI（`mobile/`）

- [x] 6.1 `src/stores/transfer-store.ts`：`publishingBySession` 域 + 清理规则 +
      `reportPublishBytes(relativePath, written)` action（按 `relative_path` 匹配已建条目，
      匹配不上就丢弃，**不凭空造条目**，design D3）
- [x] 6.2 `src/core/event-bus.ts`：加新事件 case（⚠️ 掉进 default 只会 console.warn）
- [x] 6.3 `src/core/foreign-file-access.ts`：`publishToTarget` **入口**即开始上报
      （不是 `copyIntoTarget` 入口——`ensureSafTargetFile` 的逐层 `list()` 也是静止时间）；
      `copyIntoTarget` 循环里按既有 200ms 基线节流上报 `written`
- [x] 6.4 `src/app/transfer/[sessionId].tsx`：transferring 分支补「剩余 X」；新增 publishing 分支
      「正在保存 {文件名} · N%」
- [x] 6.5 `src/components/activity-projection-card.tsx`：补「· 剩余 X」与「正在保存…」状态词
- [x] 6.6 `src/components/recent-transfer-row.tsx`：同上；并把手绘 `h-1` View 换成
      `@/components/transfer/shared` 的 `ProgressBar`（tone 查表的漏网点）
- [x] 6.7 `src/app/transfer/search.tsx`：补传 `progress` 与 `showProgress`
- [x] 6.8 `src/core/foreground-service.ts`：通知 body 追加 ETA；发布期换成「正在保存 · N%」
      （**切走后这是唯一可见面**）
- [x] 6.9 `src/locales/{zh-Hans,en}/messages.po`：四条全新（「剩余 {0}」在移动 catalog 里也没有）

## 7. 契约与门禁收尾

- [x] 7.1 `DESIGN.md` 的 Transfer Progress Contract 补**活跃进度信息位表**（四位 + 折叠规则，
      体例抄 Node Status Contract）
- [x] 7.2 同章节补 **msgId ↔ 三语文案表**（体例抄 Node Status Contract 的文案表）
- [x] 7.3 **删掉** open gaps 段里关于 Android SAF publish copy 的那半句（契约自己写了
      「delete these sentences in the PR that closes them」）。⚠️ **只删这半句**——
      outboard rebuild 与 web zip retrieval 两个缺口不在本次范围，留着
- [x] 7.4 `DESIGN.md` 的 Cross-platform UI Review Checklist 补一条「活跃传输表面四位齐全」
- [x] 7.5 重跑三份入库生成物。实际用法与原计划有出入，两条都记进了知识库：
      桌面 `cargo test -p swarmdrop export_ts_bindings`；
      Web `cargo test -p swarmdrop-web --features specta --test specta_export`
      （**漏了 `--features specta` 会静默什么都不做且报 ok**）+ `cd docs && pnpm build:wasm`；
      移动走 `ubrn generate jsi bindings` + **`bob build`**（不需要 Xcode/NDK；漏了 `bob build`
      则 `lib/typescript/` 仍是旧类型，typecheck 报「新类型不存在」）
- [x] 7.6 机器门禁：`cargo fmt --all` / `cargo check --workspace --all-targets` /
      `cargo test --workspace` / `cargo clippy --workspace` /
      `./scripts/check-wasm.sh` / `pnpm exec tsc --noEmit` / `pnpm test` /
      `pnpm check:zustand-access` / `pnpm check:shared-view` / 三处 `pnpm i18n:extract`
- [x] 7.7 `/simplify` → `/code-review`（两轮 + 四维清理审查，findings 见 design D8）
