# WebRTC 数据面在 SCTP 之上静默丢数据：一个窗口配置反了的 bug

> **日期**：2026-08-11
> **触发**：native↔native 吞吐已收敛（12–23 MB/s），但 web↔native（0.36–0.96 MB/s）
> 与 web↔web（~5 MB/s）仍慢
> **决策状态**：**回环侧根因已定案并已修复**（分支 `fix/webrtc-sctp-window-overflow`，见 §9）。
> 它是否也是真机「慢」的根因**尚未验证**——判据见 §5，但注意 §9.2：
> 在修好日志可见性之前，那条 grep 在存量日志上**必然是空的**。
> **工具**：新增 `crates/net/examples/transport_throughput.rs`
> （另有一个失败的尝试，写完又删掉了——原因见 §6.1）

## 0. 一句话

**`SCTP_RECEIVE_BUFFER`（8 MiB）比它下游的 driver 事件队列（256 条 × 8 KiB = 2 MiB）
大 4 倍。** SCTP 因此从不反压对端，队列必然溢出，而 webrtc-rs 的 driver 在队列满时
**直接丢弃已经可靠送达的消息**——只打一行 ERROR，没有任何补救。

上层 libp2p 字节流中间因此少一段，永远等不齐。**改一行配置，回环吞吐从「传不完」变成
301 MiB/s。**

## 1. 三方 transport 对照（同机回环 · 同一 Endpoint 应用层 · 只换 transport）

先确立基线。64 MiB，`cargo run -p swarmdrop-net --release --example transport_throughput`：

| transport | 吞吐 |
|---|---|
| TCP + Noise + yamux | 906–1110 MiB/s |
| QUIC | 248–265 MiB/s |
| **WebRTC-direct（现状）** | **传不完 64 MiB** |

WebRTC 那格不是「慢」，是**失败**：600 s 超时，或者提前报「对端只收到 60,719,188 字节，
应为 67,108,864」——**丢了 6.4 MB**。

## 2. 机制

`webrtc-0.20.2/src/peer_connection/driver.rs:882`：

```rust
if let Err(err) = evt_tx.try_send(DataChannelEvent::OnMessage(dc_message)) {
    let err_msg = match err {
        TrySendError::Full(_) => "Full",
        …
    };
    error!("Failed to send DataChannelMessage to data channel {}: {}", channel_id, err_msg);
    // ← 就这样。消息被丢掉，SCTP 那边认为已经送达。
}
```

队列容量是内部常量、未导出：

```rust
pub(crate) const DATA_CHANNEL_EVENT_CHANNEL_CAPACITY: usize = 256;
```

于是链路上出现一个**无背压环节**：

```
对端 → SCTP（可靠有序，窗口 8 MiB）→ driver 事件队列（256 条，满了丢）→ PollDataChannel → libp2p framing
                                      ↑
                        SCTP 的反压止步于此，管不到下游
```

**队列能装的字节数 = 256 × `max_message_size` = 256 × 8 KiB = 2 MiB。**
而我们告诉 SCTP「你可以缓存 8 MiB」。SCTP 于是放心大胆地收满 8 MiB 并往下投——
超出 2 MiB 的部分必然被丢。

> 这个数字关系是硬的：**SCTP 接收窗口 > 256 × max_message_size ⇒ 必然丢数据**。
> 本仓当前是 8 MiB > 2 MiB，超了 4 倍。

## 3. 四组变量分离实验

改动两处，2×2 交叉。64 MiB，回环：

| 组 | `PollDataChannel::poll_read` | SCTP 窗口 | 丢弃次数 | WebRTC 吞吐 |
|---|---|---|---|---|
| A | 一次只取一条消息（**现状**） | 8 MiB（**现状**） | 1073 | **超时 >600 s** |
| B | 一次填满 buf | 8 MiB | 780 | **失败**（丢 6.4 MB） |
| C | 一次填满 buf | 1 MiB | **0** | **301 MiB/s** |
| D | 一次只取一条消息 | 1 MiB | **0** | **145 MiB/s** |

三条读数：

