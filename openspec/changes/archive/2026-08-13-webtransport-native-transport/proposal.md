## Why

浏览器↔native 这条链路目前只有 webrtc-direct 一条路，而它是三端里最慢的：同机回环、同一个
`Endpoint` 应用层、只换 transport 实测 TCP+Noise+yamux ~1100 MiB/s、QUIC ~270 MiB/s、
webrtc-direct ~80 MiB/s（方差 51–203）。差距不在加密（两条链路都走 `ring` 的 AES-GCM 汇编），
而在用户态栈的深度——QUIC 一层做完可靠传输 + 多路复用 + TLS，WebRTC 要 ICE + DTLS + SCTP
三层各遍历一次数据。**WebTransport 就是 QUIC**，上限对标 270 而不是 80。

拦路的是实现缺口而非可行性：`rust-libp2p` 只有 `transports/webtransport-websys`（浏览器侧拨号，
wasm），**没有 native listener**——浏览器能拨，没人能接。上游 [PR #4348](https://github.com/libp2p/rust-libp2p/pull/4348)
（mxinden 的 native 实现 draft）自 2023-10 停在 draft 至今。

时机上有一条 2026 年的新事实：WebTransport 已于 **2026-03 成为 Baseline**（Safari 26.4 补齐最后
一块），且 `serverCertificateHashes` 三家全通（Chrome 100+、Firefox 125 修好
[Bugzilla 1873263](https://bugzilla.mozilla.org/show_bug.cgi?id=1873263)、Safari 26.4）。
它不再是「只有 Chrome 能用」的方案，浏览器覆盖面与 webrtc-direct 打平。

## What Changes

- **新建 `crates/webtransport-p2p`**——实现 `libp2p_core::Transport` 的 native WebTransport
  监听与拨号。照抄 `crates/webrtc-p2p` 的定位：**不带 `swarmdrop` 前缀、零 swarmdrop 依赖**，
  长在本仓只为借 `crates/net` 做集成测试，稳定后 subtree split 独立发布。
- **证书生命周期子系统**——spec 强制自签名证书有效期 ≤ 14 天且禁 RSA，服务端滚动生成并
  同时通告当前与下一张证书的 hash。这是本 change 的架构重心（见下）。
- **Noise 认证 + `webtransport_certhashes` 扩展**——CONNECT 后第一条流跑 Noise，只做身份
  认证不做加密（QUIC-TLS 已加密）。扩展的编解码 `libp2p-noise` 已现成支持
  （`Config::with_webtransport_certhashes`，responder 上报 / initiator 验证两侧都有）。
- **接入 `crates/net`**——`transport.rs` 按 `/quic/webtransport` 前缀分派；native 侧新增
  transport，浏览器侧改用现成的 `libp2p-webtransport-websys`。
- **bootstrap 新增 UDP 4004 监听**——独占端口，**不动 webrtc-direct 的 4003**。两条浏览器
  入口并存，可灰度对比吞吐后再决定是否下线 webrtc-direct。

四条已定的决策（详见 design.md）：

| # | 决策 | 依据 |
|---|---|---|
| ① | 底层库用 `wtransport` 0.7.1 | 唯一同时满足自签名 ECDSA P-256 + `validity_days(14)`、`Certificate::hash()` 明确给 `serverCertificateHashes` 用、`SessionRequest::path()` 可拦路径、`Endpoint::reload_config` 明写「刷新 TLS 证书不断既有连接」 |
| ② | 轮换时钟由 `Transport::poll` 顺带推进，`advance(now)` 注入 | 少一个要管生命周期的 task；「晚换几分钟」在 14 天尺度上无害 |
| ③ | 实现 native 拨号 | 生产上大概率零用例，但没有它 native↔native 集成测试写不了，CI 覆盖为零 |
| ④ | 独占 UDP 4004 | 不动存量部署与客户端清单，符合「不要动 webrtc-direct」 |

**不做**：不复用 libp2p-quic 的 UDP socket（`wtransport` 不暴露底层 quinn `Endpoint`，且这
正是 PR #4348 卡住的地方）；不做 `Backend` 抽象（只有 native 一个实现，浏览器侧用现成 crate）；
不新增应用层加密。

## Capabilities

### New Capabilities

- `webtransport-transport`: native WebTransport transport 的行为契约——multiaddr 形态与解析、
  监听与通告地址、HTTP endpoint 路径拦截、Noise 认证与归属校验、WebTransport 流到 libp2p
  子流的映射、拨号语义与失败形态。
- `webtransport-certificate-lifecycle`: 证书生命周期——两张证书的生成与有效期约束、轮换判据
  与时机、通告 certhash 集合的构成、`webtransport_certhashes` Noise 扩展的上报与验证、
  持久化端口与加载时的过期处理、轮换如何表达为 transport 的地址事件。

### Modified Capabilities

无。现有 spec 的需求不变——本 change 新增一条传输路径，不改动传输域、配对、收件箱或
节点状态的既有行为。bootstrap 增加监听端口属部署配置，不构成 `bootstrap-node-settings`
的需求变更。

## Impact

**新增**

- `crates/webtransport-p2p/`（新 crate，进根 Cargo workspace）
- 依赖 `wtransport` 0.7.1（crates.io，底层 quinn 0.11 + rustls 0.23，与本仓既有 QUIC 栈同源）

**修改**

- `crates/net/src/transport.rs` — native 分支接入新 transport；wasm 分支接入
  `libp2p-webtransport-websys`；`supported_transports` 增加变体（那份清单与组装代码同文件，
  漏改会静默拒掉合法地址）
- `crates/net/Cargo.toml` — 按 target 分派两个依赖
- `crates/bootstrap` — 新增 4004 监听与地址通告
- `docs/app/app/_lib/relay-helpers.ts` — 浏览器侧 bootstrap 清单增加 WebTransport 地址
- `src-tauri/src/logging.rs` 与移动端对应常量 — `DEFAULT_FILTER` 放行
  `webtransport_p2p` / `wtransport` / `quinn`（`EnvFilter` 按字符串前缀匹配，互不为前缀，
  漏一条那层日志生产里一条都不出现——本仓已因此吃过三次亏）
- 宿主侧新增 `CertificateStore` 实现（桌面写 `app_local_data_dir`，bootstrap 写配置目录）

**风险与已知负债**

- **通告地址的有效期是 28 天而非 14 天**（spec 要求同时通告 current + next，客户端持旧地址
  仍可在下一轮匹配上 next）。这仍然意味着 bootstrap 的客户端清单需要周期性更新——
  本 change **不解决**这个问题，靠「webrtc-direct 继续作为第一联系点」绕开。
- Rust CI 只跑 ubuntu，Windows / macOS 的编译问题要到打 tag 才暴露。
- 回环基准方差极大，单次数字不可比，吞吐对比至少取 6 次中位数。
