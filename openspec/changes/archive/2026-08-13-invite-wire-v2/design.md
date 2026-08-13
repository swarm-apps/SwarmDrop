## Context

### 密度预算早就没有余量

`crates/core` 的 `invite_stays_scannable_at_every_scale` 用四档真实网络配置钉住了码面
（含 quiet zone，上限 98 模块）：

| 配置 | 加 WebTransport 前 | 加了但不裁 | 现在（加了 + 裁） |
|---|---:|---:|---:|
| 家用 lan + circuit | 85 | 93 | 93 |
| 公网 lan + public + circuit | 89 | 105 | 97 |
| CGNAT shared + lan + circuit | 89 | 105 | 97 |
| **満配 四类齐全** | **97** | **117** | 97（11 → 8 条，WebTransport 全裁） |

第一列就已经贴着上限。也就是说 `fit_invite_to_scannable` 不是给某次改动擦屁股的补丁，
而是一道**长期处于触发状态**的闸——它每触发一次，用户就少一条真实可达路径。

### 膨胀来自 wire 把节点级数据按地址重复

按 multiaddr 二进制逐段核算（varint 协议码 + 载荷）：

| 段 | 字节 |
|---|---:|
| `/ip4/A.B.C.D` | 5 |
| `/tcp/P` | 3 |
| `/udp/P` | 4 |
| `/quic-v1` · `/webtransport` · `/webrtc-direct` · `/webrtc` · `/p2p-circuit` | 各 2 |
| `/certhash/<sha256 multihash>` | **37** |
| `/p2p/<id>` | **41** |

于是単条地址（下表右列的「紧凑后」是设计值，左列已由 tasks 1.1 **实测逐行验证**）：

| 形态 | 现在 | 紧凑后 |
|---|---:|---:|
| `/ip4/x/tcp/4001` | 8 | 10 |
| WebTransport（2 个 certhash） | **87** | **12** |
| webrtc-direct（1 个 certhash） | 48 | 11 |
| circuit tcp | 51 | 12 |
| circuit WebTransport | **130** | **14** |
| circuit webrtc（打洞） | 53 | 12 |

満配那档（3 张网卡 ×3 条 + 3 条 circuit）：

```
现在   3×143 + 234                =  663 字节   ← 实测（tasks 1.1）
紧凑   3×(10+12+11) + (12+14+12) + 表 130    =  267 字节     省 60%
                                     ↑
                     certhash 3×32 + relay 1×32，固定成本，地址越多越划算
```

以实测 117 模块反推（v23 / ECL M，payload 约 1000 个 base32 字符 ≈ 625 字节，其中核心字段
约 143 字节不可压），地址区缩掉六成后**満配估算落在 90–98 模块**。

### 顺带一笔：链接也跟着腰斩

链接与二维码是同一个字符串。按上面的比例，満配邀请链接从约 1040 字符降到约 570，
家用档从约 620 降到约 420。粘进 IM 的那一坨短一半，且离「被聊天客户端折行/截断」更远。

## Goals

- 把地址区的重复数据消掉，让常见配置有**真实余量**而不是贴着线。
- 让「按码面回收地址」成为**无私钥的纯函数**，从生成时的一次性动作变成任何持串方都能做的操作。
- 传输知识不外溢：`crates/invite` 继续只搬 `Addr` 字节。

## Non-Goals

- **通用压缩（deflate/brotli）**。见 Decision 1 的对比——它抓不到 `/p2p/<self>` 这类语义冗余，
  且高熵尾部（签名 64 + capability 16 + 身份 32 ≈ 112 字节）不可压，小 payload 下反涨，
  于是还要带一位「压没压」标志。
- **L1 单一价值序重构**（`select_invite_addrs` 与 `drop_least_valuable_addr` 各表达一遍价值序）。
  由前置闸的结论决定要不要做，见下方「前置闸」。
- **把 `INVITE_QR_MAX_MODULES` 变成参数**（预算由各端 UI 传入自己的码面 px）。同上。
- 邀请的授权语义：capability、TTL、一次性消费、注册表持久化、canonical 链接形态，全部不动。

## Decisions

### Decision 1 — 紧凑编解码住 `crates/net-base`，不住 `crates/invite`

```rust
// crates/net-base/src/compact.rs
pub struct CompactAddrs {
    certhashes: Vec<[u8; 32]>,   // 全表去重（含 relay 自己的证书）
    relays: Vec<[u8; 32]>,       // relay 身份，去掉 multihash 头只留 ed25519 公钥
    paths: Vec<CompactPath>,
}

enum CompactPath {
    Direct { host: Host, port: u16, wire: Wire, certs: Vec<u8> },  // certs = 表下标
    Circuit { relay: u8, base: Box<CompactPath>, hole_punch: bool },
    Raw(Vec<u8>),
}

pub fn pack(addrs: &[Addr]) -> CompactAddrs
pub fn unpack(c: &CompactAddrs) -> Vec<Addr>
```

