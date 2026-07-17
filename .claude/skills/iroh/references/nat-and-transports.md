# NAT 穿透与替代传输

**iroh 1.0.2 · 调研 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`**

> API 用法 → `/iroh` skill。这里讲**连通性的真实能力边界、以及 Tor/Nym/BLE 到底是什么状态**。

## 一句话结论

iroh 的 NAT 穿透比 libp2p 强且免维护，但**心智模型完全不同**：**没有 STUN、没有 DCUtR 握手**。连接建立后由 QUIC multipath 在同一条 live connection 上把 path 升级为直连 —— **穿透失败只是「降级留在 relay」，不是连不上**。

替代传输方面：**Tor/Nym 一律不要用于文件传输**（Nym README 自陈 ~15-20 KiB/s、"Not suitable for high-throughput file sync"）；**BLE 是彻底的空气**（TRANSPORTS.md 里 repo 列是空的，零实现）。局域网诉求的正确答案是 **iroh-mdns-address-lookup**，不是 BLE。

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

## n0 官方 relay：4 个，无 mesh

`iroh/iroh/src/defaults.rs` 的 `pub mod prod` 定义四个 hostname：

| 代码里的地域标签 | hostname |
|---|---|
| NA east | `use1-1.relay.n0.iroh.link.` |
| NA west | `usw1-1.relay.n0.iroh.link.` |
| EU | `euc1-1.relay.n0.iroh.link.` |
| **Asia-Pacific** | `aps1-1.relay.n0.iroh.link.` |

`default_relay_map()`（:36-42）把四者 from_iter 组成 RelayMap。staging（:100）只含 NA-east + EU。

> ⚠️ **「无一在中国大陆」是推断，不是代码事实**。defaults.rs 只能证明 hostname 与代码里的地域标签。aps1-1 的实际落地机房需要实测解析+定位才能坐实。实务上几乎必然为真（ICP 备案等），但应标注为推断。

**自建入口**：`iroh/iroh/src/endpoint.rs:1922` `pub enum RelayMode { Disabled, Default, Staging, Custom(RelayMap) }`（Custom 变体在 **:1933**）。

⚠️ `default_relay_mode()` 会被 `IROH_FORCE_STAGING_RELAYS` 环境变量改成 Staging（`endpoint.rs:1978-1985`）—— 测试时别误设。

**自建 relay 支持访问控制与限速**：`iroh-relay/src/main.rs:160-175` `enum AccessConfig`（:194 提及 `access.shared_token`），`iroh-relay/src/server.rs:507` `pub bytes_per_second: NonZeroU32` —— **这是 libp2p circuit relay 需要自己造的东西**。详见 `relay-infra.md`。

## portmapper（UPnP/PCP/NAT-PMP）

- **默认开启**：`iroh/iroh/Cargo.toml:148` `default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]`；`iroh/iroh/src/portmapper.rs:36-38` `impl Default` 返回 `PortmapperConfig::Enabled {}`
- `portmapper.rs:20-32` `pub enum PortmapperConfig { Enabled {}, Disabled }`，Disabled 变体文档明写：*"Skips the UPnP/PCP/NAT-PMP gateway probing. Use this to avoid the SSDP multicast discovery that can raise firewall dialogs (**notably on macOS**), at the cost of potentially worse direct connectivity behind some NATs."*

**价值与边界**：

✅ portmapper 能把**家用路由器**造成的 Hard/Easy NAT 变成可直连（NAT 矩阵里 Easiest 即 *"Typical of consumer routers with UPnP"*）→ 对提升**国内家宽直连率**有实质价值，**不建议关**。

⚠️ **但它救不了 CGNAT**：UPnP/PCP/NAT-PMP 是向**本地 CPE 路由器**申请端口映射；中国移动网络的 Hard 属性来自**运营商侧的 CGNAT**，它在家用路由器**之上**，**不响应 UPnP**。`nat.rs:43-50` 的 Hard 注释把 corporate firewalls 与 CGN 并列，但两者对 portmapper 的响应能力完全不同。

**含义**：portmapper 的收益范围限定在家宽直连率，**不能顺延到移动网络**。

**macOS 首次启动的防火墙弹窗需在 onboarding 里预先解释**，否则用户会误以为是恶意行为。

## custom transport API

- **成熟度**：**experimental / 不受 semver 保护**
- **依据**：
  - `iroh/iroh/Cargo.toml:162` `unstable-custom-transports = []`（空 feature，纯 gate）
  - `iroh/iroh/src/endpoint.rs:807-811`（`add_custom_transport` 的文档）与 :833-836（`path_selector` 的文档）均含：*"This API is unstable and gated behind the `unstable-custom-transport` feature. **It is not covered by semantic versioning guarantees and may change in any release without a major version bump.**"*
  - 模块级重复告警 `endpoint.rs:29-38`
  - ⚠️ 文档里写的 feature 名 `unstable-custom-transport`（**单数**）与实际 feature 名 `unstable-custom-transports`（**复数**）不符 —— 属文档笔误
  - **但已有可跑通的官方 example**（`Cargo.toml:208-210` `[[example]] name = "custom-transport"` + `required-features = ["test-utils", "unstable-custom-transports"]`）且被 Tor/Nym 两个真实 crate 消费 —— **非纯纸面设计**
- **入口**：`iroh/iroh/src/socket/transports/custom.rs`（**105 行，三个 trait 全在这**）→ `iroh/iroh/examples/custom-transport.rs`（205 行完整示例）

### 心智：这是数据报级接口，不是 libp2p 的 Transport trait

`custom.rs:58-64`：

```rust
fn poll_recv(
    &mut self, cx: &mut Context,
    bufs: &mut [io::IoSliceMut<'_>],
    metas: &mut [noq_udp::RecvMeta],
    recv_infos: &mut [RecvInfo],
) -> Poll<io::Result<usize>>      // 批量收包，:46 注释 "The maximum length of the slices is [noq_udp::BATCH_SIZE]"

fn poll_send(&self, cx, dst: &CustomAddr, src: Option<&CustomAddr>, transmit: &Transmit<'_>)
    -> Poll<io::Result<()>>       // :98-104
```

`custom.rs:27-28` 明确类比：*"Analogously to [std::net::UdpSocket::bind], this is where the actual underlying hardware resource is created."*

**心智转换**：
- libp2p 的 Transport 要你实现「**拨号出一条可靠 stream**」
- iroh 只要你实现「**像 UdpSocket 一样收发不可靠包**」—— 可靠性/拥塞/加密全由上层 QUIC 负责

**实现门槛其实更低**，但**批量收包语义必须正确处理**。

> ⚠️ **GSO 是可选的，不是必须实现项**：`custom.rs:71-73` 给了默认实现 `fn max_transmit_segments(&self) -> NonZeroUsize { NonZeroUsize::MIN }`，文档注明 *"The default is 1 (no batching). Custom transports that support batching can override this to allow more efficient transmission."*

三层 trait：`CustomTransport`(工厂) → `CustomEndpoint`(本地地址+收包) → `CustomSender`(发包)。

地址模型（`iroh/iroh-base/src/endpoint_addr.rs:186-192`）：

```rust
pub struct CustomAddr { id: u64, data: CustomAddrBytes }
// :180 注释: "Format: 8-byte little-endian u64 transport id, followed by raw address data bytes"
pub enum TransportAddr { Relay(RelayUrl), Ip(SocketAddr), Custom(CustomAddr) }   // :54-61
```

### ⚠️ 陷阱：默认 PathSelector 把 custom transport 归为 Primary

`iroh/iroh/src/socket/biased_rtt_path_selector.rs:86-88` 默认实现文档：

> *"The biases are configured per [`AddrKind`]. Defaults: IPv4 and IPv6 are primary (IPv6 has a 3ms RTT advantage), **Relay is backup, custom transports are primary with no advantage**."*

:26-28 层级语义：*"Primary paths are used preferentially. Backup paths are only used when no primary path is available. This is independent of the QUIC `PathStatus`; today the only transport classified as backup is the relay transport."*

:269 测试注释坐实：*"Primary tier beats backup tier even when the backup has a much lower RTT."*

**即：接了 Tor/Nym 而不换 PathSelector = 强制所有流量走 Tor/Nym，即便 RTT 差几个数量级。**

官方 example 因此专门写了自定义 selector：`examples/custom-transport.rs:40` `struct PreferTestTransport`，:42 `impl PathSelector for PreferTestTransport`，:78 `.path_selector(Arc::new(PreferTestTransport))`。

选路细节（:19-23）：`IPV6_RTT_ADVANTAGE = 3ms`、`RTT_SWITCHING_MIN = 5ms`（防抖动）。

**若真接了 custom transport（如 BLE），务必同时实现 PathSelector 把它降级为 Backup 或加 RTT 惩罚**，否则会出现「明明有 Wi-Fi 直连却走蓝牙」的灾难。

## 替代传输的真实状态

### iroh-tor-transport

- **成熟度**：**experimental**
- **依据**：
  - crates.io sparse index 显示**唯一版本 0.1.0**，pubtime 2026-06-15
  - README 首屏免责：*"**Experimental:** both iroh custom transports and this crate are experimental and may change."*
  - GitHub API：stars=16, open_issues=3, pushed_at=2026-06-15, archived=false
  - `iroh/TRANSPORTS.md:9` 官方状态列标注 `experimental`
  - **提交历史**：2026-01-22 ~ 2026-02-05 有连续的真实开发（Add license files / Add tags / Rename everything User... to Custom... / Update bytes 等）；此后**只在 iroh 发版时跳动**（2026-03-17 适配 0.97、2026-06-15 适配 1.0）；单一作者 rklaehn
- **是什么**：把 iroh 的 QUIC 包裹进 Tor hidden service stream。仅凭对方 EndpointId 即可推导出对应 .onion 地址并连接（无需交换地址），自带一个 address lookup 做 EndpointId→onion 映射。transport id 0x544F52（"TOR" 的 ASCII，`src/lib.rs:110` —— 注意它是**私有** `const` 而非 `pub const`，外部无法引用；「已注册」指的是 TRANSPORTS.md:9 表里有条目）

**为什么不能用于移动端**（这是**架构级排除**，不只是性能取舍）：

- README:25：*"The transport connects to Tor's control port, creates an ephemeral hidden service, and handles packet framing and stream reuse"*
- Cargo.toml 依赖 `torut = "0.2"`（:12，Tor 控制协议客户端库）+ `tokio-socks = "0.5"`（:26，走 Tor SOCKS 代理），**全无 arti**（内嵌 Tor 实现）依赖 → 二者均指向「**连接外部 daemon**」
- （README:17 那句 *"Most tests require a running Tor daemon with the control port enabled: `tor --ControlPort 9051 --CookieAuthentication 0`"* 说的是**跑测试**的前提，别拿它当运行时证据）

**iOS/Android 根本无法跑 Tor daemon** → 跨端能力不对等的传输层无法作为产品特性。且 Tor 在中国需自带网桥，可达性比自建 relay 更差，**不是「翻墙」方案**。

### iroh-nym-transport

- **成熟度**：**experimental**
- **依据**：crates.io 唯一版本 0.1.0（2026-06-15）；README 首屏同样的 Experimental 免责；stars=12, open_issues=0；提交历史与 Tor 版同构（2026-01-22~02-05 真实开发，含 2026-02-02 "nym transport" 初版与 2026-03-09 "Use latest iroh main"；此后仅随 iroh 发版 bump）；单一作者 rklaehn
  - ⚠️ **未在 `iroh/TRANSPORTS.md` 注册**（自占 id 0x4E594D，`src/lib.rs:22-23` `/// 0x4E594D = "NYM" in ASCII.` `pub const NYM_TRANSPORT_ID: u64 = 0x4E594D;`）
  - ⚠️ **维护滞后**：`n0-error` 仍 pin 在 `^0.1` 而 Tor 版已升到 `^1.0`
- **是什么**：把 iroh 的 QUIC 包裹进 Nym mixnet（3 跳混淆 + 人为延迟）以抗流量分析

**性能数字（README 自陈）**：

| | Direct | Nym |
|---|---|---|
| Latency | ~50-200ms typical | **~1-3 seconds RTT** |
| Throughput | 10+ Mbps | **~15-20 KiB/s (testnet)** |

"Why so slow?" 第 3 点：*"Rate limiting: The Nym testnet limits packet rate to ~50 packets/second per client."*

**README 的 "Not suitable for" 列表逐条**：
- **"Bulk data transfer"** ← 第一条
- "Real-time applications (voice, video, gaming)"
- **"High-throughput file sync"** ← 第三条
- "Anything requiring sub-second latency"

**按 20 KiB/s 计，传 1GB 需约 15 小时。** 与文件传输定位**不可调和**，连「可选隐私模式」都不值得做。

README 自陈适用面：低带宽控制信道、聊天、信令、抗审查的 peer discovery。

### BLE / 蓝牙 —— 不存在

**全生态仅 2 处字符串提及。**

`iroh/TRANSPORTS.md:10` 原文：

```
| 0x424C45 | BLE | Bluetooth MAC address (6 bytes) | | reserved |
```

—— **repo 列为空**（对比同表 Tor 行有 repo 链接且状态为 experimental）。

另 1 处是 `iroh/iroh-base/src/endpoint_addr.rs:392` 的测试注释 `// Small id, small data (e.g., Bluetooth MAC)`。

GitHub API 查询 `n0-computer/iroh-ble` 与 `n0-computer/iroh-bluetooth` 均返回 **404 Not Found**。

**要做只能自己基于 `unstable-custom-transports` 从零实现**，包含 BLE GATT 分片/MTU 协商/双端原生权限 —— **工作量以人月计**。

**而且不该做**：BLE 实际吞吐是百 KB/s 量级，对文件传输毫无意义；它在 AirDrop 类产品里的真实角色是**发现 + 唤醒**（然后切 Wi-Fi 传数据），而 iroh 生态里「发现」这一层已有 **iroh-mdns-address-lookup** 覆盖。

### ⚠️ TRANSPORTS.md 本身已过时且不可全信

- `TRANSPORTS.md:9` 的 Tor repo 链接 `https://github.com/n0-computer/iroh-tor` 经 GitHub API 查询返回 **301 Moved Permanently** —— 实际仓库已更名为 `n0-computer/iroh-tor-transport`
- Nym 自占 id 0x4E594D 但 **TRANSPORTS.md 全表 4 行数据行无 Nym 条目**
- 表本身仅 10 行，:3 写明维护方式为 *"If you want to publish a globally available custom transport, choose an id and do a PR against this repo."* —— **纯人工 PR，无强制**

**提示**：iroh 的「替代传输生态」整体处于早期状态 —— **连注册表都是手工维护且已 stale，说明这条线目前无人认真运营，不宜作为技术选型的依赖项。**

## 局域网的正确答案：iroh-mdns-address-lookup

**iroh 核心 crate 不含 mDNS** —— 在 iroh 仓执行 `grep -rni "mdns|swarm-discovery|local_swarm" --include="*.toml" iroh/` 返回**零结果**。能力位于独立仓。

- **成熟度**：**beta**（详见 `address-lookup.md`）
- **依据**：version 0.4.0；HEAD 2026-07-10；依赖 iroh 1.0.0。**降级理由两条**：① 0.4.0 **pre-1.0**，无 semver 承诺（「已过 0.1 摸索期」不构成 production 论据）；② **核心功能全压在 alpha 依赖上** —— `swarm-discovery = "0.6"`，而 `swarm-discovery/Cargo.toml:3` 是 `0.6.0-alpha.2`，第三方作者（rkuhn，`repository = "https://github.com/rkuhn/swarm-discovery"`），最后提交 2026-04-15
- **入口**：`iroh-address-lookups/iroh-mdns-address-lookup/README.md`（含完整可跑示例）

README 开篇：*"This crate uses an mDNS-like swarm discovery service to find address information about endpoints on your local network — no relay or outside internet needed."*

**⚠️ 注册那一步不能漏**（README 原文顺序）：

```rust
let endpoint = Endpoint::bind(presets::Minimal).await.unwrap();
let mdns = MdnsAddressLookup::builder().build(endpoint.id()).unwrap();
endpoint.address_lookup().unwrap().add(mdns.clone());   // ← 没有这行，mdns 根本没挂到 endpoint 上
let mut events = mdns.subscribe().await;
```

**这是从 libp2p 迁移最容易漏的一项**（libp2p 里 mDNS 是内置 behaviour）。

**这也是「BLE 对局域网有意义吗」的正面回答：不需要 BLE，mDNS 已覆盖发现层，且 BLE 在 iroh 生态根本无实现。**

## presets::N0 到底含什么（别记混）

`iroh/iroh/src/endpoint/presets.rs:81-87` 的 N0 preset **自述只包含三样**：
1. the DNS Address Lookup service
2. the default relay servers provided by Number 0
3. CryptoProvider（ring / aws-lc-rs）

**QAD 与 portmapper 都不属于 preset**：
- portmapper 来自 `Cargo.toml:148` 的 default feature + `PortmapperConfig::default() == Enabled{}`（`portmapper.rs:36-38`）
- QAD 是 net_report 针对 relay 跑的探测

净效果（用 `presets::N0` 就能同时拿到这三样）是对的，但**要分清 preset 与 socket/feature 默认值两层**。

> ⚠️ **N0 preset 还带 DNS Address Lookup** —— 这恰恰是要决策的一项（是否依赖 n0 的 pkarr/DNS 基础设施）。详见 `relay-infra.md` 的「presets::N0 会静默拖入三项 n0 基础设施」。

## relay 中转的上限

- `iroh-relay/src/protos/relay.rs:23` `pub const MAX_PACKET_SIZE: usize = 64 * 1024;`
- :25-29 `pub(crate) const MAX_FRAME_SIZE: usize = 1024 * 1024;`（*"This is also the minimum burst size that a rate-limiter has to accept."*）
- 服务端限速：`server.rs:507` `pub bytes_per_second: NonZeroU32`；CLI 侧 `main.rs:731-737` 校验 `if rx.bytes_per_second.is_none() && rx.max_burst_bytes.is_some() { bail_any!("bytes_per_seconds must be specified to enable the rate-limiter") }`

详见 `relay-infra.md`。

## iroh 本体的成熟度

- **production**
- **依据**：
  - `iroh/iroh/Cargo.toml:3` version = 1.0.2
  - git log -1 = `chore(ci): make sure android cleans up after itself (#4421)`（2026-07-16，调研前 1 天），**PR 编号已到 #4421，活跃度极高**
  - 含 Linux userns 网络仿真测试套件 `iroh/iroh/tests/patchbay/nat.rs` 覆盖 16 种 NAT 组合矩阵
  - 仓库含 iroh-relay / iroh-dns-server / docker 等**完整自建基础设施代码**
  - ⚠️ `iroh/iroh/Cargo.toml:170-193` 的 `[package.metadata.cargo_check_external_types] allowed_external_types` 是一条**防止非 1.0 外部类型泄漏进公开 API 的 lint 白名单**（名单里明确分组注释 workspace crates / crates owned by us that will move to 1.0 / 1.0 crates we deem fine / non-1.0 crates we decided to accept）—— 它是 semver 纪律的**佐证**，但不是 semver 保护机制本身。**semver 保护来自它是 1.0.x 这一事实**
