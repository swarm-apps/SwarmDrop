# SwarmDrop 传输吞吐综合诊断

> **日期**：2026-08-10
> **决策状态**：诊断完成，**两个根因均待实测定案**。未落地任何修复。
> **方法**：7 维度并行代码侦察 → 30 条候选逐条对抗性验证（10 条被推翻）→ 综合。
>
> ### ⚠️ 读这份报告前必须知道的两条前提
>
> 1. **「桌面↔桌面能跑几十 MB/s」这条对照基线是推断，不是实测。** 它来自症状 B 的初始
>    50 MB/s（证明桌面**发送端**能跑 50 MB/s），不是一次真实的桌面↔桌面 webrtc-direct
>    压测。§1.4 展开了为什么这条基线可能不成立——**定案前必须补一组同物理路径的对照**。
> 2. **症状 B 的「物理侧」结论是排除法得出的**，不是正向证据。它的强度完全取决于
>    §2.2 那张逐条量化表的完备性：表里每一条都有 file:line 与实测/推算数字，加总
>    < 0.1%，但「没找到」永远弱于「找到了」。B1 反向传输实验是把它变成正向结论的最短路径。
>
> ### 主线已独立核实的四条（非 agent 结论，直接读的代码）
>
> | 事实 | 位置 | 影响 |
> |---|---|---|
> | Web 端**不是** debug wasm | `docs/package.json:11` `wasm-pack build --release`，产物 4.3 MB | 排除「构建没优化」这个廉价解释 |
> | wasm rustflags **没有** `+simd128` | `.cargo/config.toml` | BLAKE3 走 portable；但只占症状 A 预算 2%（见 #8） |
> | 移动端 `opt-level = "z"` | `Cargo.toml` `[profile.mobile-release]` | 压低移动端恒定速率，**不产生**单调劣化 |
> | udp_mux **零 `SO_RCVBUF` 设置**、`BRANCH_CAPACITY = 256` 满即静默丢包 | `udp_mux.rs:157` / `:97` / `:312-327` | 症状 A 主根因假设的两块基石，均属实 |
>
> 另核实：浏览器侧 `MAX_BUFFERED_AMOUNT = 1 MiB` / 低水位 512 KiB
> （`wasm/data_channel.rs:35-38`）。3.3 MB/s 下排空 512 KiB 需 155 ms——远长于事件循环
> tick，说明 **bufferedAmount 背压是被动跟随 SCTP 排空速率，不是原因**。这条独立支持
> §1.1「瓶颈在 `dc.send()` 下游」的预算分解。

## 0. 一句话结论

| | 主根因判定 | 置信度 |
|---|---|---|
| **症状 A**（Web→桌面 恒定 3.3 MB/s） | **不在应用层。98% 的每块预算花在 `dc.send()` 下游的 webrtc-direct 数据面上**；最可能的落点是**桌面 listener 侧 `udp_mux` 的入向丢包 → SCTP 拥塞窗口塌陷**（`BRANCH_CAPACITY = 256` 满即丢，且丢包日志在生产里被日志级别屏蔽） | 中高，**未实测确认**，判别实验成本 5 分钟 |
| **症状 B**（桌面→移动 50→7 MB/s） | **不是速率口径假象**（已核实是 3 s 滑动窗口的瞬时速率）；**Rust 代码里不存在量级足够的 O(已完成)/块 机制**（全部候选加总 < 0.1%）。主根因在代码之外——**接收设备的物理侧**（闪存 pSLC 耗尽/GC、SoC+Wi-Fi 热节流），二者用「反向传输」实验一次分开 | 口径排除项高；物理侧归因中，**需实测** |

七个维度的存活结论全部自评「explains neither」，这个自评是对的。**它们不是根因，修复清单里排在第二梯队。**

---

## 1. 症状 A：Web 端发送恒定 3.3 MB/s

### 1.1 预算分解——先把「应用层」整体排除

311.7 MB ÷ 262144 = **1189 块**；94.5 s ÷ 1189 = **每块 79.5 ms 壁钟预算**。

