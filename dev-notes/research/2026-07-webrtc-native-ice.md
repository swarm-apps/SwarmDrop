# 自研 WebRTC transport：让浏览器直连 NAT 后设备

> **状态：🟢 已决策采纳（2026-07-27）。**
> 技术地基已由 spike 实测确认。决策依据不是投入产出比——**是能力建设与通用组件价值**，
> 详见文末「决策与理由」。这决定了它的成功标准与排期方式都不同于普通需求。
>
> spike 代码与完整实验数据见 `spike/webrtc-native-ice` 分支的 `spike/webrtc-ice-browser/README.md`

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

## 覆盖范围：全矩阵，前提是实现对称

信令协议是**对称**的，与两端各是什么类型无关——A 经 relay 发 offer、B 回 answer、双方
交换 candidate、打洞、relay 退场。spec（`specs/webrtc/webrtc.md:15`）把这点写死了：

> Note that _A_ and/or _B_ may as well be **non-browser nodes behind NATs** and/or firewalls.

所以同一套实现覆盖全部组合：

| 组合 | 今天 | 有了这个 crate |
|---|---|---|
| **web ↔ web** | 全程 relay 中转 | ✅ 直连 |
| **web ↔ native（NAT 后）** | 全程 relay 中转 | ✅ 直连 |
| web ↔ native（公网 / 局域网） | 已直连（webrtc-direct） | 不变 |
| native ↔ native | 已直连（DCUtR） | 不变，多一条备选路径 |

前两行是纯增量。**注意 web ↔ web 也在内**——它今天同样是全程中转
（见 [`blogs/network/2026-07-cross-end-connectivity.md`](../blogs/network/2026-07-cross-end-connectivity.md)）。

### 对称性是硬约束，不是可选优化

**两侧实现都必须能 offer 也能 answer。** 不能把浏览器侧写成「只发起」、native 侧写成
「只接受」——那样矩阵第二行就塌了。

