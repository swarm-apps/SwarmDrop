# 任务：为本仓实现 native 侧的 libp2p WebTransport

> **状态**：未开工，方案未定稿。本文件是**启动提示词**，不是设计文档。
> **写于** 2026-08-11，随 v0.17.0 发布后的吞吐调研产出。

## 一句话目标

给 `rust-libp2p` 补一个 **native（服务端）WebTransport transport**，让浏览器能用
WebTransport 直连本仓的桌面端与 bootstrap 节点——目标是吃到 QUIC 的数据面性能，
替换掉浏览器↔native 这条链路上的 WebRTC-direct。

**先读完「§2 先回答这三个问题」再写代码。** 其中第一个没有答案的话，这件事做出来也用不了。

## 1. 为什么值得做

同机回环、同一个 `Endpoint` 应用层、只换 transport（`crates/net/examples/transport_throughput.rs`，
128 MiB）：

| transport | 吞吐 |
|---|---|
| TCP + Noise + yamux | ~1100 MiB/s |
| **QUIC** | **~270 MiB/s** |
| WebRTC-direct | ~80 MiB/s（方差极大，51–203） |

WebTransport **就是 QUIC**（over HTTP/3），所以上限对标的是那个 270，不是 80。

差距的来源不是加密（两条链路都走 `ring` 的 AES-GCM 汇编，2026-08-11 查证，CLAUDE.md
里那条「WebRTC 走纯 Rust AES」的旧归因已订正），而是**用户态栈的深度**：QUIC 一层就把
可靠传输 + 多路复用 + TLS 做完，WebRTC 要 ICE + DTLS + SCTP 三层各遍历一次数据。

⚠️ **这个数字不能直接外推到真机**。回环瓶颈是 CPU，跨网瓶颈通常是带宽与 RTT。
动手前**先做真机测量**（见 §7 待办①），否则可能花大力气优化一个真机上不存在的瓶颈。

## 2. 先回答这三个问题

### ① 证书 14 天轮换，通告地址怎么办？（**最大的架构风险**）

libp2p WebTransport spec 强制：自签名证书**有效期 ≤ 14 天**、**禁 RSA**，服务端要滚动
生成并同时通告当前与下一张证书的 hash。

**这与 webrtc-direct 本质不同。** 本仓的 webrtc-direct 证书是**持久**的（PEM 存盘复用，
`crates/webrtc-p2p/src/backend/native/direct/certificate.rs` 的模块文档解释了为什么必须
持久），于是 certhash 可以写死在客户端清单里：

```ts
// docs/app/app/_lib/relay-helpers.ts:8
"/ip4/47.115.172.218/udp/4003/webrtc-direct/certhash/uEiBuBPte…/p2p/12D3KooW…"
```

**WebTransport 下这行每 14 天就会失效。** 而 bootstrap 是「第一个联系点」，客户端拿不到
新地址就连不上——鸡生蛋。

三个方向，**选定之前不要写传输层代码**：

1. **bootstrap 入口保持 webrtc-direct，WebTransport 只做已配对设备间的数据面。**
   最实际：地址稳定性问题不存在，而吞吐收益恰好落在真正传大文件的那条链路上。
   代价是两套传输并存。
2. **bootstrap 用 CA 签名证书**（spec 明说：有 CA 证书时**不要**加 `/certhash`）。
   需要域名 + 证书自动续期，`47.115.172.218` 这个裸 IP 用不了。
3. 客户端启动时从某处拉取最新地址。**大概率是伪解法**——那个「某处」自己就成了新的
   第一联系点。

### ② native 侧用哪个 crate？

`rust-libp2p` **只有 `transports/webtransport-websys`（浏览器侧拨号，wasm）**，
没有 native listener。也就是浏览器能拨、没人能接——这正是要补的缺口。

crates.io 上的候选（2026-08-11 查）：

| crate | 版本 | 最近更新 | 备注 |
|---|---|---|---|
| `web-transport-quinn` | 0.12.0 | 2026-08-07 | kixelated/web-transport-rs，基于 quinn，最活跃 |
| `wtransport` | 0.7.1 | 2026-04-26 | 独立实现 |
| `h3-webtransport` | 0.1.2 | 2025-05-06 | h3 生态，最久未更新 |

