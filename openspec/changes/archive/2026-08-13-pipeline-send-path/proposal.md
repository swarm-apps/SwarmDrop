## Why

发送端的热路径至今是一条严格串行链：`read_source_chunk().await → encode_proof() → write_frame().await`。
2026-08-10 那轮流水线化只动了接收端（收帧 ‖ 消化，拿回 50%），**发送端没被碰过**。

串行本身两端都有，但代价只在浏览器那侧显形：Android 发送时「读 + 算」是原生文件读（~GB/s）
加 NEON blake3，相对 20 MB/s 的网络几乎免费；浏览器发送时「读」是
`File.slice().array_buffer()` 的 promise 往返、「算」是无 SIMD 的 wasm blake3，
**这段时间完全不与网络写重叠**。2026-08-12 的真机实测正好落在这个形状上——
手机发 20 MB/s、浏览器发 9 MB/s，同一条局域网、同一个传输
（[`2026-08-12-webtransport-field-test.md`](../../../dev-notes/research/2026-08-12-webtransport-field-test.md)）。

还有一个同等重要的理由：**现在的探针答不了「瓶颈在哪一侧」这个问题**。串行路径上
read/proof/write/ack 四个桶都是「真实工作」，看不出哪一段本可以被藏起来。拆成两条并发
路径之后，两条探针的 `enqueue` / `queue` 直接互为对方的背压读数——这是接收端已经验证过的
判读方式。

## What Changes

- **发送数据面拆成两条并发路径**：备块循环（读源 + 建 bao proof）与发帧循环（独占流 +
  停等窗口 + 进度），中间一条有界队列，`futures::future::join` 在同一任务内驱动。
  **不 spawn、不 split 流**，与接收端同构，wasm 单线程一样成立。
- **备块循环做成自由函数**，签名里只有 `files` / `file_access` / `cancel` / `queue`——
  它碰不到流、进度、事件总线，靠签名而非约定。
- **`SendProbe` 拆成两个探针**：`send`（read / proof / enqueue）与
  `send-frame`（queue / write / ack / rest）。顺带补上现在完全没入桶的 `rest`
  （进度簿记 + 事件投递，今天只表现为「占比之和 < 100%」）。
- **fetch_plan 校验前移**成一次性的 `validate_fetch_plan`，与接收端同名同位；
  range → 块的展开抽成 `lib.rs` 里的纯迭代器。
- **删除 `write_block_paced`**：它存在的唯一理由（「`write_range` 有两条写块路径」）
  随发帧循环成为唯一写块路径而消失。
- **`run_data_channel` 泛型化**到 `S: AsyncRead + AsyncWrite + Unpin`，让两条窗口护栏
  测试从 `write_range` 提升到端到端驱动整条通道（覆盖 Hello 与 Finish）。
- **校正 `transfer-data-plane` 已漂移一整代的条款**：该 spec 仍在描述「加密并连续发送」、
  「稀疏 `Ack`」、「`BlockRequest` 补洞」、「背压由传输层承担」、「接收方读写分离避免死锁」，
  而现行实现是明文（应用层加密已整块删除）、`Window` 停等流控、整条流刻意**不** split。

**不变的**：wire 格式、帧序、窗口边界行为对对端**逐字节相同**——本次不动协议。

## Capabilities

### New Capabilities

无。本次改的是既有能力的实现结构与它的可观测性判据，没有引入新能力。

### Modified Capabilities

- `transfer-data-plane`: 发送方推送机制的条款由「串行读→算→写、背压由传输层承担」改为
  「备块 ‖ 发帧两条并发路径 + 应用层 `Window` 停等」；同时校正 `Ack` / `BlockRequest` /
  应用层加密 / 读写分离这四处已与实现分叉的描述；新增「Finish 只在两条路径都成功后写」
  与「发送端探针必须能分辨流水线是否满」两条要求。

## Impact

| 面 | 影响 |
|---|---|
| `crates/transfer/src/actor/sender.rs` | 主要改动面：拆两条循环、删 `write_block_paced` / `write_range`、`run_data_channel` 泛型化、测试重写 |
| `crates/transfer/src/probe.rs` | 新增 `SEND_*` 一组阶段常量与 `SendFrameProbe` 类型，`SendProbe` 语义改为备块侧 |
| `crates/transfer/src/lib.rs` | 新增纯迭代器 `blocks_in_range`，挨着 `calc_total_chunks` / `is_chunk_aligned_range` |
| 三端 | 桌面 / 移动 / Web 同一份 `crates/transfer`，改动自动生效；Web 端是本次收益的目标 |
| wire / 协议 | **零改动**，无需 bump `TRANSFER_DATA_PROTOCOL` |
| 内存 | 发送侧常驻多出 `PREPARE_QUEUE_CHUNKS × ~264 KiB`（取 2 ⇒ ~0.5 MiB） |
| 门禁 | `cargo test --workspace` + `./scripts/check-wasm.sh --clippy`（transfer 在 wasm 七 crate 内） |
