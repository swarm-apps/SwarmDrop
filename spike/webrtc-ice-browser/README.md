# spike：webrtc-rs 0.20 ↔ 浏览器 ICE

**结论：地基成立。** webrtc-rs `0.20.0-rc.4` 具备与浏览器 `RTCPeerConnection` 完成
**完整 ICE 协商**的能力，这正是「自研 libp2p WebRTC transport，让浏览器直连 NAT 后原生端」
方案所缺的那一半。但过载时的失败模式很差，需要应用层限流兜底。

日期：2026-07-27 · 环境：macOS 25.5 / Chrome（agent-browser）/ 单机双端

---

## 为什么做这个 spike

libp2p 现有的 `webrtc-direct` 是 **ICE-lite**：服务端只被动应答、不收集候选、SDP 确定性
构造，因此**打不了洞**——浏览器只能拨已经可达的地址（同局域网，或对端有公网 IP）。
跨网时浏览器 ↔ NAT 后设备只能走 relay 中转。

要打破它，需要**真 ICE 协商**。上游 PR [#5978] 做了浏览器↔浏览器，但 `transports/webrtc`
（native 侧）**一行没改**，不解决我们的场景。自己做之前必须先确认：**webrtc-rs 到底能不能
当一个完整 ICE agent**。

[#5978]: https://github.com/libp2p/rust-libp2p/pull/5978

## 跑法

```bash
# SPIKE_BIND 必须显式给本机可路由 IP，理由见下面「坑 1」
SPIKE_BIND=192.168.x.x,127.0.0.1 cargo run --release
# 浏览器打开 http://127.0.0.1:8099，依次点 ①②③④
```

## 实测结果

### ✅ 问题 1：能不能通

通。浏览器 offer → webrtc-rs answer → ICE connected → DataChannel open → 双向消息。
信令走本机 HTTP（vanilla ICE，等 gathering 完成一次性交换），刻意不接 libp2p。

### ✅ 问题 2：是不是完整 ICE agent（最关键）

**是。** webrtc-rs 主动向 STUN 发了绑定请求并产出 `typ srflx`：

```
a=candidate:799680034 1 udp 1694498815 120.235.173.63 18443 typ srflx raddr 192.168.50.105 rport 51836
```

ICE-lite 实现永远不会产出 srflx——这一条就是判据。**打洞所需的能力在库里是现成的。**

额外确认：webrtc-rs **能解析 Chrome 的 mDNS host candidate**（`xxx.local`）。Chrome 出于
隐私把本地 IP 换成 `.local` 名字，若解析不了，局域网 host 路径会整条作废。实测选中路径为
`host ↔ host`、RTT 1ms，说明解析成功。

### ✅ 问题 3：性能够不够

够。host 路径实测，拥塞窗口仍在爬升：

| 累计 | 耗时 | 瞬时速率 |
|---|---|---|
| 1 MiB | 47 ms | 21.5 MiB/s |
| 2 MiB | 67 ms | 29.8 MiB/s |
| 3 MiB | 84 ms | 35.5 MiB/s |

> **别被 srflx 路径的数字骗了。** 同一份代码，走 srflx（NAT hairpin）只有 **0.6 MiB/s**、
> RTT 9ms；走 host 是 **35+ MiB/s**、RTT 1ms。测吞吐前务必先用 `getStats()` 确认选中路径，
> 否则量到的是 NAT 绕行的代价，不是链路能力。

### ✅ 问题 4：过载断连能不能治（已验证：能，一个配置项）

初测时全速灌 8 MiB，连接会在 3～4 MiB 处 `disconnected` → `failed`。**根因是 SCTP 接收
窗口默认只有 1 MiB**（`with_sctp_receive_buffer_size` 的文档默认值），LAN 低 RTT 高带宽
下几百毫秒就撑爆。

同一份代码、同一个脚本，唯一变量是 `SPIKE_RWND`：

| SCTP 接收窗口 | 结果 |
|---|---|
| 默认（1 MiB） | ❌ 4 MiB 处 `disconnected` → `failed` |
| **8 MiB** | ✅ 8 MiB 完整 / 160 ms / **50.1 MiB/s**（全程爬升） |

⇒ **风险解除**，而且吞吐还从 35 MiB/s 提到 50 MiB/s。

> 上游文档只说「调**小**它可以省内存」（面向多连接 SFU 场景），没提**默认值在高带宽
> 低 RTT 下会直接断连**。这个失败模式值得反馈给上游——断连而非降速，对应用层很难诊断。

### ✅ 问题 5：发送侧背压（PR #817）好不好用

好用，但 **limit 不能设太小**。Rust → 浏览器下发 8 MiB：

| `send_buffer_limit` | Rust 侧耗时 | 浏览器实收 | 发送侧内存 |
|---|---|---|---|
| 无界（默认） | **4 ms**（假象） | 7.4 MiB/s | 8 MiB 全堆在内存 |
| **4 MiB** | 155 ms | **7.4 MiB/s** | 封顶 4 MiB |
| 1 MiB | 2216 ms | **3.6 MiB/s** | 封顶 1 MiB，**吞吐腰斩** |

三点读法：

1. **无界时 `send()` 4ms「发完」8 MiB 是假象** —— 数据只是进了发送缓冲。这正是 PR #817
   要治的 OOM 面：快生产者 + 慢对端 = 内存无限涨。
2. **设了 limit，`send()` 变成真阻塞背压**：1 MiB 那组两侧计时几乎逐毫秒对齐
   （2216 ms vs 2213 ms），说明发送端被准确地按链路速度节流了。
3. **limit 要覆盖 BDP + 接收端处理抖动**。4 MiB 时吞吐与无界持平（7.4 MiB/s）而内存封顶，
   是好折中；1 MiB 就把管道饿着了。

> 下行 7.4 MiB/s 远低于上行的 50 MiB/s，瓶颈在**浏览器侧的 onmessage 回调处理**，
> 不在 Rust。做真实传输时这个不对称要考虑进去。

### ⚠️ 坑 1：`with_udp_addrs(["0.0.0.0:0"])` 不枚举网卡

webrtc-rs 会把字面量 `0.0.0.0` 原样写进 host candidate：

```
a=candidate:291967603 1 udp 2130706431 0.0.0.0 64012 typ host      ← 对端无法使用
```

于是 host 路径整条作废，只能退到 srflx 走 NAT hairpin（就是上面那个 0.6 MiB/s）。
**必须显式传本机可路由 IP。**

这个坑在 libp2p-webrtc 的 direct 模式下不存在（ICE-lite + 确定性 SDP 不需要枚举网卡），
**切完整 ICE 后才会暴露**。集成时要自己做网卡枚举。

### 坑 2（测试基建）：连接断了别在 `bufferedAmount` 上死等

浏览器侧 `while (dc.bufferedAmount > HIGH)` 若不检查 `readyState`/`connectionState`，
连接 failed 后会锁死主线程——页面完全点不动，自动化工具也没法恢复。已在 `index.html`
里加了 `alive()` 检查。

## 0.20 的 API 变化（集成成本参考）

相对 0.17（libp2p-webrtc 当前所用）是**大重构**，不是升版本号的事：

| | 0.17 | 0.20 |
|---|---|---|
| 事件 | 闭包回调 `on_data_channel(Box::new(...))` | `PeerConnectionEventHandler` trait |
| DataChannel | 具体类型 | `Arc<dyn DataChannel>` + `poll()` 事件流 |
| PeerConnection | 具体类型 | **trait**（`Arc<dyn PeerConnection>`） |
| 运行时 | 硬绑 tokio | 自带 `webrtc::runtime` 抽象层 |
| 构造 | `APIBuilder` | `PeerConnectionBuilder` + `RTCConfigurationBuilder` |

最后一行的 runtime 抽象值得注意：它让「非 tokio 运行时」在理论上成为可能（wasm 仍不行——
webrtc-rs 自带 UDP/SCTP/DTLS 栈，浏览器侧只能用原生 `RTCPeerConnection`）。

## 集成时必须显式设置的两个参数

默认值都不适合文件传输，且失败模式都不直观：

```rust
PeerConnectionBuilder::new()
    // 默认 1 MiB 会在 LAN 高带宽下几百毫秒内断连（不是降速，是断）
    .with_sctp_receive_buffer_size(8 * 1024 * 1024)
    // 默认无界 = 快生产者可把发送缓冲撑爆；≥4 MiB 才不损吞吐
    .with_data_channel_send_buffer_limit(4 * 1024 * 1024)
```

## 下一步

1. ~~验背压~~ —— **已完成**，见问题 4 / 5。两个风险都可由配置解决。
2. **验真打洞** —— 本 spike 两端同机，srflx 走的是 NAT hairpin，不构成打洞证据。
   真结论需要两台不同网络的机器（至少一台在 NAT 后），验证 srflx ↔ srflx 能否
   在**跨 NAT** 时建立。这是方案成立与否的最后一道硬判据。
3. **再谈 libp2p 集成** —— 第 2 步过了，才值得设计 `/webrtc-signaling/0.0.1` 的
   transport + behaviour 配对。

## 清理

按仓库惯例，spike 是临时探针：结论并入 `dev-notes/knowledge/`，目录删除，快照留
`archive/spike` 分支。