| 阶段 | 位置 | 实测/推算 | 占 79.5 ms |
|---|---|---|---|
| 浏览器读源文件 | `crates/web/src/file_access.rs:137-157` | 0.255 ms | 0.32% |
| blake3 + `encode_proof` | `crates/transfer/src/bao.rs:308-321`（wasm 未开 SIMD） | 0.27–0.42 ms | 0.4% |
| 33 × `dc.send(8 KiB)` + 帧编码 | `crates/webrtc-p2p/src/backend/wasm/data_channel.rs:299-302` | 0.5–0.7 ms | 0.8% |
| **浏览器本地 CPU 小计** | | **≈ 1.1–1.5 ms** | **1.4–1.9%** |
| 桌面接收端全部工作（bao decode + pwrite + 每 10 块一次 DB） | `receiver.rs:410-447` | ≈ 0.2 ms | 0.25% |
| **剩余：等 `bufferedAmount` 排空 / 等 Window echo** | `data_channel.rs:277-280` | **≈ 77.8 ms** | **≈ 98%** |

**这条算术是硬的**：把浏览器侧所有 CPU 工作（读文件、blake3、拷贝、分配）全部归零，3.3 MB/s 只会变成 3.36 MB/s。七个维度里所有被推翻的候选（主线程无 Worker、`File.slice()`、`write_frame` 三次 await、串行发送循环、每块 1.8 MB 内存流量）之所以全部被推翻，是同一个原因：**它们都在那 1.9% 里**。

### 1.2 差分证据——瓶颈落在生产链路独有的那两段

| 链路 | 吞吐 | 出处 |
|---|---|---|
| 浏览器 ↔ webrtc-rs，host 路径，裸 DataChannel（**无 udp_mux、无 libp2p framing**） | **50.1 MiB/s** | `dev-notes/research/2026-07-webrtc-native-ice.md:180` |
| 浏览器 ↔ 桌面，relay(ws) + noise，六组基准 | **31±3 MB/s** | `dev-notes/knowledge/libp2p-wasm.md:636` |
| 浏览器 ↔ 桌面，**生产 webrtc-direct** | **3.3 MB/s** | 本次观测 |

生产链路相对那次 50 MiB/s 的 spike，多出来的东西**只有两样**：

1. **桌面 listener 侧的 `udp_mux`**。`crates/webrtc-p2p/src/backend/native/direct/upgrade.rs:279-285` 明确：只有 `socket: Some(..)`（listener）才套 `MuxedRuntime`；dialer 侧用 `PeerConnection` 自己的 socket。浏览器是 dialer、桌面是 listener，所以 **mux 只存在于生产路径，spike 那条路上没有它**。
2. **`WEBRTC_MAX_MESSAGE_SIZE = 8 KiB`**（`crates/net/src/transport.rs:42-43`）。spike 是裸 DataChannel，没有 libp2p framing 的 8 KiB 切分。

### 1.3 主根因判定：udp_mux 入向丢包 → SCTP cwnd 塌陷

**机制**（`crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs`）：

- `:97` `const BRANCH_CAPACITY: usize = 256;`
- `:312-327` `deliver()`：`tx.try_send(...)`，`Err(is_full)` → **直接丢包**，只留一行 `tracing::debug!("支路缓冲已满，丢弃数据报")`。
- `:157` `std::net::UdpSocket::bind(addr)` 之后只 `set_nonblocking(true)`。**全仓 `rg "set_recv_buffer|SO_RCVBUF|socket2"` 零命中** —— 共享 UDP 端口用的是系统默认接收缓冲（macOS `net.inet.udp.recvspace`、Linux `net.core.rmem_default`）。
- `:445-496` `MuxedSocket::poll_recv` 每次只交出 **1 个数据报**（`Poll::Ready(Ok(1))`）。

**为什么它能造出恒定 3.3 MB/s**：3.3 MB/s ÷ 1 ms LAN RTT ⇒ 等效 cwnd ≈ 3.4 KB ≈ **3 个 MTU**。Reno 类拥塞控制退到 3 个包的窗口，对应百分之几量级的持续丢包率。这正是「从第一秒起就恒定、远低于链路能力、但传输仍能完成」的签名——不是 CPU 瓶颈的形状（CPU 瓶颈会随负载波动），也不是带宽瓶颈的形状。

**这条为什么一直没被发现**：桌面默认 tracing filter 是 `swarmdrop=debug,swarmdrop_net=debug`（`src-tauri/src/setup.rs:15-16`），**`webrtc_p2p` 这个 target 一个 debug 都不输出**。丢包在生产日志里是完全隐形的。

