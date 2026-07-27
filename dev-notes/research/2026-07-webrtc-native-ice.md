# 自研 WebRTC transport：让浏览器直连 NAT 后设备

> **状态：🟡 可行性已验证，未决策。**
> 技术地基成立（spike 实测），但缺最后一道跨 NAT 打洞验证；是否投入取决于 Web 端优先级。
>
> 起于 2026-07-27 的一次讨论 · spike 代码与完整实验数据见 `spike/webrtc-ice-browser/README.md`

## 缺口在哪

SwarmDrop 目前**拿不到「浏览器 ↔ NAT 后设备」的直连**，跨网时只能经 relay 中转。
这是 Web 端最大的能力缺口——而 Web 端是当前主战场。

缺口的成因是一条完整的链，每一环都堵死：

| 环节 | 现状 |
|---|---|
| libp2p 的 `webrtc-direct` | **ICE-lite**：只被动应答、不收集候选、SDP 确定性构造 ⇒ 不打洞 |
| 浏览器侧打洞能力 | 编译期就没有——`crates/net/src/behaviour/` 里 dcutr/autonat/mdns 全是 `#[cfg(not(wasm_browser))]` |
| 上游 `webrtc-websys` | `dial` 直接拒绝 listener role，那正是 DCUtR 打洞的另一半 |
| `/webrtc-signaling/0.0.1` | libp2p spec 2023 年就定稿，**rust-libp2p 零实现** |

换 js-libp2p 也不解决——signaling 要求**两端**都实现，桌面侧是 rust。详见
[`knowledge/libp2p-wasm.md`](../knowledge/libp2p-wasm.md) 的「被推翻的旧认知」。

## 上游指望不上

