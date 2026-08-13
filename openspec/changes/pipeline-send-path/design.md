## Context

发送端热路径当前形态（`crates/transfer/src/actor/sender.rs`）：

```
run_data_channel
  write Hello
  for range in plan:
      write_range
        ├ 找文件、校验 range 越界
        ├ 空文件特例：write_block_paced(0, 0)
        └ while offset < end:
            write_block_paced
              ├ write_block
              │   read_source_chunk().await   → lap(READ)
              │   encode_proof()              → lap(PROOF)
              │   write_frame().await         → lap(WRITE)
              │   进度簿记 + 事件投递          → 【不入任何桶】
              └ 满窗则 sync_window().await    → lap(ACK)
  write Finish
  read Finish / Abort
```

接收端在 2026-08-10 已经拆成收帧 ‖ 消化两条并发路径（`receiver.rs:407` 的 `join` +
`DIGEST_QUEUE_CHUNKS` 有界队列），发送端保持原样。本次把发送端改成它的镜像。

**硬约束（不可协商）**：

1. **整条流不 split**。`futures::AsyncReadExt::split` 的 BiLock reader half 在 wasm 下、
   数据到达 muxer 后不唤醒读端（native 多线程掩盖，浏览器单线程显形）。接收端拆两条路径
   时也没有 split——那正是选「一条流 + 一条队列」的理由。
2. **不 spawn**。`join` 在同一任务内驱动两条 future，不要求 `Send`，wasm 单线程成立。
3. **wire 零改动**。对端看到的帧序与窗口边界必须逐字节相同。

## Goals / Non-Goals

**Goals:**

- 让「读源 + 建 proof」与「写流」在时间上重叠，把浏览器发送方向那段本可隐藏的 CPU/IO
  藏到网络时间背后。
- 让**探针能回答「流水线满没满」**——改完之后 `send.enqueue` 与 `send-frame.queue`
  互为对方的背压读数，与接收端的判读方式一致。
- 收掉发送端因串行结构而存在的三处冗余（`write_block_paced`、逐 range 的重复校验、
  空文件三重合取特例）。
- 校正 `transfer-data-plane` spec 中已漂移一整代的四处描述。

**Non-Goals:**

- **不动 blake3 / bao 的计算量**。`encode_ranges_validated` 每块重算一遍叶子哈希这件事
  （与 prepare 阶段建 outboard 重复）留给后续，判据是本次改完后 `proof` 桶的实测占比。
- **不开 wasm `simd128`**。同上，先拿到 `proof` 的实际数字。
- **不改流控形态**。停等窗口保持 `WINDOW_CHUNKS = 16`，不改成滑动窗口。
- 不动接收端。

## Decisions

### D1：两条循环 + 有界队列，而不是双缓冲 `join`

**取**：备块循环 → `mpsc` 有界队列 → 发帧循环，`futures::future::join` 驱动。

**弃**：在原循环里直接 `join(备下一块, 发当前块)` 的双缓冲。它行数更少、取消更简单、
没有「错误谁先抛」的规则，稳态重叠效果与深度 1 的队列**完全相同**。

**否掉它的理由只有一条，但足够**：双缓冲把 `read` 与 `proof` 关进同一个 future，探针
再也分不开这两段——而「`proof` 是不是这次改动的天花板」正是唯一悬着的问题
（见 D7）。为省二十行代码换掉答案，不划算。次要理由：备块与发帧两个关注点重新揉回
一个函数，模块划分反而退回去。

### D2：队列深度的判据与接收端不同，取 2

接收端的 `DIGEST_QUEUE_CHUNKS = WINDOW_CHUNKS`（16）是**硬要求**推出来的：队列要装得下
整窗，收帧循环才能立即回 `Window` 放行下一窗。

发送端**没有这条要求**——窗口节奏由本端 `WindowPacer` 决定，不经队列。深度的唯一职责是
盖住「备一块」与「发一块」之间的抖动，稳态下 1 就够（一块在写、一块在备），取 2 留一格。

再深只有一个假想收益：窗口 RTT 期间让备块跑得更远。但 `ack` 从来不是大头（一窗 4 MiB，
2 GB 只停 512 次，局域网下 < 1.5 s / 220 s）。**没有证据就不加深度**——每格是常驻
~264 KiB，移动端要付。