1. **SCTP 窗口是决定性变量**。降到 1 MiB 后丢弃归零、传输成功——与 `poll_read` 怎么写无关。
2. **光改消费端救不了**（B 组仍失败）。它只把丢弃从 1073 压到 780，因为根子是无背压。
3. **消费端批量取是叠加优化**：C vs D = 301 vs 145 MiB/s，**2.1 倍**。

`poll_read` 的那处低效值得单独说：它取到一条消息就 `return`，哪怕上层给的是 256 KiB 的
buf、而一条消息只有 8 KiB——**31/32 的容量空着**，每 8 KiB 都要走一遍完整 poll 链
（含一次 `Box::pin` 堆分配）。

## 4. 那条注释把因果写反了

`backend/native/mod.rs:53`：

```rust
/// SCTP receive window. The 1 MiB default drops the connection outright on a LAN (the
/// spike observed failures at 4 MiB).
const SCTP_RECEIVE_BUFFER: u32 = 8 * 1024 * 1024;
```

「在 4 MiB 上观察到失败，所以调到 8 MiB」——**方向反了**。窗口越大，SCTP 越不反压，
driver 队列溢出得越狠。当时若用的是 libp2p 默认的 16 KiB 消息，队列容量恰是
256 × 16 KiB = 4 MiB，**正好就是那个「观察到失败」的值**。

正确的判据不是拍一个数，而是**从下游队列容量推导**：

```
SCTP_RECEIVE_BUFFER ≤ DATA_CHANNEL_EVENT_CHANNEL_CAPACITY(256) × max_message_size
```

⚠️ 该常量是 webrtc-rs 的**内部**常量（`pub(crate)`，未导出）。写死 256 等于把上游的
实现细节钉进本仓，上游一改就静默失效——这一点必须在正式修复里处理（要么向上游申请导出，
要么留一条护栏测试 + 显著的注释）。

## 5. 它是不是真机「慢」的根因？—— 一条 grep 能定

**尚未验证，不要先当成结论。**

- **支持**：这个队列**只存在于 native 侧**（webrtc-rs 的 driver）；浏览器侧是
  `onmessage` 回调，没有这一环。而实测正好是
  **web↔web（无 native）~5 MB/s ≫ web↔native（有 native）0.36–0.96 MB/s**。
- **反对**：真机速率低（~120 条消息/秒），256 条的队列相当于 2 秒缓冲，消费端不太可能
  连续 2 秒不消费。丢弃真发生的话，表现应是**传输失败**而非变慢。

**判据**：在真机日志里搜

```
Failed to send DataChannelMessage to data channel
```

⚠️ **但存量日志里搜不到——不是因为没丢，是因为看不见。** 这里原本写的是
「`webrtc` 的 ERROR 不会被默认 filter 过滤掉」，**实测证伪**：`webrtc_p2p=info` 够不着
`webrtc::…`（前缀不匹配），生产 filter 下丢掉 10 MiB 而日志零条记录。§9.2 补了
`webrtc=warn,rtc=warn`，**装了带这条修复的版本之后**这条 grep 才有意义。

- 有 → 真机根因确认
- 没有 → 真机的「慢」是另一个原因（此时 §6.1 的桌面↔Web 判别仍然要做），
  但 §2 这个 bug **照样要修**——它是数据静默损坏，只是还没在真机速率下暴露。

## 6. 过程中被推翻的东西（含我自己的测量工具）

### 6.1 一个坏掉的基准，和它编出来的三个数字

第一版基准（`crates/webrtc-p2p/examples/throughput.rs`，**已删除**）直接拿
`libp2p_core::Transport` 手写 poll 循环，**测量装置自己成了主要误差源**：

| 它报的数 | 真相 |
|---|---|
| 8 KiB / 4 MiB → **104 MiB/s** | **伪影**：4 MiB ≤ `SEND_BUFFER_LIMIT`，整份数据塞进发送缓冲即返回，压根没过链路 |
| 16 MiB → 挂死 | **自锁**：把「驱动 transport」和「跑传输」绑在同一个 task，一背压就连 UDP 读循环一起停摆 |
| 修完自锁 → 14.56 MiB/s，但重跑又挂 | 仍有未定位的竞态 |

