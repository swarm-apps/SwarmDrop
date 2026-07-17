# 地基三件套：n0-future / n0-watcher / n0-error

iroh 1.0.2 · n0-future 0.3.2 · n0-watcher 1.0.0 · n0-error 1.0.0 · 调研日期 2026-07-17

> **三者都不在 iroh-study 里**（它们是 crates.io 依赖，但各自有独立仓库快照）。n0-watcher 源码在 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/n0-watcher-1.0.0/src/lib.rs`。iroh 在 `iroh/iroh/src/lib.rs:291` 做了 `pub use n0_watcher::Watcher;`。

## 一句话选型：三者定位完全不同，不要打包决策

| 库 | 结论 | 理由 |
|---|---|---|
| **n0-future** | **想上 Web 就换，否则纯噪音** | native 上是 tokio 的纯 re-export，替换是**零行为变更**的机械操作，唯一价值是把未来 Web 端的 `#[cfg]` 提前收敛进库内。**移动端（uniffi）走 native 分支，从替换里一点好处都拿不到** —— 它是一张为 Web 端提前买的期权 |
| **n0-watcher** | **被动接受** | 它是 iroh 公开 API（`iroh::Watcher`），用 iroh 就躲不掉。但 `initialized()` 有两个**已证实的真陷阱** |
| **n0-error** | **多半不用** | iroh 错误同时实现了 `std::error::Error`，thiserror 可直接 `#[from]`；而它的核心卖点 location **生产默认不采集** + 强制放弃 `#[derive(Debug)]` |

---

# n0-future

- **成熟度**：**production**
- **依据**：
  - version 0.3.2；CHANGELOG.md 记 2026-01-07 发布；最后提交 2026-06-12（**仅 GitHub Actions 依赖 bump，非功能变更**）
  - iroh 1.0.2 以 `n0-future = "0.3"` 依赖（`iroh/iroh/Cargo.toml:37`）；**14 个仓依赖它**（iroh、iroh-blobs、iroh-gossip、irpc、sendme、iroh-c-ffi、iroh-ffi、iroh-docs、iroh-doctor、iroh-examples、iroh-experiments、iroh-address-lookups、iroh-dht-experiment、n0-watcher）
  - 本体 `wc -l src/*.rs` = **1506 行**（lib.rs 116 + maybe_future.rs 215 + task.rs 681 + time.rs 494），但**绝大部分是 `#[cfg(wasm_browser)] mod wasm`**（task.rs 的 wasm 实现占 13-618 行，time.rs 同理）—— **native 路径只有 task.rs 的 6 行 + time.rs 的 8 行 re-export**。风险面极小
  - ⚠️ 仍是 **0.x**：cargo 语义下 minor bump 即可 breaking。（注意：`grep -i breaking n0-future/CHANGELOG.md` **零命中** —— 0.3.0(2025-10-20) 条目下只有两条纯新增功能。同族的 n0-watcher CHANGELOG 会明确标 `[**breaking**]`，说明 n0 团队会标；n0-future 没标就是没标。「0.x minor 可能 breaking」是 **Cargo 语义的先验**，不是 CHANGELOG 的实证）
  - MSRV `rust-version = "1.85"`、edition 2021
- **入口**：`n0-future/src/task.rs`（29 行看完 native 全貌）；再看 `src/time.rs:1-15` 与 `src/lib.rs:22-27` 的 re-export 清单

## 它在 native 上**就是 tokio**

不是包装、不是适配层：

```rust
// src/task.rs:4-9
#[cfg(not(wasm_browser))] pub use tokio::spawn;
#[cfg(not(wasm_browser))] pub use tokio::task::{AbortHandle, Id, JoinError, JoinHandle, JoinSet};
#[cfg(not(wasm_browser))] pub use tokio_util::task::AbortOnDropHandle;

// src/time.rs:6-10
#[cfg(not(wasm_browser))] pub use tokio::time::{
    error::Elapsed, interval, interval_at, sleep, sleep_until, timeout,
    Duration, Instant, Interval, MissedTickBehavior, Sleep, Timeout,
};
```

`pub use` 按定义就是同一类型 —— `n0_future::time::Instant` **就是** `tokio::time::Instant`。

## wasm 分支只对 wasm32-unknown-unknown 生效

`n0-future/build.rs:5-8`：

```rust
cfg_aliases! { wasm_browser: { all(target_family = "wasm", target_os = "unknown") } }
```

全库所有 `#[cfg(wasm_browser)]` 分支都由这**一个 alias** 控制。iroh 用完全相同的 alias 命名。

**纠正一个容易产生的误判**：n0-future **不是「跨平台抽象」，是「浏览器兼容垫片」**。编译到 aarch64-apple-ios / aarch64-linux-android 时 target_family 不是 wasm，因此 **100% 走 `pub use tokio::*`**。

## 有对应物 / 无对应物

```rust
// ---- 有对应物 ----
tokio::spawn(fut)                   -> task::spawn(fut)
tokio::task::{JoinHandle,JoinSet,AbortHandle,JoinError,Id}
                                    -> task::{同名}
tokio_util::task::AbortOnDropHandle -> task::AbortOnDropHandle
tokio::time::sleep / sleep_until    -> time::sleep / time::sleep_until
tokio::time::timeout                -> time::timeout
tokio::time::interval / interval_at -> time::interval / time::interval_at
tokio::time::{Instant,Duration,Sleep,Timeout,Interval,MissedTickBehavior}
                                    -> time::{同名}
tokio::time::error::Elapsed         -> time::Elapsed
std::time::SystemTime               -> time::SystemTime   // wasm 上 std 的会 panic

// ---- 无对应物，且【不需要】改 ----
tokio::select! { .. }        // n0-future 根本没有 select；tokio 的 select! 是纯宏，
                             // 只吃 `macros` feature、不碰运行时，wasm 上照常编译
tokio::sync::{mpsc,oneshot,watch,broadcast,Mutex,RwLock,Semaphore}  // 纯用户态，wasm 可用

// ---- 无对应物，且【无法】上 wasm ----
tokio::task::spawn_blocking  // 浏览器没线程，n0-future 不提供
tokio::fs / tokio::net       // 同上
```