**次要候选（按份额排）**：
- ② `WEBRTC_MAX_MESSAGE_SIZE = 8 KiB` —— 每块 257 KB 的 proof 要切 **33 条 SCTP user message**，抬高两端每消息成本。个位数百分比量级，且**不能盲目调大**（浏览器安全上限）。
- ③ Chrome usrsctp 自身的行为。不在本仓控制内。

### 1.4 ⚠️ 基线本身可能不成立

「同一台机器上桌面↔桌面能跑到几十 MB/s」—— 如果那是 **loopback / QUIC**，它既不经 UDP mux、也不经 SCTP、更不经物理网卡，**不构成 3.3 MB/s 的对照**。定案前必须补一组「桌面↔桌面走同一条物理路径」的基线。

### 1.5 定案还需要什么（按成本排序）

| # | 做什么 | 怎么做 | 看什么 |
|---|---|---|---|
| A1 | 打开 webrtc-p2p 日志重跑一次 | `RUST_LOG=swarmdrop=debug,swarmdrop_net=debug,webrtc_p2p=debug` 启动桌面，重传那个 311 MB 文件 | `grep -c "支路缓冲已满"`。**> 0 即 A 定案**；顺带看 `数据报超出接收缓冲，丢弃`（`udp_mux.rs:477-482`） |
| A2 | 看内核有没有静默丢 | 传输前后各跑一次 `netstat -s -p udp`（macOS）/ `nstat -az UdpRcvbufErrors`（Linux） | `dropped due to full socket buffer` 的增量 |
| A3 | 确认选中的 ICE 路径与 RTT | 浏览器 `chrome://webrtc-internals`，看 `RTCIceCandidatePair` 的 `currentRoundTripTime` / `availableOutgoingBitrate` / `bytesSent` 斜率 | RTT 若 > 5 ms，说明走的不是 host 直连（spike 里 srflx = 0.6 MiB/s） |
| A4 | 把「等」定位到具体 await | 见 §4 探针 P2：在 `crates/transfer/src/actor/sender.rs:543/562/564/427` 四处埋计时 | `write_frame` 占比 ≈ 98% ⇒ 传输层；`sync_window` 占比高 ⇒ 接收端消化慢 |

---

## 2. 症状 B：桌面→移动 随进度单调劣化

### 2.1 先排除速率口径（任务要求 #2）

**不是累计平均造成的假象。** 核实链条：

- `crates/transfer/src/progress.rs:174` `samples: VecDeque<(Instant, u64)>`
- `:179` `const SPEED_WINDOW: Duration = Duration::from_secs(3);`
- `:285-289` 每次 `add_bytes` 按 3 s 裁剪窗口前沿
- `:292-303` `speed() = (b_last - b_first) / (t_last - t_first)` —— **窗口内的瞬时速率**
- 显示端：`mobile/src/app/transfer/[sessionId].tsx:503` `formatSpeed(Number(progress.speed))`；通知同源 `mobile/src/core/foreground-service.ts:200`。桌面与 Web 用的是同一个字段。

结论：**50 → 7 MB/s 是真实的瞬时速率劣化**，口径正确。所以答案落在「漏掉了」这一侧 —— 但准确说法是：**代码里的 O(已完成)/块 机制一条不漏地被找到了，只是量级全部差 3 个数量级**。

### 2.2 代码里所有随进度增长的项——逐条量化后加总 < 0.1%

要把 50 MB/s 压到 7 MB/s，每块耗时必须从 **5243 µs 涨到 37449 µs，即 +32.2 ms/块**。

| 机制 | 位置 | 末块增量（实测） | 占所需 32.2 ms |
|---|---|---|---|
| throwaway `PreOrderOutboard<Vec<u8>>` 零填（唯一真正 O(已完成)/块 的项） | `crates/transfer/src/bao.rs:344-348` | **+15.2 µs** | **0.047%** |
| 整个 `decode_and_verify`（主体是 blake3 256 KiB） | `bao.rs:346-355` | 140→160 µs（恒定项） | 0.4%（且不增长） |
| 每块 3 次全量 bitmap popcount | `receiver.rs:430 / 612 / 613` → `checkpoint.rs:134-149` | 0.19–3.2 µs，**O(总块数) 恒定** | 0 |
| 每 10 块一次 `ranges_from_bitmap` | `receiver.rs:625` → `checkpoint.rs:169-200` | 摊 1.4–2.1 µs，**恒定** | 0 |
| checkpoint 落库（rollback journal + FULL） | `storage-sql/src/ops.rs:129-143` | Android 11.6 ms/次 ÷ 10 = 1.16 ms/块，**实测 fill=100→24000 全平** | 0（只压低恒定速率） |
| 4 份 256 KB 拷贝 | `data_frame.rs:220/460`、`bao.rs:351`、`file_sink.rs:85` | 21–28 µs，**O(1)** | 0 |
| **合计增长项** | | **≈ 15 µs** | **< 0.05%** |