我曾据 104 MiB/s 写下「framing 层已排除」——**那条排除是错的**。
后来换成 `crates/net` 的 `Endpoint`（自带后台 actor 事件循环，调用方无从手动驱动）才拿到可信数字。

> **教训**：当测量装置的复杂度接近被测对象时，它就是主要误差源。
>
> **它已被删除**，而不是留着「仅供复现挂死」——它复现的那个挂死是**它自己的** bug，
> 没有诊断价值；而一个已知会给出错误数字的基准留在仓里，下一个人未必会先读这段。
> 教训记在这里就够了，`crates/net/examples/transport_throughput.rs` 覆盖了它的全部用途。

### 6.2 三个从源码推出来、都自洽、都不成立的假说

1. ~~framing 每消息强制 flush 造成停等~~ — 形状确实是停等，但那个「等」几乎不花钱
2. ~~`dc.send()` 是网络往返，吞吐 = 消息 / RTT~~ — 它只是本地入队 + 唤醒 driver
3. ~~在途窗口只有 8 KiB~~ — 真实窗口是 `SEND_BUFFER_LIMIT = 4 MiB` / wasm 侧 1 MiB

### 6.3 顺带发现：SDP 里的消息上限是硬编码的

`libp2p-webrtc-utils` 的 `sdp.rs::render_description`（fork `d858435`，PR #6560 引入）：

```rust
max_message_size: 16 * 1024,   // ← 硬编码，与 with_max_message_size() 无关
```

它渲染的是 SDP 的 `a=max-message-size`，也就是告诉对端 SCTP「我最大能收多大」。
于是 **`DirectConfig::with_max_message_size()` 只作用于 framing 层，对 SDP 协商完全无效**。

现在没炸只因为生产值是 8 KiB（发的比声明的小）。**一旦调到 16 KiB 以上就会发出超过对端
声明上限的消息**。这是一颗地雷，且与 §2 相互作用（调大消息 = 队列容量变大，但 SDP 不跟着变）。

顺带解释了 `b8746980 fix(net): negotiate 8 KiB WebRTC messages` 为什么是个**没有正文**的
*fix*；它留在 `transport.rs` 的理由（「不超出浏览器安全上限」）也是错的——浏览器上限是
256 KiB（Chrome/Safari）到 1 GiB（Firefox）。

## 7. 已排除的

| 项 | 依据 |
|---|---|
| relay 掐断 circuit | `max_circuit_bytes = u64::MAX`、`max_circuit_duration = 6 h` |
| OPFS 落盘 | 已是 `createWritable` positioned 直写；Worker + SyncAccessHandle 实测无增益 |
| rtc 0.20.0 → 0.20.2 含吞吐修复 | `rtc-sctp` 两版 `src/` 逐字节相同 |
| udp_mux 支路缓冲（`BRANCH_CAPACITY = 256`） | 三方对照那轮**零次**告警；它只在 direct 监听端生效 |

## 7.5 上游状态：已修，但**两个修复不在同一个可用版本上**

`webrtc` 的 master 与 `0.21.0-alpha.1` 都已修掉本文 §2 那个 bug，**修法与本文的推断逐字一致**
（`driver.rs:1001`）：

```rust
// overflow: retained — **this was webrtc#858**. A reliable ordered channel
// promises delivery; this used to discard it on a full queue. Now the
// message is kept and the driver stops pulling reads from the core until the
// consumer drains, so back-pressure reaches SCTP's receive window and the
// peer throttles.
```

他们还顺手做了一次系统性审计——每处队列都标了 `overflow: retained` / `overflow: DROPS` 与理由，
并写下约束：*「这个循环绝不能阻塞在 send 上……driver → application 通道的背压意味着
**停止从 core 拉取**，而不是**在此等待**」*（RTP 那条仍是 DROPS，因为 UDP 本就无流控——
与本文 §2 对「可靠层之上丢数据」和「不可靠层丢包」的区分一致）。

