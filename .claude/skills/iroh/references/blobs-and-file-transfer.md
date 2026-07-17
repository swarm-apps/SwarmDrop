# Blobs 与文件传输：bao-tree / iroh-blobs / sendme / dumbpipe

iroh 1.0.2 / iroh-blobs 0.103.0 / bao-tree 0.16.0 / sendme 0.36 / dumbpipe 0.39 · 调研日期 2026-07-17

> **版本警告先行**：`sendme/Cargo.lock` 锁的是 **iroh 1.0.0**（不是本地 iroh-study 那份 1.0.2），`iroh-blobs 0.103.0`。**iroh-blobs 源码不在 iroh-study 里**，在 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/iroh-blobs-0.103.0/`（发布包里 cargo 规范化过 Cargo.toml，原件在 `Cargo.toml.orig`）。

## 一句话选型

| 你想要 | 用 | 代价 |
|---|---|---|
| **只要逐块验签 / 可信断点续传** | **bao-tree**（纯算法，不依赖 iroh 网络栈） | 约 0.39% outboard 存储；chunk/传输/store 全自理。**低风险、可回退、与「迁不迁 iroh」完全解耦** |
| 内容寻址 + 白拿续传/校验 + pull 模型 + 多源下载 | **iroh-blobs**（读 sendme 学怎么用） | 吞下全栈：README 自述非生产质量 + 四条硬伤 + 收件箱所有权要交出去 |
| 自研协议 + 保留现有传输层 + push 模型 | **dumbpipe 形状**（13 行 lib.rs 就是全部协议定义） | 一根裸管子，chunk/续传/校验全自理 |

**迁 iroh 的最低风险路径是 dumbpipe 形状**：保留全部自研传输逻辑，只把底层流换掉。

---

# 第一部分：bao-tree —— 只要验签，不要 blob store

- **成熟度**：**production**
- **依据**：
  - 版本 0.16.0；README **无任何免责声明**（与 iroh-blobs README 形成鲜明对比），有 CI/docs.rs/crates.io 徽章，自述 *"The merkle tree used for BLAKE3 verified streaming"*
  - 被 iroh-blobs 0.103.0 以 `bao-tree = "0.16"` 生产依赖，并随 sendme 0.36.0（2026-06-15 发布）分发
  - **5 个 open issue 全是 API 打磨类**（#56 改名 tokio-fsm、#57 暴露 truncate_ranges、#59 EmptyOutboard 语义困惑、#76 keyed bao、#11 实现 bao-tree diff），**无正确性缺陷**
  - ⚠️ **HEAD 本身就是 release commit**（`Release v0.16.0`，2025-11-04）—— 即默认分支 8.5 个月**零提交**（不只是「无新 release」）。属「小而完备、已收敛」型低 churn，不是无人维护，但读者应自行判断「稳定」与「停滞」
  - GitHub API：未归档、35 stars、last push 2025-11-04
- **入口**：`bao-tree/src/lib.rs:1-204`（全库最好的文档，含 sync/async 两个可跑的端到端示例）；outboard 实现看 `src/io/outboard.rs`；trait 定义看 `src/io/sync.rs:46-100`

## 它做什么

把「整文件一个 hash」拆成一棵**可对任意 range 独立验签**的树。核心是 outboard（树的分支哈希，与原文件分离存放，**原文件一字节不改**）+ 运行时可配的 chunk group size + 多 range 集合查询（一次问 `[0..1000, 5000..6000]`）。

**不含任何网络/存储/store 概念**，只有 encode/decode/outboard 三件事。

## 依赖极轻，且不依赖 iroh

`bao-tree/Cargo.toml:16-38` 全部依赖：range-collections、smallvec、bytes、futures-lite(opt)、**iroh-io(opt)**、positioned-io、genawaiter(opt)、tokio(opt，仅 `features=["sync"]`)、blake3、serde(opt)。

> `iroh-io` 是**独立的 io 抽象 crate**（AsyncSliceReader 等），**不是 iroh 网络栈**，且仅被 `tokio_fsm` feature 启用。

`default = ["tokio_fsm", "validate", "serde", "fs"]` → 用 `default-features = false, features = ["validate"]` 可只要同步 + 验证路径，**连 tokio/iroh-io 都不引**。

依赖规模粗略量级（Cargo.lock `[[package]]` 计数，含 dev-deps）：**bao-tree 273** vs iroh-blobs 510 vs sendme 504。
MSRV：bao-tree `rust-version = 1.75`、edition 2021（vs iroh-blobs 1.91）—— 新 edition 可依赖旧 edition crate，无兼容问题。

**这就是「只要验签不要 blob store」路线成立的技术前提：能，而且很干净。**

## outboard 的本质

每个树节点存一对子哈希（64 字节），与原文件分离。任意 range 能独立验签，是因为服务端把该 range 到 root 的**兄弟路径**一起发过来，接收端自底向上重算到 root 与已知 root hash 比对 —— 这就是 Merkle proof，只是 blake3 原生支持。

Outboard trait 的 load 签名即答案（`src/io/sync.rs:46-53`）：

```rust
fn load(&self, node: TreeNode) -> io::Result<Option<(blake3::Hash, blake3::Hash)>>
```

wire 上传的是（`src/io/mod.rs:101-106`）：

```rust
enum BaoContentItem { Parent(Parent), Leaf(Leaf) }
struct Parent { node: TreeNode, pair: (blake3::Hash, blake3::Hash) }  // mod.rs:20-25，64 字节兄弟哈希对
struct Leaf   { offset: u64, data: Bytes }                            // mod.rs:79-84，真实数据块
```

验签靠 blake3 hazmat 原生 API 而非手搓：`lib.rs:235-247` `hash_subtree()` 用 `hasher.set_input_offset(start_chunk*1024)` + `finalize_non_root()`；`lib.rs:249-262` `parent_cv()` 用 `merge_subtrees_root/merge_subtrees_non_root`。

**decode 必然验签，没有 skip 选项** —— `bao-tree/src/lib.rs:70-71` 原文：*"This function will perform validation in any case, there is no variant that skips validation since that would defeat the purpose of verified streaming."*

> ⚠️ **「我已经在用 blake3 了」是个认知陷阱** —— 用同一个哈希函数，但**不是同一个东西**。verified streaming 的价值不在哈希算法，而在 **outboard 这棵 Merkle 树**：没有它，**收到的每个 chunk 在文件收完前都无法验证**，坏块要等整文件收完才暴露，且续传时无法证明磁盘上已有的半个文件没被篡改 —— **续传只能建立在「信任对端」之上**。

## 存储开销 ≈ 0.39%（16 KiB chunk group）

公式来自源码：
- `fn outboard_hash_pairs(&self) -> u64 { self.blocks() - 1 }`（lib.rs:439-441）
- `pub fn outboard_size(&self) -> u64 { self.outboard_hash_pairs() * 64 }`（lib.rs:446-448）

BlockSize 语义（`src/tree.rs:140-161`）：`from_chunk_log(4)` → `bytes() = BLAKE3_CHUNK_SIZE << 4` = 16 KiB；`BlockSize::ZERO` = 1024 B。

iroh 选定值（`iroh-blobs/src/store/mod.rs:17-18`）：

```rust
/// Block size used by iroh, 2^4*1024 = 16KiB
pub const IROH_BLOCK_SIZE: BlockSize = BlockSize::from_chunk_log(4);
```

（磁盘扩展名 `.obao4` 里的 4 就是它）

**实算**：1 GiB → 65536 blocks → 65535 pairs × 64 B = 4,194,240 B = **0.3906%**；10 GiB 同为 0.3906%；1 MiB = 0.3845%。若 block_size=0（1 KiB）→ 64/1024 = **6.25%**。

**chunk group 是这个库存在的理由。** 16 KiB 在 `bao-tree/src/lib.rs:88` 与 :152 的示例注释里被称为 *"a good default for most cases"*（注意：**README 从未提及 16 KiB**，别去那里找）。

## wire format 兼容性是互斥的

与 oconnor663 的 bao crate 兼容的条件（lib.rs:196-203）：block size = 1024、8 字节 LE size 前缀、单 range —— 即**放弃 chunk group**，outboard 开销升到 6.25%。**要兼容就别要 chunk group，反之亦然。**

## pre-order vs post-order —— 真实的工程选择点

两种类型并存：`PreOrderOutboard<D>`（`src/io/outboard.rs:103-111`）与 `PostOrderOutboard<D>`（:128-131）。

README 原文：*"It allows to define both pre- and post order outboard formats as well as custom outboard formats. **Post order outboard formats have advantages for synchronizing append only files.**"*

语义支撑在 `enum PostOrderOffset { Stable(u64), Unstable(u64) }`（lib.rs:283-289），注释分别为 *"the node is stable and won't change when appending data"* / *"the node is unstable and will change when appending data"*。

两种 outboard 均有 sync 与 fsm 双实现，转换用 `pub fn copy(from: impl Outboard, mut to: impl OutboardMut)`（`io/sync.rs:647`）。iroh-blobs 落盘选的是 **pre-order**（`.obao4`）。

**若引 bao-tree，这是要拍的第一个设计决策**：发送端对**已完整的文件**建 outboard → PreOrderOutboard 即可；接收端边收边写、文件在增长 → post-order 的 Stable/Unstable 语义能避免每次追加都重写整个 outboard。**这直接关系到续传时 outboard 的写放大。**

## Outboard trait 可自实现 → 可进任意 KV/SQL

```rust
trait Outboard {
    fn root(&self) -> blake3::Hash;
    fn tree(&self) -> BaoTree;
    fn load(&self, node: TreeNode) -> io::Result<Option<(blake3::Hash, blake3::Hash)>>;
}
trait OutboardMut {
    fn save(&mut self, node: TreeNode, hash_pair: &(blake3::Hash, blake3::Hash)) -> io::Result<()>;
    fn sync(&mut self) -> io::Result<()>;
}
```

`src/io/sync.rs:44-45` 原文建议：*"If you use a different storage engine, you can implement this trait for it. E.g. you could store the hashes in a database and use the node number as the key."*

## 关键 API 要点

- `encode_ranges_validated` 的文档明确：*"It is possible to encode ranges from a partial file and outboard. This will either succeed if the requested ranges are all present, or fail as soon as a range is missing."* → **断点续传时发送端可以直接对着「只下了一半的文件」编码，天然适配**
- `valid_ranges(&ob, &decoded, &ChunkRanges::all())` → 问本地「我已经有哪些 range 是**验证过**的」，这才是可信的 fetch plan
- 导入路径有效：虽然 `valid_ranges` 定义在私有 `mod validate`（sync.rs:658）内，但 `sync.rs:908` 有 `pub use validate::{valid_outboard_ranges, valid_ranges};`

## 何时不用 bao-tree

1. 只传小文件（<几 MB）—— 整文件重传比维护 outboard 便宜
2. 需要 wire format 与 oconnor663 bao 互通但又想用 chunk group —— 互斥
3. 想要开箱即用的多源下载/去重/GC —— 那是 iroh-blobs 的活

---

# 第二部分：iroh-blobs —— 要不要整套 store

## ⚠️ 0.103.0 自述「不具备生产质量」，且退不回 0.35

README 第 3 行（经 `src/lib.rs:2` 的 `#![doc = include_str!("../README.md")]` 注入为 crate 文档）：

