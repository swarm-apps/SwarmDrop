# Iroh Services（n0 的商业产品线）

> 这一节回答选型问题：**用了 iroh 会不会被 n0 锁定**。
> 结论先行：**协议层不锁定，默认配置层有静默依赖**。两者要分开看 —— 混为一谈会得出错误结论。

## 1. Iroh Services 是什么

官方定义（<https://docs.iroh.computer/iroh-services/>）：

> "A web platform for managing and monitoring iroh infrastructure and networks."
> "an always-online service that helps keep your devices connected, available, and accessible globally."

**它是控制面 + 运维面的 SaaS，不是数据面。** 你的 P2P 连接不经过它；它做仪表盘、指标聚合、relay 托管、远程诊断。

| 产品 | 是什么 | 需要 API Key | 文档 |
|---|---|---|---|
| **Public Relays** | 内置在 iroh 里的免费共享 relay，开箱即用 | 否 | [relays/public](https://docs.iroh.computer/iroh-services/relays/public) |
| **Managed Relays** | 专属 relay，隔离 / 版本锁定 / SLA / 默认鉴权 | 是 | [relays/managed](https://docs.iroh.computer/iroh-services/relays/managed) |
| **Metrics** | endpoint 指标推送到平台聚合（每分钟一次） | 是 | [metrics](https://docs.iroh.computer/iroh-services/metrics/) |
| **Net Diagnostics** | 平台反向拨回你的 endpoint 跑连通性探测 | 是 | [net-diagnostics/usage](https://docs.iroh.computer/iroh-services/net-diagnostics/usage) |
| **API Keys** | 认证 endpoint 到项目，`IROH_SERVICES_API_SECRET` | — | [access](https://docs.iroh.computer/iroh-services/access) |
| **Billing** | 用量计费，月结，支持 spend cap | — | [billing](https://docs.iroh.computer/iroh-services/billing/) |
| **Support** | Discord（Free）/ 工单（Pro）/ SLA（Enterprise） | — | [support](https://docs.iroh.computer/iroh-services/support) |

官方对权限边界说得很清楚（access.md 原文）：

> "Iroh itself is permissionless — connections between two endpoints never need authorization"

API Key 只用于三件事：上传 metrics、上传诊断报告、连接你的专属 relay。**公共 relay 连接和 NAT 穿透不需要 key。**

### Metrics 采什么

metrics/index.md 列出四类：Online Endpoints、Direct Data Rate、Traffic Sent & Received、Relay Metrics。声称 "anonymized and aggregated"，无 PII。聚合频率 "Once per minute"（metrics/how-it-works.md）。

### Net Diagnostics 的反向拨回（注意这个语义）

usage.md 描述的机制：你授予 `NetDiagnosticsCap::GetAny` 能力后，**平台可以主动拨回你的 endpoint**，你的 `ClientHost` 本地跑诊断再回传：

```rust
.accept(CLIENT_HOST_ALPN, ClientHost::new(&endpoint))
```

能力模型是 object-capability（`iroh-services/src/caps.rs` 注释原文）：

> "Caps follow the [object capability model], where possession of a valid capability token is the canonical source of authorization."

不授予 cap，仪表盘的 **Run Diagnostics** 按钮就是灰的。这是显式 opt-in，不是默认开。

## 2. 开源 / 闭源边界在哪

这是本节最关键的表。**边界不在「协议 vs 服务」，而在「客户端 + 服务端程序 vs 托管平台」**：

| 组件 | 开源？ | 位置 |
|---|---|---|
| `iroh` endpoint 全栈 | 是 | `iroh/iroh/` |
| **relay 服务端程序** | 是 | `iroh/iroh-relay/`，`[[bin]] name = "iroh-relay"`，`required-features = ["server"]`（`iroh-relay/Cargo.toml:176-179`）|
| **pkarr / DNS 服务端程序** | 是 | `iroh/iroh-dns-server/`，二进制 `iroh-dns-server`，附 `config.dev.toml` / `config.prod.toml`，Apache-2.0/MIT 双许可 |
| mDNS / Mainline DHT 寻址 | 是 | `iroh-address-lookups/iroh-mdns-address-lookup`（v0.4.0）、`iroh-mainline-address-lookup`（v0.4.0）|
| **`iroh-services` 客户端 crate** | **是，Apache-2.0** | <https://github.com/n0-computer/iroh-services>（crates.io `iroh-services = "1.0.0"`）|
| **Iroh Services 平台后端** | **否 —— 未找到任何源码** | services.iroh.computer |

**纯开源版缺什么**：仪表盘、指标聚合存储与保留、告警、模拟、relay 托管运维、SLA、工单支持。
**纯开源版不缺什么**：任何协议能力。relay、寻址、打洞、E2E 加密的服务端程序全部开源且可自建。

> 值得注意：连付费功能的客户端（`iroh-services` crate）都是 Apache-2.0 开源的 —— 你能读到它到底传了什么。这在商业 SaaS 里不常见，是个正面信号。闭源的只有平台后端本身。

## 3. 开源 iroh 会不会 phone-home？

**必须拆成两个问题回答，答案相反。**

### 3.1 遥测：不会。已核实

- `iroh/iroh/Cargo.toml` 对 `iroh-services` 的引用数 = **0**。核心库没有任何路径能上传指标。
- metrics 上传必须**显式构造** `iroh_services::Client` 并提供 API key。`iroh-doctor` 的真实代码印证了这点（`iroh-doctor/src/doctor.rs:656-668`）—— 只在用户显式传了 `service_node` 时才建 client：

```rust
let rpc_client = if let Some(remote_node) = service_node {
    let client = iroh_services::Client::builder(&endpoint)
        .ssh_key_from_file(ssh_key_path).await?
        .remote(remote_node)
        .build().await?;
    Some(client)
} else {
    None
};
```

- **陷阱澄清**：`iroh` 的 `default` feature **含** `metrics`（`iroh/Cargo.toml:150`：`default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]`）。这个名字唬人，但它只等于 `iroh-metrics/metrics`，即**本地内存 Registry**。全仓 grep `push_metrics|otlp|opentelemetry|pushgateway` 在 `iroh/` 和 `iroh-relay/` 下 **0 命中**。没有 exporter，没有出站。

### 3.2 默认配置的服务依赖：**会。核实属实，且比预想的更深**

`presets::N0` 硬装了 4 样东西指向 n0（`iroh/iroh/src/endpoint/presets.rs:116-140`，原文）：

```rust
impl Preset for N0 {
    fn apply(self, mut builder: Builder) -> Builder {
        builder = Minimal.apply(builder);
        builder = builder.address_lookup(PkarrPublisher::n0_dns());
        // Resolve using HTTPS requests to our DNS server's /pkarr path.
        builder = builder.address_lookup(PkarrResolver::n0_dns());
        // Additionally resolve using DNS queries outside browsers.
        #[cfg(not(wasm_browser))]
        {
            builder = builder.address_lookup(crate::address_lookup::DnsAddressLookup::n0_dns());
        }
        builder = builder.relay_mode(default_relay_mode());
        builder
    }
}
```

具体指向的硬编码常量：

| 常量 | 值 | 位置 |
|---|---|---|
| `N0_DNS_PKARR_RELAY_PROD` | `https://dns.iroh.link/pkarr` | `address_lookup/pkarr.rs:127` |
| `N0_DNS_PKARR_RELAY_STAGING` | `https://staging-dns.iroh.link/pkarr` | `address_lookup/pkarr.rs:134` |
| `N0_DNS_ENDPOINT_ORIGIN_PROD` | `dns.iroh.link.` | `iroh-dns/src/dns.rs:45` |
| `N0_DNS_ENDPOINT_ORIGIN_STAGING` | `staging-dns.iroh.link.` | `iroh-dns/src/dns.rs:47` |
| prod relay（4 个） | `use1-1.` / `usw1-1.` / `euc1-1.` / `aps1-1.` + `relay.n0.iroh.link.` | `iroh/src/defaults.rs:27-33` |
| staging relay（2 个，无 AP） | `use1-1.` / `euc1-1.` + `staging-relay.n0.iroh.link.` | `iroh/src/defaults.rs:94-96` |

所以 `Endpoint::builder(presets::N0).bind()` 一跑起来就会：**向 dns.iroh.link 发布你的签名地址记录**、**从它解析**、并**向 4 个 n0 relay 做 net_report 探测选路**。这是功能性依赖（不发布就没人找得到你），不是遥测 —— 但它确实是默认开、静默的出站流量。

### 3.3 `.relay_mode(Custom)` 不移除 address lookup —— 核实属实

两处源码共同证明：

**其一**，`address_lookup` 是 `Vec`，方法是 **push 不是 set**（`endpoint.rs:134` + `605-608`）：

```rust
address_lookup: Vec<Box<dyn DynAddressLookupBuilder>>,
```
```rust
pub fn address_lookup(mut self, address_lookup: impl AddressLookupBuilder) -> Self {
    self.address_lookup.push(Box::new(address_lookup));
    self
}
```

其 doc 明写：

> "This method can be called multiple times and all the Address Lookup's passed in will be combined... To clear all Address Lookup's, use [`Self::clear_address_lookup`]."

**其二**，`relay_mode()` 只操作 `self.transports`，**从头到尾没碰 `self.address_lookup`**（`endpoint.rs:557-577`）：

```rust
pub fn relay_mode(mut self, relay_mode: RelayMode) -> Self {
    let transport: Option<_> = relay_mode.into();
    match transport {
        Some(transport) => {
            if let Some(og) = self.transports.iter_mut()
                .find(|t| matches!(t, TransportConfig::Relay { .. }))
            { *og = transport; } else { self.transports.push(transport); }
        }
        None => {
            self.transports.retain(|t| !matches!(t, TransportConfig::Relay { .. }));
        }
    }
    self
}
```

**⚠️ 最有力的旁证：n0 自己的付费产品也踩这个语义。** `iroh-services/src/preset.rs` 里 `IrohServicesPreset::apply` 原文：

```rust
impl Preset for IrohServicesPreset {
    fn apply(self, builder: iroh::endpoint::Builder) -> iroh::endpoint::Builder {
        // Inherit n0 defaults (crypto provider + DNS address lookup), then
        // overlay our relay map and (optionally) an explicit secret key
        let mut builder = iroh::endpoint::presets::N0.apply(builder);
        builder = builder.relay_mode(RelayMode::Custom(self.relays));
        builder = builder.secret_key(self.secret_key);
        builder
    }
}
```

也就是说：**即使你付费买了 Managed Relay，你的 endpoint 依然在向 `dns.iroh.link` 发布和解析地址。** 付费买到的是 relay 隔离，不是寻址脱钩。注释里 "Inherit n0 defaults (crypto provider + DNS address lookup)" 是明写的、有意的设计，不是 bug —— 但用户很难从 "买了专属 relay" 推断出 "DNS 还在 n0"。

### 3.4 隐私影响（官方自己的表述，不夸大）

relays/public.md 原文：

> "All traffic through the public relays is end-to-end encrypted. The relays cannot read any of the traffic they forward."

但同页也承认元数据可见：

> relays can observe "connection metadata: source and destination IP addresses, connection times, and the amount of data transferred."
> 并建议敏感数据不要走公共 relay。

另外 **pkarr 记录是公开可读的**：它是用你 EndpointId 派生的名字发布的**签名 DNS 记录**（`address_lookup/pkarr.rs:13` 注释举例 `o3dks..6uyy.dns.iroh.link`）。任何知道你 EndpointId 的人都能查到你发布的地址。这是设计使然（可发现性的代价），用 `AddrFilter`（如 `AddrFilter::relay_only()`，见 `address_lookup.rs` doc example）可以只发布 relay URL、不发布直连 IP。

### 3.5 环境变量陷阱

`IROH_FORCE_STAGING_RELAYS` 只要**非空**就会把 relay 和 pkarr/DNS **全部**切到 staging 基础设施（`endpoint.rs:1970-1984`）：

```rust
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

staging 的官方描述是 "might have incompatible changes deployed"（`defaults.rs:84`）且**没有 AP 节点**。别在生产镜像里漏掉这个 env。

## 4. 计费模型

来源：<https://iroh.computer/pricing> + billing 文档系列。

| | **Free** $0/mo | **Pro** $19/mo | **Enterprise** 询价 |
|---|---|---|---|
| 定位 | "All features in Pro, limited" | "Pay as you go pricing" | "On-prem and multi-cloud" |
| 指标保留 | 7 天 | 30 天 | Custom retention |
| Data Points/Minute | 1K DPM | 10K DPM 后按量 | — |
| 指标超额 | — | **$1.49 / 1K DPM** | — |
| Relay | 仅共享公共 relay | **$0.27 / relay / 小时** | — |
| 并发 endpoint 超额 | — | **$0.50 / 100 endpoints** | — |
| 每 relay 并发连接 | — | 60k | — |
| 支持 | Discord 社区 | 8x5 工单 | SLA + 专属支持工程师 |

计费机制（billing/faq.md 原文）：

> "Iroh Services billing is usage-based, calculated monthly."
> "Relays are billed based on hourly usage"
> "You'll be charged at the beginning of each billing cycle for your plan, and at the end of the cycle for any usage-based overages."
> 按小时计费可退：*"If you cancel after 10 hours of relay usage in a month, you'll only be billed for those 10 hours."*

**Spend cap 的行为很重要**（billing/control-costs.md，原文）：

> "Metrics ingestion will be paused. Endpoints will stop sending new metrics until the next billing cycle."
> "**Active relay connections are not affected.** Your endpoints will continue to connect through relays."
> "Your existing data is preserved. Dashboards and historical metrics remain accessible."

即 **撞上限只停可观测性，不断你的网络**。这是个良性设计 —— 账单事故不会变成线上事故。（未找到默认 cap 值，需手动配置。）

自建 relay 的成本对照：$0.27/relay/小时 ≈ **$194/relay/月**。一台同规格 VPS 通常远低于此。付费买的是运维、SLA、版本锁定和跨区部署，不是算力。

## 5. 公共 relay 的服务条款（选型必读）

relays/public.md 原文，逐条：

> - "suitable for **development and hobby use only**. For production, use managed relays."
> - "No SLA or uptime guarantee"
> - "**Only the latest stable release of iroh is officially supported**"
> - "**No version locking; n0.computer reserves the right to remove support for older iroh versions**"
> - "Traffic is rate-limited to prevent abuse"
> - "We monitor public relays for abuse. If we detect malicious activity, we reserve the right to block offending IP addresses or users."

**这是真实的锁定风险点，且是本节最该重视的一条。** 不是"n0 会扣押你的数据"，而是：**你若依赖公共 relay，就等于把「必须跟随 iroh 最新稳定版」写进了你的运维契约**。已发布的桌面/移动客户端无法强制用户升级 —— 老版本客户端有一天可能被公共 relay 拒绝。

> 注：官方文档没有列出公共 relay 的 hostname，但源码里是硬编码的（见 3.2 表格 `defaults.rs:27-33`）。

## 6. 如何彻底脱钩

### 6.1 代码层：两种写法

**推荐 —— 从 `Minimal` 起手**（不继承任何 n0 默认；`presets::Minimal` 只设 crypto provider，见 `presets.rs:61-79`）：

```rust
use iroh::{Endpoint, RelayMode, address_lookup::{self, PkarrPublisher, PkarrResolver},
           endpoint::presets};

let ep = Endpoint::builder(presets::Minimal)
    .relay_mode(RelayMode::custom(["https://relay.example.com".parse()?]))
    .address_lookup(PkarrPublisher::builder("https://dns.example.com/pkarr".parse()?))
    .address_lookup(PkarrResolver::builder("https://dns.example.com/pkarr".parse()?))
    .address_lookup(address_lookup::DnsAddressLookup::builder("dns.example.com.".to_string()))
    .bind()
    .await?;
```

自定义构造器均已确认存在：`PkarrPublisher::builder(pkarr_relay: Url)`（`pkarr.rs:290`）、`PkarrResolver::builder(pkarr_relay: Url)`（`pkarr.rs:507`）、`DnsAddressLookup::builder(origin_domain: String)`（`dns.rs:78`）。

**次选 —— `N0` + 显式清空**（`iroh-doctor` 的真实做法，`iroh-doctor/src/doctor.rs:632-648`）：

```rust
let mut endpoint = Endpoint::builder(presets::N0)
    .secret_key(secret_key)
    .alpns(vec![DR_RELAY_ALPN.to_vec()])
    .transport_config(transport_config);

if disable_address_lookup {
    endpoint = endpoint.clear_address_lookup();   // ← 必须显式调用
}

let endpoint = match relay_map {
    Some(relay_map) => endpoint.relay_mode(RelayMode::Custom(relay_map)),
    None => endpoint,
};
```

> **`clear_address_lookup()` 是唯一的逃生舱**（`endpoint.rs:585-588`）。它的 doc 也警告了后果：
> "If no Address Lookup is set, connecting to an endpoint without providing its direct addresses or relay URLs will fail."
> 清空后你必须自己提供 `EndpointAddr`（用 ticket、二维码、你自己的信令服务器，或 `MemoryLookup` 手动灌入）。

### 6.2 自建组件清单

| 你要替掉的 n0 服务 | 自建什么 | 从哪来 |
|---|---|---|
| 4 个 prod relay | `iroh-relay` 二进制 | `iroh/iroh-relay/`，`--features server` |
| `dns.iroh.link` 的 `/pkarr` 发布端 | `iroh-dns-server`（含 `PUT /pkarr`） | `iroh/iroh-dns-server/` |
| `dns.iroh.link` 的 DNS 解析 | 同一个 `iroh-dns-server`（DNS over UDP/TCP + `/dns-query` DoH） | 同上，配 `config.prod.toml` |

`iroh-dns-server/README.md` 原文确认它一个进程全包：

> - "A DNS server listening on UDP and TCP for DNS queries"
> - "A HTTP and/or HTTPS server which provides the following routes: `/pkarr`: `GET` and `PUT` for pkarr signed packets; `/dns-query`: Answer DNS queries over DNS-over-HTTPS"

即：**两个二进制（`iroh-relay` + `iroh-dns-server`）+ 一个你自己的域名 = 完全脱钩**。都在 iroh 主仓，Apache-2.0/MIT。

### 6.3 三档脱钩方案

| 方案 | 组成 | 对 n0 的依赖 | 代价 |
|---|---|---|---|
| **全自建** | 自建 relay + 自建 `iroh-dns-server` + 自有域名 | 零 | 要运维 2 个服务 + TLS 证书 |
| **无服务器（去中心）** | `presets::Minimal` + `iroh-mainline-address-lookup`（BitTorrent Mainline DHT）+ `iroh-mdns-address-lookup`（局域网）+ `RelayMode::Disabled` | 零 | 无 relay ⇒ 对称 NAT 后可能连不上；DHT 解析慢且看运气 |
| **局域网 only** | `presets::Minimal` + `iroh-mdns-address-lookup` + `RelayMode::Disabled`（≈ `presets::N0DisableRelay` 但换掉 lookup） | 零 | 只能同网段 |

mDNS 和 Mainline DHT 寻址**不在 `iroh` 主 crate 里**，是独立 crate（`address_lookup.rs` 注释原文）：

> "mDNS-based and Mainline-DHT-based Address Lookup services live in separate crates: [`iroh-mdns-address-lookup`] and [`iroh-mainline-address-lookup`]."

两者当前均为 **v0.4.0**（`iroh-address-lookups/*/Cargo.toml`）—— 版本号远低于 iroh 1.0.2，成熟度按 0.x 对待。

### 6.4 脱钩检查清单（易漏项）

- [ ] 用了 `presets::N0` 却只改了 `relay_mode` → **DNS 还在 n0**（§3.3）
- [ ] 用了 `iroh_services::preset()`（Managed Relay）→ **DNS 还在 n0**，这是 n0 自己的实现（§3.3）
- [ ] `presets::N0DisableRelay` → 名字像"全关"，实则**只关 relay，三个 n0 lookup 全留着**（`presets.rs:178-184`：`N0.apply(builder).relay_mode(RelayMode::Disabled)`）
- [ ] 忘了 `clear_address_lookup()` → lookup 是 push 语义，加新的不会顶掉旧的
- [ ] 生产镜像里残留 `IROH_FORCE_STAGING_RELAYS` → 全量切 staging（§3.5）
- [ ] wasm 目标下 `DnsAddressLookup` 本就不启用（`#[cfg(not(wasm_browser))]`），但 Pkarr publisher/resolver **仍然启用** → 浏览器里也会打 `dns.iroh.link`

## 7. 选型结论：会不会被锁定

**不会被协议锁定 —— 这个担心可以放下：**
- relay 和 pkarr/DNS 的**服务端程序都开源**且有现成二进制配置，自建是设计内的一等公民，不是 hack。
- 连付费客户端 `iroh-services` 都是 Apache-2.0，传什么可审计。
- 核心库对 `iroh-services` **零依赖**，没有隐蔽遥测通道。
- 逃生舱（`clear_address_lookup` / `Minimal` / `RelayMode::Custom`）都是公开稳定 API，n0 自己的 `iroh-doctor` 就在用。

**真实的风险是这两条，且都可控：**
1. **公共 relay 的版本契约**（§5）—— "No version locking; n0 reserves the right to remove support for older iroh versions"。对**已分发、无法强制升级的客户端**，这是实打实的运营风险。缓解：生产上自建 relay 或买 Managed。
2. **默认配置的静默依赖**（§3.2/3.3）—— 不脱钩就等于把可发现性外包给了 `dns.iroh.link`。它挂了，你的新节点互相找不到（已建立的直连不受影响）。缓解：自建 `iroh-dns-server`。

**公允地说**：n0 把默认值指向自己的基础设施，是为了"开箱即用"，且在文档和注释里都写明了（`presets.rs:95-101` 明写 "publishes to and resolves from the n0.computer dns server `iroh.link`"）。这不是暗桩。但**"文档里写了"不等于"用户会读到"** —— 尤其 `.relay_mode(Custom)` 只覆盖 relay 这个语义，连 n0 自家付费 preset 都保留了 n0 DNS，普通用户几乎不可能自行推断出来。**默认值有依赖是事实，脱钩成本低也是事实，两者都要如实告诉选型的人。**


## 附：从 FFI 绑定侧独立确认的能力范围

上面的能力清单来自官方文档。**本地唯一可读的一手证据是 `iroh-ffi/src/services.rs`** —— 它独立印证了同一组能力。

- **成熟度**：**production**（选配）
- **依据**：
  - `iroh-doctor/Cargo.lock` 中 iroh-services version = 1.0.0、`source = "registry+https://github.com/rust-lang/crates.io-index"`、带 checksum —— **已在 crates.io 正式发布 1.0.0**
  - iroh-ffi（HEAD 2026-07-16）以 `iroh-services = { version = "1.0.0", default-features = false }` 依赖它，并配有 Rust + Python 双份测试
  - ⚠️ **它是 library crate**（`iroh-ffi/src/services.rs:8` `use iroh_services::{Client, ClientBuilder};`、:65 `impl From<iroh_services::net_diagnostics::DiagnosticsReport>`），**不是 binary crate**
  - ⚠️ **源码未克隆到 iroh-study，本次未审计** —— 但**不是「不可审计」**：crates.io 的 .crate 包按定义即源码分发（cargo 必须从源码编译），docs.rs / `cargo vendor` 均可拿到
- **入口**（本地唯一可读的证据）：`iroh-ffi/src/services.rs`

#### 它是什么（能力范围经 FFI 绑定确认）

- **它是可观测性/遥测面，不是传输面。**
- 能力：`ping` / `name` / `set_name`（云端注册 endpoint 名字）/ `push_metrics`（按 metrics_interval 周期推送指标）/ `net_diagnostics(send: bool)`（跑本地网络诊断，**可选**上传云端存档）
- 凭证三选一：`api_secret`（`services1...` 编码 ticket）、`IROH_SERVICES_API_SECRET` 环境变量、或 `ssh_key_pem`（node operator / project owner 全权限）
- doc comment 直言：*"Binding for `iroh-services` — push metrics to services.iroh.computer"*


## 未找到 / 未核实

- Iroh Services **平台后端**源码 —— 未找到（仅客户端开源）。
- 官方文档中的公共 relay hostname 列表 —— relays/public.md **未列出**（源码有，见 §3.2）。
- 默认 spend cap 数值 —— control-costs.md 未给，需手动设置。
- metrics glossary 的完整指标名列表 —— 未逐项核对（见 <https://docs.iroh.computer/iroh-services/metrics/glossary>）。
- Managed relay 的可选区域清单 —— managed.md 只说 "Deploy across regions and providers"，未给枚举。
- Enterprise 具体价格与 SLA 条款 —— 询价制，未公开。
