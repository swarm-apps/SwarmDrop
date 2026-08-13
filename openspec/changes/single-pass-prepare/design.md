## Context

`TransferManager::prepare` 是三端「点了发送、还没开始传」这段的全部内容。它今天对每个文件做两件事：

```
pass 1  read_source_chunk × ⌈size/256KiB⌉  →  blake3::Hasher      →  checksum
pass 2  read_source_chunk × ⌈size/16KiB⌉   →  bao outboard        →  (root, outboard)
        debug_assert_eq!(root.to_hex() == checksum)   // release 下不执行
```

`root` 与 `checksum` 是同一个值——`BlockSize::from_chunk_log(4)` 下 bao 树根恒等于标准 BLAKE3 整文件 hash（chunk group 只影响 outboard 深度，不影响 root；`bao.rs:17-21` 的模块文档、`bao.rs:295/311` 两条测试）。pass 1 因此在语义上是纯冗余，它今天唯一的作用是产出 `PrepareProgress` 事件——所有 emit 都写在 pass 1 的 chunk 循环里（`prepare.rs:66-82`），pass 2 一个事件都不发。

三条约束框定了解空间：

1. **`checksum` 必须在 Offer 发出前算出**。它是 wire 上 `FileInfo` 的字段（`protocol.rs:46-55`），也是接收端每个块的验签 root。所以「至少读一遍全文件」是硬下限。
2. **`outboard` 不上 wire**。`FileInfo` 无此字段，唯一消费点是 `sender.rs:592` 的 `encode_proof`。
3. **三端 `read_source_chunk` 都每次重新打开文件**（桌面 `path_ops.rs:88` `File::open`+`seek`，且整个包在 `spawn_blocking` 里；移动 `foreign-file-access.ts:116` open/seek/read/close 跨 JSI，Android SAF 源每次多一趟 `ContentResolver` binder IPC；Web `File.slice().arrayBuffer()` 每次一个 Promise + 结构化拷贝）。**主导成本是调用次数，不是字节数。**

由第 3 条，pass 2 的 16 KiB 粒度是真正的大头：100 MiB 文件 = 400 + 6400 次调用，94% 落在那遍没有进度条的读上。

## Goals / Non-Goals

**Goals:**

- prepare 每个文件只读一遍，`checksum` 由 bao 构建的 `root` 得出。
- prepare 阶段的进度覆盖**全部**真实工作量——不再有静默段，进度条不再系统性偏快一倍。
- bao chunk group 与 `CHUNK_SIZE` 对齐，消除「验签粒度比传输块细 16 倍」这条从未有消费方的冗余。
- prepare 进度在三端**跨页面存活**，并覆盖 MCP、Web 收件箱转发、resume 回填三条今天完全静默的路径。
- 把「逐块验签」这块自 2026-07 落地起从未进过 spec 层的能力补上规格。

**Non-Goals:**

- **不推迟 outboard 构建到数据面**（推翻探索阶段的初始倾向，理由见 D2）。
- 不改 `PrepareProgressEvent` 的字段与字段名（理由见 D9）。
- 不新增会话 phase，不改 `TransferProgressEvent`（理由见 D8）。
- 不修 `transfer-protocol` 那份已漂移的旧 spec（它仍写着 `checksum` 是 SHA256 hex、`OfferResult` 携带 256-bit 密钥，两者在 wire v2 都已不成立）——那是独立的规格债，混进来会让本 change 的 delta 失焦。
- 不动 `TRANSFER_CTRL_PROTOCOL`（控制面帧格式不变）。
- 不做向后兼容：无版本协商、无数据迁移、无双写。

## Decisions

### D1 — 删除 flat hasher，`checksum = root.to_hex()`

`build_outboard_from_source` 已经返回 `(blake3::Hash, Vec<u8>)`（`bao.rs:64-78`），root 直接拿来用。

**代价**：`prepare.rs:96-100` 那条 `debug_assert_eq!` 是「两套独立实现互证」的唯一守卫，合并后它失去了对照物。这是本次唯一实质性的保障损失。**补偿**：在 `bao.rs` 的流式构建测试里补齐 `size ∈ {0, 1, CHUNK_SIZE-1, CHUNK_SIZE, CHUNK_SIZE+1}` 的边界，把「流式 root == 扁平 `blake3::hash`」这条不变量从运行期 assert 迁到单测里钉住。

