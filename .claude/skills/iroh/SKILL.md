---
name: iroh
description: "iroh 1.0.2 P2P 网络栈（QUIC + relay + 内容寻址）的完整指南：既回答「该用生态里哪个库、它成熟吗、代价是什么」，也回答「怎么正确地写」。覆盖 24 个仓的能力边界与成熟度判定，以及 Endpoint 生命周期与 presets、crypto_provider 注入、Connection/流原语与背压、Router/ProtocolHandler/ALPN 分发、AcceptError、relay 选路与自建、address_lookup（pkarr/DNS/mDNS/DHT，非内置 DHT）、Watcher（online/home_relay_status/watch_addr）、tickets 邀请编码、iroh-blobs 与 bao-tree 与 sendme 的 pull 式传输、gossip/docs/irpc 高层协议、uniffi/napi/C FFI 与移动端、wasm/浏览器能力天花板、NAT 穿透与替代传输、feature flags 与跨平台交叉编译。重点标注与 libp2p 的心智差异（libp2p 用户会成片踩空）。触发场景：编写或修改任何 iroh 相关 Rust 代码，配置 Endpoint::builder / presets / relay_mode / address_lookup，实现 ProtocolHandler 或注册 ALPN，调试 BindError / AcceptError / 连接不上 / relay 状态 / Watcher 挂死，交叉编译到 wasm32 / iOS / Android，自建 relay 或 pkarr/DNS 服务器，设计邀请链接或分享码，评估「该不该引入 iroh-blobs / bao-tree / gossip / docs / irpc」「有没有现成的库做 X」「这个 crate 维护还活着吗」「n0 会不会锁定我」「浏览器/移动端能做到哪一步」，或用户提到 iroh、n0、number0、EndpointId、EndpointAddr、noq、pkarr、blobs、bao、sendme、dumbpipe、iroh-relay、iroh-ffi、n0-future、n0-watcher 时。"
---

# iroh 1.0.2 开发指南（选型 + 写码）

基于 **iroh v1.0.2** 源码精读（调研日期 **2026-07-17**，源码快照 `/Volumes/yexiyue/iroh-study/`，**24 个仓**）。所有论断都带文件路径 + 行号，可自行核对。

这份 skill 同时回答两类问题，**先选型、后写码，本就是同一条路上的两步**：

