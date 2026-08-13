## Why

发送方 `prepare` 把每个文件**完整读了两遍**：`flow/prepare.rs:40-85` 逐 chunk 跑一遍扁平 BLAKE3 得到 `checksum`，`:90-95` 再调 `build_outboard_from_source` 重读一遍建 bao outboard——而后者返回的 `root` 按 bao 规格**必然等于**前者（`bao.rs:17-21` 的「root 零成本」，`bao.rs:295/311` 两条测试钉着）。第一遍在语义上是纯冗余，它今天唯一的作用是**产出进度事件**。

代价不止「多读一遍」：

- **第二遍是静默的**。所有 `PrepareProgress` 都产自第一遍的 chunk 循环内（`prepare.rs:66-82`），`build_outboard_from_source` 一个事件都不发。于是进度条按每个文件呈阶梯式卡顿，且分母只算一半工作量，**系统性比真实速度快约一倍**。
- **第二遍的调用次数是第一遍的 16 倍**。bao 的 chunk group 是 16 KiB（`bao.rs:41`），而 `CHUNK_SIZE` 是 256 KiB。100 MiB 文件 = 400 + 6400 次 `read_source_chunk`，其中 94% 来自那遍没有进度条的读。三端 `read_source_chunk` 都**每次重新打开文件**（桌面 `path_ops.rs:88` 是 `File::open`+`seek`+`spawn_blocking`、移动 `foreign-file-access.ts:116` 是 open/seek/read/close 跨 JSI、Android SAF 源每次还多一趟 `ContentResolver` binder IPC、Web 是 `File.slice().arrayBuffer()`），所以主导成本是调用次数而非字节数。
- **进度在三条路径上完全消失**。桌面 MCP `send_files`（`mcp/tools.rs:358`）自己 mint `prepared_id` 却从不注册 channel，事件在 `event_bus.rs:173` 全部 miss 后静默丢弃；Web 收件箱转发（`forward-to-device-dialog.tsx:97`）走同一个 `send_files` 但只有一个转圈；resume 的 outboard 回填（`resume/mod.rs:356-372`）同样零事件。
- **四处权威文档在说反话**，都断言 outboard 与 checksum「同一遍构建」：`dev-notes/knowledge/iroh-migration.md:228`（与自己第 237 行「目前是两遍……留作优化」直接打架）、`net-kernel.md:1409`、`crates/transfer/src/wire/mod.rs:22`、`crates/entity/src/transfer_file.rs:55`。

现在做，是因为三端都已把 prepare 摆在「点发送之后、传输开始之前」的关键路径上（桌面 `send/index.lazy.tsx:109`、移动 `transfer-store.ts:178`、Web `node.rs:927`），而这段期间**界面上一个可交互元素都不剩**——进度条把取消与发送按钮整条顶掉。

## What Changes

### 1. prepare 合并为一遍流式读

- 删除 `flow/prepare.rs` 的独立 `blake3::Hasher` 循环，`checksum` 改由 `build_outboard_from_source` 返回的 `root` 得出。每个文件从读两遍降到读一遍。
- 进度 emit 下沉到 bao 构建所走的 reader 上，使进度**覆盖 prepare 的全部真实工作量**，不再有静默段。
- `bao-tree` 在 outboard 构建路径上把 reader 当**不可 seek 的流**使用（`CreateOutboard::create` 取 `size()` 后包进 `std::io::Cursor`），offset 从 0 严格单调递增、每字节只读一次。这是库的**实现事实而非公开契约**，必须由护栏测试钉死。
- 顺带关掉一个真实缺陷：今天 `checksum` 与 `outboard` 出自两遍独立的读，文件在两遍之间被改会产出互不匹配的二者，而 `prepare.rs:96` 只有 `debug_assert_eq!` 拦着（release 下不执行），一直拖到接收端验签才炸且归因指向网络。

### 2. **BREAKING** — bao chunk group 16 KiB → 256 KiB，对齐 `CHUNK_SIZE`

