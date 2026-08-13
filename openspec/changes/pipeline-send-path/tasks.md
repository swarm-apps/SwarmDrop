## 1. 纯函数与常量（无依赖，先落）

- [x] 1.1 在 `crates/transfer/src/lib.rs` 加 `blocks_in_range(offset, length) -> impl Iterator<Item = (u64, usize)>`，挨着 `calc_total_chunks` / `is_chunk_aligned_range`；`length == 0` 产出恰好一个 `(offset, 0)`，否则按 `CHUNK_SIZE` 切分、尾块可短
- [x] 1.2 给 `blocks_in_range` 写单测：整块 / 跨多块 / 尾块短 / 零长产出一块 / 非零长绝不产出空块
- [x] 1.3 在 `crates/transfer/src/probe.rs` 加发送端两组阶段常量：`SEND_LABELS = ["read","proof","enqueue"]`（`SEND_READ` / `SEND_PROOF` / `SEND_ENQUEUE`）与 `SEND_FRAME_LABELS = ["queue","write","ack","rest"]`（`SEND_FRAME_QUEUE` / `SEND_FRAME_WRITE` / `SEND_FRAME_ACK` / `SEND_FRAME_REST`），并加 `SendProbe = StageProbe<3>` / `SendFrameProbe = StageProbe<4>` 两个类型别名
- [x] 1.4 在 `probe.rs` 的模块文档里补发送端两条线的判读表（`send.enqueue` 大 = 网络顶住了；`send-frame.queue` 大 = 备块跟不上），与已有的接收端那段并列
- [x] 1.5 把新常量加进 `stage_indices_stay_aligned_with_their_labels`

## 2. 发送端拆两条路径

- [x] 2.1 在 `sender.rs` 定义 `PreparedBlock { range: FileRange, proof: Vec<u8> }` 与 `PREPARE_QUEUE_CHUNKS: usize = 2`，常量文档写明「深度由抖动决定、不由窗口决定」（对照接收端 `DIGEST_QUEUE_CHUNKS` 的不同判据）
- [x] 2.2 加 `SenderActor::validate_fetch_plan(&[FileRange]) -> AppResult<()>`：文件存在 / range 溢出 / 越过文件末尾 / **非空文件的 `length == 0` 直接拒**；文档指向接收端同名方法的「一次计划校验一次」
- [x] 2.3 写自由函数 `encode_blocks(files, access, plan, cancel, queue)`：逐 range 走 `blocks_in_range`，每块 `read_source_chunk` → 长度校验 → `encode_proof` → 入队；三次 `probe.lap`；文档写明「签名里没有流/进度/事件，不对称于接收端是因为依赖集不同，别去统一它」
- [x] 2.4 写 `SenderActor::run_frame_loop(stream, epoch, rx, probe)`：独占流，取块 → `write_frame(BlockData)` → 满窗 `sync_window` → 进度簿记与事件投递（记入 `rest`）→ `block_done`；`WindowPacer` 是它的局部变量
- [x] 2.5 改 `run_data_channel`：写 Hello → 建队列 → `futures::future::join(encode_blocks, run_frame_loop)` → `frame_result?` → `encode_result?` → 写 Finish → 读 Finish/Abort；两条注释写死 D3（为什么不是 `try_join`：半截帧 + Abort ⇒ 归因丢失）与 D5（Finish 为什么必须在两条 result 之后）
- [x] 2.6 删除 `write_block`、`write_block_paced`、`write_range` 三个方法及 `write_block_paced` 上的 `#[expect(clippy::too_many_arguments)]`
- [x] 2.7 `run_data_channel` 与 `prefer_remote_abort` 泛型化到 `S: AsyncRead + AsyncWrite + Unpin`，`sync_window` 的既有泛型签名保持不变

## 3. 测试

- [x] 3.1 把 `sender_stops_after_one_window_until_peer_acks` 改为端到端驱动 `run_data_channel`（`StalledPeer`），断言写出的帧是 `Hello` + 恰好 `WINDOW_CHUNKS` 个 `BlockData` + 1 个 `Window`，且**没有** `Finish`；注释说明「备块端会跑前 2 块，但写出去的帧数不受队列影响」
- [x] 3.2 把 `sender_resumes_after_each_window_ack` 同样提升到 `run_data_channel`（`AckingPeer`），断言三次满窗三次确认、全部块写出
- [x] 3.3 新增护栏 `encode_failure_never_writes_finish`：`MemorySource` 注入第 N 块读失败，断言 `run_data_channel` 返回 Err **且流上不含 `Finish` 帧**（D5 的机器守卫）
- [x] 3.4 新增 `frame_error_wins_over_encode_error`：对端在首块后即断流，断言最终错误来自发帧路径而不是「发帧端已退出」这类次生文案（D4）
- [x] 3.5 新增 `validate_fetch_plan` 的单测：未知 file_id / 溢出 / 越界 / 非空文件零长 range 全部拒；空文件零长 range 放行
- [x] 3.6 **（计划外，但这是本次的中心不变量）** 新增 `pipelines_reading_ahead_of_writing`：
      对端把写按住不放直到源文件被读过第二次，**串行实现在这里必然死锁**。
      没有它，把两条循环合回一条能通过其余全部测试——流上的字节一模一样，只是慢一倍
- [x] 3.7 新增 `empty_file_still_sends_one_block`：零字节文件必须真的发出那个空块

## 4. 门禁

- [x] 4.1 `cargo fmt --all` + `cargo clippy -p swarmdrop-transfer`（新代码零 warning；仅剩 `checkpoint.rs:323` 一条**存量**告警，未在本次 diff 内）
- [x] 4.2 `cargo check --workspace --all-targets` + `cargo test --workspace` 全绿
- [x] 4.3 `./scripts/check-wasm.sh --clippy` 通过（transfer 在 wasm 七 crate 内，`--clippy` 是硬失败）
- [x] 4.4 逐条自审（未派 agent）：`blocks_in_range` 从有状态 `from_fn` 简化成 `(0..n).map`、
      降为 `pub(crate)`（前置条件与守卫都在本 crate 内）、修掉测试里跨 await 持锁、
      核对 `full_fetch_plan` 不会被新校验误拒、确认在途量上限仍是 4 MiB（队列在本进程内，不上线）
- [x] 4.5 **对新护栏做变异测试**：把 `pipelines_reading_ahead_of_writing` 的门槛调到永远开不了，
      确认它变红（`Elapsed`），再调回来——防这条测试自己假绿

## 5. 文档与知识库

- [x] 5.1 更新 `dev-notes/research/2026-08-12-webtransport-field-test.md`：把「剩下的：接收端流水线化了，发送端没有」改成已落地，并把「读一次探针」那一节改成本次新增的两条探针的判读表
- [x] 5.2 在 `dev-notes/knowledge/net-kernel.md` 里把发送方向那条负债更新为「已流水线化，天花板是 `proof`（wasm 单线程同步）」
- [x] 5.3 在 `dev-notes/blogs/transfer-throughput/` 追加一篇，记「为什么否掉双缓冲」与「探针拆两条才答得了流水线满没满」

## 6. 验收（需要真机，不阻塞合入）

- [ ] 6.1 Web 端发送 2 GB，Chrome console 读 `send` 与 `send-frame` 两条 `传输探针` 行，记下 `proof` 占比
- [ ] 6.2 按 `proof` 占比决定是否开 open question 里的两条后续（非 validated encode / wasm `simd128`）
