# Address Lookup：设备之间怎么互相找到

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

> relay 相关 → [relay.md](relay.md)。本文讲寻址与发现：**选哪个、代价是什么、怎么写**。
>
> ⚠️ **术语先行**：旧教程里的 `iroh/src/discovery/` 在 1.0.2 **不存在**。该模块已改名为 `address_lookup/`，`Discovery` trait → `AddressLookup`，是 0.96.0（2026-01-28）的 breaking rename（`iroh/CHANGELOG.md:487`，PR #3853）。builder 方法是 `.address_lookup()`，**不是 `.discovery()`**。docs.rs 上的老版本教程、绝大多数博客仍写 `Discovery`。
>
> 同时被移出核心的：0.x 时代的 `discovery-pkarr-dht` / `discovery-local-network` **feature 已不存在**（`iroh/iroh/Cargo.toml` 的 `[features]` 里查无此项），两者被彻底挪到独立仓 `iroh-address-lookups/`。历史脉络：`CHANGELOG.md:1495` 记录 0.23.0 时代曾有 `discovery-local-network`，1.0 已废除该模式。

## 一句话选型

| 场景 | 选择 | 代价 |
|---|---|---|
| 跨网寻址（默认） | **内置 pkarr**（`presets::N0` 已装好） | 信任 n0 的 dns.iroh.link 一家（它能看到你的 IP、EndpointId、你查过谁） |
| 浏览器 / wasm | **只能是 pkarr relay**（HTTP） | 没得选。DHT 与 mDNS 编译都过不去 |
| 局域网 / 离线内网 | **iroh-mdns-address-lookup**（beta） | 必须显式设 `addr_filter`（**默认不过滤**）；移动端权限是空白区 |
| 真去中心化（摆脱 n0 单点） | **iroh-mainline-address-lookup**（beta） | 源 IP 暴露给公共 BT DHT 的 bootstrap 与沿途节点；lookup 向全网泄露「你在找谁」 |
| 带外交换地址（ticket / 扫码 / 配对） | **MemoryLookup**，或干脆什么都不装 | 无 —— 这条路完全不需要任何寻址基础设施 |

三者**可同时装、无优先级**，并发查询先到先得。

---

# 第一部分：库清单

## 仓内只有 3 个模块 / 4 个类型 —— 没有 mDNS

```rust
// iroh/iroh/src/address_lookup.rs:120-128
#[cfg(not(wasm_browser))]
pub mod dns;
pub mod memory;      // 无门禁
pub mod pkarr;       // 无门禁

// address_lookup.rs:46-51
//! mDNS-based and Mainline-DHT-based Address Lookup services live in
//! separate crates: [`iroh-mdns-address-lookup`] and [`iroh-mainline-address-lookup`].
```

| 类型 | 职责 | wasm |
|------|------|------|
| `PkarrPublisher` | **只 publish**（HTTP PUT 签名 DNS 包到 pkarr relay） | ✅ 可用 |
| `PkarrResolver` | **只 resolve**（HTTPS GET） | ✅ 可用 |
| `DnsAddressLookup` | **只 resolve**（DNS TXT） | ❌ `#[cfg(not(wasm_browser))]` |
| `MemoryLookup` | 手动增删（本地表） | ✅ 可用 |

另有包装器 `FilteredAddressLookup<T>`（`:154` + impl `:180`）和 blanket `impl<T: AddressLookup> AddressLookup for Arc<T>`（`:352`）。

**`ls iroh/iroh/src/address_lookup/` 只有 dns.rs / memory.rs / pkarr.rs——无 static.rs、无 mdns.rs。**

**「静态/手动添加地址」这一角色由 `MemoryLookup` 承担，没有名为 `static` 的实现。**

