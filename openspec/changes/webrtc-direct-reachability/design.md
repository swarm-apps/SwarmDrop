# webrtc-direct-reachability 设计决策

基于 2026-07-20 webrtc-direct 证书策略调研（workflow `research-webrtc-cert-strategy`，4 角度：
libp2p API / 浏览器约束 / 真实部署 / 类比与安全，源码级 + 对抗核验）。前提：用户已拍板
「webrtc 要上、做到浏览器↔桌面直连」。

## D1：证书生命周期真相——无「过期墙」，14 天上限是张冠李戴

- **有效期**：webrtc-direct 服务端证书链 rust-libp2p → webrtc-rs v0.17.1 → rcgen 0.13，默认 `not_after ≈ 4096-01-01`，实测等于永不过期。
- **握手不校验证书年龄**：webrtc-direct **刻意禁用证书指纹校验、也不看 X.509 `notAfter`**——真身份认证在 DTLS 之上的 Noise 握手 + Ed25519 peer-id，`certhash` 仅进 Noise prologue 做带外绑定。⇒ 服务端证书年龄对浏览器不可见，**不存在过期墙**。
- **辟谣**：调研 4 角度里有 2 个坚称「自签证书 MUST ≤14 天」——那是 libp2p **WebTransport** spec 对浏览器 `serverCertificateHashes` 的约束，被错配到了 webrtc-direct。go-libp2p 的重叠双证书/滚动机制同样只在 WebTransport 传输上，webrtc-direct 传输并无此入口。**判定：webrtc-direct 服务端证书可长期固定，14 天上限不适用。**（残余不确定性见 D7。）

## D2：持久化单位是整张证书 PEM，不是密钥

`certhash = multihash(SHA-256(叶子证书 DER))`（code 0x12，base64url，塞进 multiaddr `/certhash` 段）。DER 任意 1 bit 变 → certhash 变 → 已分发地址失效。且 webrtc-rs `from_key_pair` 每次强塞随机 16 字符 SubjectAltName ⇒ **「只存 keypair 再重建」拿不到稳定 certhash**。故持久化单位必须是**整张证书 PEM**（含 PKCS#8 私钥）：rust-libp2p 开 `pem` feature 的 `Certificate::serialize_pem()` 落盘 + `from_pem()` 还原，这是官方标准做法（rust-libp2p 无内建 keystore helper）。

本项目已接好注入线，只差供给：`transport.rs:51-62` 读 `config.webrtc_cert_pem`——`Some` 走 `Certificate::from_pem`，`None` 走随机 `generate` 并 `warn!`；注入口 `endpoint/builder.rs:120` 的 `webrtc_certificate(pem)`；字段 `config.rs:99`（native only）。生产当前默认 `None` → 每次随机。

## D3：混合策略——NodeId 是长期锚点，certhash 地址是可失效 hint（推荐）

调研否决了两个极端：单把 certhash 烤进邀请（换网/换证必陈旧 → 邀请死）、单靠纯 NodeId 重解析（牺牲首连速度 + 离线不可达）。**推荐混合**：

```
长期身份锚点  = NodeId（Ed25519，Noise 握手强制，与 certhash 解耦，可经 DHT 无限重解析）
可失效 hint   = /ip4/../udp/../webrtc-direct/certhash/<h>  （进 addr hints，有则快拨）
兜底          = connect(NodeAddr::new(peer)) 先试 hint，失败落 OnlineRecordLookup 按 NodeId 重解析
```

**决定性优势：两块基建都已就位。** PairInvite 本就是这个形状——inviter = `NodeId`（`secret.node_id()`）+ addr hints，代码注释明写「地址只是提示，最终身份由握手强制」；presence 侧 `OnlineRecordLookup`（`online_key = namespaced(ONLINE_NS, NodeId)`）+ `PresenceSupervisor` 周期 announce + dial-by-NodeId 已实现（`presence/supervisor.rs`：dial_backoff + add_addrs + connect）。落地只差**把证书持久化接上**，消掉那条 warn。

**红线**：绝不据 certhash 做身份判定或授权——授权在 capability 哈希 + 握手 pin（`InviteRegistry` 一次性 CAS）。certhash 是内容寻址，天生会变，不能承担长期身份职责。