背压的传导链他们写在字段注释里，可以逐段核对：保留装不下的事件 → 有积压时**停止从 core
拉取** → 积压堆在 `pipeline_context.read_outs` → SCTP handler 看到后停止排空 reassembly
queue → `a_rwnd` 缩小 → 对端减速。也就是说，`SCTP_PIPELINE_READ_BACKLOG_LIMIT`（256 条）
接管了「限制队列占用」，而窗口回归它本来的含义。

> **一处更正。** 本节此前写着「修好根因后他们把 `with_sctp_receive_buffer_size` 整个移除了」，
> 并据此建议升级时删掉整项。**那是错的**：这个旋钮没有被移除，是**搬到了**
> `SettingEngine::set_sctp_max_receive_buffer_size`，还专门配了
> `tests/sctp_receive_buffer.rs`。它的文档也重写了，明确定义成**BDP 上限**
> （「对端可以有多少未确认数据在飞」），并给出 RFC 4960 §6 的 1500 字节下限。
> 所以升级后要删的是本仓那套**按 driver 队列容量压制窗口**的补偿逻辑，不是窗口本身——
> 窗口仍然要配，只是依据从「队列装得下多少条」换回「链路要多少在途字节」。

### 实测验证：#858 确实修好了

把窗口**故意**设成 8 MiB（0.20 下这是队列容量的 4 倍，必然溢出），回环传 128 MiB：

| | 结果 |
|---|---|
| **0.20.2** | ❌ 对端只收到 128,008,346 / 134,217,728 字节，**丢 5.9 MiB**，传输失败 |
| **0.21.0-alpha.1** | ✅ 两次都 0 丢完整收到 |

吞吐则**没有可区分的差异**——0.20.2 测得 76.8 / 89.7 / 200.8 MiB/s，0.21 测得
51.5 / 77.1 / 88.1 / 90.2 / 149.2 MiB/s，两组范围完全重叠。这个回环基准的方差远大于
版本间差距，**不要拿单次数字下结论**（我第一次就是拿 0.21 的 51 对 0.20 的 203，
差点写成「0.21 有严重性能回退」）。

### 迁移成本：6 处，全 workspace 编过、106 条测试全绿

1. `pem` feature 没了（PEM API 转正，不再门控）
2. `RTCCertificate::from_key_pair(rcgen::KeyPair)` → `generate(crypto, scheme, params)`
3. `from_pem` 多收一个 `&dyn RTCCrypto`
4. `serialize_pem()` 返回 `Result`（provider 可能持有不可导出的密钥，如 HSM）
5. `PeerConnectionBuilder::with_sctp_receive_buffer_size` → `SettingEngine::set_sctp_max_receive_buffer_size`
6. 连带：`crates/net::generate_webrtc_certificate_pem` 改 `and_then`

顺带**减两处依赖**：`rcgen` 直接依赖可以删（0.21 re-export 了 `CertificateParams`，
不必再维持「版本必须与 rtc 同线」那条约束）；crypto backend 从编译期 feature 变成运行时
`RTCCryptoProvider`。见分支 `chore/webrtc-0.21`。

### 但现在仍不能升——两个阻塞

**① crates.io 的 `0.21.0-alpha.1` 缺 rtc#159/#161。**

先纠正一个说法：这**不是「0.21 回退了修复」**，是**发布早于修复**。看合并时间——

| PR | 内容 | 合并 |
|---|---|---|
| #154 | bump to `v0.21.0-alpha.1` | 08-09 23:48 |
| #159 | ICE 终态不再上报过期 deadline | 08-10 23:38 |
| #161 | TURN 刷新从 `now` 重排 | 08-11 01:51 |

所以 crates.io 上那个快照落在两个修复之前。实证：`rtc-ice-0.21.0-alpha.1` 的
`agent_proto.rs:142` 是 `if self.ufrag_pwd.remote_credentials.is_some() {`，**终态守卫不在**，
两条护栏测试也不在；`rtc-turn` 那边同样是 `旧值.add(step)` 而非 `now.add(step)`。
少了它们就是
[`2026-08-11-webrtc-driver-busy-loop.md`](2026-08-11-webrtc-driver-busy-loop.md) 里
CPU 718%、actor 饿死、7 GB 传输卡死那一套。

