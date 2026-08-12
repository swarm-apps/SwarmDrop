# WebTransport 真机实测（2026-08-12，v0.18.0）

> **状态**：已实测，结论生效。补上了 `net-kernel.md` 里挂了半天的「真机测量未做」。
>
> **一句话**：局域网内 Android ↔ 桌面 Chrome，手机发 20 MB/s、浏览器发 9 MB/s。
> 前者落进了 native↔native QUIC 的区间（12–23 MB/s）——**浏览器在接收方向上已经不是
> 瓶颈**。这是回环那个「4.5 倍」外推不出来的结论。

## 测量条件

| 变量 | 取值 |
|---|---|
| 链路 | **局域网直连**（同一 Wi-Fi），不经 bootstrap 中转 |
| 两端 | Android 手机（`mobile-v0.18.0`）↔ 桌面 Chrome 上的 Web 端 |
| 传输 | WebTransport（`/quic-v1/webtransport`） |
| 载荷 | 单文件 ~2 GB |

⚠️ **没测的**：跨网（经公网 bootstrap 中转）、iOS、Firefox/Safari、以及**同一构建上的
WebRTC 对照**（见下面「这个倍数不要直接引用」）。

## 结果

| 方向 | 吞吐 |
|---|---|
| 手机 → Web | **~20 MB/s**（≈160 Mbps） |
| Web → 手机 | **~9 MB/s** |

横向对照（**注意各自的测量条件不同，不能当同一次 A/B**）：

| 参照 | 吞吐 | 出处 |
|---|---|---|
| native↔native QUIC | 12–23 MB/s | [`2026-08-10-v0.15.2-field-test.md`](2026-08-10-v0.15.2-field-test.md) |
| web↔native WebRTC | 0.36–0.96 MB/s | 同上 |
| web↔web WebRTC | ~5 MB/s | [`2026-08-11-web-webrtc-throughput.md`](2026-08-11-web-webrtc-throughput.md) |
| 回环 WebTransport vs webrtc-direct | 322 vs 72 MiB/s | [`net-kernel.md`](../knowledge/net-kernel.md) |

### 这个倍数不要直接引用

拿 20 MB/s 除 0.36 MB/s 会得到「快 56 倍」——**那个数是错的**，因为分母来自一个**不同的
构建**：0.36–0.96 MB/s 是在 `mobile-release` 还用 `opt-level = "z"` 时测的，而 WebRTC 的
DTLS 走 RustCrypto 的纯 Rust AES-GCM/GHASH，`-Oz` 关掉内联后它慢一个数量级（那正是把
profile 改回 `opt-level = 3` 的理由，见 `toolchain.md`）。改完之后 WebRTC 那条**没有重新
在真机上测过**。

同理，回环的「4.5 倍」也不能外推：回环瓶颈是 CPU，局域网瓶颈是 Wi-Fi 带宽与协议栈开销。

**能说的**：WebTransport 在这条链路上跑到了 20 MB/s，与原生 QUIC 同一量级；用户报告同场景下
比 WebRTC「快很多」。**不能说的**：具体几倍。要那个数就得做干净的 A/B——同一构建、同一台
手机、同一个局域网、只切传输。

## 20 vs 9 的不对称

浏览器**作为接收方**（20 MB/s）比**作为发送方**（9 MB/s）快 2.2 倍。用户独立确认这个方向
差异是**能感觉到**的，不是测量噪声。

### 已排除的三条（都由代码事实排除，不是推测）

- ❌ **不是 prepare 拖低了观感速率。** 我最初的假设是「9 MB/s 是传输+准备的合成速率」。
  证伪：Web 端把 prepare 作为**独立阶段**追踪（`docs/app/app/_lib/store.ts` 的
  `activePrepare` / `bytesHashed`，由 `PrepareProgressEvent` 驱动），传输速率另算。
  9 MB/s 是**数据面速率**。
- ❌ **不是 OPFS 写盘慢。** 浏览器收才写 OPFS，而收恰恰是快的那个方向。
- ❌ **不是 wasm 的 blake3 慢。** 两个方向的浏览器侧都要做 blake3——收端逐块验签、发端建
  outboard 加逐块 proof，工作量同量级。若 blake3 是瓶颈，两个方向该一样慢。

