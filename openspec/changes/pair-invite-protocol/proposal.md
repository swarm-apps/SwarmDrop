## Why

6 位配对码体系有结构性安全缺陷（`dev-notes/architecture/iroh-invite-link-pairing-design.md` 已立案）：低熵空间可枚举/抢注、DHT 记录只能「找到候选节点」不能证明身份、记录覆盖/过期导致静默误连。设计文档给出了替代方案——一次性、可验证、可过期的**邀请链接 PairInvite**（Ed25519 签名 + 256bit capability + 5min TTL + 一次性消费，二维码只是同一链接的图形编码）。

设计文档写于 iroh 迁移假设下，但方案完全可移植到我们的 libp2p 栈（术语映射：EndpointId→NodeId 同为 Ed25519 公钥、EndpointAddr→NodeAddr、irpc→wire v2 的 Rpc）——重构时已在 `PairingMethod` 留 `Invite` 扩展位、DhtKey 做域隔离，就是为这一步。

2026-07-19 对 iroh-tickets（v1.0.0，本地源码快照）的三路源码级调研补足了全部工程细节：编码骨架（KIND 前缀 + postcard 单变体 enum + base32-nopad）、信任模型（身份=公钥、握手即验证，ticket 无签名也安全的前提与边界）、逐字段攻击分析（签名真正兜底的是 `transport_policy`——LocalOnly 承诺的完整性）、以及 libp2p 栈的落地对应物盘点（唯一前置：net-base 补公开 sign/verify）。时机上，core 刚完成 wasm-ready（openspec: core-wasm-ready），新协议实现在 core 一次、三端（桌面/移动/Web）共享——Web 临时端本就是设计文档的阶段三交付物。

## What Changes

- **net-base 补签名 API**：公开 `SecretKey::sign(&[u8]) -> [u8; 64]` 与 `NodeId::verify(&[u8], &[u8; 64]) -> bool`（内层 libp2p-identity 能力已在，仅 `#[doc(hidden)]` 未暴露；ed25519 PeerId 是 identity multihash，验签公钥可从 NodeId 完整恢复——invite 无需额外公钥字段）。
- **core 新模块 `pairing/invite.rs`**：`PairInvite` 领域类型 + wire 编码（照抄 iroh-tickets 四件套：KIND 前缀 `sdinvite`、postcard 单变体 enum 版本化、wire 镜像结构解耦、base32-nopad 小写规范/解码大小写不敏感）+ 签名尾置（signable = 去尾 64 字节，覆盖 enum tag 防降级，零成本规范化）+ `InviteRegistry` 发起端状态表（capability 哈希 + TTL + CAS Pending→Consumed 一次性消费，内存态）。
- **配对协议启用 `PairingMethod::Invite` 变体**：PairHello（invite_id + capability + 受邀方身份）→ 发起端校验（哈希匹配/TTL/CAS 消费）→ PairOffer/Accept/Commit 双确认 → 双方写配对记录。复用现有 `/swarmdrop/pairing/2` RPC 与 `PairingManager` 的 pending 决策机制；`TransportPolicy::LocalOnly` 由接收方按 `Addr::is_private_lan` 谓词过滤地址提示并禁用公网 fallback。
- **桌面最小入口**：`generate_pair_invite` / `consume_pair_invite` 命令（返回/接受编码字符串），UI 展示邀请串 + 倒计时 + 入站确认（复用现有配对确认弹窗）。
- **非目标（后续 change）**：链接落地页/深链/Universal Link 与二维码渲染（设计文档阶段二）；Web 临时端接入（阶段三，依赖 Web 消费 core 的组合根工作）；6 位码体系下线（阶段四——本 change 期间双轨并存）；配对记录的信任迁移。

## Capabilities

### New Capabilities
- `pair-invite`: 一次性签名邀请配对——发起方生成自包含、Ed25519 签名、5 分钟 TTL、一次性消费的邀请串（base32 文本，二维码/链接同源），受邀方验签解码后经既有配对 RPC 完成 capability 校验与双向确认，写入长期配对记录。篡改、过期、重放、并发双花均无法建立信任。

### Modified Capabilities
- `pairing`: `PairingMethod` 启用 `Invite` 变体；**6 位分享码机制整体废弃**（用户 2026-07-19 决策，不考虑兼容性）——删 `Code` 变体、`code.rs`、DHT 分享码发布/查询。保留 `Direct`（LAN mDNS 直连）。配对流程不再经 DHT，邀请自包含地址提示、带外传递。

## Impact

- **crates/net-base**：`node_id.rs` 增 sign/verify 公开方法（+单测：roundtrip / 错签拒绝）。
- **crates/core**：新 `src/pairing/invite.rs`（类型/编码/Registry，双 target——进 check-wasm 门禁）；`pairing/manager.rs` 增 invite 生成入口与 PairHello 的 Invite 分支处理；`protocol/pairing.rs` 的 `PairingMethod::Invite` 变体定型（携带 invite_id + capability）。
- **src-tauri**：新命令 2 个 + specta bindings 重生成；前端邀请生成/倒计时/确认 UI（Lingui 文案）。
- **依赖**：core 增 `postcard`、`data-encoding`（两者纯 no-std 友好，wasm 无虞）。
- **回归面**：现有 Code/Direct 配对回归（协议加变体不动存量路径）；新增 invite 单测矩阵（篡改各字段拒绝 / 过期拒绝 / 重复消费拒绝 / 并发双花仅一胜 / roundtrip / 大小写不敏感解码）；`cargo test --workspace` + 六 crate wasm 门禁。
- **风险**：编码格式一旦发布即为契约（版本化靠 enum 变体，不可轻改 V1 字段序）；`InviteRegistry` 内存态意味着发起端重启丢邀请（设计文档认可的语义——只持久化哈希的要求同时满足）。