`PREPARE_QUEUE_CHUNKS = 2`，判据写进常量文档：**深度由抖动决定，不由窗口决定**。

### D3：`join` 而非 `try_join`——发送端的后果是流上的半截帧

接收端选 `join` 是为了避免在 `publish_file` 中途被 drop 导致重复发布。发送端的后果不同、
但更硬：

`try_join` 会在备块循环出错时**就地 drop 正在 `write_frame` 的发帧循环**，流上留下半截帧。
随后 `run_data_channel` 往同一条流写 `Abort`，对端读到的是「残帧 + Abort」的字节流，
解码成一句与真因无关的垃圾错误。**真因当场丢失。**

所以这里 `join` 是正确性要求，不是取舍。

### D4：错误顺序 —— 发帧端先抛

```rust
let (encode_result, frame_result) = join(...).await;
frame_result?;      // 发帧端的错误先抛
encode_result?;
```

- 备块端失败 ⇒ `tx` drop ⇒ 发帧端 `rx.next()` 得 `None` ⇒ 正常返回 `Ok`。此时真因在
  `encode_result`。
- 发帧端失败 ⇒ `rx` drop ⇒ 备块端 `tx.send()` 失败 ⇒ 返回次生错误「发帧端已退出」。
  此时真因在 `frame_result`。

两种情形下「先抛 `frame_result`」都落在真因上。这与接收端「消化端先抛」是**同一条规则的
两次应用**：抛那条不是在观察对方退场的。

### D5：`Finish` 只能写在 join 之后、两条 result 都 `Ok` 之后

备块端中途失败会让发帧端「正常」收敛（队列关了）。若发帧循环内部自己写 `Finish`，
就等于在只发了一半的情况下对端宣告「传完了」——接收端的 `ensure_files_complete` 会
在断言上报错，但那已经是错误的归因路径。

接收端有一条同形规则（`ensure_files_complete` 与 Finish 确认都在 join 之后，
理由是「收帧读到 Finish 只说明对端不再发了，队列里可能还压着块」）。

### D6：备块循环做成自由函数——本次刻意与接收端不对称

```rust
async fn encode_blocks(
    files: &[PreparedFile],
    access: &dyn FileAccess,
    plan: &[FileRange],
    cancel: &CancellationToken,
    queue: mpsc::Sender<PreparedBlock>,
) -> AppResult<()>
```

它碰不到流、进度、事件总线、协议——**不是靠约定，是靠签名里没有**。接收端的
`run_digest_loop` 做不到这一点（它要 store / events / progress / bitmaps / sinks，
自由函数得吃八个参数），所以它留在 `&self` 上是对的。

**不对称的来源是真实依赖集不同，不是风格不统一。** 这条要写进代码文档，否则下一个人
会把它「统一」回去。

### D7：探针拆两个，并补上 `rest` 桶

`probe.rs` 的模块文档已经写死判据：**一个探针横跨两条并发路径会破掉「各阶段之和 = 壁钟」
这个判读前提**（两条路径的耗时会重叠，加总必然超过壁钟，「差值 = 未计入开销」这条读法
就失效了）。所以必须拆：

| 探针 | role | 阶段 |
|---|---|---|
| 备块 | `send` | `read` · `proof` · `enqueue` |
| 发帧 | `send-frame` | `queue` · `write` · `ack` · `rest` |

判读表与接收端逐字对称：

- `send.enqueue` 占大头 → **网络顶住了**（背压真的在起作用）
- `send-frame.queue` 占大头 → **备块跟不上**（读盘 or blake3）
- 两者都小 → 流水线满了，瓶颈在别处

`rest` 是新增的桶：进度簿记、`update_file_chunk`、事件投递今天完全不在任何桶里，
只表现为「占比之和 < 100%」。接收端早有 `DIGEST_REST`，这是补齐。

**role 命名规则**：`*-frame` = 独占流的那条，裸名 = 干活的那条。与 `recv-frame` / `recv`
同规则，于是四条日志行的读法是一套。

### D8：校验前移，空文件特例随之消解

`write_range` 现在逐 range 做越界校验。前移成 `SenderActor::validate_fetch_plan`
（与接收端同名同位，它的文档已经写了「一次计划校验一次，好过每块撞一次」），并加上
接收端已有的那条：**非空文件的 `length == 0` range 直接拒**。