**顺带修掉一个真实缺陷**：今天 `checksum` 与 `outboard` 出自两遍独立的读，文件在两遍之间被改会产出互不匹配的二者，而 release 下无人拦截，一直拖到接收端验签才炸且归因指向网络。合并后二者同源，这类不一致在构造上不可能。

### D2 — 不推迟 outboard 到数据面

探索阶段一度倾向「prepare 只算 checksum，outboard 在首次 `encode_proof` 时按需建」。摊开总读盘遍数后它站不住：

| | prepare | 数据面 | 传输 | 合计（offer 被接受） | 合计（offer 被拒） |
|---|---|---|---|---|---|
| 现状 | 2 | — | 1 | **3** | 2 |
| **本方案（合并）** | 1 | — | 1 | **2** | **1** |
| 推迟 | 1 | 1 | 1 | 3 | 1 |

推迟一旦成立，prepare 就**没法**用 bao 的 root 当 checksum（那一遍不再构建 outboard），只能自己跑 flat hasher——于是 outboard 从「零边际成本的副产品」退回成「要单独付一遍读的独立工作」。它在常态（offer 被接受）输一遍，在 offer 被拒时打平，**没有任何场景赢**。

推迟还要额外背三条风险：`run_data_channel` 是单流顺序 write→read 循环，构建期间既不发块也不回 Window，接收端只看到「流没动」；`last_activity_ms` 只在 `write_block` 写出帧后更新（`sender.rs:610-613`，初值 `AtomicU64::new(0)`，idle 从创建时刻起算），长构建会被 30 分钟空闲清理误杀；Web 端把「有进度条的等待」换成「无进度的卡顿」，且 `web-app-frontend.md:1355-1357` 记的同页面续传依赖内存里的 outboard，无缓存的延迟构建会让它重读整个文件——那是 UX 回归，不只是文档断言反转。

推迟原本想解决的**内存**问题由 D3 顺手解决：outboard 小 16 倍，10 GB 一批的驻留从 ~39 MB 降到 ~2.4 MB。

**被 D2 保留下来的既有缺陷**（独立于本 change，可单独下手）：`load_file_outboard` 端口方法零消费者（`transfer/src/store.rs:103-117`，业务代码无调用点）；`send.rs:95-100` 在 Offer 发出**之前**就把 outboard 写库，对端拒绝也白写；`update_file` 是整行 load→save（`storage-sql/ops.rs:189-202`），outboard 落库后每次发送侧进度落盘都要把 BLOB 读一遍。本 change 不处理，在 tasks 里留记录。

### D3 — `BLOCK_SIZE` 16 KiB → 256 KiB，与 `CHUNK_SIZE` 相等

`BlockSize::from_chunk_log(4)` → `from_chunk_log(8)`。全仓只有 `bao.rs:41` 一处定义，无第二处硬编码 chunk group。

收益（按 `BaoTree::outboard_size() = (blocks()-1) * 64` 实算，`bao-tree` `src/lib.rs:446`）：

| | 16 KiB group | 256 KiB group |
|---|---|---|
| 1 GiB 文件 outboard | 4 194 240 B（0.3906%） | 262 080 B（0.0244%） |
| 整文件 wire parent 开销 | ~6.75 MiB（0.659%） | ~3 MiB（0.293%） |
| 接收端每块 throwaway outboard 分配上限 | ~4 MiB | ~256 KiB |
| outboard 构建的 `read_source_chunk` 次数 | ⌈size/16 KiB⌉ | ⌈size/256 KiB⌉ |

**关键副产品：不需要写顺序读缓冲。** 探索阶段设想过给 `FileAccessReader` 加一层 256 KiB 合并层，把 bao 的 16 KiB 请求攒起来。chunk group 提到 256 KiB 之后 bao 直接按 256 KiB 请求，那层缓冲整个消失——连带消失的还有它带来的两个问题：一个仓内无先例的三层装饰器结构，以及「缓冲在 EOF 附近请求超长」导致的长度判据复杂化（见 D7）。