我另外核实了各 agent 没覆盖的移动接收写盘路径：`mobile/packages/swarmdrop-core/rust/mobile-core/src/file_staging.rs:134-145` 是 **pwrite（`write_all_at`）+ 复用同一个 `Arc<File>` 句柄**，无 fsync、无预分配、无重开、无 seek —— **Rust 侧没有任何随进度增长的写盘成本**。发送侧同理：`sender.rs:318-320` 明确只在终态落库；`encode_proof` 走 `PostOrderOutboard<&[u8]>`，是 O(log n)。

**因此：主根因必然在 Rust 之外。**

### 2.3 三个可判定的外部假设

| | 假设 | 与观测的吻合度 | 判别信号 |
|---|---|---|---|
| **H1** | **闪存 pSLC 缓存耗尽 + GC**。写 6 GB 到手机内部 UFS/eMMC，pSLC 直写耗尽后落到 TLC/QLC 直写并与 GC 争用 | 形状完全吻合：起始 50 MB/s（pSLC）→ 平滑单调下降 → 稳定在直写速率 | 与**已写字节数**相关，与时间无关 |
| **H2** | **热节流**：SoC + Wi-Fi 射频 + UFS 一起降频。6 GB ÷ 平均 25 MB/s ≈ 4 分钟持续满载，正好是手机进入节流的时间尺度 | 同样单调，7 MB/s ≈ 56 Mbps 是 Wi-Fi 降 MCS / 退 20 MHz 的典型值 | 与**时间**相关，与字节数无关 |
| **H3** | 页缓存脏页回写节流（`balance_dirty_pages`） | 形状是**有拐点**（撞到 dirty_ratio 后骤降），不是平滑单调 | 拐点应出现在 RAM×20% 处 |

**一条本仓自己的放大器（不是根因，但必须记）**：Android 上 publish 是**整份复制**——`mobile-core/src/file_access.rs:225-237`，`content://` 目标走 JS `publish_to_target`（`ContentResolver`），只有 `file://` 才 rename（`:274`）。所以 **6 GB 文件在 Android 上要写盘 12 GB**，把 H1 的到达点提前一倍，并在 100% 处再制造一次长时间停顿。

### 2.4 定案实验（零代码，10 分钟）

| # | 实验 | H1 | H2 | H3 |
|---|---|---|---|---|
| **B1** | **方向反转**：同一台手机 → 桌面，传同样的 6 GB（手机只读不写） | 劣化**消失** | 劣化**保留** | 消失 |
| **B2** | 6 GB 传完立刻再传一次 3 GB | 第二次**起始就慢** | 第二次起始就慢 | 第二次正常 |
| **B3** | 传 3 GB，休息 10 分钟，再传 3 GB | 第二段起始就慢 | 第二段**恢复到 50** | 恢复 |
| **B4** | 全程 `adb shell dumpsys thermalservice`、`dumpsys battery` 温度、`iw dev wlan0 link`（速率/MCS）每 10 s 采样 | 温度平、链路速率平 | 温度升、链路速率降 | — |

外加一条 Rust 侧探针（§4 的 P1）直接回答「接收端是不是被饿着」：如果 `read_frame` 的等待时长随进度增长、而 `verify/write/checkpoint` 四个阶段都平坦 → **瓶颈在链路或发送端，不在接收端的工作量**；反之则在写盘。

---

## 3. 探针代码（先做这个，再做修复）

### P1 — 接收端逐阶段计时

`crates/transfer/src/actor/receiver.rs`：在 `run_data_channel` 的读循环（`:318-401`）与 `handle_block_data`（`:410-447`）里累加五个 `Duration`：