**没有 `self_id` 参数**——讨论时设想的「省掉 `/p2p/<本机>` 后缀」这件事**不成立**，见
Decision 7。

**为什么不放 `crates/invite`。** 反对结构化的常见理由是「会让 invite 认识 WebTransport」——
但传输知识本来就全部收口在 net-base（`is_webtransport` / `is_quic_v1` / `is_circuit` /
`is_webrtc` / `transport` / `dial_tier` 都在 `addr.rs`）。放这里，`crates/invite` 的模块文档
「只依赖 net-base 的身份/地址类型」继续成立，一行传输判断都不用加。

**为什么不是通用压缩。**

| | 结构化 | deflate |
|---|---|---|
| certhash 跨地址重复 | 表 + 下标，彻底消掉 | 能抓（字面重复） |
| relay `/p2p/<id>` 跨 circuit 重复 | 表 + 下标，**41 → 1 字节** | 能抓（字面重复） |
| 协议码 / 端口 / IP | 定长字段 | 部分抓到 |
| 高熵尾部 112 字节 | 不受影响 | 不可压，且要付 header |
| 小 payload | 无回退 | 可能反涨 → 需要「压没压」标志位 |
| 体积可推理 | 逐段可算（上表） | 不可预测 |

**相对已确认方案的一处细化：`relays` 也建表。** 讨论时定的形态是 `Circuit { relay: NodeId }`
内联身份；改成表下标是因为多条 circuit 常走同一个 relay（本仓只有一个 bootstrap），
内联要付 32 字节 × 条数。判据与 certhash 表完全同构，不引入新概念。

**`certhashes` 是「出现过的全部证书摘要」，不是「本机的证书」。** WebTransport 基址的 circuit
地址里带的是 **relay 的**证书摘要，与本机的两张无关，但它们进同一张表——表的语义是去重，
不是归属。

### Decision 2 — 认不出就 `Raw`，绝不猜

编码器只对**完整匹配**已建模形态的地址走结构化路径，任何一段不认识、或段序不符预期，
整条落 `Raw(bytes)` 原样搬。

不这么做的失败形态是最坏的那种：产出一条形式合法、能进邀请、能扫码、**只是拨不通**的地址。
和刚修掉的 circuit 裁剪 bug 同形——邀请编得出、扫得动，跨网时零可达路径。

`Raw` 也让「新传输要不要改 wire」这个问题消失：新传输不改也能进邀请，只是没有体积收益。

### Decision 3 — 地址提示移出签名覆盖范围

```
现在   sign( postcard(V1{ …字段…, addrs, sig=0 })[..len-64] )      addrs 在内
之后   sign( b"swarmdrop-invite-v2" ‖ postcard(SignedCore) )       addrs 在外
```

**依据：地址提示是下游自证的。** 拨过去之后身份由 Noise / QUIC-TLS 对**已签名的**
`inviter_id` 强制校验。篡改地址只有两种结果：拨号失败，或被引向一个完不成握手的第三方。
签名保护的是 capability 真实性与 `LocalOnly` 承诺不可降级——这两样都在 `SignedCore` 里。

**代价如实记**：能改写邀请文本的攻击者可以删空或替换地址提示，使受邀方拨不通。这是拒绝服务，
与「把整串改坏」同级，不涉及凭证或身份。

**收益**：`fit_invite_to_scannable` 不再需要 `secret`。签名从「每轮裁剪一次」变成整个流程零次
——不是被优化掉，是在构造上不存在了。这条顺带打开了后续可能性：二维码可以在渲染侧按自己的
码面裁剪，而渲染侧没有私钥（属于前置闸之后的范围，本 change 不做）。

### Decision 4 — 签名对象显式，不再靠位置约定切分

当前实现是「序列化一遍占位版，切掉末 64 字节当 signable」，因此背着一条隐式契约：
`signature` 必须是结构体最后一个字段（`InviteV1` 的字段注释写着这件事）。地址提示移出去后
签名不再位于末尾，这条契约既不成立，也不该换成另一条同样脆弱的位置约定。

改为：`SignedCore` 单独序列化成一段字节，签名对象是 `域分隔标签 ‖ 那段字节`。标签含版本标识，
于是跨版本的签名复用天然失败。

### Decision 5 — 不保留 V1 解码

邀请是 TTL 24 小时的一次性凭证，跨版本共存窗口自然收敛；Web 端尚无真实用户。双版本解码的
那条分支只在极窄时间窗被执行到——那类分支的实际表现是「没人测过，出问题时也没人想得起它」。
旧邀请的失败形态是「不是有效的配对邀请」，用户请对方重新生成一条即可，可自恢复。

