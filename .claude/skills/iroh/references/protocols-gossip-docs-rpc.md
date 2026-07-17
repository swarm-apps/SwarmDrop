# 高层协议 —— Gossip / Docs / RPC

**iroh 1.0.2 · 调研 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`**

> API 用法 → `/iroh` skill。这里讲**这三个解决的是什么问题、不是什么问题**。

## 一句话选型

| 你的问题 | gossip | docs | irpc |
|---|---|---|---|
| **它解决什么** | 大规模、成员未知的 swarm 里做 pubsub | 多设备对同一份可变 KV 数据集最终一致 | Rust↔Rust 的 typed actor / RPC 边界 |
| **它不解决什么** | 「设备 X 是否在线」的定向存活探测 | 点对点一次性传输 | 跨语言（Tauri IPC / uniffi 都接不了） |
| **典型误用** | 拿 NeighborUp/Down 当 presence | 只想要 blobs 却被拖进全栈 | 当成 bulk data plane |

## iroh-gossip

- **成熟度**：**production（推断）**
- **依据**：
  - version 0.101.0；CHANGELOG.md:5 显示 v0.101.0 于 2026-06-15 发布；发版节奏连续规律，可回溯至 2024-11-04 的 0.28.1（CHANGELOG.md:278），近一年约每月一版
  - README 全文无任何免责/实验性声明（grep `deprecat|experimental|not production` 零命中）
  - 工程成熟度信号充分：专门的仿真器 `src/bin/sim.rs` + simulation.yaml workflow；release.yaml 自动发版；`ci.yaml:150` 有 wasm32 构建门禁且 `ci.yaml:175` 断言产物无 `import "env"`（即真正浏览器可用）
  - ⚠️ **这是本 skill 的推断，不是 n0 的承诺** —— 全生态无任何上游文档自称 production-ready
  - ⚠️ **反向证据**：版本仍是 0.101.0 而 iroh 已 1.0.2 —— **无 1.0 API 稳定承诺**；本地 shallow clone（`git rev-list --count` = 1），无法评估提交频率或 issue 活跃度
- **入口**：`iroh-gossip/src/api.rs`（公开 API）；协议心智模型读 `src/proto.rs` 模块文档前 46 行（HyParView + PlumTree 全流程）；可跑参考 `examples/chat.rs`

### 它是什么

基于 **HyParView**（成员管理）+ **PlumTree**（广播树）的 topic 化 pubsub overlay。topic = 32 字节 TopicId，每个 topic 是独立的 swarm 和广播域。

分两层：`src/proto` 是**无 IO 的纯状态机**（可单独用/可仿真），`src/net` 是跑在 iroh Connection 上的网络层（`net` feature，默认开）。

> 注：`iroh-gossip/src/lib.rs:1` 是 `#![cfg_attr(feature = "net", doc = include_str!("../README.md"))]` —— README 内联是 net feature 条件门控的（net 在 default 中，故实际生效）。对比 `iroh-docs/src/lib.rs:1` 是无条件的 `#![doc = include_str!("../README.md")]`。

### 三条硬约束

#### 1. 不认证消息作者

`src/api.rs:361-372` 的 Message 结构只有三个字段：`content` / `scope` / `delivered_from`，其中 :370 注释明写 *"The endpoint that delivered the message. This is not the same as the original author."* —— **无 author、无 signature 字段**。

对 `src/proto/*.rs`、`src/net.rs`、`src/api.rs` grep `sign|signature|verify|authenticat` 的全部命中均为测试代码里的 SecretKey/insecure_skip_verify，**协议层无签名逻辑**。

**官方自己承认这是应用层责任**：`examples/chat.rs:26` 文档写 *"This broadcasts signed messages over iroh-gossip and verifies signatures on received messages"*，并自行定义 `struct SignedMessage { from: PublicKey, data: Bytes, signature: Signature }`（:224-229）与 `verify_and_decode`（:232），在 :193 手工调用。

