# WebRTC driver 忙循环：一条上游 ICE bug 如何把整个桌面端拖垮

> **日期**：2026-08-11
> **数据**：桌面 mac / Windows 同场日志两份（已 gitignore）、活动监视器截图、
> `sample(1)` 线程栈快照（`swarmdrop` pid 91689，5 秒）
> **决策状态**：**根因已确认到上游源码行，已修复**（`[patch.crates-io]` → rtc `v0.20.x`）。
> 上游 issue/PR 均已存在且合并，只是未发布到 crates.io。
> **方法**：日志时间线定位 → `sample` 抓热点栈 → 逐层读上游源码闭合因果链。
> 结论不含推测环节。

## 0. 一句话结论

| | 结论 | 证据强度 |
|---|---|---|
| **根因** | 上游 `rtc-ice` 在 ICE agent 进入 `Failed` 后，`poll_timeout()` 持续返回**一个永远落在过去的截止时刻**；sans-IO 的 `PeerConnectionDriver::event_loop` 对已过期 deadline 走 `handle_timeout` + `continue`，**那条 `continue` 跳过了整个 `select!`** → 100% 占满一个核，**且再也读不到命令通道** | **确定**。上游 [rtc#158](https://github.com/webrtc-rs/rtc/issues/158) 独立复现并已修（[PR #159](https://github.com/webrtc-rs/rtc/pull/159)），描述与本仓 sample 逐条吻合 |
| **触发** | 浏览器（Web 端）经 webrtc-direct 连入桌面端，随后页面关闭 / 刷新 / 网络切换 | **高**。用户观察「没连 Web 端时 CPU 低，连上后升高」，与 `UdpMux`/direct 是浏览器唯一入口一致 |
| **放大** | 每条这样死掉的连接**永久**留下一个烧核的 task，按次累积 | **高**。实测 718%（≈7 核）对应约 7 条死连接；抓 sample 时只剩 1 条，恰为 100.9% |
| **连带** | 「7GB 传输中途卡死 + mac 日志整片消失 4 分钟」与「web→native 仅 500 KB/s」是同一根因的**后果**，不是独立缺陷 | **高**（日志时间线闭合），吞吐那条**待复测确认** |
| **推翻** | `managed.rs` 记的 2026-08-06 那次 948% CPU，归因为「没人调 `close()`」——**不完整**。真因是同一条 ICE bug，而 `close()` 在该状态下**根本送不进去** | **确定**（见 §3） |

## 1. 现场

三个看起来无关的现象，实为一因：

1. **7GB 传输中途卡死**，发送端（mac）CPU 飙升，整机失去响应
2. **空闲时 CPU 也高**——「没连上 Web 端时占用比较低，连上后就升高」
3. **web→native 仅 500 KB/s，而 web→web 有 4 MB/s**

### 1.1 日志时间线（mac = 发送端，Windows = 接收端）

```
01:33:42  send total blocks=21487 ack=656223ms/99%   ← 第一程 665 秒，99% 在等 ACK
01:33:42  transfer-data 发送中断: …（读取帧长度）: connection lost
01:33:53  接收方发起探测式恢复                          ← 续传成功
01:34:13  send window blocks=2560 … read=861ms/42%
          ────────────── 此后 4 分钟，mac 日志一条都没有 ──────────────
01:38:14  文件日志已装载                                ← 用户强杀重启
```

两点要澄清：

- **「读取帧长度」不是帧格式错误**，是读长度前缀时 `connection lost`。它是结果不是原因，文案容易误导。
- **`read` 阶段耗时从 222ms 单调涨到 861ms** 一度像是 O(offset) 的读放大。**不是**——
  桌面端 `read_at_sync` 是 `open + seek(SeekFrom::Start) + read`，O(1)。那是**被抢 CPU 的
  症状**。

真正的异常是那 4 分钟的**彻底静默**：连 `swarmdrop_net::actor` 的 relay 日志都停了。这说明
不是某个传输任务卡住，而是 **actor 事件循环本身得不到调度**。

### 1.2 `sample` 定位

活动监视器：`swarmdrop` **718.6% CPU / 26 线程**，系统态仅 3.66%、用户态 96.34%
——纯计算自旋，排除 swap 抖动。

抓 sample 时进程已重启，只剩一条死连接（100.9%）。26 个线程里 **只有一个 tokio worker 在跑**，
其余全在 `__psynch_cvwait` / `kevent`：

```
419  9150384: tokio-rt-worker   rtc::peer_connection::handler::…
     └─ webrtc::peer_connection::driver::PeerConnectionDriver::event_loop
        ├─ 724  handle_timeout   ← DtlsHandler::handle_timeout → HashMap::get_mut + hash + malloc/free
        ├─ 599  poll_timeout     ← SCTP 285 / DTLS 253，大量 Instant::now() → mach_absolute_time
        ├─ 582  handler / data_channel
        └─ 292  mach_absolute_time
```

DTLS 的 `handle_timeout` 是最贵的一环，但它**是被空跑的**——热点全在 `HashMap::get_mut`
和 hash，没有进到握手逻辑。真正不推进的定时器在别处。

## 2. 因果链（逐层源码验证）

### ① `rtc-ice`：`contact()` 的早返回不更新时间戳

`rtc-ice-0.20.0/src/agent/mod.rs`：

```rust
fn contact(&mut self, now: Instant) {
    self.force_candidate_contact = false;

    if self.connection_state == ConnectionState::Failed {
        self.last_connection_state = self.connection_state;
        return;                       // ← :716  早返回，不更新 last_checking_time
    }
    // …（Checking 超时那条同样早返回）
    self.contact_candidates();
    self.last_connection_state = self.connection_state;
    self.last_checking_time = now;    // ← :743  唯一的更新点
}
```

`last_checking_time` **只在函数最后一行**更新。agent 一旦 `Failed`，它就永久冻结在过去。

### ② `poll_timeout()` 照旧拿它派生 deadline

`agent_proto.rs:140`（修复前）：

```rust
let ice_timeout = if self.ufrag_pwd.remote_credentials.is_some() {
    if self.force_candidate_contact {
        Some(self.last_checking_time)                              // 立即
    } else {
        Some(self.last_checking_time + self.get_timeout_interval()) // ← 永远在过去
    }
} else { None };
```

上游此前修过**同一失效模式的另一半**（`force_candidate_contact` 那条，注释明写
「so a failed/settled agent does not keep asking `poll_timeout` for an immediate wake-up」），
但漏了 `last_checking_time` 这条路径。

### ③ driver 对已过期 deadline 的处理跳过了 `select!`

`webrtc-0.20.0/src/peer_connection/driver.rs:321`：

```rust
let timeout = self.poll_timeout().await;           // 三者取 min：core / stun / turn
let now = Instant::now();
let delay_from_now = timeout.checked_duration_since(now).unwrap_or_default();

if delay_from_now.is_zero() {
    self.handle_timeout(now).await?;
    continue;                                       // ← 跳过下面整个 select!
}

let timer = self.inner.runtime.sleep(delay_from_now);
futures::select! {
    _ = timer.fuse() => { … }
    evt = driver_event_rx.recv().fuse() => { …  is_closed → return Ok(()) }   // ← close 在这
    udp_recv_result = udp_recv_future => { … }                                 // ← 收包也在这
    …
}
```

于是形成**自锁**：

1. 100% 占满一个核；
2. **命令通道再也读不到** —— `close()` 永远送不进去，driver 永不退出；
3. UDP 收包分支同样在 `select!` 里，这条连接的数据面也一并瘫掉。

## 3. 为什么 `ManagedPeerConnection` 守卫没兜住

`crates/webrtc-p2p/src/backend/native/managed.rs` 的模块文档记着 2026-08-06 那次事故
（948% CPU，热点同样是 `PeerConnectionDriver::event_loop` / `mach_absolute_time`），
当时的结论是「泄漏有两条来源：握手失败的早退点、muxer 被直接 drop」，修法是用守卫保证
**一定有人调 `close()`**：

```rust
impl Drop for ManagedPeerConnection {
    fn drop(&mut self) {
        let pc = self.pc.clone();
        self.runtime.spawn(Box::pin(async move { pc.close().await }));  // ← 永远完不成
    }
}
```

**那个归因不完整。** 守卫解决的是「谁来调 close」，而这条 bug 的要害是「**调了也没用**」——
`close()` 要经 `driver_event_rx` 送达，而 driver 已经不看那个通道了。所以：

> 只要 driver 可能陷入不读命令的循环，任何**依赖它协作**的关闭机制都不成立。

这是本次最值得留下的一条教训，已回写进 `managed.rs`。

## 4. 修复

上游 [PR #159](https://github.com/webrtc-rs/rtc/pull/159)「Stop scheduling ICE checks in
terminal states」已合并（2026-08-10 23:38），修法是终态直接不上报 deadline：

```rust
let ice_timeout = if self.ufrag_pwd.remote_credentials.is_some()
    && !matches!(self.connection_state,
                 ConnectionState::Failed | ConnectionState::Closed) { … }
```

**但它晚于 crates.io 上所有已发布版本**（`0.20.1` 与 `0.21.0-alpha.1` 都发布于 08-09，
PR 合并于 08-10），所以只能先从 git 取。

最终方案：`[patch.crates-io]` 把 **整个 rtc workspace** 指向 `v0.20.x` 分支的
`1ab0b083`（版本号 `0.20.1`，同时含 #159 与 #161）。三个决策点：

| 决策 | 理由 |
|---|---|
| **patch，而不是把 `rtc` 改成 git 依赖** | 真正跑 ICE 的 driver 在 **`webrtc` crate 内部**，它从 crates.io 拉自己那份 `rtc-ice`。改我们直接声明的依赖管不到它的传递依赖，只有 `[patch.crates-io]` 作用于整棵树 |
| **patch 整个 workspace，不只 `rtc-ice`** | 只换一个时，git 版 rtc-ice 经仓内 path 依赖拉进自己那份 rtc-shared / rtc-stun / rtc-mdns，而 crates.io 版的 rtc / rtc-dtls / rtc-sctp 仍用旧的 → 同名类型分叉。`cargo update` 实测把 4 个 crate 各解析出两份 |
| **指 `v0.20.x`，不指 master** | master 已是 `0.21.0-alpha.1` 且做过 API 大重构（`SettingEngine::set_lite` / `set_answering_dtls_role` / `set_ice_credentials` 全移除，`RTCCertificate` 改为注入 `RTCCrypto`）——实测本仓 **11 处编译错，全在 direct 模式最核心处**。而 v0.20.x 版本号仍是 `0.20.1`，满足 `^0.20.0`，**API 零变更** |

> ⚠️ 版本号必须满足依赖树里的约束，否则 patch 会被 cargo **静默忽略**（只有一句
> `patch for rtc-ice was not used`）。曾一度为此把 webrtc/rtc 升到 `0.21.0-alpha.1`，
> 发现 v0.20.x 分支已含修复后回退——**没有升 alpha 的必要**。

退出条件（可判定）：crates.io 的 rtc 发布 ≥ `0.20.2` → 删掉 patch 段、版本号提上去。

`cargo check --workspace --all-targets` 零错误零改动通过，验证了「API 零变更」这一判断。

## 5. 顺带发现（未修）

`crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs:272`，`UdpMux::poll` 在**没有任何
进展**时也无条件自唤醒：

```rust
if count == 0 { break; }          // 「什么都没准备好」
…
cx.waker().wake_by_ref();          // ← 无条件，两条路径共用这个收尾
Poll::Pending
```

`Ok(0)` 与「burst 用满」走了同一条收尾路径，而前者是**无进展**的。自唤醒 + 立即 Pending
只是把忙循环搬到 executor 上绕一圈，CPU 一分不省。

不是本次事故的原因（触发它要求底层 socket 返回 `Ok(0)`，而 webrtc-rs 的实现在无数据时
返回 `Pending`），但属同一类缺陷。改法是把 `break` 换成 `continue`——让下一轮
`poll_recv` 有机会返回 `Pending` 并登记 waker，`MAX_RECV_BURST` 兜住病态情况。
**改这个文件必须同时改文件末尾那两条护栏测试**（见其顶部注释块）。

## 6. 待复测

- [ ] **web→native 吞吐**：500 KB/s 是否随本次修复回到正常量级。理论上应该——CPU 被空转
      吃光会直接压制收发；但也不排除另有独立瓶颈（对照组：web→web 4 MB/s 不经过 native driver）
- [ ] **长时传输**：7GB 级传输在有 Web 端连入 / 断开的场景下能否跑完
- [ ] 上游 rtc 发布 0.20.2 后，删 patch 并回归