**验签粒度确实变粗**：最小可独立验证单元从 16 KiB 变成 256 KiB。在本仓的威胁模型下零损失——proof 与传输块本来就一一对应，从来没有过 sub-`CHUNK_SIZE` 的验证请求。但这是**单向门**：将来若要做 range 请求 / 部分文件预览 / 与 iroh-blobs 互通，256 KiB 是硬下限。记进 `net-kernel.md` 的已知负债。

### D4 — 协议名 bump 到 `/4`，且不保留 `/3` 与 `/2`

帧布局不变，但 **proof 的语义整体作废**：16 KiB 端产出的 proof 喂进 256 KiB 端的 `decode_ranges`，树形状对不上 → **第一个块**就验签失败 → 协议违规 → 断流 → Interrupted 无限重试。

本仓对这类破坏有写死的体例，就在 `protocol.rs:16-36`：加一个帧 tag 就必须换协议名，因为解码器对未知 tag 是硬失败。BLOCK_SIZE 改动属于**同一类且更严重**——不是 4 MiB 窗口边界才断，是第一个块就断。`TRANSFER_DATA_VERSION` 不是这个的杠杆（`data_frame.rs:15-23` 明写「加 tag 不动这个数……这个字段校验的是『共有帧怎么编码』」），能力差异一律由协议名承载。

「不考虑兼容性」不等于「不用 bump」——恰恰相反，**bump 正是把不兼容表达出来的手段**。留着 `/3`/`/2` 的注册而让它们跑新的 BLOCK_SIZE，会把「版本不匹配」变成「连上之后第一个块验签失败、无限重试」的静默死循环；bump 到 `/4` 并删掉旧注册，它变成一次响亮的协议协商失败。**失败模式的区别，就是这一条决策的全部理由。**