> ⚠️ **措辞精确性**：说 gossip「不加密」是**失准**的。gossip 跑在 iroh Connection 上，**每一跳都是 QUIC/TLS 加密且端点已鉴权**（`iroh/README.md:47-49` *"Connections are authenticated against this key, which means an EndpointId can't be impersonated"*）。
>
> **准确表述**：gossip **不提供跨多跳转发的端到端机密性与作者鉴权** —— 逐跳加密，但中间转发者可见明文。

#### 2. 单条消息默认上限 4096 字节 —— 它是控制面，不是数据面

`src/proto.rs:66-72`：

```rust
/// The default maximum size in bytes for a gossip message.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 4096;
pub const MIN_MAX_MESSAGE_SIZE: usize = 512;
```

可经 `src/proto/topic.rs:185-192` 的 `Config::max_message_size` 调整，但该字段文档（:187）警告 *"This size should be the same across a network to ensure all nodes can transmit and read large messages"* —— 即**调大需全网一致，是网络级约定而非本地选项**。构造器入口 `src/net.rs:154` `Gossip::builder().max_message_size(size)`。

#### 3. NeighborUp/Down ≠ 「设备 X 上/下线」

`src/api.rs:336-345` 的 Event 枚举：
- `NeighborUp(EndpointId)` 注释 *"We have a new, direct neighbor **in the swarm membership layer** for this topic"*
- `NeighborDown` 注释 *"We dropped direct neighbor **in the swarm membership layer**"*

限定词是 **membership layer 的 direct neighbor**。为何不等价，看 `src/proto.rs:23-24`：

> *"If one of your active peers goes offline, its slot is filled with a **random** peer from the passive set."*

且 *"regularly exchanges nodes for the passive view in a Shuffle operation"*（默认 `shuffle_interval = 60s`）。

**即：active view 是会自发轮换的部分视图，NeighborDown 可能只是视图轮换。**

**这是「gossip 能不能做 presence」的决定性否定证据**：你要的是「我的 MacBook 现在在不在线」这种定向、确定的存活判定，gossip 给的是「某个随机邻居进/出了我的部分视图」。即使 N 很小时经验上接近，也是依赖实现细节的巧合，不是协议保证。

### HyParView 参数是为大规模 swarm 调的

`src/proto/hyparview.rs:197-221` 的 `impl Default for Config`：

| 参数 | 默认值 | 来源标注 |
|---|---|---|
| `active_view_capacity` | 5 | `// From the paper (p9)` |
| `passive_view_capacity` | 30 | `// From the paper (p9)` |
| `active_random_walk_length` | Ttl(6) | `// From the paper (p9)` |
| `passive_random_walk_length` | — | `// From the paper (p9)` |
| `shuffle_random_walk_length` | — | `// From the paper (p9)` |
| `shuffle_active_view_count` | — | `// From the paper (p9)` |
| `shuffle_passive_view_count` | — | `// From the paper (p9)` |
| `shuffle_interval` | 60s | **`// Wild guess`** |
| `neighbor_request_timeout` | 500ms | **`// Wild guess`** |

> ⚠️ **视图容量与随机游走长度照搬论文；定时器参数不是。** 别把「全套参数都是论文调优过的」当结论。

`src/proto.rs:17-18` 说明设计意图：*"The membership protocol ([HyParView]) is a cluster protocol where each peer maintains a **partial** view of all nodes in the swarm."*

**partial view 的全部价值在于 swarm 大到无法全连。** 当成员数 ≤ 6 时（每节点需容纳其余 ≤5 个），partial view 退化成 full view，gossip 提供的可扩展性价值为零，却仍要背 HyParView/PlumTree 的协议开销。**但注意**：若设备数到 10，partial view 是真实生效的（5 < 9），「退化成全网状」不成立 —— 这个规模论证只在 ≤6 时有效。

另 `src/proto.rs:14-15` 提示多 topic 的代价：*"joining multiple topics increases the number of open connections to peers and the size of the local routing table"*。

### 何时用 gossip

成员规模大 / 成员集合事先未知 / 需要「把一条小消息扩散给所有对该 topic 感兴趣的人」。iroh 官方定位（`iroh/README.md:49`）：*"establishing publish-subscribe overlay networks that scale, requiring only resources that your average phone can handle"* —— 移动端负担得起是明确设计目标。