> **NOTE: this version of iroh-blobs is not yet considered production quality. For now, if you need production quality, use iroh-blobs 0.35**

HEAD = **e82cbdc**（2026-06-15，`chore: Release iroh-blobs version 0.103.0`）—— **该声明就写在 0.103.0 的发布 commit 里**，不是历史残留。

**而 0.35 是旧架构，退不回去**（已经 docs.rs 独立核实）：

- `https://docs.rs/iroh-blobs/0.35.0/iroh_blobs/api/index.html` → **HTTP 404**，而 0.103.0 同路径 → 200。**`api` 模块（`remote()` / `execute_get` / `local()` 的宿主）在 0.35 整个不存在**
- 0.35 的 `store::fs` 只导出 `Store`（另有 BatchOptions/InlineOptions/Options/PathOptions），**无 `FsStore`**。0.103 的 `FsStore::load` 在 `src/store/fs.rs:1390`，`execute_get` 在 `src/api/remote.rs:638`
- 旁证：`CHANGELOG.md` 版本头从 0.35.0（2025-05-12）**直接跳到** 0.101.0（2026-05-08），0.36–0.100 整段不在册

**存在张力**：iroh 主体已发 1.0（API 稳定），容易让人以为整个 iroh 生态都稳了；但 blobs 仍是 0.103 且自述非生产质量。n0 自家官方工具 sendme 0.36 照样发在 0.103 上，说明这句话更像「API 仍会变」而非「跑不动」——但**升级会破坏兼容，且传输主力恰恰是那个没 1.0 的 crate**。

**实锤未修缺陷**：issue **#233**（fs store Poisoned panic，2026-05-22 开、**0 评论无人认领**）、issue **#207**（WASM + irpc 不可用，2026-01-14 开、更新到 2026-06-15）。

- **入口**：`iroh-blobs/src/lib.rs`（模块地图）→ 续传心智看 `src/api/remote.rs:51-65` + :340-402 → store 后端看 `src/store/mod.rs` → 架构背景看 `DESIGN.md`

## Store 后端只有三个，浏览器只剩内存

```rust
// iroh-blobs-0.103.0/src/store/mod.rs:1-14
//! Use the [`mem`] store for sharing a small amount of mutable data,
//! the [`readonly_mem`] store for sharing static data, and the [`fs`] store
//! for when you want to efficiently share more than the available memory and
//! have access to a writeable filesystem.
#[cfg(feature = "fs-store")]
pub mod fs;
mod gc;
pub mod mem;
pub mod readonly_mem;

// Cargo.toml.orig:91-92
default = ["hide-proto-docs", "fs-store", "rpc"]
fs-store = ["dep:redb", "dep:reflink-copy", "bao-tree/fs"]
```

`fs` 是 **feature 门控**（`#[cfg(feature = "fs-store")]`）而非 **target 门控**（不是 `#[cfg(not(wasm_browser))]`）。

> ⚠️ **0.103 里没有可插拔的 store 后端 trait**：`Store` 是 **struct**（`api.rs:213`），全库 `grep 'pub trait'` 在 `store/mod.rs` 与 `api.rs` 零命中（只有 RangeSetExt/SliceInfoExt/Combine/Reset/Params 等工具 trait）。自定义 store 意味着实现 `api::Store` 背后的 irpc actor/service，**不是 impl 一个 trait**。这正解释了 issue #84 为何是在**申请**该 trait。

## 四条硬伤

### 1. 全库零加密原语 —— 与应用层 E2E 加密正面冲突

`grep -rniE "encrypt|chacha|aead|xsalsa|cipher" src/` 在整个 src/ 下 **0 匹配**。落盘就是裸文件：`store/fs/options.rs:28-42` 的 `data_path()` = `{hash}.data`、`outboard_path()` = `{hash}.obao4`。

鉴权唯一钩子是 provider events（见下文第三部分）。但 `BlobsProtocol::new(store: &Store, events: Option<EventSender>)`（`net_protocol.rs:72-79`）传 None 时 `events.unwrap_or(EventSender::DEFAULT)` —— **默认不拦截**，README 示例正是传 None。

