# wasm-debugging：一个 bug 的十一轮调试复盘

让浏览器（`crates/web`）成为真正的 SwarmDrop 传输端——**复用桌面字面同一份
`swarmdrop-transfer` 逻辑**。native e2e 16/16 全绿、五 crate wasm 编译全过、控制面（配对 /
offer / accept）在浏览器全通——**可数据面（文件字节）传不过去，卡住且无任何错误。**

这个系列，是把「浏览器数据面静默」这个 bug 从症状到逐字节一致的**完整调试过程**写成可学习
的实战复盘。花了**十一轮真实浏览器实测**（浏览器自动化驱动两个页面经 circuit relay 互传），
逐层剥开四道**编译期完全看不见**的 wasm 运行时门。每修一层，卡点前移一步——直到 OPFS 落盘
逐字节一致（`08c8ab0a`，2MB / 2097152 bytes / elapsed 93ms）。

## 篇目

| # | 篇 | 讲什么 |
|---|---|---|
| 00 | [症状：全绿，却传不过去](00-the-symptom.md) | 为什么这是最难的一类 bug——所有传统信号都是绿的 |
| 01 | [门 1：`std::time` 在 wasm 直接 panic](01-gate-1-std-time.md) | 最浅的一道，热身：能编译≠能运行 |
| 02 | [门 2：futures split 的 reader half 不唤醒](02-gate-2-split-wakeup.md) | 第一次撞上 wasm 单线程唤醒语义 |
| 03 | [门 3：accepted 流跨任务 move 的 lost-wakeup](03-gate-3-cross-task-wakeup.md) | 最烧脑的一道：waker 交接缝里丢失的唤醒 |
| 04 | [门 4：finalize 永久 pending 的真凶](04-gate-4-secure-context.md) | 反转：不是 transfer 的 bug,是 Web 平台环境 |
| 05 | [方法论：怎么调一个「全绿却不工作」的 wasm bug](05-methodology.md) | 把整套打法提炼成可复用招式 |

建议按序读：00 铺症状与心智，01→04 按**真实踩到的顺序**逐门递进（每篇结尾的「卡点前移」
接下一篇的开头），05 收束方法论。

## 四道门总览

| 门 | 症状 | 根因 | 修复 |
|---|---|---|---|
| **1** `std::time` panic | prepare 阶段直接炸,console 有 `time not implemented on this platform` | `wasm32-unknown-unknown` 无系统时钟,`Instant::now()` 是**运行时 panic**(非编译错) | transfer 5 处 `Instant` 全换 `n0_future::time::Instant`(native=tokio,wasm=web_time) |
| **2** split reader half 不唤醒 | 字节到了 muxer,接收端读循环**首帧都拉不到**,静默挂起 | `futures` `split()` 的 BiLock reader half,在 wasm 单线程下字节到达后**不被唤醒**(native 多线程掩盖) | 去 `split()`,整条流**顺序读写**(读写本不重叠,split 纯属多余) |
| **3** 跨任务 move 的 lost-wakeup | 读到首帧后,后续帧**永久 Pending** | 流在 handler 任务读 Hello 后 `move` 进 spawn 的任务;后续帧的 wake 打给**旧 waker**,新任务注册的新 waker 再无事件触发 | 入站流**不跨任务**,在读 Hello 的同一 handler 任务里 `await` 到结束(iroh「形状 A:在 accept 里跑完」) |
| **4** secure-context gating | 3 块全收全验,**卡在 finalize 落盘** | 页面开在 `http://` 私网 IP,非 secure context → `navigator.storage` 不存在 → `getDirectory()` 的 `JsFuture` **永久 pending** | 预检 `isSecureContext` + 明确报错;每个 OPFS `await` 套 5s timeout。换 `http://127.0.0.1` 即通 |

四道门有一个共同的、危险的特征:**除门 1 会 panic 外,门 2/3/4 全是静默永久挂起**——最坏
的失败模式,无错误、无超时。没有一道能被 `cargo test` / `cargo check` / 类型系统拦住。

## 核心教训

> **「native 测试全绿 + 五 crate wasm 编译全过 + 控制面全通」= 零保证。**
>
> wasm 单线程 + Web 平台的运行时语义,是一个编译期完全看不见的第三维:时钟能不能读、单线程
> 下 waker 会不会丢、Web 平台 API 在当前 context 存不存在——只能真实浏览器逐层剥。

## 相关材料

- 知识库浓缩:[`dev-notes/knowledge/libp2p-wasm.md`](../../knowledge/libp2p-wasm.md)
  「四道运行时门」一节(本系列是它的故事化展开)、`n0-future`/JoinSet 语义、编译探针结论。
- 修复代码:`crates/transfer/src/actor/{receiver.rs,sender.rs}`、
  `crates/transfer/src/wire/data_plane.rs`、`crates/web/src/file_access.rs`、
  `crates/web/src/node.rs`。
- 提交:`git show 08c8ab0a`(feat(web) 浏览器传输端 + wasm 四道门攻克)。
- 姊妹系列:同类 bug 复盘 [`presence-two-bugs-one-root-cause.md`](../pairing-transfer/presence-two-bugs-one-root-cause.md)
  (libp2p 连接生命周期);secure-context 平台机制见 `browser-platform/` 系列;n0-future /
  Router 语义见 `network-kernel/` 系列。