典型：公共房间聊天、去中心化发现、大规模状态扩散、「多人临时共享房间」（陌生人凭房间码进同一 topic）。

## iroh-docs

- **成熟度**：**production（推断）**
- **依据**：
  - version 0.101.0；CHANGELOG.md:5 显示 v0.101.0 于 2026-06-15 发布（与 gossip 同日同版本，两者跟随 iroh 发版列车）；可回溯至 2024-11-04 的 0.28.0
  - HEAD 2026-07-15 `fix: don't abort receive loop on invalid message (#110)` —— 离调研仅 2 天，**明确在维护中**
  - README 与 CHANGELOG 均无免责/弃用声明
  - **工程厚度真实**：14552 行源码；4 个集成测试文件（`tests/{client,gc,sync,util}.rs`）；`proptest-regressions/` 目录；以及**成体系的存储迁移代码** `src/store/fs/migrate_v1_v2.rs` 与 `migrate_redb_v2_tuples.rs` —— **有真实存量数据才会写迁移**
  - ⚠️ **这是本 skill 的推断，不是 n0 的承诺**
  - ⚠️ **反向证据**：① 0.101.0 vs iroh 1.0.2，无 1.0 API 稳定承诺；② `.github/workflows/` 下**没有** release.yaml（gossip 有），疑手工发版；③ wasm32 CI 构建加了 `--no-default-features`（`ci.yaml:310`）；④ shallow clone
- **入口**：`iroh-docs/src/protocol.rs`（仅 132 行，最快入口）；操作面读 `src/api.rs`；数据模型读 `src/sync.rs`（Capability :186、Record :1125）

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

#### 4. 浏览器下丧失持久化

`ci.yaml:310` 为 `cargo build --target wasm32-unknown-unknown --no-default-features`，而 default 含 `fs-store` → wasm 下 fs-store/rpc/metrics 全部关闭。

> ⚠️ 准确表述：**官方仅验证 no-default-features 的 wasm 配置**，且 redb 需文件系统 → fs-store 在浏览器下无实际可用性。说「必须 --no-default-features」超出证据（CI 只证明官方唯一测试的配置是它，不等于其他配置不可编译）。

### 冲突消解语义

`Record` 的 `impl Ord`（`src/sync.rs:1140-1147`）是 timestamp 优先、再比 hash。

> ⚠️ **「last-write-wins」是本 skill 的推断标签，不是上游术语** —— 对 `iroh-docs/src/` 与 README.md grep `last-write-wins|last write wins|LWW` **零命中**。上述 Ord 是 value ordering 的定义；未找到它被用作冲突消解的直接证据。相比之下 automerge 的 CRDT 合并语义更适合真协作编辑。

### 何时用 iroh-docs

需要「一组设备/用户对同一份可变 KV 数据集最终一致，且带多作者署名与写权限控制」时。iroh 官方定位（`iroh/README.md:50`）：*"an eventually-consistent key-value store of iroh-blobs blobs"*。典型：共享文件夹、协作笔记索引、多端设置同步。

## irpc（现役）与 quic-rpc（前身）

### irpc

- **成熟度**：**production，但 pre-1.0 且破坏性变更频繁**
- **依据**：
  - version 0.17.0（`irpc/Cargo.toml:3`）；edition 2024；rust-version 1.91；最后提交 2026-07-01（`b6f8c46 refactor: simplify docsrs cfg (#105)`）
  - CHANGELOG.md:6 记 0.17.0 于 2026-06-15 发布并 **[breaking]** Update to iroh 1.0 and noq 1.0 —— **已跟上 iroh 1.x**
  - 生态实证：**6 个仓依赖 irpc，其中 5 个在 0.17** —— iroh-blobs(:41)、sendme(:42)、iroh-gossip(:66)、iroh-doctor(:48)、iroh-docs(:36)；第 6 个 iroh-dht-experiment(:19-20) 滞后在 **0.9.0**（最后提交停在 2025-10-21）
  - ⚠️ **稳定性折价**：**pre-1.0**，且 CHANGELOG 显示 15 个月内 10 个 minor release 有 **6 个带 [breaking]**（0.5.0 / 0.11.0 / 0.13.0 / 0.14.0 / 0.16.0 / 0.17.0）—— 平均每 **1.7** 个版本一次破坏性变更。**production-grade 的维护度 ≠ 稳定的 API 契约**
