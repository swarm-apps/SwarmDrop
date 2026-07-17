# Documents / RPC / Automerge

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Protocols](https://docs.iroh.computer/protocols/) 分区下的 Documents / RPC / Automerge。

> **这三个「解决什么、不解决什么」的横向对比与成熟度** → [index-ecosystem-map.md](index-ecosystem-map.md)。
> 本文只讲域内实质。**Gossip Broadcast 官方归在 Connecting** → [02-connecting.md](02-connecting.md)。

---

# 1. iroh-docs

### 数据模型

Replica（= Document，由 NamespaceId 标识）内含无限条 Entry。Entry 的主键是**三元组** `(namespace, author, key)`（`src/sync.rs:1002-1004` 的字节布局印证），值是 `Record{hash, len, timestamp}`（:1125-1134）—— **内容本身不在 replica 里**，只存 BLAKE3 hash，真实字节由 iroh-blobs 搬运。

同步算法是 **range-based set reconciliation**（递归分区 + 比对指纹，`src/ranger.rs`，1652 行），配合 gossip 做变更实时通知（`src/engine/live.rs:41-45` 的 `enum Op { Put(SignedEntry), ContentReady(Hash), .. }`，:39 注释 *"This is the message that is broadcast over iroh-gossip"*）。

权限是双密钥：Namespace 密钥 = 写能力，Author 密钥 = 作者身份证明。

**强项**：range-based set reconciliation —— 两端只交换分区指纹就能定位差集，同步 10 万条 entry 的增量代价远低于全量对比。

### 四条代价

#### 1. 它是 meta-protocol，无法单独使用

`src/protocol.rs:101-106` 的 `Builder::spawn` **强制三个参数**：

```rust
pub async fn spawn(self, endpoint: Endpoint, blobs: BlobsStore, gossip: Gossip) -> anyhow::Result<Docs>
```

README 明说：*"Docs is a 'meta protocol' that relies on the iroh-blobs and iroh-gossip protocols. Setting up Docs will require setting up Blobs and Gossip as well."*

Cargo.toml 中三者均为**必选**（非 optional）：
```toml
iroh-blobs  = { version = "0.103",   default-features = false }
iroh-gossip = { version = "0.101.0", features = ["net"], default-features = false }
```

Router 须注册**三个 ALPN**（`iroh-examples/tauri-todos/src-tauri/src/iroh.rs:45-48` 实测印证）。

**评估时必须按「docs + blobs + gossip 三件套」算总账**（iroh-docs 14552 行 + iroh-gossip 8043 行 + blobs），不能只看 docs。

#### 2. 读权限就是 NamespaceId 本身 —— 与 E2E 模型结构性冲突

`src/sync.rs:184-191`：

```rust
/// The capability of the namespace.
pub enum Capability { Write(NamespaceSecret), Read(NamespaceId) }
```

读能力载荷就是 NamespaceId，而 NamespaceId 同时是 replica 的**公开唯一标识符**（README：*"The public key is the NamespaceId, which also serves as the unique identifier for a replica"*）。

**即「知道 doc 的 ID」= 「有读权限」，二者不可分离。** README 全文未提内容加密（签名只用于写能力与作者证明）。

**冲突点**：若应用层自行加密 key，则 `src/store.rs:183-193` 与 :309-314 提供的 `key_exact` / `key_prefix` / `single_latest_per_key` 查询将失效（前缀在密文上无意义），而 range-based set reconciliation 正是建立在 key 的 `Ord` 序上（`src/ranger.rs:12-20` 的 `RangeEntry::Key` 要求 `impl Ord` 以 *"define the range ordering used in the set reconciliation algorithm"*）—— **加密 key 会同时破坏查询与同步效率**。

**即：为了 E2E 把这个库最值钱的两个能力都关掉。这是不该用它的最硬理由。**

#### 3. 引入 redb —— 可能是你的第二个嵌入式数据库

`iroh-docs/Cargo.toml`：`redb = { version = "4.1" }` 为**必选**（:47），另有 `redb_v3 = { package = "redb", version = "3.1", optional = true }`（:48）供 `redb-v2-migration` feature 用，**且该 feature 在 default 里**（:76 `default = ["metrics", "rpc", "fs-store", "redb-v2-migration"]`）。

**即默认 feature 会同时编入 redb 4.1 和 redb 3.1 两个版本。**

README：*"The latter makes use of redb, an embedded key-value store, and persists the whole store with all replicas to a single file."*


> **浏览器下丧失持久化**（`ci.yaml:310` 的 `--no-default-features` 关掉 fs-store）
> → [06-wasm-browser.md](06-wasm-browser.md)。


### 冲突消解语义

`Record` 的 `impl Ord`（`src/sync.rs:1140-1147`）是 timestamp 优先、再比 hash。

> ⚠️ **「last-write-wins」是本 skill 的推断标签，不是上游术语** —— 对 `iroh-docs/src/` 与 README.md grep `last-write-wins|last write wins|LWW` **零命中**。上述 Ord 是 value ordering 的定义；未找到它被用作冲突消解的直接证据。相比之下 automerge 的 CRDT 合并语义更适合真协作编辑。

### 何时用 iroh-docs

需要「一组设备/用户对同一份可变 KV 数据集最终一致，且带多作者署名与写权限控制」时。iroh 官方定位（`iroh/README.md:50`）：*"an eventually-consistent key-value store of iroh-blobs blobs"*。典型：共享文件夹、协作笔记索引、多端设置同步。

---

# 2. irpc（现役）与 quic-rpc（前身）

**它是什么**：核心不是「网络 RPC」而是「**带类型的 actor 边界**」。channel 声明在消息**外面**（`WithChannels { inner, tx, rx }`），用 `#[rpc_requests]` 宏把 enum 变成 4 种 gRPC 式交互（1→1 / 1→N / N→1 / N→N）。同一份 `Client<S>` 既能 `Client::local(tx)` 走进程内 mpsc，又能 `Client::boxed(conn)` 走网络，**调用方代码不变**。

（注：`irpc/src/lib.rs:573` 那句 *"In the local case, this is just a clone which has almost zero overhead."* 文档化的是 **`request()`**，不是 `Client::local()`（:556）—— 语义上不冤，但引用时别搞错。）

**关掉 rpc feature 后依赖只剩 serde + tokio + tokio-util**（`lib.rs:62-64`）—— 纯本地 actor 用（`examples/local.rs` 的 `no_rpc` 模式：`#[rpc_requests(message = StorageMessage, no_rpc, no_spans)]`，:29）。生态验证此为常态：iroh-blobs/iroh-gossip/iroh-docs 三者的 irpc 依赖**全部 `default-features = false`** 且把 `irpc/rpc` 关在可选 feature 后面。

#### 四条 Non-goals（`lib.rs:14-19`，逐字）

```
- Cross language interop. This is for talking from rust to rust
- Any kind of versioning. You have to do this yourself
- Making remote message passing look like local async function calls
- Being runtime agnostic. This is for tokio
```

**第一条最重要**：Tauri IPC 是 Rust↔JS、uniffi 是 Rust↔TS，两者都是跨语言 FFI 边界，**irpc 按设计就接不了**。所以「加 irpc 是否与它们冗余」问错了 —— 它不与那两者重叠，而是**第三条 Rust↔Rust 边界**。判断标准应是「有没有 Rust↔Rust 的进程/网络边界需要收敛」。

#### 16 MiB 是硬上限，不可配置

`irpc/src/rpc.rs:29-33`：

```rust
/// Default max message size (16 MiB).
pub const MAX_MESSAGE_SIZE: u64 = 1024 * 1024 * 16;
/// Error code on streams if the max message size was exceeded.
pub const ERROR_CODE_MAX_MESSAGE_SIZE_EXCEEDED: u32 = 1;
```

:234 和 :240 在写入前用 `postcard::experimental::serialized_size` 校验并 `return Err(e!(WriteError::MaxMessageSizeExceeded))`（错误变体在 :48-50）。序列化固定为 postcard + varint 长度前缀（`lib.rs:47-52`，oneshot 也不例外）。

> ⚠️ 源码 doc 写的是 "**Default** max message size"，字面上暗示可配置 —— 但全仓 grep `max_message_size` / 任何 setter / builder 字段 **零命中**，该 const 在 8 处被直接引用，**无任何覆盖机制**。所以「硬上限」比源码自己的措辞更准确。

**含义**：若把 irpc 套上 data plane，文件必须切块走 mpsc 流式 channel，单块不得超 16 MiB —— 等于 chunk 逻辑一点没省，还多一层框架。

#### irpc 在 iroh-blobs 里的真实位置

**准确表述**（这一点很容易被说过头）：
- ✅ **irpc 从不承载 bulk 字节** —— 这是 n0 的分界线
- ✅ **ProtocolHandler 实现文件本身无 irpc** —— `iroh-blobs/src/net_protocol.rs`（:86 `impl ProtocolHandler for BlobsProtocol`，blobs 唯一的 p2p 协议入口）grep `irpc` 零命中
- ❌ **但不能说「p2p 传输线一行 irpc 都没有」** —— BlobsProtocol 的服务路径**确实用 irpc**：net_protocol.rs:49 `use crate::{api::Store, provider::events::EventSender};`、:54 `events: EventSender` 字段、:89-90 `crate::provider::handle_connection(conn, store, events).await;`；而 `EventSender` 内部就是 irpc client（`provider/events.rs:230` `inner: Option<irpc::Client<ProviderProto>>`、:248 `throttle` 甚至能实际节流传输）。真正搬字节的 `provider.rs` 在服务路径上调用它（:250-251、:345、:361）
- ❌ **也不能说「只用在 store API 边界」** —— `grep -rln irpc src/` 命中 **19 个文件**，含 `src/provider/events.rs`（p2p 服务事件）与 `src/get/error.rs:37`（客户端 get 路径）

**正确的心智**：**bulk data plane 手写 ProtocolHandler，control / progress / API 边界才上 irpc。** iroh-blobs 与 sendme 其实是**同一个模式**（irpc 当类型化 progress channel），不是对立的。

sendme 的用法印证：`sendme/Cargo.toml:42` `irpc = "0.17.0"`（**没有** irpc-iroh），全仓 src/ 中 irpc 仅两处命中 —— `main.rs:547` `mut rx: irpc::channel::mpsc::Receiver<RequestUpdate>`（进度事件流参数）和 :994 一条错误信息字符串。**一个 iroh 上的文件发送工具，完整功能不需要 irpc 做 RPC。**

#### iroh 核心完全不依赖 irpc

iroh 仓内全部 7 个 Cargo.toml grep `irpc` **零命中**。ProtocolHandler + cheap streams 是 iroh 的一等公民，irpc 是可选加层 —— **迁移路径上没有「必须先学 irpc」这一步**。

另注：iroh-blobs 的 `rpc` feature 走的是 **noq**（socket 地址）而非 iroh（EndpointId）—— `Cargo.toml:93` `rpc = ["dep:noq", "irpc/rpc", "irpc/noq_endpoint_setup"]`（**没有 irpc-iroh**）；`api.rs:250-256` `connect(endpoint: noq::Endpoint, addr: SocketAddr)`、:258-268 `listen(self, endpoint: noq::Endpoint)`。**它是给「CLI/app ↔ 本机 blobs daemon」用的进程边界 RPC，不是 p2p。**

**未来变动风险**：`irpc-iroh/src/lib.rs:44-45` 留有 TODO：*"remove this and provide a From instance as soon as iroh is 1.0 and we can move irpc-iroh into irpc?"*（紧跟 :47 `pub struct IrohRemoteConnection(Connection);`）。iroh 现已 1.0.2、irpc 已更新到 iroh 1.0 —— **前置条件已满足但尚未执行**，即 irpc-iroh 未来可能被并入 irpc 主 crate。

**反直觉点**：irpc **主 crate 的原生 transport 是 noq（socket 地址）而非 iroh**（`irpc/src/lib.rs:36-40` 的 `## Transports` 段：*"These must always be noq streams ... This restricts the possible rpc transports to noq (QUIC with dial by socket address) and iroh (QUIC with dial by endpoint id)."*）—— p2p 需额外引 irpc-iroh。

> **quic-rpc 已 abandoned**（绑死 iroh 0.35、外部依赖数 0、14 个月空窗）—— 判定证据链与
> 「仍有价值的设计读物」→ [index-ecosystem-map.md](index-ecosystem-map.md)。

---

# 3. CRDT / automerge —— 只有示例，没有库

### iroh-automerge

- **成熟度**：**experimental**（示例）
- **依据**：所在仓 README:9 定位就是示例；`Cargo.toml` version = "0.1.0"，**无 description/license/repository 等发布元数据，未发布到 crates.io**；仓库根目录**没有** workspace Cargo.toml；全部实现仅 **227 行**（protocol.rs 153 + main.rs 74）。仓库 HEAD 2026-06-15 `deps: update to iroh 1.0 (#164)` —— 依赖已跟到 iroh 1.0.0 + edition 2024，即**示例本身没烂尾**，但「跟版及时的示例」≠「可依赖的库」
- **入口**：`iroh-examples/iroh-automerge/src/protocol.rs`（153 行；ALPN 在 **:21**；send_msg/recv_msg 的长度前缀帧化在 :41-60）
- **它演示什么**：把 automerge 官方 sync 协议接到 iroh 的 `ProtocolHandler` 上：自定义 ALPN `iroh/automerge/2`，在一条 bi stream 上用 **8 字节小端长度前缀**来回帧化 automerge sync message，直到收敛。**CRDT 能力 100% 来自上游 automerge 0.7 crate**，iroh 侧只贡献「传输接线」
- **何时读**：要把任意一个自带 sync 协议的第三方库（automerge、CRDT、乃至你自己的协议）接到 iroh 上时，这 153 行是最小、最干净的 **ProtocolHandler + bi-stream 帧化范本**

### iroh-automerge-repo

- **成熟度**：**experimental**（示例）
- **依据**：同属 iroh-examples；version 0.1.0，无发布元数据，未上 crates.io；实现共 410 行（lib.rs 91 + main.rs 256 + codec.rs 63），其中 codec.rs 的存在本身就是示例性质的佐证（README 称其为 *"a very small utility to adapt tokio-util's LengthDelimitedCodec to work with Vec<u8> instead of Bytes"*）；关键第三方依赖 `samod = "0.5.0"` 自身也是 0.x；**README 明示 demo 仅内存态**（关掉终端文档就没了），持久化需自行接 samod 的 TokioFilesystemStorage
- **入口**：`iroh-examples/iroh-automerge-repo/src/lib.rs`（91 行）
- **比 iroh-automerge 高一层**：用 samod（automerge-repo 协议的 Rust 实现）而非裸 automerge，支持多文档（DocumentId）、连接管理与持续同步。ALPN `iroh/automerge-repo/1`

```rust
pub struct IrohRepo {
    endpoint: iroh::Endpoint,
    #[debug(skip)]           // ← 注意这个属性
    repo: Repo,              // samod::Repo
}
impl IrohRepo {
    pub const SYNC_ALPN: &[u8] = b"iroh/automerge-repo/1";   // :21
    pub async fn sync_with(&self, addr: impl Into<iroh::EndpointAddr>) -> anyhow::Result<ConnFinishedReason> { .. }
}
// impl iroh::protocol::ProtocolHandler for IrohRepo 在 :65
```

**要真用 automerge**：直接依赖上游 automerge 0.7 或 samod，自己写传输层（照着这个文件写）。

> **两仓定位差（automerge 在 iroh-examples，不在 iroh-experiments）** → [index-ecosystem-map.md](index-ecosystem-map.md)。
