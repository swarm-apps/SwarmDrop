# 任务：给 `crates/webrtc-p2p` 补上 webrtc-direct，替换官方 `libp2p-webrtc`

> **这是给新会话的启动提示词。** 直接把本文件路径丢给 Claude Code 即可开工。
>
> 前置阅读（按顺序，别跳）：
> 1. `CLAUDE.md` — 架构唯一事实源
> 2. `dev-notes/knowledge/net-kernel.md` — 网络内核知识库，**「WebRTC 打洞传输接线」整节必读**
> 3. `dev-notes/research/2026-07-webrtc-native-ice.md` — 为什么自研、spike 实测数据、已完成到哪一步
>
> 开工前先调 `/dev-workflow`（项目硬性要求）。

---

## 一句话目标

让 `crates/webrtc-p2p` 同时提供 **打洞（已完成）** 和 **webrtc-direct（本次任务）** 两种模式，
从而**完全替代官方 `libp2p-webrtc`**，把 native 依赖树里的两套 WebRTC 栈合并成一套。

## 为什么值得做

当前 native 侧**同时编译两套完整的 WebRTC 实现**——已实测坐实：

```
webrtc v0.20.0-rc.4   ← crates/webrtc-p2p（我们的打洞传输）
webrtc v0.17.2        ← 官方 libp2p-webrtc（webrtc-direct），钉死 "0.17"
  ├── webrtc-ice v0.17.2
  └── webrtc-sctp v0.17.2
```

两套 ICE / DTLS / SCTP / SRTP 同时进二进制。补上 direct 后可以把官方那份整个摘掉。

**收益不只是瘦身**：`webrtc-p2p` 会成为官方 `libp2p-webrtc` 的完整替代品——rust-libp2p
生态里目前没有第二个能同时覆盖两种模式的 WebRTC 传输，这是这个 crate 独立发布时最硬的
卖点（决策理由见 research 文档「决策与理由」一节：**能力建设 + 通用组件价值**，不是投入产出比）。

## 两种模式的差异（这是任务的核心）

| | 打洞（`/webrtc`，**已完成**） | webrtc-direct（`/webrtc-direct`，**本次**） |
|---|---|---|
| 场景 | 双方都不可 listen（浏览器 / NAT 后） | 一端有可达地址（公网 / 同网段） |
| ICE | 完整 ICE agent，双向收集候选 | **ICE-lite**：服务端只被动应答，不收集候选 |
| 信令 | `/webrtc-signaling/0.0.1` 经 relay | **无**——SDP 由 multiaddr 确定性构造 |
| 地址 | `<relay>/p2p-circuit/webrtc/p2p/<target>` | `/ip4/…/udp/…/webrtc-direct/certhash/<hash>/p2p/<id>` |
| 证书指纹 | 经**已认证**的 relay 连接交换 | 写在 multiaddr 里（certhash），**不可信** |
| 身份认证 | DTLS 指纹绑定即可，无需额外握手 | **必须再跑一次 Noise 握手** |
| 是否 listen | 否 | **是**（服务端要绑 UDP 端口） |

⚠️ **最容易踩的一条**：direct 模式的 certhash 可能经不可信信道传播，所以**连上之后必须跑
Noise**（`libp2p-webrtc-utils::noise`）。打洞模式不需要，因为 SDP 走的是已认证的 relay 连接。
这条在 spec 的 FAQ 第一条写死了，别为了"复用"把两条路径的认证逻辑合并。

## 可复用的东西（先看这些，别重写）

**`libp2p-webrtc-utils`（crates.io 0.5.0，已在依赖里）** —— 泛型、不依赖 webrtc-rs，
所以复用它**不会**把 0.17 拖回来：

| 文件 | 用途 | 打洞模式用了吗 |
|---|---|---|
| `noise.rs` (235 行) | direct 模式的 Noise 握手 | ❌ **本次要用** |
| `fingerprint.rs` (110 行) | certhash ↔ SDP fingerprint 互转 | ❌ 本次要用 |
| `sdp.rs` (160 行) | SDP 模板渲染 | ❌ 本次要用 |
| `stream.rs` + `stream/*` (1000+ 行) | DataChannel 之上的 libp2p framing | ✅ 已在用 |

**官方 `libp2p-webrtc` 的 native 实现**（`transports/webrtc/src/tokio/`，2385 行）是最好的
参照物，但**它基于 webrtc 0.17，API 与 0.20 有大改**（`PeerConnection` 从具体类型变 trait、
事件从闭包回调改 `PeerConnectionEventHandler`、DataChannel 变 `poll()` 事件流）。按文件读：