## D4：桌面端点是否也固定证书——速度 vs 隐私的可选项

helper/bootstrap **必须**固定证书（其 certhash 要么进客户端 bootstrap 配置、要么经 DHT 稳定广播）。桌面端点有取舍：

| | 桌面固定证书 | 桌面不固定（每次随机） |
|---|---|---|
| 首连速度 | ✅ hint 长期有效，直连快 | ⚠️ hint 重启即陈旧，多靠重解析 |
| 隐私 | ⚠️ 固定证书对 on-path 观察者可跨连接指纹（WebRTC TLS1.2 证书明文，spec SHOULD NOT 复用） | ✅ 无跨连接指纹 |
| 兜底 | 混合方案 NodeId 重解析照常 | 混合方案 NodeId 重解析照常（更依赖它） |

**推荐**：桌面**也固定**（一个用户主动想被自己配对设备连上的 P2P 传输工具，可达性优先于跨连接指纹；且指纹面仅对 on-path 观察者）。但架构上保留「桌面不固定、只靠 NodeId 重解析」为一行开关的降级项——在意隐私的用户/未来策略可切。因混合方案的兜底使两种选择都不致命。

## D5：helper 固定 certhash 的运维刚性 + 动态取降级

若把 helper/bootstrap 的 `/webrtc-direct/certhash` **硬编码**进客户端 bootstrap 配置当引导入口（非经 DHT 解析），则 helper 换证 = 硬编码失效 = 需随客户端发版更新。规避：
1. **优先长期不轮换**（D1 已证无过期墙，长期固定即可）；
2. 架构预留「经 identify/DHT 动态取 helper 当前 certhash」的降级路径——客户端 bootstrap 只认 helper 的 NodeId + IP，certhash 运行时从 identify/DHT 拿，把 helper 也纳入「NodeId 锚点 + 可刷新地址」模型。

## D6：兜底重解析链路 + announce 覆盖必须实证

混合方案的兜底只有在这两点成立时才有效，需**专门验证**（否则重解析只剩 TCP/QUIC/relay，web 端拨不进 webrtc-direct）：
1. **announce 覆盖**：webrtc-direct/certhash 地址确实进了 `shareable_addrs()` = `dialable()` = `direct_addrs()`，未被 loopback/unspecified/多跳 circuit 的过滤规则误杀。
2. **wasm 受邀端能重解析**：浏览器要么能查 DHT、要么经 helper 代解析，使「邀请 hint 失效 → 按 NodeId 拿当前 webrtc-direct 地址」在 web 端也成立。web 端 webrtc-direct/circuit 拨号已实测通（`net-kernel.md`），但**重解析路径未专门验**。

## D7：轮换策略 + 未决风险

- **默认不轮换**：无过期墙（D1），rcgen not_after≈4096，长期固定。仅私钥疑似泄露/密钥卫生策略时手动换证。换证 = 换 certhash，但**无需重发邀请**——presence 周期 announce 会把新 certhash 地址重新写进 DHT online 记录（NodeId 不变），受邀方按 NodeId 自动重解析。
- **否决重叠双证书滚动**（WebTransport 式）：webrtc-direct 传输在 rust/go-libp2p 均无现成滚动入口（要自搓生命周期管理），而 webrtc-direct 根本不校验 notAfter → 轮换的安全收益低、工程成本高，且任何烤进地址的 certhash 仍随轮换失效。收益不抵成本。
- **未决风险**（需冒烟/后续验证）：
  1. 14 天之争未 100% 闭环——建议一次「故意用长 not_after 固定证书跨 Chrome/Safari/Firefox 实拨」冒烟（当前只测过 Chrome），落到 `spike/net-web-smoke` 语义。
  2. Safari/Firefox + https 页面组合对自签证书 + certhash 的接受行为未验证。
  3. 私钥落盘无吊销；helper 被入侵可在传输层冒充该 certhash 端点，但冒充不了 NodeId（Noise 握手失败），损失限于「假装可达」。
  4. 重解析兜底依赖 DHT/relay 在线 + 对端近期 announce；对端长期离线且 hint 陈旧、DHT 无记录 → 拨不通（可用性非安全）。