```rust
// receiver.rs:324 前
let t_wait = Instant::now();
let frame = tokio::select! { ... };
stats.wait += t_wait.elapsed();           // ← 关键：被链路饿着的时间
```
`handle_block_data` 内分别包住 `verify_block`(:420)、`ensure_sink`(:436)、`persist_chunk`(:439) 里的 `write_sink_chunk`(:601) 与 `update_file_checkpoint_ranges`(:626)。每 1024 块 `tracing::info!(block_idx, wait_ms, verify_ms, write_ms, ckpt_ms, ...)`。

**判读**：`write_ms` 随进度增长 ⇒ H1；`wait_ms` 增长而其余平坦 ⇒ H2（链路）；全平坦 ⇒ 去查发送端。

### P2 — 发送端逐阶段计时（同时服务 A 与 B）

`crates/transfer/src/actor/sender.rs`：`read_source_chunk`(:543-546)、`encode_proof`(:561-562)、`write_frame`(:564-578)、`sync_window`(:427-459) 四段各计时，同样每 1024 块打一条。

**判读（症状 A）**：`write_frame` ≈ 98% ⇒ 传输层背压（对应 §1.3 主根因）；`sync_window` 占大头 ⇒ 接收端消化慢。

### P3 — udp_mux 丢包计数器

`crates/webrtc-p2p/src/backend/native/direct/udp_mux.rs:318-320`：把 `tracing::debug!` 提成 `warn!`，并加一个 `AtomicU64` 计数每 1000 次打一条。**这是症状 A 的判决书。**

---

## 4. 修复清单（按投入产出排序）

### 第一梯队 —— 诊断与症状 A 候选修复

| # | 位置 | 改法 | 预期收益 | wire | 护栏测试 |
|---|---|---|---|---|---|
| **1** | 运行环境 | `RUST_LOG=...,webrtc_p2p=debug` 重跑一次 311 MB 传输，统计「支路缓冲已满」 | 0（诊断），但**决定后面全部动作** | 无 | 无 |
| **2** | `udp_mux.rs:318-320`、`receiver.rs`、`sender.rs` | 探针 P1/P2/P3，约 60 行 | 0（诊断） | 无 | 新增即可，不碰现有 |
| **3** | `udp_mux.rs:157` | `std::net::UdpSocket::bind` 后用 `socket2` 设 `SO_RCVBUF = 4 MiB`（macOS 还需 `sysctl net.inet.udp.recvspace` 允许） | 若 A2 计数 > 0：**可能 3.3 → 数十 MB/s** | 无 | `gro_buf_sized_by_segment_not_by_mtu`(:749)、`is_retryable_recv_error` 那两条不受影响 |
| **4** | `udp_mux.rs:97` | `BRANCH_CAPACITY: 256 → 2048`（2.4 MB/连接），丢包日志 `debug!`→`warn!`+计数 | 同上；两者需一起改，否则只是把丢包点从内核挪到用户态 | 无 | 需**新增**一条「支路满时计数递增」的测试 |
| **5** | `crates/net/src/transport.rs:42-43` | `8 KiB → 16 KiB`（libp2p 官方默认值） | 个位数百分比；**必须在 #1/#2 有数据后再改** | **不需要 bump `TRANSFER_DATA_PROTOCOL`** —— 这是 WebRTC 层的 `a=max-message-size`（`sdp.rs:82`）+ Noise 后协商，两端**自动取较小值**，新旧端混连自动降级 | `wasm/muxer.rs:133-136` 的 `read_buffer_limit` 已经是 `DEFAULT_MAX_READ_BUFFER.max(msg_size)`，16 MiB > 16 KiB，无需同改 |

> ⚠️ #3/#4 是**假设驱动**的。**不做 #1 就不要改它们** —— 若丢包计数为 0，说明主根因判错，改了也白改，还平白加了 2.4 MB/连接的内存。

### 第二梯队 —— 已确认的真实缺陷，与两个症状无关