校验前移之后，「`length == 0` ⟹ 发一个空块」这条规则在整个定义域上都成立，不再需要
`file.size == 0 && range.offset == 0 && range.length == 0` 这个三重合取——那三个条件
的合取只是在补校验缺位。

range → 块的展开抽成 `lib.rs` 的纯迭代器 `blocks_in_range`，挨着已有的
`calc_total_chunks` / `is_chunk_aligned_range`（本仓 chunk 数学的既有落点），可单测。

### D9：`write_block_paced` 删除

它的文档写着「存在的理由只有一个：`write_range` 有两条写块路径，窗口簿记漏在任何一条上
都会让在途量悄悄越过上限」。发帧循环成为唯一写块路径之后，`WindowPacer` 只有一个调用点，
这个方法连同它的 `#[expect(clippy::too_many_arguments)]` 一起消失。

### D10：`run_data_channel` 泛型化

现在写死 `P2pStream`，于是两条窗口护栏测试只能驱动 `write_range`（它是泛型的）。
改成 `S: AsyncRead + AsyncWrite + Unpin` 之后，测试可以端到端驱动整条通道，
连 Hello 与 Finish 一起覆盖。`prefer_remote_abort` 同样泛型化。

## Risks / Trade-offs

| 风险 | 缓解 |
|---|---|
| **停等窗口的护栏被改坏**。`sender_stops_after_one_window_until_peer_acks` 是「删掉窗口」唯一的机器守卫——去掉窗口在 native↔native 之间照样跑得通，只有浏览器接收端会在几 MiB 处被撑爆，那是跑不进 CI 的失效模式 | 两条窗口测试必须保留并改为端到端驱动。备块循环会比发帧循环跑前 2 块，**但写出去的帧数必须仍是恰好 `WINDOW_CHUNKS`**——断言看的是写出的字节，不受队列影响 |
| **备块端失败却写出了 Finish**（D5 的失效形态）：对端把半截会话判成完成 | 新增一条护栏测试：备块端注入读失败 ⇒ 流上不得出现 `Finish` 帧 |
| **`try_join` 被后人改回去**（D3） | 代码注释写明后果（半截帧 + Abort ⇒ 归因丢失），与接收端那条并列 |
| **探针阶段下标与标签错位**：数字依旧自洽、只是归因全错，真机日志里没有可辨识的形状 | `probe.rs` 已有 `stage_indices_stay_aligned_with_their_labels`，新常量一并加入 |
| **收益可能远小于预期**：wasm 单线程下 `encode_proof` 是同步 CPU，谁也压不住它。若 `proof` 占大头，本次改动几乎无效 | 这是**已知且接受**的天花板（见下）。即便如此，改完之后探针能直接给出 `proof` 的占比，而现在只能推 |
| 发送侧常驻内存 +~0.5 MiB | 深度取 2 而非 16（D2） |

### 天花板的诚实估计

`join` 给的是**并发不是并行**。真正能重叠的只有「已经交出去、在我们线程之外跑的事」：

| 阶段 | 在哪跑 | 能否与我们的 CPU 重叠 |
|---|---|---|
| `read`（`File.slice().array_buffer()`） | 浏览器线程池 | ✅ |
| `write`（WebTransport 写流） | 浏览器网络栈 | ✅ |
| `proof`（blake3 + 拼 bao 切片） | **wasm 主线程，同步** | ❌ |

每块壁钟从 `read + proof + write` 降到约 `proof + max(read, write)`：

- `read≈8 / proof≈8 / write≈13`（ms）⇒ 29 → ~21 ⇒ 9 → **~12.4 MB/s**
- `read≈16 / proof≈2 / write≈11` ⇒ 29 → ~18 ⇒ **~14.5 MB/s**
- `proof` 独占 ⇒ 收益接近 0，该动的是 wasm blake3（本仓 wasm 构建没开 `+simd128`，是纯标量）

**这是估算不是结论。** 验收靠改完之后读那两条探针。

## Open Questions

1. `encode_ranges_validated` 每块重算叶子哈希（与 prepare 阶段建 outboard 重复）——换成
   非 validated 版本能省一遍全量 blake3，代价是源文件传输中被改动时错误形态从
   「源已变更」退化成「对端验签失败」。**判据是本次改完后 `proof` 桶的实测占比**，不在本次范围。
2. wasm `+simd128` 是否能让 blake3 实质提速，需要单独 spike。同样由 `proof` 占比触发。
