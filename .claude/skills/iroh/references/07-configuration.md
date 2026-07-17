# Configuration：feature flags · 跨平台 · 自建 relay · 兼容性

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Configuration](https://docs.iroh.computer/configuration/) 分区（Use your own relay / Compatibility），
外加 iroh 的 feature / cfg 面 —— 官方没有独立页，但它决定了「哪些 API 在你的构建里存在」。

> **relay 是什么、home relay 怎么选出来** → [01-concepts.md](01-concepts.md)。
> **建 Endpoint 时怎么指向自建 relay** → [02-connecting.md](02-connecting.md)。
> **带宽账单、限流、准入控制** → [08-deployment.md](08-deployment.md)。

---

# 1. Feature Flags 与 cfg alias

**核心心智**：iroh 的 feature 面比 libp2p 小得多——**只有 10 个 feature，且没有任何 discovery-\* feature**。跨平台**不是靠 feature 裁剪**，而是靠 build.rs 的两个 cfg alias + Cargo.toml 里的 **target-gated 依赖**。

## 全部 10 个 feature

```toml
# iroh/iroh/Cargo.toml
[features]
default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]
portmapper = ["dep:portmapper"]
metrics = ["iroh-metrics/metrics", "iroh-relay/metrics"]
test-utils = ["iroh-relay/test-utils", "iroh-relay/server", "dep:axum"]
# Enables fetching TLS trust anchors from the operating system
platform-verifier = ["iroh-relay/platform-verifier"]
qlog = ["noq/qlog"]
# Use private Apple APIs to send multiple packets in a single syscall.
fast-apple-datapath = ["noq/fast-apple-datapath"]
# Use ring as the crypto backend.
tls-ring = ["noq/ring", "iroh-relay/tls-ring", "iroh-dns/tls-ring"]
# Use aws-lc-rs as the crypto backend, unless `ring` is also enabled.
tls-aws-lc-rs = ["noq/aws-lc-rs", "iroh-relay/tls-aws-lc-rs", "iroh-dns/tls-aws-lc-rs"]
# Unstable: Custom transport API (may change without notice)
unstable-custom-transports = []
# Unstable: API to access an endpoint's NetReport (may change without notice)
unstable-net-report = []
```

> ⚠️ **`discovery-*` 系列 feature 在 1.0.2 不存在。** grep 整个 workspace 的 `*.toml`，"discovery" 只在 `iroh-dns/Cargo.toml` 的 description 文案里出现一次。**别照搬 0.x 的 `discovery-local-network` / `discovery-pkarr-dht`**（历史脉络：`CHANGELOG.md:1495` 记录 0.23.0 时代曾有 `discovery-local-network`，1.0 已废除该模式）。

> **libp2p 对照**：libp2p 习惯是「一个能力 = 一个 feature」（mdns/kad/relay/dcutr 全是 feature）。**iroh 反过来：能力靠 trait 对象 + builder 注入，feature 只用来选 crypto backend / 平台加速 / 可选副作用（metrics、portmapper）。不要用 libp2p 的 feature 心智去找 iroh 的开关。**

## 两个 cfg alias

```rust
// iroh/iroh/build.rs
use cfg_aliases::cfg_aliases;
fn main() {
    cfg_aliases! {
        wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
        with_crypto_provider: { any(feature = "tls-ring", feature = "tls-aws-lc-rs") }
    }
}
```

⚠️ **`wasm_browser` 的判据含 `target_os = "unknown"`，所以只命中 `wasm32-unknown-unknown`。`wasm32-wasip1`/`wasip2` 不走 wasm_browser 分支，会去走 `not(wasm_browser)` 的 IP transport 路径。别把「wasm」和「wasm_browser」当同义词。**

`with_crypto_provider` **gate 的是 `Minimal` / `N0` / `N0DisableRelay` 三个 preset 类型的存在性** —— 详见下一节。

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

## ⚠️ `default-features = false` 的连带杀伤

**关掉 default 会连带关掉 `tls-ring` → `with_crypto_provider` 为假 → `presets::N0` / `Minimal` / `N0DisableRelay` 三个类型整个从 API 消失。**

而 crypto provider 缺失是**运行时**错误（`endpoint.rs:228-230` 的 `ok_or_else`），**代码照样编过，跑起来 `bind()` 才炸**。

**无 tls-* feature 时必须**：

```rust
Endpoint::builder(presets::Empty)
    .crypto_provider(Arc::new(/* 你自己的 provider */))
```

详见 [02-connecting.md](02-connecting.md)。

---

# 2. crypto backend：tls-ring vs tls-aws-lc-rs

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


---

# 3. 平台与 target

## ⚠️ fast-apple-datapath：默认开着，但实为死代码

**iroh 自身源码里零实现**——`grep` `iroh/src` 结果为空，Cargo.toml 那一行是唯一出现处，纯透传到 `noq/fast-apple-datapath` → `noq-udp`。

```toml
# iroh 侧
fast-apple-datapath = ["noq/fast-apple-datapath"]
# noq-1.0.1/Cargo.toml
fast-apple-datapath = ["udp/fast-apple-datapath"]
```

```rust
// noq-udp-1.0.1/build.rs
apple: {
    any(target_os = "macos", target_os = "ios", target_os = "tvos",
        target_os = "visionos", target_os = "watchos")
},
apple_fast: { all(apple, feature = "fast-apple-datapath") },
apple_slow: { all(apple, not(feature = "fast-apple-datapath")) },
```

**作用域只在 apple 系，其他平台完全惰性——这解释了为什么 Android CI 和 wasm CI 都懒得关它。**

（证据链跨仓：实现在 crates.io 的 `noq-udp-1.0.1`，本机路径 `~/.cargo/registry/src/index.crates.io-*/noq-udp-1.0.1/`，**不在 iroh-study 树内**。该 feature 是 0.x 后期才加的，`CHANGELOG.md:371`「Add fast-apple-datapath feature and enable it by default (#3724)」，隶属 `:365` 的 `## [0.96.0] - 2026-01-28`。）


> **私有符号的 App Store 风险评估**（dlsym 动态解析、`set_apple_fast_path` 全链路无人调用、
> 默认配置下是死代码）→ [08-deployment.md](08-deployment.md) 的 Security & Privacy 一节。


## iOS / Android：没有任何专属 feature

```yaml
# iroh/.github/workflows/ci.yml —— Android
- name: Build workspace test binaries
  if: matrix.target == 'x86_64-linux-android'
  env:
    ANDROID_NDK_HOME: ${{ steps.setup-ndk.outputs.ndk-path }}
    CARGO_NDK_TARGET: ${{ matrix.target }}
  run: |
    cargo ndk test --no-run -p iroh-base --all-features
    cargo ndk test --no-run -p iroh-dns --features tls-ring
    cargo ndk test --no-run -p iroh-relay --features tls-ring,metrics
    cargo ndk test --no-run -p iroh --features tls-ring,metrics,portmapper,test-utils
```

矩阵 target：`aarch64-linux-android` / `armv7-linux-androideabi` / `x86_64-linux-android`，NDK **r25c**，cargo-ndk **4.1.2**。

⚠️ **`--features X` 是叠加在 default 之上的**（没有 `--no-default-features`），所以这行实际生效的 feature = default ∪ {test-utils} = `metrics + fast-apple-datapath + portmapper + tls-ring + test-utils`。

**这份列表反映的是 n0 心目中「Android 该有的样子」（tls-ring + metrics + portmapper），但它并没有真的排除 fast-apple-datapath——靠的是它在非 apple 平台惰性。**

**iOS 在 ci.yml 里 grep 不到任何 job**，只有 noq-udp 的 apple alias 覆盖 `target_os = "ios"`。即 **iOS 无 CI 覆盖**。

## portmapper：no-op stub 而非报错

```rust
// iroh/iroh/src/portmapper.rs
//! Portmapper integration.
//!
//! Wraps the real [`portmapper`] crate when the `portmapper` feature is enabled,
//! or provides a no-op stub otherwise.

pub(crate) fn create_client(config: &PortmapperConfig) -> Client {
    match config {
        #[cfg(all(not(wasm_browser), feature = "portmapper"))]
        PortmapperConfig::Enabled {} => Client::Enabled(::portmapper::Client::default()),
        _ => {
            let (tx, rx) = watch::channel(None);
            Client::Disabled { _tx: tx, rx }
        }
    }
}
```

**这是刻意的 API 稳定性设计：`PortmapperConfig` 无 cfg，跨平台代码可以无脑 `.portmapper_config(...)`。真实开关是 `all(not(wasm_browser), feature = "portmapper")` 双条件。**

值得写进产品决策的点：`PortmapperConfig::Disabled` 的 doc 提到 **UPnP 的 SSDP multicast 会在 macOS 上弹防火墙授权对话框**——对桌面 app 分发是个真实的 UX 决策点。

> libp2p 的 `libp2p-upnp` 是独立 behaviour，不装就没有。iroh 是「**装了但可能是空壳**」。

## 各 target 速查

| target | IP transport | relay | QAD 探测 | DNS resolver | portmapper | 备注 |
|--------|-------------|-------|---------|-------------|-----------|------|
| linux/macos/windows | ✅ | ✅ | ✅ | ✅ | feature 决定 | 完整 |
| iOS / Android | ✅ | ✅ | ✅ | ✅ | feature 决定 | **iOS 无 CI**；Android 需手写 JNI 初始化（见 [05-languages.md](05-languages.md)）|
| wasm32-unknown-unknown | ❌ **整个编译掉** | ✅ WebSocket | ❌ 只剩 Https | ❌ 只剩 pkarr HTTPS | no-op stub | **无直连无打洞**；custom transport 仍可注册 |
| wasm32-wasip1/p2 | ✅（**不走 wasm_browser 分支**）| ✅ | ✅ | ✅ | — | 未见 CI 覆盖 |


> **macOS 上 ring 编不到 wasm32**（本地 toolchain 问题，不是 iroh 约束）
> → [06-wasm-browser.md](06-wasm-browser.md)。

## MSRV 汇总

| 库 | rust-version | edition |
|---|---|---|
| iroh | 1.91 | 2024 |
| **n0-watcher** | **1.91**（`Cargo.toml:13`） | — |
| n0-future | 1.85 | 2021 |
| n0-error | 未声明 | 2024 |

**迁 iroh 会一次性把 MSRV 底线抬到 1.91。** 若 CI/Docker/贡献者环境锁了旧 toolchain，需提前记一笔。


---

# 4. 自建 relay

## 一句话结论

**自建 relay 完全可行，且比官方文档说的宽松得多** —— `tls` 是 `Option<TlsConfig>`，整段省掉就是合法的生产配置（不需要域名、不需要证书）。且**自建 relay 不需要配套任何 DNS/pkarr 服务器**。

**但代价必须算清**：**没有 `[tls]` 配置段 ⇒ 没有 QAD ⇒ 拿不到公网地址 ⇒ 直连率显著下降 ⇒ 流量大量常驻中继。**

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

> ⚠️ **别把这条说成「QAD 挂了就永远不可能直连」** —— 那是错的。`socket.rs:1821-1841` 的 `update_direct_addresses()` 依次装配**四类**候选：**portmapper**（`DirectAddrType::Portmapped`，**与 QAD 完全独立**）/ `global_v4/v6`（`Qad`）/ `collect_local_addresses()`（`Local`）/ `configured_addrs`（`Config`）。**准确表述**：拿不到 QAD 会**显著降低直连率**，不是「永远不可能直连」。详见 [01-concepts.md](01-concepts.md)。

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

## 环境变量后门：`IROH_FORCE_STAGING_RELAYS`

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


## relay 中转的协议上限

- `iroh-relay/src/protos/relay.rs:23` `pub const MAX_PACKET_SIZE: usize = 64 * 1024;`
- :25-29 `pub(crate) const MAX_FRAME_SIZE: usize = 1024 * 1024;`（*"This is also the minimum burst size that a rate-limiter has to accept."*）
- 服务端限速：`server.rs:507` `pub bytes_per_second: NonZeroU32`；CLI 侧 `main.rs:731-737` 校验 `if rx.bytes_per_second.is_none() && rx.max_burst_bytes.is_some() { bail_any!("bytes_per_seconds must be specified to enable the rate-limiter") }`

详见 `07-configuration.md`。

## 把 iroh-doctor 指向自建 relay

配置文件路径由 `iroh-doctor/src/config.rs` 的 `iroh_config_root()` 决定，**它是分平台的**：

| 平台 | 路径 |
|---|---|
| Linux | `$XDG_CONFIG_HOME` 或 `$HOME/.config/iroh` |
| **macOS** | **`$HOME/Library/Application Support/iroh`** |
| Windows | `{FOLDERID_RoamingAppData}/iroh` |

文件名 `CONFIG_FILE_NAME = "iroh.config.toml"`（`config.rs:18`）。也可用 `IROH_CONFIG_DIR` 环境变量无条件覆盖（`config.rs:14`），或 **`--config <PATH>`（跨平台最稳）**。

```toml
[[relay_nodes]]
url = "https://relay.example.com"
```

对应 `iroh-relay/src/relay_map.rs:232-246` 的 `RelayConfig { url, quic(serde default), auth_token(optional) }`。

⚠️ `NodeConfig` 用 `#[serde(default, deny_unknown_fields)]`（`config.rs:24`）—— **字段名写错会直接报错**（这是好事）。

`impl Default for NodeConfig`（`config.rs:36-51`）用 `iroh::endpoint::default_relay_mode().relay_map()` 填充 relay_nodes；`NodeConfig::load`（:59 起）在配置文件不存在时回落到 `Self::default()` —— **所以不写 config 也能直接跑，不是「必须先写 config」**。

---

# 5. Compatibility

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

> **iroh 的版本承诺全貌**（Release & Support Policy、wire 兼容矩阵、0.x → 1.x 已实锤破坏、
> 默认 relay 地址会漂移、平台/硬件/传输兼容性的市场话术与实现差距）→ [10-about-and-policy.md](10-about-and-policy.md)。
