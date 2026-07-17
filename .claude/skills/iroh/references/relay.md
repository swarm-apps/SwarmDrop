# Relay：选路 · 协议 · 自建 · 成本

iroh 1.0.2 / iroh-relay 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

> 「设备之间怎么互相找到」→ [address-lookup.md](address-lookup.md)。本文只讲 relay。
>
> ⚠️ **两个同名的 `RelayConfig`**：`relay_map::RelayConfig`（**客户端侧**，被 `iroh/src/lib.rs:290` re-export，即 `iroh::RelayConfig`）vs `server::RelayConfig`（**服务端侧**，需 `use iroh_relay::server::RelayConfig`）。字段完全不同，同时 import 必须 alias。**文档/示例里看到 `RelayConfig` 一定要先看 use 语句。**

## 一句话结论

**自建 relay 完全可行，且比官方文档说的宽松得多** —— `tls` 是 `Option<TlsConfig>`，整段省掉就是合法的生产配置（不需要域名、不需要证书）。且**自建 relay 不需要配套任何 DNS/pkarr 服务器**。

**但代价必须算清**：**没有 `[tls]` 配置段 ⇒ 没有 QAD ⇒ 拿不到公网地址 ⇒ 直连率显著下降 ⇒ 流量大量常驻中继。**

---

# 第一部分：relay 是什么

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

## RelayMode 四态：Disabled 不是「空 RelayMap」

```rust
// endpoint.rs:1920-1934
pub enum RelayMode {
    Disabled,          // 既不监听也不拨号 relay
    Default,           // n0 生产 relay（4 个）
    Staging,           // n0 staging relay（只有 2 个：NA east + EU，无 AP）
    Custom(RelayMap),  // :1933
}

// endpoint.rs:1936-1944 —— 这条路返回空 map
impl RelayMode {
    pub fn relay_map(&self) -> RelayMap {
        match self { RelayMode::Disabled => RelayMap::empty(), ... }
    }
}

// endpoint.rs:153-170 —— 但 Builder 走的是另一条路
impl From<RelayMode> for Option<TransportConfig> {
    fn from(mode: RelayMode) -> Self {
        match mode { RelayMode::Disabled => None, ... }        // ← None
    }
}
// endpoint.rs:557-575 —— None 分支直接 retain 掉整个 Relay transport
None => { self.transports.retain(|t| !matches!(t, TransportConfig::Relay { .. })); }
```

**两条路径语义不同：`RelayMode::Disabled.relay_map()` 返回空 map，但 `Builder::relay_mode(Disabled)` 是把 Relay transport 从 transports 列表整个移除。不要用 `relay_map().is_empty()` 去判断 relay 是否启用。**

> libp2p 里 relay 是一个可选 Behaviour（`libp2p::relay::client::Behaviour`），不加就没有；iroh 里 relay 是 Builder 上一个必答的枚举，且 `presets::N0` 默认给你开 Default。

### staging 后门

`presets::N0` 调的是 `default_relay_mode()`（`presets.rs:136`），所以**只要环境里有个非空的 `IROH_FORCE_STAGING_RELAYS`，你的「生产」程序会静默连到 staging relay**。CI/容器里误传这个变量很难查。文档也明说 staging「might have incompatible changes deployed」（`defaults.rs:84`）。

staging 只有 2 个 relay（`defaults.rs:82-101`），注释：「we have staging servers in EU and NA, but no corresponding staging server for AP at this time」。

**同一个环境变量还会同时切换 pkarr relay 端点**（`address_lookup/pkarr.rs` 的 `n0_dns()` 里 `force_staging_infra() ? N0_DNS_PKARR_RELAY_STAGING : N0_DNS_PKARR_RELAY_PROD`）。

> ⚠️ **一条 stale 文档**：`presets.rs:103-105` / `pkarr.rs:520-522` 写着「When in tests, or when the `test-utils` feature is enabled...」——**与代码不符**。`force_staging_infra` 全树只有一处定义（`endpoint.rs:1970-1972`），**纯读环境变量，无 `#[cfg(test)]` / `#[cfg(feature="test-utils")]` 分支**。

---

# 第二部分：home relay 的选优

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

### 多 relay 不分摊压力：mesh 已死