| # | 位置 | 改法 | 收益 | wire | 护栏测试 |
|---|---|---|---|---|---|
| **6** | `crates/transfer/src/bao.rs:32` + `:344-348` | `PreOrderOutboard { data: Vec::new() }` → `EmptyOutboard { root, tree }`，import 同改；`:342-343` 那两行注释现在描述的是旧实现，**必须一并改写** | 末块 decode −9%（160.3→145.1 µs），消掉 6 GiB 传输中 **19.32 GB** 无谓 memset | **无** | ⚠️ `EmptyOutboard::save` 对 `is_relevant_for_outboard()==false` 的节点**返 Err**，而 `PreOrderOutboard::save` 在半叶子尾块静默 no-op。**必须新增**尾部非对齐尺寸的护栏测试（`CHUNK_SIZE*2+88KiB+123`、`*3+4321`、`*5+7`、`*9+999`）。现有 `roundtrip_*` / `tampered_block_is_rejected` / `wrong_root_is_rejected` 全绿 |
| **7** | `crates/migration/src/lib.rs:52-53` | 自建 `SqlitePoolOptions` + `journal_mode(Wal)` + `synchronous(Normal)` + `create_if_missing`，再 `SqlxSqliteConnector::from_sqlx_sqlite_pool`。**不能走连接串**（sqlx URL query 只认 `mode`/`cache`/`immutable`/`vfs`），**也不能连上后 `PRAGMA`**（那是每连接设置，后面是池子） | Android checkpoint **11.62 → 0.288 ms（40×）**，峰值吞吐 +19%（50→60 MB/s）。桌面/iOS 无感 | 无 | 必跑 `cargo test -p migration`（`m20260805_000001_init/mod.rs:191-193` 的 `PRAGMA foreign_keys == 1` 断言是头号回归点）；`lib.rs:80` 那句「副文件通常不存在」的注释要改；`remove_sqlite_files` 已经删三个后缀，自愈路径本来就对 |
| **8** | `crates/transfer/Cargo.toml:44-45` | 在已有的 wasm target 段加 `blake3 = { version = "1.8.3", features = ["wasm32_simd"] }`。`crates/core/Cargo.toml:54` 同样照抄。**不需要 `-C target-feature=+simd128`**（blake3 走 cargo feature，`build.rs:24` 只认 `CARGO_FEATURE_WASM32_SIMD`） | wasm blake3 996→1941 MB/s（1.95×），但只占症状 A 预算的 2% ⇒ 端到端 **+0.85%** | 无（SIMD 与 portable 输出逐位相同） | ⚠️ 浏览器地板抬到 Chrome 91 / Safari 16.4（不支持则**整个模块 instantiate 失败**）；`pnpm build:wasm` 的 wasm-opt 可能要 `[package.metadata.wasm-pack.profile.release] wasm-opt = ["-O","--enable-simd"]`。必跑 `./scripts/test-wasm.sh` |
| **9** | `src-tauri/src/host/file_sink.rs:76,85` + `file_source.rs:209` | `write_at(&self, offset, data: &[u8])` → `data: Vec<u8>`，删掉 `:85` 的 `let data = data.to_vec();`，调用点去 `&` | 4.4 µs/块，零设计代价 | 无 | `src-tauri/src/host/file_sink/path_ops.rs:207/235/236` 三处调用点要跟着改；:235/:236 是**并发 pwrite 正确性**测试，别顺手把 `tokio::join!` 拆成串行 |
| **10** | `mobile-core/src/file_access.rs:225-237` | Android SAF 目标的 publish 是**整份复制**，6 GB 文件写盘 12 GB。可考虑：staging 直接建在 SAF 目标目录的隐藏子目录里（但会撞回 `file_staging.rs:8-16` 记录的 SAF fd `EBADF` 事故），或至少在 UI 上把这段停顿显式化 | 减半 Android 的闪存写入量（对 H1 是直接缓解） | 无 | 需重新评估 `file_staging.rs` 模块文档里那条「随机写只施加于本进程完全拥有的 fd」的不变量——**不要为了性能破它** |

### 明确不要做

