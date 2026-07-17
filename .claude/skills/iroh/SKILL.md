---
name: iroh
description: "iroh 1.0.2 P2P 网络栈（QUIC + relay + 内容寻址）的完整指南，按 docs.iroh.computer 官方分区组织：既回答「该用生态里哪个库、它成熟吗、代价是什么」，也回答「怎么正确地写」。覆盖 24 个仓的能力边界与成熟度判定，以及 Endpoint 生命周期与 presets、crypto_provider 注入、Connection/流原语与背压、Router/ProtocolHandler/ALPN 分发、AcceptError、relay 选路与自建、address_lookup（pkarr/DNS/mDNS/DHT，非内置 DHT）、Watcher（online/home_relay_status/watch_addr）、tickets 邀请编码、iroh-blobs 与 bao-tree 与 sendme 的 pull 式传输、gossip/docs/irpc 高层协议、uniffi/napi/C FFI 与移动端、wasm/浏览器能力天花板、NAT 穿透与替代传输、feature flags 与跨平台交叉编译、Iroh Services 的锁定边界、版本承诺与兼容性政策。重点标注与 libp2p 的心智差异（libp2p 用户会成片踩空）。触发场景：编写或修改任何 iroh 相关 Rust 代码，配置 Endpoint::builder / presets / relay_mode / address_lookup，实现 ProtocolHandler 或注册 ALPN，调试 BindError / AcceptError / 连接不上 / relay 状态 / Watcher 挂死，交叉编译到 wasm32 / iOS / Android，自建 relay 或 pkarr/DNS 服务器，设计邀请链接或分享码，评估「该不该引入 iroh-blobs / bao-tree / gossip / docs / irpc」「有没有现成的库做 X」「这个 crate 维护还活着吗」「n0 会不会锁定我」「浏览器/移动端能做到哪一步」，或用户提到 iroh、n0、number0、EndpointId、EndpointAddr、noq、pkarr、blobs、bao、sendme、dumbpipe、iroh-relay、iroh-ffi、n0-future、n0-watcher 时。"

---

# iroh 1.0.2 开发指南（按官方分区组织）

基于 **iroh v1.0.2** 源码精读（调研日期 **2026-07-17**，源码快照 `/Volumes/yexiyue/iroh-study/`，**24 个仓**）。
所有论断都带文件路径 + 行号，可自行核对。

