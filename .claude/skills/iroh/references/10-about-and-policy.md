# About / Other：版本承诺、兼容性、路线图、排障

> 本章回答的不是「怎么写」，而是「**能不能押上去、押多久、代价是什么**」。
>
> 核实基准：本地快照 `/Volumes/yexiyue/iroh-study/iroh`，分支 tip `c717c70`（2026-07-16），
> `iroh/Cargo.toml` 版本 `1.0.2`。文档侧以 `https://docs.iroh.computer/*.md` 原始 markdown 为准。
> 下文每条结论都带文件路径或官方 URL；**核不到的地方明确写「未找到」**。

## 0. 先给结论

| 问题 | 答案 |
|------|------|
| iroh 核心稳定吗？ | 是。`iroh` 1.0.2，wire 兼容有成文政策 + 代码里有真实的版本协商与门控 |
| 1.0 覆盖 blobs / gossip / docs 吗？ | **不覆盖**。它们仍是 0.x（见 §2.4），这是最容易误判的一条 |
| 存量客户端能一直连上吗？ | 跨 major 有承诺（N 兼容 N-1）；**0.x → 1.x 无承诺且已实锤破坏**（§2.2） |
| 免费支持能拿多久？ | minor 只有 3 个月，且只修在**最新 minor** 上（§1.3）——这是真实成本 |
| 官方 roadmap 可信吗？ | 页面停在 2026-02-09，仍是「1.0 前」的内容，已过期（§3） |

**一句话**：核心 `iroh` 可以押；`iroh-blobs` 要按「0.x 依赖」对待。
真正的长期风险不是 wire 协议（那条有承诺且有工程支撑），而是
**① 生态 crate 还在 0.x ② 免费支持逼你跟最新 minor ③ 默认 relay 地址被编进二进制会漂移**。

## 1. Release & Support Policy

来源：<https://docs.iroh.computer/about/release-policy.md>

### 1.1 版本类型与节奏

官方表格原文（`release-policy.md`）：

| Type | Description | Cycle | Example |
|------|-------------|-------|---------|
| Major | New features, breaking changes | ≥6 months | `1.x.x` |
| Minor | Incremental features, improvements | ≥4 weeks | `1.2.x` |
| Patch | Bug fixes, security updates | As needed | `1.2.3` |
| Release candidate | 接近稳定的预览 | As needed | `1.0-rc` |
| Canary | API 不稳定的预览 | As needed | `0.97` |
| Experimental | 特定用途的 fork/branch | As needed | `branch-name` |

**关键读法：`Cycle` 一列是「最小间隔」，不是「多久发一版」。**

- `Major ≥6 months` 的真实含义是：**breaking change 最快也只能每 6 个月来一次**。这是给你的保护，不是排期。
- 由此可推：1.0.0 发布于 2026-06-15（`CHANGELOG.md:50`），按政策 **2.0 最早也要到 2026-12 前后**。
- `Canary` 一栏把 `0.97` 当例子——即 **0.9x 全系在官方定义里就是 canary / API 不稳定**，不是「快到 1.0 了所以差不多稳」。

### 1.2 「1.0」到底意味着什么

1.0.0 = 2026-06-15，1.0.1 = 2026-06-29，1.0.2 = 2026-07-06（`CHANGELOG.md:5,19,50`）。
**截至 2026-07-17，1.0 只发布了 32 天。** 政策成文了，但还没被时间检验过——目前尚无任何一次 major 迁移的历史可参照。

1.0 带来的**具体**变化，能在代码里核到的：

1. **弃用代替删除**。`iroh-relay/src/tls.rs:28`：
   ```rust
   #[deprecated(since = "1.0.0", note = "Renamed to `CaTlsConfig`")]
   pub type CaRootsConfig = CaTlsConfig;
   ```
   同类还有 `iroh-relay/src/tls.rs:110`（`custom` → `custom_roots`）、
   `iroh/src/endpoint.rs:719`（`ca_roots_config` → `ca_tls_config`）。
   1.0.1 甚至专门补了一个 `fix(iroh): Add missing item for backwards compatibility (#4346)`（`CHANGELOG.md:25`）——
   说明改名后保留 alias 已经是他们的自觉动作。这是 0.x 时代看不到的纪律。

2. **公共类型标记 `non_exhaustive`**，在 0.98.0 就位：
   `refactor(iroh, iroh-relay)! Mark public types as non_exhaustive (#4107)`。为 1.x 内加字段留了空间。

3. **不再暴露第三方类型**：`iroh-base` 的
   `Don't expose third-party error types in iroh-base (#4073)`，roadmap 里叫 "Own all foreign types"。
   意义是：依赖升级不再从 iroh 的公开 API 漏出来变成你的 breaking change。

### 1.3 支持窗口 —— 免费支持 = 只有最新 minor（真实成本在这）

`release-policy.md` 的支持矩阵：

| Release | Full Support | Maintenance Mode | Extended Support |
|---------|--------------|------------------|------------------|
| Major | 1 year | 1–3 years after release | 付费（contact） |
| Minor | **3 months** | 3 months – 1 year after release | 付费（contact） |
| Canary / Experimental / RC | N/A | N/A | — |

四档定义（原文摘要）：Full Support = 及时修 bug/安全补丁；Extended Support = **付费**，带 SLA；
Maintenance Mode = 只在 n0 **自行裁量**下修关键问题；EOL = 完全不管。

官方自己举的例子（`release-policy.md` "Examples"，逐字要点）：

> 1. 如果你在生产用 `0.35`，它是超过 1 年的 minor，**要拿 bug 修复 / 补丁就得买 Extended Support**。
> 2. 如果你在超过 3 个月的版本里发现 bug，**团队可能只在最新 minor 上修**。
>    你若不想升到最新 minor 而要 backport，**必须购买 Extended Support 来支付 backport 的人力**。

**这条要单独想清楚。** 把它和 `Minor ≥4 weeks` 放一起，实际约束是：

- 想**免费**拿到安全补丁 ⇒ 你得跟着**最新 minor** 跑，而 minor 可能每 4 周就来一个。
- 落后 3 个月以上 ⇒ 要么升，要么付费 backport。
- 换句话说：**「1.0 稳定」保的是 API/wire 不乱动，不等于「你可以在 1.0.2 上待两年不动」**。

> **有自动更新通道的（桌面）这条压力可控；移动端是软肋** —— 应用商店审核 + 用户不升级，
> 天然会沉淀一批老版本客户端。
> 好消息是 §2.1 的 wire 承诺让老客户端「连得上」，坏消息是它们「拿不到补丁」。
> 这两件事是分开的，别混为一谈。

### 1.4 政策 vs 实际节奏（用 CHANGELOG 核实）

从 `CHANGELOG.md` 抽出的真实发布日期：