**选型前必须验证三件事**（都没验过，别信任何一个的 README）：

- server 侧能不能用**自签名 ECDSA 证书**起监听，并把实际使用的证书 DER 取出来算 certhash；
- 能不能把 HTTP endpoint 固定到 spec 要求的 `/.well-known/libp2p-webtransport`
  （带 `?type=noise` 参数）；
- 能不能在**证书轮换**时不断开既有连接。

### ③ 单独 crate 还是进 `crates/net`？

**单独 crate**（用户 2026-08-11 的意向，也与本仓既有形态一致）。照抄
`crates/webrtc-p2p` 的定位：

- **不带 `swarmdrop` 前缀、不依赖任何 swarmdrop crate**——它长在本仓只是为了借
  `crates/net` 做集成测试，稳定后要 subtree split 出去独立发布。任何反向依赖都会把这条路
  堵死（`crates/webrtc-p2p/Cargo.toml` 顶部那段注释写了同一条，评审时优先看它）。
- 实现 `libp2p_core::Transport`，由 `crates/net/src/transport.rs` 按 multiaddr 前缀分派。

名字建议 `webtransport-p2p`，与 `webrtc-p2p` 同构。

## 3. 前置阅读

**必读**：

- `dev-notes/knowledge/net-kernel.md` — 尤其 webrtc-direct 那几节。**新传输会重蹈的坑
  几乎都在里面**：udp_mux 的 GRO 拆包与错误判据、`Transport::poll` 驱动读循环的代价、
  日志 target 前缀匹配、SCTP 窗口与 framing 的两个丢数据 bug。
- `crates/webrtc-p2p/src/backend/native/direct/` — **最好的模板**。同样是「自签名证书 +
  certhash 进 multiaddr + 之后跑 Noise 认证」的形态，`upgrade.rs` 顶部那张表和
  `certificate.rs` 的模块文档直接对应 WebTransport 要做的事。
