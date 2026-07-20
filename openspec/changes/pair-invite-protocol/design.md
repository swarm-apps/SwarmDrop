# pair-invite-protocol 设计决策

基于 2026-07-19 iroh-tickets 三路源码级调研（v1.0.0 快照 /Volumes/yexiyue/iroh-study/，
全部论断带文件行号，原始数据在当次会话 workflow 产出）+ 原设计文档
`dev-notes/architecture/iroh-invite-link-pairing-design.md` 的 libp2p 化落地。

## D1：信任模型——为什么 ticket 不签名而我们要签

iroh ticket 无签名/无 TTL/无一次性（纯明文地址快照）的成立前提：**身份=公钥，握手即验证**
（拨号方把目标 id 编进 TLS SNI，`verify_server_cert` 逐字节比对 SPKI——连错人在密码学上
不可能，地址被篡改最坏连不上）。这个性质 libp2p 同构（noise 握手同样强制 PeerId），我们白拿。

签名保护的是**身份 pin 覆盖不到的字段完整性**，逐字段攻击分析：

| 被篡改字段 | 后果 | 签名必要性 |
|---|---|---|
| `inviter_addrs` | 无害于身份，可引流恶意 relay | 中 |
| **`transport_policy`** | **LocalOnly→Auto 静默走公网 relay，违反产品承诺** | **高——签名真正兜底的字段** |
| `display_hint` | 确认界面冒充（社会工程） | 中（终防线=双方确认+短指纹） |
| `capability` | 哈希不匹配仅 DoS | 低 |
| 整链替换 | 签名防不了（=换邀请人），防线在带外信道+用户确认 | — |

结论：签名把攻击面从「改任意字段」压缩到「换整条链接」，成本 64 字节 + 一次验签。

## D2：编码——照抄 iroh-tickets 四件套 + base32 修订原设计

1. **KIND 前缀**：`sdinvite`（纯字母，QR alphanumeric 合法）+ 无分隔符直拼。
2. **postcard 单变体 enum 版本化**：`enum InviteWire { V1(InviteV1) }`——强制 1 字节判别码，
   未知版本天然解码失败；n0 口径是「变体可并存皆有效」非线性升级（BlobTicket 至今 Variant0
   与 EndpointTicket Variant1 共存是活证）。比 `version: u16` 字段优。
3. **wire 镜像结构解耦**：`InviteV1` 是手工镜像非 derive 领域类型——领域类型改字段不碰 wire。
   地址用 `Multiaddr::to_vec()` 二进制（文本形态 ~2x 膨胀，QR 长度敏感）。
4. **base32-nopad 替代原设计的 base64url**（修订设计文档 §7）：小写为规范形态（URL/双击/
   口述友好）、解码大小写不敏感——生成二维码前整串 `to_ascii_uppercase()` 走 QR
   alphanumeric mode，比 base64url（必掉 byte mode）省 ~17% 模块数。
   长度预算：InviteV1 ≈ 200-240 B（含 64B 签名 + 2 条 LAN 地址）→ 330-390 字符，
   QR version 13-15（M 纠错），无压力。
5. **ParseError 四分类**（Kind/Postcard/Encoding/Verify）照抄——验签失败/TTL 预检挂 Verify。

## D3：签名尾置——零成本规范化

`signature: [u8; 64]` 放 `InviteV1` **末位**（postcard 定长数组不写长度前缀）→
signable = `bytes[..len-64]`，天然覆盖 wire enum tag（**绑定版本，防降级攻击**），
无需第二次规范化序列化。验签公钥从 `inviter_id` 就地恢复（ed25519 PeerId 是 identity
multihash ≤42B 不哈希，`PublicKey::try_decode_protobuf(multihash.digest())`）——
invite 不需要独立公钥字段。

## D4：capability/TTL/一次性——发起端内存状态表，网络层零需求

```rust
struct PendingInvite { capability_hash: [u8; 32], expires_at: u64, state: InviteState }
enum InviteState { Pending, Consumed { by: NodeId }, Revoked }
```

- 发起端只存 `sha256(capability)`，明文绝不落盘/日志（设计文档 §7 要求）
- TTL 权威判定在发起端（收 PairHello 时查表）；接收端解码时预检仅为 UX
- **一次性 = 原子 CAS Pending→Consumed**——防两台设备同时扫同一码的并发双花；
  Consumed 后才允许 PairCommit
- 内存态（重启丢邀请是可接受语义）；与现有 6 位码的 DHT-TTL 机制正交，可整体替换

## D5：TransportPolicy 执行

net-base 已备好谓词（`Addr::is_private_lan / is_public_routable / is_circuit`）：
LocalOnly 时接收方过滤地址提示只留私网、禁用 relay fallback；该字段在签名覆盖内（D1）。

## D6：范围切分（修订——用户 2026-07-19 决策调整）

- **本 change 已含**：协议内核 + 单测矩阵（Phase 1-2 ✅）、Invite 协议接入（Phase 3 ✅）、
  **6 位配对码整体废弃**（原阶段四提前，`PairingMethod` 只剩 `Direct` + `Invite`，无双轨期）。
- **三端 UI**（本次探索后另立 change）：桌面 React + 移动 RN 配对屏重写（生成二维码/复制链接/
  倒计时 + 扫码/粘贴/剪贴板感知）；web 简单 demo（后续交外部开发者做完整 UI）。
- **后续**：深链/Universal Link（设计文档阶段二的链接分发）、Web 临时端完整版（依赖 Web
  消费 core 组合根，另一条线）。

## D7：剪贴板感知 UX（三端「感知 + 一键确认」，非全自动）

复制邀请链接后回到应用自动继续配对——但静默读剪贴板的隐私模型三端天差地别，故统一为
「**感知到就亮一键入口，用户点击才真读+发起**」（邀请是信任凭证，全自动发起配对不留确认反
危险，一键确认同时是安全闸）：

| 端 | 感知手段 | 交互 |
|---|---|---|
| 桌面 Tauri | 窗口 focus 时静默读（无提示）+ `startsWith` 前缀校验 | 命中 → 顶部亮条「检测到配对邀请，点此配对」 |
| iOS | `Clipboard.hasStringAsync()`（**只探有无字符串、不读内容、不弹系统横幅**） | 有内容 → 亮「粘贴邀请」chip → 点击才 `getStringAsync` 真读（横幅由用户触发，合规） |
| Android | 回前台读 + 前缀校验（12+ 读时弹「已粘贴」toast，可接受） | 同 iOS 的 chip 交互 |
| web demo | 无静默读能力（`readText` 需手势 + 权限） | 粘贴框 + 按钮 / Ctrl+V paste 事件 |

前缀校验用 `sdinvite`（裸串）或 `swarmdrop://`/`https://swarmdrop.app/i#`（深链）即可秒判真伪。
读到后本地 `PairInvite::decode` 验签 → 亮确认卡（对端设备名/平台/短指纹）→ 用户确认发起。

## 术语映射（原设计文档 iroh 术语 → 本栈）

| 设计文档 | 本栈 |
|---|---|
| EndpointId | `NodeId`（同为 Ed25519 公钥） |
| EndpointAddr | `NodeAddr`（net-base） |
| irpc + `com.yexiyue.swarmdrop.pairing/1` | wire v2 Rpc + `/swarmdrop/pairing/2` 的 Invite 变体 |
| iroh connect + 身份验证 | swarmdrop-net `connect` + noise 握手 PeerId 强制 |