| 版本 | 日期 | 距上一个 minor |
|------|------|----------------|
| 1.0.2 | 2026-07-06 | patch（+7d） |
| 1.0.1 | 2026-06-29 | patch（+14d） |
| **1.0.0** | **2026-06-15** | — |
| 1.0.0-rc.1 | 2026-05-27 | — |
| 1.0.0-rc.0 | 2026-05-07 | — |
| 0.98.0 | 2026-04-17 | +32d |
| 0.97.0 | 2026-03-16 | +47d |
| 0.96.0 | 2026-01-28 | +85d |
| 0.95.0 | 2025-11-04 | +14d |
| 0.94.0 | 2025-10-21 | +12d |
| 0.93.0 | 2025-10-09 | +21d |
| 0.92.0 | 2025-09-18 | +50d |
| 0.91.0 | 2025-07-30 | +34d |
| 0.90.0 | 2025-06-26 | （从 0.35 跳版） |
| 0.35.0 | 2025-05-12 | — |

读数：

- 0.9x 时代 minor 间隔 **12 天 ~ 85 天**，有几次明显快于「≥4 周」的下限
  （0.93→0.94 = 12d，0.94→0.95 = 14d）。但那是 canary 期，政策不适用于它们。
- **1.0 之后还没出现过 minor**（截至 2026-07-17 只有 1.0.1 / 1.0.2 两个 patch）。
  所以「≥4 周 minor」这条在 1.x 上**尚无实测数据**。
- patch 很快（7~14 天），响应是实的。

### 1.5 LTS？

**没有 LTS。** `release-policy.md` 全文未出现 LTS / long-term support 字样。
最接近的是 **Extended Support（付费 + SLA）**，需要 <mailto:support@iroh.computer>。
另有 Support 页 <https://docs.iroh.computer/iroh-services/support.md>（本次未展开核实）。

### 1.6 MSRV

- 全 workspace 统一 `rust-version = "1.91"`（`iroh/Cargo.toml`、`iroh-base/`、`iroh-relay/`、`iroh-dns/` 均为 1.91）。
- **MSRV 政策：未找到。** README.md / CONTRIBUTING.md 里 grep 不到 MSRV / minimum supported 的任何表述，
  也就是说「MSRV bump 算不算 breaking」没有成文承诺。
- 1.91 是相当新的工具链，交叉编译/CI 镜像/移动端 NDK 工具链要跟得上。

### 1.7 ⚠️ unstable feature 不在 semver 承诺内

`endpoint.net_report()` 所依赖的 `unstable_net_report` 模块（`iroh/iroh/src/lib.rs:294-301`）：

```rust
#[cfg(feature = "unstable-net-report")]
pub mod unstable_net_report {
    //! This API is unstable and gated behind the `unstable-net-report` feature.
    //! It is not covered by semantic versioning guarantees and may change in any
    //! release without a major version bump.
```

导出 `Probe`、`RelayLatencies`、`Report as NetReport`。`iroh/iroh/Cargo.toml:164` `unstable-net-report = []`（**默认不开**）。`Report` 本身还带 `#[non_exhaustive]`（`report.rs:17`）。

**若要在 app 内做网络诊断上报（例如给用户看「当前 relay 延迟」），就得开这个 feature，等于把一个无 semver 保证的 API 引入产品。**

**建议**：把它包在自己的**薄适配层**后面，别让 `NetReport` 类型渗进 core 的公开接口或 uniffi 桥。

## 2. Compatibility（对长期维护存量客户端 = 生死线）

### 2.1 官方 wire 承诺（原文）

来源：`release-policy.md` "Wire Protocol Compatibility"：

> The wire protocol must remain backward-compatible with the *non-deprecated* parts of the *previous* major version series. It may break compatibility with versions older than the last.
>
> - `2.x` 的 wire 必须向后兼容 `1.x`
> - `3.x` 的 wire 必须向后兼容 `2.x`
> - `3.x` **可以**与 `1.x` 不兼容

同一 major 内所有 minor 必须互通：「`2.x` must connect with any `2.1` through `2.x`」。

| Version | Compatible with |
|---------|-----------------|
| 1.x | 1.0 ≤ 2.0.x |
| 2.x | 2.0 ≤ 3.0.x |

官方建议（原文要点）：
- 部署下一个 major（如 `v2.0`）前，**先确保所有设备升到上一个 major 的最新**（`v1.x`）。
- 部署下一个 minor（如 `v2.1`）前，先确保设备都到了 `v2.0`。

**注意 "non-deprecated parts" 这个限定词。** 承诺只覆盖上个 major 里**没被标弃用**的部分。
即：n0 可以在 1.x 里先把某段 wire 标弃用，然后 2.x 合法地不再兼容它。
**弃用即预告删除**——1.x 期间任何 wire 层弃用通告都要当作 2.0 的破坏性变更来跟踪。

### 2.2 0.x → 1.x：没有承诺，且已经实锤破坏

兼容表最低只到 `1.x`，**0.x 不在任何兼容范围内**。这不是推测，`CHANGELOG.md` 里有直接证据：

- `0.90.0`（2025-06-26）：
  `feat(iroh)! Remove deprecated x509 libp2p TLS authentication (#3330)` —— **认证握手方式被删**。
- `0.90.0`：`feat(iroh)! Introduce transport abstraction (#3279)`。
- `0.95.1` 引入 **QUIC multipath** 与 **QUIC NAT traversal**（roadmap 页确认）。
- `0.98.0`：`Update relay protocol to iroh-relay-v2 (#3955)`。

代码侧交叉验证 —— `iroh/src/tls.rs:1-6` 开头逐字：

```rust
//! TLS configuration for iroh.
//!
//! Currently there is one mechanism available:
//! - Raw Public Keys, using the TLS extension described in [RFC 7250]
```

**只剩 RFC 7250 Raw Public Keys 一种机制**，libp2p 风格的 x509 自签路径已彻底移除。
结论：**0.x 客户端和 1.x 客户端连不上，且没人承诺它们该连上。** 迁移到 iroh 必须做一次「全量客户端换代」。

> ⚠️ 这里有个文档陷阱：FAQ 至今仍写着「at the moment iroh uses self-signed certificates …
> borrowing the libp2p handshake specification. In the future, we plan on switching to the
> raw public key TLS certificate type (RFC 7250) instead」——
> **这段是过期的**，那个「future plan」在 0.90.0 就已落地。以 `iroh/src/tls.rs` 为准。

### 2.3 承诺覆盖谁：relay 协议有真协商，**你的应用协议归你自己**

这是最容易理解错的一层。iroh 的 wire 承诺覆盖的是 **iroh 自己的协议**，不是你在它上面跑的协议。

**(a) relay 协议：有真实的版本协商 + 帧级门控（不是嘴上说说）**