### 剩下的：发送路径是串行的，而它独有一份跨边界拷贝

`crates/transfer/src/actor/sender.rs` 的 `write_block` 严格串行，**没有预取**：

```text
read_source_chunk().await  →  encode_proof()  →  write_frame().await
```

浏览器上那个 `read_source_chunk`（`crates/web/src/file_access.rs:137`）每 256 KiB 要做：

1. `File.slice(offset, end)` — 廉价
2. `blob.array_buffer()` — 磁盘读 + 在 **JS 堆**上产生一个新 ArrayBuffer（第一次拷贝）
3. `JsFuture` await — 一次 promise/microtask 往返
4. `Uint8Array::new(&buf).to_vec()` — **第二次拷贝**，JS 堆 → wasm 线性内存

2 GB / 256 KiB = 8192 次，即 ~8192 次 promise 往返 + **~4 GB 的 memcpy**，而这些**全都
不与网络发送重叠**——串行链上读完才发，发完才读下一块。

接收方向没有对称的停顿：它由网络驱动，写盘与收下一帧天然交错。

⚠️ **以上是「剩下的那个」，不是实测结论。** 别照着它去优化——先读探针（下一节）。
[`2026-08-11-web-webrtc-throughput.md`](2026-08-11-web-webrtc-throughput.md) 记的教训正是
凭推理排除掉一层、后来发现排除是错的。

### 怎么量：探针已经在那儿，不用改代码

`SendProbe`（`crates/transfer/src/probe.rs`）已经把每块的壁钟时间摊到四个阶段上：
**read / proof / write / ack**。

- 每 `REPORT_EVERY = 256` 块（= 64 MiB）打一条，会话结束时由 `Drop` 打一条汇总
  （失败路径也漏不掉）。
- 发的是 `tracing::info!(target: "swarmdrop_transfer")`，而 Web 端的 filter 给
  `swarmdrop_transfer` 开到 **DEBUG**（`crates/web/src/lib.rs:75`）——**它本来就打在浏览器
  console 上**。

**做法**：Chrome DevTools console 里跟着 Web 端发送时的 `传输探针` 行看 `mb_s` 与 `stages`。

| 读到什么 | 结论 |
|---|---|
| `read` 占大头 | 上面那条成立：瓶颈是 `File` 读 + 两次跨边界拷贝。解法是预取（读下一块与发当前块重叠），而不是把块调大 |
| `write` 占大头 | 瓶颈是传输层背压（浏览器 WebTransport 的 `WritableStream`），与文件读无关 |
| `proof` 占大头 | 上面对 blake3 的排除错了，回去查 wasm 侧的 bao/blake3（考虑 `simd128`） |
| 各阶段之和 ≪ 100% | 有没被计入的等待，在循环之外——探针的文档明确说这个差值本身是信息 |

移动端发送时同一条日志也会打（Android logcat），**两边对比**就能把「浏览器特有」与
「本来就这样」分开。

## 对既有决策的影响

**4003（webrtc-direct）仍然不能下线**，理由与吞吐无关，别把这次验证当成下线依据：

1. **bootstrap 上它是发现路径。** 浏览器不写死 WebTransport 地址——先用 webrtc-direct 连上
   bootstrap，经 identify 学到带**当前** certhash 的那条。这天然绕开「证书 14 天一换、
   清单里的地址会过期」。砍掉 4003 就得把一个会过期的地址写进客户端清单。
2. **打洞只有 WebRTC 有。** WebTransport 没有对应的 NAT 穿越机制，跨网场景下两者覆盖的
   路径不同。本次只测了局域网，跨网那格还是空的。

## 下一步（按价值排序）

1. **跨网测一次**（手机走蜂窝或异地 Wi-Fi）——局域网 20 MB/s 说明不了中转/打洞路径。
2. **确认 prepare 假设**（上面那个单一观察，几乎零成本）。
3. **干净的 WebTransport vs WebRTC A/B**，同构建同链路——有了它才能写下倍数。
4. iOS + Safari；Firefox（Gecko）至今未测。