**若你的应用层已有自己的加密，必须二选一，没有中间路线**：

- **(A) 先加密再入库** → hash 变成密文哈希；若 nonce/密钥按接收方派生，同一文件对不同接收方是不同 blob → **内容寻址的去重/复用价值归零**，只剩一个笨重的 store
- **(B) 存明文靠 QUIC TLS** → blob 在磁盘上是明文、**hash 即凭证**（谁有 hash + 能连上就能拉，除非自己接 Intercept 写鉴权）

**这条比性能/体积都更该先拍板。**

### 2. 浏览器端只有 MemStore、零持久化

`browser-blobs/README.md` 第一段写死：

> This example runs iroh-blobs in the browser, compiled to web assembly. **For now, only the in-memory store works in the browser, so there is no persistence.**

- CI wasm job 唯一构建命令是 `cargo build --target wasm32-unknown-unknown --no-default-features`（`.github/workflows/ci.yaml:309`），而 default 含 `fs-store` → wasm 构建**必然不含 fs-store**
- fs store 靠 redb（`store/fs/meta.rs:15`）—— 浏览器无文件系统
- `grep -rniE "indexeddb|idb::|opfs|origin_private|web_sys|local_storage" src/ Cargo.toml` **0 匹配** → **无任何浏览器持久化后端**
- wasm 下 `import_path` 直接返回错误（比「降级为拷贝」更狠）：

```rust
// iroh-blobs-0.103.0/src/store/mem.rs:758-764
#[cfg(wasm_browser)]
async fn import_path(cmd: ImportPathMsg) -> Result<ImportEntry> {
    Err(n0_error::anyerr!("import_path is not supported in the browser"))
}
```

对照 `:767` 的 `#[cfg(not(wasm_browser))]` 真实现。**浏览器下只能走 `import_bytes`（`mem.rs:755`）全量入内存。**

> ⚠️ **常见误传纠正 1**：「必须关 default features，否则 **redb** 编不过」——**redb 编 wasm 完全没问题**。实测：新建 crate 仅依赖 redb 4.1.0，`cargo check --target wasm32-unknown-unknown` → Finished，零报错。机理是 redb 自带 fallback 后端（`src/tree_store/page_store/file_backend/mod.rs`：`#[cfg(not(any(windows, unix, target_os = "wasi")))] pub use fallback::FileBackend;`，wasm32-unknown-unknown 正好落进这条分支）。
>
> **「必须关 default features」结论成立，但报错不在 redb 而在 `mio`**：iroh-blobs 默认 features 编 wasm32 时 48 个 error 全在 `mio v1.2.2`（E0308/E0599，UdpSocket/IoSource），经 tokio 由 bao-tree/iroh-io/irpc/iroh-relay 等多路拉入。**归因给 redb 会被一条 `cargo check` 打脸。**

> ⚠️ **常见误传纠正 2**：「iroh-blobs 没有 wasm cfg、靠 feature flag 做跨平台」—— 它**有** build.rs（:7 定义 `wasm_browser: { all(target_family = "wasm", target_os = "unknown") }`，Cargo.toml:79 依赖 cfg_aliases 0.2.1），src/ 下有 3 处 `cfg(wasm_browser)`（`store/util.rs:76`、`store/mem.rs:758`、`:767`）。**字面 grep `target_family` 会零命中 —— 这是 cfg alias 造成的间接层，别被骗。**

**关于内存上限**：源码只说「受可用内存限制」（`mem.rs:1-9`：*"Being a memory store, this store has to import all data into memory before it can serve it. So the amount of data you can serve is limited by your available memory."*），**没给任何数字**。

> ⚠️ **别写「2GB 直接 OOM」** —— 这个具体数字**在仓库和 issue 里均无出处**（`gh search issues --repo n0-computer/iroh-blobs "2GB OR OOM"` 返回空）。而 wasm32 的**架构硬上限是 4GiB**（`2^16 pages × 64KiB`，非 2GB）。**正确表述**：「mem store 全量入内存，容量受可用内存限制；wasm32 寻址硬上限 4GiB，实际远低于此（mem store 同时存 data + outboard 两份，import 期还有额外拷贝）」。真要给数字必须自己压测。

### 3. fs store 的 Poisoned panic（issue #233 未修）

源码可验证的路径：

1. `bao_file.rs:519-521` `pub fn take(&mut self) -> Self { std::mem::replace(self, BaoFileStorage::Poisoned) }` —— take 必然留下 Poisoned
2. `fs.rs:991-1008` `async fn persist(&self)` —— `self.state.send_if_modified(|guard| { let BaoFileStorage::Partial(fs) = guard.take() else { return false; }; ... fs.sync_all(&path) ...; false })`，**两条分支都不还原 guard**
3. `bao_file.rs:409-411` `BaoFileStorage::Poisoned => { panic!("poisoned storage should not be used") }`

唯一调用方是 `fs.rs:230-236` `EmParams::on_shutdown` → `state.persist().await`，而 `enum ShutdownCause { Idle, Soft, Drop }`（`fs/util/entity_manager.rs:18-25`，Idle = *"The entity is shutting down gracefully because the entity is idle"*）→ **进程存活期间的空闲驱逐也会走这条路**，而 BaoFileHandle 是 "cheaply cloneable handle"。

> ⚠️ **精确化**：Poisoned **不是永久状态**。entity manager 在 recycle 的两条分支上都会调 `Reset for BaoFileHandle`（`bao_file.rs:532-535`）→ `send_replace(BaoFileStorage::Initial)`（`entity_manager.rs:482` 及 recycle()）。所以它是 persist() 与 reset() 之间的**竞态窗口**。
>
> **危害仍在**：`Initial` 同样 panic；issue #233 的生产 trace 证明该窗口可达。

issue #233 元数据：OPEN、作者 mkdir700、2026-05-22、**0 评论**；症状级修复 PR #214 自 2026-03-19 OPEN、未合并。⚠️ 该 issue **自述为 AI 辅助撰写**（"drafted with the help of Claude Code"）——但上述代码路径已逐行核实存在。

**这正是 README「非生产质量」的具体注脚**，且踩中最核心的场景（桌面端持久化 store 长时间运行 + 大量 blob handle 空闲驱逐）。panic 炸的是 store worker task。

### 4. `FsStore::load` 自建一个独立的 multi_thread tokio runtime

`fs.rs:1398-1409`：

```rust
let rt = tokio::runtime::Builder::new_multi_thread()
    .thread_name_fn(|| format!("iroh-blob-store-{}", THREAD_NR.fetch_add(1, Ordering::Relaxed)))
    .enable_time().build()?;
```

关闭路径 `fs.rs:833-844`：`impl Drop for RtWrapper { fn drop(&mut self) { if let Some(rt) = self.0.take() { tokio::task::block_in_place(|| { drop(rt); }); } } }`

`FsStore::load(root)`（:1390-1395）只是包一层默认 Options + `{root}/blobs.db`。GC 也挂在上面（:1425-1427）。显式关停 API：`Store::shutdown()`(`api.rs:276`) / `wait_idle()`(`api.rs:293`)。

**代价**：进程里多一组 `iroh-blob-store-*` 线程，非你所控。且 **`block_in_place` 在 current_thread runtime 上会 panic** —— 若某处在单线程 runtime 里 drop 掉 store 就会炸。**这条在「能不能塞进现有架构」上比依赖体积重要得多。**

## 值得抄的设计：range-set 差集续传

**Bitfield 不是位数组，是 range set**（`src/api/proto/bitfield.rs:28-33`）：