`iroh-relay/src/http.rs:50-68`：

```rust
pub enum ProtocolVersion {
    /// Version 1 (the only version supported until iroh 0.98.0)
    #[strum(serialize = "iroh-relay-v1")]
    V1,
    /// Version 2 (added in iroh 0.98.0)
    /// - Removed `Health` frame (id 11)
    /// - Added `Status` frame (id 13)
    #[default]
    #[strum(serialize = "iroh-relay-v2")]
    V2,
}

impl ProtocolVersion {
    /// All supported protocol versions, in order of preference (newest first).
    pub const ALL: &'static [Self] = &[Self::V2, Self::V1];
```

协商走 WebSocket 子协议（`Sec-WebSocket-Protocol`）：
- 客户端把支持的版本全发出去，读服务端回的版本（`iroh-relay/src/client.rs:345-355`），
  对不上报 `ConnectError::BadVersionHeader`。
- 服务端取交集里的**最大值**（`iroh-relay/src/server/http_server.rs:612-622`）：
  ```rust
  let protocol_version = subprotocols
      .split(",")
      .map(|s| s.trim())
      .filter_map(ProtocolVersion::match_from_str)
      .max()
      .ok_or_else(|| e!(RelayUpgradeReqError::UnsupportedRelayVersion { .. }))?;
  ```
- 帧按版本严格门控（`iroh-relay/src/protos/relay.rs:428-461`）：
  `Health` 帧要求 `protocol_version == V1`，`Status` 帧要求 `>= V2`，
  否则 `Error::FrameNotAllowedInVersion`。并且有测试兜底：`v1health_rejected_in_v2`、`status_rejected_in_v1`。
- 被删的 `Health` 变体**没有**用 `#[deprecated]`（会污染 serde derive），而是留注释
  `/// Removed since relay-protocol-v2:`（`relay.rs:105-107`）。

**判断**：`iroh-relay-v1` 到今天仍在 `ALL` 里可协商，说明兼容承诺是有工程支撑的，不是 PR 稿。
但也要注意：V1 是 0.x 时代的产物，按 §2.1「N 兼容 N-1」，**2.0 完全有权把 V1 摘掉**。

**(b) 你的应用协议：ALPN 精确匹配，版本化是你的活**

`iroh/src/protocol.rs:381-382` —— 分发就是一次 HashMap 精确查表：

```rust
pub(crate) fn get(&self, alpn: &[u8]) -> Option<&dyn DynProtocolHandler> {
    self.0.get(alpn).map(|p| &**p)
}
```

**没有前缀匹配、没有通配、没有版本协商。** ALPN 对不上 → `protocols.get(&alpn)` 返回 `None` → 连接被拒。

生态自己就是这么干的，把版本号焊进 ALPN 字符串：

| crate | ALPN | 位置 |
|-------|------|------|
| iroh-blobs | `/iroh-bytes/4` | `iroh-blobs/src/protocol.rs:406` |
| iroh-gossip | `/iroh-gossip/1` | `iroh-gossip/src/net.rs:45` |
| iroh-docs | `/iroh-sync/1` | `iroh-docs/src/net.rs:18` |

`/iroh-bytes/**4**` 说明 blobs 的 wire 已经迭代到第 4 个版本（即此前至少断代过 3 次）。

**直接含义**：iroh 的 wire 承诺**一点也不保护你自己的传输协议**。
你的 ALPN 自己定、自己版本化。好消息是这个失败模式是**干净的**——
版本不匹配直接连不上，而不是连上后数据错乱。要平滑升级，就得让新端同时注册新旧两个 ALPN。

### 2.4 ⚠️ 生态 crate 不在 1.0 承诺内 —— 最容易误判的一条

`release-policy.md` 通篇只说 "how **iroh** is versioned"。而实际版本分布（本地 24 仓实测）：

| 仓库 | 版本 | 最后提交 | 在 1.0 承诺内？ |
|------|------|----------|-----------------|
| **iroh** | **1.0.2** | 2026-07-16 | ✅ |
| iroh-tickets | 1.0.0 | 2026-06-15 | ✅ |
| n0-error | 1.0.0 | 2026-06-15 | ✅ |
| n0-watcher | 1.0.0 | 2026-07-09 | ✅ |
| iroh-ffi | 1.1.0 | 2026-07-16 | ✅（依赖 `iroh = "1.0.0"`）|
| **iroh-blobs** | **0.103.0** | 2026-06-15 | ❌ **0.x** |
| **iroh-gossip** | **0.101.0** | 2026-06-15 | ❌ **0.x** |
| **iroh-docs** | **0.101.0** | 2026-07-15 | ❌ **0.x** |
| iroh-doctor | 0.101.0 | 2026-06-24 | ❌ 0.x |
| iroh-c-ffi | 0.101.0 | 2026-06-25 | ❌ 0.x |
| irpc | 0.17.0 | 2026-07-01 | ❌ 0.x |
| n0-future | 0.3.2 | 2026-06-12 | ❌ 0.x |
| n0-mainline | 0.5.0 | 2026-06-15 | ❌ 0.x |
| iroh-mainline-address-lookup | 0.4.0 | 2026-07-10 | ❌ 0.x |
| iroh-mdns-address-lookup | 0.4.0 | 2026-07-10 | ❌ 0.x |
| bao-tree | 0.16.0 | 2025-11-04 | ❌ 0.x |
| quic-rpc | 0.20.0 | **2025-05-12** | ❌ 0.x，14 个月没动 |
| sendme | 0.36.0 | 2026-06-15 | ❌ 0.x（应用）|
| dumbpipe | 0.39.0 | 2026-06-24 | ❌ 0.x（应用）|
| swarm-discovery | 0.6.0-alpha.2 | 2026-04-15 | ❌ alpha |
| iroh-js | 0.0.1 | **2023-12-07** | ❌ 事实上已死 |

**「iroh 1.0 了」≠「iroh-blobs 1.0 了」。**
`iroh-blobs` 0.103.0 是 0.x：按 Cargo semver，0.x 的每个 minor 都可以是 breaking；
按 §1.1 官方定义，0.x 属于 **canary（API 不稳定）**；按 §1.3，0.x **没有 Full Support 档位**。

> 判断：如果引入 `iroh-blobs` 做文件传输，你拿到的稳定性等级是
> **0.x + canary + 无支持承诺**，而不是 1.0。核心 `iroh` 的 1.0 保不住 blobs。
> 要么接受 blobs 的 0.x 节奏（ALPN 已经断代到 4），要么只用 `iroh` 核心自己写传输协议
> ——若你已有自己的 chunk/加密/断点续传实现，后者反而更可控。

### 2.5 默认 relay 地址被编进二进制，且**会漂移**

`iroh/src/defaults.rs:26-33` —— 当前 1.0.2 的生产 relay：

