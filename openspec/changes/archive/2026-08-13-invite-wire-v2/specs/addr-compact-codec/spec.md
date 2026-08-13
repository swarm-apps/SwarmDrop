## ADDED Requirements

### Requirement: 紧凑地址编解码的还原保真

`crates/net-base` SHALL 提供 `&[Addr] ⇄ CompactAddrs` 的紧凑二进制表示。对于任何由本编码器
产出的 `CompactAddrs`，解码 MUST **逐字节还原**输入的每一条 `Addr`，顺序、条数、字节内容
三者都不得变化。

判据用字节而非语义相等：地址的全部价值就在那串字节上，还原出一条「看起来对但少一个
certhash」的地址不会报错，只会让对端拨不通——与错误的地址提示是同一种失败形态，而它比
「没有这条地址」更难归因。

#### Scenario: 已建模形态往返

- **WHEN** 输入含 `/ip4/…/tcp/…`、`/ip4/…/udp/…/quic-v1`、
  `/ip4/…/udp/…/quic-v1/webtransport/certhash/<h1>/certhash/<h2>`、
  `/ip4/…/udp/…/webrtc-direct/certhash/<h>`、
  `/ip4/…/tcp/…/p2p/<relay>/p2p-circuit`、`…/p2p-circuit/webrtc`
- **THEN** `unpack(pack(addrs))` MUST 逐字节等于 `addrs`

#### Scenario: IPv6 与非 IP 主机

- **WHEN** 输入含 `/ip6/…` 地址
- **THEN** MUST 与 IPv4 同样走结构化路径并逐字节还原
- **WHEN** 输入含 `/dns4/…`、`/dnsaddr/…` 等非 IP 主机段
- **THEN** MUST 逐字节还原（走结构化路径或 `Raw` 兜底皆可，还原保真是唯一判据）

### Requirement: 未知形态原样透传

编码器 MUST NOT 猜测。只有**完整匹配**已建模形态的地址才走结构化路径；任何一段不认识、
或段序不符合预期的地址，MUST 整条落 `Raw(bytes)` 原样搬运。

不这么做的后果不是编码失败，而是**静默产出一条拨不通的地址**：形式合法、能进邀请、能扫码，
只有对端连不上。`crates/core` 的 `unknown_transport_class_still_keeps_one_path` 已经钉着
「未知传输也要留一条路径」这条规则，本要求是它在编码层的对应物。

#### Scenario: 未来传输段

- **WHEN** 输入含一条编码器不认识的地址（如 `/ip4/…/udp/…/quic-v2`）
- **THEN** 该条 MUST 以 `Raw` 编码，且 `unpack` 后逐字节等于输入

#### Scenario: 段序异常

- **WHEN** 输入含形态可识别但段序与建模不符的地址
- **THEN** MUST 落 `Raw`，MUST NOT 按建模形态重排后编码

### Requirement: 节点级数据去重

certhash 与 relay 身份 SHALL 各自存入去重表，路径按下标引用。同一份 certhash 在多条地址上
出现时 MUST 只在 wire 里出现一次。

这是本编码存在的理由：一条 WebTransport 地址 87 字节里 74 字节是 certhash，而 certhash 是
节点级的（当前 + 下一张，14 天轮换），三张网卡上逐字相同。

#### Scenario: 多网卡共用同一组 certhash

- **WHEN** 输入含 3 条 WebTransport 地址，各自 IP 不同但携带同一组 2 个 certhash
- **THEN** 编码产物中该 certhash 的 32 字节摘要 MUST 各出现且仅出现一次

#### Scenario: 多条 circuit 经同一 relay

- **WHEN** 输入含 3 条 circuit 地址，relay 相同
- **THEN** 该 relay 身份 MUST 只在 wire 里出现一次

### Requirement: 编解码不依赖外部上下文

`pack` / `unpack` SHALL 是仅以地址列表为输入的纯函数，MUST NOT 需要调用方额外提供身份、
时间或配置。任何「由调用方的其他字段补回被省略的段」的优化 MUST NOT 引入。

依据：这类优化会把「地址列表怎么编码」与「谁在用它」耦合起来，让同一份编码在不同调用方
手里还原出不同结果——而还原保真是本 capability 唯一的硬要求。具体到本仓，曾设想省略
`/p2p/<本机>` 后缀由邀请的 `inviter_id` 补回，但那个形态根本不出现在
`Endpoint::watch_addrs().dialable()` 里（见 design.md Decision 7）。

#### Scenario: 尾部带身份段的地址

- **WHEN** 输入 `/ip4/…/tcp/…/p2p/<relay>/p2p-circuit/p2p/<id>`
- **THEN** 该 `/p2p/<id>` 段 MUST 完整保留并逐字节还原，无论 `<id>` 是谁

### Requirement: 损坏输入的失败语义

对**非本编码器产出**的输入（被篡改或损坏），解码 SHALL 逐条隔离：单条路径不可还原时
MUST 跳过该条并继续解其余路径；MUST NOT 因一条路径不可还原而丢弃全部地址提示。

地址提示是尽力而为的信息，少一条只是少一条可拨路径；而整体丢弃会把一条本可用的邀请变成
零地址邀请——那是最坏的输出形态。

#### Scenario: 下标越界

- **WHEN** 某条路径引用的 certhash 下标超出表长
- **THEN** MUST 跳过该条路径，其余路径 MUST 正常还原