**摘除只完成了一半，还得给失败分类**（实现期由 altitude 审查发现）：`open_data_stream` 原先把 `OpenError::UnsupportedProtocol` 和拨号超时一样压成 `AppError::Transfer(String)`，于是版本不匹配走 `NetworkSignal::Interrupted` → suspended/**recoverable**，续传机器拿同一个协议名一次次重试。协商阶段确实响亮地失败了，但那份信息在函数里就没了，用户看到的仍然是「传输老是断」——上面那句「失败模式的区别就是全部理由」因此在落地时是**假的**。修法是给它一个真正的分类：`OpenFailure::{ProtocolUnsupported, Transient}` 在压成字符串**之前**分流，前者经 `ActorReport::FatalError(FailureCode::PeerProtocolUnsupported)` 直接推终态，三端渲染成「对方版本太旧，请让对方升级」。判别码到达用户，这条决策才算兑现。

删除范围：`TRANSFER_DATA_PROTOCOL` 改 `/4`、删 `TRANSFER_DATA_PROTOCOL_V2` 常量与它的接收侧注册、删 `wire::data_plane::open_data_stream` 里拨 `/3` 被拒后退回 `/2` 的回退分支（回退后不发窗口帧那套逻辑一并去掉）。

### D5 — outboard 有效性判据：长度即格式版本

现有失效判据是 `outboard.is_empty()`（`resume/mod.rs:360`、`plan.rs:188` 的 `unwrap_or_default()`）。一个 16 KiB 时代写下的**非空**BLOB 会被原样载入、当成 256 KiB 树用，`encode_ranges_validated` 每次都 `ParentHashMismatch`，而回填分支永不触发——这不是 panic、不是数据损坏，是**安静的、永久的**「这个会话再也续不上传」。

判据改为：`outboard.len() as u64 == outboard_len(size)`，其中 `outboard_len` 是 `bao.rs` 新增的公开函数（`BaoTree::new(size, BLOCK_SIZE).outboard_size()` 的薄封装）。

- **不加 migration**：长度校验同时**就是**格式版本号——将来再调 block size 也不需要迁移，这是比一次性迁移更强的性质。
- **不把 `bao_tree` 类型泄漏到 flow 层**：全仓 `bao_tree` 仅 `bao.rs:26-32` 一处 import，`resume` 只调 `crate::bao::outboard_len(size)`。
- **顺带修掉一处既有浪费**：新粒度下 ≤256 KiB 的文件 `blocks == 1` → outboard 恒为空，`is_empty()` 判据会对每一个这样的文件在每次 resume 时做一次完整重读再存回空 vec。长度判据让它们直接命中。

### D6 — 进度 emit 挂在 reader 装饰器上，reporter 跨文件持有

进度必须挂到唯一那遍读上，而那遍读的循环在 bao 库内部，唯一的挂载点是传给 `PostOrderOutboard::create` 的 reader。

形态（两层，不是三层——D3 消掉了缓冲层）：

```
ProgressReader { inner: FileAccessReader, reporter: &mut PrepareReporter }
        ↑ 发事件 + 节流                    ↑ 纯适配，保持不动
```

可行性依据（`AsyncSliceReader` 的 `impl` 对 `&mut T` 也成立，`iroh-io` `lib.rs:88`；该 trait **没有 `Send` bound**，`lib.rs:57-66`）。wasm 的 `!Send` 已在 `crates/web/src/file_access.rs` 端口实现内部由 `SendWrapper` 收口，`bao.rs` 这一侧碰不到 `JsValue`，**一行 `SendWrapper` 都不需要**；节流用的 `n0_future::time::Instant` 在 wasm 下是 `web_time::Instant`，可用。

**reporter 必须跨文件存活**，由 `prepare` 持有一次、按可变借用传进每个文件的 reader。两个状态都是跨文件的：节流的 `last_emit`（每文件重置会让小文件批量刷屏）、累计的 `completed_bytes`（reader 只知道自己文件的 offset）。不要退化成 `Arc<Mutex<..>>`——单线程顺序访问上锁是纯开销。

**两个入口而非 `Option` 参数**：`build_outboard_from_source`（无进度，resume 回填用）与 `build_outboard_from_source_with_progress`（prepare 用）。给前者塞一个必填 reporter 会逼 resume 造一个假的 `prepared_id`，那会让 UI 收到一条没有对应 prepare 流程的进度事件。`_with_progress` 后缀在本仓有先例（`path_ops::compute_hash_with_progress`）。

**emit 的错误必须继续被 `let _ =` 吞掉。** `read_at` 返回 `io::Result`，一旦把 `emit` 的 `Err` 转成 `io::Error`，事件总线的一次抖动就会中断整个 outboard 构建、让一次正常的发送准备失败。

### D7 — 宿主违约检查的判据

`prepare.rs:56-63` 有一条比端口契约更严的检查（严格 `!=`，短读也报错），而 `bao.rs:97` 只拒超长。合并后它要搬进 reader。

判据是 `chunk.len() != len`，**裸的、不带 min 修正**。理由：bao 的 `leaf_byte_ranges3` 把叶子范围 clamp 到 `self.size`（`bao-tree` `lib.rs:348-355`），而 tree size 就取自 `AsyncSliceReader::size()`，所以 outboard 构建**永远不会**请求越过 EOF 的字节——`offset + len <= size` 恒成立，等长是可断言的。

（若 D3 不成立、需要保留 256 KiB 顺序缓冲层，判据就必须写成 `chunk.len() != len.min(size - offset)`：缓冲层会在 EOF 附近合法地请求 256 KiB 而只拿到剩余字节，裸 `!=` 会在每个非 256 KiB 整数倍的文件末尾误报。D3 让这个复杂度不必存在——这也是把两件事放进同一个 change 的理由。）

配套：这条不变量要由护栏测试断言（见 D11），而不是只靠读源码相信。既有测试 `overlong_host_read_is_rejected_not_panic`（`bao.rs:383`）断言的错误串 `"违反契约"` 必须保留。

### D8 — PrepareProgress 升级为三端 typed event，落进按 `preparedId` 索引的 store

**桌面的 per-call Channel 从来不是权衡的产物。** `PrepareProgress` 出生于 `ff47e1dd`（2026-02-26）就是 Channel，比 tauri-specta typed events 引入（`09fc459c`，2026-05-19）早三个月——当时那个选项根本不存在。2026-05 迁移 13 个 event 时它被排除，commit body 里**一个字的理由都没有**。`b0fd8785` 后来加过一条全局 `app.emit` fallback，`3d2d764e`（2026-06-30）以「前端从不 listen 的死 fallback」为由顺手删掉——`event_bus.rs:169-177` 那条注释描述的是**删掉一条从未生效的分支**，不是「经权衡选择了 Channel」。唯一成文的取舍论证在 `dev-notes/archive/pre-refactor-blogs/`（`/dev-workflow` 不加载 archive），判据是「只有当前发送页面关心」——share-target 加入后这条前提已经不成立。

现状是 21 : 1 的孤例，而**另外两端本来就走广播总线**：Web 的 `store.ts:487` 已经是 `prepares: { ...s.prepares, [ev.event.preparedId]: ev.event }`（形状正确但零读者、零裁剪、零测试，实际被消费的是旁路 `latestPrepareProgress`）；移动端 `routeEventToStores` 里 PrepareProgress **根本没有 case**，落进 `default` 的 `console.warn("unhandled event tag")`——每次发送刷成百上千条。所以改成 typed event 是**三端对齐**，不是新开先例。

**MCP 那条路只有广播能修**：`mcp/tools.rs:358` 自己 mint `prepared_id`，没有 invoke 生命周期可挂 Channel。

落点形状（各端与自身既有体例一致）：

| 端 | 落点 | 认领 |
|---|---|---|
| 桌面 | `PrepareProgress` 进 `collect_events![]`，新建 store 域按 `preparedId` 索引 | 事件自我认领 |
| 移动 | `event-bus.ts` 加 `case` → `transfer-store` 的 `preparesByPreparedId` + `activePrepareId` | 同上 |
| Web | 保留已有 `prepares` 表、删 `latestPrepareProgress` 旁路、加 `activePreparedId` | 同上 |

**认领方式只有一条可行**：让事件自带的 `preparedId` 自我认领（第一条事件到达即写入活跃标记）。`preparedId` 拿不到「提前」——事件在 `prepareSend()` / `send_files()` 返回**之前**就开始发，三端的调用点都不把它交给页面。自我认领天然也支持协议本来就允许的并发 prepare。

**渲染门从组件局部的 `pending` 改成 store 里的 `activePreparedId !== null`**——这一步才是真正让它跨页面。

**不新增会话 phase，不灌进 `TransferProgressEvent`。** prepare 阶段**没有 `sessionId`**（`send_offer` 第 68 行才 `Uuid::new_v4()`），任何挂进 `projections` / `progressBySession` 的方案第一行就卡住。而灌进 `transferred_bytes` 会：把 `speed` 变成本地读盘速度（`DESIGN.md` 把 transfer-speed 读数列为全系统最大字号的机器真值）；让计数冲到 total 再回落；并触发 `packages/shared-view/src/file-browser/adapters.ts:267` 那条 `if (size > 0 && transferred >= size) return "completed"` 的无条件短路，把一个字节都没发出去的文件在三端列表里显示成「已完成」。

### D9 — `bytes_hashed` 不改名

合并后那一遍就是 hashing（bao root 就是 BLAKE3），字段名字面依然准确。改名要动 2 份生成的 bindings + uniffi record + 重跑生成 + 三端 6 个使用点，而 `.po` 里它只出现在自动生成的 placeholder 注释行、msgid 用编号占位——翻译零成本、工程有成本、收益为零。

### D10 — 对齐判据只有一处，且必须在协商阶段就拒

改完之后「块必须落在 chunk group 边界上」从「有 16 倍冗余的软约定」变成零冗余的硬前提。违反它的失败形态是 `OffsetReadAt` 的 `InvalidInput("read_at 越过块起点")` 或 `read_exact_at` 的 `UnexpectedEof`——后者与真正的 IO 错误无法区分。

判据（**必须允许跨多个整叶子**，否则会对合法输入误报）：

```
offset % CHUNK_SIZE == 0
  且 ( (offset + len) % CHUNK_SIZE == 0  或  offset + len == file_size )
```

仓内现成的合法反例：`tampered_block_is_rejected`（`bao.rs`）的块长 312 320 B，跨一个整叶子加一个尾叶子。写成 `len == CHUNK_SIZE || offset + len == file_size` 会把它误判。

**这条判据有三个消费者，收成一处 `is_chunk_aligned_range`（`lib.rs`）**：接收侧
`checkpoint::validate_block_range`（入站 BlockData）、发送侧
`resume::validation::validate_fetch_plan`（对端提交的续传计划）、以及 `bao::encode_proof`
（生成 proof 的前置条件）。三者恒等，因为 `BLOCK_SIZE` 就是从 `CHUNK_SIZE` 推导的。

**而第二个消费者原先根本不存在**（实现期由 altitude 审查发现）：`validate_fetch_plan` 查了
file_id、溢出、`end > size`、`length == 0`，唯独不查对齐。chunk group 是 16 KiB 时有 16 倍
冗余替它兜着，所以这个洞一直没显形；冗余归零之后，对端提交一个非对齐 offset 的
`ResumeCommit` 会让本端回 `accepted: true`、建 actor、开流、读盘，直到 `encode_proof` 在第
N 块抛一句泛型错误 → abort → Interrupted → 对端拿同一个计划再提交一次。**一次计划校验
一次，好过每块撞一次**——所以判据的权威落点是协商阶段的 validator，`encode_proof` 里那道
只是最后的防御。

### D11 — 「bao 顺序读」是实现事实，必须由护栏测试钉死

`PostOrderOutboard::create` 先取 `size()`，然后把 reader 包进 `std::io::Cursor` 当作 **`AsyncStreamReader`** 用（`iroh-io` `lib.rs:188` 的文档原话：「A non seekable reader, e.g. a network socket」）；`outboard_impl` 对 data 的唯一调用是 Leaf 分支的 `read_bytes_exact(size)`；叶子由后序 DFS 左到右产出。四条合起来给出：offset 从 0 严格单调递增、每次 ≤ `chunk_group_bytes`、末次为精确剩余、总和恰好等于文件大小、每字节只读一次。

**但 `AsyncSliceReader` 的文档明确说它是随机读接口**——顺序性是 bao-tree 的实现事实，不是它承诺的契约。本 change 把这个未承诺的行为变成了两条正确性前提（进度单调递增、D7 的等长判据）。因此必须加一条护栏测试：用记录调用序列的 mock reader 断言「offset 严格单调递增、每次长度 ≤ `chunk_group_bytes`、总和 == 文件大小」。**升级 bao-tree rev 时那条测试是唯一的警报。** 体例照 `crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs:624-930` 那组带 doc 注释的不变量测试。

## Risks / Trade-offs

**[bao-tree 升级会静默破坏顺序性前提]** → D11 的护栏测试。测试名与 doc 注释要写明「这条红了说明 bao-tree 换了读取策略，进度单调性与等长判据同时失效」。

**[存量 SQLite 的 outboard 变有毒且不自愈]** → D5 的长度判据。这是本次唯一会**安静地永久失效**的东西：不 panic、不报错，只是那个中断的发送会话每次 resume 都同样失败。开发库、e2e 遗留库、以及任何已装 v0.14.0 的设备都会撞上。

**[两条既有测试必红]** → 已逐条推演，都是「测试尺寸在新粒度下走不到目标分支」的可修形态，不是设计冲突：
- `overlong_host_read_is_rejected_not_panic`（`bao.rs:383`）：98 061 B 在 256 KiB group 下只有一个叶子，请求 len 被 clamp 到 98 061，`ignore_len` 的 mock 返回恰好 98 061 → 超长分支永不触发。**修法**：尺寸改到 > 256 KiB（如 `CHUNK_SIZE + 12345`），使有多个叶子。
- `roundtrip_from_16kib_offset`（`bao.rs:398`）：单叶子树下 `start_chunk = 0`，`OffsetReadAt{base: 16384}` 的 `checked_sub` → `None` → `InvalidInput`。**修法不是删**：它守护的「resume 场景的非对齐 offset 也能工作」在新粒度下是个**假能力**（生产路径的 offset 恒是 `CHUNK_SIZE` 倍数，且接收端 `checkpoint.rs:104` 的 `validate_block_range` 早就会拒非对齐 offset），改写成断言「非对齐 offset 被 D10 的断言明确拒绝」，让它从守护假能力变成守护真契约。

其余 8 条测试逐条推演为绿（`streaming_build_*`、`roundtrip_single_block`、`multi_block_aligned`、`tail_unaligned`、`tampered_block_is_rejected`、`wrong_root_is_rejected`、`empty_file_roundtrips`、`checksum_hex_roundtrips_as_root`），`crates/core/tests/e2e_transfer.rs` 的尺寸全是 `CHUNK_SIZE` 倍数加零头，同样绿。**这是分析确认不是实测确认**，落地第一步就跑一遍。

**[丢失 `debug_assert_eq!` 的两套实现互证]** → D1 的边界单测补偿。特别是 **0 字节文件**：合并后它的 `checksum` 来源从 `hasher.finalize()` 变成 bao root，而流式路径今天**没有 `size == 0` 的测试**。这是本次最不该省的那条测试。

**[错误信息降级]** → 「文件被外部改短」的报错会从 `prepare.rs` 那条带文件名与精确字节数的消息，退化成 bao 包裹的 `"bao outboard 构建失败: unexpected end of file"`。缓解：把 D7 的守卫收紧在 `FileAccessReader` 内并让它带上 `relative_path`。代价是这个纯适配器多一个纯诊断用途的字段——接受。

**[协议名 bump 让旧客户端彻底连不上]** → 这是**有意的**（D4）。用户已明确不考虑兼容性；bump 的价值在于把失败从「静默死循环」变成「响亮的协商失败」。

**[验签粒度变粗是单向门]** → 记进 `net-kernel.md` 已知负债。将来若要 range 请求 / 部分文件预览 / 与 iroh-blobs 互通，256 KiB 是硬下限。

**[发送端 per-block 瞬时内存 +240 KiB]** → `encode_ranges_validated` 内部的 `vec![0u8; tree.chunk_group_bytes()]` 从 16 KiB 涨到 256 KiB（每次 `encode_proof` 一次分配）。在 `WINDOW_CHUNKS = 16`（在途 4 MiB）的语境下确认一遍即可。接收端反向受益（throwaway outboard 上限降 16 倍），净效果对内存是好的。

**[改 `collect_events!` 会重新生成 `bindings.ts`]** → 那是自动产物（`pnpm tauri dev` debug build 时导出），不要手改；但它会进 diff，review 时别当成手写改动。

**[移动端 `default` 分支的信号价值]** → 补 PrepareProgress 的 case 时，同样漏网的 `PairingCompleted` 也要一并处理（补 case 或改成带注释的显式空 case）。否则 `default` 分支仍然不是「真的遇到未知事件」的信号。仓里表达「决策不落」的方式是带注释的显式空 case（`TransferAccepted` / `TransferResumed` 就是这么写的，理由是与 `TransferProjectionUpdate` 冗余）。

## Migration Plan

**不做兼容**：无版本协商、无数据迁移、无双写、无灰度。

- **协议**：`/4` 是唯一注册的数据面协议。旧客户端拨 `/3` 或 `/2` 得到协商失败——这是期望的失败形态。
- **SQLite**：schema 不变（列类型仍是 BLOB），变的是内容语义。存量有毒 BLOB 由 D5 的长度判据在读取时判为失效并重算回存，**不加 migration**。
- **Web IndexedDB**：**不需要提 `DB_VERSION`，也不需要动 `STORES`**——`FileRowDef.outboard` 是 `#[serde(skip)]`（`crates/web/src/store.rs:891`），outboard 从来没进过 IndexedDB；`completed_chunks` 绑的是 `CHUNK_SIZE`（不变）而非 chunk group。这条要写进 change，否则 CLAUDE.md 那条「换记录格式必提 `DB_VERSION`」会让人白提一版。
- **回滚**：`git revert` 即可，无持久化状态需要反向迁移（长度判据对旧格式 outboard 同样判失效重算）。

## Open Questions

1. **`load_file_outboard` 这个死端口在本 change 之后仍然零消费者**——resume 走的是 `plan.rs:188` 的 `unwrap_or_default()`。D5 的长度判据落地时是顺手让它有消费者（把 `plan.rs` 的载入改成经端口 + 判据），还是维持现状留给独立清理？倾向前者：判据要生效就得有个统一的载入点，否则 `plan.rs` 与 `resume/mod.rs` 两处各判一次。
2. **移动端 prepare 期间没有任何跨页面表面**——前台服务通知只在 `TransferProgress` 上更新（`event-bus.ts:121`）。让 prepare「跨页面存活」之后，大目录 hash 时用户切后台仍可能被系统回收。前台服务那一路要不要一并纳入本 change？倾向不纳入（它是独立的保活议题），但要在 tasks 里留记录。
3. **`docs/content/docs/security.mdx:48`** 是**用户可见**的安全说明，里面写着 16 KiB 的验证粒度。改数字之外，要不要顺带说明「验签粒度 == 传输块粒度」这个更好懂的表述？