```rust
pub const NA_EAST_RELAY_HOSTNAME: &str = "use1-1.relay.n0.iroh.link.";
pub const NA_WEST_RELAY_HOSTNAME: &str = "usw1-1.relay.n0.iroh.link.";
pub const EU_RELAY_HOSTNAME:      &str = "euc1-1.relay.n0.iroh.link.";
pub const AP_RELAY_HOSTNAME:      &str = "aps1-1.relay.n0.iroh.link.";
```

`default_relay_map()`（`defaults.rs:36-43`）返回这 **4 个**。
staging 另有 2 个：`use1-1.staging-relay.n0.iroh.link.` / `euc1-1.staging-relay.n0.iroh.link.`（`defaults.rs:87-99`）。

**这些 hostname 至少变过两次**：
- `0.90.0`：`chore(iroh)! Change default relays to new "canary" relays (#3368)`
- `1.0.0`：`feat: Update relay urls to 1.0 stable (#4341)`

佐证：官方 troubleshooting 页的 `iroh-doctor report` 示例里印的还是老域名
`aps1-1.relay.iroh.network` / `use1-1.relay.iroh.network` / `euw1-1.relay.iroh.network`
（<https://docs.iroh.computer/troubleshooting.md>），而代码里早已是 `*.relay.n0.iroh.link.`。

**风险链条**：默认 RelayMap 是**编译期常量**，会随二进制固化在用户设备上。
一个 2025 年发出去的老客户端，里面焊的是老域名。
即使 §2.1 的 wire 兼容成立，**只要 n0 停掉老 hostname 的 DNS，老客户端照样失联**——
这条风险绕过了 wire 兼容承诺，是独立的。

叠加 `release-policy.md` 最后一句：

> Number 0 runs public relays for the **latest major version** of iroh.

公共 relay 只保**最新 major**。老版本要 relay ⇒ 自建，或买 dedicated relay。

> **直接建议**：只要你已经在自建基础设施，就别用 `default_relay_map()`。
> **显式配置自己的 relay URL**，把「n0 改域名 / 停老 relay / 限流」这三条外部风险一次性摘掉。公共 relay 官方也明说了是给开发测试用、**有 rate-limit**（FAQ）。

### 2.6 平台 / 硬件 / 传输兼容性 —— 市场话术与实现的差距

`compatibility.md` 的表格给的都是 "Yes"，但逐条核到代码后水分不小：

**操作系统**（`compatibility.md`）：Linux / macOS / Windows / Android / iOS / WebAssembly(browser) / FreeRTOS 全 "Yes"。

- ⚠️ **FreeRTOS "Yes" + ESP32 "Supported with caveats"（4 MiB Flash / 2–4 MiB RAM）
  在开源仓里找不到任何对应物**。
  实测：`rg -ni 'esp32|freertos|xtensa|esp-idf'` 扫全仓 **0 命中**。
  文档自己也写着「To use ESP32 in production, **contact us for licensing & support**」——
  即这是**商业/闭源**产物，不在 `MIT OR Apache-2.0` 的 `iroh` 里。**别把它算进开源可用能力**。

**传输**（`compatibility.md` 全标 "Yes"）——与仓内注册表 `iroh/TRANSPORTS.md` 对照：

