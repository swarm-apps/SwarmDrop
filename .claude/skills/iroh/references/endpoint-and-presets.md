# Endpoint 生命周期与 Preset 机制

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

> 路径提醒：iroh crate 在 `iroh/iroh/`（比直觉多一层）。`endpoint.rs` 4121 行，`endpoint/presets.rs` 184 行。**`Drop for EndpointInner` 在 `socket.rs:220`，不在 `endpoint.rs`。** `BindError` 枚举定义在 `socket.rs:844-870`，也不在 `endpoint.rs`。
>
> workspace 里还有 `iroh-base` / `iroh-relay` / `iroh-dns` / `iroh-dns-server`，grep 极易串台（例如 `install_default` 的搜索结果**全部**来自 iroh-relay / iroh-dns-server 的二进制，与 Endpoint 无关）。

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

### `presets::Staging` 不存在

`grep -rn "presets::Staging|pub struct Staging"` 全仓**零输出**。`presets.rs` 全文只有 4 个 preset：`Empty`(37) / `Minimal`(59) / `N0`(113) / `N0DisableRelay`(175)。

Staging 只是 `RelayMode` 的一个枚举变体（`endpoint.rs:1920-1934`），由 `default_relay_mode()` 在环境变量非空时自动切入：

```rust
// endpoint.rs:1966-1984
pub const ENV_FORCE_STAGING_RELAYS: &str = "IROH_FORCE_STAGING_RELAYS";
pub fn force_staging_infra() -> bool {
    matches!(std::env::var(ENV_FORCE_STAGING_RELAYS), Ok(value) if !value.is_empty())
}
pub fn default_relay_mode() -> RelayMode {
    match force_staging_infra() {
        true => RelayMode::Staging,
        false => RelayMode::Default,
    }
}
```

**同一个环境变量还会同时切换 pkarr relay 端点**（`address_lookup/pkarr.rs` 的 `n0_dns()` 里 `force_staging_infra() ? N0_DNS_PKARR_RELAY_STAGING : N0_DNS_PKARR_RELAY_PROD`）。即：设一个 `IROH_FORCE_STAGING_RELAYS` 会同时改 relay map 和 DNS/pkarr 端点。

> `force_staging_infra` 全树只有一处定义（`endpoint.rs:1970-1972`），**纯读环境变量，无 `#[cfg(test)]` / `#[cfg(feature="test-utils")]` 分支**。iroh 自己的 doc comment 里写的「When in tests, or when the `test-utils` feature is enabled...」（`presets.rs:103-105`, `pkarr.rs:520-522`）是**与代码不符的陈旧文档**，别信。

## `with_crypto_provider` 是 cfg alias，不是 feature

```rust
// iroh/iroh/build.rs:3-10
cfg_aliases! {
    wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
    with_crypto_provider: { any(feature = "tls-ring", feature = "tls-aws-lc-rs") }
}
```

`tls-ring` 在 default features 里，所以**默认构建下 N0/Minimal/N0DisableRelay 都可用**。

**它是 build.rs 生成的，不是 Cargo feature——直接在下游 crate 里写 `#[cfg(with_crypto_provider)]` 不会生效。** 文档里满屏的 `# #[cfg(with_crypto_provider)]` doctest 门控（`endpoint.rs:348/428/1205/1339/1720` 等）就是这个 alias。

`with_crypto_provider` 在 `iroh/src` 下共 41 处命中，其中**非测试只有 7 处**：`presets.rs` 6 处（三个 preset 的 struct + impl 各一）+ `tls.rs:24-25` 的 `pub use iroh_relay::tls::default_provider`。其余 34 处是 doctest 包装（19 处 `# #[cfg`）与测试模块（12 处 `all(test`，加 `address_lookup.rs:1333/1336/1361`）。

**它 gate 的 pub 类型有三个**：`Minimal`(57/59)、`N0`(111/113)、`N0DisableRelay`(173/175)。关掉两个 tls-* feature 后这三个类型**直接从 API 里消失**。

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

### tls-ring vs tls-aws-lc-rs：优先级只在 preset 路径上成立

Cargo.toml 注释写得很直白：`# Use aws-lc-rs as the crypto backend, unless "ring" is also enabled.`——因为 default 已含 tls-ring，两者都开时 `Minimal`/`N0`/`N0DisableRelay` 选 **ring**（见上文 Minimal 的两个 `#[cfg]` 分支）。

**但这不代表 aws-lc-rs「不生效」。** `Builder::crypto_provider` 是公开 setter 且在 preset 之后覆盖，显式传 aws-lc-rs 时 ring 完全不参与——这正是官方两个 example 的做法：

```rust
// examples/pq-only-key-exchange.rs  —— 用 Empty，preset 压根不设 provider
Endpoint::builder(presets::Empty).crypto_provider(pq)

// examples/prefer-pq-key-exchange.rs:42/73  —— 用 N0，然后覆盖
Endpoint::builder(presets::N0).crypto_provider(pq)
```

`pq-only-key-exchange.rs` 的头注释把这条逐字写死了：

> "With iroh's default features still on, both `ring` and `aws-lc-rs` get linked. That's harmless — we wire the aws-lc-rs provider in directly via `Builder::crypto_provider`."

所以：**想真用 aws-lc-rs，`default-features = false` 不是必需的**——只要显式调 `.crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))` 即可。ring 只是被一起链接进来（多编一份，二进制变大）。`default-features = false` 只有在你想真正把 ring 从依赖树里去掉（体积/合规诉求）时才必要。

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

### 一个会误导排查的代码路径

`bind()` 里若 address lookup 服务构造失败（`endpoint.rs:303` 的 `?`），此时 `ep` 已存在（`:293-295`）且未 close，early-return 会 drop 掉它 → 触发 `Drop for EndpointInner` → 打出那条看起来像 bug 的 `error!` 日志：

```
Endpoint dropped without calling `Endpoint::close`. Aborting ungracefully.
```

即：**一次 `BindError::AddressLookup` 会附带一条误导性的 error 日志**。

> 这是从两处源码推出的代码路径（`Arc` 唯一持有者 `ep` 在 `?` 处离开作用域 → `EndpointInner::drop` 跑），**不是由某个测试断言的行为**。

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