### Decision 6 — 解码失败不得降级为零地址邀请

结构不可解析 / 版本不认识 / 验签失败 → 整条报错。**不**做「核心可用、提示丢空」的降级：
零地址邀请编得出、扫得动、复制得走，唯独没有任何东西可拨，两端都不报错。这与
`drop_least_valuable_addr` 里「最后一条谁都不许动」守的是同一条价值判断。

例外只有一处，且方向相反：结构**已经解出**之后，单条路径不可还原（如下标越界）时跳过该条
并继续——那是少一条可拨路径，不是零地址。

### Decision 7 — 不做 `/p2p/<本机>` 后缀省略：那个形态不会出现在邀请里

设想中的一笔节省是「circuit 可达地址 `<relay>/p2p-circuit/p2p/<本机>` 的尾部由 `inviter_id`
补回，每条省 41 字节」。**实现前查证，前提不成立。**

邀请的地址来自 `Endpoint::watch_addrs().get().dialable()` = `listen ∪ external`，而其中的
circuit 地址是 libp2p relay listener 报上来的 `<relay-base>/p2p-circuit`——**不带自身身份**。
那条带 `/p2p/<本机>` 的地址由 `Actor::circuit_addr_for` 拼出，唯一消费点是
`actor.rs:1661` 的 `RelayState::Active { circuit_addr }`，即**节点状态弹窗的诊断展示值**；
它的 rustdoc 自己写着「这是展示值，不是可拨地址，不可当作可达地址分发给对端」。

于是这项省略是给一个不会出现的形态写的机器，代价是 `pack`/`unpack` 各多一个参数、
一条 spec 要求、以及 `crates/invite` 里多传一个值。删掉。

万一将来真出现带自身后缀的地址：它落 `Raw`，逐字节还原，只是不省字节——安全降级。

## 前置闸：这一步的结论决定本 change 要不要扩大

紧凑编码落地后，用同一组四档夹具重测満配那档的模块数：

| 实测落点 | 结论 |
|---|---|
| **≤ 80** | 到此为止。余量够厚，裁剪路径基本不触发 |
| **80 – 98** | **追加「预算由 UI 传入」**：三端把自己的码面 px 传进 QR 接口，core 只留 `MIN_PX_PER_MODULE`。桌面拿回自己的 240px（120 模块），満配立刻有 20+ 格余量，且跨语言常量副本归零 |
| **> 98** | 上一格 + 单一价值序重构（`select` 与 `drop` 合并成一个显式 rank，裁剪变成截断） |

### 闸门结论（2026-08-13 实测）

| 配置 | wire v1 不裁 | wire v1 裁后 | **wire v2** |
|---|---:|---:|---|
| 家用 lan + circuit | 93 | 93 | **85**（6 条全留） |
| 公网 lan + public + circuit | 105 | 97 | **89**（9 条全留） |
| CGNAT shared + lan + circuit | 105 | 97 | **89**（9 条全留） |
| 満配 四类齐全 | 117 | 97（12 → 8） | **93**（12 条全留） |
| 満配 + 40 字中文设备名 | — | — | **97**（12 → 8，不裁则 101） |

**落在 80–98 格。** 常规设备名下裁剪已经完全不触发——这是本 change 的主要成果。

**但闸门暴露了一件设计时没想到的事：最坏情况的主导变量不再是地址数，而是设备名。**
`DeviceName::MAX_CHARS = 40`，40 个中文字 = 120 字节，比压缩后的整个地址区（约 252 字节）的
一半还多。也就是说「压缩解决了密度问题」只是半句话：对短名成立，对顶格名不成立。

按上表，下一步是「预算由 UI 传入」——桌面按自己真实的 240px = 120 模块，101 放得下，
裁剪归零。**单一价值序重构（L1）仍不必做**：裁剪路径只剩一种极端情形，且再往前一步就
彻底不触发了。

这道闸的判定已完成，`INVITE_QR_MAX_MODULES` 与三端 UI 的改动**由用户拍板是否纳入本 change**。

## Open Questions

1. ~~真机的 circuit 地址带不带 `/p2p/<self>` 后缀？~~ **已查实：不带**，见 Decision 7。
   夹具是准确的，估算无需调整。
2. **`Raw` 的体积回退有多大？** 一条走 `Raw` 的地址比现在还多 1–2 字节（变体 tag + 长度前缀）。
   若真机上出现大量未建模形态，收益会被吃掉。→ 由 tasks 1.2 的快照一并确认建模覆盖率。
3. **`Host` 要不要建模 DNS？** 本仓自己的地址全是 IP，但用户可以配置 DNS 形式的自定义
   bootstrap，而 circuit 基址取自地址簿。`Raw` 兜得住，问题只是省不省得下来。
   → 先走 `Raw`，由 Open Question 2 的数据决定是否补建模。
