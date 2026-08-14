# 设计决策

## D1 — ETA 的抖动只做「展示粒度粗化」，不做有状态平滑

后端 `eta()` 直接取 3 秒滑窗速率的商，没有任何平滑，逐帧（200 ms）会跳。

**做法**：`formatEta` 里把秒数向上取整到 5 秒（<1 分钟）或 10 秒（≥1 分钟）的整数倍再交给
`formatDuration`。秒级抖动看不见了，量级变化仍然照实反映。

**为什么不做指数平滑/卡尔曼**：那需要状态，而三端各有一份渲染代码——状态放前端必然写三遍
且各自漂移，放后端则要给 `ProgressTracker` 加一份只服务于展示的可变字段。粗化是纯函数，
放进 `packages/shared-view` 一处即可，三端输出逐字符一致（该包的归属判据正是这三条）。

**向上取整而非四舍五入**：沿用 `format/time.ts` 已写下的理由——「ETA 说还剩 0 秒却没结束，
比多报一秒难解释」。

## D2 — 发布阶段用**独立事件**，不加 progress 字段，更不动会话 phase 枚举

三个候选：(a) 新增独立事件；(b) 给 `TransferProgressEvent` 加 `phase` 字段；
(c) 给会话 `TransferPhase` 加 `Publishing`。**选 (a)**。

**(b) 被否的三条**：

1. **粒度错配**。`TransferProgressEvent` 是会话级的，而 publish 是**逐文件**的——收齐即发布，
   一个 100 文件的会话会有 100 次 publish 散布在整条传输里，不是末尾一次。
2. **数值会中毒**。发布期间没有新样本，`speed()`/`eta()` 返回的是陈旧值（这正是 D5 要修的）。
   UI 拿到 `transferredBytes == totalBytes && speed=旧值 && phase=publishing` 无从区分真假，
   直接污染本 change 刚补好的 ETA。
3. **成本反向**。`ProgressTracker` 活在 `run_data_channel` 的栈上，`publish_file` 要用得改签名；
   而 `self.events` 与 `self.session_id` **本来就在 `ReceiverActor` 上**——发独立事件是那条
   **零签名改动**的路。另外 200 ms 一帧的高频事件多带一个恒为默认值的字段是纯浪费，且移动端
   `MobileTransferProgress` 是手写镜像，加字段两处都要动。

**(c) 被否的两条**：

1. **它违反既有规格**。`openspec/specs/transfer-lifecycle-state/spec.md:13` 把 publish 定为
   「文件级副作用」，明确发生在 `Active → Completed` 转换**内部**。
2. **它是持久化列**。`TransferPhase` 落 SeaORM，Web 端 IndexedDB 存同一份记录格式——按
   `CLAUDE.md` 的硬规则要 bump `DB_VERSION` 并改 `idb.rs` 的 `STORES` 格式版本，还要连带改
   reducer 转换守卫、`legacy_status` 映射、`is_deletable`、恢复清理与三端 phase 字面量映射。
   每个文件进出 publishing 还要多 dispatch 两次。为一个纯展示态付这个价不成立。

**事件全部不过网**：wire 是 `crates/transfer/src/wire/data_frame.rs` 的 `TransferDataFrame`
（Hello / BlockData / Abort / Finish / Window），`TransferEvent` 只走本地 UI 链路。
**不碰 wire，不 bump `TRANSFER_DATA_PROTOCOL`。**

### 发射位置的不变量

`receiver.rs` 的 doc 写死了：**`finalize_sink` 与 `mark_file_completed` 之间不得插入任何其他
await**（那个窗口被杀进程会留下「staging 已消失、bitmap 却不完整」）。因此：

- `Started` 在 `finalize_sink` **之前**；
- `Finished` 在 `mark_file_completed` **之后**；
- **绝不夹在两者中间**。

失败路径不额外发事件——`publish_file` 的 `?` 冒泡成可恢复的 Interrupted，UI 靠既有的
`TransferProjection`(非 active) / `TransferFailed` / `TransferPaused` 清掉 publishing 态即可。

空文件补发布路径 `publish_pending_empty_files` **不发事件**：它零耗时，而一个含 100 个空文件
的会话会瞬间刷 200 条事件。

## D3 — Android 的字节级进度由 JS 侧直接上报 store，不经 Rust 事件

`Started`/`Finished` 只够画一个不确定态转圈，而 DESIGN.md 第 4 条要的是**百分比**。

数字是白送的：`mobile/src/core/foreign-file-access.ts` 的 `copyIntoTarget` 循环里已经有
`totalBytes` 与逐块累加的 `written`（目前只在抛错时拼进错误串），块间还有 `await setTimeout(0)`
这个天然让出点。

**做法**：JS 侧在循环里直接调 store 的上报 action，不发 Rust 事件。

**为什么不让 Rust 拿到这个数**：那要给 `FileAccess::finalize_sink`（`crates/host/src/ports.rs`）
加进度回调参数 → 端口 trait 一改，桌面 / Web / iOS 三个实现全部要跟着改，而它们的 publish
是 O(1)、根本没有循环可上报。**为一个平台的慢路径去改三端共用的端口签名，方向是反的。**
`ForeignFileAccess` 本来就是移动端宿主适配器、SAF publish 本来就归它，它上报自己的进度是
本层职责。

**怎么挂到正确的会话**：`MobileFileMetadata` 不含 session_id / file_id，所以 JS 侧**不自己判定
归属**——Rust 的 `Started` 事件带 `(session_id, file_id, name, relative_path, total_bytes)`
先在 store 里建好条目，JS 的上报只带 `(relative_path, written)`，由 store 按 `relative_path`
匹配已建条目。匹配不上就丢弃（`Started` 还没到，后续帧会补上），不凭空造条目。