- [libp2p WebTransport spec](https://github.com/libp2p/specs/blob/master/webtransport/README.md)
  — 短，读全文。
- `dev-notes/research/2026-08-11-web-webrtc-throughput.md` — 吞吐数据的来源与方法论，
  尤其 §6.1「测量装置自己成了主要误差源」。

**按需**：`dev-notes/knowledge/libp2p-wasm.md`（浏览器侧的平台门：mixed content、
Chrome LNA）、`crates/net/src/transport.rs`（transport 分派与地址构造）。

## 4. Spec 的硬约束（照抄，别凭记忆）

1. **地址格式**：`/ip4/…/udp/…/quic/webtransport/certhash/<hash>`（可多个 certhash）。
   CA 签名证书时**不加** `/certhash`。
2. **HTTP endpoint 固定**：`/.well-known/libp2p-webtransport`，且 `?type=noise`。
3. **证书**：有效期 ≤ 14 天，**禁 RSA**（用 ECDSA P-256——与 webrtc-direct 同款理由：
   浏览器实现的事实标准）。首启生成两张（第二张从第一张过期日开始），过期后切换并再
   生成一张，同时更新通告地址。
4. **仍然必须跑 Noise**：certhash 只证明「证书没被换」，证明不了「对面是那个 PeerId」。
   客户端在 CONNECT 之后开的**第一条流**用于 Noise 握手，不等服务端响应就可以开始。
5. **`webtransport_certhashes` Noise 扩展**：服务端必须在握手里带上当前 + 所有已通告的
   未来证书的 hash（近期过期的也建议带），客户端逐一验证。

   ✅ **`libp2p-noise` 已经支持**，两个方向都有：
   `NoiseConfig::with_webtransport_certhashes(HashSet<Multihash<64>>)`——responder 上报、
   initiator 验证。**最难的一块是现成的**，别自己实现。

## 5. 本仓的硬约束

- `crates/net` 之外的 crate **不许依赖 swarmdrop 任何 crate**（见 §2③）。
- **wasm 双 target 门禁**：新 crate 若被 `crates/net` 依赖，就必须能编到
  `wasm32-unknown-unknown`（浏览器侧走 `webtransport-websys`，native 实现要
  `cfg(not(target_family = "wasm"))` 门控）。`./scripts/check-wasm.sh --clippy` 是硬失败。
- **日志 target 要单独放行**。`EnvFilter` 按**字符串前缀**匹配：`webtransport_p2p`、
  `web_transport_quinn`、`quinn`、`h3` 互不为前缀，漏一条那层日志在生产里**一条都不出现**。
  本仓已经因此吃过三次亏（udp_mux 丢包、SCTP 丢消息、framing 合并），
  桌面与移动是**两份独立常量**，要一起改。
- **不新增应用层加密**（QUIC-TLS 已经加密；且会与 bao-tree 逐块验签冲突）。
- 走 `/dev-workflow`：门禁 → `/simplify` → `/code-review` 三道关。

## 6. 已知坑（来自 webrtc-direct 那一轮，大概率重演）

- **别把 transport 驱动和数据传输绑在同一个 task 上**——会自锁挂死。
  `crates/net/examples/transport_throughput.rs` 顶部注释记了这个教训（上一版基准就是这么
  死的，删掉重写了）。
- **基准要建在 `Endpoint` 上，不要手写 `libp2p_core::Transport` 的 poll 循环**：
  测量装置的复杂度一接近被测对象，它就成了主要误差源。曾把「数据全塞进发送缓冲」读成
  104 MB/s。
- **回环基准方差极大**（WebRTC-direct 实测 51–203 MiB/s）。**单次数字不可比**，至少取
  6 次中位数；我曾差点据此写出「0.21 有 30% 性能回退」的错误结论。
- **负向验证**：新写的护栏测试必须**红过一次**才算有效。本轮有条测试断言写对了但复现
  不出目标行为，改坏实现照样绿。另见 `toolchain.md` 里 `sed -i.bak` 骗过增量编译那条。

## 7. 建议顺序

**① 先做真机测量（不写代码）。** 当前唯一没分离的变量是「打洞 vs webrtc-direct」——
真机上那条 0.36–0.96 MB/s 走的是打洞路径，而回环测的是 direct。如果真机瓶颈根本不在
CPU，那么整件事的收益预期要重估。判据与方法见 research 报告 §5。

**② 定 §2① 的地址方案。** 没定之前不要写传输层。

**③ 最小可用路径**：native listener + 浏览器拨号跑通一条 echo，证明
「自签名证书 + certhash + Noise 认证」这条链闭合。此时**不要**接进 `crates/net`。

**④ 接入**：`crates/net/src/transport.rs` 加地址分派，bootstrap 加监听，
`docs/app/app` 侧改用 `webtransport-websys` 拨号。

**⑤ 证书轮换**：单独一步，因为它牵动通告地址与 Noise 扩展里的 hash 列表。

## 8. 验收标准

- [ ] 浏览器（Chrome）能用自签名证书拨通 native listener，Noise 握手完成，PeerId 正确
- [ ] `webtransport_certhashes` 扩展被验证：**故意给错 hash 时握手必须失败**
      （只测成功路径等于没测）
- [ ] 三方吞吐基准里加一档 WebTransport，与 QUIC / WebRTC-direct 同图对比，各取 6 次中位数
- [ ] 证书跨过期切换后，既有连接不断、新连接用新证书、通告地址已更新
- [ ] `./scripts/check-wasm.sh --clippy` 绿
- [ ] 日志 target 已进两端的 `DEFAULT_FILTER`，且有测试断言它们能过
      （照抄 `src-tauri/src/logging.rs` 的 `default_filter_passes_the_targets_we_depend_on`）

## 9. 不做什么

- **不要用 WebRTC 的媒体通道（RTP/SRTP）传文件**。已评估否决：RTP 不保证送达，
  GCC 拥塞控制为低延迟设计、网络一紧就主动丢帧，方向与文件传输相反；要在上面做可靠
  传输等于重新实现一遍 SCTP，而且底座更不合适。
- **不要为了提速去调大 WebRTC 的消息尺寸**。已实测：8 KiB → 16 KiB 吞吐**没有提升**
  （中位 88 → 50 MiB/s），per-message 开销不是瓶颈。这条路走到底了。
- **不要动 webrtc-direct**。在 WebTransport 真的跑通并验证收益之前，它是浏览器唯一的
  可用入口。