**`select!` 那条很省事**：在 n0-future 全仓 grep `select` 只有 3 处命中，且全是它自己文档/测试里在用 `tokio::select!`（`src/maybe_future.rs:24, 57, 200`）—— **库作者自己都没打算替换它**。

**反证在 iroh**：iroh 全仓用 `tokio::select!` 35 次，其中 `net_report.rs:384` 的 select! **位于 wasm 编译路径内**（该文件第 8 行是 `#![cfg_attr(wasm_browser, allow(unused))]`，即 wasm 下照常编译），而 `iroh/.github/workflows/ci.yml:330` 明确 `cargo build --target wasm32-unknown-unknown -p iroh`、:343 还跑 wasm 集成测试。iroh 的基础 tokio 依赖（`iroh/iroh/Cargo.toml:54-59`，无 target 限定即全平台生效）只开了 io-util/macros/sync/rt。

## 根部 re-export（不用再单独引 futures-*）

```rust
use n0_future::{Stream, StreamExt, Future, FutureExt, Sink, SinkExt,
                TryFutureExt, TryStreamExt, io, pin, ready, stream, future,
                MaybeFuture, Either};
// futures_buffered::* 全量：FuturesUnordered / FuturesOrdered / MergeBounded /
// MergeUnbounded / BufferedStreamExt / join_all / try_join_all / IterExt ...
use n0_future::boxed::{BoxFuture, BoxStream};  // native=Send, wasm=!Send（关键差异）
```

**io / Stream 用的是 futures 生态的 trait，不是 tokio::io**：
- `src/lib.rs:25` `pub use futures_lite::{io, pin, ready, stream, Future, FutureExt, Stream, StreamExt};`
- `futures-lite-2.6.1/src/io.rs:21` `pub use futures_io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite};`

→ **`n0_future::io::AsyncRead` 就是 `futures_io::AsyncRead`。** 已经站在 futures 生态（例如从 libp2p 迁来的代码）的 `AsyncRead`/`AsyncWrite` 实现**一行不用改**，也降低了后续接 iroh 流的摩擦。

## ⚠️ wasm 版 JoinSet 有作者自己标注的 TODO 级缺陷

`src/task.rs:113-122` doc 原文：

> *"If you `.spawn` a new task onto this `JoinSet` while the future returned from this is currently pending, then this future will continue to be pending, even if the newly spawned future is already finished. **TODO(matheus23): Fix this limitation.** Current work around is to recreate the `join_next` future when you newly spawned a task onto it. This seems to be the usual way the `JoinSet` is used *most of the time* in the iroh codebase anyways."*

native 端无此问题（直接用 `tokio::task::JoinSet`）。

**只在 Web 端才会咬人，但正好命中一种常见写法**：

```rust
select! {
    Some(r) = set.join_next() => ...,
    new = rx.recv() => set.spawn(...),      // ← wasm 下新 spawn 的任务可能永不被 join 出来
}
```

> ⚠️ **wasm 版 spawn 没有 Send 约束**（底层是 `wasm_bindgen_futures::spawn_local`）：自由函数 `spawn` 在 **`src/task.rs:606`** —— `pub fn spawn<T: 'static>(fut: impl IntoFuture<Output = T> + 'static) -> JoinHandle<T>`。
>
> （注意：`task.rs:72-74` 是 **`JoinSet::spawn`**（方法，签名带 `&mut self`），不是与 `tokio::spawn` 对位的自由函数 —— 两者结论一致，但按行号去读会找到方法而非函数。）
>
> JoinSet 是靠 `futures_buffered::FuturesUnordered` 自己实现的 shim（:44-49），还用了 `send_wrapper::SendWrapper`（:26/:259）。

## ⚠️ 「零行为变更」的精确版本

**类型等价成立**（`pub use tokio::*`），所以**默认运行时行为不变**，**不需要回归网络/传输逻辑**。

**但「二进制逐字节等价」是假的**：`n0-future/Cargo.toml` 在 native target 下**无条件启用 tokio 的 `test-util` feature**：

```toml
[target.'cfg(not(all(target_family = "wasm", target_os = "unknown")))'.dependencies]
tokio = { version = "1.28", features = ["rt", "time", "macros", "test-util"] }
```

若你的项目此前没启用过 `test-util`，引入 n0-future 会通过 **feature unification** 首次把 tokio 的**可 mock 时钟代码路径**编进生产包。默认 `start_paused=false`，运行时行为不变 —— **但二进制必然不同**。

## ⚠️ Instant 跨签名边界

native 下 `n0_future::time::Instant` 是 `tokio::time::Instant`（同一类型、零风险），**但真上 wasm 时它会变成 `web_time::Instant`**。所以把 Instant 用作**结构体字段**或**公开函数签名**的地方，在 Web 化那一刻会变成类型边界 —— 替换时要单独盯这些点。

---

# n0-watcher