```rust
pub struct Bitfield { pub(crate) size: u64, pub ranges: ChunkRanges }
// ChunkRanges = range_collections::RangeSet2<ChunkNum>
```

序列化就是 size + 一串边界 u64（:41-48）—— 极紧凑，不随文件大小线性膨胀。

`missing()` 本体就一行集合减法（`src/api/remote.rs:341-346`）：

```rust
pub fn missing(&self) -> GetRequest {
    let Some(root_requested) = self.requested_root_ranges() else {
        return GetRequest::new(self.request.hash, ChunkRangesSeq::empty());
    };
    let mut builder = GetRequest::builder().root(root_requested - &self.bitfield.ranges);
    // HashSeq 子节点同理：requested.difference(&children.bitfields[hash].ranges)
}
```

三步心智写在 `remote.rs:51-65` 的 trait 文档里：

```rust
let local = store.remote().local(hash_and_format).await?;  // 查本地
if local.is_complete() { return Ok(()); }
let req: GetRequest = local.missing();                     // 差集
store.remote().execute_get(conn, req).await?;
// 一步到位版：store.remote().fetch(conn, content).await?;  (fetch_sink 内部就是上面三步)
```

**核心：「我要这个 hash 的哪几段」是协议的一等公民，不是应用层自己加的 offset 字段。** `root_requested - &self.bitfield.ranges` 是**集合差运算**——支持**任意空洞的乱序续传**，这是并行/多源下载能力的基础。

**不下载就拿到「已验证大小」的技巧**（`get/request.rs:230-232`）：只请求 last_chunk —— bao 树的最后一块携带 size 证明。`max_size` 参数防御恶意超大 hash seq（`get/request.rs:241-247`，sendme 传 32MiB）。

### 验签性质：每块落盘前就能验

**注意 `remote.rs:341-346` 只体现「range 位图集合差」，不体现验签。** 验签的依据在别处：

```
// iroh-blobs README.md:9
The provider answers with the requested data, encoded as BLAKE3 verified streams, on the same QUIC stream

// iroh-blobs src/protocol.rs:12-14
//! Data integrity is considered more important than performance. Data will be validated both on
//! ... responses to range requests contain sufficient information to validate the data
```

**因此可以从另一个完全不同的 peer 续传同一个 hash，且每块落盘前验签，恶意 peer 无法投毒。这是内容寻址相对会话式传输的结构性优势。**

## 分块：blake3 chunk vs chunk group

```
// src/protocol.rs:25-29
//! It is possible to request entire blobs or ranges of blobs, where the minimum granularity is a
//! chunk group of 16KiB or 16 blake3 chunks. The worst case overhead when doing range requests
//! is about two chunk groups per range.

// src/protocol.rs:98-102
//! Ranges are always given in terms of 1024 byte blake3 chunks, *not* in terms
//! of bytes or chunk groups. The reason for this is that chunks are the fundamental
//! unit of hashing in BLAKE3.
```

**两个粒度别混：**
- `ChunkRanges` 的**坐标系**是 1024B blake3 chunk（协议层寻址单位）——`ChunkRanges::chunks(..10)` 的 10 指 blake3 chunk
- 实际**传输/落盘/校验**按 16KiB chunk group（优化单位，减少 outboard 元数据）

**chunk 大小不是你的自由参数**——它由 blake3 树形哈希结构决定。且正因为固定，接收端才能对任意 range 做独立的密码学校验（不需要先收完整个文件才能验 hash）。**这是「自选 chunk size」换不来的性质。**

## FsStore：不是「每个 blob 四个文件」

```rust
// iroh-blobs-0.103.0/src/store/fs/options.rs:28-42
pub fn data_path(&self, hash)     -> PathBuf { ... format!("{}.data", hash.to_hex()) }
pub fn outboard_path(&self, hash) -> PathBuf { ... format!("{}.obao4", hash.to_hex()) }
pub fn sizes_path(&self, hash)    -> PathBuf { ... format!("{}.sizes4", hash.to_hex()) }
pub fn bitfield_path(&self, hash) -> PathBuf { ... format!("{}.bitfield", hash.to_hex()) }
```

（后缀里的 `4` 对应 `IROH_BLOCK_SIZE` 的 chunk_log=4。换 block size 会让老 outboard 不兼容，所以后缀带版本号。）**`{hash}.bitfield` = 已收到的 range set = 续传断点。** 元数据另用 redb（`store/fs/meta.rs:15`），入口 `FsStore::load` 默认 `{root}/blobs.db`。

> ⚠️ **但「每个 blob 都有这四个文件」不成立**，实际是**四类存储位置按大小与 ImportMode 择一**：
>
> | 位置 | 条件 |
> |------|------|
> | **Inline** | data/outboard **< 16KiB** 时内联进 redb（`fs/options.rs:71-78` 的 `InlineOptions::default()`；`fs.rs:22-23` 模块文档：「as well as **inlined data and outboard data for small files**」） |
> | **External** | `ImportMode::TryReference` → `DataLocation::External(vec![path], size)`（`fs.rs:1042-1044`）——**store 里根本没有 `<hash>.data`**（sendme 发送端正是这个模式） |
> | **NotNeeded** | 单 chunk group 的小 blob 无 outboard（`fs.rs:1065`） |
> | **Owned** | 完整的 sidecar 文件组 |

**这也印证了浏览器的后果链**：无 fs-store → 无 `.bitfield` 落盘 → **刷新即从零开始**。

### bitfield 与数据不是单一事务

```rust
// iroh-blobs-0.103.0/src/store/fs.rs:318-334
async fn write_batch(&self, batch: &[BaoContentItem], bitfield: &Bitfield) -> io::Result<()> {
    self.state.send_if_modified(...);                // ① 改内存/文件状态
    self.global.db.update(self.id, update).await?;   // ② 更新 redb
}
```

**两步，非单一原子事务。** `fs.rs:50-57` 模块文档反而明确承认不一致窗口存在：

> some parts of the persistent state won't make it to disk... It is also **not guaranteed that all write operations will make it to disk**. The on-disk store will be in a consistent state, but might miss some writes in the last seconds before shutdown

**准确说法：iroh 不是靠事务消灭不一致，而是靠内容寻址让 bitfield 可从 data+hash 重算，代价是下次启动的重校验延迟。**

## 心智税：tag / GC / TempTag

blob 默认会被 GC，必须靠 tag / TempTag 保活：

```rust
pub struct GcConfig {
    pub interval: Duration,
    pub add_protected: Option<ProtectCb>,   // ⚠️ 字段名是 add_protected，不是 protect
}
```
（`src/store/gc.rs:152`，struct 跨 138-153）

protect 回调文档（gc.rs:142-145）：*"The callback is called before each garbage collection run. It gets a `&mut HashSet<Hash>` ... All hashes that are added to the HashSet will be protected from garbage collection during this run."*

add 系 API 返回的 `AddProgress` 必须显式收尾成一种保活形态：`with_named_tag(name)` / `with_tag()` / `temp_tag()`（`api/blobs.rs:672` 与 :687；注意 :545-560 那组是 **BatchAddProgress** 的 impl，它委托给前者）。

**迁移面的真实尺寸**：不是「多一个存储层」，而是**收件箱的所有权要交出去**。若已有关系型收件箱（记录「这个文件属于哪次传输、来自谁、什么时候」），而 iroh-blobs 只认 hash + tag，两套并存 = 双写 + 要写 ProtectCb 把收件箱里的 hash 全捞出来防误删。

## 「增量同步」有歧义，写文档要写清

**iroh-blobs 的「增量」= 同一 hash 未下完的部分补齐（bitfield 集合差）。**

