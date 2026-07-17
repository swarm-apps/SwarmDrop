# Blobs：bao-tree / iroh-blobs / sendme / dumbpipe

iroh 1.0.2 / iroh-blobs 0.103.0 / bao-tree 0.16.0 / sendme 0.36 / dumbpipe 0.39 · 调研日期 2026-07-17

> **版本警告先行**：`sendme/Cargo.lock` 锁的是 **iroh 1.0.0**（不是本地 iroh-study 那份 1.0.2），
> `iroh-blobs 0.103.0`。**iroh-blobs 源码不在 iroh-study 里**，在
> `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/iroh-blobs-0.103.0/`
> （发布包里 cargo 规范化过 Cargo.toml，原件在 `Cargo.toml.orig`）。
>
> **「该不该引入 iroh-blobs / bao-tree」的选型结论与成熟度证据链** → [index-ecosystem-map.md](index-ecosystem-map.md)。
> 本文只讲**机制与用法**。撞到 panic → [index-gotchas.md](index-gotchas.md)。

---

# 1. bao-tree —— 只要验签，不要 blob store

## 它做什么

把「整文件一个 hash」拆成一棵**可对任意 range 独立验签**的树。核心是 outboard（树的分支哈希，与原文件分离存放，**原文件一字节不改**）+ 运行时可配的 chunk group size + 多 range 集合查询（一次问 `[0..1000, 5000..6000]`）。

**不含任何网络/存储/store 概念**，只有 encode/decode/outboard 三件事。

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

---

# 2. iroh-blobs

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

> **零加密原语 —— 与应用层 E2E 加密正面冲突**（明文落盘、hash 即凭证、(A) 先加密再入库去重归零
> vs (B) 靠 QUIC TLS 二选一）→ [08-deployment.md](08-deployment.md) 的 Security & Privacy 一节。
>
> **浏览器只有 MemStore、零持久化** → [06-wasm-browser.md](06-wasm-browser.md)。
>
> **两条已验证的 panic 路径**（#233 空闲驱逐 Poisoned / `FsStore::load` 自建 multi_thread runtime +
> `block_in_place` 在 current_thread 上炸）→ [index-gotchas.md](index-gotchas.md) 的 I11 / I12。

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

## 心智税：tag / GC / TempTag —— 保活的所有权舞蹈

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

### Collection / TempTag：GC 保护的所有权舞蹈

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

## 「增量同步」有歧义，写文档要写清

**iroh-blobs 的「增量」= 同一 hash 未下完的部分补齐（bitfield 集合差）。**

**不是** 文件 v1→v2 之间只传改动量。`grep delta|rsync|版本间 diff` **未找到任何 rsync 式跨版本 delta 同步 API**——内容寻址下**文件一改 hash 全变**，每次都是全新 hash、全量重传。

（**白送但 sendme 没吃的能力**：`api::downloader` 的多源并行下载 `SplitStrategy::Split`，`src/api/downloader.rs:317-320`。对「一份文件发给多台设备」的群发场景有实质价值；在 libp2p 下要自造相当于重写一个 BitTorrent 调度器。）

---

# 3. pull vs push —— 关键的模型冲突

**iroh-blobs 是 pull 模型**（收方拉）。push API 存在但：

1. `remote.rs:573` `pub fn execute_push(&self, conn: Connection, request: PushRequest) -> PushProgress` 是公开的
2. 但其内层 `execute_push_sink`（`pub(crate)`）文档（remote.rs:588-590）原文：*"Note that many nodes will reject push requests. Also, this is an experimental feature for now."*
3. `PushRequest(GetRequest)`（`protocol.rs:567`）—— 推方自己决定推哪些 range，**不存在** pull 侧 `local.missing()` 那样的自动协商
4. **最硬的一条**：`EventMask::DEFAULT`（`provider/events.rs:189-198`）里 `push: RequestMode::Disabled` —— **push 默认即禁用**
   - `events.rs:165-166` 注释：*"push requests are disabted by default, as they can write to the local store"*（原文拼写如此）
   - `events.rs:200-204`：*"If you want to enable push requests, **which can write to the local store**, you need to do it manually. **Providing constants that have push enabled would risk misuse.**"* —— **官方明确拒绝提供开启 push 的便捷常量**

**这就是「many nodes will reject push requests」的机制成因**：不是「作者说是实验性」，而是**协议默认关闭、需手动开启、且官方拒绝给便利**。sendme 完全未用 push（main.rs 只有 `execute_get`）。

**含义**：若产品模型是「发方 offer → 收方 accept → 发方推」，迁 iroh-blobs 会**落在它最不成熟的分支上**。要么改成 pull（发方只发 ticket，收方拉），要么绕开 iroh-blobs 走 dumbpipe 形状。

> `EventMask::DEFAULT` 的完整档位表、以及 `push: RequestMode::Disabled` 不是 `None` 的举证
> → [02-connecting.md](02-connecting.md) 的 Endpoint Hooks 一节。
> **鉴权四档（sendme 不鉴权是产品决定，不是能力缺失）** 同处。

---

# 4. sendme —— 官方文件传输范式

> **成熟度**：**官方示例，同时作为可用工具发布**（README 两句成对，`also` 一词的全部作用就是
> 「首先是示例，其次才是工具」）。判定依据 → [index-ecosystem-map.md](index-ecosystem-map.md)。
>
> **入口**：`sendme/src/main.rs`（1184 行 + tests/cli.rs 180 行，无其他 .rs 文件，**无 lib target**）。
> **1184 行里传输逻辑接近于 0** 的分区行数表同样在 ecosystem-map。

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

> **`online()` 超时行为两家不一样**（dumbpipe 5s 仅告警 vs sendme 30s 硬失败）
> → [02-connecting.md](02-connecting.md)。
> **ticket 里放多少地址（`AddrInfoOptions` 四档）** → [01-concepts.md](01-concepts.md) 的 Tickets 一节。

---

# 5. dumbpipe —— 最小可用 P2P 的下限

**协议定义只需 13 行**（一个 ALPN 常量 + 5 字节 handshake + 一个 re-export）
→ [03b-writing-a-protocol.md](03b-writing-a-protocol.md) 的「最小协议的下限」。
**四步骨架、正确样板 vs 反面写法、`QuinnEndpoint` 适配器** → [03a-using-quic.md](03a-using-quic.md)。

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

> **六个成对子命令与 `listen`/`connect` 的反直觉命名** → [index-ecosystem-map.md](index-ecosystem-map.md)。
