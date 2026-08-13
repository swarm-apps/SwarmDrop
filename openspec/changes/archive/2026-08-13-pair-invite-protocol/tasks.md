# pair-invite-protocol 任务分解

## Phase 1 — net-base 签名 API

- [x] `SecretKey::sign(&[u8]) -> [u8; 64]` / `NodeId::verify(&[u8], &[u8; 64]) -> bool` 公开方法（包内层 libp2p-identity 能力）
- [x] 单测：sign/verify roundtrip、错签/错公钥拒绝、公钥从 NodeId multihash 恢复

## Phase 2 — PairInvite 类型与编码（core/pairing/invite.rs）

- [x] `PairInvite` 领域类型 + `InviteWire::V1` 镜像结构（签名尾置、地址二进制、字段序即契约）
- [x] 编码：KIND `sdinvite` + postcard + base32-nopad（小写规范/解码大小写不敏感）；`Display`/`FromStr`
- [x] `ParseError` 四分类（Kind/Postcard/Encoding/Verify）
- [x] 解码验签（signable=去尾 64B，公钥从 inviter_id 恢复）+ TTL 预检
- [x] `InviteRegistry`：capability 哈希表 + CAS 消费 + 过期清理
- [x] 单测矩阵：roundtrip / 大小写 / 逐字段篡改拒绝（含 transport_policy）/ 过期 / 重复消费 / 并发双花仅一胜 / 未知版本变体拒绝 / wire hex 快照固化（契约锁定）
- [x] 双 target：进 check-wasm 门禁（core 已在集合内，确认新模块无 native 残留）

## Phase 3 — 配对协议 Invite 变体

- [x] `protocol/pairing.rs`：`PairingMethod::Invite { invite_id, capability }` 定型（wire v2 增量）
- [x] `PairingManager`：`generate_invite(policy) -> PairInvite`（取 dialable 地址 + 签名 + 登记 Registry）
- [x] `PairingManager`：入站 PairHello 的 Invite 分支（Registry 校验/CAS → 复用 pending 决策 → 双确认 → 写配对记录）
- [x] 受邀方流程：解码验签 → 按 TransportPolicy 过滤地址 → connect + 身份 pin 校验 → 发起 RPC
- [ ] 集成测试：双节点完整 Invite 配对（成功/篡改/过期/双花四路径）；既有 Code/Direct 回归

## Phase 4 — 桌面最小入口

- [x] 命令 `generate_pair_invite` / `consume_pair_invite` + specta bindings 重生成（+ mobile uniffi 对等方法）
- [ ] UI：邀请串展示 + 5 分钟倒计时 + 失效/已用状态；粘贴接受入口；入站确认复用现有弹窗
- [ ] Lingui 文案 + `pnpm i18n:extract`

## Phase 5 — 收尾

- [ ] 桌面双机冒烟：生成 → 粘贴 → 双确认 → 配对记录落 keychain → 断线重连
- [ ] 知识库：net-kernel.md（wire 契约点补 Invite）、设计文档标注「阶段一已实施 + base32 修订」
- [ ] `cargo test --workspace` + 六 crate wasm 门禁全绿

## Phase 6 — 6 位配对码下线（提前，用户 2026-07-19 决策）

- [x] core：删 `pairing/code.rs`（PairingCodeInfo/ShareCodeRecord）、`PairingMethod::Code`、manager 的 DHT 分享码逻辑（generate_code/get_device_info/active_code/discovered_peers）
- [x] 宿主：桌面 `generate_pairing_code`/`get_device_info` → `generate_pair_invite`/`consume_pair_invite`；mobile 同；web 删 `share_code.rs`/lookup
- [x] `cargo test --workspace` 213 全绿 + 六 crate wasm 门禁 + clippy 干净
- [ ] 前端 UI 重写（桌面 React 配对屏 / RN / web）——邀请生成/二维码/粘贴入口，属独立 UI 工程
