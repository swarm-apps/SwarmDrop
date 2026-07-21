## Why

spike 定的第一道门：**今天浏览器对我们的网络零公网可达入口**——生产只 listen 裸 TCP、bootstrap 是裸 IP，浏览器既拨不了裸 TCP 也拨不了裸 QUIC（`libp2p-wasm.md` §总判决）。给浏览器免域名免证书开公网门的唯一路是 **webrtc-direct**（地址 `/ip4/../udp/../webrtc-direct/certhash/<h>`，裸 IP + 自签证书哈希）。

**大债已付**：libp2p 全家已 pin git rev（`Cargo.toml:51-53`，正是为 webrtc-direct——crates.io `0.9.0-alpha.1` 握手坏、修复在 master）；native webrtc-direct server/dialer 已落地（`crates/net/src/transport.rs:48-65`，M2 接入），wasm webrtc-websys 已接（`:156`），spike 双端 Chrome 实测通过（RTT 400µs）。缺的**不是 transport，是三块收尾**。

用户 2026-07-21 决策：webrtc 要上，优先做到 **浏览器 ↔ 同一局域网内的桌面与移动端点直连**（不止 helper）。

三块缺口：
1. **证书持久化没接通**：`webrtc_cert_pem` 字段（`crates/net/src/config.rs:99`，native only）与 `builder.webrtc_certificate(pem)` setter（`crates/net/src/endpoint/builder.rs:120`）都在，但 core 的 `NetworkRuntimeConfig` 没暴露、`build_endpoint` 从不调 → 默认 `None` → 每次随机 certhash → 已分发地址重启即失效（`transport.rs:54-61` 已 `warn!("certhash addresses will not survive restarts")`）。
2. **无 `/webrtc-direct` listen**：桌面端点 + helper/bootstrap 都没 listen webrtc-direct 地址（`transport.rs` 注释「是否 listen 由地址决定」）。
3. **分享物不带 webrtc-direct 地址**：invite/分享地址没携带该多地址。

证书策略经 2026-07-20 网上调研（workflow `research-webrtc-cert-strategy`，4 角度源码级 + 对抗核验）定为**混合方案**：详见 design.md。

## What Changes

- **证书持久化（混合策略核心）**：`webrtc_cert_pem` 从 net `EndpointConfig` 提到 core `NetworkRuntimeConfig`；`build_endpoint` 在需要时调 `.webrtc_certificate(pem)`。桌面 PEM 存 Stronghold/keychain（已有），helper/bootstrap 存服务器数据目录（权限 600）。消掉 `transport.rs` 那条 warn。持久化单位是**整张证书 PEM**（含私钥），不是密钥——certhash=SHA-256(证书 DER)，且 webrtc-rs 每次重建强塞随机 SAN。
- **listen /webrtc-direct**：桌面与移动端点（以及后续 helper/bootstrap）各 listen 一个 `/ip4/../udp/../webrtc-direct` 地址（证书固定 → certhash 固定）。
- **分享物锚点分离**：invite 长期身份锚点 = `NodeId`（Noise 握手强制）；webrtc-direct/certhash 地址作为**机会主义 hint** 进 addr hints。拨号 `connect(NodeAddr::new(peer))` 先试 hint，失败回落 presence `OnlineRecordLookup` 按 NodeId 从 DHT 重解析当前地址。**绝不据 certhash 做身份判定或授权**（授权在 capability 哈希 + 握手 pin）。
- **可达性验证**：实证 webrtc-direct/certhash 地址进了 `shareable_addrs()`=`dialable()` 且被 presence announce（未被 loopback/unspecified/circuit 过滤误杀）；实证 wasm 受邀端「hint 失效 → 按 NodeId 重解析」兜底成立。
- **跨浏览器冒烟**：Chrome 已测，补 Safari / Firefox + https 上下文（`net-kernel.md` 已列为未测组合）。
- **非目标（后续/他轨）**：节点装配泛化（A 轨 `generalize-node-assembly`，正交并行）；证书自动轮换（默认**不轮换**——无过期墙，仅私钥疑似泄露时手动换）；重叠双证书滚动（WebTransport 式，对 webrtc-direct 收益不抵成本，否决）；web 前端 UI。

## Capabilities

### New Capabilities

- `webrtc-direct-transport`: 浏览器经 webrtc-direct（裸 IP + certhash，无 TLS/域名/CA）直连自托管 helper/bootstrap 与桌面端点，跨网可达。证书持久化固定 certhash；分享物以 `NodeId` 为长期身份锚点、certhash 地址为可失效 hint，hint 失效自动经 DHT/presence 按 NodeId 重解析当前地址。

## Impact

- **crates/net**：`config.rs` 的 `webrtc_cert_pem` 保持；`transport.rs` listen 逻辑按地址生效、消 warn。（transport 层已落地，本 change 主要打通配置与 listen 地址。）
- **crates/core**：`NetworkRuntimeConfig` 增 `webrtc_cert_pem`（或证书来源）字段并透传到 `build_endpoint`；helper/bootstrap 与桌面端点的 listen 地址集加 webrtc-direct 条目。**双 target——进 check-wasm 门禁**（web 拨号侧受益）。
- **src-tauri / mobile-core**：桌面首启生成 webrtc 证书 → `serialize_pem` 存系统 Keychain；移动端同样存 SecureStore；两端节点都 listen /webrtc-direct。
- **helper/bootstrap（47.115.172.218）**：持久化证书到服务器数据目录（600）+ listen /webrtc-direct；若把 certhash 硬编码进客户端 bootstrap 配置则**优先长期不轮换**，并预留「经 identify/DHT 动态取 helper certhash」的降级路径（避免换证=全客户端发版的运维刚性）。
- **crates/invite / pairing**：确认 `shareable_addrs()`/`dialable()` 纳入 webrtc-direct 地址；invite 携带它当 hint（依赖 `pair-invite-protocol`，其 inviter 已是 NodeId + addr hints 形状）。
- **回归面**：native 端 webrtc-direct server 持久化证书跨重启 certhash 稳定；presence 重解析链路（`presence/supervisor.rs` dial_backoff + OnlineRecordLookup）在 hint 陈旧时兜底；浏览器↔桌面 + 浏览器↔helper 双路径实拨。
- **风险**：① **14 天之争未 100% 闭环**——判定「webrtc-direct 无 14 天墙」基于 rcgen not_after≈4096 + WebRTC 自签不做 PKIX 校验，但某浏览器未来若向 WebTransport 收紧可能拒长期证书；缓解=混合方案的 NodeId 重解析兜底使其不致命，建议一次「长 not_after 固定证书跨三浏览器实拨」冒烟。② Safari/Firefox + https 未测。③ 私钥落盘无吊销，helper 被入侵可在传输层冒充该 certhash 端点，但**冒充不了 NodeId 身份**（Noise 握手会失败），损失限于「假装该 helper 可达」。④ 重解析依赖 DHT/relay 在线 + 对端近期 announce。⑤ 固定证书的跨连接指纹隐私面（桌面端可选不固定、只靠 NodeId 重解析——见 design D4）。
- **依赖底座（不重复造）**：`upgrade-rust-deps`（git-master libp2p pin = webrtc-direct 前置）、`pair-invite-protocol`（invite 携带地址 hint 的载体）、`bootstrap-node-settings` / `lan-helper-node`（helper 配置面）。
