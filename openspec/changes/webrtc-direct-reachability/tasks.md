# webrtc-direct-reachability 任务分解

## Phase 1 — 证书持久化打通（混合策略核心）

- [ ] `NetworkRuntimeConfig`（core `network/config.rs`）增 `webrtc_cert_pem`（或证书来源）字段，透传到 `build_endpoint` → `builder.webrtc_certificate(pem)`
- [ ] 桌面：首启 `Certificate::generate` → `serialize_pem()` 存 Stronghold/keychain；后续 `from_pem()` 复用
- [ ] helper/bootstrap：证书 PEM 持久化到服务器数据目录（权限 600），复用
- [ ] 消掉 `transport.rs:54-61` 的 `certhash will not survive restarts` warn（有持久化证书时）
- [ ] 回归：native 端跨重启 certhash 稳定（同一 PEM → 同一 certhash 的断言测试）

## Phase 2 — listen /webrtc-direct

- [ ] 桌面端点 listen 地址集加 `/ip4/0.0.0.0/udp/0/webrtc-direct`（+ ipv6）
- [ ] helper/bootstrap listen /webrtc-direct（证书固定 → certhash 固定）
- [ ] 实证 listen 生效：本机拨自身 webrtc-direct 地址成功

## Phase 3 — 分享物锚点分离（NodeId 锚点 + certhash hint）

- [ ] 确认 webrtc-direct/certhash 地址进 `shareable_addrs()` = `dialable()` = `direct_addrs()`，未被 loopback/unspecified/circuit 过滤误杀（D6 覆盖验证）
- [ ] invite addr hints 携带 webrtc-direct 地址（复用 `pair-invite-protocol`，inviter=NodeId+hints 已就位）
- [ ] 拨号链路：`connect(NodeAddr::new(peer))` 先试 hint、失败回落 `OnlineRecordLookup` 按 NodeId 重解析（`presence/supervisor.rs` 已实现，验证 webrtc-direct 地址纳入）
- [ ] 断言：certhash 变更后（换证）按 NodeId 重解析自动拿到新地址，无需重发邀请

## Phase 4 — 双路径实拨 + wasm 兜底

- [ ] 浏览器 ↔ helper：web 经 webrtc-direct 连自托管 helper，跨网可达
- [ ] 浏览器 ↔ 桌面端点：桌面 invite 携带自己的 webrtc-direct 地址，浏览器端到端直连
- [ ] wasm 受邀端重解析兜底：hint 失效 → 按 NodeId 拿当前 webrtc-direct 地址（浏览器查 DHT 或经 helper 代解析）——D6 未验路径，专门验
- [ ] helper certhash 降级路径预留：客户端认 helper NodeId+IP，certhash 运行时经 identify/DHT 动态取（避免换证=全客户端发版）

## Phase 5 — 跨浏览器冒烟 + 收尾

- [ ] 长 not_after 固定证书跨 Chrome / Safari / Firefox + https 实拨冒烟（14 天之争闭环，落 `spike/net-web-smoke`）
- [ ] `cargo test --workspace` + 六 crate wasm 门禁 + `wasm-pack test --headless --chrome -p swarmdrop-web`
- [ ] 知识库：`libp2p-wasm.md`（证书策略结论 + 14 天辟谣）、`net-kernel.md`（webrtc-direct listen 地址 + 证书持久化装配点 + Safari/Firefox 实测补齐）