这正是 [#5978](https://github.com/libp2p/rust-libp2p/pull/5978) 的局限：它只实现了
`webrtc-websys`（浏览器侧），native 侧一行没动，所以只能拿到第一行。**我们要多做的那
一半，恰恰是让矩阵完整的那一半。**

⚠️ **spike 只验证了 native 作为 answerer**（浏览器 offer → Rust answer）。native 作为
offerer 尚未验证——`webrtc-rs` 有 `create_offer`，预期没问题，但这是对称性的必要验收项，
别漏。

### relay 省不掉，但角色降级

信令必须经一条**已建立的连接**传输，而两端都在 NAT 后时那只能是 relay circuit。所以
relay 依然必需。变的是它承担什么：

```
今天：  relay 转发全部文件数据        →  几百 MB
之后：  relay 只转发 SDP + candidate  →  几 KB，打洞成功后信令流即关闭
```

**这才是本方案真正的收益形态**——不是「不再需要 relay」，而是「relay 从数据管道降级为
握手信道」。自建 relay 的成本模型会完全不同。

### 对 API 设计的影响

既然要覆盖全矩阵，crate 就不能设计成「浏览器专用」或「native 专用」，而是**同一个协议
实现的两个 target 特化**：signaling 状态机、SDP 处理、multiaddr 格式全部共享，只有底层
PeerConnection 不同（`webrtc-rs` vs 浏览器 `RTCPeerConnection`）。

这也是「通用」的第二层含义——不只是别的项目能用，而是**在同一个项目里覆盖所有端的组合**。

## 为什么不实现 webrtc-direct 模式（以及它的代价）

js-libp2p 的 [`transport-webrtc`](https://github.com/libp2p/js-libp2p/tree/main/packages/transport-webrtc)
一个包导出两种模式（`webRTC()` 打洞 / `webRTCDirect()`）。**我们只做打洞那一种。**

原因是 js 把两者放一起是为了复用底层，而 **rust 侧早已把共享层抽了出来**：
`libp2p-webrtc-utils` 发布在 crates.io（0.5.0），含 `fingerprint.rs` / `sdp.rs` /
`stream.rs`。关键是它**泛型且不依赖 webrtc-rs**：

```rust
pub struct Stream<T> where T: AsyncRead + AsyncWrite   // 与具体 WebRTC 实现无关
```

所以我们用 webrtc-rs 0.20 照样能复用它，**不会因此把 0.17 拖进来**。加上 `multiaddr`
已有 `Protocol::WebRTC` 与 `Protocol::WebRTCDirect` 两个协议段，分派天然不冲突：

| 模式 | 谁实现 | 状态 |
|---|---|---|
| webrtc-direct（native server） | 官方 `libp2p-webrtc` | 已有，本项目在用 |
| webrtc-direct（browser dialer） | 官方 `libp2p-webrtc-websys` | 已有，本项目在用 |
| **打洞（两端）** | **本 crate** | 缺口所在 |

### 代价：native 侧会有两套 WebRTC 栈

官方 `libp2p-webrtc` 钉死 `webrtc = "0.17"`，我们要 `0.20`。两个不兼容版本**同时进
依赖树**，native 侧编译两份完整的 ICE/DTLS/SCTP/SRTP，编译时间与二进制体积翻倍。

> **看到依赖树里两个 webrtc 版本不是配置错误，是已知取舍。** 留档于此以免后人误删。

wasm 侧没有这个问题——浏览器用原生 `RTCPeerConnection`，压根没有 webrtc-rs。这是一处
native/wasm 的不对称。

### 这个代价反过来是「将来补 direct」的唯一实际理由

若本 crate 也实现 direct，就能**完全替代**官方 `libp2p-webrtc`，只留一套 0.20。
理由不是功能缺失（官方实现可用），而是消除双份依赖——顺带让这个 crate 成为官方的完整
替代品，通用性再上一档。

**但排在打洞之后。** 现在的约束是：架构上不假设「只有打洞」——复用 `webrtc-utils`、
模块划分给 direct 留位置，将来补上时不需要重构。

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

## 待验：跨 NAT 打洞

spike 两端同机，srflx 走的是 NAT hairpin，**不构成打洞证据**。需要两台不同网络的机器
（至少一台在 NAT 后），验证 srflx ↔ srflx 能否在跨 NAT 时建立。

ICE 打洞本身是成熟技术（视频会议全靠它），业界成功率约 70–90%，对称 NAT 除外。所以
这一步**大概率会过**——它现在是**验收项**而非决策判据（决策依据已变，见文末）。

## 决策与理由（2026-07-27）

**决定：做。** 但驱动它的**不是投入产出比**——按纯工程账算，下面那个 UPnP 方案能以
两个数量级更低的成本吃掉大部分收益。真正的理由是另外两条：

1. **能力建设** —— ICE / STUN / SDP / SCTP 这套东西是 P2P 的核心，自己实现一遍拿到的
   理解，是读文档换不来的。本项目的网络内核已经自研（`crates/net`），这是同一条路的延伸。
2. **通用组件价值** —— rust-libp2p 至今没有「浏览器 ↔ NAT 后原生端」的直连实现
   （见上文上游现状）。这个缺口是整个 rust 生态共有的，不是 SwarmDrop 独有的问题。

**这两条理由改变了成功标准**，必须写下来，否则后续容易用错的尺子衡量：

| | 若按「省 relay 带宽」 | 实际按「能力建设 + 通用组件」 |
|---|---|---|
| 成功标准 | 覆盖 X% 的跨网场景 | 符合 spec、能与 js-libp2p 互通、API 清晰 |
| 排期压力 | 越快上线越好 | 按理解深度推进，不赶 |
| 质量要求 | 能用即可 | 文档 / 测试 / API 设计是一等公民 |
| 失败判定 | 覆盖率不够就是失败 | 只有「做不出可互通的实现」才算失败 |

**一条硬约束因此升级为前提**：必须照官方 spec 实现 `/webrtc-signaling/0.0.1`。
通用性的全部前提就是互通——自创协议会让这个 crate 只能和自己说话，两条理由同时落空。

### 执行顺序

1. ~~验 ICE 能力与背压~~ —— ✅ 已完成（见上文 spike 结论）
2. ~~signaling 的 transport + behaviour 配对~~ —— ✅ 已完成，见 `crates/webrtc-p2p`
   （分支 `feat/webrtc-p2p-transport`）
3. ~~native 后端~~ —— ✅ 已完成。两个真实 webrtc-rs 后端在本机跑通**信令 + 数据面**：
   init 通道 → offer/answer → trickle ICE → DTLS → Connected → 开子流 → 双向传数据、
   字节一致。`native 作为 offerer` 这项验收随之达成（spike 当时只验了 answerer 方向）
4. **wasm 后端** —— 浏览器 `RTCPeerConnection`；对称矩阵的另一半
5. **跨 NAT 打洞验收** —— 需要两台不同网络的机器。ICE 打洞本身成熟，此步是
   「确认实现正确」而非「决定要不要做」
6. 独立仓库 + 社区化

### 已落地实现的形状（截至 2026-07-27）

    protocol/   线上格式，零 libp2p-swarm 依赖
    backend/    WebRTC 栈抽象；native 已实现，wasm 待补；mock 供状态机测试
    swarm/      session（纯逻辑状态机）/ handler（poll 适配）/ behaviour / transport

依赖方向单向 `swarm → backend → protocol`。状态机与协议层都是纯逻辑，可脱离真实
WebRTC 与真实 `Stream` 测试——这是把「最容易出错的部分」隔离出来的刻意安排。

3、4 仍应分开：API 设计、文档、CI、发版、issue 响应这些开销，在跑通之前都是负担；
而真实的设计约束要跑通了才知道，那时设计的 API 才靠谱。

## 被评估但未选择的替代路径：UPnP 端口映射

**结论：成本低两个数量级，但解决的不是同一个问题。** 保留论证，将来若要「先让用户
用上直连」可随时启用——它与本方案不互斥。

`libp2p-upnp` 已在依赖树里，且**天然支持 webrtc-direct 地址**（读码确认，未实测）：

- `protocols/upnp/src/behaviour.rs:654-664` —— 私网地址判定只看
  `/ip4/<私网>/{tcp,udp}/<port>` 前两段，不关心后缀是 `quic-v1` 还是 `webrtc-direct`
- `behaviour.rs:116` —— `external_addr()` 用 `multiaddr.replace(0, ...)` **只替换 IP 段**，
  其余原样保留

于是映射成功后公告的是：

```
/ip4/192.168.50.105/udp/54321/webrtc-direct/certhash/uEiXxx
        ↓
/ip4/<公网IP>/udp/54321/webrtc-direct/certhash/uEiXxx      ← certhash 完整保留
```

即**桌面端自动获得浏览器可直拨的公网地址，零自研代码**，且对移动端同样生效。

| | UPnP | 自研 ICE |
|---|---|---|
| 代码量 | 一个 behaviour + 开关 | 4000+ 行两端 + 长期维护 |
| 上线 | 几天 | 数月 |
| 失效于 | CGNAT、UPnP 被关、企业网 | 对称 NAT、部分企业防火墙 |

**两者失效场景高度重叠**——CGNAT 通常就是对称 NAT，ICE 在那儿也打不通。所以自研 ICE
相对 UPnP 的增量只剩「NAT 类型友好但 UPnP 不可用」这一格。

未选择它作为主线，是因为它满足不了上面那两条决策理由（学不到 ICE，也不填生态缺口）。
但如果哪天需要**快速**给用户交付跨网直连，这是最短路径。

三个已知风险（启用前需实测）：UPnP 有安全争议、不少路由器默认关闭；映射靠租约续期，
路由器重启后 certhash 地址会短暂失效；CGNAT 下完全无效。

## 相关材料

- `spike/webrtc-ice-browser/` —— spike 代码、完整实验数据、复现脚本
- [`blogs/network/2026-07-cross-end-connectivity.md`](../blogs/network/2026-07-cross-end-connectivity.md)
  —— 当前三端连接拓扑（这个方案要改变的正是其中「方向不对称」那一节）
- [`knowledge/libp2p-wasm.md`](../knowledge/libp2p-wasm.md) —— 浏览器可达性的四道门
- [`knowledge/net-kernel.md`](../knowledge/net-kernel.md) —— libp2p fork 的退出条件