1. **该用哪个** → 下方[能力 → 库速查表](#能力--库速查表)、[成熟度分级](#成熟度分级速览)、[`ecosystem-map.md`](references/ecosystem-map.md)
2. **怎么正确地写** → 下方[核心心智速查](#核心心智速查)、[Critical 坑速览](#critical-坑速览)、各 references

> **路径约定**：本文所有路径相对 iroh-study 根。**注意 iroh crate 的真实路径是 `iroh/iroh/src/...`，比直觉多一层。**
>
> **方法论边界（必须知道）**：iroh-study 下的仓库多为 **shallow clone（depth=1）**，`git log` 只有 1 条，且**不是 git 仓库整体**（无统一 `.git`）。因此成熟度判定**不使用**提交频率与 issue 活跃度，只用五类证据：**版本号 / HEAD 日期 / 依赖版本 / README 声明 / CI 配置**。要评活跃度请 `git fetch --unshallow` 或直接看 GitHub。

## 与 libp2p 的心智差异（迁移必读）

带着 libp2p 的思维定式读 iroh，以下 10 条会全部踩空。

| # | libp2p 的直觉 | iroh 的事实 | 后果 |
|---|--------------|------------|------|
| 1 | **流是稀缺资源**，要流池/限流器，超限 yamux 返回 `Err(TooManyStreams)` 打死整条连接 | 流超限是 `Poll::Pending` **背压排队**，从不报错 | 原有的限流/流池逻辑是**净负债** |
| 2 | 协议按 **substream** 用 multistream-select 协商，一条连接并发跑多协议 | ALPN 在 **QUIC/TLS 握手时协商一次**，一条连接终身绑定一个 ALPN → 一个 handler | 同时跑两个协议要开**两条 Connection** |
| 3 | Kademlia DHT + 路由表 + bootstrap 自组织发现 | **没有 DHT、没有路由表、没有迭代查找**。EndpointId 就是 Ed25519 公钥，解析 = 一次 HTTP GET 或 DNS TXT 查询 | 默认依赖 n0 的中心化服务；局域网发现要外挂 crate |
| 4 | drop 掉 Swarm 就完事 | **`endpoint.close().await` 必须显式 await**，不调则 Drop 里 `abort()` 硬砍 | 对端把「已传完的连接」判成超时失败 |
| 5 | Swarm 不是 Clone，必须自己 poll 事件循环 | `Endpoint` 是 `Clone`（`Arc<EndpointInner>`），自带后台 actor | 直接 `.clone()` 丢进 `tokio::spawn`，不需要 command channel |
| 6 | Behaviour/ConnectionHandler 是 **poll 状态机**（`&mut self`，手写状态字段） | `ProtocolHandler::accept` 是 **`async fn(&self, Connection)`**，线性顺序写 | handler 内部状态要自己 `Arc<Mutex<_>>`（只有 `&self`） |
| 7 | `swarm.next()` 是**无损有序**事件流 | `Watcher` 是 **last-value-wins 有损采样**，中间值会被覆盖跳过 | 别用 Watcher 统计状态变化次数 |
| 8 | rustls provider 靠进程默认 | `crypto_provider` 是 **Builder 字段的显式依赖注入** | `install_default()` 对 iroh **完全无效**（no-op） |
| 9 | `request_response::Codec` 给你类型化 Request/Response + 超时 + 重试 | 只给**裸双向流**，framing/序列化/超时/并发全自理 | 自定义协议要从零写 wire 层 |
| 10 | 浏览器有 WebRTC/WebTransport 可绕开 relay | 浏览器下 **IP transport 整个被编译掉**，只剩 relay + custom | Web 端**没有直连、没有打洞**，全程中继 |

> **第 8 条的正确对比**（常见误传）：libp2p-tls **也不读**进程默认 provider——它把 ring **硬编码在内部**（`libp2p-tls-0.6.2/src/lib.rs:48,75` 是 `rustls::crypto::ring::default_provider()`，一个**构造函数**，不是 `CryptoProvider::get_default()`），随后同样走 `builder_with_provider`。真实差异是：**libp2p 硬编码 ring 不给你选，iroh 要求你显式注入**。两者都不读进程默认。

---

# 第一部分：该用哪个（选型）

## 能力 → 库速查表

成熟度含义见[下方分级](#成熟度分级速览)；完整证据链见 [`ecosystem-map.md`](references/ecosystem-map.md)。

### 寻址与发现

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| 跨网寻址（EndpointId → 地址） | **iroh 内置 pkarr**（`presets::N0` 已默认装好） | production | `iroh/iroh/src/address_lookup.rs` |
| 局域网发现（对标 libp2p mDNS behaviour） | **iroh-mdns-address-lookup**（iroh 核心**不含** mDNS，必须外挂） | **beta** | `iroh-address-lookups/iroh-mdns-address-lookup/src/lib.rs` |
| 去中心化寻址（摆脱对 n0 DNS 的单点依赖） | **iroh-mainline-address-lookup**（BitTorrent Mainline DHT） | **beta** | `iroh-address-lookups/iroh-mainline-address-lookup/src/lib.rs` |
| 自建 pkarr relay + DNS 权威 | **iroh-dns-server** | production | `iroh/iroh-dns-server/config.prod.toml` |
| 「短码 → 地址」rendezvous | ❌ **生态里不存在**。iroh 的寻址原语只有 pubkey→addr | — | [`tickets.md`](references/tickets.md) |

### 数据传输

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| 逐块验签 / 可信断点续传（不要 blob store） | **bao-tree**（纯算法库，**不依赖 iroh 网络栈**） | production | `bao-tree/src/lib.rs:1-204` |
| 内容寻址 blob store + 传输 + GC + 多源下载 | **iroh-blobs** | **experimental**（README 自述非生产质量） | `iroh-blobs/src/api/remote.rs` |
| 文件传输的官方范式参考 | **sendme**（读它的 1184 行 main.rs） | **官方示例** | `sendme/src/main.rs` |
| 最小 P2P 骨架（裸管子，自研协议） | **dumbpipe**（lib.rs 只有 13 行） | production | `dumbpipe/src/lib.rs` |
| 内容发现（"谁有这个 hash"） | **iroh-content-discovery**（tracker 式，非 DHT） | experimental | `iroh-experiments/content-discovery/` |

### 邀请 / 分享链接

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| 把「连到某节点所需的一切」编码成可粘贴字符串 | **iroh-tickets** 的 `Ticket` trait（自定义 payload） | production | `iroh-tickets/src/lib.rs` |
| 分享一个 blob（地址 + hash） | **iroh-blobs 的 `BlobTicket`** | 随 iroh-blobs | `iroh-blobs/src/ticket.rs` |
| 过期 / 一次性 / 撤销 | ❌ **ticket 零支持**，必须自己在 payload 里做 | — | [`tickets.md`](references/tickets.md) |

### 高层协议

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| 大规模 swarm 的 topic pubsub | **iroh-gossip** | production（推断） | `iroh-gossip/src/api.rs` |
| 多设备 KV 最终一致同步（共享文件夹） | **iroh-docs**（meta-protocol：强制 blobs + gossip 三件套） | production（推断） | `iroh-docs/src/protocol.rs` |
| Rust↔Rust typed actor 边界 / RPC | **irpc** | production，但 **pre-1.0 且破坏性变更频繁** | `irpc/src/lib.rs` |
| 把 irpc 跑在 iroh 上 | **irpc-iroh** | production（生态实证仅 n=1） | `irpc/irpc-iroh/src/lib.rs` |
| ~~RPC 抽象 transport~~ | ❌ **quic-rpc 已被 irpc 取代**，卡死在 iroh 0.35 | **abandoned** | `quic-rpc/` |
| CRDT 协作文档 | ⚠️ 生态里只有 **示例**（iroh-automerge / iroh-automerge-repo），无库 | experimental | `iroh-examples/iroh-automerge/src/protocol.rs` |

### 传输层

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| 自建 relay | **iroh-relay**（`--features server`，自带 binary + Dockerfile） | production | `iroh/iroh-relay/src/main.rs` |
| 自定义物理/覆盖网络 | **custom transport API**（数据报级，非 stream 级） | **不受 semver 保护** | `iroh/iroh/src/socket/transports/custom.rs` |
| Tor 隐藏服务传输 | **iroh-tor-transport**（要外部 Tor daemon + control port） | experimental | GitHub `n0-computer/iroh-tor-transport` |
| Nym mixnet 传输 | **iroh-nym-transport**（~15-20 KiB/s，README 自陈不适合文件传输） | experimental | GitHub `n0-computer/iroh-nym-transport` |
| BLE / 蓝牙传输 | ❌ **不存在**。只在 `iroh/TRANSPORTS.md:10` 预留了 id 0x424C45，repo 列为空 | — | `iroh/TRANSPORTS.md:10` |

### 语言绑定

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| Swift / Kotlin / Python | **iroh-ffi**（uniffi 0.31） | production | `iroh-ffi/src/endpoint.rs` |
| Node.js | **iroh-ffi/iroh-js** = npm `@number0/iroh`（napi-rs 3） | production | `iroh-ffi/iroh-js/src/endpoint.rs` |
| C / C++ / Go / 嵌入式 | **iroh-c-ffi**（safer-ffi，**全同步阻塞**） | production（打折：0.101.0 + edition 2021） | `iroh-c-ffi/src/endpoint.rs` |
| React Native Turbo Module | ❌ **不存在**，uniffi_bindgen 内置 backend 只有 kotlin/python/ruby/swift | — | [`ffi-and-bindings.md`](references/ffi-and-bindings.md) |
| 浏览器 / wasm | **iroh 本体**（`default-features = false, features = ["tls-ring"]`） | production | `iroh-examples/browser-echo/src/node.rs` |
| ⚠️ 顶层 `iroh-js/` 目录 | ❌ **墓碑**。2023-12-07 单 commit，指向已下线的 api.iroh.network | **abandoned** | `iroh-js/README.md` |

### 地基与工具

| 我要做 X | 用 Y | 成熟度 | 入口 |
|---|---|---|---|
| tokio ↔ wasm 的 spawn/time 垫片 | **n0-future**（native 上就是 `pub use tokio::*`） | production | `n0-future/src/task.rs` |
| 状态广播（latest-value-wins） | **n0-watcher**（是 iroh 公开 API，用 iroh 就躲不掉） | production | `n0-watcher/src/lib.rs` |
| 带 location 的错误库 | **n0-error** —— 但**大概率不该用**（location 生产默认不采集） | production | `n0-error/src/meta.rs` |
| 网络诊断（relay 可达性 / NAT / 打洞实测） | **iroh-doctor** CLI（**不是可嵌入的库**） | production | `iroh-doctor/src/doctor.rs` |
| 云端指标 dashboard / 托管 relay | **iroh-services**（**纯选配 SaaS**，开源 iroh 对它零依赖） | production | `iroh-ffi/src/services.rs` |

## 成熟度分级速览

**iroh 1.0 ≠ 生态 1.0**：iroh / iroh-tickets / iroh-relay / iroh-dns-server / n0-watcher / n0-error / bao-tree 已 1.x（bao-tree 是 0.16 但按证据判 production）；blobs(0.103) / gossip(0.101) / docs(0.101) / irpc(0.17) / address-lookups(0.4) 全是 0.x，**无 API 稳定承诺**。

| 分级 | 含义 | 成员 |
|---|---|---|
| **production** | 可生产依赖 | iroh · iroh-relay · iroh-dns-server · iroh-tickets · bao-tree · dumbpipe · n0-future · n0-watcher · n0-error · iroh-ffi · iroh-c-ffi(打折) · iroh-doctor · iroh-services |
| **production（推断）** | 证据支持但**无上游背书** | iroh-gossip · iroh-docs |
| **beta** | 能用，别当 1.0 看 | iroh-mdns-address-lookup · iroh-mainline-address-lookup · n0-mainline |
| **experimental** | 读，别依赖 | iroh-blobs · custom transport API · swarm-discovery · iroh-tor/nym-transport · iroh-experiments/\* · iroh-automerge\* · iroh-dht-experiment |
| **官方示例** | 抄模式，别当依赖 | sendme · browser-echo/chat/blobs · tauri-todos |
| **abandoned** | 不要用 | quic-rpc · 顶层 `iroh-js/` · bao-docs · iroh-s3-bao-store |

> ⚠️ 两条容易被写高的：**sendme 是「官方示例，同时作为可用工具发布」**（`README.md:11-15` 两句成对：*"**This is an example application**..."* → *"It is **also** useful as a standalone tool"*，`also` 一词的全部作用就是「首先是示例」）；**mDNS / DHT lookup 是 beta 不是 production**（0.4/0.5 pre-1.0；DHT 唯一测试 `#[ignore = "flaky"]`；mDNS 核心压在 alpha 依赖 `swarm-discovery 0.6.0-alpha.2` 上；n0-mainline 把 `ed25519-dalek` 精确 pin 在 `=3.0.0-rc.0`）。

## 三条最重要的选型结论

### 1. 要 bao-tree，多半不要 iroh-blobs

bao-tree 是**不依赖 iroh 网络栈**的纯算法 crate（`bao-tree/Cargo.toml:16-38`；`default-features = false, features = ["validate"]` 连 tokio 都不引），精准补上「逐块验签」，代价约 **0.39% outboard 存储**，`Outboard` trait 可自实现 → outboard 能进你自己的 SQLite/KV。**与「迁不迁 iroh」完全解耦**。

iroh-blobs 则是整套 store：README 自述非生产质量、**全库零加密原语**、浏览器只有 MemStore、fs store 有已验证的 Poisoned panic 路径（#233）、强制 tag/GC 心智、`FsStore::load` 还会自建一个独立的 multi_thread tokio runtime。→ [`blobs-and-file-transfer.md`](references/blobs-and-file-transfer.md)

### 2. 迁 iroh 的最小形状是 dumbpipe，不是 iroh-blobs

n0 自己的分界线：**bulk data plane 手写 `ProtocolHandler`，control/progress 才上 irpc**（iroh-blobs 自己也是这个模式）。iroh-blobs 是 pull 模型，push API 源码自述 experimental 且 `EventMask::DEFAULT` 里 `push: RequestMode::Disabled` —— **协议默认关闭**，官方还明确拒绝提供开启 push 的便捷常量（`iroh-blobs/src/provider/events.rs:200-203`）。

dumbpipe 证明最小可用 iroh P2P 的协议定义只需 **13 行**（一个 ALPN 常量 + 5 字节 handshake + 一个 re-export）。→ [`blobs-and-file-transfer.md`](references/blobs-and-file-transfer.md)

### 3. 不会被 n0 锁定，但默认配置会静默依赖 n0

**锁定风险不成立**：开源 iroh 对 iroh-services **零依赖、零 phone-home**（iroh/iroh-relay/iroh-base 三个 Cargo.toml grep `iroh-services` 零命中；全仓 grep `services\.iroh\.computer|api_secret` 零命中）；iroh-relay 自带 server binary + Dockerfile，自建是一等公民路径。

**但**：`presets::N0` 会静默拖入 n0 的三项基础设施（pkarr publisher + pkarr resolver + DNS lookup，全部指向 dns.iroh.link），且 `.relay_mode(RelayMode::Custom(..))` **只覆盖 relay，不移除 address lookup**——想彻底自建必须用 `presets::Minimal`，或显式 `clear_address_lookup()`。→ [`relay.md`](references/relay.md) · [`tooling.md`](references/tooling.md)

---

# 第二部分：怎么正确地写

## 核心心智速查

### Endpoint = QUIC socket + 后台 actor + 可插拔发现

```rust
use iroh::{Endpoint, endpoint::presets};

// 最常用：N0 preset = Minimal(crypto) + pkarr 发布/解析 + DNS 查找 + 默认 relay
let ep = Endpoint::builder(presets::N0)
    .secret_key(key)              // 不给就每次随机生成新身份！
    .alpns(vec![ALPN.to_vec()])   // 只接受入站时才需要；纯 connect 传 vec![]
    .bind().await?;               // 返回即 socket 已绑好，失败直接 Err

ep.online().await;                // 等至少一个 relay 完成握手（外面务必包 timeout）
let addr = ep.addr();             // EndpointAddr = id + BTreeSet<TransportAddr>

// 关闭必须 await，不能省
ep.close().await;
```

**preset 没有魔法**——`Preset` trait 唯一方法是 `apply(self, Builder) -> Builder`，等价于一串手写链式调用，且 **preset 先应用、后续 setter 可覆盖**（`iroh/iroh/src/endpoint/presets.rs:21-24`, `endpoint.rs:180-188`）。

| preset | 内容 | 何时用 |
|--------|------|--------|
| `Empty` | **恒等函数，一个字段都不设** → `bind()` **必然失败** | 只在自带 crypto_provider 时（见 `examples/pq-only-key-exchange.rs`） |
| `Minimal` | **只设 crypto_provider**（ring 优先于 aws-lc-rs） | 孤岛 endpoint：无 relay、无发现，靠手工 `EndpointAddr` 直连。**自建基础设施时固定用它**。iroh 自己的测试也以它为主 |
| `N0` | Minimal + `PkarrPublisher::n0_dns()` + `PkarrResolver::n0_dns()` + (非 wasm) `DnsAddressLookup::n0_dns()` + `relay_mode(default_relay_mode())` | 默认选择。**隐含对外网络行为：把地址发布到 n0 的 dns.iroh.link** |
| `N0DisableRelay` | **先跑完整 N0 再 `relay_mode(Disabled)`** | ≠ 离线：pkarr 发布/DNS 查找一个不少，只是不走 relay |

> ⚠️ **别照 N0 的 doc 摘要数它含什么**：`presets.rs:81-87` 的三条 bullet 只提「the DNS Address Lookup service / default relay servers / CryptoProvider」，**漏掉了 pkarr 的发布与解析**；但同一段 doc 的 `:99-101` 又写明 *"This is equivalent to adding a PkarrPublisher, a PkarrResolver, and (outside browsers) a DnsAddressLookup"* ——**以 `:116-140` 的 impl 为准**（三项 address_lookup 全在）。

三个 preset 都被 `#[cfg(with_crypto_provider)]` 门控 = `any(feature="tls-ring", feature="tls-aws-lc-rs")`（`iroh/iroh/build.rs`）。**`presets::Staging` 不存在**——Staging 只是 `RelayMode` 的枚举变体。

### 流很便宜（这是最大的心智解放）

```rust
// Connection 是 Clone，open_bi 只要 &self —— 直接 clone 进 N 个 task 各开各的流
for sock in incoming_sockets {
    let conn = conn.clone();
    tokio::spawn(async move {
        let (mut send, recv) = conn.open_bi().await?;
        send.write_all(b"hello").await?;   // ← 必须先写，否则对端 accept_bi 永不返回
        // ...
    });
}
```

写进 rustdoc 的正式合同（`iroh/iroh/src/endpoint/connection.rs:820-831`, `lib.rs:147-148`）：

> "may be opened and closed without incurring any extra cost" / "Opening a new stream does not incur any extra overhead compared to sending data on an existing stream" / "extremely light-weight ... Allowing many streams to co-exist, regardless of how long they last"

**但「便宜」≠「无限并发」**：默认 bi/uni 各 **100** 条并发上限（noq-proto 默认值，iroh 应用面未覆盖）。超限是 `Poll::Pending` 排队，不是错误。且你设的 `max_concurrent_*_streams` 限制的是**对端能向你开多少**，不是你能开多少——与 yamux 语义**正好相反**。

### Router = ALPN → handler 的 BTreeMap + 一个自带 task 的 accept 循环

```rust
#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = connection.accept_bi().await?;
        tokio::io::copy(&mut recv, &mut send).await.map_err(AcceptError::from_err)?;
        send.finish()?;
        connection.closed().await;   // 等对端先关，否则 return 即 drop conn 可能截断响应
        Ok(())
    }
}

let router = Router::builder(endpoint)
    .accept(ALPN, Echo)      // 只是往 BTreeMap insert
    .spawn();                // ← 这里才 set_alpns + 起 accept 循环。#[must_use]！
```

`ProtocolHandler` 三个方法，只有 `accept` 必须实现（`iroh/iroh/src/protocol.rs:228-287`）：

- `on_accepting(&self, Accepting) -> Result<Connection, AcceptError>`——默认 `accepting.await`。早期拦截点 / 0-RTT 入口
- `accept(&self, Connection) -> Result<(), AcceptError>`——无默认实现
- `shutdown(&self) -> ()`——默认空。`Router::shutdown()` 会**并发调用并等待所有 handler 的 shutdown 跑完**，之后才 abort accept future

全部走 RPITIT（`impl Future`），所以直接写 `async fn accept(&self, ...)`；supertrait 是 `Send + Sync + Debug + 'static`（**忘了 `#[derive(Debug)]` 编译不过**）。

### 没有 DHT：解析就是一次名字查询

`EndpointId` = `PublicKey` 的 type alias（`iroh/iroh-base/src/key.rs:70`），**就是 32 字节 Ed25519 公钥本身，不是哈希**。

```
发布：HTTP PUT  https://dns.iroh.link/pkarr/<z32-endpoint-id>   （签名包，服务器不可伪造）
解析：HTTP GET  https://dns.iroh.link/pkarr/<z32-endpoint-id>   （PkarrResolver）
     DNS TXT   _iroh.<z32-endpoint-id>.dns.iroh.link           （DnsAddressLookup，非浏览器）
```

信任来自记录上的 **Ed25519 签名**，不是 DHT 拓扑。所以中心服务器**不可伪造内容**，但可以审查/丢弃/观测查询者。

发布/解析**职责分离**：`PkarrPublisher` 只实现 `publish`，`PkarrResolver`/`DnsAddressLookup` 只实现 `resolve`（trait 两个方法都有默认空实现）。只加 `DnsAddressLookup` 你能查别人，别人查不到你。

**iroh 1.0.2 里没有 mDNS**——不是 feature 关着，是代码不在这个 crate 里（`iroh/iroh/src/address_lookup.rs:46-51` 只有 5 行文档链接指向 `iroh-mdns-address-lookup` / `iroh-mainline-address-lookup` 两个独立 crate）。要局域网发现必须加依赖，**不是开 feature**（0.x 时代的 `discovery-local-network` feature 已废除）。

### 隐私

**iroh 全链路从不发布 hostname / 用户名 / 设备名**——可发布字段只有地址集 + 可选 `user_data`，且 `user_data` 默认 `None`、必须显式 opt-in（`iroh/iroh-dns/src/endpoint_info.rs:70-76`；发布路径 grep `hostname|whoami|username|device_name` 零命中）。

`PkarrPublisher` 自带默认 `AddrFilter::relay_only()`，doc 明写「This avoids leaking IP addresses to the public pkarr server」——**默认不外泄 IP**。

> **反直觉的坑**：`AddrFilter` 只过滤地址，**`apply_filter` 会原样保留 `user_data`**（`endpoint_info.rs:189-199`；filter 的函数签名里根本看不到 user_data）。所以 `relay_only()` 对 user_data **完全不设防**。防线在 user_data 那一侧，不在 AddrFilter。**唯一需要 code review 卡住的 API 是 `user_data_for_address_lookup()` / `set_user_data_for_address_lookup()`。**
>
> 另一条：**mDNS 与 DHT 的默认过滤策略完全相反** —— DHT 默认 `relay_only()`（不泄 IP），mDNS 默认 `AddrFilter::default()`（**恒等过滤器，不过滤**，广播全部本地 IP + relay + user_data）。同时装两个时，你以为设过 filter 了，其实只有一半生效。

## Critical 坑速览（详见 [gotchas.md](references/gotchas.md)）

| 症状 | 根因 |
|------|------|
| `BindError::InvalidCryptoProvider`，且 `install_default()` 无效 | iroh 全程 `builder_with_provider` 显式传参，**从不查进程默认**。`install_default()` 是纯 no-op |
| 对端把成功的传输判成超时失败 | 漏了 `endpoint.close().await`。Drop 里只 `abort()` 硬砍 |
| `open_bi()` 后 `read` 永久挂死 | 流是**懒创建**的：不写数据对端 `accept_bi()` 永不返回 |
| 服务端起来了但没人能连上 | `Router::spawn()` 返回值被丢弃 → `AbortOnDropHandle` 立刻 abort 循环 |
| 某个 ALPN 神秘失灵，无任何报错 | `spawn()` 的 `set_alpns` 是**覆盖式**的；或两次 `.accept()` 用了同一 ALPN（BTreeMap 静默覆盖） |
| 整个节点静默下线，`Router::shutdown()` 还返回 Ok | 任一 handler task **panic** → 掀掉 accept 循环 → `endpoint.close()` 连坐所有协议 |
| `home_relay_status().initialized()` 返回了但根本没连上 | `Nullable<Vec<T>>::into_option` 是 **`self.pop()`**，非空即返回。RelayActor 在**拨号前**就发布了 `Connecting` |
| `initialized().await` 静默僵死，无日志无错误 | Watchable 被 drop 后 `InitializedFut` 返回 `Poll::Pending` 且**永不再唤醒**（Output 是裸 `T`，类型上无法表达失败） |
| 换 relay 时 `is_connected()` 抖动，误报"掉线" | 切 home 时**无条件先 publish `Connecting`**，哪怕目标 relay 早已 Connected |
| 生产程序静默连到 staging relay | 环境里有非空 `IROH_FORCE_STAGING_RELAYS`。`presets::N0` 走 `default_relay_mode()` 会吃这个后门 |
| 自建 relay 每次 net_report 白等 3s | 客户端 `RelayConfig::from(RelayUrl)` **默认开 QUIC 地址发现（7842/udp）**，服务端 `enable_quic_addr_discovery` **默认 false**——两边默认值相反 |
| 自建 relay 打洞率莫名很低 | 照抄官方 Dockerfile 放行了 **3478/udp**（STUN 时代遗留）。QAD 的真实端口是 **7842/udp**；README 写的 7824 是数字转置 |
| 自以为完全自建，却每 5 分钟往 dns.iroh.link 发 pkarr | `presets::N0` + `relay_mode(Custom)` —— `relay_mode()` 只改 transports，**不碰 address_lookup**。固定用 `presets::Minimal` |
| `default-features = false` 后 `presets::N0` 不存在了 | 连带关掉 `tls-ring` → `with_crypto_provider` 为假 → 三个 preset 整个消失 |
| macOS 上 ring 编不到 wasm32 | **本地 toolchain 问题**，不是 iroh 约束：Apple 系统 clang 缺 wasm 后端。iroh CI 在 Linux 上带 `tls-ring` 编 wasm32 是**通过**的 |
| 抄官方测试代码，生产构建编译不过 | `CaTlsConfig::insecure_skip_verify()` 被 `#[cfg(any(test, feature = "test-utils"))]` 门控。别为它开 test-utils——那会把整个 relay server 链进客户端 |
| 鉴权服务拒绝掉每一个连接 | `access.http` callout 的 header 字面量是 **`X-Iroh-NodeId`**，不是 rustdoc 说的 `X-Iroh-Endpoint-Id` |

## 详细参考文档

| 文件 | 内容 |
|------|------|
| **[ecosystem-map.md](references/ecosystem-map.md)** | 生态全景图（mermaid）、**成熟度总表的完整证据链**（每条判定的版本号/HEAD 日期/依赖/README/CI 依据）、「不存在——别找了」清单（每条带 grep 级依据）、导航陷阱（两个 iroh-js / 改名后 grep 不到） |
| **[endpoint-and-presets.md](references/endpoint-and-presets.md)** | Preset 机制、4 个 preset 逐行拆解、`bind()` 12 步流程、8 个 BindError 变体、crypto_provider 依赖注入、`close()` vs `Drop`/`abort()` 的 4 点差异、Endpoint 的 Clone 语义 |
| **[streams-and-protocol.md](references/streams-and-protocol.md)** | 四个流原语签名与开销模型、100 条并发上限与背压、`max_concurrent_*` 的反直觉方向、内存上界（100×1.25MB）、Router/ProtocolHandler/DynProtocolHandler、ALPN 两层分发、AcceptError 5 变体、incoming_filter 三层门禁、关闭编排四步、0-RTT |
| **[foundations.md](references/foundations.md)** | **地基三件套**：n0-future（native 就是 tokio、wasm JoinSet 缺陷、test-util 副作用）、**n0-watcher 全解**（有损采样语义、`initialized()` vs `updated()` 的类型与断开双重不对称、`Nullable<Vec<T>>` = `pop()`、`online()` 的标准写法、close() 不断开 watcher）、n0-error（为什么多半不该迁）、MSRV 汇总 |
| **[relay.md](references/relay.md)** | relay 是 transport 不是配置、home relay 选优（近 5min 最小延迟 + 2/3 迟滞）、RelayMode 四态、WebSocket-only(0.91) 与协议 v2(0.98)、**自建全流程**（TLS 是 Option / cert_mode=Reloading / 端口真相 7842 / 准入控制四档 / 限流 / 成本模型 / accept_conn_limit 是死配置）、relay 不做暂存、同 EndpointId 双连接互顶 |
| **[address-lookup.md](references/address-lookup.md)** | pkarr/DNS/mDNS/DHT 四选一与代价、发布解析职责分离、pkarr 签名包格式与 1104 字节上限、**隐私全解**（user_data 是唯一防线 / AddrFilter 两层 AND / mDNS 与 DHT 默认相反 / DHT 键 = SHA1(EndpointId) 无 salt / 源 IP 暴露面）、组合行为（无优先级并发）、subscribe vs resolve 分流、移动端 multicast 空白区 |
| **[blobs-and-file-transfer.md](references/blobs-and-file-transfer.md)** | **bao-tree**（outboard 原理、0.39% 开销、pre/post-order 取舍、Outboard trait 自实现）、**iroh-blobs 四条硬伤**（零加密原语 / 浏览器只有 MemStore / #233 Poisoned panic / 自建 runtime）、range-set 差集续传、FsStore 四类存储位置、pull vs push 模型冲突、provider 事件拦截四档、**sendme 与 dumbpipe 逐行对照**、Collection/TempTag/GC |
| **[tickets.md](references/tickets.md)** | Ticket trait 自定义模板、长度实测（63/122/184 字符）、零过期/零一次性/零撤销、base32 大小写与 QR 优化、wire format 版本陷阱（Variant1 判别符是 0x00）、BlobTicket 的有损编码、KIND 前缀 |
| **[ffi-and-bindings.md](references/ffi-and-bindings.md)** | 绑定矩阵（uniffi/napi/C）、iroh-ffi 两个硬伤（不暴露 blobs / 不暴露 discovery）、uniffi `&self` 门槛导致的 `Mutex<Option<T>>` 降级、`async_runtime="tokio"` 的真身（async-compat TOKIO1 单线程）、回调 trait 与 WatchHandle、错误模型（Object 而非 Enum）、Android 手写 JNI 缺口、Apple 部署目标下限 17.5 与三个必链 framework、数据全是 Vec<u8> 拷贝的量化证据 |
| **[wasm-and-browser.md](references/wasm-and-browser.md)** | 浏览器能力天花板（relay-only 是**编译期**事实）、三个官方例子的共享架构范式、**存储层才是卡点**（iroh-blobs 只有 MemStore，#84/#86 未合）、pkarr 在浏览器**可用**（常见误传）、构建链路两种范式、体积数据（附可信度声明）、wasm-opt 是负收益、macOS ring 坑、Rust Stream → JS ReadableStream |
| **[protocols-gossip-docs-rpc.md](references/protocols-gossip-docs-rpc.md)** | gossip 三条硬约束（不认证作者 / 4096 字节上限 / **NeighborUp/Down ≠ presence**）、HyParView 参数来源、docs 四条代价（meta-protocol / ID 即读权限 / redb / 浏览器无持久化）、irpc 的正确位置（16MiB 硬上限、四条 Non-goals）、quic-rpc 为什么 abandoned、automerge 只有示例 |
| **[features-and-platforms.md](references/features-and-platforms.md)** | 10 个 feature 全表、两个 cfg alias、default features 在 wasm 上**能编过**、`default-features=false` 的连带杀伤、tls-ring vs tls-aws-lc-rs、fast-apple-datapath 的真相（死代码）、Android/iOS CI 实况、ring→wasm 的 toolchain 问题、各 target 速查 |
| **[nat-and-transports.md](references/nat-and-transports.md)** | **没有 STUN、没有 DCUtR 握手**、官方 16 组 NAT 矩阵（3 个 Hard 组合打不通）、QAD 走 UDP/7842、portmapper 能救家宽不能救 CGNAT、custom transport API（数据报级 + PathSelector 陷阱）、Tor/Nym/BLE 的真实状态 |
| **[tooling.md](references/tooling.md)** | iroh-doctor 7 个子命令与**三个「看起来能用其实不能」**（NAT 分类器没接线 / 永不返回 Easy / `--https` flag 是装饰性的）、relay-urls 超时硬编码 2s、iroh-experiments 逐个点评、iroh-services 的开源边界与锁定风险证伪 |
| **[gotchas.md](references/gotchas.md)** | **所有 critical 坑集中一处**，按「症状 → 根因 → 正确做法」组织。先撞到症状再来查的入口 |

## 关键版本与依赖事实

- **iroh 1.0.2**（`iroh/iroh/Cargo.toml:3`）——**API 已稳定**。注意 iroh 根 `Cargo.toml` **没有** `[workspace.package]`，版本在 `iroh/iroh/Cargo.toml:3`
- **iroh-blobs 0.103.0**——**未稳定**，且 README 第 3 行自述「not yet considered production quality. For now, if you need production quality, use iroh-blobs 0.35」。而 0.35 是旧架构（`api` 模块整个不存在，无 `FsStore`），**退不回去**。**别把 iroh 1.0 的稳定性承诺套到 iroh-blobs 头上——传输主力恰恰是那个没 1.0 的 crate**
- **noq / noq-proto / noq-udp 1.0.1**——n0 自家的 QUIC 栈（quinn 分支），**crates.io 依赖，不在 iroh-study 里**，源码在 `~/.cargo/registry/src/index.crates.io-*/noq-1.0.1/`
- **sendme 0.36** 依赖 iroh **1.0.0**（非 1.0.2）；**dumbpipe 0.39** 依赖 iroh **1.0.0**（`dumbpipe/Cargo.toml:19`）——引用它们时注意版本差
- **iroh-ffi 1.1.0**：uniffi 0.31.1（lock 0.31.2），纯 proc-macro 无 `.udl`；JS 走**完全独立的 napi-rs 3 通道**，不是 uniffi。⚠️ `Cargo.toml:5` 是 `publish = false`——1.1.0 **不是 crates.io release**
- **MSRV**：iroh 1.91 / n0-watcher 1.91 / n0-future 1.85。**迁 iroh 会一次性把 MSRV 底线抬到 1.91**
- **术语已重命名**（1.0 前夕的大范围改名，跨版本抄代码必踩）：`NodeId` → `EndpointId`、`NodeAddr` → `EndpointAddr`、`Discovery` → `AddressLookup`（0.96.0 breaking, PR #3853）、`discovery/` 模块 → `address_lookup/`、`EndpointMap/EndpointState` → `RemoteMap/RemoteState`、`CaRootsConfig` → `CaTlsConfig`。`Endpoint::conn_type` 已在 #3647 **移除**；公开返回 Watcher 的只剩 `watch_addr()` / `home_relay_status()` / `net_report()` 三个——**没有 `direct_addrs()`**
  > 📌 **推论**：iroh 生态里凡是 grep 不到 `discovery` / `pkarr` 的地方，**先想想是不是改名了**（例如 content-discovery 的 pkarr 集成以 `iroh-mainline-address-lookup` 的形式完整存在，字面 grep `pkarr` 却零命中）
