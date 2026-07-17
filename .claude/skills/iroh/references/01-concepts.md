# Concepts：iroh 的心智模型

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Concepts](https://docs.iroh.computer/concepts/) 分区：Endpoints / Relays / Address Lookup /
NAT Traversal / Protocols / Tickets。

> **本文只回答「这是什么」，不回答「怎么写」。** 建 Endpoint / 挂发现服务 / 拒绝入站 → [02-connecting.md](02-connecting.md)；
> 流与协议 → [03-protocols.md](03-protocols.md)；自建 relay 的配置 → [07-configuration.md](07-configuration.md)。
>
> 从 libp2p 迁过来的，先读 [SKILL.md](../SKILL.md) 的「与 libp2p 的心智差异」10 条。

---

# 1. Endpoints —— 身份就是公钥

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

# 2. Protocols —— 一条连接终身绑一个 ALPN

**iroh 的「协议」不是一个 trait 树，是一个 ALPN 字节串。**

- ALPN 在 **QUIC/TLS 握手时协商一次**，之后这条连接终身绑定它 → 一个 `ProtocolHandler`。
- 匹配是**整串字节精确相等**：没有前缀匹配、没有通配、没有版本协商回退。
- 要在一条连接里同时跑两个协议是**做不到的** —— 得开两条 Connection（复用同一个 Endpoint 与同一次打洞的路径）。
- 你拿到的是**裸双向流**：framing / 序列化 / 超时 / 请求关联全部自理，**没有 Codec**。

> **与 libp2p 最本质的区别**：libp2p 用 multistream-select 在**每条 substream** 上协商协议名，
> 一条连接可以并发跑 identify + kad + gossipsub + 自定义协议。这也是「流为什么在 libp2p 里贵」的第二个原因
> —— 每条流一次协商往返。**所以 libp2p 里「合并多个逻辑通道到一条流」是优化，iroh 里这么做是反优化。**

机制、Router 分发、ALPN 命名与版本策略 → [03b-writing-a-protocol.md](03b-writing-a-protocol.md)。

---

# 3. Relays

## relay 不是「配置一个中转服务器」，是一条与 IP 并列的 transport

`RelayTransport` 实现 `poll_recv`/`poll_send`，**QUIC 直接跑在它上面**。这是理解 iroh relay 的起点。

```rust
// iroh/iroh/src/socket/transports.rs:99-118
pub(crate) enum TransportConfig {
    #[cfg(not(wasm_browser))]
    Ip { config: ip::Config, is_user_defined: bool },
    Relay { relay_map: RelayMap, is_user_defined: bool },
    Custom(Arc<dyn CustomTransport>),
}
```

它**不是「转发 TCP 流」的东西**。它只在两个已连接的 EndpointId 之间转发**不透明的 QUIC datagram**（`RelayToClientMsg::Datagrams`），自己解不开任何内容，且**完全无状态** —— 对端不在线的包直接丢。

## relay 转发的是不透明密文

`iroh/iroh/src/lib.rs:99-103`：

> *"Relay servers exist to ensure all iroh endpoints are always reachable. They accept **encrypted** traffic for iroh endpoints which are connected to them, forwarding it to the correct destination based on the [EndpointId] only. Since endpoints only send encrypted traffic, the Relay servers can not decode any traffic for other iroh endpoints and only forward it."*

协议层佐证（`protos/relay.rs:71-80`）：`RelayToClientMsg::Datagrams { remote_endpoint_id: EndpointId, datagrams: Datagrams }` —— **relay 只认 EndpointId 和不透明字节**。

**两个含义**：

1. **明文 HTTP relay 并不意味着「数据明文过中继」** —— 底下是 QUIC，relay 看到的只有密文和 EndpointId。明文 relay 泄露的是**元数据**（谁在什么时候跟谁通信、流量大小），以及缺少 TLS 带来的中间人对 relay 连接本身的干扰能力
2. **应用层的额外加密与 relay 是否 TLS 完全正交**

## relay 不做暂存

目标 EndpointId 当前没连着这台 relay，**包直接丢弃**（只记一个 metric）：

```rust
// clients.rs:207-211
let Some(client) = self.0.clients.get(&dst) else {
    debug!(dst = %dst.fmt_short(), "no connected client, dropped packet");
    metrics.send_packets_dropped.inc();
    return Ok(());
};
```

**没有任何持久化/排队分支。** 队列满也丢（:218-224，队列深度 `protos/relay.rs:38-40` `PER_CLIENT_SEND_QUEUE_DEPTH: usize = 512`）。

**「relay 帮我暂存，对方上线再收」是幻想** —— iroh relay 做不到，libp2p circuit relay 也做不到。丢包本身不是问题（上层是 QUIC，会重传），问题是产品语义：**离线投递必须自己在应用层做。**

## home relay 是算出来的，不是配出来的

**没有人「决定」home relay，是算出来的。** `defaults.rs` **只负责把 4 个官方 relay 平等塞进一个 RelayMap，谁都不是默认 home relay**——整个文件没有任何 latency / geo / 选择代码：

```rust
// iroh/iroh/src/defaults.rs:20-79
pub mod prod {
    pub const NA_EAST_RELAY_HOSTNAME: &str = "use1-1.relay.n0.iroh.link.";
    pub const NA_WEST_RELAY_HOSTNAME: &str = "usw1-1.relay.n0.iroh.link.";
    pub const EU_RELAY_HOSTNAME: &str      = "euc1-1.relay.n0.iroh.link.";
    pub const AP_RELAY_HOSTNAME: &str      = "aps1-1.relay.n0.iroh.link.";

    pub fn default_relay_map() -> RelayMap {
        RelayMap::from_iter([default_na_east_relay(), default_na_west_relay(),
                             default_eu_relay(), default_ap_relay()])
    }
}
```

> **4 个 hostname 常量末尾都带一个点**（`use1-1.relay.n0.iroh.link.` 是 FQDN 绝对域名，绕过 DNS search domain）。照抄字符串做字符串比较时会因为这个尾点对不上。

**决策链是三段**：`net_report`（算）→ `socket.rs`（兜底）→ `RelayActor`（执行）。


`iroh/iroh/src/defaults.rs` 的 `pub mod prod` 定义四个 hostname：

| 代码里的地域标签 | hostname |
|---|---|
| NA east | `use1-1.relay.n0.iroh.link.` |
| NA west | `usw1-1.relay.n0.iroh.link.` |
| EU | `euc1-1.relay.n0.iroh.link.` |
| **Asia-Pacific** | `aps1-1.relay.n0.iroh.link.` |

`default_relay_map()`（:36-42）把四者 from_iter 组成 RelayMap。staging（:100）只含 NA-east + EU。

> ⚠️ **「无一在中国大陆」是推断，不是代码事实**。defaults.rs 只能证明 hostname 与代码里的地域标签。aps1-1 的实际落地机房需要实测解析+定位才能坐实。实务上几乎必然为真（ICP 备案等），但应标注为推断。


## ① net_report 选优：本轮有响应的 relay 中，取近 5 分钟最小延迟

```rust
// iroh/iroh/src/net_report.rs:747-821
fn add_report_history_and_set_preferred_relay(&mut self, r: &mut Report) {
    const MAX_AGE: Duration = Duration::from_secs(5 * 60);        // :763
    let mut best_recent = RelayLatencies::default();
    for (t, pr) in prevs_iter {
        if now.duration_since(*t) > MAX_AGE { to_remove.push(*t); continue; }
        best_recent.merge(&pr.relay_latency);
    }
    best_recent.merge(&r.relay_latency);
    // :790-791 注释：pick which currently-alive relay server *from the current report*
    //               has the best latency over the past MAX_AGE
    for (_, url, duration) in r.relay_latency.iter() {      // :795 —— 遍历的是本轮 report
        if let Some(best) = best_recent.get(url)
            && (r.preferred_relay.is_none() || best < best_any)
        { best_any = best; r.preferred_relay.replace(url.clone()); }
    }
}

// net_report/report.rs:145-158 —— merge 取最小值
pub(super) fn update_relay(&mut self, url: RelayUrl, latency: Duration, probe: Probe) {
    let old_latency = list.entry(url).or_insert(latency);
    if latency < *old_latency { *old_latency = latency; }
}
```

**精确表述**（容易概括错）：
- **探测覆盖 RelayMap 全部 relay**（`net_report/probes.rs:98-112` 的 `ProbePlan::initial` 对 `relay_map.relays::<Vec<_>>()` 逐个建 HTTPS ProbeSet）
- **但选优只在「本轮有响应的」relay 中进行**——循环遍历的是 `r.relay_latency`（本次 report 里有延迟读数的），不是 RelayMap 全集
- 排序判据是「**近 5 分钟窗口内的最小延迟**」，不是当前延迟

**后果 A**：一个 5 分钟窗口内历史延迟极佳、但**本轮探测没响应**的 relay，**不可能**被选中。

**后果 B**：一个偶发抖到很低的 relay 会在之后 5 分钟内一直保持竞争优势，直到那条历史记录过期被 `to_remove` 清掉。

全量报告间隔：`net_report.rs:132` `const FULL_REPORT_INTERVAL: Duration = Duration::from_secs(5 * 60);`

> **libp2p 无等价物。** iroh 这套 net_report 相当于 STUN + 延迟基准测试的合体；libp2p 的 autonat 只答「我公网可达吗」，不测 relay 延迟、不排序、不选优。

## ② 迟滞：新 relay 要快 33% 才切

```rust
// net_report.rs:807-816
// If we're changing our preferred relay but the old one's still
// accessible and the new one's not much better, just stick with where we are.
if prev_relay.is_some()
    && r.preferred_relay != prev_relay
    && !old_relay_cur_latency.is_zero()
    && best_any > old_relay_cur_latency / 3 * 2
{
    r.preferred_relay = prev_relay;
}
```

测试用例名：`preferred_relay_hysteresis_no_switch`（`:1199`）/ `preferred_relay_hysteresis_do_switch`（`:1214`）。

**`!old_relay_cur_latency.is_zero()` 是关键前置**：如果本次 report 里旧 relay **完全没探测到**（latency 缺席 → 保持默认 0），**迟滞条件短路失效，会立刻切走**。即「旧 relay 探测失败」和「旧 relay 变慢」走的是两条完全不同的路径——正好和上面「选优只看本轮响应者」咬合成同一套逻辑。

## ③ socket.rs 兜底：preferred_relay 为空时塞回当前的

```rust
// iroh/iroh/src/socket.rs:1964-1979
if r.preferred_relay.is_none() && let Some(my_relay) = self.sock.my_relay()
{ r.preferred_relay.replace(my_relay); }
```

## ④ RelayActor 只是消费者

```rust
// iroh/iroh/src/socket/transports/relay/actor.rs:1124-1149
async fn on_network_change(&mut self, report: Report) {
    let prev = self.config.my_relay.get();
    let prev_url = prev.as_ref().map(RelayStatus::url);
    if report.preferred_relay.as_ref() == prev_url { return; }   // No change.

    if let Some(relay_url) = report.preferred_relay {
        self.config.metrics.relay_home_change.inc();
        info!("home is now relay {}, was {:?}", relay_url, prev_url);
        // Publish `Connecting` initially. If an `ActiveRelayActor` already
        // exists for this URL it will republish its actual status ...
        self.config.my_relay.set(relay_url.clone(), RelayConnectionState::Connecting);  // :1138-1145
        self.set_home_relay(relay_url).await;
    } else {
        self.config.my_relay.clear();
    }
}
```

> ⚠️ **切 home 时无条件先 publish `Connecting`**（`:1142-1144`），哪怕目标 relay 的 ActiveRelayActor 早已 Connected。所以 `home_relay_status()` 观察者在换 relay 时**必然**看到一次 Connected → Connecting → Connected 抖动，**即使连接从未断过**。把 `is_connected()==false` 当成「掉线」来告警会误报。

## home relay ≠ 唯一连的 relay

```
// actor.rs:6-9
//! - The [`RelayActor`] manages all connections to relay servers.
//!   - It starts a new [`ActiveRelayActor`] for each relay server needed.
//!   - The [`ActiveRelayActor`] will exit when unused.
//!     - Unless it is for the home relay, this one never exits.

// actor.rs:64-84
const RELAY_INACTIVE_CLEANUP_TIME: Duration = Duration::from_secs(60);
const PING_INTERVAL: Duration = Duration::from_secs(15);   // QUIC max_idle_timeout 30s 的一半
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

// actor.rs:1166-1211 —— 会复用「已知道该对端」的其它 relay 连接
/// The endpoint is expected to be reachable on `url`, but if no [`ActiveRelayActor`] for
/// `url` exists but another existing [`ActiveRelayActor`] already knows about the endpoint,
/// that other endpoint is used.
```

**给对端发包时若对端 home 在另一个 relay，本地会再开一条到那个 relay 的连接。** 所以自建 relay 部署时，一个客户端在 RelayMap 里配几个 relay，最坏就可能开几条长连接。

**`home_relay_status()` 只反映 home 那一条的状态，不代表别的 relay 连接健康。**

## ⚠️ 同一 EndpointId 多连接 = active/inactive 互顶

`metrics.rs:89-96` doc 原文：

> *"Number of times a client was moved into the inactive state. A client becomes inactive when a new client connects with the same endpoint id. An inactive client can still send messages, but won't receive anything. If the currently-active client disconnects, and if there are inactive clients, the most-recent inactive client becomes active again."*

行为落地在 `clients.rs:85-130`（`try_send_health(Status::SameEndpointIdConnected)` 通知被顶替方，:127 提升回来时通知）。

**两个要小心的场景**：
1. 若两台设备**不慎复用了同一份密钥**（例如用户导入导出身份），会互相把对方顶成 inactive —— 表现为「**一台设备收不到任何东西但发得出去**」，极难排查
2. 网络切换后的重连瞬间会短暂出现两条连接

**务必确认身份密钥是设备级唯一。**

## relay 的线协议

### 0.91 起只剩 WebSocket

CHANGELOG 在 0.91.0（2025-07-30）的 Refactor 段落（`iroh/CHANGELOG.md:733` 版本头 / `:757` 条目）：

> *(iroh,iroh-relay)* Remove legacy relay path, make websocket connections default ([#3384])

**同版本另有两条 breaking 重塑了帧格式**（`CHANGELOG.md:738-739`）：`[breaking]` Use stride instead of custom split protocol, send ECN bits (#3389) / `[breaking]` Implement new handshake protocol, refactor frame types (#3331)。

**所以「只剩 WebSocket」不是单纯删了一条路径，而是整个 relay 线协议在 0.91 换了一代。跨 0.91 的 relay 客户端/服务端不互通。**

历史：0.20.0（2024-07-09）引入 WS 升级（CHANGELOG.md:1632）；0.91.0 移除 legacy 自定义升级路径 → WS 成为唯一实现。

**当前代码交叉验证**：

```rust
// iroh-relay/src/server/http_server.rs:747-768 —— /relay 没有第二个 handler
if matches!((req.method(), req.uri().path()), (&hyper::Method::GET, RELAY_PATH)) {
    let response = match self.handle_relay_ws_upgrade(req) { ... };
    return std::future::ready(response);
}
// http_server.rs:586-600 —— 强制 websocket
ensure!(upgrade_header == HeaderValue::from_static(WEBSOCKET_UPGRADE_PROTOCOL), ...);
ensure!(version.as_bytes() == SUPPORTED_WEBSOCKET_VERSION.as_bytes(), ...);
```

native 走 `tokio_websockets`（`client.rs:302`），browser 走 `ws_stream_wasm`（`client.rs:415`）—— **两端都是 WebSocket**。路径固定 `RELAY_PATH = "/relay"`（`http.rs:13`）。

**为什么是 WebSocket**：走 443/wss 的 WS 与普通 HTTPS 流量同形，是穿透企业防火墙/DPI 最保守的做法，也是**浏览器唯一能用的长连接**。

### ⚠️ 三处 stale 文档会骗人

| 位置 | 骗人的内容 | 真相 |
|------|-----------|------|
| **`iroh/iroh/src/lib.rs:97-110`**（crate 根文档 # Relay Servers 段） | 「The connections to the Relay server are initiated as normal HTTP 1.1 connections using **TLS**. Once connected the transport is **upgraded to a plain TCP connection using a custom protocol**.」 | 实际是 **WebSocket**（帧仍在 WS 里，不是裸 TCP），且 **TLS 非强制**（`http://` 会走 `ws://`）。**别引用 lib.rs 的 Relay Servers 段落写文档**，以 `iroh-relay/src/client.rs` + `server/http_server.rs` 为准 |
| `iroh-relay/src/http.rs:11-13` | 「/relay ... (over websockets **and a custom upgrade protocol**)」 | grep 全仓 iroh-relay/src 没有第二种 upgrade 分支 |
| `iroh-relay/src/server/http_server.rs:329` | 「Defaults to handling relay requests on the "/relay" (**and "/derp" for backwards compatibility**) endpoint」 | grep 没有任何 `/derp` 路由常量 |

同段 `lib.rs:115` 的 *"responding to ICMP echo requests"* 也已失效：`Probe` 枚举（`net_report/probes.rs:25-34`）只有 Https / QadIpv4 / QadIpv6。

> ⚠️ **另一个 stale 死代码**：`iroh-relay/src/client.rs:101` 有个 `RelayProtoNotAvailable` 错误变体（"The relay protocol is not available in browsers"），但 `grep -rn RelayProtoNotAvailable` 扫全 workspace **只有这一处声明，从未被构造**——它是 pre-1.0 时代曾有 HTTP-upgrade 自定义 relay transport 的残留。**1.0.2 里根本不存在「自定义 relay 协议 vs WebSocket」的分叉。**
>
> **「资料互相矛盾」大概率就来自这几处。**

### 尺寸上限

- `protos/relay.rs:23` `pub const MAX_PACKET_SIZE: usize = 64 * 1024;`（服务端在 `streams.rs:129-133` 强制 `ensure!(size <= MAX_PACKET_SIZE, ...)`）
- `protos/relay.rs:25-29` `pub(crate) const MAX_FRAME_SIZE: usize = 1024 * 1024;`（*"This is also the minimum burst size that a rate-limiter has to accept."*；客户端 `client.rs:314` 与服务端 `http_server.rs:861` 都用它设 `tokio_websockets::Limits::max_payload_len`）

**⚠️ `max_burst_bytes` 若配得比 1MiB 小，会和 MAX_FRAME_SIZE 打架**（注释明写「minimum burst size that a rate-limiter has to accept」）。

#### 关键背景：HTTPS 探测是 QUIC 被封网络下的唯一手段

`iroh/iroh/src/net_report.rs:89-100` `NetReportConfig::https_probes` 的 doc：

> *"HTTPS latency probes perform an empty HTTPS GET request to each configured relay server and measure latency. They are performed in addition to the QUIC address discovery (QAD) probes. **In networks that do not allow QUIC traffic, they are the only way to detect relay latencies and thus the preferred relay.** Disabling them is harmless on networks that do allow QUIC traffic, but will completely prevent finding the home relay on networks that do block QUIC."*

:123-129 Default 为 `https_probes=true`；`minimal()`（:114-120）则把它关掉。

**⚠️ 迁移风险点**：国内运营商对 UDP/QUIC 常有限速或阻断。若为了省事用 `NetReportConfig::minimal()` 或某些 preset 关掉 https_probes，**在封 QUIC 的网络下将完全选不出 home relay**。**默认值是安全的，别乱关。**

> **自建 relay 怎么配** → [07-configuration.md](07-configuration.md)；**带宽账单与限流** → [08-deployment.md](08-deployment.md)。

---

# 4. Address Lookup

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

## iroh 生态没有「短码 → 地址」的 rendezvous

`n0-mainline/README.md` 原文：*"The main purpose for which iroh uses n0-mainline is endpoint address lookup via BEP_0044."* —— 即便 iroh 用到 Mainline DHT，用途也只是**按 endpoint 公钥查地址**，不是按任意短码查记录。

**iroh 的寻址原语只有 pubkey→addr（pkarr / DNS / mainline），没有 code→record 这层。**

**若要「人能念出来的短码」，只能自己维护一个 rendezvous 服务**（且要自己解决短码空间的可枚举问题——6 位数字码只有 10^6 空间，攻击者持续扫描即可捕获每一次进行中的配对；缓解手段是加长/加盐/限速/短 TTL）。**这是一笔明确的自维护基础设施成本，iroh 生态不提供任何对等物。**

**ticket 方案的对比**：「码」就是 32 字节 ed25519 公钥（EndpointId，`iroh/iroh-base/src/key.rs:70` `pub type EndpointId = PublicKey;`），**2^256 空间不可枚举，且根本不需要往任何公共存储写记录**（自包含）。

**换句话说：ticket 变长的那 57+ 个字符，买的正是「不可枚举 + 不用公开广播自己的地址」。** 若能接受放弃人念短码，改**二维码 + 链接 + 剪贴板**三件套即可（正是 sendme 做的 —— `main.rs:793-867` 有专门的剪贴板支持，feature 名就叫 `clipboard`）。

> **三条通路的接线** → [02-connecting.md](02-connecting.md)（DNS / DHT / mDNS 各一节）。

---

# 5. NAT Traversal

**iroh 的 NAT 穿透比 libp2p 强且免维护，但心智模型完全不同：没有 STUN、没有 DCUtR 握手。**
连接建立后由 QUIC multipath 在同一条 live connection 上把 path 升级为直连 ——
**穿透失败只是「降级留在 relay」，不是连不上**。

## 心智差异：没有 STUN，没有 DCUtR

### STUN 已彻底移除

`iroh/iroh/src/net_report/probes.rs:25-33` 探测协议枚举**只有三个变体**：

```rust
pub enum Probe { Https, QadIpv4, QadIpv6 }
```

—— **无任何 Stun 变体**。`iroh/CHANGELOG.md:585` 记录 `*(iroh)* Remove stun-rs (#3546)`。

全仓 `grep -rni stun --include="*.rs"` 仅 20 处命中，**全部为标识符残留**（如 `src/socket.rs:1464 periodic_re_stun_timer`、`src/socket/metrics.rs:96 actor_tick_re_stun`），**无一处是 STUN 协议实现**。

`probes.rs:1-5` 模块文档：*"Preferably the QAD probes work and we also learn about our public IP addresses and ports. But fallback probes for HTTPS exist as well."*

**排查「连不上/打洞失败」时该看 net_report 的 QAD 结果，不是找 STUN 服务器配置。**

### 没有 DCUtR 式的独立打洞握手

全仓 grep `call_me_maybe|CallMeMaybe` 在 `iroh/iroh/src/` 下 **零命中**（⚠️ 注意：grep `disco` **有 92 处命中**（address_lookup/discovery/discovered/Disconnected 等），别拿它当证据）。`iroh/CHANGELOG.md:718` `Use quinn multipath`。

**机制**：一条 `Connection` 持有多个 path，其一被选中。path 升级由 iroh 在底下静默完成，业务无感。

`iroh/iroh/tests/patchbay/nat.rs:106-108` 的测试断言即为该语义：

```rust
assert!(is_relayed(&conn), "connection started relayed");
conn.wait_ip(timeout).await.context("holepunch to direct")?;
info!("connection became direct");
```

`tests/patchbay/util.rs:383` 的 `is_relayed()`：`conn.paths().iter().find(|p| p.is_selected()).expect("no selected path").is_relay()`。

### ⚠️ 「连接一律先走 relay」是测试脚手架造成的，不是连接层定律

`nat.rs` 里 `assert!(is_relayed(&conn), "connection started relayed")` 之所以恒成立，是因为 **`tests/patchbay/util.rs:474-476` 主动把直连地址过滤掉了**：

```rust
fn addr_relay_only(addr: EndpointAddr) -> EndpointAddr {
    EndpointAddr::from_parts(addr.id, addr.addrs.into_iter().filter(|a| a.is_relay()))
}
```

在 `util.rs:232-233` 调用，注释写得很直白：*"Send address to client task. Make it a relay-only address, **like in the default address lookup services**."*

**即：relay-first 是 n0 默认 address lookup（pkarr/DNS）只发布 relay 地址这一策略的后果，不是连接层定律。** 全部 16 个 NAT 测试（含 `nat_none_x_none` 这种无 NAT、公网可路由的场景）都走同一个 `run_nat_holepunch` 脚手架、都吃这个过滤，所以「连 None×None 都先走 relay」只能证明脚手架生效。

**反证**：`iroh-mdns-address-lookup` 的 README 明写 *"By default, MdnsAddressLookup publishes all addresses it receives: direct IP addresses and up to one RelayUrl"* —— **局域网下拨号方手里就有对端直连 IP，连接可以直接起在 IP path 上，全程不碰 relay。**

**弱化但正确的版本**：relay **始终作为兜底**存在；path 升级由 iroh 静默完成；穿透失败 = 降级而非连不上。

## 官方 NAT 矩阵：16 组合，3 种打不通

`iroh/iroh/tests/patchbay/nat.rs` 是 Linux userns 网络仿真测试套件。模块文档（:11-12）：*"Every test expects a direct path to be established. Tests where holepunching is not yet working are marked `#[ignore]`."*

**打不通的三个**（全是涉及 Hard NAT 的组合）：

| 测试 | 标注 |
|---|---|
| `nat_easy_x_hard()`(:202-203) | `#[ignore = "not yet passing (and likely can't without port guessing)"]` |
| `nat_hard_x_easy()`(:223-224) | 同上 |
| `nat_hard_x_hard()`(:230-231) | 同上 |

**其余 13 个无 ignore，即 CI 中必须通过** —— **包括 `nat_hard_x_none()`(:211) 与 `nat_hard_x_easiest()`(:217)**。

NAT 类型定义（`nat.rs:24-52` 的 `enum NatKind`）：None / Easiest / Easy / **Hard**。Hard 的文档注释（:43-50）：*"Endpoint-Dependent Mapping, Address-and-Port-Dependent Filtering (EDM/APDF) / RFC 3489: Symmetric NAT"*，并注 *"Typical of corporate firewalls and carrier-grade NAT (CGN)"*。

> ⚠️ `nat.rs:3-9` 的**模块文档已 stale** —— 仍把 NAT 类型写作 "None, Home, Corporate"，与实际 enum 不符。

**含义**：中国移动网络普遍 CGNAT（= Hard）。**手机↔手机（Hard×Hard）必然走 relay，这是 iroh 也解决不了的物理事实** —— 自建 relay 的带宽成本要按「移动端之间全量中转」预算。反之**手机↔有公网/UPnP 的桌面（Hard×None / Hard×Easiest）是能直连的**。

## 中国网络的真正卡点：QAD 走 UDP/7842

| 通道 | 端口 | GFW 友好度 |
|---|---|---|
| **relay 数据面** | WebSocket over HTTPS/**443** | ✅ 与普通 HTTPS 同形 |
| **QAD** | UDP/**7842** | ⚠️ 非标端口 |

- QAD 端口：`iroh/iroh-relay/src/defaults.rs:7` `pub const DEFAULT_RELAY_QUIC_PORT: u16 = 7842;`
- QAD 机制：`iroh-relay/src/quic.rs:294` `transport.receive_observed_address_reports(true);`、:325 `conn.observed_external_addr()`、:10 定义专用 ALPN `ALPN_QUIC_ADDR_DISC = b"/iroh-qad/0"`（client 侧使用在 :274）
- relay 数据面是 WSS：`iroh-relay/src/client.rs:283` `debug!(%dial_url, "Dialing relay by websocket");`、:302 `tokio_websockets::ClientBuilder::new()`

**公网地址只由 QAD 产生**（`iroh/iroh/src/net_report/report.rs:18-34`）：`udp_v4`/`udp_v6` 注释为 *"A QAD IPv4/IPv6 round trip completed"*，`global_v4`/`global_v6` 为 *"The discovered global IPv4 address and port, if any"*；:49-51 `has_udp()` 仅由 `udp_v4||udp_v6` 决定。**HTTPS 探测只测延迟不产地址。**

### ⚠️ 但「QAD 挂了 → 永远无法直连」是错的

`iroh/iroh/src/socket.rs:1821-1841` 的 `update_direct_addresses()` 依次装配**四类**候选：

| # | 来源 | DirectAddrType | 依赖 QAD？ |
|---|---|---|---|
| 1 | **portmapper**（`self.direct_addr_update_state.port_mapper.watch_external_address()`，注释 *"First add PortMapper provided addresses"*） | `Portmapped` | ❌ **完全独立** |
| 2 | `net_report_report.global_v4/global_v6` | `Qad` | ✅ |
| 3 | `collect_local_addresses()` | `Local` | ❌ |
| 4 | `configured_addrs` | `Config` | ❌ |

（`socket.rs:2079-2103` 的 `DirectAddrType` 枚举有 6 个变体：Unknown / Local / Qad / Portmapped / Qad4LocalPort / Config。）

**所以 QAD 挂掉 ≠ 直连候选为空**：portmapper（默认开启）仍能产出公网直连候选，local 地址仍能产出局域网直连候选。

**尤其讽刺的是**：假设的失效模式是「7842 这个非标端口被针对性封锁/QoS」—— 而这**正是 portmapper 完好无损、能救场的情形**（封的是特定端口，不是所有 UDP）。

**准确表述**：拿不到 QAD 会**显著降低直连率**，不是「永远不可能直连」。

**运维结论仍然成立**：自建 relay 时**必须同时放通 443/tcp 与 7842/udp**，只开 443 会显著劣化直连率、带宽账单上涨却看不出原因。**上线前用 iroh-doctor 在真实国内网络实测 7842 可达性。**

## portmapper（UPnP/PCP/NAT-PMP）

- **默认开启**：`iroh/iroh/Cargo.toml:148` `default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]`；`iroh/iroh/src/portmapper.rs:36-38` `impl Default` 返回 `PortmapperConfig::Enabled {}`
- `portmapper.rs:20-32` `pub enum PortmapperConfig { Enabled {}, Disabled }`，Disabled 变体文档明写：*"Skips the UPnP/PCP/NAT-PMP gateway probing. Use this to avoid the SSDP multicast discovery that can raise firewall dialogs (**notably on macOS**), at the cost of potentially worse direct connectivity behind some NATs."*

**价值与边界**：

✅ portmapper 能把**家用路由器**造成的 Hard/Easy NAT 变成可直连（NAT 矩阵里 Easiest 即 *"Typical of consumer routers with UPnP"*）→ 对提升**国内家宽直连率**有实质价值，**不建议关**。

⚠️ **但它救不了 CGNAT**：UPnP/PCP/NAT-PMP 是向**本地 CPE 路由器**申请端口映射；中国移动网络的 Hard 属性来自**运营商侧的 CGNAT**，它在家用路由器**之上**，**不响应 UPnP**。`nat.rs:43-50` 的 Hard 注释把 corporate firewalls 与 CGN 并列，但两者对 portmapper 的响应能力完全不同。

**含义**：portmapper 的收益范围限定在家宽直连率，**不能顺延到移动网络**。

**macOS 首次启动的防火墙弹窗需在 onboarding 里预先解释**，否则用户会误以为是恶意行为。

> **选路优先级（含 custom transport 会被默认判成 Primary 的陷阱）** → [04-transports.md](04-transports.md)。
> **自建 relay 要放通哪些端口** → [07-configuration.md](07-configuration.md)。

---

# 6. Tickets

## 一句话定位

**ticket 只是「postcard + base32 的自描述地址信封」，不是 rendezvous 方案，更不是一次性凭证。**

它**零过期、零 nonce、零签名**。要一次性/过期语义，必须自己在 payload 里做 + 服务端记状态。

### 真正的价值是那个 trait，不是 EndpointTicket

`EndpointTicket` 的全部状态就是**一个字段**（`endpoint.rs:30-32`）：

```rust
pub struct EndpointTicket { addr: EndpointAddr }
```

塞不进 nonce / expires_at / 设备名。**扩展的唯一方式是自己 impl `Ticket` trait**（`lib.rs:26-65`，只要求 KIND / encode_bytes / decode_bytes，encode_string/decode_string 有默认实现）。

### 自定义 ticket 模板

照 `iroh-docs/src/ticket.rs` 的结构（那是唯一用了 verification 的实现）：

```rust
use iroh_tickets::{Ticket, ParseError};
use serde::{Serialize, Deserialize};

// 单变体 enum：强制 postcard 写出 1 字节判别符，给未来留版本位
#[derive(Serialize, Deserialize)]
enum MyInviteWire { V0(MyInvite) }

#[derive(Serialize, Deserialize, Clone, Debug, derive_more::Display)]
#[display("{}", Ticket::encode_string(self))]
pub struct MyInvite {
    pub addr: iroh::EndpointAddr,   // 直接存 EndpointAddr —— 内含完整 BTreeSet<TransportAddr>
    pub nonce: [u8; 16],            // 一次性语义要自己加，ticket 本身没有
    pub expires_at: i64,            // 过期语义要自己加，ticket 本身没有
}

impl Ticket for MyInvite {
    const KIND: &'static str = "invite";   // 必须小写 ascii（lib.rs:29）
    fn encode_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(&MyInviteWire::V0(self.clone())).expect("postcard")
    }
    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let MyInviteWire::V0(t) = postcard::from_bytes(bytes)?;   // postcard::Error -> ParseError 有 From
        if t.addr.addrs.is_empty() {
            return Err(ParseError::verification_failed("addressing info cannot be empty"));
        }
        Ok(t)
    }
}
impl std::str::FromStr for MyInvite {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Ticket::decode_string(s) }
}
// 产出: "invite" + base32(postcard(..)) 小写
```

**要点**：直接存 `EndpointAddr` 而不是照抄 BlobTicket 的拆字段写法（后者是有损的，见下）。

### 零过期 / 零一次性 / 零撤销

对 `iroh-tickets/src/` 执行 `grep -rn -i "expir|ttl|timestamp|nonce|one.time|revoke|valid_until"` → **零命中**。trait 只要求三个方法，**没有任何 validate/expire 钩子**。

**自己实现时的两层区分**：
- `decode_bytes` 里的校验只能防「格式 / 时间」
- **防不住重放** —— 重放必须服务端维护 nonce 已用集合

## ticket 长度 —— 实测

用本地 iroh-tickets 1.0.0 编译真实测试实测：

| 内容 | 字节 | 字符 |
|---|---|---|
| id-only（`EndpointAddr::from_parts(pk, [])`） | 34 | **63** |
| + 1 个真实 n0 relay（`https://use1-1.relay.n0.iroh.link./`） | 71 | **122** |
| relay + 3 个 IP（192.168 / 100.64 / IPv6） | 110 | **184** |

实例：`endpointacxfr74igmsbvsbnn73wcecg5vt3kbzncqwfrdiampuufwnhkublmaa`（63 字符）。

字节结构（由 `endpoint.rs:203-222` 的测试向量佐证）：1 字节 variant + 32 字节 endpoint id + 1 字节 addr 计数 + 每个 addr（1 字节 tag + 内容）。

**含义**：
- **人念 / 电话报码彻底不可能** —— 最短 63 字符，是 6 位码的 10 倍以上
- **二维码完全够** —— 184 字符远在 QR 容量内
- 若还要塞 nonce(16B) + expires_at(8B) + 设备名，再加约 25 字节 ≈ **+40 字符** → 按 **160~230 字符**规划预算

## base32 大小写 —— QR 优化点

- `encode_string`（lib.rs:44-49）末尾 `out.make_ascii_lowercase()` 把整串（含 base32 body）转小写
- `decode_string`（lib.rs:57-64）是 `let Some(rest) = s.strip_prefix(expected)`（**KIND 必须精确匹配 = 必须小写**）后 `BASE32_NOPAD.decode(rest.to_ascii_uppercase().as_bytes())` —— **body 解码前主动转大写，故 body 大小写不敏感**

实测验证：`endpoint` + 大写 body 解码成功；整串大写则失败，报 `wrong prefix, expected endpoint`。

**QR 优化**：QR 的 alphanumeric 模式只收大写字母+数字（每字符 5.5 bit），小写会掉进 byte 模式（8 bit/字符）。可以把 body 大写 —— 但 **KIND 前缀卡死必须小写**，会强制整串掉回 byte 模式。绕法：二维码里只编码大写 body（不含前缀），扫码后代码里补回小写前缀再 decode。

> ⚠️ 这是实测的 iroh-tickets 1.0.0 行为，**非文档承诺**。

### 3. BlobTicket 的编码是有损的

`iroh-blobs/src/ticket.rs:72-73` encode_bytes 内：
- `relay_url: self.addr.relay_urls().next().cloned()` —— **`.next()` 只取第一个**
- `direct_addresses: self.addr.ip_addrs().cloned().collect()`

wire 结构（:58-62）`struct Variant0AddrInfo { relay_url: Option<RelayUrl>, direct_addresses: BTreeSet<SocketAddr> }` —— **没有容纳 `TransportAddr::Custom` 的位置**（`relay_urls()`/`ip_addrs()` 是按变体过滤的 filter_map，`iroh/iroh-base/src/endpoint_addr.rs:137-152`，Custom 两个都不匹配、直接被滤掉）。

对照 `iroh-tickets/src/endpoint.rs:52-54`，EndpointTicket 存的是完整 `addrs: self.addr.addrs.clone()`（`BTreeSet<TransportAddr>`），**无损**。

> ⚠️ **多 relay 丢失在实践中影响有限**：iroh 自己的文档（`iroh-base/src/endpoint_addr.rs:146`）对 `relay_urls()` 说 *"In practice this is expected to be zero or one home relay for all known cases currently."* —— iroh 的寻址模型是 home-relay-singular。**Custom 地址丢失才是这条 finding 更真实的一半。**

**结论**：自定义 ticket 时直接存 `EndpointAddr`，别照抄 BlobTicket 的 Variant0 拆字段写法 —— 后者是历史包袱（`ticket.rs:44` 有显式 `// Legacy` 标记，结构体名 `Variant0NodeAddr` 与 `Variant0BlobTicket` 的 `node:` 字段（:47）都是遗留；注意 `endpoint_id` 反而是**当前**命名，不是旧名）。

## KIND 前缀是自描述类型标签

浏览器样例的用法（`iroh-examples/browser-chat/frontend/src/components/homescreen.tsx:15-20`）：

```ts
const [ticket, setTicket] = useState(() => {
  const url = new URL(document.location.toString())
  const ticket = url.searchParams.get("ticket")
  if (ticket?.startsWith("chat")) return ticket   // KIND 当廉价类型标签
  return ""
})
```

其中 `"chat"` 正是 `shared/src/lib.rs:55` 的 `const KIND: &'static str = "chat";`。

生成侧（`invitepopup.tsx:19-23`）走的是 **`?ticket=` 查询参数**，不是自定义 scheme：

```ts
function ticketUrl(ticket: string) {
  const baseUrl = new URL(document.location.toString())
  baseUrl.searchParams.set("ticket", ticket)
  return baseUrl.toString()
}
```

另见 `iroh-examples/dumbpipe-web/src/main.rs:113-123` `parse_subdomain`：先试 `iroh::EndpointId::from_str(subdomain)`，失败再试 `dumbpipe::EndpointTicket::from_str(subdomain)` —— 靠前缀+长度天然区分两种形态。

**自定义 ticket 只要 8~12 行**：`iroh-examples/browser-chat/shared/src/lib.rs:54-65` 的 ChatTicket impl 是 12 行（含 `#[derive]`），payload 只有 `topic_id` + `bootstrap: BTreeSet<EndpointId>`（**只有 id、没有地址** —— 那是因为 gossip 有自己的成员发现，点对点配对没有这个兜底，id-only 会强制你依赖 pkarr/DNS）。

> ⚠️ 别抄 browser-chat 的 `postcard::to_stdvec(&self).unwrap()` 错误处理（`shared/src/lib.rs:55` 区域）。

## ticket 里放多少地址 —— 长度 vs 连通率

这个权衡在生态里有两处独立实现，结论一致：**「长一点但连得上」优先。**

### ticket：EndpointAddr + BlobFormat + Hash

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

### 短票技巧

```rust
// main.rs:632-639
fn create_short_ticket(addr: &EndpointAddr) -> EndpointTicket {
    let mut short = EndpointAddr::new(addr.id);
    for relay_url in addr.relay_urls() { short = short.with_relay_url(relay_url.clone()); }
    short.into()
}
```

只保留 id + relay url，丢弃直连地址。⚠️ **`online()` 超时只 warn 不 fail——拿到的 ticket 可能缺 relay url，对端只能靠直连碰运气。**

## 落地要点

| 决策点 | 建议 |
|---|---|
| **KIND 取值** | ⚠️ **必须先拍板再发版** —— KIND 会烤进每一个发出去的链接，改了就废掉所有存量 ticket。注意别与 URL scheme 冗余：若 scheme 是 `myapp://`，KIND 再取 `"myapp"` 会得到 `myapp://myapp<base32>` |
| **一次性 + 过期** | payload 里放 nonce + expires_at，`decode_bytes` 里用 `ParseError::verification_failed` 拒过期；**重放必须服务端记 nonce 已用集合**（`decode_bytes` 只能防「格式 / 时间」，防不住重放） |
| **长度预算** | 160~230 字符（id-only 63 + nonce/expires/元数据）。二维码 + 剪贴板 + 链接三件套，放弃人念 |
| **版本兼容** | 单变体 enum 留位，老变体永不删。**iroh 自己都破坏过一次**（见下） |
| **地址存法** | 直接存 `EndpointAddr`，**别拆字段**（BlobTicket 的拆字段写法是有损的，见下） |
| **online() 超时** | 照 sendme：超时就拒绝生成 ticket（除非确定配了 pkarr publisher） |

> **FFI 形状**（ticket 过 uniffi 边界用 Object 还是 Record）→ [05-languages.md](05-languages.md)。
> **生成 ticket 前必须先 `online()`** 的完整论证 → [02-connecting.md](02-connecting.md)。