| 文件 | 行数 | 说明 |
|---|---|---|
| `udp_mux.rs` | 579 | **最难的一块**：多路复用同一个 UDP 端口给多个 PeerConnection，按 ufrag 分流 |
| `transport.rs` | 568 | Transport trait + listener |
| `connection.rs` | 309 | muxer |
| `upgrade.rs` | 265 | Noise 握手 + 连接升级 |
| `sdp.rs` | 144 | 确定性 SDP 构造 |
| `certificate.rs` | 113 | 证书生成 / PEM 序列化 / certhash 派生 |

**先确认 webrtc 0.20 是否已自带 UDP mux**——0.20 的重构里可能已经提供，果真如此的话
`udp_mux.rs` 那 579 行就不用重写了。这是开工前第一件要查的事。

## 现有代码的结构（必须沿用）

```
crates/webrtc-p2p/src/
├── lib.rs           门面：new(config, factory) → (Transport, Behaviour)
├── config.rs        Config（stun_servers / signaling_timeout / udp_bind_addrs）
├── protocol/        线上格式，零 libp2p-swarm 依赖
│   ├── message.rs   protobuf 编解码（信令用，direct 不需要）
│   ├── codec.rs     asynchronous-codec 适配
│   └── addr.rs      /webrtc multiaddr 解析 ← **direct 要在这里加 /webrtc-direct**
├── backend/         WebRTC 栈抽象；两个 target 各自特化
│   ├── mod.rs       Backend trait + BackendEvent + Factory
│   ├── native/      webrtc-rs 0.20
│   └── wasm/        浏览器 RTCPeerConnection
└── swarm/           接到 Transport / NetworkBehaviour 两个平面
    ├── session.rs   信令状态机（纯逻辑，可脱离真 WebRTC 测试）
    ├── handler.rs   ConnectionHandler poll 适配
    ├── behaviour.rs
    └── transport.rs
```

**依赖方向单向**：`swarm → backend → protocol`，下层不反向引用上层。**别破坏它。**

### 架构决策：direct 该放在哪一层

direct **不需要 behaviour**（无信令），所以它只涉及 `transport` + `backend` 两层。建议：

- `protocol/addr.rs` 加 `/webrtc-direct` 的解析与构造（与现有 `is_webrtc` 并列）
- `backend/` 加 direct 所需的能力（ICE-lite 配置、确定性 SDP、证书）
- `swarm/transport.rs` 的 `dial` / `listen_on` 按地址段分派到两条路径

**分派点是唯一的架构风险**：一个 Transport 同时处理两种地址，要保证
`/webrtc` 与 `/webrtc-direct` 互不误认（现有的 `rejects_foreign_addrs` 测试已经在防这个，
扩展它）。

## 硬约束（违反了会被 CI 拦或将来堵死路）

1. **`crates/webrtc-p2p` 不依赖任何 swarmdrop crate**。它要 subtree split 出去独立发布，
   任何反向依赖都会堵死这条路。评审时优先看这一条。
2. **双 target 必须都编过**：`./scripts/check-wasm.sh --clippy`。webrtc-rs 编不到 wasm，
   浏览器侧只能用 `RTCPeerConnection`——direct 的浏览器侧只做 **dialer**（浏览器不能 listen）。
3. **不带 swarmdrop 前缀的命名**，不引入 swarmdrop 特有概念。
4. **注册顺序**：本传输产出的 transport 必须排在任何"按前缀吞地址"的 transport 之前
   （详见 `crates/net/src/transport.rs` 的 `webrtc_and_relay` 文档——**这条踩过两次**）。

## 从上一轮接线学到的坑（省你几个小时）

这些都是**实测踩出来的**，写在 `dev-notes/knowledge/net-kernel.md`，这里只列标题：

1. **`with_relay_client` 会把 relay 排到 `or_transport` 最前** —— 用 `with_other_transport`
   永远抢不过它。症状极隐蔽：一切正常，只是你的 transport 一次都没被调用过。
2. **circuit 地址会被按前缀误吞** —— WebSocket 曾这样吞掉 `/p2p-circuit` 地址（已移除 ws，
   但引入新 transport 时要重新审）。
3. **`Transport::listen_on` 必须唤醒 poll** —— 它是外部同步调用，往 pending 塞事件时没有
   任何东西会唤醒 poll。