| 不做 | 理由 |
|---|---|
| 增量维护 bitmap 计数替掉 popcount | 收益 0.09%；`mark_chunk_completed` 是幂等 `\|=`，换计数器后重复块会静默把 `completed` 算高 → 文件残缺就被 publish。引入的正确性风险 > 收益 |
| 把停等窗口改成真滑动窗口 | 要求边写边读确认 → 必须 split，重新引入 wasm 的 BiLock lost-wakeup（native 多线程掩盖、只在浏览器显形、跑不进 CI）。收益 ~3% |
| 删除/削弱 `WINDOW_CHUNKS` 流控 | 它是 webrtc-direct 上**唯一**的端到端流控（浏览器 `RTCDataChannel` 是推模型、SCTP 接收窗口永不收缩，`data_channel.rs:43-56` 有实测记录）。削弱 = 复现「20 MB 传到 12–22% 子流被重置」 |
| 单独调大 `WINDOW_CHUNKS`（16→32）而不动 `DEFAULT_MAX_READ_BUFFER` | `crates/transfer/src/lib.rs:47-51` 写死了「4× 余量」判据，只改一边就退回上面那个已修复的 bug。两处必须同改（`lib.rs:55` + `wasm/data_channel.rs:62`） |
| 给 bao-tree 内部的 `vec![0u8; chunk_group_bytes()]` 做手术 | 在库内部、无注入点，只值 12.7 µs/块。要么给上游提 PR，要么放弃「不手写 Merkle」这条选型 |

---

## 5. wire 协议与护栏测试影响一览

| 修复 | 需 bump `TRANSFER_DATA_PROTOCOL`？ | 碰哪些护栏测试 |
|---|---|---|
| #3 #4 udp_mux 缓冲/容量 | 否 | `udp_mux.rs:749 gro_buf_sized_by_segment_not_by_mtu`、`:908 empty_buffers_yield_pending_not_zero_count`、`:922 branch_reports_own_gro_and_delegates_gso` 均不受影响；**需新增**丢包计数测试 |
| #5 `WEBRTC_MAX_MESSAGE_SIZE` 8→16 KiB | **否**（WebRTC 层双向协商取小值，新旧端自动兼容） | 无；但需 `./scripts/check-wasm.sh` + 一次真机浏览器互通 |
| #6 `EmptyOutboard` | 否（proof 字节流、`CHUNK_SIZE`、`bao::BLOCK_SIZE`、树形状全不变） | **必须新增**半叶子尾部测试；`records_sequential_forward_reads`(:712)、`is_outboard_usable` 那两条守的是**发送/构建**侧，不受影响 |
| #7 SQLite WAL | 否 | `cargo test -p migration`（`PRAGMA foreign_keys`）+ `crates/core/tests/e2e_transfer.rs:1363`（末块不刷 checkpoint）、`:1735` |
| #8 blake3 wasm SIMD | 否（hash 逐位相同，`outboard_len` 判据不变，存量 outboard 不作废） | `./scripts/test-wasm.sh`（模块能否 instantiate）+ CI 的 wasm-opt |
| #9 去掉 `to_vec()` | 否 | `path_ops.rs:207/235/236` 三处调用点 |
| #10 Android publish | 否 | `file_staging.rs` 的四条 tokio 测试；**不要破**「随机写只施加于本进程 fd」这条不变量 |

**任何改 `bao::BLOCK_SIZE` 或 `CHUNK_SIZE` 的想法都要 bump `TRANSFER_DATA_PROTOCOL`**（proof 树形状变，旧端第一个块就验签失败）——本清单里**没有一条**需要动它们。

---

## 6. 执行顺序建议

1. **今天**：跑 #1（`RUST_LOG` 重传一次）+ B1/B4（方向反转 + 温度采样）。两个实验加起来不到半小时，直接决定 A 和 B 的定案。
2. **本周**：合入 #2 探针（长期留在代码里，`tracing::info!` 级别，不影响生产），带着它再跑一次两个场景。
3. **拿到数据后**：A 侧按判决走 #3/#4（丢包为真）或回到 #5/§1.5-A3（丢包为假，去查 ICE 选路）；B 侧按 H1/H2 分支走 #10 或「这是设备物理上限，UI 层面给出解释」。
4. **任何时候可以顺手做**：#6 #7 #8 #9。它们是四个真实缺陷，加起来能给 Android 峰值 +19%、给 Web CPU 侧 1.95×、消掉 19 GB 无谓内存流量——**但改完两个症状的数字都不会变，不要拿它们当验收标准**。
---

## 7. 实测指引（v0.15.1 探针版，2026-08-10 落地）

探针已合入，**三端常开、`info!` 级、自动落进日志文件**，不需要设 `RUST_LOG`。

### 7.1 落地了什么