- **入口**：`irpc/src/lib.rs:1-153`（模块文档：Goals / Non-goals / 交互模式 / Transports / History）；`:539-870` 是 Client 的全部 API；最小可跑用法读 `examples/local.rs`

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

### irpc-iroh

- **成熟度**：**production，但生态实证仅 n=1**
- **依据**：
  - version 0.17.0（`irpc/irpc-iroh/Cargo.toml:3`）；与 irpc 主 crate 同仓同版本同步发布（`irpc/Cargo.toml:102` `members = ["irpc-derive", "irpc-iroh"]`）；`:16` `iroh = { workspace = true }`（解析到 iroh "1"）
  - 有 wasm CI 保障（`irpc/.github/workflows/ci.yml:130-151`，含断言 `! wasm-tools print --skeleton ... | grep 'import "env"'` 对 `irpc.wasm` 与 `irpc_iroh.wasm` 各一行）
  - ⚠️ **置信度应显式低于 irpc**：全生态 0.17 版的**外部消费者只有 iroh-doctor 一个**（`Cargo.toml:49`，`src/swarm/rpc.rs:6` 实际使用 `IrohRemoteConnection`），且它是诊断/压测 CLI 而非传输生产路径；另一个消费者 iroh-dht-experiment 停在 0.9.0 且仓名自陈 experiment。对比 irpc 的 5 个 0.17 消费者（含 iroh-blobs/iroh-gossip/iroh-docs 这类真实生产 crate）
- **入口**：`irpc/irpc-iroh/src/lib.rs`；最该读的是 `examples/remote-and-local.rs`（同一个 StorageApi 本地/远端两用）

**未来变动风险**：`irpc-iroh/src/lib.rs:44-45` 留有 TODO：*"remove this and provide a From instance as soon as iroh is 1.0 and we can move irpc-iroh into irpc?"*（紧跟 :47 `pub struct IrohRemoteConnection(Connection);`）。iroh 现已 1.0.2、irpc 已更新到 iroh 1.0 —— **前置条件已满足但尚未执行**，即 irpc-iroh 未来可能被并入 irpc 主 crate。

**反直觉点**：irpc **主 crate 的原生 transport 是 noq（socket 地址）而非 iroh**（`irpc/src/lib.rs:36-40` 的 `## Transports` 段：*"These must always be noq streams ... This restricts the possible rpc transports to noq (QUIC with dial by socket address) and iroh (QUIC with dial by endpoint id)."*）—— p2p 需额外引 irpc-iroh。

### quic-rpc —— 不要用