> 关于「MemoryLookup 的旧名是 StaticProvider」：**本仓无法核实，别写。** `grep -rn "StaticProvider" iroh/src/` 零命中；`grep -rni "MemoryLookup"` 在 CHANGELOG.md / CHANGELOG_old.md 零命中；仓是 shallow clone 无历史可追。0.96.0 至今的全部 rename 条目只有：`Discovery`→`AddressLookup`(#3853)、`CustomAddr::as_vec`→`to_vec`(#4074)、`CaRootsConfig`→`CaTlsConfig`(#4300)、`EndpointMap/EndpointState`→`RemoteMap/RemoteState`(#3673)。

> **libp2p 对照**：libp2p 把 mDNS、Kademlia、Identify 都作为 Behaviour 放在主 repo 内、组合进同一个 Swarm；iroh 把 mDNS/DHT 拆到独立 crate，**核心只保留 pkarr + DNS 两条中心化通路**。**要局域网发现（对应 libp2p 的 mDNS behaviour），必须额外加 `iroh-mdns-address-lookup` 依赖，不是开 feature。**

## trait 本身极轻

```rust
// address_lookup.rs:333-350 —— 两个方法都有默认空实现
pub trait AddressLookup: std::fmt::Debug + Send + Sync + 'static {
    fn publish(&self, _data: &EndpointData) {}
    fn resolve(&self, _endpoint_id: EndpointId) -> Option<BoxStream<Result<Item, Error>>> { None }
}
```

自己写一个成本很低 —— 这一点在下面「无优先级」那条里会变成一个真实的可行方案。

### 发布/解析职责分离

```rust
// pkarr.rs:357-361 —— 只有 publish
impl AddressLookup for PkarrPublisher {
    fn publish(&self, data: &EndpointData) { self.update_endpoint_data(data); }
}
// dns.rs:113-117 —— 只有 resolve
impl AddressLookup for DnsAddressLookup { fn resolve(...) -> Option<...> { ... } }
// memory.rs:219 —— 显式空实现：MemoryLookup 明确不发布任何东西
fn publish(&self, _data: &EndpointData) {}
```

**所以「只实现一半」是合法且常规的。要「既能发又能被找到」必须同时注册 Publisher 和 Resolver——只加 `DnsAddressLookup` 你能查别人，别人查不到你**（DNS 侧没有发布通道，发布只能走 pkarr）。

> libp2p Kademlia 一个 Behaviour 同时承担 put_record/get_record 与路由；iroh 的发布面与查询面是两个互不相干的对象，甚至走不同协议（HTTP PUT vs DNS UDP）。

## iroh-mdns-address-lookup

- **成熟度**：**beta**（0.4.0，HEAD 2026-07-10，依赖 iroh 1.0.0）
- **降级依据**：(1) 0.4.0 pre-1.0 —— cargo 语义下 minor bump 即 breaking，**测试好 ≠ API 稳**；(2) 核心功能**全压在 alpha 上** —— 依赖 `swarm-discovery = "0.6"`，而 `swarm-discovery/Cargo.toml:3` 是 `0.6.0-alpha.2`，第三方作者（rkuhn），最后提交 2026-04-15
- **测试反而扎实**：6 个 tokio 测试全部**未被 ignore**（`mdns_publish_resolve`:613 / `mdns_publish_expire`:678 / `mdns_subscribe`:735 / `non_advertising_endpoint_not_discovered`:784 / `test_service_names`:818 / `mdns_publish_relay_url`:878）
- **入口**：`iroh-address-lookups/iroh-mdns-address-lookup/src/lib.rs`
- **独有能力**：除了 `resolve(EndpointId)` 主动查，还提供 `subscribe()` 拿**被动发现事件流**（`Discovered` / `Expired`）—— DHT 和 DNS 都没有这个

**广播内容**：EndpointId 用 base32 小写编码当服务实例名（`<base32-id>._irohv1._udp.local`），IP 走 A/AAAA + SRV，relay URL 和 user_data 走 TXT 属性。

**⚠️ 注册那一步不能漏**（README 原文顺序）：

```rust
let endpoint = Endpoint::bind(presets::Minimal).await.unwrap();
let mdns = MdnsAddressLookup::builder().build(endpoint.id()).unwrap();
endpoint.address_lookup().unwrap().add(mdns.clone());   // ← 没有这行，mdns 根本没挂到 endpoint 上
let mut events = mdns.subscribe().await;
```

**这是从 libp2p 迁移最容易漏的一项**（libp2p 里 mDNS 是内置 behaviour）。

**两种装法都合法**：
- builder 链式**是有效的** —— `impl AddressLookupBuilder for MdnsAddressLookupBuilder`（lib.rs:225-231）内部就是 `self.build(endpoint.id())`，框架会把 Endpoint 递给 builder
- 运行期 `.add()` 的真实理由是**你需要一个具体 handle 才能后续调 `.subscribe()`** —— 这正是 crate 自己的 doc example（lib.rs:21-22）那么写的原因

> ⚠️ 常见误传：「build() 需要 endpoint.id()，所以不能链式」—— **错的**。需要 endpoint.id() 不构成避开链式的理由。

**何时不用**：浏览器/wasm 不可用（UDP 多播）；跨网段无效；公共 WiFi 隐私敏感（见下）；企业 WiFi / AP 隔离会丢多播包；移动端权限是硬门槛（见下）。

## iroh-mainline-address-lookup

- **成熟度**：**beta**（0.4.0，HEAD 2026-07-10，依赖 iroh 1.0.0；被 iroh 核心文档正式指引 `address_lookup.rs:46-51`）
- **降级依据**：唯一的集成测试 `dht_address_lookup_smoke`（lib.rs:374）带 `#[ignore = "flaky"]`（lib.rs:372）—— crate 内**没有其他 tokio 测试**，即**整条 DHT publish/resolve 路径零常态 CI 覆盖**；README 无任何生产背书
- **行为**：把寻址信息以 pkarr 签名包形式存进 BitTorrent Mainline DHT（BEP44 mutable item）。默认只发布 relay 地址（不泄 IP）、默认 client 模式（不当 DHT server）
- **关键常量**（lib.rs:28-41）：`DEFAULT_PKARR_TTL = 30`（秒）、`REPUBLISH_DELAY = 3600`（秒，内容不变时每小时重发）、`PUBLISH_DEBOUNCE_DELAY = 50ms`

**何时不用**：
1. 浏览器/wasm **完全不可用**（全仓 0 处 wasm cfg）
2. 在意 IP 隐私时 —— `relay_only` 只挡「记录里写 IP」，挡不住「源 IP 暴露给 bootstrap 和沿途节点」
3. 要求低延迟首连（DHT 迭代查询是秒级，mDNS/DNS 是毫秒级）
4. 移动端常驻会持续 UDP 收发 + 每小时 republish，耗电与后台存活是问题

### DHT 发布不做 CAS

`iroh-mainline-address-lookup/src/lib.rs:264` 注释：*"We publish without CAS. We assume a single logical writer per endpoint key."*，对应 :295 `put_mutable(item.clone(), None)`（第二参 cas 传 None）。seq 直接取 pkarr 包的**本地时间戳微秒数**（lib.rs:44-51）。

n0-mainline 侧其实**支持** CAS（`n0-mainline/src/dht.rs:492-495`），且其文档警告 "Lost Update Problem"（dht.rs:**444**）与 ConcurrencyError（:487）—— 即 iroh 是**主动选择不用**。

**含义**：只要坚持「一台设备一个 EndpointId」，无影响。但若设想过「同一身份多端登录」（同一 keypair 在手机和桌面同时跑），DHT 记录会被两端互相覆盖、来回抖动，且因 seq 取本地时钟微秒，**时钟不同步的两台设备会打架**（seq 大的赢）。**这是跨设备同步身份的明确禁区信号**（另见 [relay.md](relay.md) 的「同一 EndpointId 双连接互顶」）。

## MemoryLookup：带外地址的官方通路

```
// memory.rs:1-11
//! An in-memory address lookup system to manually add endpoint addressing information.
//!
//! Often an application might get endpoint addressing information out-of-band in an
//! application-specific way.  [`EndpointTicket`]'s are one common way used to achieve this.
//! This addressing information is often only usable for a limited time so needs to
//! be able to be removed again once you know it is no longer useful.
```

**`MemoryLookup` 只有 resolve 侧意义，它不会把地址发布到任何地方**（`memory.rs:219` 的 `fn publish` 是显式空实现）。

`Item::provenance()` 返回的静态字符串（`"pkarr"` / `"dns"` / `"memory_lookup"`，后者见 `memory.rs:102`）是区分结果来源的唯一手段。

> 对应 libp2p 的 `Swarm::add_peer_address` / Kademlia `add_address`；差别是 iroh 把它做成了一个**平等的 AddressLookup 实现**，与 pkarr/dns 走同一条 resolve 合流管道，而不是塞进全局路由表。

## swarm-discovery（mDNS 的底层引擎）

- **成熟度**：**experimental**（`0.6.0-alpha.2`，Cargo.toml:3；依赖 `hickory-proto = "=0.26.0-beta.4"` 精确 pin 在 beta；HEAD 2026-04-15；`git tag` 为空）
- **是什么**：原作者 Roland Kuhn，本地 clone 是 n0 的 fork。**不是普通 mDNS** —— 它按算法自适应控制查询/响应频率（参数 τ 发现时间目标、φ 响应频率目标），使**带宽不随 swarm 规模膨胀**，同时维持稳定包流当 liveness 信号（这是 `Expired` 事件能工作的基础）

> ⚠️ **本地 clone 不是编译进产物的那份**。`iroh-address-lookups/Cargo.lock` 把 `swarm-discovery` 解析到 **0.6.1**（`source = "registry+..."`，checksum `36ae41d2...`）。本地这份 0.6.0-alpha.2 按 cargo 语义（`^0.6` 默认排除 pre-release）**不满足约束**。读本地源码排障时别默认它就是线上跑的那份。
>
> 同一份 Cargo.lock 也确认：n0-mainline → 0.5.0、iroh → 1.0.0，均来自 crates.io。

---

# 第二部分：pkarr 的机制

## 默认目标是 n0 的中心服务

```rust
// address_lookup/pkarr.rs:127 / :134
pub const N0_DNS_PKARR_RELAY_PROD: &str    = "https://dns.iroh.link/pkarr";
pub const N0_DNS_PKARR_RELAY_STAGING: &str = "https://staging-dns.iroh.link/pkarr";

// iroh-dns 的 dns.rs:45-47
pub const N0_DNS_ENDPOINT_ORIGIN_PROD: &str    = "dns.iroh.link.";
pub const N0_DNS_ENDPOINT_ORIGIN_STAGING: &str = "staging-dns.iroh.link.";
```

`pkarr.rs:121-123` 的 doc：

> This server is both a pkarr relay server as well as a DNS resolver ... **However it does not interact with the Mainline DHT, so is a more central service.**

**iroh 客户端默认路径上完全没有 DHT**，这一点独立核实过：`grep -rniE "kademlia|k-bucket|kbucket|routing_table"` 在 `iroh/src/` 只命中 `endpoint.rs:397` / `endpoint/bind.rs:13,25` 三处，且均指**操作系统 IP 路由表**（用于选 socket 绑定），与 DHT 无关；`grep -rni "bootstrap" iroh/src/` **零命中**。

> ⚠️ **但「不与 Mainline DHT 交互」是 doc comment，描述的是 n0 那台部署实例的配置，不是软件能力**：本仓 `iroh-dns-server` **支持 mainline DHT fallback**（`Cargo.toml:38` `mainline = "7"`；`src/store.rs:11` `use mainline::{Dht, DhtBuilder, MutableItem};`；`src/lib.rs:8-9`「With the mainline fallback enabled, keys missing from the local store are looked up on the BitTorrent mainline DHT」；`config.rs:53` `pub mainline: Option<MainlineConfig>`，Default 里 `config.rs:286` `mainline: None` **默认关闭**）。准确说法是「**n0 服务器按其文档不接 DHT，且服务端 DHT fallback 默认关闭**」——是配置决定，不是架构上不可能。
>
> `pkarr.rs:9-20` 明确写了通用 pkarr relay「will usually perform the publishing to the Mainline DHT on behalf on the client」——**n0 是刻意的例外**。

> **这是与 libp2p 最大的心智落差**：libp2p Kademlia 默认是去中心的（bootstrap 后自组织路由表）；iroh 默认把「谁在哪」的解析交给 n0 的一台中心服务器，没有 DHT 参与。想真去中心要自己加 `iroh-mainline-address-lookup`。

## 关于 BEP44：字面零出现，但机制在服务端

`grep -rni "bep44|bep 44|bep-44" --include="*.rs" --include="*.toml" --include="*.md" .` **全树 0 命中**。

**但字面零出现 ≠ BEP44 机制不在仓内**：`mainline` crate 的 `MutableItem`（`iroh-dns-server/src/store.rs:198-200`、`lib.rs:308`）**就是 BEP44 mutable item**，只是没写这个词。

**正确表述**：iroh 自己不实现 BEP44、**客户端默认路径一步不沾 DHT**；BEP44 语义经 `mainline` crate 出现在**服务端可选 fallback** 里（默认关闭）。客户端侧的 Mainline lookup 实现（`iroh-mainline-address-lookup`）在仓外。

> **「iroh 用 BEP44 把记录存进 BitTorrent Mainline DHT」这个说法对默认配置是错的**：默认走 HTTP PUT 到 n0 服务器。

## pkarr 签名包：iroh 自己实现，不依赖 pkarr crate

`grep '^name = "pkarr"' iroh/Cargo.lock` → **NOT IN LOCKFILE**。格式由 `iroh-dns` 基于 `simple-dns` 自己实现（`iroh-dns/Cargo.toml:24` `simple-dns = "0.11"`）：

```
// iroh-dns/src/pkarr.rs:1-3
//! Implements the [pkarr] signed DNS packet format: `<32 pubkey><64 sig><8 timestamp><DNS packet>`.

// pkarr.rs:17-24
const MAX_DNS_PACKET_SIZE: usize = 1000;
pub const MAX_SIGNED_PACKET_SIZE: usize = HEADER_SIZE + MAX_DNS_PACKET_SIZE;
// :30 doc: The DNS packet must be at most 1000 bytes. Total max size is 1104 bytes.
```

**1104 字节是硬上限**，超了 `SignedPacketBuildError::PacketTooLarge`（`pkarr.rs:72-77`）。**地址多时会顶到上限**——这正是 `EndpointData::new` 文档说「地址顺序被保留，可为 lookup 服务编码优先级，以防塞不进单个 DNS 包」的原因（`endpoint_info.rs:86-89`）。

> libp2p 记录是 protobuf、无 DNS 包尺寸约束；iroh 因为要兼容真实 DNS TXT 传输，被 DNS 包大小卡死，地址集大时必须靠 AddrFilter 排序取舍。

## 发布/解析就是两个 HTTP 动作

```rust
// pkarr.rs:657-673 —— publish
let mut url = self.pkarr_relay_url.clone();
url.path_segments_mut()...push(&signed_packet.public_key().to_z32());
let response = self.http_client.put(url).body(signed_packet.to_relay_payload()).send().await

// pkarr.rs:620-652 —— resolve
url.path_segments_mut()...push(&endpoint_id.to_z32());
let response = self.http_client.get(url).send().await...;
let packet = SignedPacket::from_relay_payload(&endpoint_id, &payload)
    .map_err(|err| e!(PkarrError::Verify, err))?;

// iroh-dns/src/pkarr.rs:106-109 —— 验签
.verify(&signable(timestamp, encoded_packet), &signature)
    .map_err(|e| e!(SignedPacketVerifyError::SignatureError, e))?;
```

**GET 无任何鉴权：只要知道对方 EndpointId（= 公钥），任何人任何时候都能拉到完整记录。服务器不可伪造内容（有签名），但可以审查/丢弃/观测查询者。**

> libp2p Kad 的 GET_VALUE 要在 DHT 上做 O(log n) 次迭代查询、并从多个节点收敛；iroh 是一次 HTTP GET 拿到签名包，**延迟低且确定，但可用性完全系于那台服务器**。

## DNS 侧：`_iroh.<z32>.<origin>` 的 TXT

```
// iroh/iroh/src/address_lookup/dns.rs:24-38
/// It uses the [`Endpoint`]'s DNS resolver to query for `TXT` records under the domain
/// `_iroh.<z32-endpoint-id>.<origin-domain>`
/// ... The supported attributes are: * `relay=<url>`: The URL of the home relay server
```

> ⚠️ **dns.rs 的 doc 只列了 `relay=` 一个属性。** 三个属性的真正依据在 iroh-dns：

```rust
// iroh-dns/src/attrs.rs:20
pub const IROH_TXT_NAME: &str = "_iroh";

// iroh-dns/src/attrs.rs:78-89 —— kebab-case 正是 UserData → user-data 的来源
#[strum(serialize_all = "kebab-case")]
pub(crate) enum IrohAttr { Relay, Addr, UserData }

// iroh-dns/src/endpoint_info.rs:486-500 —— 编码点
TransportAddr::Relay(url)  => attrs.push((IrohAttr::Relay, url.to_string())),
TransportAddr::Ip(addr)    => attrs.push((IrohAttr::Addr, addr.to_string())),
TransportAddr::Custom(addr)=> attrs.push((IrohAttr::Addr, addr.to_string())),   // :492 —— 与 Ip 共用 addr=
_ => {}                                                                        // :493 兜底
if let Some(user_data) = &info.data.user_data { attrs.push((IrohAttr::UserData, ...)) }
```

即：`relay=<url>` / `addr=<...>`（**Ip 与 Custom 共用**）/ `user-data=<...>`。

**staggered 并发而非单发**（`dns.rs:18-22`）：

```rust
const DNS_STAGGERING_MS: &[u64] = &[200, 300, 600, 1000, 2000, 3000];
// doc: 每次查询自身超时 3s，因此整体最迟 6 秒放弃
```

**这个 6s 上限是首连失败时的关键时序常数。**

## EndpointId / EndpointAddr

```rust
// iroh-base/src/key.rs:70
pub type EndpointId = PublicKey;        // ← 就是 32 字节 Ed25519 公钥本身，不是哈希

// iroh-base/src/endpoint_addr.rs:41-47
pub struct EndpointAddr { pub id: EndpointId, pub addrs: BTreeSet<TransportAddr> }

// endpoint_addr.rs:54-62
pub enum TransportAddr { Relay(RelayUrl), Ip(SocketAddr), Custom(CustomAddr) }   // #[non_exhaustive]

// endpoint_addr.rs:155-159 —— 只有 id 就能 connect
impl From<EndpointId> for EndpointAddr { fn from(id: EndpointId) -> Self { EndpointAddr::new(id) } }
```

**`TransportAddr` 标了 `#[non_exhaustive]`**，match 必须带兜底分支（iroh 自己在 `endpoint_info.rs:493` 就写了 `_ => {}`）。

> **libp2p 对照**：libp2p PeerId 是公钥的 **multihash**（大公钥需先取哈希，要拿回公钥得靠 Identify）；iroh 的 EndpointId **直接就是 32 字节 Ed25519 公钥**，可直接验签 pkarr 记录、**无需额外交换公钥这一步**。Multiaddr（含协议栈语义）↔ iroh 扁平的 TransportAddr 枚举也是明显落差。

---

# 第三部分：隐私分析（本文最重要的一节）

## 1. iroh 的记录里根本没有 hostname 字段

`enum IrohAttr`（`iroh/iroh-dns/src/attrs.rs:82-89`）**只有三个变体**：`Relay` / `Addr` / `UserData`。所以「向公共 DHT 广播主机名」这类泄露在 iroh 模型下**不可能由 iroh 自己引入**。

`grep -rn "hostname|host_name|gethostname|whoami|username|device_name"` 覆盖 `iroh-dns/src/` 与 `iroh/src/address_lookup*`：**发布路径零命中**（仅 dns.rs 两处是「解析 URL 里的主机名」，与发布无关）。

mDNS 侧同理：SRV 的 target 是 `{base32-id}-{port}.local.`（`swarm-discovery/src/sender.rs:181`），**不是 `gethostname()`** —— 对 swarm-discovery 全仓 grep `hostname|gethostname|host_name` **零命中**。

**结论**：泄露面被收窄到唯一一个你完全可控的字段 —— `user_data`。

> **libp2p 对照**：libp2p 的 **Identify 协议默认就会向每个连上的 peer 广播 agent_version / protocol_version / listen_addrs**（很多项目正是在这里泄露主机名或内网地址）；iroh **没有 Identify 等价物**，元数据面默认是空的——**默认更保守**。

## 2. user_data 是 endpoint 全局的，且 AddrFilter 永远剥不掉它

```rust
// iroh-dns/src/endpoint_info.rs:70-76 —— 可发布字段只有两个
pub struct EndpointData {
    addrs: Vec<TransportAddr>,
    user_data: Option<UserData>,
}
// iroh/iroh/src/endpoint.rs:205
address_lookup_user_data: Default::default(),   // == None

// endpoint.rs:631-642 / :1661 —— 唯一注入点，需显式调用
pub fn user_data_for_address_lookup(mut self, user_data: UserData) -> Self { ... }
pub fn set_user_data_for_address_lookup(&self, user_data: Option<UserData>)
```

类型 `pub struct UserData(String)` 上限 **245 字节**（`endpoint_info.rs:314`），文档明说 *"Iroh does not keep track of or examine the user-defined data"*。

**关键**：`EndpointData::apply_filter`（`endpoint_info.rs:189-199`）在过滤后**显式把 user_data 重新挂回**：

```rust
pub fn apply_filter(&self, filter: &AddrFilter) -> Cow<'_, Self> {
    match self.filtered_addrs(filter) {
        Cow::Borrowed(_) => Cow::Borrowed(self),
        Cow::Owned(addrs) => {
            let mut data = EndpointData::new(addrs);
            data.set_user_data(self.user_data.clone());   // ← :195 user_data 被原样带过去
            Cow::Owned(data)
        }
    }
}

// endpoint_info.rs:229-230 —— filter 的签名里根本看不到 user_data
type AddrFilterFn = dyn Fn(&Vec<TransportAddr>) -> Cow<'_, Vec<TransportAddr>> + Send + Sync + 'static;
```

> **AddrFilter 在任何层都不可能剥掉 user_data。** 这是最容易误判的一条：以为「加了 relay_only 就安全了」。真相是 filter 的函数签名里根本没有 user_data，无从过滤。
>
> **要不发布 user_data，唯一办法是不设它（默认即不设）或 `set_user_data_for_address_lookup(None)`。防线在 user_data 那一侧，不在 AddrFilter。**
>
> 想做到「mDNS 广播设备名、DHT 不广播」，唯一办法是**自己包一层 AddressLookup** —— 这不是推断，是源码级事实。

**任何往 user_data 里塞可识别信息（设备名/用户名/机器码）的代码，都会被公开发布到 dns.iroh.link 且全球可无鉴权 GET。这是唯一需要 code review 卡住的 API。**

## 3. AddrFilter 是两层 AND，`Builder::addr_filter` 打不开 service 内建的过滤

- **第一层**（endpoint 级）：`AddressLookupServices::publish`（`address_lookup.rs:517-521`）先 `data.apply_filter(...)`。**endpoint 级默认是 `None` = 不过滤**（`endpoint.rs:206` `addr_filter: None`）
- **第二层**（service 级）：各 service 再用自己的 filter 过一遍 —— `PkarrPublisher`（`pkarr.rs:350-351`，默认 `AddrFilter::relay_only()` 见 :168）、DHT（lib.rs:323，默认 `relay_only()` 见 :169）、mDNS（lib.rs:310，默认 `AddrFilter::default()` 见 **:173**）

两层都是「只减不增」语义，故合成 **AND**。

**后果**：想让有公网 IP 的桌面端发布 IP 求直连，改 `Builder::addr_filter(AddrFilter::unfiltered())` 是**无效的**，会白白 debug 很久。正确写法是在 service 自己的 builder 上设：

```rust
PkarrPublisher::n0_dns().addr_filter(AddrFilter::unfiltered())
DhtAddressLookup::builder().addr_filter(AddrFilter::unfiltered())
```

官方 example 就是这么写的（`iroh-mainline-address-lookup/examples/dht_address_lookup.rs:33`，文件头注释「this example explicitly removes the filter to publish all addresses」）。

注：`Builder::clear_addr_filter`（`endpoint.rs:626-629`）文档称可清除「including filters set by presets」，但它清的是 **endpoint 级那一层**，对 service 内建 filter 无效。

> ⚠️ **`PkarrPublisher` 默认 relay_only 的归属别搞错**：默认设在 **`PkarrPublisherBuilder::new`**（`pkarr.rs:163`，filter 在 `:168`），**不是 `PkarrPublisher::new`**——后者在 `pkarr.rs:298-330`，**私有且 filter 必须显式传入，本身没有默认值**。默认之所以是铁的，恰恰因为**唯一的公开入口** `PkarrPublisher::builder()`(:290) / `n0_dns()`(:332) 都强制经过 builder，外部无法绕过。**所以别以为「builder 没设 filter = 会发 IP」——Publisher 自己会兜住。**

## 4. ⚠️ mDNS 与 DHT 的默认过滤策略**完全相反**

| | 默认 filter | 后果 |
|---|---|---|
| **DHT** | `AddrFilter::relay_only()`（lib.rs:169，注释明写 *"This avoids leaking IP addresses to the public DHT"*） | 不泄 IP |
| **mDNS** | `AddrFilter::default()`（lib.rs:**173**）| **不过滤** —— 广播全部本地 IP + relay + user_data |

`AddrFilter::default()` 就是恒等过滤器：`#[derive(Clone, Default)] pub struct AddrFilter(Option<Arc<AddrFilterFn>>)`（`endpoint_info.rs:242-243`），Default 即 `None`，apply 走 `None => Cow::Borrowed(addrs)` 原样返回（:279-284），Debug 甚至直接打印 `"identity"`（:250）。mDNS 模块文档也自认（lib.rs:42-43）*"By default, MdnsAddressLookup publishes all addresses it receives: direct IP addresses and up to one RelayUrl"*。

**同时装两个时，你以为设过 filter 了，其实只有一半生效。**

**公共 WiFi 上，默认配置下链路内任何人被动嗅 5353 就能收集：EndpointId + 全部内网 IP + relay URL + user_data。** 局域网直连本来就需要 IP，所以不能无脑 `relay_only()` —— 缓解手段是 `AddrFilter::ip_only()` + `service_name("你的应用名")` 隔离 + user_data 留空。

> **libp2p 对照**：libp2p Kademlia 的 provider/peer record 会把 listen_addrs（常含 192.168.\*/10.\* 内网地址）**原样进 DHT**，且没有内建的「只发 relay 不发 IP」开关；iroh 把地址发布做成一等公民的可插拔 `AddrFilter`，且默认最小化。

## 5. DHT 键 = SHA1(EndpointId)，无 salt、无 ACL，记录只签名不加密

键的算法（`n0-mainline/src/common/mutable.rs:46-58`）：

```rust
pub fn target_from_key(public_key: &[u8; 32], salt: Option<&[u8]>) -> Id {
    let mut encoded = vec![]; encoded.extend(public_key);
    if let Some(salt) = salt { encoded.extend(salt); }
    let mut hasher = Sha1::new(); hasher.update(&encoded); /* ... */
}
```

iroh 侧调用时 **salt 恒为 None**（查：`iroh-mainline-address-lookup/src/lib.rs:116`；发布：lib.rs:44-52，最后一参亦 None，且传的 `packet.encoded_packet()` 是**明文 DNS 包**）。

**含义**：任何知道你 EndpointId 的人都能算出 target 并查到你的 relay（乃至 IP，若 unfiltered）。**EndpointId 本身就是一个长期有效的定位能力（bearer capability）**。

若你的产品是配对模型（双方长期持有对方 EndpointId），这意味着**一次配对 = 永久授予对方（以及任何窃取到该 ID 的人）定位你的能力**，只要你开着 DHT 发布（每小时 republish，**窗口是永久**）。**缓解手段**：只在「可被发现」开关打开时才 add DHT lookup。

## 6. AddrFilter 挡不住「你的 IP 暴露给谁」

DHT publish/lookup 是**裸 UDP**（`n0-mainline/Cargo.toml` 依赖 `noq-udp` + tokio `net`）。默认 bootstrap 是硬编码的公共 BT 基础设施（`n0-mainline/src/actor/config.rs:3-8`）：

```rust
pub const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881", "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401", "relay.pkarr.org:6881",
];
```

所以源 IP 必然暴露给这 4 个节点 + 迭代查询沿途所有节点；lookup 还额外泄露「你在找哪个 EndpointId」。BEP42 的存在（`n0-mainline/src/common/id.rs:84-107`，DHT 节点 ID 由 IP 派生）进一步说明这一层与 IP 强绑定。

**「E2E 加密」容易被理解成「没人知道我在跟谁传」—— 开 DHT 后这条不成立。** 观察者（跑几个 DHT 节点即可，成本极低）能看到「IP a.b.c.d 在查 EndpointId X」，交集分析可还原社交图谱。

相比之下 pkarr 只把这些暴露给 n0 一家（走 HTTPS）—— **不是「DHT 更私密」，而是「换了个信任对象」**。

---

# 第四部分：组合与运行时行为

## 无优先级 —— 全部并发，先到先得

装载是 push 进 Vec（`iroh/iroh/src/endpoint.rs:605-608`），可多次调用。resolve 时对所有 service 调 `resolve` 后 `MergeBounded::from_iter(streams)`（`address_lookup.rs:553-566` / :599-606）。文档明写：

> *"All services are queried concurrently and their results are merged into a single stream. Each Item is yielded as Ok(Ok(item)) as soon as it is produced, allowing the caller to act on the first usable address while slower services are still working."*

publish 则**无条件广播给所有 service**（:517-531 `for service in &*services { service.publish(&data); }`）。

错误语义（:609-649）：单个 service 失败只 inline 产出 `Ok(Err(error))` 不终止流；全部结束且无一 Item 才产出 `Err(AddressLookupFailed::NoResults { errors })`；有过结果则丢弃 buffered errors 正常结束。

**好消息**：「局域网优先、跨网兜底」不需要自己编排 —— 同时装 mDNS + pkarr(+DHT)，mDNS 毫秒级天然先返回。

**坏消息**：你无法表达「有 mDNS 结果就别查 DHT」—— DHT 查询照发，隐私代价照付。真要做条件查询只能自己包一层 AddressLookup（trait 只有两个方法，成本不高）。

## 解析行为：惰性 + 短路 + 合流

```rust
// iroh/iroh/src/socket/remote_map/remote_state.rs:862-880
/// Does not start Address Lookup if we have a selected path or if Address Lookup is
/// currently running.
fn trigger_address_lookup(&mut self) {
    if self.selected_path.is_some() || self.address_lookup_stream.is_some() { return; }
    let stream = self.address_lookup.resolve(self.endpoint_id);
    ...
}
```

- 全服务并发、先到先用、单服务失败不中断
- **未配置任何服务时不是静默成功**，而是 stream 立刻吐 `AddressLookupFailed::NoServiceConfigured`（`:620-626`）；`remote_state.rs:896-898` 对它**只打 trace 不打 debug**。**用 `presets::Empty` 或 `clear_address_lookup()` 后忘了加服务，表现就是「connect 只认显式地址」**

> libp2p Kad 查询一旦发起会跑完整个迭代流程并维护路由表（持续后台流量）；iroh 的 lookup 是**一次性、需求驱动**的，拿到可用路径立刻停，**没有任何后台拓扑维护**。

## 发布行为：fire-and-forget + 定期重发

```rust
// pkarr.rs:136-146
pub const DEFAULT_PKARR_TTL: u32 = 30;                                   // 秒
pub const DEFAULT_REPUBLISH_INTERVAL: Duration = Duration::from_secs(60 * 5);   // 5 分钟

// pkarr.rs:386-405 —— 线性退避
Err(err) => { failed_attempts += 1; let retry_after = Duration::from_secs(failed_attempts); ... }

// address_lookup.rs:334-341 —— trait 契约
/// This is fire and forget, since the [`Endpoint`] can not wait for successful
/// publishing. If publishing is async, the implementation should start its own task.
```

**`publish()` 无返回值也不报错——发布失败只体现为 `warn!` 日志（`pkarr.rs:391-397`），应用层拿不到任何信号。想确认「我已可被发现」只能自己去 GET 一次。**

> `DEFAULT_PKARR_TTL` 的注释自己都带着 `// TODO(flub): huh?`（`pkarr.rs:142`），且说明 iroh-dns-server 会忽略该 TTL、走 relay 时也忽略——**这个常数实际影响面很小，别过度强调**。

> libp2p Kad 的 put_record 有 Quorum 语义、能通过事件拿到成功/失败与副本数；iroh 的 publish 是**彻底的单向 fire-and-forget**，没有 quorum 概念（因为只有一个服务器）。

## 运行时可增删 + 新服务立刻补发

```rust
// address_lookup.rs:487-498
/// If there is historical Address Lookup data, it will be published immediately on this service.
pub fn add_boxed(&self, service: Box<dyn AddressLookup>) {
    {
        let data = self.last_data.read().expect("poisoned");
        if let Some(data) = &*data { service.publish(data) }
    }
    self.services.write().expect("poisoned").push(service);
}
```

补发用的 `last_data` 是「**已经过 endpoint 级 addr_filter 过滤后**」的数据（publish 里先 filter 再存，`:517-530`）——所以后加服务拿到的是过滤后的快照，语义一致，不会因为晚加入而绕过 endpoint 级过滤。

> libp2p 的 Behaviour 在 Swarm 构建后基本不可增删（需 Toggle/自定义组合）；iroh 的 lookup 服务集合是运行时可变的 Vec。

## mDNS 的被动模式与 subscribe/resolve 分流

- **`advertise(false)`**（lib.rs:177-183，默认 true 见 :171）→ `publish` 直接空转（:589-593）。**「我能发现别人，别人发现不了我」的隐身模式，零成本**。有测试覆盖：`non_advertising_endpoint_not_discovered`(:784-814)
- **resolve 有 10 秒硬超时**：`const LOOKUP_DURATION: Duration = Duration::from_secs(10);`（:94-95）
- ⚠️ **`subscribe()` 只推送被动发现的设备**：被 `resolve()` 显式解析到的**不会**推给 subscriber（:399-407 注释 *"only send endpoints to the `subscriber` if they weren't explicitly resolved"*，对应 `if !resolved { subscribers.send(...) }`）

**坑**：如果既用 `subscribe()` 维护「附近设备」列表、又对已知设备调 `endpoint.connect()`（内部触发 resolve），那么**这些设备可能不会出现在 subscribe 流里**，UI 列表缺项。**写列表时要合并两路来源。**

---

# 第五部分：平台边界

## 浏览器 / wasm

**DHT 与 mDNS 完全不可用** —— 不是降级，是编译/运行都没有。

- 正面：内置 pkarr 模块处处有 wasm 分支（`pkarr.rs:310/313`、:458/461 多处 `#[cfg(wasm_browser)]` 成对出现）—— 被刻意支持
- `DnsAddressLookup` 整模块被排除：`address_lookup.rs:120-126` `#[cfg(not(wasm_browser))] pub mod dns;`；presets 里同样跳过（`presets.rs:131-134`）
- 反面：对 `iroh-address-lookups` 全仓 grep `wasm|target_arch`（含 `*.rs` 与 `*.toml`）**零结果**；对 `swarm-discovery` 的 src/ 与 Cargo.toml grep `wasm` 亦**零结果** —— 两者根本没考虑过 wasm，底层分别是 noq-udp 与 socket2 UDP 多播

**Web 端必须依赖 pkarr relay**（n0 的 dns.iroh.link 或自建）。这意味着「无服务器」卖点在 Web 端天然打折 —— 桌面/移动可纯 DHT，Web 不行。**要么三端统一用 pkarr，要么接受三端能力不对等并在 UI 讲清楚 —— 这是产品决策，不只是技术细节。**

## ⚠️ 移动端 multicast 权限 —— 生态空白区

> **在全部 24 个仓库中均无任何文档或代码痕迹 —— 未找到。**

对 `/Volumes/yexiyue/iroh-study` 全目录 grep（`--include=*.rs,*.md,*.toml,*.xml,*.plist,*.kt,*.java,*.swift`）关键词 `multicast-networking|MulticastLock|CHANGE_WIFI_MULTICAST|com.apple.developer.networking.multicast|multicast entitlement` —— **零命中**。iroh-ffi / iroh-js 中 grep `mdns` 亦零命中（**官方 FFI 绑定压根不暴露 mDNS**）。iroh 主仓 CHANGELOG 里 mdns 相关条目只涉及 service_name、passive mode、expiry events，**无任何平台权限说明**。

客观事实：swarm-discovery 走 socket2 的 `join_multicast_v4`/`join_multicast_v6`（`swarm-discovery/src/socket.rs:139` 与 :200）监听 224.0.0.251 与 ff02::fb 的 5353 端口（:12-14），因此 **OS 层多播限制必然适用** —— 只是 n0 未记录。

**两个直接后果**：
1. **官方 FFI 不含 mDNS** → 要在 uniffi 桥里暴露局域网发现**必须自己写绑定，无先例可抄**。（对照组：`iroh-c-ffi/src/endpoint.rs:55` 的 `pub enum DiscoveryConfig { None, DNS, Mdns, All }` —— **C 绑定反而做到了**，可作为「该怎么暴露」的形状参考）
2. **iOS multicast entitlement 需向 Apple 单独申请**（非自助开关）、**Android 需 `CHANGE_WIFI_MULTICAST_STATE` + 运行期 `MulticastLock`** —— 都得自行验证

**「iOS/Android 真机 mDNS 能否收到包」是 iroh 生态里风险最高、信息最少的一块。**