- `BLOCK_SIZE` 改为 `BlockSize::from_chunk_log(8)`。验签粒度从「比传输块细 16 倍」收敛成「恰好等于传输块」——细出来的那 16 倍从来没有消费方。
- 收益（按 `BaoTree::outboard_size() = (blocks-1)*64` 实算）：1 GiB 文件 outboard 从 4 194 240 B 降到 262 080 B；整文件 wire 上的 parent 开销从 0.659% 降到 0.293%；接收端每块的一次性 throwaway outboard 分配上限同样降 16 倍；outboard 构建的读次数降 16 倍。
- **副产品：不需要写顺序读缓冲**。bao 直接按 256 KiB 请求，`read_source_chunk` 的调用次数与传输那遍一致。
- **BREAKING** — proof 的树形状随之改变，16 KiB 端产出的 proof 喂进 256 KiB 端的 `decode_ranges` 会在**第一个块**就验签失败 → 协议违规 → 断流 → Interrupted 无限重试。按 `protocol.rs:16-36` 写死的既定体例，能力差异一律由协议名承载：`TRANSFER_DATA_PROTOCOL` 从 `/swarmdrop/transfer-data/3` bump 到 `/4`，且**不保留** `/3` 与 `/2` 的注册——让版本不匹配变成响亮的协商失败，而不是连上之后的静默死循环。
- **BREAKING** — 存量 SQLite 的 `transfer_files.outboard` 变成有毒数据。现有失效判据是 `outboard.is_empty()`（`resume/mod.rs:360`、`plan.rs:188`），放不掉一个「非空但格式作废」的 BLOB，会被原样载入喂给新树、每次 resume 都 `ParentHashMismatch`，且回填分支永不触发——一个**安静的、永久的**「这个会话再也续不上传」。判据改为长度校验（outboard 长度是 chunk group 的确定性函数，**它本身就是格式版本号**），将来再调 block size 也不需要迁移。

### 3. PrepareProgress 升级为三端 typed event

- 桌面 `PrepareProgress` 进 `collect_events![]`，废除 per-call `tauri::ipc::Channel` 与 `PrepareChannelGuard` 的 DashMap 路由。**这不是新开先例**：该 Channel 出生于 2026-02（`ff47e1dd`），比 tauri-specta typed events 引入早三个月，2026-05 迁移时（`09fc459c`）被排除且 commit body 零理由；Web 与移动本来就走广播总线。
- 三端各自把进度按 `preparedId` 落进 store（Web 已有形状正确但零读者的 `prepares` 表，移动端 `routeEventToStores` 里 PrepareProgress 根本没有 case、落进 `default` 的 `console.warn`），使进度**跨页面存活**。
- MCP、Web 收件箱转发、resume 回填三条静默路径由此同时获得反馈——它们是同一处修复。

### 4. 顺带清理

- 上述四处「同一遍构建」的文档漂移断言，加上核验发现的第五处 `entity/src/transfer_file.rs:55` 与 `rust-backend.md:604-609`（后者讲的 16 KiB 非对齐读失效形态在改动后每个数字都错）。
- 桌面 `host/file_source/path_ops.rs` 的 `compute_hash` / `compute_hash_with_progress` / `verify_checksum` 一族**零外部调用者**（接收侧 `receive-staging-publish` 那次删除留下的残骸），连同 `src-tauri` 的 `blake3` 依赖一并删除；`crates/core` 的 `blake3` 只在 `tests/` 出现，移入 `dev-dependencies`。
- Web `latestPrepareProgress` 从不清空导致的 stale 渲染（第二次点发送的头 200ms 显示上一批的完成态，且因终局事件 `current_file` 是空串而文件名位置空白）。
- 桌面 `scanSources` 阶段零反馈——那才是真正的「选完文件之后」，大目录时界面纯粹无响应。

## Capabilities

### New Capabilities

- `send-preparation`: 发送前置准备（prepare）阶段的契约——单遍流式读同时产出 `checksum` 与验签树、进度必须覆盖该阶段的全部真实工作量、进度事件的三端投递与跨页面存活、宿主读取的严格长度判据。
- `bao-chunk-verification`: 逐块验签树的规格——chunk group 与传输块尺寸的对齐关系、`root ≡ 扁平 BLAKE3` 不变量、outboard 有效性判据（长度即格式版本）、`encode_proof` 的对齐前提，以及「bao 顺序读」这条实现事实必须由护栏测试钉死。这块能力自 `9323d4f3`（2026-07-18）落地以来从未进过 spec 层。

### Modified Capabilities

- `transfer-data-plane`: 新增一条规则——**数据面协议名同时承载帧格式与验签树形状**，任何使旧端解码/验签硬失败的改动 MUST bump 协议名。这条规则今天只活在 `protocol.rs:16-36` 的注释里，本次是它第一次被规格化，也是本次 BLOCK_SIZE 改动（`/3` → `/4`，不再注册 `/3` 与 `/2`）的判据来源。

  （`transfer-protocol` 那份 spec 只覆盖控制面类型，且已漂移——它仍写着 `checksum` 是 SHA256 hex、`OfferResult` 携带 256-bit 对称密钥，两者在 wire v2 都已不成立。本次不顺手改它：那是另一笔独立的规格债，混进来会让本 change 的 delta 失焦。）