**起点放在 `publishToTarget` 入口，不是 `copyIntoTarget` 入口**：`ensureSafTargetFile` 会逐层
`parent.list()` 全量枚举，用户选 Downloads 这类大目录时，拷贝**开始前**还有一段同样没有反馈
的静止时间。

## D4 — tone 复用既有的 `local`，不加第四档

`DESIGN.md:638-639` 的语义定义原文：「Muted grey = this machine is preparing, **nothing is on
the wire yet**」。publish 是纯本机拷贝，逐字落在 `local` 的定义里。

加新档要为新颜色单独做 WCAG 2.2 SC 1.4.11 的 3:1 实测——琥珀档就是这么错了整整一个版本，
而且**暗色两边都过，只看暗色永远发现不了**（`DESIGN.md:702-709`）。三张 tone 查表零改动。

## D5 — 两条「已有信息就是错的」，同 PR 修

它们不是本次的附加功能，而是**不修就会让新加的 ETA 继续撒谎**的前提。

1. **`speed()` 停滞归零**。现在它只读 `samples` 首尾、不问有多旧；传输停滞（publish 期间、
   对端卡住、磁盘 stall）时会持续返回早已不成立的速率。改为：最后一个样本已老于
   `SPEED_WINDOW` 即返回 `0.0` ⇒ `eta()` 自然 `None` ⇒ UI 显示「计算中」而不是一个冻住的数字。

   这与 `openspec/specs/send-progress/spec.md:14-16`（「3 秒窗口内 ≥2 采样时事件必须含非零
   speed」）不冲突——本改动只在**没有新样本超过 3 秒**时归零。

   实现取「判断末样本年龄」而非「修剪 `samples`」：后者要 `&mut self`，而 `speed()` 是 `&self`
   且被 `eta()` 复用。

2. **文件边界强制发一帧**。`progress_event()` 的 200 ms 节流无条件生效，而块间隔远小于它，
   于是「该文件刚好收齐」那一帧大概率被丢弃，UI 停在 99.x%。给它一个 `force` 逃逸口，
   收发两侧在文件收齐时传 `true`。**这也是发布提示的前提**——否则用户看到的是
   「99.7% → 正在保存」，比现在更费解。

## D6 — P0 接受「逐文件行在发布期显示已完成」

`ProgressTracker` 在末块就把文件置为 `FileTransferStatus::Completed`，**先于** publish。所以
逐文件行在发布期显示的是「已完成」——这不只是缺信息，是错信息。

正解是加第四态 `FileTransferStatus::Publishing`，但它会波及 `packages/shared-view` 的
`projectionFileStatus`、`packages/file-browser` 的 tone 轴（那条轴是 `transfer` vs `paused`，
与 `local` 正交，塞不进去）与四份 catalog。

**本次接受这个偏差**：会话级横幅点名了正在保存的文件，用户不会把「文件行显示已完成」读成
「整件事结束了」。**显式记在这里，不是漏掉。**

## D8 — 两条审查提出、本次不修的已知限制

### D8.1 `PUBLISH_ANNOUNCE_MIN_BYTES` 只覆盖了拷贝，没覆盖建目录树

阈值的论证是「1 MiB 的拷贝在最慢的 SAF 目标上也只有几十毫秒」。但 Android publish 的静止时间
还有另一个来源：`ensureSafTargetFile` 逐层 `parent.list()` **全量枚举**目标目录。用户把接收
目录选成 Downloads（已有数千条目）时，一个 10 KB 的文件也可能等上数百毫秒，而它 `< 1 MiB`
⇒ Rust 根本不发 `Started` ⇒ store 里没条目 ⇒ 应用内与通知都停在「接收中 · 100%」。

不在本次修的理由：修它要么去掉阈值（回到每文件两条事件的 O(N)，那是本次刚修掉的），要么让
JS 侧能在没有 Rust 事件的前提下独立开启发布态（等于把「发布态由谁拥有」这条判据劈成两半，
正是 D3 刚收敛掉的）。正解应该是**让目录树的建立不再是每文件一次**（缓存已建目录），
那是移动端 SAF 层的独立改动。

### D8.2 `foreign-file-access` 现在参与一条 import 环

新增的两条 import（store / 前台通知）让 `foreign-file-access → foreground-service | transfer-store
→ mobile-core → foreign-file-access` 成环。今天不炸的唯一原因是 `mobile-core` 是它**唯一**的
消费者——环的入口固定，`ExpoFileAccess` 的类定义在被 `new` 之前已经求值。

给这个模块加**第二个**消费者就会踩：Babel 的 ESM→CJS 把 `exports.ExpoFileAccess = …` 放在
类定义处，若从别处先进入这条环，`mobile-core` 顶层的 `new ExpoFileAccess()` 会拿到 `undefined`
并抛在 `_layout` 的 import 期间——既在 `runBoot` 的 try/catch 之外，也在 `<Try>` 边界之外，
用户看到的是致命红屏而不是带「重试」的启动错误屏。

不在本次修的理由：拆环要给宿主适配器加一层「上报端口」的注册机制，属于移动端分层的独立改动。
**判据先写在这里**：给 `foreign-file-access.ts` 加第二个消费者之前，先拆环。

## D7 — 发送侧的同一段停顿仍然无解释（已知缺口）

Android publish 期间接收循环整条挂起、不回 `Window` 确认，发送端 `actor/sender.rs` 的
`sync_window` **无超时**地干等——发送方 UI 同样静止、同样没有解释。

要修必须过网（新 wire 帧 + 协议 bump），远超本次范围。**记为已知缺口。**
