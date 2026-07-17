# Transports：Tor / Nym / BLE / 自定义传输

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Transports](https://docs.iroh.computer/transports/) 分区。

> **一句话**：**Tor/Nym 一律不要用于文件传输**（Nym README 自陈 ~15-20 KiB/s、"Not suitable for
> high-throughput file sync"）；**BLE 是彻底的空气**（`TRANSPORTS.md` 里 repo 列是空的，零实现）。
> 局域网诉求的正确答案是 **iroh-mdns-address-lookup**（→ [02-connecting.md](02-connecting.md)），不是 BLE。
>
> ⚠️ **官方 `TRANSPORTS.md` 注册表本身已过时且不可全信** —— 判据与举证见
> [index-ecosystem-map.md](index-ecosystem-map.md) 的导航陷阱一节。

---

# 1. custom transport API —— 三者共同的扩展点

官方 Transports 只列 Tor/Nym/BLE，没有「写自己的 transport」页（对比 Protocols 有 Write your own Protocol）。
但这正是 Tor/Nym/BLE 三者底下的同一个扩展点。

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

---

# 2. 替代传输的真实状态

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

> 三者的成熟度判定与证据链 → [index-ecosystem-map.md](index-ecosystem-map.md)。