| PR / Issue | 内容 | 状态（2026-07-27） |
|---|---|---|
| [#5978](https://github.com/libp2p/rust-libp2p/pull/5978) | `webrtc-websys` 浏览器↔浏览器 | OPEN，+2938 行 / 27 文件，2025-04 开的，**卡 review 一年**，`mergeable_state: dirty` |
| [#4389](https://github.com/libp2p/rust-libp2p/issues/4389) | 对应需求 issue | OPEN |
| #5784 | 早期 `create webrtc-signaling` | CLOSED |

**#5978 解决不了我们的场景。** 拉过它的文件清单：改动全在
`transports/webrtc-websys/src/browser/`，`transports/webrtc/`（native 侧）**一行没动**。
它做的是浏览器↔浏览器，我们要的是浏览器↔NAT 后的桌面/手机——那需要 native 侧也实现
signaling + 真 ICE 协商，**无人在做**。

维护者 dariusc93 2026-06-01 留言「没忘记，有时间会 review」，之后无动静。需求真实，
但不是核心维护者的优先级。

## 方案：独立 crate，基于 webrtc-rs 0.20

不改 rust-libp2p 本体，做一个外部 transport crate（libp2p 的 transport 本来就都是独立
crate，我们已经在用 `with_other_transport` 接入 libp2p-webrtc）。

**为什么独立仓库**：绕开两个真正的瓶颈——等 review（#5978 卡了一年）、以及改动只能落在
libp2p 本体里（会加深已有的 fork 债务）。

**为什么 0.20**：0.20 相对 0.17（libp2p-webrtc 当前所用）是**大重构**——`PeerConnection`
从具体类型变 trait、事件从闭包回调改 `PeerConnectionEventHandler`、DataChannel 变
`poll()` 事件流、自带 runtime 抽象层。**适配层无论如何都得重写**，这反而拉平了
「另起炉灶」与「改 libp2p-webrtc」的相对成本。而 0.20 带来的是实打实的性能改进
（SCTP 批处理、UDP GSO/GRO、发送背压 API），对文件传输直接受益。

**两点必须想清楚**：

1. **两端不共享底层实现**。webrtc-rs 是纯 Rust 的 UDP/SCTP/DTLS 栈，**编不到 wasm**；
   浏览器侧只能用原生 `RTCPeerConnection`（经 web-sys）。共享的只有协议层（signaling、
   SDP 处理、multiaddr 格式）。工作量是两份——libp2p 官方也是 `webrtc` / `webrtc-websys`
   两个 crate 分开。
2. **transport 与 behaviour 必须配对**。signaling 要在 relay 连接上开 libp2p 流（behaviour
   的活），建出的连接要交给 Transport trait——libp2p 里这是两个分离的平面。#5978 的结构
   印证了这点（`browser/{behaviour,transport,handler}.rs` 靠内部通道串联），DCUtR 也是
   同样的拧巴法。对外 API 会是「注册 transport 时必须同时注册配套 behaviour」，设计时
   要处理好，否则极易接错。

**一条硬约束**：照官方 spec 实现 `/webrtc-signaling/0.0.1`，**不要自创协议**。收益是能和
js-libp2p 互通、#5978 若合并也能互通、社区接受度高。自创省不了多少事，却会让这个 crate
永远只能和自己说话。

## spike 验证结论（2026-07-27）

完整实验数据见 `spike/webrtc-ice-browser/README.md`。核心四条：

### ✅ webrtc-rs 0.20 是完整 ICE agent —— 地基成立

判据是它主动向 STUN 发绑定请求并产出 `typ srflx` candidate。ICE-lite 实现永远产不出这个。
**打洞所需的那一半能力在库里是现成的。**

额外确认：能解析 Chrome 的 mDNS host candidate（`xxx.local`）。解析不了的话局域网 host
路径会整条作废。

### ✅ 性能够

host 路径上行 **50.1 MiB/s**（8 MiB / 160 ms，全程爬升）。

> ⚠️ 测吞吐前必须先用 `getStats()` 确认选中路径。同一份代码走 srflx（NAT hairpin）只有
> **0.6 MiB/s** / RTT 9ms，走 host 是 50 MiB/s / RTT 1ms。差了两个数量级。

### ⚠️ 两个默认值不适合文件传输，且失败模式都不直观

```rust
PeerConnectionBuilder::new()
    // 默认 1 MiB。LAN 高带宽下几百毫秒撑爆 → 连接直接 disconnected → failed
    // （不是降速，是断；应用层几乎无法诊断）
    .with_sctp_receive_buffer_size(8 * 1024 * 1024)
    // 默认无界 → 快生产者可把发送缓冲撑爆（PR #817 要治的 OOM 面）
    // 但 limit < 4 MiB 会把管道饿着，吞吐腰斩
    .with_data_channel_send_buffer_limit(4 * 1024 * 1024)
```

接收窗口那条是最值得记的：调大之后不仅不断连，吞吐还从 35 提到 50 MiB/s。上游文档只说
「调**小**它可以省内存」（面向多连接 SFU），完全没提默认值在高带宽低 RTT 下会断连。
**这条值得反馈给上游。**

### ⚠️ 集成时要自己枚举网卡

`with_udp_addrs(["0.0.0.0:0"])` **不展开网卡**，webrtc-rs 会把字面量 `0.0.0.0` 原样写进
host candidate，对端无法使用 → host 路径作废、退到 srflx。这个坑在 direct 模式下不存在
（ICE-lite + 确定性 SDP 不需要枚举），**切完整 ICE 才暴露**。

## 未决：跨 NAT 打洞

spike 两端同机，srflx 走的是 NAT hairpin，**不构成打洞证据**。真结论需要两台不同网络的
机器（至少一台在 NAT 后），验证 srflx ↔ srflx 能否在跨 NAT 时建立。

**这是方案成立与否的最后一道硬判据**，需要额外的机器或部署环境。

## 判断

**技术可行，但不建议现在投入。**

- Web 端还在收敛基础功能（Phase 5 In Progress），直连是优化不是阻塞项
- 即使做成，ICE 打洞对对称 NAT 仍然无解，relay 兜底不能省
- 桌面↔桌面已有 DCUtR，受益面只有涉及浏览器的那条腿

**如果要做，按这个顺序**（每步都是硬判据，不过就停）：

1. ~~验 ICE 能力与背压~~ —— ✅ 已完成
2. **验跨 NAT 打洞** —— 见上节，需要两台机器
3. 再设计 signaling 的 transport + behaviour 配对，跑通我们的场景
4. 最后才考虑独立仓库与社区化

把 3、4 分开的理由：「做社区 crate」意味着 API 设计、文档、CI、发版、issue 响应，这些
开销在可行性验证完成前都是负担；而真实的设计约束要跑通了才知道，那时设计的 API 才靠谱。

## 更便宜的替代路径

如果目标只是**省 relay 带宽**而非「技术上做成直连」：

- 引导用户自建 relay 已是一等模式，webrtc-direct 让裸 IP VPS 就能服务浏览器（免域名）
- 浏览器 ↔ **公网可达**桌面的直连已经能用

真正无解的只有「浏览器 ↔ 对称 NAT 后设备」，而那一格即使做了 ICE 也未必打得通。

## 相关材料

- `spike/webrtc-ice-browser/` —— spike 代码、完整实验数据、复现脚本
- [`blogs/network/2026-07-cross-end-connectivity.md`](../blogs/network/2026-07-cross-end-connectivity.md)
  —— 当前三端连接拓扑（这个方案要改变的正是其中「方向不对称」那一节）
- [`knowledge/libp2p-wasm.md`](../knowledge/libp2p-wasm.md) —— 浏览器可达性的四道门
- [`knowledge/net-kernel.md`](../knowledge/net-kernel.md) —— libp2p fork 的退出条件