- **成熟度**：**production**
- **依据**：
  - version 1.0.0；iroh 1.0.2 以 `n0-watcher = "1.0.0"` 依赖并在 `iroh/iroh/src/lib.rs:291` `pub use n0_watcher::Watcher;` —— **它是 iroh 公开 API 的一部分，用 iroh 就绕不开**
  - README 无免责声明
  - ⚠️ **1.0 非常年轻**：1.0.0-rc.0 才发于 2026-05-06、1.0.0 发于 2026-06-15。此前 0.6.0（2025-11-12）还在**破坏性地**把 `get` 拆成 `update`/`peek`；0.6.1（2026-02-04）修掉过 **waker 重复注册**与 **Weak 分配泄漏**两个真 bug
  - ⚠️ **最后提交 2026-07-09 是 dependabot 的 `chore(deps): bump actions/checkout from 6 to 7`** —— **最后功能性变更实为 2026-06-15 的 1.0.0 发布**
  - ⚠️ 源码内含 loom 并发测试（`#[cfg(watcher_loom)]`，见 src/lib.rs:77/86/863/1072/1075/1093/1095，Cargo.toml:35 check-cfg 已声明），但 `.github/workflows/` 下 **7 个 workflow 无一引用 loom** —— **loom 测试未进 CI**
  - MSRV `rust-version = "1.91"`（`Cargo.toml:13`，注释：*"Sadly this also needs to be updated in .github/workflows/ci.yml"*），与 iroh 1.0.2 一致 —— **这三个库里最激进的**
- **入口**：`n0-watcher/src/lib.rs`（单文件 1698 行）—— 先读 :241-395（Watcher trait 全部语义），**再读 :108-124（Nullable 陷阱）与 :736-762（InitializedFut 陷阱）**

## 心智起点：Watcher 是有损的状态采样，不是事件流

模块文档开门见山（`n0-watcher-1.0.0/src/lib.rs:3-5`）：

> A [`Watchable`] exists to keep track of a value which may change over time. It allows observers to be notified of changes to the value. **The aim is to always be aware of the *last* value, not to observe *every* value change.**

```rust
// n0-watcher lib.rs:147-172 —— 值没变就不通知
pub fn set(&self, value: T) -> Result<T, T> {
    let mut state = self.shared.state.write().expect("poisoned");
    let changed = state.value != value;
    let ret = if changed {
        let old = std::mem::replace(&mut state.value, value);
        state.epoch += 1;
        Ok(old)
    } else {
        Err(value)          // ← 值没变：原样退回，且下面不唤醒
    };
    drop(state);
    if changed {
        for watcher in self.shared.wakers.lock().expect("poisoned").drain(..) { watcher.wake(); }
    }
    ret
}
```

这是 Value 要求 `Clone + Eq` 的原因。`Map` 组合子同理，只在**映射后**的值变化时才触发（`lib.rs:651-660`）。

**两条硬约束：**
1. **没有「心跳」语义**——同一个值重复 set 不会再通知你
2. **中间值会被跳过**——`lib.rs:213-218` 明说「if the threads with the underlying Watchable change the value faster than the threads with the Watcher can keep up with, then it'll miss in-between values」

**所以绝不能用 Watcher 做「统计状态变化次数」或「捕获每一次 Connecting→Connected 翻转」。**

> **这是与 libp2p 最根本的心智差异。** libp2p 的 `SwarmEvent` 流是**无损、有序、每个事件必达**（你不 poll 它就在队列里等）。Watcher 是 **last-value-wins：不 poll 就被覆盖**。libp2p 里「订阅事件」的直觉套到 Watcher 上会导致丢状态。
>
> 模块文档 `lib.rs:63-70` 自己对比过 `tokio::broadcast`（带队列，不丢）和 `tokio::watch`（同为最新值语义）——**Watcher 属于后者**。

## API 一览（语义差异是重灾区）

```rust
use n0_watcher::{Watchable, Watcher as _};   // iroh 亦 re-export: use iroh::Watcher;

let w = Watchable::new(None::<u8>);   // T: Clone + Eq（Eq 用来判「变没变」）
let mut watcher = w.watch();          // -> Direct<T>

// —— 同步读 ——
watcher.peek()   -> &T      // 不刷新，零拷贝
watcher.update() -> bool    // 刷新，返回是否变了
watcher.get()    -> T       // = update() + peek().clone()
watcher.is_connected() -> bool

// —— 异步读 ——
watcher.updated().await      -> Result<Value, Disconnected>  // 完整 Value；断开=Err ✅
watcher.initialized().await  -> T   // ⚠️ 解包后的内层 T，不是 Value；断开=永久挂起 ❌
watcher.stream()             -> Stream<Item=Value>  // 首项=当前值；断开=None 正常结束 ✅
watcher.stream_updates_only()-> Stream<Item=Value>  // 跳过当前值

// —— 组合器 ——
watcher.map(|v| v.len())         // 只在【映射后】的值变化时才触发（内部去重）
watcher.or(other)                // -> Tuple<A,B>，任一变则一起吐
n0_watcher::Join::new(iter)      // -> Watcher<Value = Vec<T>>

// —— 写端 ——
w.set(v) -> Ok(old)  // 值变了，已唤醒 watchers
        -> Err(v)    // 值没变（T: Eq 判定），【不唤醒】—— Err 不代表失败
```

## 1.0.2 的公开 Watcher API 只有三个

穷举 `grep "pub fn .*->.*Watch"` 在 `iroh/iroh/src/` 下：

| API | 位置 | Value 类型 |
|-----|------|-----------|
| `Endpoint::watch_addr()` | `endpoint.rs:1270`（非 wasm）/ `:1297`（wasm） | `EndpointAddr` |
| `Endpoint::home_relay_status()` | `endpoint.rs:1384` | `Vec<RelayStatus>` |
| `Endpoint::net_report()` | `endpoint.rs:1433`（feature `unstable-net-report` 门控） | `Option<NetReport>` |