| 改动 | 位置 | 作用 |
|---|---|---|
| 逐阶段耗时探针 | `crates/transfer/src/probe.rs`（新增） | 每 256 块（64 MiB）一条窗口报告 + 会话结束一条汇总（由 `Drop` 打，**失败路径也有**） |
| 发送端接线 | `actor/sender.rs` | 四阶段 `read` / `proof` / `write` / `ack` |
| 接收端接线 | `actor/receiver.rs` | 五阶段 `wait` / `verify` / `write` / `ckpt` / `rest` |
| udp_mux 丢包计数 | `backend/native/direct/udp_mux.rs` | `debug!` → 限频 `warn!`（按 2 的幂次），带累计数 |
| 三端 filter 放行 `webrtc_p2p` | `src-tauri/src/logging.rs`、`mobile-core/src/logging/mod.rs` | 桌面/移动此前**完全看不到** webrtc-direct 的告警（Web 端本就有 `TRACE`） |

日志长这样：

```
传输探针 role=send session=… kind=window blocks=512 elapsed_ms=15230 mb_s=8.61
  stages=read=45ms/0% proof=201ms/1% write=14880ms/98% ack=98ms/1%
```

`stages` 各段占比**之和 < 100%** 是有意义的信号：差值是没被任何阶段覆盖的开销。

### 7.2 怎么读

| 观察 | 结论 |
|---|---|
| 发送端 `write` 占 90%+ | 卡在传输层背压（`dc.send()` 下游）——症状 A 的主根因假设成立 |
| 发送端 `ack` 占大头 | 卡在停等窗口，即**对端消化不过来**，去看对端的 recv 探针 |
| 接收端 `wait` 占大头、其余平坦 | 被链路饿着，瓶颈**不在**接收端工作量 |
| 接收端 `write` 随窗口单调增长 | 症状 B 出在闪存侧（H1），代码无关 |
| 接收端 `ckpt` 占比高 | SQLite 未开 WAL 的代价（修复 #7，实测 Android 40×） |
| 桌面日志出现「支路缓冲已满」 | **症状 A 定案**：udp_mux 丢包 → SCTP cwnd 塌陷。走修复 #3/#4 |

### 7.3 四个案例

> ⚠️ **症状 A 要看桌面的日志，不是浏览器的。** `udp_mux` 只存在于 **listener 侧**
> （`upgrade.rs:279-285`：只有 `socket: Some(..)` 才套 `MuxedRuntime`）。浏览器是
> dialer，它那边没有这个组件。浏览器 console 只提供发送端的四阶段分布。

| # | 案例 | 看哪里 | 判什么 |
|---|---|---|---|
| **A1** | 浏览器 → 桌面，≥300 MB，局域网 webrtc-direct | 桌面日志文件 + 浏览器 console | `grep 支路缓冲已满` → 丢包定案；console 的 `role=send` → `write` 占比 |
| **B1** | 桌面 → 手机，6 GB | 手机日志（设置→关于→导出日志） | `role=recv` 各窗口的 `write` / `ckpt` / `wait` **趋势** |
| **B2** | **手机 → 桌面，同样 6 GB**（反向，手机只读不写） | 桌面日志 | 劣化消失 ⇒ 闪存侧(H1)；劣化保留 ⇒ 热节流/链路(H2) |
| **B3** | 桌面 → 桌面，≥1 GB，同一条物理网络 | 两端日志 | 补上 §1.4 缺的那条对照基线 |

日志位置：桌面 macOS `~/Library/Logs/com.yexiyue.swarmdrop/swarmdrop.<日期>.log`、
Windows `%APPDATA%\com.yexiyue.swarmdrop\logs\`；移动端走**设置 → 关于 → 导出日志**；
Web 端是浏览器 console（`swarmdrop_transfer` 已在 filter 里）。

### 7.4 探针本身的约定

- **不要**把它做成 feature 门控或 `debug_assertions` 门控——这类问题只在用户机器上出现，
  关着的探针等于没有。开销是每块 4–6 次 `Instant::now()`，相对 256 KiB 的真实工作量可忽略。
- 时间源必须是 `n0_future::time::Instant`（wasm 上 `std::time::Instant` 是 panic）。
- 汇总由 `Drop` 打而非调用方显式收尾：传输的失败路径有十几个 `?`，而**失败的会话恰恰
  最需要这份数据**。