**不是** 文件 v1→v2 之间只传改动量。`grep delta|rsync|版本间 diff` **未找到任何 rsync 式跨版本 delta 同步 API**——内容寻址下**文件一改 hash 全变**，每次都是全新 hash、全量重传。

（**白送但 sendme 没吃的能力**：`api::downloader` 的多源并行下载 `SplitStrategy::Split`，`src/api/downloader.rs:317-320`。对「一份文件发给多台设备」的群发场景有实质价值；在 libp2p 下要自造相当于重写一个 BitTorrent 调度器。）

## 何时用 iroh-blobs

1. 桌面端、要多源并发下载（`api::downloader`）、跨设备/跨会话去重、或传目录树（HashSeq/Collection）
2. 能接受「blob 明文落盘、hash 即凭证」
3. 愿意等它 1.0

**想抄心智但不想引库时**：`remote.rs:340-402` 的 `missing()` 和 `bitfield.rs` 值得直接读。

---

# 第三部分：pull vs push —— 关键的模型冲突

**iroh-blobs 是 pull 模型**（收方拉）。push API 存在但：

1. `remote.rs:573` `pub fn execute_push(&self, conn: Connection, request: PushRequest) -> PushProgress` 是公开的
2. 但其内层 `execute_push_sink`（`pub(crate)`）文档（remote.rs:588-590）原文：*"Note that many nodes will reject push requests. Also, this is an experimental feature for now."*
3. `PushRequest(GetRequest)`（`protocol.rs:567`）—— 推方自己决定推哪些 range，**不存在** pull 侧 `local.missing()` 那样的自动协商
4. **最硬的一条**：`EventMask::DEFAULT`（`provider/events.rs:189-198`）里 `push: RequestMode::Disabled` —— **push 默认即禁用**
   - `events.rs:165-166` 注释：*"push requests are disabted by default, as they can write to the local store"*（原文拼写如此）
   - `events.rs:200-204`：*"If you want to enable push requests, **which can write to the local store**, you need to do it manually. **Providing constants that have push enabled would risk misuse.**"* —— **官方明确拒绝提供开启 push 的便捷常量**

**这就是「many nodes will reject push requests」的机制成因**：不是「作者说是实验性」，而是**协议默认关闭、需手动开启、且官方拒绝给便利**。sendme 完全未用 push（main.rs 只有 `execute_get`）。

**含义**：若产品模型是「发方 offer → 收方 accept → 发方推」，迁 iroh-blobs 会**落在它最不成熟的分支上**。要么改成 pull（发方只发 ticket，收方拉），要么绕开 iroh-blobs 走 dumbpipe 形状。

### ⚠️ `EventMask::DEFAULT` 的真相（与常见误传相反）

```rust
// iroh-blobs-0.103.0/src/provider/events.rs:189-198
// :190 doc 原文：All event notifications are fully disabled. Push requests are disabled by default.
DEFAULT = EventMask {
    connected: ConnectMode::None,
    get:       RequestMode::None,
    get_many:  RequestMode::None,
    push:      RequestMode::Disabled,      // ← 不是 None！
    throttle:  ThrottleMode::None,
    observe:   ObserveMode::None,
}
```

**唯一能写入本地 store 的请求类型（push）恰恰是默认拒绝的。别写成「其他请求类型默认档位是 None（不通知但正常处理）」——那会让人误以为节点默认接受 push 写入。**

---

# 第四部分：鉴权 —— sendme 不鉴权是产品决定，不是能力缺失

```rust
// iroh-blobs-0.103.0/src/provider/events.rs:20-31
pub enum ConnectMode {
    #[default] None,     // 不通知
    Notify,              // 通知
    Intercept,           // ← 可以拒绝入站连接
}

// events.rs:46-67
pub enum RequestMode {
    None, Notify,
    Intercept,           // ← 可拒绝单条请求，无传输事件
    NotifyLog,           // 通知 + 详细传输事件
    InterceptLog,        // ← 可拒绝 + 详细传输事件
    Disabled,            // 完全禁用，所有请求被拒
}
// events.rs:81-86
pub enum AbortReason { RateLimited, Permission }

// sendme/src/main.rs:713-718 —— 但 sendme 只用了观测档位
EventMask {
    connected: ConnectMode::Notify,
    get: provider::events::RequestMode::NotifyLog,
    ..EventMask::DEFAULT
}
```

**直接照抄 sendme 会得到一个「任何人只要知道 hash 且能连上就能拉走文件」的节点。** sendme 这么做是因为它的安全模型就是「**ticket 即凭据，谁拿到谁能下**」——不是 iroh-blobs 缺能力。

**要做白名单/接收确认，用 `ConnectMode::Intercept` + `RequestMode::InterceptLog`**（既拦截又要进度事件）。

### iroh 层面的分工

`iroh/iroh/src/lib.rs:81-94`：

> When accepting connections the peer's [`EndpointId`] is authenticated. **However it is up to the application to decide if a particular peer is allowed to connect or not.**

**E2E 加密 ≠ 授权。** QUIC-TLS 保证「对端确实是这个 EndpointId」，但「这个 EndpointId 该不该被允许」永远是应用层的事，不会被 TLS 取代。

> **libp2p 对照**：libp2p 里准入通常在网络层自己做（`handle_established_inbound_connection` 等）。iroh-blobs 把准入做成 **provider 的事件拦截钩子** —— 授权逻辑不在网络层而在 blobs 协议层，粒度是「连接」和「单条 get 请求」。**心智上更接近 HTTP middleware。**

---

# 第五部分：sendme —— 官方文件传输范式

- **成熟度**：**官方示例，同时作为可用工具发布**
- **依据（诚实版）**：
  - `README.md:11-15` 原文是**成对的两句**：*"**This is an example application** using iroh with the iroh-blobs protocol to send files and directories over the internet."* → *"It is **also** useful as a standalone tool for quick copy jobs."*
  - `also` 一词的全部作用就是「**首先是示例，其次才是工具**」。只引第二句会得出「production 参考实现」的错误结论
  - 它**不是实验品**：version 0.36.0；HEAD 2026-06-15；README 有 Crates.io/downloads/Chat/License/CI 徽章；release profile 是认真的（`Cargo.toml:60-66`：`panic="abort"` / `opt-level="s"` / `codegen-units=1` / `lto=true` / `strip=true`）；有 `tests/cli.rs` 端到端测试
  - **张力**：它依赖 `iroh-blobs = "0.103"`，而后者 README 自述非生产质量 —— **工具本身成熟，底座自述不成熟**
- **入口**：`sendme/src/main.rs`（1184 行 + tests/cli.rs 180 行，无其他 .rs 文件，**无 lib target**）

## 1184 行里，传输逻辑接近于 0

| 区段 | 行数 | 内容 |
|------|------|------|
| CLI 定义块 | 55–298 = **244** | clap 参数 / Format / RelayModeOption / AddrInfoOptions |
| 密钥与路径处理 | 299–364 = **66** | `get_or_create_secret` / `validate_path_component` / `canonicalized_path_to_string` |
| **import** | 373–476 = **104** | |
| **export** | 487–541 = **55** | |
| **send** | 648–792 = **145** | |
| **receive** | 1007–1155 = **149** | |
| 进度条工厂 | 874–958（7 个函数） | `show_download_progress` 在 960-973 是消费循环 |

**四个传输函数合计 453 行**，且其中大量是进度条与路径处理。

`grep -rniE "chunk|checkpoint|resume|range" --include="*.rs" .` 在整个 sendme 仓只命中一处，且是 `tests/cli.rs:44` 的注释 `// rand::thread_rng().gen_range(10000u16..60000)` —— **与传输无关**。

**`grep -c "impl ProtocolHandler" sendme/src/main.rs` → 0。sendme 没有实现任何 ProtocolHandler。** 真正实现的是 iroh-blobs 的 `BlobsProtocol`：

