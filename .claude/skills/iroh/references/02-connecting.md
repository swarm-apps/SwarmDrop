# Connecting：怎么连上

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Connecting](https://docs.iroh.computer/connecting/) 分区：Creating an Endpoint /
DNS / DHT / mDNS (Local) / Endpoint Hooks / Gossip Broadcast。

> **心智模型（relay 是什么、没有 DHT、NAT 穿透怎么回事）** → [01-concepts.md](01-concepts.md)。
> **连上之后传什么** → [03-protocols.md](03-protocols.md)。**自建基础设施的配置** → [07-configuration.md](07-configuration.md)。
>
> **路径提醒**：iroh crate 在 `iroh/iroh/`（比直觉多一层）。`endpoint.rs` 4121 行，`endpoint/presets.rs` 184 行。
> **`Drop for EndpointInner` 在 `socket.rs:220`，不在 `endpoint.rs`**；`BindError` 枚举定义在 `socket.rs:844-870`，也不在 `endpoint.rs`。
> workspace 里还有 `iroh-base` / `iroh-relay` / `iroh-dns` / `iroh-dns-server`，grep 极易串台
> （例如 `install_default` 的搜索结果**全部**来自 iroh-relay / iroh-dns-server 的二进制，与 Endpoint 无关）。

---

# 1. Creating an Endpoint

## Preset 是什么：一坨 Builder 链式调用的打包

`Preset` trait 只有一个方法（`iroh/iroh/src/endpoint/presets.rs:21-24`）：

```rust
pub trait Preset {
    /// Applies the configuration to the passed in [`Builder`].
    fn apply(self, builder: Builder) -> Builder;
}
```

没有任何魔法。`Builder::new(preset)` 就是 `Builder::empty().preset(preset)`，且 `preset()` 可重复调用叠加（`endpoint.rs:180-188`）：

```rust
pub fn new(preset: impl Preset) -> Self {
    Self::empty().preset(preset)
}
pub fn preset(mut self, preset: impl Preset) -> Self {
    self = preset.apply(self);
    self
}
```

**语义是「先应用 preset、后续 setter 可覆盖」**——setter 就是 `self.field = Some(...)` 直接赋值。所以 `Endpoint::builder(presets::N0).crypto_provider(x)` 能覆盖 N0 装的 provider（`endpoint.rs:761`）。这一点很重要，见下文 aws-lc-rs 那节。

> **libp2p 对照**：libp2p 的 `SwarmBuilder` 是类型状态机，每步必须显式给 transport/security/muxer，少一步**编译不过**。iroh 的 preset 只是把默认打包成一个 unit struct，且约束下放成运行时 `Option` 检查——**Empty 能编译通过，只在 `.bind().await` 时炸**。iroh 自己的 `bench/src/iroh.rs:37` 注释原话就是「// allow building, but fail at runtime」。

## 四个 preset 逐行拆解

### `Empty`：恒等函数 → `bind()` 必然失败

```rust
// presets.rs:36-43
#[derive(Debug, Copy, Clone, Default)]
pub struct Empty;

impl Preset for Empty {
    fn apply(self, builder: Builder) -> Builder {
        builder          // ← 原样返回，一个字段都不设
    }
}
```

文档自己写死了（`presets.rs:26-35`）：「This doesn't set mandatory builder options, so using this in `Endpoint::bind(presets::Empty)` will always fail.」

**Empty 不带 `#[cfg]`**，任何 feature 组合下都可用。官方唯一的正确用法是 `iroh/iroh/examples/pq-only-key-exchange.rs`：`Endpoint::builder(Empty).crypto_provider(pq.clone())`。想要「最小但保证能跑」用 `Minimal`。

### `Minimal`：只设 crypto_provider

```rust
// presets.rs:57-79
#[cfg(with_crypto_provider)]
pub struct Minimal;

#[cfg(with_crypto_provider)]
impl Preset for Minimal {
    fn apply(self, mut builder: Builder) -> Builder {
        #[cfg(feature = "tls-ring")]
        { builder = builder.crypto_provider(Arc::new(rustls::crypto::ring::default_provider())); }

        #[cfg(all(feature = "tls-aws-lc-rs", not(feature = "tls-ring")))]
        { builder = builder.crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider())); }

        builder
    }
}
```

**不碰 relay、不碰 address lookup。** 配合 `Builder::empty()` 的默认底座，得到的是「IPv4+IPv6 UDP socket、无 relay、无发现服务」的孤岛 endpoint，只能靠手工 `EndpointAddr` 直连。

iroh 自己的测试**以 Minimal 为主**：测试模块（起于 `endpoint.rs:1995`，实际约 2126 行测试）里 `presets::Minimal` 43 处、`presets::N0` **11 处**（2341/2350/2411/2458/2516/2560/2616/2666/3850/3957/3979），约 4:1。

### `N0`：Minimal + 3 个 address lookup + 默认 relay

```rust
// presets.rs:115-140
#[cfg(with_crypto_provider)]
impl Preset for N0 {
    fn apply(self, mut builder: Builder) -> Builder {
        builder = Minimal.apply(builder);
        builder = builder.address_lookup(PkarrPublisher::n0_dns());
        // Resolve using HTTPS requests to our DNS server's /pkarr path.
        builder = builder.address_lookup(PkarrResolver::n0_dns());
        // Additionally resolve using DNS queries outside browsers.
        #[cfg(not(wasm_browser))]
        { builder = builder.address_lookup(crate::address_lookup::DnsAddressLookup::n0_dns()); }
        builder = builder.relay_mode(default_relay_mode());   // presets.rs:136
        builder
    }
}
```

**N0 = 「用 n0 的公共基础设施」。`PkarrPublisher` 会把你的地址主动发布到 n0 的 dns.iroh.link——这是隐含的对外网络行为，不是纯本地配置。**

> 约等于 libp2p 里一次性配好 Kademlia(发现) + Identify + relay client + bootstrap 节点，但 iroh 走 pkarr/DNS 而非 DHT。

### `N0DisableRelay`：先跑完整 N0 再关 relay

```rust
// presets.rs:177-184
impl Preset for N0DisableRelay {
    fn apply(self, builder: Builder) -> Builder {
        N0.apply(builder).relay_mode(RelayMode::Disabled)
    }
}
```

`relay_mode()` 是「替换/移除」语义（`endpoint.rs:557-577`）：`Disabled` 走 `None` 分支 → `self.transports.retain(|t| !matches!(t, TransportConfig::Relay { .. }))`。

净效果：**没有 relay transport，但 address lookup 服务一个不少**。`N0DisableRelay ≠ 离线`，它依然联网发布地址。


> **`presets::Staging` 不存在** —— Staging 只是 `RelayMode` 的枚举变体，由 `IROH_FORCE_STAGING_RELAYS`
> 环境变量切入，且会**同时**切换 relay map 与 pkarr/DNS 端点。完整机制 → [07-configuration.md](07-configuration.md)。
>
> **三个 preset 都被 `#[cfg(with_crypto_provider)]` 门控**（= `any(feature="tls-ring", feature="tls-aws-lc-rs")`）。
> 关掉两个 tls-* feature 后这三个类型**直接从 API 里消失** → [07-configuration.md](07-configuration.md)。

## presets::N0 到底含什么（别记混）

`iroh/iroh/src/endpoint/presets.rs:81-87` 的 N0 preset **自述只包含三样**：
1. the DNS Address Lookup service
2. the default relay servers provided by Number 0
3. CryptoProvider（ring / aws-lc-rs）

**QAD 与 portmapper 都不属于 preset**：
- portmapper 来自 `Cargo.toml:148` 的 default feature + `PortmapperConfig::default() == Enabled{}`（`portmapper.rs:36-38`）
- QAD 是 net_report 针对 relay 跑的探测

净效果（用 `presets::N0` 就能同时拿到这三样）是对的，但**要分清 preset 与 socket/feature 默认值两层**。

> ⚠️ **N0 preset 还带 DNS Address Lookup** —— 这恰恰是要决策的一项（是否依赖 n0 的 pkarr/DNS 基础设施）。详见 `07-configuration.md` 的「presets::N0 会静默拖入三项 n0 基础设施」。

## crypto_provider：显式依赖注入，`install_default()` 是 no-op

这是整个 skill 里最容易被 libp2p 习惯带沟里的一条。

### 事实：iroh 全程 `builder_with_provider`，从不查进程默认

iroh crate 里 rustls config 的**全部**构造入口只有两处，都是 `builder_with_provider`：

```
tls.rs:81:  let mut crypto = rustls::ClientConfig::builder_with_provider(self.crypto_provider.clone())
tls.rs:109: let mut crypto = rustls::ServerConfig::builder_with_provider(self.crypto_provider.clone())
```

全 workspace 搜 `install_default|get_default|process_default|CryptoProvider::get`，**在 iroh crate 零命中**——只命中 relay 服务端和 dns 服务端二进制：

```
iroh-relay/tests/relay_hyper.rs:98    (测试)
iroh-relay/tests/relay_axum.rs:209    (测试)
iroh-relay/src/main.rs:580            (relay 服务器 bin)
iroh-dns-server/src/main.rs:25        (dns 服务器 bin)
```

连 CA 校验链也是显式传参（`iroh-relay/src/tls.rs:204-206`）。provider 从 `bind()` 一路显式传穿三处（`endpoint.rs:240-244` TlsConfig::new / `:233` RustlsTokenKey::new / `:263` ca_tls_config.client_config），覆盖 iroh 连接自身 TLS、relay 的 HTTPS、pkarr 发布的 HTTPS。

**结论：「装了进程默认还是 InvalidCryptoProvider」不是 bug，是设计。** `install_default()` 只影响那些**不显式传 provider** 的第三方 rustls 调用方——iroh 不属于此列。

### 正确的 libp2p 对照（常见误传，请勿写反）

libp2p-tls **也不读**进程默认 provider。核对 libp2p-tls 0.6.2（`~/.cargo/registry/src/index.crates.io-*/libp2p-tls-0.6.2/src/lib.rs`）：

```rust
:48  let mut provider = rustls::crypto::ring::default_provider();   // ← 构造函数，不是 get_default()
:56  ClientConfig::builder_with_provider(provider.into())
:75  let mut provider = rustls::crypto::ring::default_provider();
:83  ServerConfig::builder_with_provider(provider.into())
```

与 iroh 的 `tls.rs:81/109` 是**完全同一个模式**。且 `grep -rl "install_default" ~/.cargo/registry/src/*/libp2p-*/src/` **零命中**；libp2p-quic 0.13.1 整个 src/ 对 CryptoProvider/install_default/get_default 也零命中（它把 TLS 委托给 libp2p-tls）。

> 讽刺的是 iroh 自己的 `Minimal` preset（`presets.rs:68`）调用的就是同一个 `rustls::crypto::ring::default_provider()`。

**真实差异**：libp2p-tls 把 ring **硬编码在内部**（用户根本无从提供 provider，也无从换 aws-lc-rs），iroh 则要求用户经 builder 显式注入。**两者都不读进程默认**，所以「libp2p 缺了要 install_default()」这个说法不成立。


> `tls-ring` 与 `tls-aws-lc-rs` 的取舍、以及「想真用 aws-lc-rs 要不要 `default-features = false`」
> → [07-configuration.md](07-configuration.md)。


## `bind()` 完整流程（`endpoint.rs:225-308`）

顺序固定：

1. `secret_key` 缺省则**随机生成**（`:226`）
2. 取 `crypto_provider`，无则 `InvalidCryptoProvider`（`:228-230`）
3. `RustlsTokenKey::new`，不兼容则同错（`:232-235`）
4. 建 tracing span
5. `TlsConfig::new` + `make_server_config`/`make_client_config`
6. `StaticConfig` + `create_server_config(alpns)`
7. `dns_resolver` 缺省
8. `ca_tls_config.client_config`（`:263`）
9. 组 `socket::Options`
10. `EndpointInner::bind()` 真正绑 socket / 建 QUIC endpoint
11. 包成 `Endpoint`（`:293-295`）
12. **最后**才装 addr_filter 与各 address lookup 服务（`:297-305`）

**`secret_key` 不给就每次随机生成新身份，EndpointId 随之变——持久化身份必须显式 `.secret_key()`。**

address lookup 服务在 endpoint 绑好之后才构造（因为 `into_address_lookup(&ep)` 需要 `&Endpoint`），这是个真实的循环依赖处理。

> **libp2p 对照**：libp2p 的 `Swarm::listen_on()` 是 build 之后单独调用，且返回前不保证绑定成功（靠 `SwarmEvent::NewListenAddr` 异步确认）；iroh 的 `bind().await` **返回即代表 socket 已绑好**，失败直接是 `Err`。

### `Builder::empty()` 的默认底座

```rust
// endpoint.rs:190-219（节选，中间有 hooks/portmapper_config/net_report_config 等字段）
pub fn empty() -> Self {
    let transports = vec![
        #[cfg(not(wasm_browser))]
        TransportConfig::default_ipv4(),
        #[cfg(not(wasm_browser))]
        TransportConfig::default_ipv6(),
    ];
    Self {
        // ...
        transports,                    // :212
        // ...
        crypto_provider: None,         // :217
        // ...
    }
}
```

`:190` 的 doc 原文：「Creates an empty builder with no address lookup services, and [`RelayMode::Disabled`]」。

**「Empty」这个名字只指 preset 不写任何字段，不代表 builder 是空的**——底座仍会绑 `0.0.0.0` 和 `[::]` 两个 socket。要改得用 `clear_ip_transports()` + `bind_addr()`（`endpoint.rs:353-358` 有官方示例）。IPv6 那个 socket **允许绑失败而不报错**（`:319-330` doc：「This bind is allowed to fail however」）。

> libp2p 里不 `listen_on` 就完全不监听；iroh 里「什么都不配」= 已经在所有网卡上监听 UDP 随机端口了，默认行为更激进。

## BindError：8 个变体，`#[non_exhaustive]`

定义在 **`socket.rs:844-870`**（不在 endpoint.rs），用的是 `n0_error` 的 `#[stack_error(derive, add_meta)]`，**不是 thiserror**——`e!`/`bail!`/`anyerr!` 都是 n0_error 宏。

| 变体 | 抛出点 | 真实含义 |
|------|--------|---------|
| `Sockets { source: io::Error }` | `socket.rs:946` | `Transports::bind` 失败（端口占用等） |
| `CreateQuicEndpoint` | `socket.rs:1033` | `noq::Endpoint::new_with_abstract_socket` 失败 |
| `CreateNetmonMonitor` | `socket.rs:1037` | `netmon::Monitor::new` 失败 |
| `InvalidTransportConfig` | `socket.rs:908` | **配了 >1 个 relay transport**（`socket.rs:906` 注释「Currently we only support a single relay transport」）——名字完全看不出来 |
| `InvalidCaRootConfig` | `endpoint.rs:264` | CA 根证书配置 |
| `AddressLookup` | `endpoint.rs:303` 的 `?` | address lookup 服务构造失败 |
| `InvalidCryptoProvider` | `endpoint.rs:230` **和** `:234` | **两个语义**，见下 |
| `TlsConfigError` | `endpoint.rs:246/247` 的 `?` | TLS 配置 |

### `InvalidCryptoProvider` 的两个触发点语义不同

```rust
// endpoint.rs:228-230 —— 「没设」
let crypto_provider = self.crypto_provider
    .ok_or_else(|| e!(BindError::InvalidCryptoProvider))?;

// endpoint.rs:232-235 —— 「设了但不兼容」
let token_key = Arc::new(
    RustlsTokenKey::new(&mut rand::rng(), &crypto_provider)
        .ok_or_else(|| e!(BindError::InvalidCryptoProvider))?,
);
```

错误信息刻意写成 `"Missing or incompatible rustls crypto provider configured"`（`socket.rs:863-864`）。

第二个触发点的判据是「provider 里找不到任何 TLS 1.3 cipher suite」（`tls/misc.rs:25-41`：`.filter_map(|suite| suite.tls13()).next()?`）。

**自定义裁剪版 provider 若把 TLS 1.3 suite 剔光，会得到和「忘了设 provider」一模一样的错误变体**，排查时极易误判。另有第三条相关路径：provider 缺 `TLS13_AES_128_GCM_SHA256` 时报的是 `BindError::TlsConfigError → TlsConfigError::CryptoProviderNoInitialCipherSuite`（`tls.rs:130-135`）。

**第一个触发点在 `bind()` 的最前面**（只在 secret_key 生成之后），早于任何 socket/TLS 工作——所以 Empty 的失败是**瞬时**的，不会先绑端口再报错，也没有任何网络副作用。


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

## 观察 Endpoint 的状态

`Watcher` 的语义（**有损采样，不是事件流**）与 `initialized()` 的两个陷阱是地基层的事
→ [index-foundations.md](index-foundations.md)。这里只讲 iroh 自己暴露了什么、怎么用对。

## 1.0.2 的公开 Watcher API 只有三个

穷举 `grep "pub fn .*->.*Watch"` 在 `iroh/iroh/src/` 下：

| API | 位置 | Value 类型 |
|-----|------|-----------|
| `Endpoint::watch_addr()` | `endpoint.rs:1270`（非 wasm）/ `:1297`（wasm） | `EndpointAddr` |
| `Endpoint::home_relay_status()` | `endpoint.rs:1384` | `Vec<RelayStatus>` |
| `Endpoint::net_report()` | `endpoint.rs:1433`（feature `unstable-net-report` 门控） | `Option<NetReport>` |

> ⚠️ **1.0.2 没有 `direct_addrs()`。** `direct_addrs` / `direct_addresses` / `conn_type` 在公开 API 里**都不存在**。历史轨迹有据可查：`CHANGELOG.md:1717`「Rename Endpoint::local_endpoints to direct_addresses」；`:631`「[breaking] Make direct_addresses always be initialised (#3505)」；`:441`「Remove Endpoint::conn_type (#3647)」。**现在直连地址只能通过 `watch_addr()` 返回的 EndpointAddr 里的 `TransportAddr::Ip(..)` 间接看到。**

> libp2p 的 `swarm.listeners()` / `SwarmEvent::NewListenAddr` 直接给你监听地址列表；iroh 把「直连地址 + relay 地址」打包成单个 `EndpointAddr` 让你 watch，不单独暴露 direct addrs。


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

1. **`Connecting` 涵盖整个握手期**（doc：「Dialing **or performing the relay handshake**」），`Connected` 才是「handshaked」。这正是 `initialized()` 返回时刻与 `online()` 返回时刻之间的那段窗口（见 [index-foundations.md](index-foundations.md)）
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

`flatten()` 把 Option 的「无」压成了 Vec 的「空」——信息没丢，但正因如此 `Nullable<Vec<T>>` 才会把空 Vec 当 null，[index-foundations.md](index-foundations.md) 的陷阱链条就此闭合。

**公开文档措辞很准**（`endpoint.rs:1374-1376`）：「is empty when no relays are configured **or before the endpoint has selected a home relay**」——注意是「**selected**」而非「connected」。

⚠️ **空 Vec 有两种截然不同、无法区分的含义**：(a) 压根没配 relay（`RelayMode::Disabled`），(b) 配了但 net_report 还没跑出 preferred_relay。**启动阶段用 `.is_empty()` 判断「relay 没开」会误判。想等「上线」请直接用 `Endpoint::online()`。**

## `home_relay_status().initialized()` 为什么不能用来等连接

链条闭合于三处：

1. **`home_relay_status()` 的 Value 是 `Vec<RelayStatus>`**（底层 `Watchable<Option<RelayStatus>>` 经 Join + flatten，见 [07-configuration.md](07-configuration.md)），所以 `Nullable` 走 `pop()` 分支——**非空即返回**
2. **RelayActor 在拨号之前就发布 `Connecting`**（`socket/transports/relay/actor.rs:1138-1145`），`Connected` 要等 `run_dialing()` 返回后才写入（`:364-372`）
3. 于是 Vec 的状态序列是 `[]` → `[RelayStatus{Connecting}]` → `[RelayStatus{Connected}]`

`Endpoint::online` 的文档把这句话写死了（`endpoint.rs:1310-1312`）：

> This currently means at least one relay server has completed its connection handshake... **Merely selecting a relay URL is not sufficient.**

**`initialized()` 拿到的恰恰就是「只选了 URL」这个不充分状态。**

> ⚠️ **精确表述（别写「必然」）**：证据只证明了 set 调用的先后顺序，证明不了 `initialized()` **必定**观测到 `Connecting`。两个反例：(a) `InitializedFut` 的 `initial` 字段在 future **创建时就同步求值**（`lib.rs:312` `initial: self.get().into_option()`），若此时 relay 已连上，`get()` 直接返回 `[Connected]`，`initialized()` 立刻返回 `is_connected()==true`；(b) 依 last-value-wins 语义，watcher 未及时 poll 时 `Connecting` 可被 `Connected` 覆盖跳过（实践中拨号需网络 RTT，极少发生）。
>
> **准确说法：`initialized()` 在它观测到的第一个非空值处返回，而该值在典型时序下是 `Connecting`，故它对「是否已连上」不提供任何保证。** 注意这个准确版结论反而**更有力**——正因为不保证，才不能用它等连接。

> libp2p 里等 relay 可用是匹配 `SwarmEvent::Behaviour(relay::client::Event::ReservationReqAccepted{..})`——一个明确的「预约已被接受」事件；iroh 这里没有等价事件，你只能采样状态并自己判 `is_connected()`。

## 正确写法：`Endpoint::online()` + timeout

```rust
// endpoint.rs:1355-1370 —— 实现本身就是标准答案
pub async fn online(&self) {
    let mut watcher = self.inner.home_relay_status();
    let mut value = watcher.get();
    loop {
        if value.into_iter().any(|status| status.is_connected()) { return; }
        value = match watcher.updated().await {
            Ok(value) => value,
            Err(_disconnected) => {
                std::future::pending::<()>().await;   // ← 断开时显式永久挂起
                break;
            }
        }
    }
}
```

判据是 `any(|status| status.is_connected())` —— **至少一个 home relay 完成握手**。

**`online()` 自身没有超时，且两种情况下永久挂起：**
1. **没配 relay**（文档 `endpoint.rs:1314`：「If no relays are configured, this will pend forever.」）
2. watcher 断开时它显式 `std::future::pending::<()>().await`

**文档建议用接近 `NET_REPORT_TIMEOUT` 的超时包住**（`endpoint.rs:1316-1319`）。`NET_REPORT_TIMEOUT = 5`（秒，`iroh/iroh/src/net_report/defaults.rs:14`，`lib.rs:292` re-export）。

dumbpipe 的实践（`dumbpipe/src/main.rs:28, 364`）：

```rust
const ONLINE_TIMEOUT: Duration = Duration::from_secs(5);
if (timeout(ONLINE_TIMEOUT, endpoint.online()).await).is_err() {
    eprintln!("Warning: Failed to connect to the home relay");
}
```

⚠️ **两家超时后的行为不同，别搞混**：dumbpipe 5s **仅告警继续**；sendme 30s（`main.rs:731-736`）**硬失败**（`.await?` 把 Elapsed 传进 anyhow）。

> `online()` 是官方且推荐的写法，但不必说成「唯一正解」——它本身就是 `home_relay_status()` + `updated()` 循环 + `any(is_connected)` 的手写样板。iroh 自己的测试 `endpoint.rs:4090` 就是 `home_relay_status().stream()` 手写循环（判 `last_error()`）。

> **libp2p 对照**：对应 libp2p 里「循环 `swarm.next()` 直到匹配到 relay 预约成功事件」的那段样板；iroh 把它收敛成一个 `online()` 方法，所以自己手写 `initialized()` 版本纯属倒退。


## online() 超时行为 —— 两家不一样，别搞混

| | 超时 | 超时后 |
|---|---|---|
| **dumbpipe** | 5s（`main.rs:28 ONLINE_TIMEOUT`） | **仅告警继续** |
| **sendme** | 30s（`main.rs:731-736`） | **硬失败** —— `.await?` 把 Elapsed 传进 anyhow，整个 send 失败 |

**sendme 已经做了「超时就拒绝生成 invite」这个选择** —— 不需要重新发明。

### 4. 生成 ticket 前必须先 await online()

所有官方样例都带这个等待。dumbpipe 在全部 5 处生成 ticket 的路径上重复同一模式（`main.rs:364/472/550/648/770`）：

```rust
if (timeout(ONLINE_TIMEOUT, endpoint.online()).await).is_err() {
    eprintln!("Warning: Failed to connect to the home relay");
}
let addr = endpoint.addr();
```

注释（`main.rs:363`）：*"wait for the endpoint to figure out its home relay and addresses before making a ticket"*。

**两家超时行为不同**（别搞混）：
- dumbpipe：5s，**超时仅告警继续**
- sendme：30s（`main.rs:731-736`），**超时硬失败**（`.await?` 把 Elapsed 传进 anyhow）

> ⚠️ 「跳过 online() 就会产出连不上的废 invite」这个说法要加限定：id-only ticket **并非天生不可连** —— sendme `main.rs:660-662` 就是故意发 id-only ticket，靠 `PkarrPublisher::n0_dns()` 让它可用；`iroh/iroh/src/endpoint.rs:1036-1040` 也说没有地址的 EndpointAddr 仍可能靠 AddressLookup 连上。
>
> **准确表述**：跳过 online() 只在**没有配置 address_lookup / pkarr publisher** 时才产出废 invite。若这是你的默认配置，建议照 sendme 的做法：超时就拒绝生成 invite，别静默产出空 addrs。

## `watch_addr()` 中同一个陷阱

**EndpointAddr 里出现 relay 地址 ≠ 已连上 relay。**

```rust
// socket/transports/relay.rs:180-185 —— 只要 status 是 Some 就产出 (url, id)，完全不看 Connecting/Connected
pub(super) fn local_addr_watch(&self) -> RelayAddrWatcher {
    let my_endpoint_id = self.my_endpoint_id;
    self.my_relay.watch()
        .map(move |status| status.map(|status| (status.url().clone(), my_endpoint_id)))
}

// endpoint.rs:1270-1284 —— watch_addr 把它直接拼进 EndpointAddr
pub fn watch_addr(&self) -> impl n0_watcher::Watcher<Value = EndpointAddr> + use<> {
    let watch_addrs = self.inner.ip_addrs();
    let watch_relay = self.inner.home_relay();
    let endpoint_id = self.id();
    watch_addrs.or(watch_relay).map(move |(addrs, relays)| {
        EndpointAddr::from_parts(endpoint_id,
            relays.into_iter().map(TransportAddr::Relay)
                .chain(addrs.into_iter().map(|x| TransportAddr::Ip(x.addr))))
    })
}
```

而 status 在 `Connecting` 阶段就已经是 `Some`。

**「watch_addr 里已经有 relay 地址了 → 应该能被拨到了吧」是错的。** iroh 自己的文档也提醒先 `online()`（`endpoint.rs:1189-1192`）。

另外 **`Endpoint::addr()` 就是 `self.watch_addr().get()`**（`endpoint.rs:1196-1198`）——一个可能残缺的瞬时快照，早期调用会拿到不完整的 EndpointAddr。


> **wasm + `RelayMode::Disabled` 的真实失效模式**（静默产出零地址的 EndpointAddr，不是挂死）
> → [06-wasm-browser.md](06-wasm-browser.md)。

---

## 关闭

## `close()` 必须 await：它做了 4 件 Drop 做不到的事

`Endpoint::close()` 只是 `self.inner.close().await`；实质在 `socket.rs:1133-1197`。

**与 `abort()`（Drop 走的就是它，`socket.rs:1213-1234`）共享**这四步：`at_close_start.cancel()`、`address_lookup().clear()`、`at_endpoint_closed.cancel()`、`closed.store(true)`。

**真正「Drop 做不到」的只有 4 件**：

| # | 动作 | 位置 | 为什么关键 |
|---|------|------|-----------|
| ① | `noq_endpoint().close(0u16.into(), b"")` | `socket.rs:1146` | 排 QUIC CONNECTION_CLOSE 帧 |
| ② | **`wait_all_draining().await`** | `socket.rs:1164` | **把 CONNECTION_CLOSE 冲出去并等对端 ACK**（坏网络下 `:1156` 注释原话「usually 3 seconds」） |
| ③ | 100ms 优雅收尾 actor task | `socket.rs:1174` | 让任务干净退出 |
| ④ | `runtime.shutdown().await` 而非 `runtime.abort()` | `:1192` vs `:1228` | 等任务结束而非硬砍 |

②是关键。源码里 `wait_all_draining()` 上方有一段罕见的「祖训注释」（`socket.rs:1148-1153`）——这行代码被删过两次（PR #1753、#3165）、加回过两次，注释明确写「So before removing this call, please consider carefully.」

`Endpoint::close` 的文档（`endpoint.rs:1690-1699`）说得最直白：

> It is highly recommended you *do* wait for this close call to finish, if possible. Not doing so will make connections that were still open while closing the endpoint time out on the remote end. **Thus remote ends will assume connections to have failed even if all application data was transmitted successfully.**
>
> Note: Someone used to closing TCP sockets might wonder why it is necessary to wait for timeouts when closing QUIC endpoints... This is due to QUIC and its acknowledgments being implemented in user-land, while TCP sockets usually get closed and drained by the operating system in the kernel during the "Time-Wait" period.

> **libp2p 对照**：libp2p Swarm 没有 async close() 这回事——停止 poll / drop 掉 Swarm 就完事，优雅关闭靠上层协议自理。iroh 把「关闭」变成了必须 await 的一等公民操作。**从 libp2p 迁过来最容易漏的就是这个 await。**

### 不调 close() 的后果：`socket.rs:220`

```rust
// socket.rs:220-230 —— 注意在 socket.rs 不在 endpoint.rs
impl Drop for EndpointInner {
    fn drop(&mut self) {
        if self.sock.is_closed() { return; }
        tracing::error!("Endpoint dropped without calling `Endpoint::close`. Aborting ungracefully.");
        self.abort();
    }
}
```

`abort()` 的文档（`socket.rs:1199-1234`）明说「This does not wait for any current connections or tasks to close gracefully.」

**这条 `error!` 是 iroh 少见的用 error 级别喊话的地方——日志里看到它 = 你漏了 close()，不是 iroh 出错。** 它走 tracing，**app 没装 subscriber 就什么都看不到**，只会表现为对端莫名超时。

### 陷阱：给 close() 套 timeout 会两头落空

- `Drop` 的早退条件是 `is_closed()`（完全关闭，`socket.rs:222`）
- `abort()` 的早退条件是 `is_closed() || is_closing()`（`socket.rs:1214`）
- `is_closing()` 的置位点是 `close()` 的**第一步**（`socket.rs:1140` 的 `at_close_start.cancel()`）

所以：`close()` 已开始但 future 被中途丢弃（例如被 `tokio::time::timeout` 取消）→ Drop 照样打 `error!`，随后 `abort()` 却因 `is_closing()==true` 立刻 return，**什么都不做**。

**结论：要么完整 await close()，要么干脆别调直接 drop（至少 abort 会真的执行）。给 close() 套 timeout 然后超时放弃是最差的选择——同时失去优雅关闭和兜底 abort，只留一条误导性的 error 日志。**

## Endpoint 是 Clone：Drop 只在最后一个克隆落地时触发

```rust
// endpoint.rs:897-900
#[derive(Clone, Debug)]
pub struct Endpoint {
    inner: Arc<EndpointInner>,
}

// endpoint.rs:1703-1705 —— close 只要 &self，任何克隆都能关
pub async fn close(&self) {
    self.inner.close().await;
}
```

`Drop` 实现在 `EndpointInner` 上而非 `Endpoint` 上（`socket.rs:205`）。

**后果**：把 Endpoint clone 进 `tokio::spawn`（examples 里到处这么写），只要有一个克隆还活着，Drop 就不触发、`error!` 也不会打——endpoint 会**静默常驻**。反过来 `close()` 取 `&self`，任意一个克隆调用即可让所有克隆共享的 endpoint 关闭。

`close()` 文档最后一句还提醒：UDP socket 本身要等**所有 Endpoint 克隆都 drop** 才真正释放。

> **libp2p 对照**：libp2p Swarm 不是 Clone（要多处使用得靠 channel 把命令送给持有 Swarm 的那个 task）；iroh Endpoint 直接 Clone 共享，且自带后台 actor，不需要你手动 poll。这是 iroh 工程性上最舒服的地方之一。

## 生命周期：`close()` 不断开 watcher，只有 drop 才会

三个公开 Watcher API 的文档都重复了这段警告（`endpoint.rs:1235-1266` / `:1380-1383` / `:1412-1415`），说明这是踩过的坑：

```
/// ## Closing behavior
///
/// The returned watcher only becomes disconnected once the last clone of the [`Endpoint`]
/// is dropped. Closing the endpoint does not disconnect the watcher. Thus, a stream created
/// via [`Watcher::stream`] only terminates once the endpoint is fully dropped. To stop a task
/// that loops over a watcher stream once the endpoint stops, combine with [`Self::closed`]:
///
/// let mut addr_stream = endpoint.watch_addr().stream();
/// let endpoint_closed = endpoint.closed();
/// tokio::spawn(endpoint_closed.run_until(async move {
///     while let Some(addr) = addr_stream.next().await {
///         info!("our address changed: {addr:?}");
///     }
///     info!("endpoint closed");
/// }));
```

**`close()` 是「优雅关闭连接」，不是「释放 watcher」——两者生命周期是分离的。**

> libp2p 没有这个区分：Swarm 没了事件流就没了。iroh 的 Endpoint 是 Arc-clone 语义，`close()` 与 `drop()` 是两件事，**watcher 只认 drop**。


---

## 地址过滤与运行时装配

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

---

## 全栈装配参考

## 现成的全栈装配参考：tauri-todos

- `iroh-examples/tauri-todos` 是一个 **509 行**的 Tauri 应用，结构为 `src-tauri/src/{lib.rs, main.rs, ipc.rs, iroh.rs, state.rs, todos.rs}` + 根目录 vite.config.ts / tsconfig.json / package.json
- 依赖 tauri ^2 + iroh 1.0.0 + iroh-docs 0.101
- 它也用 `name = "tauri_todomvc_lib"` 规避 Windows lib/bin 命名冲突，注释 *"The _lib suffix may seem redundant but it is necessary to make the lib name unique and wouldn't conflict with the bin name. This seems to be only an issue on Windows"* —— **Tauri + Rust lib 的通用做法**
- `src/iroh.rs:20-49` 给出**持久化全栈装配**：`load_secret_key` → `Endpoint::builder(presets::N0).secret_key(key).bind()` → `Gossip::builder().spawn()` → `FsStore::load(&path)` → `Docs::persistent(path).spawn(endpoint, (*blobs).clone(), gossip.clone())` → `Router::builder().accept(BLOBS_ALPN/GOSSIP_ALPN/DOCS_ALPN).spawn()`

**即便最终不采用 docs/gossip，Endpoint 装配与 Router 注册那段可直接对照。**

---

# 2. DNS（pkarr relay + DNS TXT）

**这是 `presets::N0` 默认走的那条路。** 「默认状态下谁在替你解析、你信任了谁」的心智
→ [01-concepts.md](01-concepts.md) 的 Address Lookup 一节。

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

---

# 3. DHT（Mainline）

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

**含义**：只要坚持「一台设备一个 EndpointId」，无影响。但若设想过「同一身份多端登录」（同一 keypair 在手机和桌面同时跑），DHT 记录会被两端互相覆盖、来回抖动，且因 seq 取本地时钟微秒，**时钟不同步的两台设备会打架**（seq 大的赢）。**这是跨设备同步身份的明确禁区信号**（另见 [07-configuration.md](07-configuration.md) 的「同一 EndpointId 双连接互顶」）。

> **隐私代价**（源 IP 暴露给公共 BT bootstrap、lookup 泄露「你在找谁」、DHT 键 = SHA1(EndpointId) 无 salt）
> → [08-deployment.md](08-deployment.md) 的 Security & Privacy 一节。

---

# 4. mDNS（局域网）

**iroh 核心 crate 不含 mDNS** —— 在 iroh 仓执行 `grep -rni "mdns|swarm-discovery|local_swarm" --include="*.toml" iroh/` 返回**零结果**。能力位于独立仓。

README 开篇：*"This crate uses an mDNS-like swarm discovery service to find address information about endpoints on your local network — no relay or outside internet needed."*


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

## swarm-discovery（mDNS 的底层引擎）

- **成熟度**：**experimental**（`0.6.0-alpha.2`，Cargo.toml:3；依赖 `hickory-proto = "=0.26.0-beta.4"` 精确 pin 在 beta；HEAD 2026-04-15；`git tag` 为空）
- **是什么**：原作者 Roland Kuhn，本地 clone 是 n0 的 fork。**不是普通 mDNS** —— 它按算法自适应控制查询/响应频率（参数 τ 发现时间目标、φ 响应频率目标），使**带宽不随 swarm 规模膨胀**，同时维持稳定包流当 liveness 信号（这是 `Expired` 事件能工作的基础）

> ⚠️ **本地 clone 不是编译进产物的那份**。`iroh-address-lookups/Cargo.lock` 把 `swarm-discovery` 解析到 **0.6.1**（`source = "registry+..."`，checksum `36ae41d2...`）。本地这份 0.6.0-alpha.2 按 cargo 语义（`^0.6` 默认排除 pre-release）**不满足约束**。读本地源码排障时别默认它就是线上跑的那份。
>
> 同一份 Cargo.lock 也确认：n0-mainline → 0.5.0、iroh → 1.0.0，均来自 crates.io。

## mDNS 的被动模式与 subscribe/resolve 分流

- **`advertise(false)`**（lib.rs:177-183，默认 true 见 :171）→ `publish` 直接空转（:589-593）。**「我能发现别人，别人发现不了我」的隐身模式，零成本**。有测试覆盖：`non_advertising_endpoint_not_discovered`(:784-814)
- **resolve 有 10 秒硬超时**：`const LOOKUP_DURATION: Duration = Duration::from_secs(10);`（:94-95）
- ⚠️ **`subscribe()` 只推送被动发现的设备**：被 `resolve()` 显式解析到的**不会**推给 subscriber（:399-407 注释 *"only send endpoints to the `subscriber` if they weren't explicitly resolved"*，对应 `if !resolved { subscribers.send(...) }`）

**坑**：如果既用 `subscribe()` 维护「附近设备」列表、又对已知设备调 `endpoint.connect()`（内部触发 resolve），那么**这些设备可能不会出现在 subscribe 流里**，UI 列表缺项。**写列表时要合并两路来源。**


## ⚠️ 移动端 multicast 权限 —— 生态空白区

> **在全部 24 个仓库中均无任何文档或代码痕迹 —— 未找到。**

对 `/Volumes/yexiyue/iroh-study` 全目录 grep（`--include=*.rs,*.md,*.toml,*.xml,*.plist,*.kt,*.java,*.swift`）关键词 `multicast-networking|MulticastLock|CHANGE_WIFI_MULTICAST|com.apple.developer.networking.multicast|multicast entitlement` —— **零命中**。iroh-ffi / iroh-js 中 grep `mdns` 亦零命中（**官方 FFI 绑定压根不暴露 mDNS**）。iroh 主仓 CHANGELOG 里 mdns 相关条目只涉及 service_name、passive mode、expiry events，**无任何平台权限说明**。

客观事实：swarm-discovery 走 socket2 的 `join_multicast_v4`/`join_multicast_v6`（`swarm-discovery/src/socket.rs:139` 与 :200）监听 224.0.0.251 与 ff02::fb 的 5353 端口（:12-14），因此 **OS 层多播限制必然适用** —— 只是 n0 未记录。

**两个直接后果**：
1. **官方 FFI 不含 mDNS** → 要在 uniffi 桥里暴露局域网发现**必须自己写绑定，无先例可抄**。（对照组：`iroh-c-ffi/src/endpoint.rs:55` 的 `pub enum DiscoveryConfig { None, DNS, Mdns, All }` —— **C 绑定反而做到了**，可作为「该怎么暴露」的形状参考）
2. **iOS multicast entitlement 需向 Apple 单独申请**（非自助开关）、**Android 需 `CHANGE_WIFI_MULTICAST_STATE` + 运行期 `MulticastLock`** —— 都得自行验证

**「iOS/Android 真机 mDNS 能否收到包」是 iroh 生态里风险最高、信息最少的一块。**


> **这也是「BLE 对局域网有意义吗」的正面回答：不需要 BLE，mDNS 已覆盖发现层，且 BLE 在 iroh 生态根本无实现**
> → [04-transports.md](04-transports.md)。
>
> **mDNS 与 DHT 的默认过滤策略完全相反**（同时装两个时只有一半生效）→ [08-deployment.md](08-deployment.md)。

---

# 5. Endpoint Hooks（观察与拒绝入站）

**iroh 把「准入」做成钩子，不做成网络层策略。** 官方原文（`iroh/iroh/src/lib.rs:81-94`）：

> When accepting connections the peer's [`EndpointId`] is authenticated.
> **However it is up to the application to decide if a particular peer is allowed to connect or not.**

**E2E 加密 ≠ 授权。** QUIC-TLS 保证「对端确实是这个 EndpointId」，但「这个 EndpointId 该不该被允许」
永远是应用层的事，不会被 TLS 取代。

## 三层门禁：选对层

| 层 | API | 时机 | 能看到什么 |
|----|-----|------|-----------|
| ① 握手前 | `RouterBuilder::incoming_filter` | `Incoming` 刚出来，spawn 之前，**内联在 accept 循环主线** | 只有 `&Incoming`。**拿不到 ALPN、拿不到对端 EndpointId** |
| ② 握手后 | `EndpointHooks::after_handshake`（`endpoint/hooks.rs:68`） | 握手完成 | 对端身份 |
| ③ 协议级 | `ProtocolHandler::on_accepting` | ALPN 查表后 | 该 ALPN 的 `Accepting` |

### incoming_filter 四选一

```rust
// protocol.rs:214-215
pub type IncomingFilter =
    Arc<dyn Fn(&crate::endpoint::Incoming) -> IncomingFilterOutcome + Send + Sync + 'static>;

// protocol.rs:562-585 —— 内联在 select! 的 accept 分支里，spawn 之前
match filter(&incoming) {
    IncomingFilterOutcome::Accept => {}
    IncomingFilterOutcome::Retry  => { /* incoming.retry() → 发 QUIC RETRY 包 */ continue; }
    IncomingFilterOutcome::Reject => { incoming.refuse(); continue; }   // 对端立刻收到 CONNECTION_REFUSED
    IncomingFilterOutcome::Ignore => { incoming.ignore(); continue; }   // 不回任何包，对端超时
}
```

**三点：**
1. **filter 是同步闭包**（`Fn` 不是 async），跑在 accept 循环主线上——**里面做慢事会拖住整个 Router 的 accept 吞吐**
2. 想在这一层看 ALPN 只能 `Incoming::decrypt()` 解 Initial 包偷看 ClientHello（~1200 字节的 clone + 解密）。官方明说贵（`protocol.rs:465-468`）：「Note that `decrypt()` is relatively expensive, so filters should reject based on cheaper signals (e.g. remote address) first.」
3. **`Retry` 的语义随连接类型而变**（`protocol.rs:170-208` 写得非常细）：直连 UDP 上它是真的源地址验证（防伪造源地址的放大攻击）；走 relay 时没有源地址可验，它只是给客户端**强加一个 relay 往返 + 重发 ClientHello 的成本**

> libp2p 的 `handle_pending_inbound_connection` 大致对应这一层，但它拿不到 QUIC RETRY 这种传输层原语，也没有 `Ignore`（静默丢弃）——因为 libp2p 的 transport 抽象把 QUIC 细节藏起来了。iroh 直接坐在 QUIC 上，把 RETRY / refuse / ignore 原样暴露，**DoS 防护手段更细**。

## 协议层的第四道门：iroh-blobs 的 provider 事件拦截

上面三层是 iroh 核心提供的。**用现成的高层协议时，那个协议自己可能还有一层。**
iroh-blobs 就把准入做成了 provider 的事件拦截钩子 —— 授权逻辑不在网络层而在 blobs 协议层，
粒度是「连接」和「单条 get 请求」。**心智上更接近 HTTP middleware。**

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

#### ⚠️ `EventMask::DEFAULT` 的真相（与常见误传相反）

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


> **`push: RequestMode::Disabled` 意味着 iroh-blobs 的 push 模型默认关闭** —— 这条对「offer → accept → 推」
> 型产品模型的影响 → [03c-blobs.md](03c-blobs.md)。

---

# 6. Gossip Broadcast

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

> **成熟度判定与证据链** → [index-ecosystem-map.md](index-ecosystem-map.md)。
> **浏览器可行性（iroh-gossip 零 wasm 适配代码就能跑）** → [06-wasm-browser.md](06-wasm-browser.md)。
