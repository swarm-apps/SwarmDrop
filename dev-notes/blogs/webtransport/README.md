# WebTransport：给浏览器开的第二条门

> 2026-08-12。SwarmDrop 的浏览器端此前只有 WebRTC 一条路——一整套为 NAT 打洞而生的
> ICE/DTLS/SCTP，用来做「浏览器 → 局域网里的一台桌面」这种**明确知道对方在哪**的连接。
>
> 这个系列讲我们为什么加了第二条路、它凭什么快 4.5 倍，
> 以及为什么真正难的部分**不在传输层**。

## 一句话概括

**WebTransport 是浏览器上的 QUIC 客户端 API，而它对我们唯一重要的能力是
`serverCertificateHashes`——允许连一台没有域名、只有自签名证书的机器。**

代价是那张证书的有效期被规范限死在 **14 天**。于是一件本该是静态配置的事（服务器身份）
突然有了生命周期，而这个 crate 八成的复杂度都来自这一行。

## 篇目

| # | 篇 | 讲什么 |
|---|---|---|
| 00 | [WebTransport 到底是什么](00-what-is-webtransport.md) | **零基础入门。** 和 WebSocket / WebRTC 逐条对比，队头阻塞、datagram、`serverCertificateHashes`，以及它适合与不适合的场景 |
| 01 | [libp2p 的 WebTransport，和上游缺的那一半](01-libp2p-webtransport.md) | certhash 为什么进地址、为什么之后还必须跑 Noise、与 webrtc-direct 的**机制互斥**；上游只有浏览器一半，我们补的那一半怎么分层 |
| 02 | [重心不在传输层：证书轮换](02-certificate-rotation.md) | **全系列的核心。** 两张证书为什么必须重叠、一条地址为什么只活 28 天、退役哈希为什么必须持久化、时钟为什么从参数进来 |
| 03 | [数字与取舍](03-numbers-and-tradeoffs.md) | 回环 4.5 倍、真机 20/9 MB/s、三处「没查清」，以及一个即使有了这些数字也**不能下线**的旧入口 |

**建议读法**

- 完全不了解 WebTransport：**00** 是独立可读的入门，看完就能判断它适不适合你的场景
- 想知道 libp2p 怎么用它：**01**
- 只想学一个设计教训：直接读 **02**——它讲的是「引入时间维度之后，一个模块该怎么切」
- 只要数字：**03**

## 这个系列真正想讲的一件事

传输层的代码几乎是机械劳动——把 `wtransport` 的 async API 翻译成 libp2p 的 poll API。
真正有状态、有时钟、驱动外部可见行为的只有证书那一块。

把两个自研传输摊开对比，这一点特别清楚：

| 维度 | `webrtc-p2p` | `webtransport-p2p` |
|---|---|---|
| 模式数 | 2（打洞 + direct） | 1 |
| 建连协商 | SDP / ICE / DTLS 角色 / ufrag | 无 |
| socket 复用 | 自写 1100 行 `UdpMux` | 无 |
| 子流 | DataChannel + 自做 framing | QUIC 流本身就是流 |
| 后端抽象 | 必须 | 不需要 |
| **证书** | **一张，永不改变** | **两张，会过期，14 天轮换** |

**只有最后一行变复杂**，而它引入了一个前者完全不存在的维度：**时间**。

结论直接决定了模块划分：证书生命周期必须是一个真子系统，而不是 `transport.rs` 里的几个字段。
若把它塞进去，这个 crate 就写坏了——写坏的形态是每 14 天出现一次、持续十分钟、
错误信息只说「证书不受信」的故障。

## 一份诚实的免责声明

系列里有三处「没查清」，都在正文里标了出来，没有藏：

| 没查清的 | 在哪 |
|---|---|
| WebTransport 比裸 QUIC 还快 21%（理论上不该） | [03](03-numbers-and-tradeoffs.md) |
| 发送方向流水线化之后实际提升多少 | [03](03-numbers-and-tradeoffs.md) |
| 跨网、iOS/Safari、Firefox 全部未测 | [03](03-numbers-and-tradeoffs.md) |

以及一处**已经纠正过的错误归因**：「浏览器发送慢是因为多一份跨 JS↔wasm 拷贝」——
两个方向都跨两次边界，拷贝次数对称，不对称的是**重叠**。照着「减少拷贝」优化会走偏。

## 相关材料

- **代码**：`crates/webtransport-p2p/`（零 swarmdrop 依赖，将来 subtree split）；
  证书持久化端口的实现在 `crates/net/src/cert_store.rs`
- **设计与决策**：`openspec/changes/webtransport-native-transport/design.md`（12 条 decision）
- **实测记录**：[`research/2026-08-12-webtransport-field-test.md`](../../research/2026-08-12-webtransport-field-test.md)
- **知识库**：[`knowledge/net-kernel.md`](../../knowledge/net-kernel.md) 的 WebTransport 小节
- **姊妹系列**：
  - [`webrtc/`](../webrtc/) —— 另一条浏览器入口的完整复盘（零基础入门 + 六个上游补丁）。
    **两条入口是并存关系，不是替代**，理由见本系列 03 篇
  - [`browser-platform/`](../browser-platform/) —— 浏览器平台侧的约束（secure context、
    mixed content、能不能 listen）
  - [`transfer-throughput/`](../transfer-throughput/) —— 数据面本身的吞吐调优，
    05 篇正是本系列 03 篇里那个「20 vs 9」的修复