## Impact

**Rust — 传输核心**

- `crates/transfer/src/bao.rs`：`BLOCK_SIZE` 常量、`build_outboard_from_source` 的进度形态、新增 outboard 长度判据的公开函数（不把 `bao_tree` 类型泄漏到 flow 层——全仓 `bao_tree` 仅此文件 import）、`encode_proof` 的对齐断言、护栏与边界测试。既有测试 `roundtrip_from_16kib_offset`（:398）与 `overlong_host_read_is_rejected_not_panic`（:383）在新粒度下**必红**。
- `crates/transfer/src/flow/prepare.rs`：删除 hasher 循环与 `debug_assert_eq!`，进度改由 reader 驱动。`prepare.rs:56-63` 那条比端口契约更严的宿主违约检查要搬到 reader 内，且判据必须是 `chunk.len() != len.min(size - offset)`——裸 `!=` 会在每个非 256 KiB 整数倍的文件末尾误报。
- `crates/transfer/src/flow/resume/{mod.rs,plan.rs}`：outboard 失效判据从 `is_empty()` 换成长度校验。
- `crates/transfer/src/protocol.rs`：协议名 bump、删除旧版本注册。
- `crates/transfer/src/progress.rs`、`events.rs`：事件定义不变（`bytes_hashed` 字段名合并后依然字面准确，不改名）。

**Rust — 宿主与桌面**

- `src-tauri/src/setup.rs`（`collect_events!`）、`events.rs`（新增 Event newtype）、`host/event_bus.rs`（删 DashMap 路由）、`commands/transfer.rs`（删 Channel 入参与 `PrepareChannelGuard`）、`src/lib/bindings.ts`（自动重新导出）。
- `src-tauri/src/host/file_source/path_ops.rs` + `file_source.rs`：删死代码。
- `src-tauri/Cargo.toml`、`crates/core/Cargo.toml`：blake3 依赖调整。

**三端前端**

- 桌面 `src/routes/_app/send/{index.lazy.tsx,share-target.lazy.tsx}` + `-components/prepare-progress-bar.tsx`、新增 store 落点。
- 移动 `mobile/src/core/event-bus.ts`（补 case，顺带把同样漏网的 `PairingCompleted` 变成显式空 case 或补上，否则 `default` 分支永远不是「真未知」信号）、`stores/transfer-store.ts`、两个发送页（`select-device.tsx` / `share-target.tsx`，两页的渲染门今天不一致，合并时统一）。
- Web `docs/app/app/_lib/store.ts`（删 `latestPrepareProgress` 旁路、启用 `prepares` 表并补裁剪与测试）、`_components/send-panel.tsx`、`forward-to-device-dialog.tsx`。

**不受影响（已核实，避免白改）**

- **Web IndexedDB 不需要提 `DB_VERSION`**：`FileRowDef.outboard` 是 `#[serde(skip)]`（`crates/web/src/store.rs:891`），outboard 从来没进过 IndexedDB；`completed_chunks` 绑的是 `CHUNK_SIZE`（不变），不是 chunk group。
- **SQLite schema 不变**：列类型仍是 BLOB，变的是内容语义，不是结构。
- **三端 `read_source_chunk` 实现零改动**：契约本就要求精确读任意 offset/length。
- `crates/webrtc-p2p` 的 16 KiB（WebRTC 消息上限）与本次无关，grep 时别误伤。

**文档**

`dev-notes/knowledge/{iroh-migration.md,net-kernel.md,rust-backend.md}`（`/dev-workflow` 当现行事实加载，必须同 PR 改）、`crates/transfer/src/wire/mod.rs:22`、`crates/entity/src/transfer_file.rs:55`、`crates/web/README.md`、`docs/content/docs/security.mdx:48`、`openspec/changes/receive-staging-publish/design.md:223`、`dev-notes/blogs/transfer/01-bao-tree-per-chunk-verify.md` 与 `transfer-architecture/04-bao-tree-verified-streaming.md`（后两者的论证整节反转，建议补后续篇而非原地改写——「为什么当初选 16 KiB、后来为什么改」正是最值得留档的那类推导）。

**CI 门禁**

改动横跨 native 与 wasm 且触及 `crates/transfer`：`./scripts/check-wasm.sh`（含 `--clippy`）、`cargo test --workspace`、`./scripts/test-wasm.sh` 全部必跑。
