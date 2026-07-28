# webrtc-direct-reachability 任务分解

## Phase 1 — 证书持久化打通（混合策略核心）

- [x] 宿主 Keychain 凭据读取后透传到 core `build_endpoint` → `builder.webrtc_certificate(pem)`（不得放入前端可序列化的 `NetworkRuntimeConfig`）
- [x] 桌面与移动：首启 `Certificate::generate` → `serialize_pem()` 分别存系统 Keychain / SecureStore；后续 `from_pem()` 复用
- [x] helper/bootstrap：证书 PEM 持久化到服务器数据目录（权限 600），复用
      —— `crates/bootstrap/src/util/identity.rs` 的 `load_or_generate_webrtc_certificate`，
      `write_private_file` 落 `0o600`
- [x] 有持久化证书时不再触发 `transport.rs:54-61` 的 `certhash will not survive restarts` warn
- [x] 回归：native 端跨重启 certhash 稳定（同一 PEM → 同一 certhash 的断言测试）

## Phase 2 — listen /webrtc-direct

- [x] 桌面与移动端点 listen 地址集加 `/ip4/0.0.0.0/udp/0/webrtc-direct`
- [x] helper/bootstrap listen /webrtc-direct（证书固定 → certhash 固定）
      —— `crates/bootstrap/src/lib.rs`（4003 端口 + `webrtc_direct_addr_from_pem` 登记外部地址），
      测试 `external_addresses_include_webrtc_certhash`
- [x] 实证 listen 生效：本机拨自身 webrtc-direct 地址成功
      —— `crates/net/tests/webrtc.rs::dial_own_webrtc_direct_listen_addr`：服务端只监听
      webrtc-direct，拨号方只拿这一个地址，连上后跑一次子流字节往返。
      **这条测试撞出了子流无序通道的静默丢包**（见 net-kernel.md）

## Phase 3 — 分享物锚点分离（NodeId 锚点 + certhash hint）

- [x] 确认 webrtc-direct/certhash 地址进 `shareable_addrs()` = `dialable()` = `direct_addrs()`，未被 loopback/unspecified/circuit 过滤误杀（D6 覆盖验证）
- [x] invite addr hints 携带 webrtc-direct 地址（复用 `pair-invite-protocol`，inviter=NodeId+hints 已就位）
      —— `PairingManager::encode_invite` 直接吃 `shareable_addrs()` = `dialable()`（listen ∪ external，
      无按传输段过滤）；`TransportPolicy::LocalOnly` 的 `is_private_lan()` 只看 IP 段、
      不看传输段，故不会误杀 webrtc-direct 地址
- [ ] 拨号链路：`connect(NodeAddr::new(peer))` 先试 hint、失败回落 `OnlineRecordLookup` 按 NodeId 重解析（`presence/supervisor.rs` 已实现，验证 webrtc-direct 地址纳入）
- [ ] 断言：certhash 变更后（换证）按 NodeId 重解析自动拿到新地址，无需重发邀请

## Phase 4 — 双路径实拨 + wasm 兜底

- [x] 浏览器 ↔ helper：web 经 webrtc-direct 连自托管 helper，跨网可达
      —— 2026-07-28 实拨：`浏览器出站连接就绪 remote=47.115.172.218:4003 peer=12D3KooWCkaj…`，
      并在其上拿到 circuit reservation
- [x] 浏览器 ↔ 桌面端点：桌面 invite 携带自己的 webrtc-direct 地址，浏览器端到端直连
      —— 2026-07-28 实拨全链路：桌面 `generate_pair_invite` → 浏览器消费 →
      `浏览器出站连接就绪 remote=127.0.0.1:56442 peer=12D3KooWRkj1…`（桌面 peer id 对上）→
      `/swarmdrop/pairing/2` 配对成功 → 桌面发 256 KB → 浏览器 OPFS 落盘，
      SHA-256 与源文件逐字节一致；桌面侧日志 `already has a non-relayed path` 确认非中继。
      ⚠️ 两端同机，命中的是 127.0.0.1 那条 hint——**跨机 LAN 仍未单独验**
- [ ] wasm 受邀端重解析兜底：hint 失效 → 按 NodeId 拿当前 webrtc-direct 地址（浏览器查 DHT 或经 helper 代解析）——D6 未验路径，专门验
- [ ] helper certhash 降级路径预留：客户端认 helper NodeId+IP，certhash 运行时经 identify/DHT 动态取（避免换证=全客户端发版）

## Phase 5 — 跨浏览器冒烟 + 收尾

- [ ] 长 not_after 固定证书跨 Chrome / Safari / Firefox + https 实拨冒烟（14 天之争闭环，落 `spike/net-web-smoke`）
- [ ] `cargo test --workspace` + 六 crate wasm 门禁 + `wasm-pack test --headless --chrome -p swarmdrop-web`
- [ ] 知识库：`libp2p-wasm.md`（证书策略结论 + 14 天辟谣）、`net-kernel.md`（webrtc-direct listen 地址 + 证书持久化装配点 + Safari/Firefox 实测补齐）