```rust
// iroh-blobs-0.103.0/src/net_protocol.rs:86-99
impl ProtocolHandler for BlobsProtocol {
    async fn accept(&self, conn: Connection) -> std::result::Result<(), AcceptError> {
        let store = self.store().clone();
        let events = self.inner.events.clone();
        crate::provider::handle_connection(conn, store, events).await;
        Ok(())
    }
    async fn shutdown(&self) { ... }
}
```

**`ProtocolHandler` 是「你要自造协议时才实现」的扩展点。用现成的 blobs 传文件，一行 `.accept(ALPN, blobs)` 就够。**

**它把三件事下沉了**：chunk/续传/校验 → iroh-blobs（23457 行）；加密 → QUIC-TLS（RFC 7250 raw public key，EndpointId 即公钥）；连接 → ticket（自带 EndpointAddr，无需任何 rendezvous）。

> ⚠️ **「1184 行实现完整文件传输」这个数字容易被误读成 iroh 很省。准确说法是：sendme 里跟传输协议有关的代码接近于 0，1184 行绝大多数是 clap 参数、indicatif 进度条、路径校验、剪贴板。真正的传输在 iroh-blobs 里 —— sendme 的对应物是 23457 行，不是 0。**
>
> 而且 sendme **没有** offer/accept 门控、没有收件箱、没有历史、没有多会话并发管理、不是 push 模型。**拿它的行数跟一个功能完整的传输层比是无效对比。差别不在总量，而在这些代码是 n0 维护还是你维护。**

依赖数：`[dependencies]` **20 项** + target-specific 的 libc/windows-sys = **22 个直接依赖**。其中真正的网络/传输相关只有 **iroh 和 iroh-blobs 两个**。（`irpc` 是显式直接依赖，因为 provider 事件流的类型泄漏在公开 API 上——`main.rs:547` 用到 `irpc::channel::mpsc::Receiver`。）

## ALPN 是复用的

```rust
// iroh-blobs-0.103.0/src/protocol.rs:406
pub const ALPN: &[u8] = b"/iroh-bytes/4";     // ← 注意是 iroh-bytes 不是 iroh-blobs（历史遗留名）；4 是版本号
```

sendme 三处引用全是复用，**没有定义任何自己的 ALPN**。**接收端 builder 用的是 `.alpns(vec![])`（`main.rs:1012`）——空的。因为 receive 只主动 connect、从不 accept，不需要声明任何 ALPN。**

## 发送端对传输进度完全无状态

```rust
// sendme/src/main.rs:746-751 —— send 端做的全部「传输」准备
let hash = temp_tag.hash();
let mut addr = router.endpoint().addr();
apply_options(&mut addr, args.ticket_type);
let ticket = BlobTicket::new(addr, hash, BlobFormat::HashSeq);

// main.rs:771-789 —— 然后就是等 ctrl-c，无任何会话状态
println!("to get this data, use");
println!("sendme receive {ticket}");
tokio::signal::ctrl_c().await?;
drop(temp_tag);
tokio::time::timeout(Duration::from_secs(2), router.shutdown()).await??;
tokio::fs::remove_dir_all(blobs_data_dir).await?;
```

**「发送端无状态」不是 iroh 的某个 feature，而是 pull 模型的直接推论**：既然是接收端说「我要哪些 range」，发送端就永远不需要记住上次发到哪。

**断点续传因此是免费的——不是 iroh 实现了续传，是 pull 模型让续传这个问题不存在。**

## 续传的全部代码就是三行

```rust
// sendme/src/main.rs:1042-1074
let local = db.remote().local(hash_and_format).await?;
if !local.is_complete() {
    let connection = endpoint.connect(addr, iroh_blobs::protocol::ALPN).await?;
    let get = db.remote().execute_get(connection, local.missing());  // ← 断点续传就这一行
    let mut stream = get.stream();
    while let Some(item) = stream.next().await { /* Progress / Done / Error */ }
}
```

**没有 checkpoint 表、没有 SQLite** —— 续传状态即 redb 里的 bitfield，是存储的副产品而非独立子系统。

### 续传靠的是一个约定，不是协议状态

- `main.rs:1026` `let dir_name = format!(".sendme-recv-{}", ticket.hash().to_hex());` —— **目录名确定性绑 hash**
- `main.rs:1142` `tokio::fs::remove_dir_all(iroh_data_dir).await?;` —— **仅在成功路径执行**
- `main.rs:1136-1140` Ctrl-C 分支 `endpoint.close().await; db2.shutdown().await?; std::process::exit(130);` —— **不删目录**

所以重跑同一 ticket 时 `FsStore::load` 复用残留 redb 库，`local()` 读出已有 bitfield 而自动续传。（发送端反而用随机后缀 `.sendme-send-{random}`，`main.rs:671-673` —— 发送端无需续传。）

**目录名用 ticket hash 而非 session id——这正是内容寻址的红利：同一份内容无论谁发、发几次，落到同一个续传目录。**

**两个 CLI 特有的粗糙**（GUI 产品必须自己处理）：
1. **续传隐含依赖「同一个 cwd」**（`current_dir().join(...)`）——换目录重跑 = 从零下载
2. `export()` 在目标已存在时直接 bail 并提示「The download will not be repeated.」（`main.rs:494-502`），即「已下完但导出失败」是一个需要用户手工清理的状态

## 关键接线（骨架）

```rust
// 发送端（main.rs:656-751 一带；add_path_with_opts 实际在 :414-418 的 import_files 里）
let mut builder = Endpoint::builder(presets::N0)
    .alpns(vec![iroh_blobs::protocol::ALPN.to_vec()])
    .secret_key(secret_key)
    .relay_mode(relay_mode.clone());
// :660-662 —— 选 Id（最短 ticket）就必须把地址发布到 pkarr
if args.ticket_type == AddrInfoOptions::Id {
    builder = builder.address_lookup(PkarrPublisher::n0_dns());
}
let endpoint = builder.bind().await?;

let store = FsStore::load(&blobs_data_dir2).await?;   // :709 —— ⚠️ 内部自建 multi_thread runtime
let blobs = BlobsProtocol::new(&store, /* events */ ...);
let router = iroh::protocol::Router::builder(endpoint)
    .accept(iroh_blobs::ALPN, blobs.clone())
    .spawn();                                          // :725
router.endpoint().online().await;                      // :733 —— 等 relay/地址就绪再打票
let ticket = BlobTicket::new(router.endpoint().addr(), hash, BlobFormat::HashSeq);  // :751

// import：TryReference 模式 = 不拷贝进库，只引用原文件（:414-418）
let import = db.add_path_with_opts(AddPathOptions {
    path, mode: ImportMode::TryReference, format: BlobFormat::Raw,
});
```

## 磁盘成本：发送端 1x，接收端 2x

> ⚠️ **`ImportMode` 的默认是 `Copy`，不是 `TryReference`**：
>
> ```rust
> // iroh-blobs-0.103.0/src/api/proto.rs:629-644
> pub enum ImportMode {
>     #[default]
>     Copy,          // :635-636 doc: This mode will copy the file into the database before hashing.
>                    //             This is the safe default because the file can not be accidentally modified
>     TryReference,  // :642-643 doc: Stores are allowed to **ignore this mode** and always copy the file
> }
> ```
>
> **sendme 是显式写 `mode: ImportMode::TryReference` 主动选入的，且它只是个 hint。** 语义（`proto.rs:637-644`）：*"will try to reference the file in place... large performance and storage benefit, but less safe since the file might be modified after import"* —— 对「用户选中文件后可能改动」的场景需评估。