`CHANGELOG.md:2062`「Remove derp meshing (#2079)」位于 0.13.0 段（2024-03-25）；现源码 `grep -rni mesh iroh-relay/src/ iroh/src/` **零命中**。

**结论：先只部署一台 relay。** 多 relay 只在「用户地理分布广、想各自就近接入」时才有意义，且要求两端都能连到对方的 home relay。单 relay 下所有人 home relay 相同，行为最可预测。将来加第二台靠 `insert_relay` 运行时下发即可。

## RelayStatus：公开只读视图

```rust
// endpoint.rs:1886-1918（struct 在 :1890）
pub struct RelayStatus { url: RelayUrl, state: RelayConnectionState }
impl RelayStatus {
    pub fn url(&self) -> &RelayUrl { &self.url }
    pub fn is_connected(&self) -> bool { self.state.is_connected() }
    /// Returns the most recent connection error, if the relay is currently disconnected.
    pub fn last_error(&self) -> Option<&AnyError> { self.state.last_error().map(Arc::as_ref) }
}

// socket/transports/relay/actor.rs:894-923 —— 内部三态，pub(crate) 刻意不暴露
pub(crate) enum RelayConnectionState {
    Connecting,                                       // Dialing or performing the relay handshake.
    Connected,                                        // Connected and handshaked.
    Disconnected { last_error: Option<Arc<AnyError>> },
}
```

**三个细节：**

1. **`Connecting` 涵盖整个握手期**（doc：「Dialing **or performing the relay handshake**」），`Connected` 才是「handshaked」。这正是 `initialized()` 返回时刻与 `online()` 返回时刻之间的那段窗口（见 [foundations.md](foundations.md)）
2. **`last_error()` 在 Connecting 状态下也返回 None**（只 match `Disconnected` 分支，`actor.rs:917-922`）。重连过程中报错信息会「消失」：`Disconnected{Some(err)}` → 重试 → `Connecting`(last_error 变 None) → 再失败 → `Disconnected{Some(err2)}`。**想留存错误必须自己缓存。**
3. **`PartialEq` 对 `Disconnected.last_error` 用 `Arc::ptr_eq` 按指针比较**（`actor.rs:925-939`）。这是**故意**的：注释写「each new failure produces a fresh allocation, so the watcher fires on every new error」——为了绕过 Watchable「值不变不通知」的特意设计。**别指望「同样的错误不会重复通知」。**

## `home_relay_status()` 为什么是 Vec

```rust
// socket/transports.rs:76-79
pub(super) type HomeRelayWatcher = n0_watcher::Map<
    n0_watcher::Join<Option<RelayStatus>, n0_watcher::Direct<Option<RelayStatus>>>,
    Vec<RelayStatus>,
>;

// transports.rs:380-383 —— Option 被 flatten 掉，None → 空 Vec
pub(super) fn home_relay_watch(&self) -> HomeRelayWatcher {
    n0_watcher::Join::new(self.relay.iter().map(|t| t.my_relay_status()))
        .map(|v| v.into_iter().flatten().collect())
}
```

每个 `RelayTransport` 持有恰好一个 `HomeRelayWatch`，Vec 是把它们 Join + flatten 的结果。**常规配置下长度为 0 或 1。**

`flatten()` 把 Option 的「无」压成了 Vec 的「空」——信息没丢，但正因如此 `Nullable<Vec<T>>` 才会把空 Vec 当 null，[foundations.md](foundations.md) 的陷阱链条就此闭合。

**公开文档措辞很准**（`endpoint.rs:1374-1376`）：「is empty when no relays are configured **or before the endpoint has selected a home relay**」——注意是「**selected**」而非「connected」。

⚠️ **空 Vec 有两种截然不同、无法区分的含义**：(a) 压根没配 relay（`RelayMode::Disabled`），(b) 配了但 net_report 还没跑出 preferred_relay。**启动阶段用 `.is_empty()` 判断「relay 没开」会误判。想等「上线」请直接用 `Endpoint::online()`。**

## ⚠️ 同一 EndpointId 多连接 = active/inactive 互顶

`metrics.rs:89-96` doc 原文：

> *"Number of times a client was moved into the inactive state. A client becomes inactive when a new client connects with the same endpoint id. An inactive client can still send messages, but won't receive anything. If the currently-active client disconnects, and if there are inactive clients, the most-recent inactive client becomes active again."*

行为落地在 `clients.rs:85-130`（`try_send_health(Status::SameEndpointIdConnected)` 通知被顶替方，:127 提升回来时通知）。

**两个要小心的场景**：
1. 若两台设备**不慎复用了同一份密钥**（例如用户导入导出身份），会互相把对方顶成 inactive —— 表现为「**一台设备收不到任何东西但发得出去**」，极难排查
2. 网络切换后的重连瞬间会短暂出现两条连接

**务必确认身份密钥是设备级唯一。**

---

# 第三部分：relay 的线协议

## 0.91 起只剩 WebSocket

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

## ⚠️ 三处 stale 文档会骗人

| 位置 | 骗人的内容 | 真相 |
|------|-----------|------|
| **`iroh/iroh/src/lib.rs:97-110`**（crate 根文档 # Relay Servers 段） | 「The connections to the Relay server are initiated as normal HTTP 1.1 connections using **TLS**. Once connected the transport is **upgraded to a plain TCP connection using a custom protocol**.」 | 实际是 **WebSocket**（帧仍在 WS 里，不是裸 TCP），且 **TLS 非强制**（`http://` 会走 `ws://`）。**别引用 lib.rs 的 Relay Servers 段落写文档**，以 `iroh-relay/src/client.rs` + `server/http_server.rs` 为准 |
| `iroh-relay/src/http.rs:11-13` | 「/relay ... (over websockets **and a custom upgrade protocol**)」 | grep 全仓 iroh-relay/src 没有第二种 upgrade 分支 |
| `iroh-relay/src/server/http_server.rs:329` | 「Defaults to handling relay requests on the "/relay" (**and "/derp" for backwards compatibility**) endpoint」 | grep 没有任何 `/derp` 路由常量 |

同段 `lib.rs:115` 的 *"responding to ICMP echo requests"* 也已失效：`Probe` 枚举（`net_report/probes.rs:25-34`）只有 Https / QadIpv4 / QadIpv6。

> ⚠️ **另一个 stale 死代码**：`iroh-relay/src/client.rs:101` 有个 `RelayProtoNotAvailable` 错误变体（"The relay protocol is not available in browsers"），但 `grep -rn RelayProtoNotAvailable` 扫全 workspace **只有这一处声明，从未被构造**——它是 pre-1.0 时代曾有 HTTP-upgrade 自定义 relay transport 的残留。**1.0.2 里根本不存在「自定义 relay 协议 vs WebSocket」的分叉。**
>
> **「资料互相矛盾」大概率就来自这几处。**

## URL scheme 映射是兜底式 match

```rust
// iroh-relay/src/client.rs:267-281 (native) 与 :391-405 (wasm_browser)，两处逻辑一致
let mut dial_url = (*self.url).clone();
dial_url.set_path(RELAY_PATH);                     // ← 覆盖式写入！你 URL 里的 path 全被丢弃
dial_url.set_scheme(match self.url.scheme() {
    "http" => "ws",
    "ws"   => "ws",
    _      => "wss",                               // ← 通配兜底，不是白名单
}).map_err(|_| e!(ConnectError::InvalidWebsocketUrl { url: dial_url.clone() }))?;
```

`RelayUrl` **不做 scheme 校验**（`iroh-base/src/relay_url.rs:21-44`，只是 `Arc<Url>` 的 newtype）。

**写错 scheme（`htp://`、`foo://`）不会报错，会被静默当成 wss 去连，错误延后到 TLS 握手才炸，报错信息完全不指向 scheme 拼错。**

> libp2p 的 multiaddr `/dns4/x/tcp/443/wss` 里 transport 是显式声明且被解析器强校验的；iroh 用普通 URL + 隐式映射，错拼不报错。

## 协议版本协商（第二层版本线，别与 0.91 混为一谈）

```rust
// iroh-relay/src/http.rs:25-68
#[non_exhaustive]
pub enum ProtocolVersion {
    #[strum(serialize = "iroh-relay-v1")] V1,   // the only version supported until iroh 0.98.0
    #[default]
    #[strum(serialize = "iroh-relay-v2")] V2,   // added in iroh 0.98.0
                                                // - Removed `Health` frame (id 11)
                                                // - Added `Status` frame (id 13)
}
impl ProtocolVersion { pub const ALL: &'static [Self] = &[Self::V2, Self::V1]; }   // :68，newest first
```

走 WebSocket 子协议头 `Sec-WebSocket-Protocol`（`client.rs:309-312` 把全部版本列进头；`:345-355` 要求服务端回一个能识别的，否则 `BadVersionHeader` 硬失败）。

**两层版本线**：0.91 定「传输壳子只能是 WS」，0.98 定「WS 里跑的帧格式是 v2」。

**已核实的兼容性事实**：**新服务端对老客户端是明确硬拒的**——`http_server.rs:603` 用 `expect_header(&req, SEC_WEBSOCKET_PROTOCOL)?` 强制要求客户端带子协议头，不带就 MissingHeader 报错。

⚠️ 反方向（新客户端 → 老服务端）**本仓无法核实**，只能确定：服务端不回显该头 → `protocol_version_str` 为 None → 同样 `?` 出 `BadVersionHeader` 硬失败。**别写「老服务端会回落 v1，能用」——那只在老服务端主动回显 `iroh-relay-v1` 时才成立。**

## 尺寸上限

- `protos/relay.rs:23` `pub const MAX_PACKET_SIZE: usize = 64 * 1024;`（服务端在 `streams.rs:129-133` 强制 `ensure!(size <= MAX_PACKET_SIZE, ...)`）
- `protos/relay.rs:25-29` `pub(crate) const MAX_FRAME_SIZE: usize = 1024 * 1024;`（*"This is also the minimum burst size that a rate-limiter has to accept."*；客户端 `client.rs:314` 与服务端 `http_server.rs:861` 都用它设 `tokio_websockets::Limits::max_payload_len`）

**⚠️ `max_burst_bytes` 若配得比 1MiB 小，会和 MAX_FRAME_SIZE 打架**（注释明写「minimum burst size that a rate-limiter has to accept」）。

---

# 第四部分：自建 relay

**入口**：`iroh/iroh-relay/src/main.rs`（**Config struct 就是 TOML schema 的唯一真相**：:88-156 顶层字段，:394-457 `[tls]` 段，:536-564 加载逻辑，:689-781 `build_relay_config` 是「哪些组合合法」的判据）。

## TLS 不是必需的

```rust
// main.rs:119-127
/// TLS specific configuration.
/// TLS is disabled if not present and the Relay server will serve all services over plain HTTP.
/// ...
/// Must exist if `enable_quic_addr_discovery` is `true`.
tls: Option<TlsConfig>,

// main.rs:350-364 —— Default
enable_relay: true,
http_bind_addr: None,                    // → [::]:80 (main.rs:339-342)
tls: None,
enable_quic_addr_discovery: false,
enable_metrics: true,
access: AccessConfig::Everyone,

// main.rs:537-549 —— 无 config 路径直接 default
async fn load(opts: &Cli) -> Result<Self> {
    let config_path = if let Some(p) = &opts.config_path { p } else { return Ok(Config::default()); };
    if config_path.exists() { Self::read_from_file(&config_path).await } else { Ok(Config::default()) }
}
```

佐证：`main.rs:936-957` 的测试 `test_enable_relay_config` 用 `let config = "";`（**完全空配置**）调用 `build_relay_config(config).await?` 后 `assert!(relay_config.relay.is_some())`。

**所以最小可用自建 relay = 跑个二进制、开 80 端口、客户端用 `http://host` —— 零 TLS、零证书、零配置文件。官方文档说的 "public IP and DNS name" 只对 Let's Encrypt 成立。**

## ⚠️ 关键代价：没有 `[tls]` 段 ⇒ 没有 QAD

**QAD（QUIC Address Discovery）是 net_report 里唯一能产出公网地址的探测**（HTTPS 探测只测延迟不产地址），且它**强制要 TLS**：

```rust
// main.rs:585-586
if cfg.enable_quic_addr_discovery && cfg.tls.is_none() {
    bail_any!("TLS must be configured in order to spawn a QUIC endpoint");
}
// main.rs:597-598 —— 第二道同义检查（措辞不同）
if cfg.tls.is_none() && cfg.enable_quic_addr_discovery {
    bail_any!("If QUIC address discovery is enabled, TLS must also be configured");
}
// server.rs:844-849 —— 库层：拿不到 server_config 就 QuicSpawnError::TlsNotConfigured
```

而 `enable_quic_addr_discovery` **默认 false**。

**为什么 QAD 重要**（`iroh/iroh/src/net_report/report.rs`）：:31-34 `pub global_v4: Option<SocketAddrV4>` 的 doc 是 *"The discovered global IPv4 address and port, if any."*；:68-128 `Report::update` 的 match 只有三个臂：`ProbeReport::Https(..)` **仅更新 `relay_latency`，不碰 global_v4/v6**；只有 `QadIpv4/QadIpv6` 才写 `self.global_v4 = Some(ipp)` / `self.udp_v4 = true`。

而 QUIC 自己的 ObservedAddr 事件**被显式忽略**（`socket/remote_map/remote_state.rs:632-634`）：

```rust
NoqPathEvent::RemoteStatus { .. } | NoqPathEvent::ObservedAddr { .. } => {
    // Nothing to do for these events.
}
```

即便 `endpoint/quic.rs:446/457` 提供了 `send/receive_observed_address_reports` 开关，iroh 1.0.2 也没拿它做地址发现。

> ⚠️ **别把这条说成「QAD 挂了就永远不可能直连」** —— 那是错的。`socket.rs:1821-1841` 的 `update_direct_addresses()` 依次装配**四类**候选：**portmapper**（`DirectAddrType::Portmapped`，**与 QAD 完全独立**）/ `global_v4/v6`（`Qad`）/ `collect_local_addresses()`（`Local`）/ `configured_addrs`（`Config`）。**准确表述**：拿不到 QAD 会**显著降低直连率**，不是「永远不可能直连」。详见 [nat-and-transports.md](nat-and-transports.md)。

**对文件传输**：中继流量就是真金白银的带宽费。没 QAD → 直连率塌陷 → 1GB 文件老老实实从你的机器进 1GB 出 1GB。

## ⚠️ `--dev` 到底做了什么（**两份旧资料在这里互相矛盾，以下按源码逐行核对**）

**`--dev` 不忽略 TLS 段，也不会杀掉 QAD。** 顺序如下（`main.rs`）：

```rust
:585-586  if cfg.enable_quic_addr_discovery && cfg.tls.is_none() {
              bail_any!("TLS must be configured in order to spawn a QUIC endpoint"); }
:588      if cli.dev {
:591          if let Some(ref mut tls) = cfg.tls { tls.dangerous_http_only = true; }
:594          if cfg.http_bind_addr.is_none() { cfg.http_bind_addr = Some((.., DEV_MODE_HTTP_PORT).into()); }
          }
:597-598  if cfg.tls.is_none() && cfg.enable_quic_addr_discovery {
              bail_any!("If QUIC address discovery is enabled, TLS must also be configured"); }
```

然后 `build_relay_config`（:698-716）里 `dangerous_http_only` 分支的注释原文：

> *"we disable HTTPS by setting `RelayConfig::tls` to `None`. **We still enable the QUIC server, and thus pass the TLS config from the loaded TLS config only to the QUIC server**"*

返回 `(None, quic_config)`，且 :709 会 `bail_any!("--dev is incompatible with cert_mode LetsEncrypt")`。

**所以三条精确结论**：

| 组合 | 结果 |
|---|---|
| `--dev` + QAD + **无** `[tls]` 段 | **bail**（:586 先撞上） |
| `--dev` + `[tls]`(cert_mode=**Manual**) + QAD | ✅ **可用** —— 明文 HTTP relay（端口钉死 3340）+ **QAD 正常工作**。这是「明文 relay + 可用 QAD」的**唯一**组合 |
| `--dev` + cert_mode=**LetsEncrypt** | **bail**（:709） |

⚠️ **但它是 dev-only 逃生门，不是生产方案**：`dangerous_http_only` 的 doc（`main.rs:449-452`）明写 *"This field should never be manually set"*，且 `--dev` 会把 http 端口钉到 3340（`main.rs:34 DEV_MODE_HTTP_PORT`）。

> 📌 **别信「`--dev` 与 QAD 是互斥死路」这个说法** —— 它只在「没配 TLS」时成立。配了 Manual 证书时 `--dev` 恰恰是唯一能同时拿到明文 HTTP + QAD 的路。

## 生产推荐：`cert_mode = "Reloading"`

```toml
# 顶层 key 必须写在 [tls] 之前，否则被 TOML 吃进 tls 段
enable_quic_addr_discovery = true   # UDP 7842；必须有 tls 否则 main.rs:585 直接 bail

[tls]
cert_mode = "Reloading"             # main.rs:673-683 -> server/resolver.rs:37
manual_cert_path = "/etc/ssl/relay.crt"
manual_key_path  = "/etc/ssl/relay.key"
https_bind_addr  = "[::]:443"
```

### 为什么不用内置 Let's Encrypt

**它只走 TLS-ALPN-01（443 入站），不走 HTTP-01**：

- `iroh-relay/Cargo.toml` 依赖 `tokio-rustls-acme = "0.9"`；`iroh/Cargo.lock:5042-5043` 锁定 0.9.1
- 该 crate 的 `acme.rs:187-191`：`.find(|c| c.typ == ChallengeType::TlsAlpn01)` 找不到就 `return Err(AcmeError::NoTlsAlpn01Challenge)`
- `acme.rs:24` `pub const ACME_TLS_ALPN_NAME: &[u8] = b"acme-tls/1"` —— **只实现 TLS-ALPN-01，没有 HTTP-01 路径**

**即：LE 验证服务器必须能从公网打进你的 443。** 入站 443 不可靠（被墙、被运营商拦、在 NAT 后）的机器上，这是最不可靠的一环 —— 改用外部 acme.sh 走 **DNS-01** 签证书 + Reloading 模式即可完全绕开入站验证。

### Reloading 模式做什么

`main.rs:673-683`：`CertMode::Reloading => { let resolver = reloading_resolver(server_config.crypto_provider(), tls.cert_path(), tls.key_path(), DEFAULT_CERT_RELOAD_INTERVAL).await?; ... }`

`server/resolver.rs:19` `pub const DEFAULT_CERT_RELOAD_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);`（**24h**）

resolver.rs:21-31 doc：*"...then spawns a background task that re-reads both files every interval. The returned resolver hands the most recently loaded CertifiedKey to rustls for each TLS handshake, so certificate rotation takes effect without restarting the server."* 以及 ⚠️ *"**Reload failures on the interval are silently ignored**; the previously loaded certificate remains in use."*

**优势**：避开 LE 的 443 入站验证依赖 · 拿到浏览器认可的真 TLS（Web 端的前提）· 换证书不用重启 relay，不打断在途传输。

**⚠️ 注意「静默失败」**：证书过期前的 reload 失败不会报错，**要自己加监控**。

> ⚠️ **CertMode 的文档不全**：`main.rs:62-68` `enum CertMode { Manual, LetsEncrypt, #[cfg(feature = "server")] Reloading }`，但 `main.rs:412` 的 doc 只写 *"Possible options: 'Manual', 'LetsEncrypt'."* —— **漏了 Reloading**。文档不全，但代码支持。

## ⚠️ 端口真相 —— 官方文档是错的

| 端口 | 用途 | 出处 |
|---|---|---|
| **80/tcp** | http | `iroh-relay/src/defaults.rs` |
| **443/tcp** | https | 同上 |
| **7842/udp** | **QAD** | `iroh-relay/src/defaults.rs:3-7` |
| **9090/tcp** | metrics（Prometheus） | `main.rs:344-347` + defaults.rs |

**唯一真值**（`iroh-relay/src/defaults.rs:3-7`）：

```rust
/// The default QUIC port used by the Relay server to accept QUIC connections
/// for QUIC address discovery
///
/// The port is "QUIC" typed on a phone keypad.
pub const DEFAULT_RELAY_QUIC_PORT: u16 = 7842;
```

（手机键盘 Q=7, U=8, I=4, C=2 —— 7842 自洽，也是个好记忆点。）

**两处官方文档都是错的**：
- `iroh-relay/README.md:138`：*"...it will also run a QUIC server on port **7824**."* —— **7824 ≠ 7842，数字转置**
- `iroh/docker/Dockerfile:41-44`：`EXPOSE 80 443 **3478**/udp 9090`（STUN 时代遗留）；`docker/README.md:21` 同样 `-p 3478:3478/udp`

**验证**：`grep -rn "3478" --include="*.rs" .` 在 iroh 仓**零命中**。消费侧对齐 7842：`relay_map.rs:304-310` `impl Default for RelayQuicConfig { fn default() -> Self { Self { port: DEFAULT_RELAY_QUIC_PORT } } }`。

**照抄官方 docker 命令 = QAD 静默不通 = 打洞退化。** relay 看起来跑得好好的、客户端也能连上中继，但 QAD 永远超时，你只会看到「打洞成功率莫名其妙很低」，排查成本极高。

HTTP 路由（`server.rs:736-740` + :824）：`/` `/index.html` `/ping` `/robots.txt` `/healthz` `/relay`(WebSocket) `/generate_204`(捕获门户)。

## ⚠️ 客户端默认开 QAD，服务端默认关 —— 两边默认值相反

```rust
// iroh-relay/src/relay_map.rs:230-246 + :272-310
pub struct RelayConfig { pub url: RelayUrl, #[serde(default = "quic_config")] pub quic: Option<RelayQuicConfig>, pub auth_token: Option<String> }
impl From<RelayUrl> for RelayConfig {
    fn from(value: RelayUrl) -> Self {
        Self { url: value, quic: quic_config(), auth_token: None }   // ← Some(default)！
    }
}
fn quic_config() -> Option<RelayQuicConfig> { Some(RelayQuicConfig::default()) }
```

`FromIterator<RelayUrl>` 走的也是这条（`relay_map.rs:203-217`，注释明说「The [`RelayConfig`]s in the [`RelayMap`] will have the default QUIC address discovery ports」）。

**症状**：用 `RelayMode::custom(["http://myrelay".parse()?])` 连自建 relay（服务端 `enable_quic_addr_discovery = false`），客户端会**白白往 7842/udp 发 QAD 探测、超时失败**——不致命，只是每次 net_report 都浪费 3s PROBES_TIMEOUT。

**要关掉必须显式构造 `RelayConfig::new(url, None)` 再 `RelayMap::from_iter`。**

## relay 服务器没有 iroh 身份

```rust
// iroh-relay/src/server.rs:105-119 / :121-147 —— 全无 secret_key 字段
pub struct ServerConfig { pub relay: Option<RelayConfig>, pub quic: Option<QuicConfig>, pub metrics_addr: ... }
pub struct RelayConfig { pub http_bind_addr: SocketAddr, pub tls: Option<TlsConfig>,
                         pub limits: Limits, pub key_cache_capacity: Option<usize>,
                         pub access: Arc<dyn DynAccessControl> }
```

**relay 是「按 EndpointId 转发密文」的哑管道，没身份也无需身份。** 客户端身份由 relay 的 handshake 校验（ServerChallenge → ClientAuth），**不是 mutual 的**。所以自建 relay 的信任模型 = 「相信它不做流量分析」，而不是「相信它的公钥」。**纯 HTTP 自建 relay 的 URL 可被中间人劫持**（虽然 payload 仍是端到端加密的）。

> ⚠️ **精确表述**：**服务端侧文件**（`server.rs` / `server/http_server.rs` / `main.rs`）的 SecretKey 命中全部落在 `#[cfg(test)] mod tests` 内。但**客户端侧**（`client.rs:328`、`protos/handshake.rs:225/254/342`、`client/conn.rs:90`）在非测试代码里正常使用 SecretKey 签 challenge——因为 iroh-relay 这个 crate 同时装着 relay 客户端和服务端。**别说成「SecretKey 在 iroh-relay 里只出现在测试代码」。**

> **libp2p 对照**：libp2p 的 relay 是个完整 libp2p 节点，有 PeerId，客户端 dial 时 multiaddr 里带 `/p2p/<relay-peer-id>` 且会做身份校验。**iroh relay 只是 URL，没有公钥钉扎**。运维心智也完全不同：libp2p relay 是 libp2p 身份 + noise 加密，**没有 web PKI / Let's Encrypt 那套**；iroh relay 是个 HTTP 服务器，TLS 是 web 那一套。

## 无 TLS 时客户端认证静默降级

```rust
// iroh-relay/src/protos/handshake.rs:251-269
impl KeyMaterialClientAuth {
    /// Generates a client's authentication ... by using TLS keying material instead of a received challenge.
    pub(crate) fn new(secret_key: &SecretKey, io: &impl ExportKeyingMaterial) -> Option<Self> {
        let key_material = io.export_keying_material(...)?;   // 无 TLS → None
        ...
    }
}
// handshake.rs:340-378 —— 拿不到 keying material 就走 challenge-response（多一个 RTT）
```

两条路径见 `protos/handshake.rs:8-25`：TLS keying material 走 RFC 5705 省一个 RTT，但 *"it relies on the keying material extraction feature of TLS, which is not available in browsers"*；否则回退 ServerChallenge/ClientAuth 签名挑战。

**这个降级是静默的、只有一行 debug 日志**（`client.rs:328-335`）。**纯 HTTP 自建 relay 能连上、能用，只是每次建连多一个 RTT。别以为「没配 TLS 但连上了」说明 auth 被跳过了——auth 一直在做，只是换了路子。**

## 自建 relay 不需要 DNS/pkarr 服务器

**iroh-dns-server 存在（v1.0.2），但它解决的是「只知道 EndpointId 反查地址」，与 relay 完全正交。**

`EndpointAddr`（`iroh-base/src/endpoint_addr.rs:42-62`）里 **relay url 本身就是地址的一种**（`TransportAddr::Relay(RelayUrl)`）。官方测试里跑通的完整链路（`endpoint.rs:2105-2114`）**全程无任何 address_lookup**：

```rust
let ep = Endpoint::builder(presets::Minimal)
    .relay_mode(RelayMode::Custom(relay_map))
    .alpns(...)
    .ca_tls_config(CaTlsConfig::insecure_skip_verify())   // ← 仅测试用！见下
    .bind().await?;
let endpoint_addr = EndpointAddr::new(server_peer_id).with_relay_url(relay_url);
let conn = ep.connect(endpoint_addr, TEST_ALPN).await?;
```

**若你的配对模型是「带外交换地址」（ticket / 扫码 / 配对握手），整套 pkarr/DNS 基础设施可以直接不部署。** 相比 libp2p 需要 DHT + bootstrap 才能找人，这是**净减法**。

### iroh-dns-server（若真需要）

- **成熟度**：production（version 1.0.2，与 iroh 主线同步发版；自带 `config.dev.toml` / `config.prod.toml`；`iroh/docker/Dockerfile` 有独立 build target；n0 用它跑生产 dns.iroh.link）
- **入口**：`iroh/iroh-dns-server/config.prod.toml`（**18 行就是全部生产配置面**）

```toml
pkarr_put_rate_limit = "smart"       # dev 用 "disabled"
[https]
port = 443
domains = ["irohdns.example.org"]
cert_mode = "lets_encrypt"           # dev 用 "self_signed"
letsencrypt_prod = true
[dns]
port = 53
default_soa = "dns1.irohdns.example.org hostmaster.irohdns.example.org 0 10800 3600 604800 3600"
default_ttl = 30
origins = ["irohdns.example.org", "."]
rr_a = "203.0.10.10"
rr_ns = "ns1.irohdns.example.org."
[mainline]
enabled = false                      # true 则同时读写 BitTorrent Mainline DHT
```

**何时不用**：① 只为自建 relay 而部署它 —— **纯属多余**；② 想复用 n0 的 dns.iroh.link 省事 —— 那台和 n0 relay 一样有可达性风险，且 `presets::N0` 会把它偷偷带进来（见下）。

## 客户端配置

```rust
use iroh::{Endpoint, EndpointAddr, RelayMode, endpoint::presets};
use iroh_relay::{RelayConfig, RelayMap, RelayQuicConfig, tls::CaTlsConfig};

// A. 明文 HTTP relay：URL 写 http:// 即可，客户端自己降级到 ws://
//    第二参 None = 不对这台 relay 做 QAD（relay_map.rs:238-241）
let map = RelayMap::from(RelayConfig::new("http://relay.example.com:80".parse()?, None));

// B. 自签 TLS relay（推荐）：把自己的 CA 钉进去
let map = RelayMap::from(
    RelayConfig::new(
        "https://relay.example.com.".parse()?,      // 结尾的 . 是 FQDN，官方建议
        Some(RelayQuicConfig::default()),           // QAD -> UDP 7842
    )
    .with_auth_token("my-secret"),
);

// C. 用 Minimal 而不是 N0
let ep = Endpoint::builder(presets::Minimal)
    .relay_mode(RelayMode::Custom(map))
    .ca_tls_config(CaTlsConfig::custom_roots(my_ca_certs))  // 只信我的 CA
    .alpns(vec![b"myapp/1".to_vec()])
    .bind().await?;
ep.online().await;                                  // 等 home relay 真连上

// D. 寻址完全不需要 DNS/pkarr —— relay url 就在地址里
let addr = EndpointAddr::new(peer_id).with_relay_url("https://relay.example.com.".parse()?);
let conn = ep.connect(addr, b"myapp/1").await?;

// E. 运行时换 relay，不重建 endpoint
ep.insert_relay(new_url, Arc::new(new_cfg)).await;
ep.remove_relay(&old_url).await;

// F. relay 连接态给 UI
let mut w = ep.home_relay_status();
```

### ⚠️ 绝对不要用 `CaTlsConfig::insecure_skip_verify()`

```rust
// iroh-relay/src/tls.rs:90-99
/// INSECURE: Do not verify server certificates at all.
/// May only be used in tests or local development setups.
#[cfg(any(test, feature = "test-utils"))]
pub fn insecure_skip_verify() -> Self
```

它被 `#[cfg(any(test, feature = "test-utils"))]` 门控 —— **生产构建里这个函数根本不存在**。

**这是最容易踩的坑**：抄 iroh 官方测试代码（`endpoint.rs:2067` 就是它）会在生产构建下编译不过，然后误以为要开 test-utils feature（那会**把整个 relay server 都链进客户端**，`iroh/Cargo.toml:151` `test-utils = ["iroh-relay/test-utils", "iroh-relay/server", "dep:axum"]`）。

**正解**：自签一张 CA，用 `tls.rs:101-107` `pub fn custom_roots(roots: impl IntoIterator<Item = CertificateDer<'static>>) -> Self`（`Mode::ExtraRootsOnly` → :185-191 建一个只含你的 CA 的空 RootCertStore），或 :141-146 `custom_server_cert_verifier`。挂到 endpoint：`endpoint.rs:713-716` `pub fn ca_tls_config(...)`。

⚠️ `endpoint.rs:718-722` 有 `#[deprecated(since = "1.0.0", note = "Renamed to ca_tls_config")] pub fn ca_roots_config(...)` —— **网上 1.0 前的示例用的是旧名**。

### ⚠️ presets::N0 会静默拖入三项 n0 基础设施

`presets.rs:116-140` 的 `impl Preset for N0` 装了 `PkarrPublisher::n0_dns()` + `PkarrResolver::n0_dns()` + (非 wasm)`DnsAddressLookup::n0_dns()`，**全部指向 dns.iroh.link**。

而 `endpoint.rs:557-577` 的 `relay_mode()` **只在 `self.transports` 里替换/移除 `TransportConfig::Relay { .. }`，完全不碰 `self.address_lookup`**。清空 lookup 必须显式调 `endpoint.rs:585-588` `pub fn clear_address_lookup(mut self) -> Self`。

**非常容易踩**：照抄示例写 `Endpoint::builder(presets::N0).relay_mode(RelayMode::Custom(my_map))`，自以为完全自建，实际上**每 5 分钟还在往 dns.iroh.link 发 pkarr**（`DEFAULT_REPUBLISH_INTERVAL`，`pkarr.rs:146`），表现为神秘的启动卡顿/超时，还把设备的 relay url 泄露给了第三方。

**自建基础设施时固定用 `presets::Minimal`。**
（注：默认发布过滤器是 `AddrFilter::relay_only()`，`pkarr.rs:168` —— 只发 relay url 不发直连 IP，泄露面有限但不是零。）

## 运行时可变

`RelayMap` 是 `Arc<RwLock<BTreeMap<RelayUrl, Arc<RelayConfig>>>>`（`relay_map.rs:29-33`）+ `insert`/`remove`（:131-139，`&self` 不需要 mut）。

`endpoint.rs:982-991` `pub async fn insert_relay(...)` / :996-1001 `pub async fn remove_relay(...)`；底层 `socket.rs:1241-1245` 会 `send(ActorMessage::RelayMapChange)` —— **主动通知 actor 重新评估选路**。

**实用能力**：relay 列表可做成远端可配（从更新服务拉），relay 换 IP/加节点**不用发版**。

服务端限流也能热改：`http_server.rs:938-939` `pub fn set_client_rate_limit(&self, rate_limit: Option<ClientRateLimit>)`；`streams.rs:695` 的 `test_ratelimiter_live_update` 断言限流「**mid-connection 生效且不掉连接**」。

---

# 第五部分：准入控制与成本

## 准入控制（四档，1.0.0 已 GA）

```rust
// main.rs:158-197
enum AccessConfig {
    Everyone,                          // default
    Allowlist(Vec<EndpointId>),
    Denylist(Vec<EndpointId>),
    Http(HttpAccessConfig),
    #[serde(rename = "shared_token")]
    SharedToken(Vec<String>),
}
```

TOML 形状（`main.rs:829-909` 的测试）：`access = "everyone"` / `access.allowlist = [...]` / `access.http.url = "..."` / `access.shared_token = ["token-a", "token-b"]`。

**EndpointId 不可伪造**（`server.rs:226-231` doc）：*"The relay handshake authenticates this id before the access hook is invoked. The client proves possession of the secret key for this public key by either signing keying material exported from the TLS session or a challenge issued by the server."*

环境变量覆盖：`main.rs:38-40` `IROH_RELAY_HTTP_BEARER_TOKEN` / `IROH_RELAY_ACCESS_TOKEN`（:231-241 env 版单 token 覆盖整个列表，空 token 启动即失败）。

**吊销限制**（`README.md:84`）：*"**Note:** this shared token does not support revocation other than updating the config and restarting the service."* 需要动态吊销就自己实现 `AccessControl`（`server.rs:285-305` trait，含 `on_connect` + `on_disconnect(endpoint_id, connection_id)`），完整可运行范例见 `iroh/iroh-relay/tests/runtime_auth.rs`。

### ⚠️⚠️ access.http 的 header 名：文档和代码互相矛盾

| | 值 |
|---|---|
| **代码实际发出的** | `X-Iroh-NodeId` —— `main.rs:36` `const X_IROH_ENDPOINT_ID: &str = "X-Iroh-NodeId";`，`main.rs:319` `.header(X_IROH_ENDPOINT_ID, endpoint_id.to_string())` |
| **rustdoc 说的** | `X-Iroh-Endpoint-Id` —— `main.rs:168-170` `AccessConfig::Http` 的 rustdoc |

**照 rustdoc 去实现鉴权服务、按 `X-Iroh-Endpoint-Id` 取 header 的人，会拿到 None 然后拒绝掉每一个连接** —— 而且因为 relay 侧只会打 warn、鉴权服务侧看起来「工作正常」，**这是个排查成本极高的坑**。

（注意常量**名**叫 ENDPOINT_ID 但线上 header **字面量**是 X-Iroh-NodeId —— 双重迷惑。）

callout 语义：relay 每来一个连接就 POST 你的服务，你回 200 + 文本 `true` 才放行（`main.rs:329-333` 严格判等 `text == "true"`）。

## 成本模型

### relay 中转 1GB = ingress 1GB + egress 1GB

1 进 1 出，**无压缩无去重**。`metrics.rs:13-24` 两个独立计数器：`bytes_sent` 与 `bytes_recv`。

### 默认完全不限流，且限流只限「客户端→relay」

```
// streams.rs:326-331
/// Rate limiter for reading from a [RelayedStream].
/// The writes to the sink are not rate limited.
```

代码印证：:553-605 `impl AsyncRead for RateLimited` 里有 bucket 消费，而 :608-630 `impl AsyncWrite for RateLimited` 三个方法**全是直穿**，**无 bucket**。

默认无限流：`server.rs:487-500` `#[derive(Debug, Default)] pub struct Limits { pub client_rx: Option<ClientRateLimit>, ... }` → `client_rx: None`。

桶参数（`streams.rs:417-430`）：`max_burst_bytes` 默认 = `bytes_per_second / 10`，refill 周期 100ms。

**唯一的限流开关**：

```toml
[limits.client.rx]
bytes_per_second = 5_000_000
max_burst_bytes  = 500_000          # 不写则默认 = bytes_per_second/10
```

**没有任何按连接/按会话的字节总量上限。**

### ⚠️ 陷阱：`accept_conn_limit` / `accept_conn_burst` 是死配置

TOML 能解析、类型能通过、`build_relay_config` 会赋值（`main.rs:754-755`），但**服务端从不读取**：

```rust
// server.rs:485-500
/// Rate limits.
// TODO: accept_conn_limit and accept_conn_burst are not currently implemented.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Limits {
    pub client_rx: Option<ClientRateLimit>,
    /// Rate limit for accepting new connections. Unlimited if not set.
    /// Not currently implemented, setting this has no effect.
    pub accept_conn_limit: Option<f64>,
    /// Not currently implemented, setting this has no effect.
    pub accept_conn_burst: Option<usize>,
}
```

下游 `server.rs:741-743` **只消费 `relay_config.limits.client_rx`**。

**按官方 TOML 字段名配了 `accept_conn_limit` 就以为挡住了连接洪水 —— 那是纸糊的。** 连接数配额只能靠 relay 前面的 nginx/iptables 或云厂商限速。

> **libp2p 对照**：libp2p circuit relay 有 `max_reservations` / `max_circuits` / `max_circuits_per_peer` / `max_circuit_bytes` 这类配额。**iroh-relay 一个连接数配额的等价物都没有**，只有 `limits.client.rx.bytes_per_second` 这一个字节速率开关。

### 可观测：Prometheus

`server.rs:700-710` metrics server 挂在 `config.metrics_addr`，默认 `[::]:9090`。**9090 的 `bytes_sent`/`bytes_recv` 直接接 Prometheus 就是你的账单曲线。**

## 浏览器 = 编译期 relay-only

`socket/transports.rs:99-117` 的 `TransportConfig::Ip` 变体带 `#[cfg(not(wasm_browser))]` —— **浏览器下这个变体不存在**。

鉴权路径分叉：wasm 走 `?token=`（`client.rs:407-411`），原生走 `Authorization: Bearer`（:320-326）。doc（:240-243）：*"When compiled to WebAssembly the token is sent as a ?token=TOKEN query parameter on the upgrade URL, since browsers don't allow setting headers on WebSocket requests."*

**两个硬结论**：
1. 浏览器 100% 走 relay、**永远没有直连兜底** —— Web 端的每一个字节都是你的带宽账单
2. `?token=` 会出现在 URL 里（可能进代理日志/浏览器历史），**shared_token 别当长期凭据用**，配合 `access.http` callout 做短期 token 更稳

另：**HTTPS 页面连 `ws://` 会被浏览器 mixed-content 拦截**（这是浏览器平台规则，源码里无对应证据）—— **Web 端一上马，明文 relay 方案直接出局。** 详见 [wasm-and-browser.md](wasm-and-browser.md)。
