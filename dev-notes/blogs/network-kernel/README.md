# network-kernel 系列：学 iroh，重构 SwarmDrop 的网络内核

这个系列记录 SwarmDrop 网络层的一次架构重写：**保留 libp2p 协议栈，学 iroh 的架构边界与 API 表达**，把旧的 `libs/core`（`swarm-p2p-core`）重构成 `crates/net`（内核）+ `crates/net-base`（类型底座）。

每篇聚焦一个内聚知识点、短小可读，配 mermaid 图与真实代码路径（读者可逐一核对）。决策依据见 [`dev-notes/why-libp2p-not-iroh.md`](../../why-libp2p-not-iroh.md)，内核开发速览见 [`dev-notes/knowledge/net-kernel.md`](../../knowledge/net-kernel.md)。

## 篇目

| # | 标题 | 一句话 |
|---|---|---|
| [00](00-why-not-migrate-iroh.md) | 保留 libp2p，却要学 iroh | 为什么评估了迁移 iroh、最后不迁、却照着 iroh 重写了架构——决策复盘与「要学的六件事」。 |
| [01](01-endpoint-facade.md) | Endpoint 门面：从裸 Swarm 到 `Arc<Inner>` | 门面是 Clone 廉价的句柄，单中枢 actor 是唯一 Swarm poll 点，快路径（开流/读状态）不经 actor。 |
| [02](02-router-protocol-handler.md) | 按协议路由，而不是巨型事件分支 | Router + ProtocolHandler，路由粒度是 **stream** 而非 iroh 的 per-connection ALPN；accept 在独立任务跑完；重复注册 panic。 |
| [03](03-event-dual-track.md) | 事件双轨制：watch 状态 vs 必达事件流 | 状态用 watch（last-value-wins 采样），边沿用 bounded mpsc；为什么两者不能混用，巨型 `NodeEvent` 怎么拆家。 |
| [04](04-extension-points.md) | 可插拔扩展点：async trait 好写又可存 | 「人体工学 RPITIT trait + Dyn 孪生 + blanket impl」范式，复用于 ProtocolHandler / RpcService / AddressLookup。 |
| [05](05-typed-rpc-on-streams.md) | 裸流上的 typed RPC | `Rpc<Req,Resp>` 一流一问一答；handler 能就地 await 用户决策，于是 `pending_id`/`PendingMap` 整套机制消失；1 MiB 帧防 OOM。 |
| [06](06-address-lookup-dht.md) | 可插拔发现与 DHT 子 API | 地址的 push 源（Swarm behaviour）与 pull 源（AddressLookup）分明；DHT 是独立子 API，`DhtKey` 长度前缀域分离。 |
| [07](07-type-boundary.md) | libp2p 类型不穿透：newtype 边界 | net-base 的 NodeId/Addr/ProtocolId 收口 libp2p 类型；base58/protobuf 兼容存量；serde 即 wire 契约——这条边界让「将来只改适配层」成立。 |

## 建议阅读顺序

```mermaid
graph LR
    R00["00 决策<br/>为什么这么做"] --> R01["01 Endpoint<br/>门面 + actor"]
    R01 --> R02["02 Router<br/>协议路由"]
    R02 --> R03["03 事件双轨"]
    R03 --> R04["04 扩展点范式"]
    R04 --> R05["05 typed RPC"]
    R05 --> R06["06 发现 + DHT"]
    R06 --> R07["07 类型边界<br/>收束全系列"]
    style R00 fill:#ffd43b
    style R07 fill:#4dabf7,color:#fff
```

- **按顺序读**（00 → 07）是设计的默认路径：从「为什么」到门面、路由、事件、扩展范式、控制面、发现，最后由类型边界收束。
- **只想懂 API 手感**：01（门面）→ 02（路由）→ 05（RPC）三篇即可拼出「怎么用」。
- **关心 iroh 对照**：00（要学什么）、02（stream vs ALPN 的刻意差异）、04（同款扩展范式）、03（Watcher 陷阱的取舍）四篇集中讲了「学什么、不抄什么」。
- **04 是范式篇**：02 和 05 都引用它；想深挖 async trait 的类型擦除可先读它，否则顺读即可。