**两个仓的 master 都已经对了**（webrtc master 的 submodule 指针就是 rtc 的 `b0ab7f4` = #161），
只等 alpha.2。

**② 直接 patch 到 git master 会撞类型分叉。**

`rtc` 是 webrtc 仓的 **git submodule**，webrtc 对它是 `path` 依赖。我们再 git 依赖一次 `rtc`
就成了两个 source、同名两套类型，实测报错：

```
expected `RTCDataChannelState`, found `rtc::data_channel::RTCDataChannelState`
expected trait `webrtc::peer_connection::rtc_crypto::RTCCrypto`, found `rtc::rtc_crypto::RTCCrypto`
```

上游其实已经认识到这件事，并在 `peer_connection/mod.rs` 写明「**`rtc` is a private dependency
of this crate**」，为 `CipherSuiteId` / `SrtpProtectionProfile` / `crypto` 做了 re-export。
但规则没走完——本仓要用的 5 个参数类型都不在：`MulticastDnsMode`、`NetworkType`、
`RTCDtlsRole`、`SctpMaxMessageSize`、`CertificateParams`，全是
`SettingEngineBuilder` 的 setter 参数或 `RTCCertificate::generate` 的参数。
（`UDPNetwork` / `InterfaceFilterFn` / `IpFilterFn` 也缺，但它们在 rtc master 上还是
`//TODO:` 注释掉的状态，不算。）

已向上游提 PR 补全，并附一条只用 `webrtc::` 路径、绝不 import `rtc` 的回归测试——
将来新增 setter 若带进未导出的类型，那条测试会编译失败。

那之后本仓还有两处要动：`rtc::stun`（`udp_mux` 解析入站 STUN 学 ufrag）改成直接依赖
`rtc-stun`——它的类型**不跨 API 边界**（只解析字节拿 ufrag 字符串，不传给任何 webrtc API），
两份并存也不会撞；`math_rand_alpha` 只用一处，自己写几行。

### 退出条件（可判定）

```console
# ① alpha.2 及以上，且 rtc-ice 含终态守卫
$ rg -A4 'let ice_timeout' rtc-ice-*/src/agent/agent_proto.rs   # 需含 ConnectionState::Failed | Closed
# ② 5 个参数类型可从 webrtc:: 直接命名
$ rg 'pub use rtc::ice::\{mdns::MulticastDnsMode' webrtc-*/src/peer_connection/mod.rs
```

届时切到 `chore/webrtc-0.21`，删掉 `sctp_receive_buffer` 的推导（连同
`DRIVER_EVENT_QUEUE_LEN` / `DRIVER_QUEUE_HEADROOM` 这两个镜像上游内部常量的坏味道），
按 BDP 重新选一个窗口值。

## 8. 进度

**已落地**（分支 `fix/webrtc-sctp-window-overflow`，详见 §9）：

- ✅ 窗口从消息尺寸推导，不再拍常量；§4 那条写反的注释已纠正，三条护栏测试看守
- ✅ `poll_read` 一次填满 buf（2.1×，且独立成立）
- ✅ 补 `rtc=warn`，让 §5 那类日志在生产里看得见（桌面 + 移动两份一起改）
- ✅ 逐 transport 吞吐基准（`crates/net/examples/transport_throughput.rs`）

**已查清、无需再做**：