> ⚠️ **1.0.2 没有 `direct_addrs()`。** `direct_addrs` / `direct_addresses` / `conn_type` 在公开 API 里**都不存在**。历史轨迹有据可查：`CHANGELOG.md:1717`「Rename Endpoint::local_endpoints to direct_addresses」；`:631`「[breaking] Make direct_addresses always be initialised (#3505)」；`:441`「Remove Endpoint::conn_type (#3647)」。**现在直连地址只能通过 `watch_addr()` 返回的 EndpointAddr 里的 `TransportAddr::Ip(..)` 间接看到。**

> libp2p 的 `swarm.listeners()` / `SwarmEvent::NewListenAddr` 直接给你监听地址列表；iroh 把「直连地址 + relay 地址」打包成单个 `EndpointAddr` 让你 watch，不单独暴露 direct addrs。

## ⚠️ 核心陷阱：`initialized()` vs `updated()` 的双重不对称

### 不对称一：返回类型（一个解包一个不解包）

```rust
// n0-watcher lib.rs:306-315
fn initialized<T, W>(&mut self) -> InitializedFut<'_, T, W, Self>
where
    W: Nullable<T> + Clone,
    Self: Watcher<Value = W>,
{
    InitializedFut {
        initial: self.get().into_option(),   // ← future 创建时就同步求值
        watcher: self,
    }
}

// lib.rs:675-682 —— updated(): 完整 Value
impl<W: Watcher> Future for NextFut<'_, W> {
    type Output = Result<W::Value, Disconnected>;
}

// lib.rs:698-719 —— initialized(): 解包后的 T
impl<T: Clone + Eq + Unpin, V: Nullable<T> + Clone, W: Watcher<Value = V> + Unpin> Future
    for InitializedFut<'_, T, V, W>
{
    type Output = T;
}
```

**两个方法名字读起来像一对，返回类型却一个解包一个不解包。** 对 `Vec<T>` 的 watcher：

| 调用 | 拿到的 |
|------|--------|
| `w.initialized().await` | `T` —— **单个元素** |
| `w.updated().await` | `Result<Vec<T>, Disconnected>` —— **完整集合** |

**静默换了维度。**

> libp2p 里 `swarm.next().await` 永远返回同一个 `SwarmEvent` 类型，不存在「哪个等待方法决定了你拿到的是元素还是集合」这种事。

### 根因：`Nullable<Vec<T>>::into_option` 就是 `pop()`

```rust
// n0-watcher lib.rs:109-125
/// Abstracts over `Option<T>` and `Vec<T>`
pub trait Nullable<T> {
    fn into_option(self) -> Option<T>;
}
impl<T> Nullable<T> for Option<T> {
    fn into_option(self) -> Option<T> { self }
}
impl<T> Nullable<T> for Vec<T> {
    fn into_option(mut self) -> Option<T> { self.pop() }   // ← 关键
}
```

**`Nullable` 只为 `Option<T>` 和 `Vec<T>` 实现**，没有为 `BTreeSet` 等实现。

**`pop()` 不只是「判空」——它取的是最后一个元素，其余元素被静默丢弃。** 实测：`Watchable::new(vec![1u8,2,3]).watch().initialized().await` 返回 **`3`**，1 和 2 无声消失。（仓内测试 :1056 `assert_eq!(poll.unwrap(), 1u8)` 也印证返回值是解包后的 u8 而非 Option<u8>。）

**这在 iroh 上是真实可达的**：`Endpoint::home_relay_status()`（`endpoint.rs:1384`）签名正是 `impl Watcher<Value = Vec<RelayStatus>>`。写 `ep.home_relay_status().initialized().await` 会拿到**单个 RelayStatus** 而不是 Vec，多 relay 场景下另外几个 relay 的状态被静默丢弃，且**类型推断出来就是 RelayStatus，review 时看不出问题**。

**更隐蔽的是**：`initialized()` 在 Vec 上的真实语义是「**等到至少有一个元素**」（空 Vec → pop() → None → 继续等），**单 relay 场景下恰好正确** —— 所以这个 bug 会一直潜伏到接入第二个 relay。

> 补充：iroh 自己就在 Vec watcher 上推荐 initialized() —— `iroh/iroh/src/socket.rs:486-487` 的文档：*"Note that this can be used to wait for the initial home relay to be known using [`Watcher::initialized`]"*。（该 `Socket::home_relay()`(:488) 本身是 `pub(crate)` 且 `iroh/iroh/src/lib.rs:268` 是私有的 `mod socket;` —— **下游调不到它**，所以它不是你会踩的坑，但它证明这个用法在库内部是被主动引导的。）

### 不对称二：断开语义（一个永久挂起一个报错）

```rust
// n0-watcher lib.rs:703-718
fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
    let mut this = self.as_mut();
    if let Some(value) = this.initial.take() { return Poll::Ready(value); }
    loop {
        if ready!(this.watcher.poll_updated(cx)).is_err() {
            // The value will never be initialized
            return Poll::Pending;          // ← 永久挂起，不是 Err
        };
        ...
    }
}

// lib.rs:744-749 —— 对比 Stream 的断开处理
match self.as_mut().watcher.poll_updated(cx) {
    Poll::Ready(Ok(())) => Poll::Ready(Some(self.as_ref().watcher.peek().clone())),
    Poll::Ready(Err(Disconnected)) => Poll::Ready(None),   // ← 流正常结束
    Poll::Pending => Poll::Pending,
}
```

| 上游 Watchable 被 drop 后 | 行为 |
|---------------------------|------|
| `initialized()` | **`Poll::Pending` 且永不再被唤醒——静默永久挂起，不报错** |
| `updated()` | `Err(Disconnected)` |
| `stream()` | `None`（正常结束） |

**根因：`InitializedFut` 的 Output 是裸 `T` 而非 `Result`，所以它在类型上就没有表达失败的能力，只能靠永久 Pending 来「表示」失败。**

**实测**：drop 掉 Watchable 后 `timeout(300ms, watcher.initialized())` → `Err(Elapsed)`（挂死）；同条件下 `watcher.updated()` → `Ok(Err(Watcher lost connection to underlying Watchable, it was dropped))`。**仓内 22 个测试无一覆盖 initialized() 的断连路径。**

**真实可触发场景**：spawn 一个只持有 Watcher（不持有 Endpoint clone）的后台任务去 await `endpoint.home_relay_status().initialized()`，随后应用关闭并 drop 掉最后一个 Endpoint → **该任务永远不退出，shutdown 时 join 会卡死**。而 iroh 文档（`endpoint.rs:1381-1384`）明确 watcher 只在「**最后一个 Endpoint clone 被 drop**」时才断开，`close()` 不算 —— **正好命中这个组合**。

**任何 `initialized().await` 都必须外包 timeout**，或优先用 `.stream()`（断开正常结束）/ `.updated()`（断开返 Err），或与 `Endpoint::closed()`（`endpoint.rs:1732`）select。

> libp2p 的 Swarm 是 owned 的，你 drop 它整个 task 就结束了；iroh 的 Watcher 是对内部 Watchable 的 **Weak 引用**，生命周期与你的 task 解耦，因此多出这个「上游没了但我还在等」的失败模式。

## ⚠️ 陷阱三：`set` 的返回值语义与直觉相反

- **`Ok(old)` = 值变了**
- **`Err(v)` = 值没变** —— **Err 不代表失败**

实测：`set(2)` 在原值为 1 时返回 `Ok(1)`；紧接着再 `set(2)` 返回 `Err(2)`。iroh 与 n0-watcher 自己的测试普遍写 `.set(x).ok();` 忽略返回值。

**含义**：
1. 不要把 `set()` 的 Err 当失败处理或 `?` 传播
2. `T: Eq` 的去重是**隐式的** —— 把一个「内容相同但需要触发副作用」的值 set 进去**不会唤醒下游**，需要额外塞 epoch/序号字段。`Watcher::map`（:694-703）同样对**映射后**的值做去重

## `home_relay_status().initialized()` 为什么不能用来等连接

链条闭合于三处：

1. **`home_relay_status()` 的 Value 是 `Vec<RelayStatus>`**（底层 `Watchable<Option<RelayStatus>>` 经 Join + flatten，见 [relay.md](relay.md)），所以 `Nullable` 走 `pop()` 分支——**非空即返回**
2. **RelayActor 在拨号之前就发布 `Connecting`**（`socket/transports/relay/actor.rs:1138-1145`），`Connected` 要等 `run_dialing()` 返回后才写入（`:364-372`）
3. 于是 Vec 的状态序列是 `[]` → `[RelayStatus{Connecting}]` → `[RelayStatus{Connected}]`

`Endpoint::online` 的文档把这句话写死了（`endpoint.rs:1310-1312`）：

> This currently means at least one relay server has completed its connection handshake... **Merely selecting a relay URL is not sufficient.**

**`initialized()` 拿到的恰恰就是「只选了 URL」这个不充分状态。**

> ⚠️ **精确表述（别写「必然」）**：证据只证明了 set 调用的先后顺序，证明不了 `initialized()` **必定**观测到 `Connecting`。两个反例：(a) `InitializedFut` 的 `initial` 字段在 future **创建时就同步求值**（`lib.rs:312` `initial: self.get().into_option()`），若此时 relay 已连上，`get()` 直接返回 `[Connected]`，`initialized()` 立刻返回 `is_connected()==true`；(b) 依 last-value-wins 语义，watcher 未及时 poll 时 `Connecting` 可被 `Connected` 覆盖跳过（实践中拨号需网络 RTT，极少发生）。
>
> **准确说法：`initialized()` 在它观测到的第一个非空值处返回，而该值在典型时序下是 `Connecting`，故它对「是否已连上」不提供任何保证。** 注意这个准确版结论反而**更有力**——正因为不保证，才不能用它等连接。

> libp2p 里等 relay 可用是匹配 `SwarmEvent::Behaviour(relay::client::Event::ReservationReqAccepted{..})`——一个明确的「预约已被接受」事件；iroh 这里没有等价事件，你只能采样状态并自己判 `is_connected()`。

## 正确写法：`Endpoint::online()` + timeout

```rust
// endpoint.rs:1355-1370 —— 实现本身就是标准答案
pub async fn online(&self) {
    let mut watcher = self.inner.home_relay_status();
    let mut value = watcher.get();
    loop {
        if value.into_iter().any(|status| status.is_connected()) { return; }
        value = match watcher.updated().await {
            Ok(value) => value,
            Err(_disconnected) => {
                std::future::pending::<()>().await;   // ← 断开时显式永久挂起
                break;
            }
        }
    }
}
```

判据是 `any(|status| status.is_connected())` —— **至少一个 home relay 完成握手**。

**`online()` 自身没有超时，且两种情况下永久挂起：**
1. **没配 relay**（文档 `endpoint.rs:1314`：「If no relays are configured, this will pend forever.」）
2. watcher 断开时它显式 `std::future::pending::<()>().await`

**文档建议用接近 `NET_REPORT_TIMEOUT` 的超时包住**（`endpoint.rs:1316-1319`）。`NET_REPORT_TIMEOUT = 5`（秒，`iroh/iroh/src/net_report/defaults.rs:14`，`lib.rs:292` re-export）。

dumbpipe 的实践（`dumbpipe/src/main.rs:28, 364`）：

```rust
const ONLINE_TIMEOUT: Duration = Duration::from_secs(5);
if (timeout(ONLINE_TIMEOUT, endpoint.online()).await).is_err() {
    eprintln!("Warning: Failed to connect to the home relay");
}
```

⚠️ **两家超时后的行为不同，别搞混**：dumbpipe 5s **仅告警继续**；sendme 30s（`main.rs:731-736`）**硬失败**（`.await?` 把 Elapsed 传进 anyhow）。

> `online()` 是官方且推荐的写法，但不必说成「唯一正解」——它本身就是 `home_relay_status()` + `updated()` 循环 + `any(is_connected)` 的手写样板。iroh 自己的测试 `endpoint.rs:4090` 就是 `home_relay_status().stream()` 手写循环（判 `last_error()`）。

> **libp2p 对照**：对应 libp2p 里「循环 `swarm.next()` 直到匹配到 relay 预约成功事件」的那段样板；iroh 把它收敛成一个 `online()` 方法，所以自己手写 `initialized()` 版本纯属倒退。

## 全仓证据：`initialized()` 只该配 `Option`

`grep -rn "\.initialized()" /Volumes/yexiyue/iroh-study/` **全部命中只有一处**：

```
iroh/iroh/src/endpoint.rs:1428:    /// let _report = ep.net_report().initialized().await;
```

——且那是 `net_report()` 的**文档示例**，Value 是货真价实的 `Option<NetReport>`，不存在解包歧义。

而所有真实的「等上线」调用点无一例外用 `online()`：

```
sendme/src/main.rs:733:                let _ = ep.online().await;
dumbpipe/src/main.rs:364:    if (timeout(ONLINE_TIMEOUT, endpoint.online()).await).is_err() {
iroh/iroh/examples/echo.rs:28:    router.endpoint().online().await;
iroh/iroh/examples/listen.rs:45:    endpoint.online().await;
iroh/iroh/tests/integration.rs:54:    time::timeout(Duration::from_secs(20), server.online())
```

**判据极简：Value 是 `Option<T>` 才用 `initialized()`，是 `Vec<T>` 就别用。**

## `watch_addr()` 中同一个陷阱

**EndpointAddr 里出现 relay 地址 ≠ 已连上 relay。**

```rust
// socket/transports/relay.rs:180-185 —— 只要 status 是 Some 就产出 (url, id)，完全不看 Connecting/Connected
pub(super) fn local_addr_watch(&self) -> RelayAddrWatcher {
    let my_endpoint_id = self.my_endpoint_id;
    self.my_relay.watch()
        .map(move |status| status.map(|status| (status.url().clone(), my_endpoint_id)))
}

// endpoint.rs:1270-1284 —— watch_addr 把它直接拼进 EndpointAddr
pub fn watch_addr(&self) -> impl n0_watcher::Watcher<Value = EndpointAddr> + use<> {
    let watch_addrs = self.inner.ip_addrs();
    let watch_relay = self.inner.home_relay();
    let endpoint_id = self.id();
    watch_addrs.or(watch_relay).map(move |(addrs, relays)| {
        EndpointAddr::from_parts(endpoint_id,
            relays.into_iter().map(TransportAddr::Relay)
                .chain(addrs.into_iter().map(|x| TransportAddr::Ip(x.addr))))
    })
}
```

而 status 在 `Connecting` 阶段就已经是 `Some`。

**「watch_addr 里已经有 relay 地址了 → 应该能被拨到了吧」是错的。** iroh 自己的文档也提醒先 `online()`（`endpoint.rs:1189-1192`）。

另外 **`Endpoint::addr()` 就是 `self.watch_addr().get()`**（`endpoint.rs:1196-1198`）——一个可能残缺的瞬时快照，早期调用会拿到不完整的 EndpointAddr。

### wasm + `RelayMode::Disabled` 的真实失效模式

⚠️ **常见误传**：「浏览器里 `watch_addr()` 会一直 pending 直到 home relay 选出来；Disabled + wasm 会静默挂死」——**这是错的**。

`watch_addr()` 返回 `impl Watcher<Value = EndpointAddr>`，而 `initialized()` 有 `W: Nullable<T>` 约束，**`EndpointAddr` 不实现 `Nullable`，所以在 `watch_addr()` 的返回值上根本调不到 `initialized()`**，谈不上「永远 pending」。

**真实失效模式是「静默产出空地址」**：wasm 版 `watch_addr()`（`endpoint.rs:1297`）只 map `home_relay()`，而 `home_relay()`（`socket.rs:488`）是 `local_addrs_watch.map(filter_map(Addr::Relay))`——Disabled 下过滤出空 Vec，`get()` 立刻返回一个 `EndpointAddr::from_parts(endpoint_id, [])`，即**零地址的 EndpointAddr**。不报错、不 pending。

**危害其实更隐蔽**：你拿到的是个看起来合法、实际没人能拨通的 EndpointAddr，把它塞进 ticket 分发出去也不会有任何报错。

**真正会「等」的是 `Endpoint::online()`**，Disabled 下确实永不返回——但那是 `online()` 的语义，不是 `watch_addr()`。

## 生命周期：`close()` 不断开 watcher，只有 drop 才会

三个公开 Watcher API 的文档都重复了这段警告（`endpoint.rs:1235-1266` / `:1380-1383` / `:1412-1415`），说明这是踩过的坑：

```
/// ## Closing behavior
///
/// The returned watcher only becomes disconnected once the last clone of the [`Endpoint`]
/// is dropped. Closing the endpoint does not disconnect the watcher. Thus, a stream created
/// via [`Watcher::stream`] only terminates once the endpoint is fully dropped. To stop a task
/// that loops over a watcher stream once the endpoint stops, combine with [`Self::closed`]:
///
/// let mut addr_stream = endpoint.watch_addr().stream();
/// let endpoint_closed = endpoint.closed();
/// tokio::spawn(endpoint_closed.run_until(async move {
///     while let Some(addr) = addr_stream.next().await {
///         info!("our address changed: {addr:?}");
///     }
///     info!("endpoint closed");
/// }));
```

**`close()` 是「优雅关闭连接」，不是「释放 watcher」——两者生命周期是分离的。**

> libp2p 没有这个区分：Swarm 没了事件流就没了。iroh 的 Endpoint 是 Arc-clone 语义，`close()` 与 `drop()` 是两件事，**watcher 只认 drop**。

## API 细节速查

### `stream()` vs `stream_updates_only()`

```rust
// n0-watcher lib.rs:330-362 + :728-750
fn stream(mut self) -> Stream<Self> where Self: Unpin {
    Stream { initial: Some(self.get()), watcher: self }        // 第一个 item = 当前值
}
fn stream_updates_only(self) -> Stream<Self> where Self: Unpin {
    Stream { initial: None, watcher: self }                    // 只给后续变化
}
```

唯一区别就是 `initial` 字段。两者都在 Watchable 被 drop 时以 `None` 结束。**都消耗 self**（`fn stream(mut self)`），而 get/updated/initialized 取 `&mut self`。

**`stream().next().await` 会立刻返回、不等任何事情发生**（首个 item 是「当前值」而非「一次变化」）——跟 `updated()`（必等下一次变化）**语义相反**。

- 想要「先看一眼当前，再跟变化」→ `stream()`
- 想要纯粹的变化通知 → `stream_updates_only()`

### `get()` vs `peek()`

```rust
// n0-watcher lib.rs:255-275
fn get(&mut self) -> Self::Value { self.update(); self.peek().clone() }
/// Updates the watcher to the latest value and returns whether it changed.
fn update(&mut self) -> bool;
/// Returns a reference to the value currently stored in the watcher.
/// Watchers keep track of the "latest known" value they fetched.
/// Calling this won't update the latest value, unlike [`Watcher::get`] or [`Watcher::update`].
fn peek(&self) -> &Self::Value;
```

**`peek()` 返回的是「这个 watcher 上次抓到的值」，不是「Watchable 当前的值」。** 想要最新值必须 `get()`/`update()`，代价是要持有 `&mut`。

另外 **`get()` 在上游 Watchable 已 drop 时会返回最后一次抓到的旧值而不报错**（`lib.rs:243-247` 明说会返回 outdated value），n0-watcher 自己的测试 `test_updated_then_disconnect_then_get` 就固化了这个行为。

## 内部 Watcher（非公开 API，读源码时可能撞见）

```rust
// iroh/iroh/src/socket.rs:461 / :480 / :484-505
pub(crate) fn ip_addrs(&self) -> n0_watcher::Direct<BTreeSet<DirectAddr>>
pub(crate) fn home_relay(&self) -> impl Watcher<Value = Vec<RelayUrl>> + use<>
pub(crate) fn net_report(&self) -> impl Watcher<Value = Option<Report>> + use<>
pub(crate) fn home_relay_status(&self) -> impl Watcher<Value = Vec<RelayStatus>> + use<>
```

⚠️ **`socket.rs:446-460` 的 `ip_addrs()` 文档已经过期**：它写着「the current item in this Watcher will be [`None`]... To get the current direct addresses, use [`Watcher::initialized`]」，但字段实际类型是 `Watchable<BTreeSet<DirectAddr>>`（`socket.rs:2029`），而 **`Nullable` 根本没为 `BTreeSet` 实现——照文档写 `.initialized()` 编译不过**。

已实测坐实：新建 crate 复刻 `Watchable<BTreeSet<DirectAddr>>` 并按文档调 `.initialized()`，rustc 报 `E0277: the trait bound BTreeSet<u8>: Nullable<_> is not satisfied`，并自动列出只有 `Option<T>`/`Vec<T>` 实现 `Nullable`。

（疑似是 `CHANGELOG.md:631`「[breaking] Make direct_addresses always be initialised (#3505)」把 Option 去掉后的遗留注释——**但这个因果归因未经 git blame 证实，只是推断**。）

## 何时用 / 不用

**用**：消费 iroh 的 `endpoint.watch_addr()` / `home_relay_status()` / `net_report()` 时（**没得选**）。自己的状态若需要「多处订阅 + 同步读当前值 + 要上 wasm」，比 tokio::watch 好用。

**不用**：❌ **不要用它做事件流/消息队列** —— 它会丢中间值，**语义是状态而非事件**。「每个事件都不能丢」的场景必须继续用 mpsc/broadcast。

---

# n0-error

- **成熟度**：**production**（能用 ≠ 该用）
- **依据**：
  - version 1.0.0，edition 2024；最后提交 2026-06-15 `chore: Release`
  - iroh / iroh-base / iroh-relay 均以 `n0-error = "1.0.0"` 依赖，且 iroh 的公开错误类型确实由它生成（`ConnectError` 见 `iroh/iroh/src/endpoint.rs:928-940`、`BindError` 见 `iroh/iroh/src/socket.rs:847`）
  - ⚠️ **1.0 极年轻**：0.1.3 发于 2026-01-15（CHANGELOG.md:13），1.0.0-rc.0 发于 2026-05-06（CHANGELOG.md:3），**1.0.0 于 2026-06-15**（⚠️ **CHANGELOG 里没有 1.0.0 条目** —— 最顶部就是 1.0.0-rc.0；日期来自 git log `56019f0 chore: Release` + Cargo.toml version）—— **从 0.1 跳 1.0 只隔 5 个月**
  - ⚠️ **iroh 只 `pub use n0_watcher::Watcher`（`iroh/iroh/src/lib.rs:291`），【没有】re-export n0_error** —— 下游**非必须**引入
- **入口**：`n0-error/src/meta.rs`（116 行，先看清 **location 默认不采集**这件事）；再看 `src/lib.rs:1-74` 与 `examples/basic.rs`

## 它是什么

**thiserror 的替代品而非补充**。`#[stack_error(...)]` 属性宏会往每个 variant **插一个 `meta: Meta` 字段**（thiserror 做不到 —— derive 宏不能改结构体），再由 `#[derive(StackError)]` 生成 StackError / Display / Debug / std::error::Error / From 五套 impl。另配一个 anyhow 式的 `AnyError`，但能穿透保留整条链的 location。

## ⚠️ 致命：location 生产环境默认不采集

```rust
// meta.rs:108-115
fn location() -> Option<Location> {
    if backtrace_enabled() { Some(Location(std::panic::Location::caller())) } else { None }
}

// meta.rs:81-96
fn backtrace_enabled() -> bool {
    // 只认 RUST_BACKTRACE=1|full 或 RUST_ERROR_LOCATION=1
    #[cfg(not(test))]
    return *(BACKTRACE_ENABLED.get_or_init(from_env));   // ← OnceLock 缓存！
}
```

`lib.rs:38-40` 模块文档亦确认。

**实测**：
- 不带 env 时 `err.meta().and_then(|m| m.location())` → **`None`**，Debug 输出无位置
- 带 `RUST_ERROR_LOCATION=1` 时 → `Some("src/main.rs:19:37")`，Debug 输出 `invalid input (src/main.rs:19:37)`

**打包分发的桌面/移动应用不会带 `RUST_BACKTRACE=1`** → 迁到 n0-error 在**真实用户机器上拿不到任何 location** —— **迁移成本全付、收益为零**。

真要用必须在 `main()` 最早处、**任何错误构造之前** `unsafe { std::env::set_var("RUST_ERROR_LOCATION", "1") }`（edition 2024 下 set_var 是 unsafe），且**晚一步就被 OnceLock 锁死**。

**这一条基本单独否决了「为了 location 而迁移」这个提案。**

## ⚠️ 不能再写 `#[derive(Debug)]`

derive 会自己生成 Display + Debug + std::error::Error 三套 impl（`n0-error-macros/src/lib.rs:710` / :717 / :730，struct 版在 :881/:888/:899）。

实测：`#[derive(Debug)] #[stack_error(derive, add_meta)] struct Boom {}` → **`error[E0119]: conflicting implementations of trait 'Debug' for type 'Boom'`**。

**而且 `{:?}` 的含义被改写**（macros :717-728）：

| 格式 | 输出 |
|---|---|
| `"{}"` | `invalid input`（只有最外层） |
| `"{:#}"` | `invalid input: wanted 23 but got 13`（整条链，单行） |
| `"{:?}"` | **多行 report + `Caused by:`**（不是 derive(Debug) 的结构体转储！） |
| `"{:#?}"` | 才是类 derive(Debug) 的字段转储 |

**迁移要逐个删掉 Debug derive，同时所有 `unwrap()`/`expect()`/`tracing` 里 `{:?}` 的输出形态全变**（其实变好看了，但是行为变更）。

## 零 serde 支持

在 n0-error 的 `src/`、`n0-error-macros/src/`、`Cargo.toml` 全量 grep `Serialize|serde` → **零命中**。`Cargo.toml` 的 `[features]` 只有 `default = []` 与 `anyhow = ["dep:anyhow"]`。

`meta.rs:22-25` `pub struct Meta { location: Option<Location> }`，其中 `Location(&'static std::panic::Location<'static>)` 本身也**不可 Serialize**。

> ⚠️ **别把这说成「与 IPC/序列化正面冲突」** —— 那取决于你的错误类型是**怎么**实现 Serialize 的：
> - 若是 `#[derive(Serialize)]` → 加 `meta: Meta` 确实编译失败
> - 若是**手写 `impl Serialize`**（例如 match 投影成 `{kind, message}`）→ **多出的 meta 字段会被静默忽略**，不会编译失败，也不需要 `#[serde(skip)]`
>
> **真实成本是另一回事**：n0-error 的 `add_meta` 要求**具名字段**，而典型的 thiserror 错误枚举常是 tuple/unit variant（`Io(#[from] std::io::Error)`、`NotStarted`）—— 迁移要把 variant 改成 struct 形式并重写手写 Serialize 的 match arm。**这是工作量，不是契约冲突。**

## ✅ 好消息：不必引入它

**iroh 的错误虽由 stack_error 生成，但同时实现了 `std::error::Error`**（`n0-error-macros/src/lib.rs:730-736`）：

```rust
impl ::std::error::Error for #enum_ident {
    fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {...}
}
```

**所以现有 thiserror 错误枚举可以直接 `#[from]` 吃下，source 链也完整。**

实测：用 thiserror 包一个 stack_error 类型 —— `#[derive(Debug, thiserror::Error)] enum AppError { #[error("P2P error: {0}")] Iroh(#[from] IrohishError) }` **编译通过**，输出 `P2P error: connect failed`，`std::error::Error::source(&err)` → `Some("connect failed")`。

**落地写法就是一行**：

```rust
#[error("P2P error: {0}")]
Iroh(#[from] iroh::endpoint::ConnectError),
```

**唯一例外**：想读 iroh 错误的 `.meta()` location 或用 `.report()` 打日志时才需要 `n0-error = "1.0"` —— **建议届时只加到需要的 crate、不要全局铺开**。

## API 速览（若真要用）

```rust
use n0_error::{e, bail, ensure, try_or, stack_error,
               StackError, StackErrorExt, StackResultExt, StdResultExt, AnyError};

#[stack_error(derive, add_meta, from_sources)]
// ⚠️ 绝对不要再写 #[derive(Debug)] —— 宏自己生成 Debug，会 E0119 冲突
enum MyError {
    #[error("invalid input")]
    InvalidInput { source: InvalidInput },      // source 默认必须 impl StackError
    #[error(transparent)]                        // Display 直接转发给 source
    Io {
        #[error(std_err)] source: std::io::Error, // 非 StackError 的 source 必须标 std_err
    },
}

// 构造：e! 自动补 meta 字段，不用手写
return Err(e!(InvalidInput { expected: 12, actual: input }));
bail!(MyError::InvalidInput { source: x });
ensure!(cond, InvalidInput { expected: 23, actual: n });

// 加 context
res.context("...")       // StackError -> AnyError，【保留】location
res.std_context("...")   // std error  -> AnyError
res.anyerr()             // std error  -> AnyError，无 context
err.report().full() / .sources(Some(SourceFormat::OneLine))
```

---

## MSRV 汇总

| 库 | rust-version | edition |
|---|---|---|
| iroh | 1.91 | 2024 |
| **n0-watcher** | **1.91**（`Cargo.toml:13`） | — |
| n0-future | 1.85 | 2021 |
| n0-error | 未声明 | 2024 |

**迁 iroh 会一次性把 MSRV 底线抬到 1.91。** 若 CI/Docker/贡献者环境锁了旧 toolchain，需提前记一笔。