`ExportMode::TryReference`（move 而非 copy）是存在的（`proto.rs:654-669`），**sendme 却选了 Copy——因为它收完就 `remove_dir_all` 整个 store，用 TryReference 会把已导出的文件也一并连坐删掉。**

**收端峰值占 2 倍磁盘**：先在 `.sendme-recv-*` store 里落一份，再 copy 到目标目录。**移动端磁盘紧张，这是实打实的成本。**

（import 阶段仍要为每个文件计算 blake3 outboard——`AddProgressItem::CopyDone` 之后是 "computing outboard"，`main.rs:435-441`。大文件这一步不是零成本，「秒发」的前提是文件已经在 store 里。）

## Collection / TempTag：GC 保护的所有权舞蹈

```rust
// main.rs:366-372 —— import() 的文档注释
/// The returned tag always refers to a collection. If the input is a file, this
/// is a collection with a single blob, named like the file.

// main.rs:384-400 —— WalkDir 扁平化 + 显式跳过 symlink
let files = WalkDir::new(path.clone()).into_iter();

// main.rs:466-473 —— 组装 collection，GC 保护责任转移
let (collection, tags) = names_and_tags.into_iter()
    .map(|(name, tag, _)| ((name, tag.hash()), tag))
    .unzip::<_, _, Collection, Vec<_>>();
let temp_tag = collection.clone().store(db).await?;
// now that the collection is stored, we can drop the tags
// data is protected by the collection
drop(tags);

// main.rs:751 —— 单文件也是 HashSeq
let ticket = BlobTicket::new(addr, hash, BlobFormat::HashSeq);
```

**TempTag / tags 是 GC 保护机制，不是业务标签。** import 期间每个文件的 TempTag 防止数据被 GC，collection 存好后这些 tag 才能 drop（保护责任转移给了 collection）。`send()` 末尾 `drop(temp_tag)`（`main.rs:779`）就是显式释放对整个 collection 的保护。

**这个所有权舞蹈是 iroh-blobs 的独特心智，不理解会写出数据被 GC 掉的 bug。**

并发 `main.rs:455` `.buffered_unordered(parallelism)`，parallelism = `num_cpus::get()`。Collection 定义在 `iroh-blobs/src/format/collection.rs:22-25` `pub struct Collection { blobs: Vec<(String, Hash)> }`。

## ticket：EndpointAddr + BlobFormat + Hash

```rust
// iroh-blobs-0.103.0/src/ticket.rs:16-23
pub struct BlobTicket { addr: EndpointAddr, format: BlobFormat, hash: Hash }

// sendme/src/main.rs:255-267 —— ticket 尺寸 vs 连通率的权衡
pub enum AddrInfoOptions {
    /// Only the Endpoint ID is added.
    #[default]                    // ← 枚举的 Default 是 Id
    Id,
    RelayAndAddresses,
    Relay,
    Addresses,
}
// main.rs:221 —— 但 clap 的默认才是实际生效的
#[clap(long, default_value_t = AddrInfoOptions::RelayAndAddresses)]
```

⚠️ **两个 default 并存**：枚举 `Default` 是 `Id`（`:259-260`），clap CLI default 是 `RelayAndAddresses`（`:221`）。**后者才是实际生效的** —— 说明 n0 自己也认为「长一点但连得上」优先。

> 注意 `AddrInfoOptions` **是 sendme 本地定义的，不是 iroh 或 iroh-tickets 的公开 API**，想要就自己抄。

**短 ticket 的代价钉死在两处**：
- 发送端 `main.rs:660-662`：选 `Id`（最短）就**必须**发布 pkarr
- 接收端 `main.rs:1016-1018`：ticket 里没地址就**必须**开 DNS 查询

```rust
if ticket.addr().relay_urls().next().is_none() && ticket.addr().ip_addrs().next().is_none() {
    builder = builder.address_lookup(DnsAddressLookup::n0_dns());
}
```

**即：想要「短码」就必须依赖 n0 的 DNS/pkarr 基础设施做地址发现。iroh 没有免费午餐。** sendme 自己的注释（`main.rs:216-217`）：*"Generally, the more information the higher the likelihood of a successful connection, but also the bigger a ticket to connect."*

## 下载前先拿一次 sizes

```rust
// sendme/src/main.rs:1049-1057
let (_hash_seq, sizes) = get_hash_seq_and_sizes(&connection, &hash_and_format.hash, 1024 * 1024 * 32, None)
    .await.map_err(show_get_error)?;
let total_size   = sizes.iter().copied().sum::<u64>();
let payload_size = sizes.iter().skip(2).copied().sum::<u64>();
let total_files  = (sizes.len().saturating_sub(1)) as u64;
```

**索引有讲究**：`total_size` 是全部（含 hashseq 自身与 collection 元数据 blob）；`payload_size` 要 `.skip(2)`（跳过 hashseq blob 和 metadata blob）才是真实文件负载；`total_files` 是 `len()-1`。这几个 magic 偏移量来自 Collection 的编码布局，照抄时容易搞错。

`1024*1024*32` 是这次请求的**大小上限（32MiB），不是 chunk size**（用途见 `iroh-blobs/src/get/request.rs:241-247`：*"check the size to avoid parsing a maliciously large hash seq"*）。

## online() 超时行为 —— 两家不一样，别搞混

| | 超时 | 超时后 |
|---|---|---|
| **dumbpipe** | 5s（`main.rs:28 ONLINE_TIMEOUT`） | **仅告警继续** |
| **sendme** | 30s（`main.rs:731-736`） | **硬失败** —— `.await?` 把 Elapsed 传进 anyhow，整个 send 失败 |

**sendme 已经做了「超时就拒绝生成 invite」这个选择** —— 不需要重新发明。

## 何时不该照抄 sendme 的产品模型

它是 **pull**（收方主动拉）+ 自包含 ticket + 前台阻塞进程（Ctrl-C 即停止服务、删临时目录）。且它的安全模型就是「**ticket 即凭证**」—— 任何人拿到 ticket 即可下载，无 TTL、无配对。

**若你的产品语义是「配对设备之间的持久通道」或「短码 rendezvous」，sendme 的 ticket 模型不能直接照搬。** 详见 [tickets.md](tickets.md)。

---

# 第六部分：dumbpipe —— 最小可用 P2P 的下限

- **成熟度**：**production**（version 0.39.0；HEAD 2026-06-24 `ci: add semver check (#102)`；`tests/cli.rs` 494 行）
- **入口**：`dumbpipe/src/main.rs`（894 行）、`dumbpipe/src/lib.rs`（**13 行**）

## lib.rs 一共 13 行，3 行有效代码

```rust
// dumbpipe/src/lib.rs —— 全文
/// The ALPN for dumbpipe.
///
/// It is basically just passing data through 1:1, except that the connecting
/// side will send a fixed size handshake to make sure the stream is created.
pub const ALPN: &[u8] = b"DUMBPIPEV0";

/// The handshake to send when connecting.
///
/// The side that calls open_bi() first must send this handshake, the side that
/// calls accept_bi() must consume it.
pub const HANDSHAKE: [u8; 5] = *b"hello";

pub use iroh_tickets::endpoint::EndpointTicket;
```

**握手存在的唯一理由写在注释里**：QUIC 的 `open_bi()` 在真正写入字节前不会在对端触发 `accept_bi()`，所以连接方必须先写点什么。**这是个纯粹的 QUIC 语义补丁，不是身份/版本协商。**

894 行的 main.rs 几乎全是 stdio/TCP/Unix socket 转发与 clap。**整个「协议」= 一个 ALPN 字符串 + 一次 open_bi/accept_bi。**