- ~~向上游报 §2~~ ——[webrtc#858] 早在 08-09 就已 CLOSED 并修复，修法与本文推断逐字一致（§7.5）
- 0.21 升级已完整验证并留在 `chore/webrtc-0.21`，阻塞条件与退出判据见 §7.5

**待办**：

1. **真机回归**（唯一一条卡在本仓之外的）。注释里「1 MiB 在 LAN 上掉线」那条历史观察
   虽然归因错了，但失败现象本身可能真实存在，不能只凭回环定案。
2. **修 §6.3 的 SDP 硬编码**——`render_description` 里 `max_message_size: 16 * 1024` 是写死的，
   本仓 fork 参数化了却没接到 `StreamConfig`。属于自己的 PR #6560，半实现状态。
3. **上游 re-export PR** 合并后，按 §7.5 的退出条件切 0.21。

[webrtc#858]: https://github.com/webrtc-rs/webrtc/issues/858

## 9. 本轮落地的修复（分支 `fix/webrtc-sctp-window-overflow`）

### 9.1 窗口从消息尺寸推导，不再是拍出来的常量

`crates/webrtc-p2p/src/backend/native/mod.rs`：

```rust
const DRIVER_EVENT_QUEUE_LEN: u32 = 256;   // 镜像上游未导出的 pub(crate) 常量
const DRIVER_QUEUE_HEADROOM: u32 = 32;     // 队列还要装控制事件，见下
const SCTP_WINDOW_CEILING: u32 = 4 * 1024 * 1024;

fn sctp_receive_buffer(stream_config: StreamConfig) -> u32 {
    let max_message_size = stream_config.max_message_size() as u64;
    let slots = u64::from(DRIVER_EVENT_QUEUE_LEN.saturating_sub(DRIVER_QUEUE_HEADROOM));
    let derived = slots.saturating_mul(max_message_size);
    let ceiling = u64::from(SCTP_WINDOW_CEILING).max(max_message_size);
    derived.clamp(max_message_size, ceiling) as u32
}
```

两个模式各按自己的配置算：direct 用 `ctx.stream_config`（本仓 8 KiB ⇒ 1.75 MiB），
打洞用 `StreamConfig::default()`（16 KiB ⇒ 3.5 MiB）。

第一版没有 headroom、也没有 ceiling，是 code review 打回来的三条（都已复核成立）：

1. **队列不只装数据消息**。`OnOpen` / `OnClose` / `OnBufferedAmountLow` / `High` 等
   `try_send` 进**同一条**队列（`driver.rs:718-735`），而 `SEND_BUFFER_LIMIT` 会让
   buffered-amount 那对在双向传输中频繁触发。窗口取满 256 条数据消息 ⇒ 一个控制事件
   就挤掉一条数据。故留 32 槽。
2. **`as u32` 会在 clamp 之前就截断**，而 `saturating_mul` 饱和到 `u32::MAX` = 4 GiB 窗口
   ——恰好是这个函数要避免的最坏情况。改为全程 u64 + 显式上界。
   （`clamp(min, max)` 在 `min > max` 时 panic，所以 ceiling 要对单条消息取 max。）
3. **`max_message_size` 是本端声明值，不是协商值**。`build()` 跑在 Noise 握手**之前**，
   而窗口在 `PeerConnection` 建成后改不了。对端若声明更小，它发的消息更小、条数更多，
   窗口就相对偏大。本仓三端声明同一个值所以不受影响，但对第三方实现是个缺口
   ——**原先那条注释写的是「按协商出的尺寸推导」，是错的**，已订正。

护栏测试也重写过：原先两条是**同义反复**（拿函数体当期望值，任何实现都能过）。
现在一条断言结构性不变量（「满窗口的在途条数 ≤ 可用槽位」），一条钉死两个真实调用点的
**绝对值**，一条覆盖上下界与不 panic。负向验证过：把 headroom 改成 0，绝对值那条立刻红。

### 9.2 让它在生产里看得见

**`webrtc` 与 `webrtc_p2p` 是两个 target**，而 `"webrtc::…".starts_with("webrtc_p2p")` 为假。
两端的 `DEFAULT_FILTER` 补上 `webrtc=warn,rtc=warn`，各配一条护栏测试（负向验证过：
去掉后测试红，并直接说出后果）。

`rtc=warn` 是 review 追加的，而且**比 `webrtc` 那条更该早点加**：同一套栈的另一半
（`rtc_ice` / `rtc_sctp` / `rtc_dtls` / `rtc_turn`）同样不以 `webrtc` 开头——
2026-08-11 刚修的 driver 忙循环打在 `rtc_ice`，而「接收窗口 / 重组」这类问题打在
`rtc_sctp`，**正是本轮在追的那一类**。

> 这也修正了 §5 那条建议的一个前提错误：**在此之前，那条 grep 在存量日志上必然为空**
> ——不是因为没丢，是因为看不见。这与 2026-08-10 的 udp_mux 是同一个教训的第二次。

顺带修掉移动端那条护栏测试自身的缺陷：它写了 `for target in [...]`，但循环体里发的是
**固定的三条**日志（`tracing` 宏的 `target:` 必须是字面量，无法参数化），断言又只看
"probe" 在不在——于是「三个里有一个能过」就算通过。现在逐条断言。

### 9.3 消费端一次填满 buf（叠加的 2.1×）

`PollDataChannel::poll_read` 早先取到一条消息就返回，上层给 256 KiB 的 buf 只填 8 KiB。
改为循环填满；`Poll::Pending` 时若已搬到字节就先交出去（**`Ok(0)` 会被读成 EOF**，
只在一个字节都没有时才挂起）。

### 9.4 实测

同机回环、`crates/net/examples/transport_throughput.rs`，64 / 128 / 256 MiB 各测过：

| | 修复前 | 修复后 |
|---|---|---|
| 丢弃次数 | 780–1073 | **0**（三种体量都是 0） |
| WebRTC-direct | **传不完**（超时 / 丢 6.4 MiB） | 完成；128 MiB 实测 **203 MiB/s** |
| QUIC（对照） | 248–267 MiB/s | 261–267 MiB/s |
| TCP（对照） | 906–1110 MiB/s | 920–1063 MiB/s |

⚠️ **WebRTC 那格方差很大**（各轮：52 / 95 / 138 / 141 / 144 / 203 MiB/s）。回环基准本来
就抖，**不要拿它做窗口大小的细粒度调优**——窗口该由正确性（≤ 队列可用槽位）决定，不由
这个数。判别实验里那次 301 MiB/s 同样只是运气好的一次。

256 MiB 那轮另有 12 条 driver ERROR，**全部是关闭阶段的噪声**（连接已关而事件仍在途的
`Failed to get data_channel: N`、我们主动关闭 mux 支路、以及 ICE 往本机 Tailscale 网卡
发包的 `Can't assign requested address`）——没有一条是数据丢失。

### 9.5 基准工具自己也按 review 修了

它是这轮唯一的证据来源，所以它的可信度不能靠自觉：

- **资源清理统一收尾，且超时改在 `bench` 内部施加**——从外层 `timeout` 会把 future
  连同没跑到的清理代码一起 drop，把两个活着的 Endpoint 留给下一档。三档在同一进程里
  顺序跑，一档漏关就污染后面几档，**正是这个文件宣称要消除的那类误差**。
- `wait_addrs` 的 `expect("watch closed")` 改为返回 `Err`：endpoint 若在此刻收摊，
  panic 会带走整个进程，剩下的 transport 一个都测不成。
- 尺寸参数解析失败改为报错，不再静默回退到 64 MiB（`-- 256M` 这类手误会让表头印着
  「64 MiB」而实际也跑 64，数字随后被引用进文档）。
- 输出单位由 `MB/s` 更正为 `MiB/s`——差 4.9%，而这些数字与真机报告里的 MB/s 交叉比较过。

### 9.6 仍未做

- **真机回归**（必须）：旧注释「1 MiB 在 LAN 上掉线」的归因虽然错了，但那个失败现象
  本身可能真实存在，回环证明不了它不会发生。
- §6.3 的 SDP 硬编码（未动）。
- 向上游报本轮的量化数据。

## 附：复现

```console
$ cargo run -p swarmdrop-net --release --example transport_throughput -- 64
$ RUST_LOG=warn … 2>&1 | grep -c 'DataChannelMessage.*Full'
```

同机回环、native↔native。绝对值不可外推到真机（无丢包、RTT≈0、lo0 MTU 远大于 1500），
它的用途是**给数据面设上界、并暴露与网络无关的结构性缺陷**——本轮那个 bug 正是这么露出来的。