4. **wasm 的 DataChannel `Connecting` 不是错误** —— PeerConnection 的 `connected`（DTLS 完成）
   早于 DataChannel 的 `open`（SCTP 完成），要注册 waker 等 `onopen`。
5. **浏览器排障先收紧 tracing filter** —— Web 端曾是全局 DEBUG，libp2p 的日志把 console
   行数上限冲爆，自己的日志一条都看不到，**因此连续两轮下了错误结论**。

### webrtc-rs 0.20 的三个不能省的设置（spike 实测）

```rust
PeerConnectionBuilder::new()
    // 默认 1 MiB，LAN 高带宽下几百毫秒撑爆 → 连接直接断（不是降速，是断）
    .with_sctp_receive_buffer_size(8 * 1024 * 1024)
    // 默认无界，快生产者可撑爆内存；但 < 4 MiB 会把管道饿着，吞吐腰斩
    .with_data_channel_send_buffer_limit(4 * 1024 * 1024)
```

外加 **`with_udp_addrs` 必须传具体网卡 IP**：传 `0.0.0.0` 时 webrtc-rs 不展开网卡，会把
字面量写进 host candidate，对端无法使用 → host 路径整条作废（实测吞吐从 50 MiB/s 掉到
0.6 MiB/s）。现有代码用 `if-addrs` 枚举，direct 侧同理。

## 实施顺序（建议）

1. **先查 webrtc 0.20 是否自带 UDP mux** —— 决定要不要重写那 579 行，直接影响工作量估算
2. `protocol/addr.rs` 加 `/webrtc-direct` 解析 + 单测（纯逻辑，最快见效）
3. 证书：生成 / PEM 持久化 / certhash 派生 —— 对齐官方 `certificate.rs`，
   **certhash 必须与官方算法一致**，否则存量地址全失效
4. native listener：绑 UDP + ICE-lite + 确定性 SDP + Noise 握手
5. native dialer
6. wasm dialer（浏览器拨 certhash 地址）
7. `swarm/transport.rs` 的地址分派 + 扩展 `rejects_foreign_addrs` 测试
8. **替换验收**：把 `crates/net` 里的官方 `libp2p_webrtc` 换成本 crate，跑通后
   从 `crates/net/Cargo.toml` 删掉 `libp2p-webrtc` 依赖，确认依赖树只剩一个 webrtc 版本

## 验收标准

**功能**（缺一不可）：

- [ ] 浏览器 → 桌面 webrtc-direct 直连跑通（替换官方实现后）
- [ ] certhash 与官方算法一致——**存量客户端的地址仍能拨通**
- [ ] 打洞模式不回归（`cargo test -p webrtc-p2p` + 浏览器实测）
- [ ] 与官方 `libp2p-webrtc` 互通（过渡期两版并存时）

**工程**：

- [ ] `cargo test --workspace` 全绿
- [ ] `./scripts/check-wasm.sh --clippy` 过
- [ ] `cargo tree` 里**只剩一个 webrtc 版本**（这是本任务的量化目标）
- [ ] `crates/webrtc-p2p` 仍不依赖任何 swarmdrop crate

**实测**（本地能做的）：

- [ ] `cd docs && pnpm build:wasm && pnpm dev`，浏览器连桌面 LanHelper
- [ ] 桌面端 `pnpm tauri dev`，用 tauri MCP 查 `get_network_status` 的 listen 地址

## 参考资料

- spec：<https://github.com/libp2p/specs/blob/master/webrtc/webrtc-direct.md>
- 官方实现：`~/.cargo/git/checkouts/rust-libp2p-*/989cb61/transports/webrtc/src/tokio/`
- 共享层：`~/.cargo/git/checkouts/rust-libp2p-*/989cb61/misc/webrtc-utils/src/`
- webrtc-rs 0.20：<https://github.com/webrtc-rs/webrtc>（0.17 → 0.20 是大重构，别照抄旧 API）
- 本项目的决策与实测数据：`dev-notes/research/2026-07-webrtc-native-ice.md`

## 一句话交代现状

打洞模式**已端到端跑通**（web↔web 打洞 + 在直连上完成配对），三端默认开启，已随
v0.9.0 / mobile-v0.9.0 / bootstrap-v0.7.0 发布。**跨 NAT 打洞与 js-libp2p 互通两项验收
仍未做**（前者需要两台不同网络的机器）——那是另一条线，与本任务并行不冲突。
