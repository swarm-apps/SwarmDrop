# Protocols：连上之后传什么

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Protocols](https://docs.iroh.computer/protocols/) 分区。**内容较多，拆成四个子文件：**

| 文件 | 内容 | 官方对应 |
|---|---|---|
| **[03a-using-quic.md](03a-using-quic.md)** | 四个流原语与开销模型、finish/reset/stop 三种通知、六种交互模式、背压与 100 并发上限、内存上界、流优先级、Datagram（官方明说多数情况别用）、关闭连接、0-RTT / 0.5-RTT | Using QUIC |
| **[03b-writing-a-protocol.md](03b-writing-a-protocol.md)** | 可编译模板、ALPN 命名与版本策略、Router / ProtocolHandler / DynProtocolHandler、AcceptError、流的粒度决策、framing（没有 Codec）、handler panic 连坐、关闭编排、上线 checklist | Write your own Protocol |
| **[03c-blobs.md](03c-blobs.md)** | bao-tree（outboard 原理 / 0.39% 开销 / pre-post-order 取舍）、iroh-blobs（store 后端 / range-set 差集续传 / FsStore 布局 / tag+GC 保活 / pull vs push）、sendme 与 dumbpipe 逐行对照 | Blobs |
| **[03d-docs-rpc-automerge.md](03d-docs-rpc-automerge.md)** | iroh-docs（数据模型 / 四条代价）、irpc（16MiB 硬上限 / 四条 Non-goals / 真实位置）、quic-rpc 为什么 abandoned、automerge 只有示例 | Documents / RPC / Automerge |

> ⚠️ **官方 Protocols 分区还有一个 [Streaming](https://docs.iroh.computer/protocols/streaming) 页，本 skill 未覆盖。**
> 它讲的是 **iroh-live**（Media over QUIC）、**iroh-roq**（RTP）、**callme**（Opus 音频 demo）——
> 这三个仓**不在 iroh-study 的 24 个仓里，本次未审计**，因此这里不做成熟度判定也不给结论。
> 要做实时音视频请直接查官方页。**本文其余四个子文件与官方分区是一一对应的，只有 Streaming 是空缺。**

---

## 这一分区的三条总纲

**1. iroh 不替你做协议设计。** 官方 using-quic 页开宗明义：

> Think of iroh as giving you **reliable, secure tunnels between peers**.
> While iroh handles the hard parts of networking, **you still need to design how your application exchanges data once connected**.

没有 Codec，没有 request/response 抽象，没有自动的消息关联。你拿到的是 QUIC 流。

**2. 流很便宜，这是最大的心智解放。** 写进 rustdoc 的合同：「may be opened and closed without incurring
any extra cost」。唯一的开销来源是**流控**，不是流本身。**从 libp2p 迁过来时，原先的限流器/流池是可以直接删掉的净负债。**

**3. bulk data plane 手写 `ProtocolHandler`，control / progress 才上 irpc。** 这是 n0 自己的分界线，
iroh-blobs 自己也是这个模式。**迁移路径上没有「必须先学 irpc」这一步** —— iroh 核心全部 7 个
Cargo.toml grep `irpc` 零命中。

---

## 相关但不在本分区

- **一条连接终身绑一个 ALPN** 的心智 → [01-concepts.md](01-concepts.md)
- **三层入站门禁**（`incoming_filter` / `EndpointHooks::after_handshake` / `on_accepting`）→ [02-connecting.md](02-connecting.md)
- **Gossip Broadcast**（官方把它归在 Connecting，不在 Protocols）→ [02-connecting.md](02-connecting.md)
- **该不该引入 blobs / docs / irpc** → [index-ecosystem-map.md](index-ecosystem-map.md)
- **撞到报错** → [index-gotchas.md](index-gotchas.md)