- **成熟度**：**abandoned**
- **依据**：
  - irpc 亲口承认继承：`irpc/src/lib.rs:148-153` `# History` 段：*"This crate evolved out of the [quic-rpc](...) crate, which is a generic RPC framework for any transport with cheap streams such as QUIC. Compared to quic-rpc, this crate does not abstract over the stream type and is focused on iroh and our noq."*（反向确认：quic-rpc 仓内 grep `irpc` 在 *.md/*.rs/*.toml 中零命中 —— **单向继承声明**）
  - 最后提交 **2025-05-12**（`0e3358e chore: Release`），距 irpc 的 2026-07-01 有 **14 个月空窗**
  - version 0.20.0（`Cargo.toml:3`）；edition 2021 / rust-version 1.76（irpc 已 edition 2024 / 1.91）
  - **致命**：`Cargo.toml:22` `iroh = { version = "0.35", optional = true }`，而 iroh 现为 1.0.2 —— 其 iroh-transport 与当前 iroh **完全不兼容**；CHANGELOG 的 `[unreleased]` 段还停在 "Update to iroh@0.29.0"
  - 生态实证：依赖 quic-rpc 的**只有它自己的 examples 和 quic-rpc-derive**，**外部依赖数 0**
  - ⚠️ **措辞准确性**：quic-rpc 的 README 和 src/lib.rs 里**没有任何显式 deprecation 声明**。abandoned 判定靠上述三项证据（提交时间 / 版本 pin / 零依赖）+ irpc 的单方面继承声明
- **它砍掉了什么**：quic-rpc 抽象了 transport（`src/transport/` 下有 flume / quinn / hyper / iroh / combined / boxed / mapped 七种）—— 这个「抽象 stream 类型」正是 irpc 主动砍掉的东西
- **唯一例外场景**：需要 **HTTP/2 transport**（README 称大块数据吞吐上 http2/tcp 仍优于 QUIC），而 irpc 只支持 noq/iroh —— 但代价是绑死 iroh 0.35，**实践中不可接受**
- **仍有价值的部分**：`quic-rpc/README.md` 的 `Why?` 一节把「optional rpc framework / 进程内子系统边界」的设计动机讲得比 irpc 更透，作为**设计思路读物**仍可一读

## CRDT / automerge —— 只有示例，没有库

**⚠️ 常见误判：automerge 集成不在 iroh-experiments，在 iroh-examples。**

对全 iroh-study 树 grep `automerge` 的命中**全部**落在 `iroh-examples/` 下。iroh-experiments **零命中**（其 README 列出的全部内容为 content-discovery、h3-iroh、iroh-dag-sync、iroh-pkarr-naming-system、iroh-s3-bao-store）。

两仓定位不同：
- `iroh-experiments/README.md`：*"Things in here can be very low level and unpolished"*
- `iroh-examples/README.md:9-10`：*"Examples how to use iroh... should be somewhat easy to understand"*，:12 指路 *"For very experimental things there is [iroh-experiments]"*

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

## 现成的全栈装配参考：tauri-todos

- `iroh-examples/tauri-todos` 是一个 **509 行**的 Tauri 应用，结构为 `src-tauri/src/{lib.rs, main.rs, ipc.rs, iroh.rs, state.rs, todos.rs}` + 根目录 vite.config.ts / tsconfig.json / package.json
- 依赖 tauri ^2 + iroh 1.0.0 + iroh-docs 0.101
- 它也用 `name = "tauri_todomvc_lib"` 规避 Windows lib/bin 命名冲突，注释 *"The _lib suffix may seem redundant but it is necessary to make the lib name unique and wouldn't conflict with the bin name. This seems to be only an issue on Windows"* —— **Tauri + Rust lib 的通用做法**
- `src/iroh.rs:20-49` 给出**持久化全栈装配**：`load_secret_key` → `Endpoint::builder(presets::N0).secret_key(key).bind()` → `Gossip::builder().spawn()` → `FsStore::load(&path)` → `Docs::persistent(path).spawn(endpoint, (*blobs).clone(), gossip.clone())` → `Router::builder().accept(BLOBS_ALPN/GOSSIP_ALPN/DOCS_ALPN).spawn()`

**即便最终不采用 docs/gossip，Endpoint 装配与 Router 注册那段可直接对照。**

## 浏览器可行性

| | wasm CI | 备注 |
|---|---|---|
| **iroh-gossip** | ✅ `ci.yaml:150` job "Build wasm32"，:169 `cargo build --target wasm32-unknown-unknown`（**未加** --no-default-features，即默认 net+metrics 可用），:175 断言无 `import "env"` | 有真实浏览器实跑参考 `iroh-examples/browser-chat`（README:3-5 含线上 demo 链接），其 Cargo.toml 用 `iroh-gossip = { version = "0.101", default-features = false, features = ["net"] }`；架构为 shared(Rust lib) + cli + browser-wasm(wasm-bindgen) + frontend(TypeScript/React/Vite/shadcn) |
| **iroh-docs** | ⚠️ `ci.yaml:282` job "Build & test wasm32"，:310 为 `--no-default-features` → default 的 metrics/rpc/fs-store/redb-v2-migration **全部关闭** | **浏览器里丧失持久化**，只能内存态 |
