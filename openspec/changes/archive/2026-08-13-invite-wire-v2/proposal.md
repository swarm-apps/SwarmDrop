## Why

邀请二维码的密度预算已经没有余量。`invite_stays_scannable_at_every_scale` 实测：**満配桌面
（CGNAT 覆盖网 + 局域网 + 公网 + circuit 同时在场）未裁剪是 117 模块，上限 98**，而在把
WebTransport 加进邀请之前它就已经是 **97**——距上限只剩一格。于是每加一样东西都要靠
`fit_invite_to_scannable` 逐条丢地址来抵消，而丢掉的是真实的可达路径。

膨胀的根因是 wire 把**节点级数据按地址重复**：

- `announce_addrs` 对每个本机 IP 产出一条 WebTransport 地址，**每条都带全部 certhash**，
  而 certhash 是节点级的（当前 + 下一张，14 天轮换）。三张网卡 = 6 个 certhash 段，4 个是重复的。
  单个 certhash 段 37 字节，一条 WebTransport 地址 87 字节里 **74 字节是 certhash**。
- circuit 地址携带 relay 的 `/p2p/<id>`（41 字节），多条 circuit 走同一个 relay 时逐条重复，
  而本仓只有一个 bootstrap。

按四档真实夹具逐段核算（字节账已由 tasks 1.1 实测逐行验证），地址区 **663 → 267 字节
（省 60%）**，満配那档从 117 模块降到估算 **90–98**。链接与二维码是同一个串，所以邀请链接
也同步腰斩：満配约 1040 → 570 字符，家用档约 620 → 420。

第二件事顺带解决：`fit_invite_to_scannable` 每轮裁剪都要重新签名，因为签名覆盖了地址提示。
而**地址提示不需要签名**——拨过去之后身份由 Noise/TLS 对已签名的 `inviter_id` 强制校验，
篡改地址只能导致拨号失败或引向一个完不成握手的第三方。签名保护的是 capability 的真实性，
不是路由的可用性。把它移出签名覆盖范围后，裁剪成为**无私钥的纯函数**。

## What Changes

- **BREAKING** — 新增 `InviteWire::V2`，**不保留 V1 解码**。Web 端还没有真实用户，桌面/移动
  的存量邀请 TTL 只有 24 小时，且邀请本来就是一次性凭证；保留双版本解码换不到任何东西，
  只换来一个要永久维护的分支。
- **新增 `crates/net-base/src/compact.rs`** —— `&[Addr] ⇄ CompactAddrs` 的紧凑编解码。
  certhash 与 relay NodeId 各自建表去重、路径按下标引用；认不出的形态落 `Raw(bytes)` 原样透传。
  `crates/invite` 只调 `pack`/`unpack` 两个函数。
  放 net-base 而不是 `crates/invite`：传输知识（`is_webtransport` / `is_circuit` / `transport`）
  本来就全部收口在这里，`crates/invite` 得以继续只搬 `Addr`，一行传输判断都不加。
- **BREAKING** — 签名覆盖范围收缩到不含地址提示。wire 改为 `V2 { core: Vec<u8>, signature,
  hints }`，签名对象是 `域分隔标签 || core 字节`，**不再依赖「signature 必须是最后一个字段」
  这条隐式契约**（当前实现靠切掉末 64 字节取 signable）。
- **`fit_invite_to_scannable` 不再需要私钥**，签名从每轮一次变成整个流程零次。
- **一道前置闸**：紧凑编码落地后用同一组夹具重测模块数，据此决定要不要把预算参数化
  （见 tasks 第 1 组）。**这一步的结论可能扩大本 change 的范围。**

**不做**（已评估，理由见 design.md 的 Non-Goals）：通用压缩、L1 单一价值序重构、
把 `INVITE_QR_MAX_MODULES` 变成参数——最后一条由前置闸的结论决定。

## Capabilities

### New Capabilities

- `addr-compact-codec`: `Addr` 列表的紧凑二进制表示。pack/unpack 的**逐字节还原**契约、
  未知形态的 `Raw` 兜底、节点级数据去重、自身 `/p2p/<id>` 后缀的省略与还原。
- `pair-invite-wire`: 邀请 wire 的结构与**签名覆盖边界**。哪些字段进签名、地址提示为什么
  不进、解码时各区域的失败语义。

### Modified Capabilities

无。`pair-invite` / `pair-invite-link` / `invite-lifecycle` 三个 capability 的 spec 仍在各自
未归档的 change 里（`openspec/specs/` 下没有它们），本 change 不改它们已写下的任何要求：
capability 语义、TTL、一次性消费、canonical 链接形态、注册表持久化全部不变。

## Impact

| 层 | 改动 |
|---|---|
| `crates/net-base` | 新增 `compact.rs`（`CompactAddrs` / `pack` / `unpack`）。`Addr` 的公开面不变 |
| `crates/invite` | `InviteWire` 换 V2；`encode` / `decode_wire` 重写签名切分；`PairInvite` 领域类型**不变** |
| `crates/core` | `fit_invite_to_scannable` / `drop_least_valuable_addr` 去掉 `secret` 参数；`encode_invite` 少传一个参数 |
| 三端 UI | **零改动**。`encode_invite` 的签名与返回值形态不变，`invite_qr_svg` / `invite_qr_matrix` 不变 |
| wire 兼容 | **断裂**。跨版本的邀请互相解不开，表现为「不是有效的配对邀请」——这是可接受的失败形态（清晰、可自恢复：重新生成一条） |
| 依赖 | 无新增 |

`./scripts/check-wasm.sh` 覆盖 net-base / invite / core / web 四个受影响 crate，必过。
