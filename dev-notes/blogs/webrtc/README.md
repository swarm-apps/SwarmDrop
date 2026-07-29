# WebRTC：从零理解，到给上游提六个补丁

> 一个「无账号、无服务器」的文件传输工具，要让浏览器参与进来，就只有 WebRTC 一条路。
> 这个系列从**完全不懂 WebRTC** 讲起，一路讲到为什么我们不得不自研传输层、
> 并向 `rtc` / `webrtc` / `rust-libp2p` 三个上游提了 11 个 PR。

## 为什么会有这个系列

浏览器给 JS 的网络能力只有三样：`fetch`、`WebSocket`、`RTCPeerConnection`。前两样都必须
连服务器，**只有第三样能让两台设备直接交换字节**。SwarmDrop 的 Web 端因此从第一行代码起
就绑在 WebRTC 上。

代价是：官方封装帮你挡掉的坑，全部要自己踩。这个系列就是那些坑的完整记录——
以及把它们变成上游补丁的过程。

## 篇目

| # | 篇 | 讲什么 |
|---|---|---|
| 00 | [WebRTC 到底是什么](00-what-is-webrtc.md) | **零基础入门。** ICE / DTLS / SCTP / SDP 四层拆解，每层解决什么问题、怎么静默失败 |
| 01 | [libp2p 的两种 WebRTC](01-libp2p-webrtc-direct.md) | 打洞与 direct：信令怎么送、certhash 为什么必须持久化、为什么 direct **必须**再跑一次 Noise |
| 02 | [一个有 setter 没 reader 的开关](02-dtls-fingerprint-dead-switch.md) | `rtc#137`：指纹校验开关是死代码；正反双向测试；一次让我发现自己有语义 bug 的评审 |
| 03 | [`send` 返回 `Ok(())`，数据却蒸发了](03-datachannel-silent-send.md) | `rtc#138`：**最贵的一个坑**。检查条件与决定成败的条件不是同一个，错误被 `warn!` 掉 |
| 04 | [每条流的首条消息都丢](04-datachannel-ordered-default.md) | `rtc#140`：`#[derive(Default)]` 让 `ordered` 成了 `false`，用户数据超车 DCEP 控制消息 |
| 05 | [这条通道是谁开的](05-who-opened-this-channel.md) | `webrtc#825`：`on_data_channel` 把本端开的通道也回报，而且交出去的是死句柄 |
| 06 | [0.20 把远端证书弄丢了](06-remote-fingerprint-via-stats.md) | `webrtc#828`：Noise prologue 需要对端指纹，而 API 没了；一次关于 W3C API 边界的争论 |
| 07 | [怎么把踩的坑变成上游补丁](07-upstream-methodology.md) | 方法论收束：何时该提、一个 PR 一件事、测试判据怎么选、怎么接评审、fork pin 怎么治理 |

**建议读法**

- 完全不懂 WebRTC：**00 → 01** 就够了，它们是独立可读的入门
- 想看踩坑复盘：01 打底，然后 02～06 任选（每篇自带前情）
- 只关心怎么和上游打交道：直接读 **07**

## 六个坑的共同形状

| 篇 | 症状 | 根因 | 为什么难查 |
|---|---|---|---|
| 02 | direct 服务端起不来，`ErrNoMatchingCertificateFingerprint` | 开关有 setter 无 reader，是死代码 | 无法从外部区分「开关无效」和「还有别的问题」 |
| 03 | Noise 首包写出去就消失，全链路零报错 | `send` 检查「通道已注册」，真正条件在 `handle_write` 且只 `warn!` | **`Ok(())` 是骗人的**，挂起点与出错点差一步 |
| 04 | 每条流首包丢，multistream-select 永不完成 | `ordered` derive 成 `false`，用户数据超车 DCEP | 日志说「未知 PPID 53」，把人引向对端 |
| 05 | muxer 收到一条本端自己开的「入站流」 | driver 把所有 `OnOpen` 都当对端开的回报 | 拿到的句柄是死的，等事件永远等不到 |
| 06 | Noise prologue 算不出来 | 0.17 的 `get_remote_certificate` 在 0.20 没了 | 替代路径的两步查找**写反了不报错** |

看出规律了：**六个里有五个是静默失败**。没有一个能被 `cargo check`、类型系统或常规单测
拦住，因为它们全都发生在「编译期看不见、运行时不报错」的那一层。

这也是为什么每篇都要花大量篇幅讲**怎么定位**——在这类问题上，定位比修复难十倍。

## 相关材料

- **代码**：`crates/webrtc-p2p/`（自研传输，刻意不依赖任何 swarmdrop crate，将来要独立发布）
- **依赖治理**：`Cargo.toml` 的 `[patch.crates-io]` 段落——每条 pin 的原因与**可判定的退出条件**
- **决策背景**：[`dev-notes/research/2026-07-webrtc-native-ice.md`](../../research/2026-07-webrtc-native-ice.md)
- **姊妹系列**：
  - [`browser-platform/`](../browser-platform/) —— 浏览器平台侧的约束（能不能 listen、
    secure context、mixed content），与本系列同题不同层
  - [`wasm-debugging/`](../wasm-debugging/) —— 同样是「全绿却不工作」的静默 bug 复盘，
    但那边是 wasm 运行时语义，这边是 WebRTC 协议栈
  - [`network/`](../network/) —— 三端连通性与公网 relay 部署