**本 skill 的结构对齐 [docs.iroh.computer](https://docs.iroh.computer) 的官方分区**，好处有二：
每个主题有单一归属（不用猜去哪找、往哪加），且查官方文档时能一一对应。

> 🚀 **第一次用 iroh / 只想先跑通两端** → [`00-getting-started.md`](references/00-getting-started.md)
> （实测通过的最小 Cargo.toml + `echo.rs` 逐字全文 + 新手 8 个坑）。
> **别照抄 iroh 仓库 README 的 quickstart —— 它对 1.0.2 编译不过**（`Endpoint::bind()` 缺 preset、
> `spawn().await` 不是 async），详见该文坑 1。

> **路径约定**：本文所有路径相对 iroh-study 根。**注意 iroh crate 的真实路径是 `iroh/iroh/src/...`，比直觉多一层。**
>
> **方法论边界（必须知道）**：iroh-study 下的仓库多为 **shallow clone（depth=1）**，`git log` 只有 1 条，
> 且**不是 git 仓库整体**（无统一 `.git`）。因此成熟度判定**不使用**提交频率与 issue 活跃度，只用五类证据：
> **版本号 / HEAD 日期 / 依赖版本 / README 声明 / CI 配置**。要评活跃度请 `git fetch --unshallow` 或直接看 GitHub。

---

## 怎么用这份 skill：两套索引，按你手里的问题选

**这份 skill 有两层目录，解决两类完全不同的查找。别在错的那层里翻。**

| 你现在的处境 | 用哪层 | 入口 |
|---|---|---|
| **不知道去哪找**（「relay 怎么自建」「怎么写协议」「浏览器能做到哪一步」）| **按官方分区**：00–10，每个主题单一归属 | [下方分区导航](#按官方分区导航0010) |
| **撞到报错 / 挂死 / 静默失灵**（此时你根本不知道它属于哪个分区）| **按症状** | [`index-gotchas.md`](references/index-gotchas.md) |
| **选型**（「有没有现成的库做 X」「这个 crate 维护还活着吗」「代价多少」）| **按能力** | [`index-ecosystem-map.md`](references/index-ecosystem-map.md) |
| 「这个 `Watcher` / `n0-error` / `noq` 是什么鬼」（官方文档完全没讲）| **地基** | [`index-foundations.md`](references/index-foundations.md) |

**为什么要有 `index-` 这三个正交层？** 因为它们按**用途**而非**领域**组织，天然横跨所有分区，打散就失效：

- **gotchas** —— 你先撞到的是 `BindError`、是挂死、是「某个 ALPN 神秘失灵」，**此时无从判断它属于哪一分区**。
  99 条坑分 A–K 十一组，根因散在 Connecting / Protocols / Configuration / Deployment / Languages / foundations 六处。
- **ecosystem-map** —— 「要做 X 用哪个库」跨全部分区，还含官方文档**结构上不存在**的东西：
  成熟度判定、「不存在——别找了」清单、导航陷阱（两个 iroh-js、两个 RelayConfig、改名后 grep 不到）。
- **foundations** —— n0-future / n0-watcher / n0-error / noq 是 iroh 的地基，官方零覆盖，但会在你写第一行代码时咬到你。

### 按官方分区导航（00–10）

| 文件 | 官方对应 | 内容 |
|---|---|---|
| [**00-getting-started.md**](references/00-getting-started.md) | Getting Started | iroh 是什么 / 不解决什么、实测最小 Cargo.toml、`echo.rs` 与 `echo-no-router.rs` 逐字对照、真·两进程实测、**新手 8 个坑**、17 个官方例子清单 |
| [**01-concepts.md**](references/01-concepts.md) | Concepts | **只回答「这是什么」**：Endpoints（身份就是公钥）/ Relays（是 transport 不是配置、home relay 是算出来的、线协议）/ Address Lookup（没有 DHT）/ NAT Traversal（没有 STUN、没有 DCUtR）/ Protocols / Tickets |
| [**02-connecting.md**](references/02-connecting.md) | Connecting | **怎么连上**：Creating an Endpoint（preset / crypto_provider / bind / BindError / close / 观察状态）、DNS、DHT、mDNS、**Endpoint Hooks（三层入站门禁）**、Gossip Broadcast |
| [**03-protocols.md**](references/03-protocols.md) | Protocols | 分区导航页，下含四个子文件 ↓ |
| ├ [03a-using-quic.md](references/03a-using-quic.md) | Using QUIC | 四个流原语、finish/reset/stop、六种交互模式、背压与 100 并发上限、内存上界、优先级、Datagram、关闭连接、0-RTT |
| ├ [03b-writing-a-protocol.md](references/03b-writing-a-protocol.md) | Write your own Protocol | 可编译模板、ALPN 命名与版本、Router/ProtocolHandler/DynProtocolHandler、AcceptError、流的粒度、framing、panic 连坐、关闭编排、上线 checklist |
| ├ [03c-blobs.md](references/03c-blobs.md) | Blobs | bao-tree（outboard / 0.39% / pre-post-order）、iroh-blobs（store 后端 / range-set 续传 / FsStore 布局 / tag+GC）、sendme、dumbpipe |
| └ [03d-docs-rpc-automerge.md](references/03d-docs-rpc-automerge.md) | Documents / RPC / Automerge | iroh-docs 四条代价、irpc 16MiB 硬上限与 Non-goals、quic-rpc 为什么 abandoned |
| [**04-transports.md**](references/04-transports.md) | Transports | custom transport API（数据报级 + PathSelector 陷阱）、Tor / Nym / BLE 的真实状态 |
| [**05-languages.md**](references/05-languages.md) | Languages | 绑定矩阵、iroh-ffi 两个硬伤、uniffi `&self` 门槛、`async_runtime="tokio"` 的真身、Android 手写 JNI、Apple 部署下限 17.5、JS napi、iroh-c-ffi |
| [**06-wasm-browser.md**](references/06-wasm-browser.md) | Languages → WebAssembly and Browsers | 能力天花板（relay-only 是**编译期**事实）、**存储层才是卡点**、pkarr 在浏览器**可用**（常见误传）、构建链路、体积、macOS ring 坑 |
| [**07-configuration.md**](references/07-configuration.md) | Configuration | 10 个 feature 全表、两个 cfg alias、`default-features=false` 的连带杀伤、crypto backend、各 target 速查、**自建 relay 全流程**、Compatibility |
| [**08-deployment.md**](references/08-deployment.md) | Deployment | 基础设施与成本模型、限流、Prometheus 指标、**Security & Privacy**（user_data 是唯一防线、DHT 键、relay 信任模型、准入控制） |
| [**09-iroh-services.md**](references/09-iroh-services.md) | Iroh Services | n0 的商业产品线、开源/闭源边界、**锁定风险的完整分析**、计费、三档脱钩方案 |
| [**10-about-and-policy.md**](references/10-about-and-policy.md) | About / Other | Release & Support Policy、**wire 兼容矩阵**、Roadmap（已过期）、Troubleshooting（含 iroh-doctor 深入）、FAQ |

---

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

> **这 10 条的正文分别在**：#1/#2/#9 → [03a](references/03a-using-quic.md) + [03b](references/03b-writing-a-protocol.md)；
> #3 → [01-concepts.md](references/01-concepts.md) 的 Address Lookup；#4/#5 → [02-connecting.md](references/02-connecting.md)；
> #6 → [03b](references/03b-writing-a-protocol.md)；#7 → [index-foundations.md](references/index-foundations.md)；
> #8 → [02-connecting.md](references/02-connecting.md)；#10 → [06-wasm-browser.md](references/06-wasm-browser.md)。

---

# 核心心智速查

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

## Critical 坑速览（详见 [index-gotchas.md](references/index-gotchas.md)）

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

## 关键版本与依赖事实

- **iroh 1.0.2**（`iroh/iroh/Cargo.toml:3`）——**API 已稳定**。注意 iroh 根 `Cargo.toml` **没有** `[workspace.package]`，版本在 `iroh/iroh/Cargo.toml:3`
- **iroh-blobs 0.103.0**——**未稳定**，且 README 第 3 行自述「not yet considered production quality. For now, if you need production quality, use iroh-blobs 0.35」。而 0.35 是旧架构（`api` 模块整个不存在，无 `FsStore`），**退不回去**。**别把 iroh 1.0 的稳定性承诺套到 iroh-blobs 头上——传输主力恰恰是那个没 1.0 的 crate**
- **noq / noq-proto / noq-udp 1.0.1**——n0 自家的 QUIC 栈（quinn 分支），**crates.io 依赖，不在 iroh-study 里**，源码在 `~/.cargo/registry/src/index.crates.io-*/noq-1.0.1/`
- **sendme 0.36** 依赖 iroh **1.0.0**（非 1.0.2）；**dumbpipe 0.39** 依赖 iroh **1.0.0**（`dumbpipe/Cargo.toml:19`）——引用它们时注意版本差
- **iroh-ffi 1.1.0**：uniffi 0.31.1（lock 0.31.2），纯 proc-macro 无 `.udl`；JS 走**完全独立的 napi-rs 3 通道**，不是 uniffi。⚠️ `Cargo.toml:5` 是 `publish = false`——1.1.0 **不是 crates.io release**
- **MSRV**：iroh 1.91 / n0-watcher 1.91 / n0-future 1.85。**迁 iroh 会一次性把 MSRV 底线抬到 1.91**
- **术语已重命名**（1.0 前夕的大范围改名，跨版本抄代码必踩）：`NodeId` → `EndpointId`、`NodeAddr` → `EndpointAddr`、`Discovery` → `AddressLookup`（0.96.0 breaking, PR #3853）、`discovery/` 模块 → `address_lookup/`、`EndpointMap/EndpointState` → `RemoteMap/RemoteState`、`CaRootsConfig` → `CaTlsConfig`。`Endpoint::conn_type` 已在 #3647 **移除**；公开返回 Watcher 的只剩 `watch_addr()` / `home_relay_status()` / `net_report()` 三个——**没有 `direct_addrs()`**
  > 📌 **推论**：iroh 生态里凡是 grep 不到 `discovery` / `pkarr` 的地方，**先想想是不是改名了**（例如 content-discovery 的 pkarr 集成以 `iroh-mainline-address-lookup` 的形式完整存在，字面 grep `pkarr` 却零命中）