> libp2p 里「定义一个协议」要写 NetworkBehaviour / ConnectionHandler / UpgradeInfo；iroh 里最小协议 = **一个 ALPN 字节串 + 一个 impl ProtocolHandler**（dumbpipe 连 ProtocolHandler 都没用，直接裸 accept 循环）。

**依赖面**：`[dependencies]` 实际有 **11 项**：clap、hex、iroh 1.0.0、iroh-tickets 1.0.0、noq 1.0.0、tokio(full)、tokio-util、tracing、tracing-subscriber、data-encoding、n0-error。准确说法是「**网络**依赖面极窄」—— 网络相关只有 iroh + iroh-tickets，其余是 CLI/日志/编码。

## 四步骨架（可直接复制）

```rust
// 监听端（main.rs:308-317 create_endpoint + :380-400）
let endpoint = Endpoint::builder(presets::N0).secret_key(sk).alpns(alpns).bind().await?;
timeout(ONLINE_TIMEOUT, endpoint.online()).await;   // 等 home relay，5s
let ticket = EndpointTicket::new(endpoint.addr());
loop {
    let connecting = endpoint.accept().await.unwrap();
    let connection = connecting.await?;
    let (s, mut r) = connection.accept_bi().await?;   // 拿到一对 QUIC 流
}

// 连接端（main.rs:308-317 + :430-436）—— 注意空 alpns
let endpoint = Endpoint::builder(presets::N0).secret_key(sk).alpns(vec![]).bind().await?;
let connection = endpoint.connect(args.ticket.endpoint_addr(), ALPN).await?;
let (mut s, r) = connection.open_bi().await?;
if !args.common.is_custom_alpn() {
    s.write_all(&dumbpipe::HANDSHAKE).await?;         // ← 破懒创建死锁
}
```

**`.alpns()` 是「我接受哪些入站协议」，只连出去就传 `vec![]`；出站协议在 `connect(addr, alpn)` 时逐次指定。**

**dumbpipe 没用 Router**——单协议场景直接裸 accept 循环即可，Router 是多 ALPN 分派才需要的。

## 正确样板 vs 反面写法（同一个仓里两种写法并存）

**正确样板**——一条 Connection clone 进每个 task，每个本地 socket 开一条新流，无流池/限流器（`main.rs:834-856` accept 循环 + `:797-831` handler）：

```rust
loop {
    let next = tokio::select! { stream = unix_listener.accept() => stream, _ = ctrl_c() => break };
    let connection = connection.clone();          // :844 复用同一条 iroh Connection
    tokio::spawn(async move {                     // :846 每个本地连接一个 task
        handle_unix_accept(next, connection, handshake).await
    });
}
// handle_unix_accept @ :797-831
let (mut endpoint_send, endpoint_recv) = connection.open_bi().await?;   // :808-811
```

**反面写法**——`handle_tcp_accept`（`main.rs:483-514`）每个 TCP 连接都重新 `endpoint.connect(addr, alpn)`（:494-497）建**新连接**再 `open_bi()`（:498-501）。**那是打洞成本很高的写法。**

（已核实 dumbpipe src/ 下确无 semaphore / limiter / pool，grep 零命中。）

## 短票技巧

```rust
// main.rs:632-639
fn create_short_ticket(addr: &EndpointAddr) -> EndpointTicket {
    let mut short = EndpointAddr::new(addr.id);
    for relay_url in addr.relay_urls() { short = short.with_relay_url(relay_url.clone()); }
    short.into()
}
```

只保留 id + relay url，丢弃直连地址。⚠️ **`online()` 超时只 warn 不 fail——拿到的 ticket 可能缺 relay url，对端只能靠直连碰运气。**

## 六个成对子命令 + 一个反直觉命名

```rust
pub enum Commands {
    GenerateTicket,
    Listen(ListenArgs), ListenTcp(ListenTcpArgs),
    Connect(ConnectArgs), ConnectTcp(ConnectTcpArgs),
    #[cfg(unix)] ListenUnix(ListenUnixArgs),
    #[cfg(unix)] ConnectUnix(ConnectUnixArgs),
}
```

⚠️ **listen/connect 的语义是相对 iroh endpoint 而言、不是相对 TCP**——源码专门为此写了免责注释：

```
/// Listen on an endpoint and forward incoming connections to the specified host and port. ...
/// As far as the endpoint is concerned, this is listening. But it is
/// connecting to a TCP socket for which you have to specify the host and port.
ListenTcp(ListenTcpArgs),
```

另外 `listen`(stdio) **只服务第一个连接就 break**（"stop accepting connections after the first successful one"），`listen-tcp`/`listen-unix` 才是 per-connection spawn。

> ⚠️ README 说的是「inspired by the unix tool **netcat**」，Cargo.toml description 是「A cli tool to pipe data over the network, with NAT hole punching」——**没有 "Unix pipes between devices" 这句话，别编造引文**。

## custom-alpn：退化成裸 QUIC 流的逃生舱

```rust
/// A custom ALPN to use for the endpoint.
/// This is an expert feature that allows dumbpipe to be used to interact
/// with existing iroh protocols.
/// When using this option, the connect side must also specify the same ALPN.
/// The listen side will not expect a handshake, and the connect side will not send one.
/// Alpns are byte strings. To specify an utf8 string, prefix it with `utf8:`.
#[clap(long)]
pub custom_alpn: Option<String>,
```

```bash
echo request1.bin | dumbpipe connect <ticket> --custom-alpn utf8:/iroh-bytes/2 > response1.bin
```

**custom-alpn 下 dumbpipe 不再是对称管道，而是「任意 iroh 协议的裸客户端」**——握手、分帧、语义全部由用户负责。**对调试自研 ALPN 协议非常有用（可以用 dumbpipe 当 netcat 打自己的协议端口）。**

## 一个可直接复用的适配器

```rust
// iroh-examples/dumbpipe-web/src/quinn_endpoint.rs
//! QuinnEndpoint is a wrapper around a quinn::SendStream and quinn::RecvStream
//! It implements AsyncRead and AsyncWrite so it can be used with tokio::io::copy
use iroh::endpoint::{RecvStream, SendStream};

pub struct QuinnEndpoint { pub send: SendStream, pub recv: RecvStream }

impl AsyncRead for QuinnEndpoint {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().recv).poll_read(cx, buf)
    }
}
```

**iroh 的 SendStream/RecvStream 各自已实现 AsyncWrite/AsyncRead，需要的只是把「一对流」合成「一个双向 IO」。** 注意 iroh 1.0 的这些类型实际来自 **noq**（n0 的 QUIC 实现）。

（⚠️ `dumbpipe-web` 本身**不是浏览器例子** —— 它的 package name 是 `reverse-proxy`，是个原生 tokio + hyper 反向代理。见 [ecosystem-map.md](ecosystem-map.md) 的导航陷阱。）

---

## 附：bao-docs 是幻灯片，不是文档

- **成熟度**：**abandoned**
- 仅 4 个文件（README.md 933B / INDEX.md 958B / bao.html 4.7MB / bao-thing.html 4.8MB），无 src、无 CI；HEAD `e1cb932` **2023-04-20**；README 里 bao-thing 的 video 链接至今是 `[video](http://tbd)`；INDEX.md 推荐的 abao crate 早被 bao-tree 取代
- 内容确实有效但密度极低（53 个 `<section>` 里约 20 张有文字，其余是图片 data: URI）。有价值的 slide：17-31（Persist branch hashes / Verified streaming / Inline vs Outboard / Slice encoding）、34（Chunk groups）、38（Size proofs）、43（Don't flip to pre-order）

**学原理请直接读 `bao-tree/src/lib.rs:1-204`** —— 它比 bao-docs 新 2 年半且更深。bao-docs 仅在需要给团队做原理分享时当 slide 素材。