| transport id | transport | repo | status（TRANSPORTS.md 原文）|
|---|---|---|---|
| 0x00-0x1F | - | - | reserved |
| 0x20 | Test | iroh (test-utils) | internal |
| 0x544F52 | Tor | [iroh-tor](https://github.com/n0-computer/iroh-tor) | **experimental** |
| 0x424C45 | BLE | *（空）* | **reserved** |

- **BLE**：`compatibility.md` 标 "Yes"，但 `TRANSPORTS.md` 里状态是 **reserved 且 repo 栏是空的**。
  文档页 <https://docs.iroh.computer/transports/bluetooth.md> 指向的是**第三方**仓
  `mcginty/iroh-ble-transport`（**不是 n0-computer**），并且：
  - **AGPL 许可**，「Commercial licenses are available for use cases where the AGPL is not suitable」；
  - 性能：**每设备约 3–5 条连接，每连接约 100 kbps**；
  - 明确警告：「Both iroh's custom transport API and this crate are experimental. Expect breaking changes.」

  > 对**闭源分发 + 文件传输**类产品来说，AGPL 是**许可证地雷**，100 kbps 对传文件也基本无意义。
  > 「iroh 支持蓝牙所以能做离线传输」是个**误判**，别基于它做规划。

- **Tor** / **Nym**：都是 n0-computer 自己的仓（`iroh-tor-transport` / `iroh-nym-transport`），
  但两页都挂着同一条警告：**custom transport API 与这两个 crate 均为 experimental，预期 breaking changes**。

**硬件**：x86_64 / Apple Silicon / Raspberry Pi = Fully supported；ESP32 = 见上。

**总结**：`compatibility.md` 是**销售页**，`iroh/TRANSPORTS.md` 才是**工程注册表**。两者冲突时信后者。

## 3. Roadmap

来源：<https://docs.iroh.computer/about/roadmap.md>
（307 跳转 → <https://iroh.computer/roadmap> → 308 → <https://www.iroh.computer/roadmap>）

### 3.1 ⚠️ 这个页面已经过期了

页面标题仍是「**iroh 1.0 roadmap**」，页内自述 **"Last Updated: February 9, 2026"**。
它把 `v1.0` 排在 **Q1 2026**，而 `CHANGELOG.md:50` 显示 **1.0.0 实际发布于 2026-06-15（Q2）**。

对照它的排期与真实发布：

| roadmap 计划 | 实际（CHANGELOG） | 偏差 |
|---|---|---|
| `v0.97.0` @ 2026-02-23（release candidate）| 0.97.0 @ 2026-03-16 | 晚 3 周，且**不是** rc |
| （未出现在 roadmap 上）| **0.98.0 @ 2026-04-17** | roadmap 根本没预见这一版 |
| `v1.0.0-rc.0` @ Q1 2026 | 1.0.0-rc.0 @ 2026-05-07 | 滑到 Q2 |
| `v1.0` @ Q1 2026 | **1.0.0 @ 2026-06-15** | **滑一个季度** |

页面结尾还留着 "Party — That's it. All done. no more work left to do, ever. :)"。

**结论：官方没有「1.0 之后」的公开路线图。** 这个页面是 1.0 前的产物，1.0 落地后没有更新。
**不要拿它规划任何事。**

### 3.2 页面上列的「1.0 前 future work」——完成情况未知

roadmap 的 "future work" 区块（逐字要点）：

- **draft specification** —— 「draft a specification for the iroh protocol, outlining all open standards
  iroh uses, noting any deviations」，列举：self-signed TLS / QUIC / ICE over QUIC / STUN over QUIC /
  DNS Discovery / Pkarr / MDNs / WebSockets / iroh relay
- **finalize 1.0 spec** —— 「**ratify the iroh 1.0 wire protocol**」
- **Finalize FFI integration**
- **documentation refinement** —— 「ensure documentation is accurate and robust」

**核实结果：这份 wire 协议规范，我找不到。**
- `docs.iroh.computer/llms.txt`（73 行完整索引）里**没有任何 spec / 协议规范页面**；
- `iroh` 仓内 `find . -iname '*spec*'` **0 命中**。

**⇒ 「ratify the iroh 1.0 wire protocol」这件事没有可见产出。**
实践含义：**wire 兼容承诺目前的唯一权威实现就是 Rust 代码本身**，没有独立规范可对照。
想写第三方互操作实现（比如 Go），只能读源码——这也解释了 FAQ 为什么说没有官方 Go 版本。

至于 "documentation refinement" 这条：从 §5.1 那张不一致清单看，**显然还没做完**。

### 3.3 已经落地的大变动（roadmap 标 past，可在 CHANGELOG 交叉核实）

- **QUIC multipath**（`v0.95.1`，2025-11-05）+ **QUIC NAT traversal**
- **`EndpointHooks`**：连接生命周期回调，用于鉴权与连接审查
- **`Discovery` → `AddressLookup` 重命名**（`v0.95.1`）
- **Custom Transports**（`v0.96.0`，2026-01-28）
- **Own all foreign types**（`v0.96.0`）
- **`snafu`/`n0-snafu` → `n0-error`**
- **`Connection::alpn` / `Connection::remote_id` 改为 infallible**
- **`iroh-blobs` 可编译到 WASM**

> 这批命名大改（`Discovery`→`AddressLookup`、`NodeId`→`EndpointId` 等）都发生在 **1.0 之前**。
> 好消息：现在进场不用再吃这轮改名。坏消息：**网上 2025 年及更早的 iroh 教程/博客/AI 记忆基本全是旧 API**，
> 照抄必崩。以本地 1.0.2 源码为准。

### 3.4 已知的下一个大变动

- **2.0 的时间**：政策 `Major ≥6 months` ⇒ 最早 ~2026-12（1.0.0 + 6 个月）。**官方无公开日期。**
- **2.0 可能摘掉的东西**（按 §2.1「只保上个 major 的 non-deprecated 部分」推断）：
  - `iroh-relay-v1` 协议（`ProtocolVersion::V1`，0.x 时代产物）
  - 所有 `#[deprecated(since = "1.0.0")]` 的 API（`CaRootsConfig`、`ca_roots_config`、`custom` 等）
- **post-quantum**：FAQ 明确**当前不支持**，且说明了为何不做（Xyber 公钥比 Ed25519 大 37 倍，
  握手塞不进单个 UDP 包，DNS 包会分片，EndpointId 也会大 37 倍）。
  措辞是「follow developments closely」——**没有排期**。
- **Ed25519 之外的密钥**：FAQ 明确 **No**，且称是有意为之（Ed25519 已深度耦合 Pkarr 签名 / mTLS raw public key / relay 认证）。

## 4. Troubleshooting（官方排障路径）

来源：<https://docs.iroh.computer/troubleshooting.md>

官方只给了 **3 条**路径，非常薄：

### 4.1 日志

iroh 用 `tracing`。官方给的最小配置：

```bash
cargo add tracing-subscriber
```

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // ...existing code...
}
```

```bash
RUST_LOG=iroh=info cargo run     # 概况
RUST_LOG=iroh=debug cargo run    # 细节
```

> 已经在用 `tracing` + `EnvFilter` 的项目，加一段 `iroh=debug` 即可，零额外成本。
>
> 注意 1.0.1 的 `fix(logging): Do not use span levels higher than info (#4375)`
> —— 之前 span level 用得过高会污染日志，**1.0.1 才修**。低于 1.0.1 的版本调日志会更吵。

### 4.2 `iroh-doctor`

```
cargo install iroh-doctor
iroh-doctor report
```

**本地核实**（`/Volumes/yexiyue/iroh-study/iroh-doctor`）：
- 版本 `0.101.0`，依赖 `iroh = { version = "1.0.0", features = ["metrics", "unstable-net-report"] }`
- 最后提交 2026-06-24（`ci: add semver check (#82)`）—— **对 1.0 是跟得上的，活的**
- ⚠️ 但它自己仍是 **0.x**，不在 1.0 承诺内（§2.4）；且用了 `unstable-net-report` feature

`iroh-doctor report` 输出的关键字段（官方示例，注意里面的域名是**过期的**，见 §2.5）：

```
Report {
    udp: true,
    ipv6: true, ipv4: true,
    ipv6_can_send: true, ipv4_can_send: true,
    os_has_ipv6: true,
    mapping_varies_by_dest_ip: Some(false),   // ← true = 对称 NAT，打洞很难
    hair_pinning: Some(false),
    portmap_probe: Some(ProbeOutput { upnp: false, pcp: false, nat_pmp: false }),
    preferred_relay: Some(RelayUrl("https://use1-1.relay.iroh.network./")),
    relay_latency: RelayLatencies({ ... }),
    global_v4: Some(72.**.**.**:54696),
    global_v6: Some([2600:**:**:***::100b]:53549),
    captive_portal: None,
}
```

关于 home relay，官方原文：

> All iroh endpoints will maintain a single home relay server that they're reachable at.
> On startup iroh will probe its configured relays & choose the one with the lowest latency.

排障时最该先看的几个：
- `udp: false` ⇒ UDP 被封，只能走 relay（FAQ 也确认：只放 TCP 出站的防火墙下必然回落 relay）
- `mapping_varies_by_dest_ip: true` ⇒ 对称 NAT，直连概率大跌
- `preferred_relay` + `relay_latency` ⇒ 选到的 home relay 是否合理（跨洲 = 延迟爆炸）
- `captive_portal` ⇒ 强制门户劫持

### 4.3 Network Diagnostics（**付费产品**）

官方第三条路径是 <https://docs.iroh.computer/iroh-services/net-diagnostics/usage.md>：
跑与 `iroh-doctor` **同类**的探测（UDP 连通性 / NAT 类型 / relay 延迟 / 端口映射），
但由 **Iroh Services 控制台**远程触发到你项目的在线 endpoint，报告回到控制台
——「without asking them to run a CLI tool」。

要求：给 endpoint 授予 `NetDiagnosticsCap::GetAny` capability 并运行一个 `ClientHost`。

> 即：**生产环境「查用户为什么连不上」是 n0 的商业化抓手。**
> 开源侧你只有「让用户自己跑 iroh-doctor」或「自己埋点」。
> 这不影响技术选型，但要计入运维成本预期。

### 4.4 官方没覆盖、但你一定会撞上的

`troubleshooting.md` 只有上面三节，**以下均未找到官方排障文档**：
`BindError` / `AcceptError` / crypto provider 未安装 / wasm 下的行为差异 / 移动端后台被挂起。
这些请看本 skill 的对应深水区章节。

### 4.5 iroh-doctor 深入：子命令、可用性与三个「看起来能用其实不能」

> **成熟度**：production，但**是排障 CLI，不是可嵌入的库** —— 独立 0.x 版本线、消费
> `unstable-net-report`（无 semver 保证）。判定依据 → [index-ecosystem-map.md](index-ecosystem-map.md)。
> **把 doctor 指向自建 relay 的配置** → [07-configuration.md](07-configuration.md)。

#### 7 个子命令

`report` / `accept` / `connect` / `port-map-probe` / `port-map` / `relay-urls` / `swarm-client`

> ⚠️ **README 的命令列表已过时**：文档里列的 `plot` 子命令在代码中**不存在**（全仓 `grep -rn "Plot" src/` **零命中**），而实际存在的 `swarm-client` 未被列出；README 也漏掉了 `--service-node` / `--ssh-key` 两个全局选项（`src/main.rs` 的 Cli struct：`service_node: Option<EndpointId>`（`#[clap(long, requires("ssh_key"))]`）与 `ssh_key: Option<PathBuf>`）。
>
> **以 `--help` 与源码的 `Commands` enum 为准。**

#### 迁移评估的第一步

```sh
# 1) 网络环境总览：逐 relay 延迟 + captive portal + NAT 映射行为
#    无 config 文件时自动使用 n0 默认 4 个 relay（含 AP 的 aps1-1）
iroh-doctor report

# 2) 直接对每个 relay 测 connect 与 Ping/Pong 延迟（5 轮）
iroh-doctor relay-urls --count 5
#    输出形如：Node https://aps1-1.relay.n0.iroh.link./
#              Connect: 231ms
#              Latency: 198ms
#    失败则：   Connection Error: "..."

# 3) 两端实测连通性 / 吞吐 / relay→直连 切换
iroh-doctor accept --secret-key local                  # A 机，打印 endpoint-id
iroh-doctor connect <endpoint-id> --relay-url <url>    # B 机
#   → 输出 "Connection with <id> changed: Ip(..)/Relay(..) (after ..)"
```

**`report` 打出的 `Report` 结构**（`iroh/iroh/src/net_report/report.rs:18`，第 19-37 行公开字段）：

| 字段 | 含义 |
|---|---|
| `udp_v4` / `udp_v6: bool` | UDP 通不通 |
| `mapping_varies_by_dest_ipv4` / `_ipv6: Option<bool>` | NAT 是否按目的地变映射 |
| `preferred_relay: Option<RelayUrl>` | 选出的 home relay |
| **`relay_latency: RelayLatencies`** | doc: *"The measured latency to each relay, keyed by relay URL"* |
| `global_v4` / `global_v6` | 发现的公网地址 |
| **`captive_portal: Option<bool>`** | 是否被劫持 |

`iroh-doctor/src/commands/report.rs:48-51` 即 `let mut stream = endpoint.net_report().stream(); while let Some(report) = stream.next().await { println!("{report:#?}"); }`。

**对中国网络排查确实有用，且是最省事的入口**：一条命令同时得到「UDP 通不通 / 各 relay 延迟 / 是否被劫持 / NAT 是否按目的地变映射」。`captive_portal` 对国内酒店、校园网场景尤其有价值。

#### ⚠️ 三个「看起来能用，其实不能」

##### 1. NAT 类型分类器从未被任何 CLI 命令调用

`iroh-doctor/src/nat_classifier.rs` 里有 Easy/Medium/Hard 分类器，但：

全仓 grep `classify_nat_type|nat_classifier|NatType` 排除 nat_classifier.rs 自身后，**唯一命中是 `src/lib.rs:6` 的 `pub mod nat_classifier;`** —— 即除了模块声明，**无任何调用点**。`src/doctor.rs` 的 `pub async fn run`（739 行起）逐个 match 七个子命令，**无一处触及 NAT 分类**。`classify_nat_type` 仅在 nat_classifier.rs 自己的 `#[cfg(test)] mod tests` 里被调用。

**「iroh-doctor 能告诉你 NAT 类型」对 CLI 用户来说是错的。**

##### 2. 即使接上，也永远不会返回 Easy

`iroh-doctor/src/swarm/net_report_ext.rs:13/16` 两字段的 doc 明写 *"Whether the NAT mapping varies by destination PORT for IPv4 (**not implemented**)"*；`from_base_report`（:22-27）把 `mapping_varies_by_dest_port_ipv4/_ipv6` **恒置为 `None`**。生产路径上构造 ExtendedNetworkReport 的两处（`swarm/client.rs:92` 与 :123）都走 from_base_report。

而 `nat_classifier.rs:76-84` 的 match 表中，**Easy 唯一入口是 `(Some(false), Some(false))`** —— port 项恒为 None 时只能落到 `(Some(false), None) => Medium` 或 `(Some(true), None) => Hard`。只有单测（:116/:130）手动赋值 `Some(..)`。

**抄这张 match 表时必须自己补齐「同一 NAT 对不同目的端口是否给出不同映射」的探测**，否则最好的网络也只会被判成 Medium，**NAT 画像数据会系统性偏悲观**。

##### 3. `report` 的三个 flag 是装饰性的

`--quic-ipv4` / `--quic-ipv6` / `--https` **只被 println 打印，从未传给 endpoint**。

`iroh-doctor/src/commands/report.rs` 中这三个变量的全部命中集中在：:11-13（函数参数）、:16-19（全 false 则全置 true）、:22-29（`if quic_ipv4 { println!("quic ipv4") }` 等）。第 33-36 行构造 endpoint 时**只调了 `.relay_mode(...)`**：

```rust
Endpoint::builder(presets::N0).relay_mode(RelayMode::Custom(relay_map.clone())).bind().await?
```

—— **没有任何 `.net_report_config(...)`**（而该 setter 确实存在于 `iroh/iroh/src/endpoint.rs:798`）。故 report 恒以 `NetReportConfig::default()`（https_probes=true, captive_portal_check=true）运行。

**含义**：**别以为 `--https` 能单独隔离 HTTPS 探测来判断「是不是 QUIC/UDP 被封」** —— flag 无效，你拿到的永远是全量探测结果。要区分只能读 report 输出里 relay_latency 各条目的来源，或自己写代码传 `NetReportConfig{https_probes:false}`。

#### ⚠️ relay-urls 的超时硬编码 2 秒

`iroh-doctor/src/commands/relay_urls.rs`（全文 153 行）中 `tokio::time::timeout(Duration::from_secs(2), client_builder.connect())` 用于连接；`ping()` 函数内同样 `Duration::from_secs(2)` 等 Pong。**CLI 侧 RelayUrls 只暴露 `--count`（`doctor.rs` 中 `#[clap(long, default_value_t = 5)] count: usize`），无超时参数。**

**国内 → aps1-1 的 RTT 在丢包时冲破 2s 并不罕见。** 用 relay-urls 得国内 relay 结论时，**别只看通过/失败**，要交叉验证 `report` 里的 relay_latency，否则可能得出「n0 relay 在中国完全不可用」这个**过强**的结论。

#### ⚠️ report 是持续输出，不会自行退出

`report.rs:48-51` 订阅 net_report 的 Watcher stream 循环打印，循环结束后才 `endpoint.close().await`。完整报告间隔见 `iroh/iroh/src/net_report.rs:132` `const FULL_REPORT_INTERVAL: Duration = Duration::from_secs(5 * 60);`。

**脚本化采集国内网络数据时（例如让多个用户跑一遍回传），别直接 `iroh-doctor report > out.txt` 就等它结束** —— 需要加 timeout 或取首份报告。首份报告很快出，后续每 5 分钟刷新一次。

#### ⚠️ swarm-client 依赖不开源的 coordinator

**这是 doctor 里唯一真正锁定 n0 的部分。**

`iroh-doctor/src/swarm/rpc.rs` **只定义了客户端**：`pub(super) type DoctorServiceClient = irpc::Client<DoctorProtocol>;`（:206）与 `pub struct DoctorClient`（:19），`DoctorClient::with_ssh_key`（:35）通过 `rcan::Rcan` 能力票据认证后 `.connect(coordinator_addr, N0DES_DOCTOR_ALPN)`（:70）。

`DoctorProtocol` 的**八个 RPC**（:459-476，Auth/Register/GetAssignments/CreateTestRun/ReportResult/GetNodeInfo/MarkTestStarted/GetTestRunStatus）**没有任何服务端 handler 实现**。仓内唯一的 ProtocolHandler 是 `swarm/runner.rs:35-37` 的 `SwarmProtocolHandler`，它注册的是节点间测试用的 `DOCTOR_SWARM_ALPN`（:247），**不是 coordinator**。

CLI 也强制要求 `--ssh-key` 与 `--coordinator <EndpointId>`（doctor.rs 中均为 `required = true`）。

ALPN 是 `n0/n0des-doctor/1` —— 即 **n0des 产品的闭源后端**。

**「组织一群国内节点跑分布式连通性矩阵测试」这个诱人用法用不了。** 要做国内多点连通率统计，只能自己写：doctor 的 accept/connect 两两对测 + 自建调度。

## 5. FAQ 里值得知道的

来源：<https://docs.iroh.computer/about/faq.md>

### 5.1 ⚠️ 先说 FAQ 自己的坑（逐条核过代码）

FAQ 有**过期内容**和**自相矛盾**，用之前先看这张表：

| FAQ 说法 | 实际 | 证据 |
|---|---|---|
| 「at the moment iroh uses self-signed certificates … borrowing the libp2p handshake specification. In the future, we plan on switching to RFC 7250」 | **已经切完了**。当前**只有** RFC 7250 Raw Public Keys | `iroh/src/tls.rs:1-6`；`0.90.0` 的 `Remove deprecated x509 libp2p TLS authentication (#3330)` |
| 「By default iroh is configured with **3** relay servers」 | **4 个** | `iroh/src/defaults.rs:36-43` |
| 「configured with **four** public relay servers（two US, one EU, one Asia）」 | ✅ 这条对（同一份 FAQ 里 3 和 4 自相矛盾）| `defaults.rs:26-33` |
| 「Both WebRTC and iroh work in browsers」 vs 同一答案结尾「**iroh doesn't run in the browser**」 | **自相矛盾**。正确理解：iroh **能**跑在浏览器（可编到 wasm32），但**不能打洞**，只能走 relay | FAQ 自己也写了「WebRTC remains the only choice for hole-punched connections due to the current state of Web APIs」 |
| troubleshooting 示例里的 `*.relay.iroh.network` | 已换成 `*.relay.n0.iroh.link.` | `defaults.rs:26-33`；`1.0.0` 的 `Update relay urls to 1.0 stable (#4341)` |

呼应 §3.2：roadmap 把 "documentation refinement / ensure documentation is accurate" 列为 1.0 前的活。
**从上表看，这条没干完。** 一般规律：**docs 站落后于代码，冲突时以本地 1.0.2 源码为准。**

### 5.2 relay：架构与成本

- **约 9/10 的连接能直连**，relay 只是垫脚石。
  > ⚠️ 这个 "roughly 9 out of 10" 在 FAQ 里出现两次（relay 答案 + WebRTC 对比），
  > 但**没有给出测量方法、样本或数据来源**。当市场数字看，别当 SLA。
- relay 是 **stateless** 的：只转发加密包、不存任何东西 ⇒ 便宜、易扩、无 DB 同步、自动 failover。
- relay **读不到流量**：QUIC/TLS 1.3 端到端加密，「从我们 QUIC 实现的角度看，relay 就是另一个 UDP socket」。
  relay 理论上能看到「X 和 Y 在通信 + 字节数」（**仅限直连建立之前**），官方称**不记录**这些数据。
- 公共 relay **有 rate limit**（防滥用），定位是 dev/testing。生产建议 dedicated 或自建。
- 自建门槛：**公网 IP + 指向它的 DNS 名**，内置 ACME 自动 TLS。
- 自建**不影响互通**：「Running your own relay doesn't affect interoperability」——
  relay 无状态、逻辑在客户端，可独立替换。
- 跑公共 relay 的风险主要是**流量**，不是安全：relay **没有到公网的 egress**，
  「像 Tor 的 guard/middle relay，不是 exit node」。

### 5.3 端口与网络

- **两个 UDP 端口**（IPv4 一个、IPv6 一个），用于直连，可通过 `endpoint::Builder` 配置。
- 只允许 TCP 出站的防火墙下 iroh **能工作，但拿不到直连**，全部回落 relay。
- 可能**同时连多个 relay**（当对端 home relay 与你不同），每条各占一个 TCP socket。

### 5.4 无 relay / 离线

可以。`EndpointAddr` 里带 "direct addresses" 时，iroh 会直接用这些地址连（有无 relay 都行）。
同一局域网可开 **local network address lookup（mDNS）**，即使 `EndpointAddr` 里没有直连地址也能连上。

> 这条对**局域网直传**场景直接对应——等价于 libp2p 里 mDNS 那条路径。

### 5.5 与 libp2p 的对比（**FAQ 官方口径**，与本 skill 心智差异章互为印证）

FAQ 自己承认的分工：

**iroh 强的地方**：
- **Peer discovery / NAT traversal**：「Getting peers to find each other reliably is one of the
  hardest parts of libp2p in practice; iroh has largely solved this」
- **直接消息**：可靠加密的 QUIC 连接直达指定 peer
- **Gossip**：`iroh-gossip` 提供 gossipsub-like fan-out
- **Blob transfer**：内置 hash 寻址传输协议

**libp2p 强的地方（FAQ 直认）**：
- **DHT**：「Libp2p has a Kademlia-based DHT; **iroh does not**.
  If your design depends on DHT-based routing or content discovery, **that's a gap**.」

FAQ 还解释了取舍来由：iroh 由**深度参与 libp2p 的开发者**创立，因为多年做传统 P2P 后有
「**abstraction fatigue**」；所以「where many P2P networks ship their own DHT for discovery,
iroh resisted that temptation」，转而复用**已有的最大 DHT**——BitTorrent Mainline DHT——做 address lookup。

> **直接影响**：若你在 libp2p 上用 Kademlia DHT 做过「短码 → SHA256(code) → DHT 记录」这类通用 KV 用法，
> **iroh 没有内置 DHT，这个能力没有 1:1 替代。**
> Mainline DHT（`n0-mainline` 0.5.0 / `iroh-mainline-address-lookup` 0.4.0）只做
> **EndpointId → 地址**的解析，**不是**通用 KV 存储，搬不过来。
> 详见本 skill 的 address_lookup 章节。这是选型时最容易低估的一处工作量。

### 5.6 Mainline DHT 的两个要点

- **默认关闭**，必须显式开启。原因之一：**免得移动 App 看起来像 BitTorrent 客户端而被 OS 标记**。
  > 这条对移动端是硬约束级别的提示。
- 开启后，**任何 Mainline 节点都可能响应**：每条 [BEP 44](https://www.bittorrent.org/beps/bep_0044.html)
  记录存在 **20 个随机 Mainline 节点**上。所以「一个跑得够久、在路由表里的 BitTorrent 客户端会响应你的查询」——
  **是的，会**。

### 5.7 安全边界（说得很清楚，值得照抄进威胁模型）

- E2E 加密**永远开启**，无需配置。QUIC + TLS 1.3，forward & backward secret，双向认证。
- **前提假设**：你连的那个 EndpointId 是**安全交换**来的（扫码 / 加密聊天里发链接 / 可信服务器），
  且私钥没泄露。**iroh 不解决 EndpointId 的分发信任问题**——这是你的活。
- **0-RTT 有前向保密的注意事项**（`Connecting::into_0rtt` 文档里写了），opt-in。
- **非后量子安全**（§3.4）。
- 准入控制走 **endpoint hooks**：可在接受连接前拦截，按 EndpointId / 自定义 allowlist / 任意业务策略放行或拒绝。
  > 「只接受已配对设备」这类配对模型，挂载点就在这里。

### 5.8 与 Tailscale 的差别（选型常见混淆）

- **Tailscale**：**global to your device**，建一个网络接口，机器上所有 App 共用。
- **iroh**：**embedded into each individual application**，连接性活在你的 App 里。
- 关键差异：iroh **不需要装 VPN 或 daemon**，「You can ship an Android or iOS app that uses iroh
  direct connections under the hood, and the person using it never has to know or care that iroh is involved」。

> 「应用内连接、无 daemon」正是 iroh 的设计目标场景。

### 5.9 语言绑定

- 重心是 **Rust**；可直接用于 Rust / C / C++，并可嵌入 JavaScript / Python / Swift / Kotlin。
- **没有官方 Go 版本**。理论上可行（QUIC + multipath 扩展 + 少量自定义 TLS 逻辑），
  有第三方 [go-iroh](https://github.com/tmc/go-iroh)，但「Our own focus stays on the Rust implementation」。
  > 呼应 §3.2：**没有公开 wire 规范**，第三方实现只能读 Rust 源码——这也是 Go 版本难产的根因。
- 本地核实：`iroh-ffi` **1.1.0**（2026-07-16，依赖 `iroh = "1.0.0"`）是活的、跟得上 1.0 的；
  而 `iroh-js` 仓是 **0.0.1 / 最后提交 2023-12-07**，**事实上已死**——
  JS 走 `iroh-ffi`（napi），别碰 `iroh-js` 仓。

### 5.10 商业模式（判断「n0 会不会跑路」）

FAQ 原文要点：
- 背后公司是 **number 0**，**部分 VC、部分创始人自投**（founders have invested their own money）。
- 「Number 0 is healthy and has investors we actually think are a value-add」。
- 收入来自 **Iroh Services**（托管 relay + DNS address lookup），从免费公共设施到生产专用云部署。
- **承诺开源**：「We rely on iroh remaining open source, and are committed to keeping it that way,
  **including server-side code for relays and DNS address lookup**」。

**「如果 n0 不跑 relay 了怎么办」** 的官方回答：「**You're not dependent on us.**」
relay 代码开源（`iroh-relay`），自建只需公网 IP + DNS 名，ACME 自动 TLS 内置。

> **锁定风险评估**：核心 `iroh` 是 `MIT OR Apache-2.0`（`iroh/Cargo.toml`），relay 服务端在同仓开源
> ——**技术上不存在硬锁定**。真实的锁定点是**默认配置的惰性**（§2.5：不改就默认用 n0 的 relay）
> 和**商业化排他的能力**（§4.3 的 Network Diagnostics）。
> 前者一行配置就能解，**建议一开始就解**。
> ⚠️ 但注意 §2.6：ESP32/FreeRTOS 那条线是**闭源商业**的，那部分**确实**锁定。


- **iroh wire 协议规范文档**：`llms.txt` 全索引无 spec 页；`iroh` 仓 `find -iname '*spec*'` 0 命中。
  roadmap 的 "ratify the iroh 1.0 wire protocol" **无可见产出**。
- **1.0 之后的官方路线图**：roadmap 页停在 2026-02-09，仍是 1.0 前内容。
- **2.0 的具体日期 / 内容**。
- **MSRV 政策**：README/CONTRIBUTING 无 MSRV 表述，「MSRV bump 算不算 breaking」无成文承诺。
- **LTS**：不存在。最接近的是付费 Extended Support。
- **公共 relay 是否仍接受 `iroh-relay-v1`**：`ProtocolVersion::ALL` 里 V1 仍可协商（客户端侧证据），
  但 n0 公共 relay 实际部署行为**无法从本地源码核实**。
- **「9 out of 10 连接直连」的测量方法/样本/来源**：FAQ 引用两次，均无出处。
- **ESP32 / FreeRTOS 实现**：开源仓 0 命中，属闭源商业产物。
- **`iroh-relay-v1` 的退役时间表**。
