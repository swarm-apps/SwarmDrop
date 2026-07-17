# 流原语与协议分发（cheap streams + Router/ProtocolHandler/ALPN）

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

> **noq 不在 iroh-study 里**。它是 n0 自家的 QUIC 栈（quinn 分支），iroh 1.0.2 依赖 `noq`/`noq-proto`/`noq-udp` **1.0.1**（`iroh/iroh/Cargo.toml`），源码 vendor 在 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/noq-1.0.1/`。本文引用 noq 时会标注。
>
> 注意 `endpoint.rs` 里有个叫 `Noq` 的**错误变体**（`ConnectWithOptsError::Noq { source: QuicConnectError }`），那是错误枚举不是模块，grep 时别串。

---

# 第一部分：流很便宜

## 四个流原语：非 async 的 `&self` 方法，一行转发

```rust
// iroh/iroh/src/endpoint/connection.rs:874-903
#[inline] pub fn open_uni(&self)   -> OpenUni<'_>   { self.inner.open_uni() }
#[inline] pub fn open_bi(&self)    -> OpenBi<'_>    { self.inner.open_bi() }
#[inline] pub fn accept_uni(&self) -> AcceptUni<'_> { self.inner.accept_uni() }
#[inline] pub fn accept_bi(&self)  -> AcceptBi<'_>  { self.inner.accept_bi() }

// connection.rs:736-741
#[derive(Debug, Clone)]
pub struct Connection<State: ConnectionState = HandshakeCompleted> {
    inner: noq::Connection,
    data: State::Data,
}
```

四个方法都是**普通 fn**（非 async）、取 **`&self`**（非 `&mut self`）、带 `#[inline]`、函数体是对 `noq::Connection` 的纯转发。返回的是 Future 结构体而非 async block。

impl 块是 `impl<T: ConnectionState> Connection<T>`（`connection.rs:811`），意味着这四个方法在 **0-RTT 状态**（`IncomingZeroRtt` / `OutgoingZeroRtt`）下同样可用——握手没完成就能开流。

noq 侧的 Future Output（`noq-1.0.1/src/connection.rs:963-991`）：

| Future | Output |
|--------|--------|
| `OpenUni` | `Result<SendStream, ConnectionError>` |
| `OpenBi` | `Result<(SendStream, RecvStream), ConnectionError>` |
| `AcceptUni` | `Result<RecvStream, ConnectionError>` |
| `AcceptBi` | `Result<(SendStream, RecvStream), ConnectionError>` |

## 最重要的架构含义：clone + spawn + open_bi

因为 `open_*` 只要 `&self` 且 `Connection` 是 `Clone`（iroh 侧 `connection.rs:736` derive；noq 侧 `noq/src/connection.rs:310-311` 也是 Clone，doc：「May be cloned to obtain another handle to the same connection.」），可以把 Connection clone 进任意多个 task，每个 task 各自开流。

**调用方不必自己加锁、不需要流管理器、不需要中央 multiplexer。**

> ⚠️ 精确表述：「不必自己加锁」只在**应用层**成立。noq 内部每条连接有一把互斥锁——`poll_open` 第一行就是 `conn.lock_without_waking("poll_open")`（`noq-1.0.1/src/connection.rs:1000`），`ConnectionRef` 本身即 `Arc<Mutex<State>>` 语义。别说成「无锁」。

> **libp2p 对照**：libp2p 里要开流必须通过 Swarm 的事件循环（Swarm 不是 Clone、要独占 `&mut`），跨 task 开流得自己搭 channel + 命令模式转发。iroh 下 `Connection.clone()` 直接丢进 `tokio::spawn` 就行。

## 「流很便宜」是写进 rustdoc 的合同

不是社区口头说法。iroh 与 noq 两层文档共 **7 处**明确表述：

```
// connection.rs:820-824（open_uni 的 # QUIC streams 段）
/// QUIC can multiplex many streams onto a single connection. Streams can be short or
/// long lived and may be opened and closed without incurring any extra cost. ...
/// Thus streams do not suffer head-of-line blocking.

// connection.rs:828-831（# Opening streams 段）
/// Opening a new stream does not incur any extra overhead compared to sending data on an
/// existing stream. However only once some data has been transmitted on the stream, will
/// the peer become aware of the newly opened stream.

// lib.rs:147-148
//! Additionally to being extremely light-weight, streams can be interleaved and will not
//! block each other.  Allowing many streams to co-exist, regardless of how long they last.

// lib.rs:152（warning 块）
//! To keep streams cheap, they are lazily created on the network ...

// endpoint.rs:875
/// Note that due to the light-weight properties of streams a stream will only be accepted
/// once the initiating peer has sent some data on it.

// noq connection.rs:321 (open_uni) / :333 (open_bi)
/// Streams are cheap and instantaneous to open unless blocked by flow control.
```

**noq 那句一字点破成本模型：唯一的开销来源是流控，不是流本身。**

## 超限是背压不是错误（区别于 libp2p 的技术核心）

```rust
// noq-1.0.1/src/connection.rs 的 poll_open
fn poll_open<'a>(ctx, conn, mut notify, dir) -> Poll<Result<(ConnectionRef, StreamId, bool), ConnectionError>> {
    let mut state = conn.lock_without_waking("poll_open");
    if let Some(ref e) = state.error {
        return Poll::Ready(Err(e.clone()));          // ← 只有连接已出错才 Err
    } else if let Some(id) = state.inner.streams().open(dir) {
        ...
        return Poll::Ready(Ok((conn.clone(), id, is_0rtt)));
    }
    loop {
        match notify.as_mut().poll(ctx) {
            Poll::Pending => return Poll::Pending,   // ← 额度耗尽：挂起等待，不报错
            Poll::Ready(()) => notify.set(conn.shared.stream_budget_available[dir as usize].notified()),
        }
    }
}
```

**「无脑多开」在 iroh 下最坏结果是变慢（排队），不是失败。这就是为什么可以不做流管理。**

### 对照实锤：yamux 超限是致命错误

```rust
// yamux-0.13.10/src/connection.rs:489-491 —— 出站超限
fn poll_new_outbound(&mut self, cx: &mut Context<'_>) -> Poll<Result<Stream>> {
    if self.streams.len() >= self.config.max_num_streams {
        log::error!("{}: maximum number of streams reached", self.id);
        return Poll::Ready(Err(ConnectionError::TooManyStreams));   // ← 硬错误，非背压
    }
```

而且**入站超限更狠**：`connection.rs:622-624` 与 `:725-727` 直接 `Action::Terminate(Frame::internal_error() / protocol_error())`，**当场终止整条连接**。

注意 `TooManyStreams` 属于 `ConnectionError`（`yamux-0.13.10/src/error.rs:16-30`，含 `Io` / `Decode` / `NoMoreStreamIds` / `Closed` / `TooManyStreams` / `InvalidWindowUpdate`）——是**连接级**错误，整条连接不可用。默认上限 `max_num_streams: 512`（`yamux-0.13.10/src/lib.rs:90`）。

> **这就是问题的答案**：libp2p 下「流是稀缺资源、要多路复用管理」是**被迫的**——不管理就 `TooManyStreams` 打死连接。iroh/QUIC 下同样情形只是 `Poll::Pending` 排队。**从 libp2p 迁 iroh 时，原先的限流/流池逻辑属于可以直接移除的净负债。**
>
> 反直觉：yamux 默认 512 比 QUIC 默认 100 更宽——但 yamux 超限是致命的，QUIC 超限只是排队。**「数字更大」反而更危险。**

## 默认并发上限 100，且不是你想的那个方向

```rust
// noq-proto-1.0.1/src/config/transport.rs:552-560
max_concurrent_bidi_streams: 100u32.into(),
max_concurrent_uni_streams: 100u32.into(),
max_idle_timeout: Some(VarInt(30_000)),
stream_receive_window: STREAM_RWND.into(),   // = 12500*1000/1000*100 = 1_250_000 bytes
receive_window: VarInt::MAX,
send_window: (8 * STREAM_RWND).into(),
send_fairness: true,
```

iroh 的 builder（`iroh/iroh/src/endpoint/quic.rs:151-162`）只覆盖 keepalive、multipath、NAT 相关项，**未触碰 `max_concurrent_*_streams`**：

```rust
fn new() -> Self {
    let mut cfg = noq::TransportConfig::default();
    cfg.keep_alive_interval(Some(HEARTBEAT_INTERVAL));
    cfg.default_path_keep_alive_interval(Some(HEARTBEAT_INTERVAL));
    cfg.default_path_max_idle_timeout(Some(PATH_MAX_IDLE_TIMEOUT));
    cfg.max_concurrent_multipath_paths(MAX_MULTIPATH_PATHS);
    cfg.max_remote_nat_traversal_addresses(MAX_QNT_ADDRESSES);   // quic.rs:537
    cfg.server_handshake_migration(true);
    Self(cfg)
}
```

调用链：`endpoint.rs:202` `transport_config: QuicTransportConfig::default()` → `quic.rs:140-142` → `QuicTransportConfigBuilder::new()`（153-163）→ noq-proto 默认 100/100。

> 唯一的例外不在应用面：`iroh-relay/src/quic.rs:108-109` 把 `max_concurrent_uni_streams(0)` / `max_concurrent_bidi_streams(0)` 显式设为 0——那是 relay 的 QUIC 地址发现服务端（ALPN_QUIC_ADDR_DISC），刻意禁止对端开任何流，与应用数据面无关。所以准确说法是「**应用面 Endpoint 没改过**」。

### 100 是并发数，2^62 是生命周期总量——别读串

```
// connection.rs:863-865
/// The protocol limits the total number of streams during the lifetime of a connection
/// to 2**62, this limit applies to the sum of uni- and bi-directional streams. For most
/// practical purposes this is essentially unlimited.
```

文档里那句 `2^62 "essentially unlimited"` 说的是**生命周期累计总数**，不是并发数。**要开 >100 并发流必须显式调 `max_concurrent_bidi_streams()`，否则第 101 条静默排队。**

### 反直觉：你设的限制是「对端能向你开多少」

```rust
// quic.rs:169-176 —— 注意 "incoming" 和 "for the peer to open"
/// Maximum number of incoming bidirectional streams that may be open concurrently.
///
/// Must be nonzero for the peer to open any bidirectional streams.
///
/// Worst-case memory use is directly proportional to `max_concurrent_bidi_streams *
/// stream_receive_window`, with an upper bound proportional to `receive_window`.
pub fn max_concurrent_bidi_streams(mut self, value: VarInt) -> Self { ... }

// 运行期版本（connection.rs:1084-1108），&self 且可随时调，不用重建连接
pub fn set_max_concurrent_uni_streams(&self, count: VarInt)
pub fn set_receive_window(&self, receive_window: VarInt)
pub fn set_max_concurrent_bi_streams(&self, count: VarInt)
```

**想让自己能开 500 条并发流，改自己的 config 没用——必须让对端把它的 `max_concurrent_bidi_streams` 调到 500。** 双方都是 iroh 默认时，双向各 100。

> **libp2p 对照**：yamux 的 `set_max_num_streams` 是**本地对称限制**（`self.streams.len() >= self.config.max_num_streams` 直接卡自己出站），语义**正好相反**——libp2p 卡自己，QUIC 卡对端。迁移时这个方向搞反会导致限制完全不生效。

## N 条流的真实代价是内存

```
// connection.rs:857-861（# Stream limits 段）
/// Each stream has a *receive window* of a maximum number of bytes that may be
/// in-flight before the sender is blocked from transmitting more. ... There is also a
/// [`QuicTransportConfigBuilder::receive_window`] which applies to all streams combined
/// and can be changed during a connection using [`Self::set_receive_window`].
```

默认推导（`noq-proto/src/config/transport.rs:544-558`）：

```
EXPECTED_RTT        = 100 ms
MAX_STREAM_BANDWIDTH = 12500 * 1000 bytes/s
STREAM_RWND         = MAX_STREAM_BANDWIDTH / 1000 * EXPECTED_RTT = 1_250_000
stream_receive_window: STREAM_RWND
receive_window: VarInt::MAX      // = (1 << 62) - 1，见 noq-proto/src/varint.rs:18
```

**worst-case ≈ 100 × 1.25MB = 125MB/连接。** 而 `receive_window` 默认是 `VarInt::MAX`（无限），所以文档那句「upper bound proportional to `receive_window`」在**默认配置下不构成任何保护**——真正的天花板就是 125MB/连接。

**对文件传输类应用（同时开多条大流、且可能来不及读）这是个真实的内存风险**：要么调小 `stream_receive_window`，要么显式设 `receive_window` 做聚合兜底。

`quic.rs:216-222` 的官方告诫：

> Setting this smaller than `receive_window` helps ensure that a single stream doesn't monopolize receive buffers, which may otherwise occur if the application chooses not to read from a large stream for a time while still requiring data on other streams.

> **libp2p 对照**：yamux **强制**你先算内存——`max_connection_receive_window` 必须 ≥ 256KiB × `max_num_streams`，否则 assert 直接 panic（`yamux-0.13.10/src/lib.rs:129-134`）。QUIC 不强制，默认给你无限聚合窗口——更自由，也更容易踩内存。

## 配套调优旋钮

- **`send_fairness`**（默认 `true`，`noq-proto/src/config/transport.rs:560`）：同优先级流 round-robin 公平调度。文档（`transport.rs:156-165`）中立地指出「Disabling fairness can reduce fragmentation and protocol overhead for workloads that use **many small streams**」——即多小流场景反而应该**关掉** fairness，与直觉相反。默认 `true` 是为公平性优化，不是为吞吐。
- **`SendStream::set_priority`**（`connection.rs:843-847`）：高优先级流的数据先于低优先级流发送。这是「多开流」后配套的编排手段，替代了 libp2p 里手写的调度逻辑。

## ⚠️ 懒创建：open_bi() 后不写数据 = 死锁

跨越 iroh 与 noq 两层文档三处警告：

```
// iroh connection.rs:839-841
/// Note that opening a stream is not sufficient for the accept call to yield a new
/// stream. Data must be sent on a stream before the respective accept call at the peer
/// will yield a [`RecvStream`].

// iroh lib.rs:151-156
//! To keep streams cheap, they are lazily created on the network: only once a sender starts
//! sending data on the stream will the receiver become aware of a stream. ... The sender
//! **must** send data on the stream before the receiver's [`Connection::accept_bi`] call
//! will return.

// noq-1.0.1/src/connection.rs:357-362
/// **Important Note**: The `Connection` that calls [`open_bi()`] must write to its [`SendStream`]
/// before the other `Connection` is able to `accept_bi()`. Calling [`open_bi()`] then
/// waiting on the [`RecvStream`] without writing anything to [`SendStream`] will never succeed.
```

**「便宜」和「懒创建」是同一枚硬币的两面**：正因为 `open_bi()` 不发任何网络包（纯本地记账），对端才完全不知道流存在。

**经典死锁**：A `open_bi()` 后等 B 先说话，B 卡在 `accept_bi()` 上永远等不到。**协议设计上必须保证「开流方先写」。** dumbpipe 就是靠发一个 5 字节 HANDSHAKE 常量来破这个局。

> **libp2p 对照**：libp2p 开流会立刻走 multistream-select 协商往返，对端马上收到 inbound stream 事件。**从 libp2p 迁过来最容易踩的就是这条**——原本「开流即通知」的假设在 iroh 下不成立。

## dumbpipe：cheap streams 的样板（注意有两种写法，别抄错）

**正确样板**——一条 Connection clone 进每个 task，每个本地 socket 开一条新流，无流池/限流器（`dumbpipe/src/main.rs:834-856` accept 循环 + `:797-831` handler）：

```rust
loop {
    let next = tokio::select! { stream = unix_listener.accept() => stream, _ = ctrl_c() => break };
    let connection = connection.clone();          // :844 复用同一条 iroh Connection
    tokio::spawn(async move {                     // :846 每个本地连接一个 task
        handle_unix_accept(next, connection, handshake).await
    });
}

// handle_unix_accept @ :797-831
let (mut endpoint_send, endpoint_recv) = connection.open_bi().await?;   // :808-811
```

**反面写法**——`handle_tcp_accept`（`main.rs:483-514`）每个 TCP 连接都重新 `endpoint.connect(addr, alpn)`（:494-497）建**新连接**再 `open_bi()`（:498-501）。那是打洞成本很高的写法。同一个仓里两种写法并存。

> 版本提醒：`dumbpipe/Cargo.toml:19` 声明 `iroh = "1.0.0"`，Cargo.lock 解析到 **1.0.0**（crates.io 版，不是本地 iroh-study 那份 1.0.2）。用到的 API（`Connection: Clone` + `&self open_bi`）两版一致，架构论点不受影响。已核实 dumbpipe src/ 下确无 semaphore / limiter / pool（grep 零命中）。

---

# 第二部分：协议分发（Router / ProtocolHandler / ALPN）

## Router 极薄：BTreeMap + 一个自带 task 的 accept 循环

`spawn()` 做四件事（`iroh/iroh/src/protocol.rs:499-622`）：

```rust
#[must_use = "Router aborts when dropped, use Router::shutdown to shut the router down cleanly"]
pub fn spawn(self) -> Router {
    // 1) 收集所有已注册 ALPN，写进 TLS server config
    let alpns = self.protocols.alpns().map(|alpn| alpn.to_vec()).collect::<Vec<_>>();
    let protocols = Arc::new(self.protocols);          // 2) 共享给每条连接的 task
    self.endpoint.set_alpns(alpns);                    // ← 真正让 Endpoint 开始接受
    ...
    let task = task::spawn(run_loop_fut.instrument(...));   // 3) accept 循环
    let task = AbortOnDropHandle::new(task);
    Router { endpoint: self.endpoint, task: Arc::new(Mutex::new(Some(task))), cancel_token: cancel }  // 4)
}

// protocol.rs:484-492 —— accept() 只是 insert
pub fn accept(mut self, alpn: impl AsRef<[u8]>, handler: impl Into<Box<dyn DynProtocolHandler>>) -> Self {
    self.protocols.insert(alpn.as_ref().to_vec(), handler.into());
    self
}
```

**`accept()` 只是往 BTreeMap 里 insert，真正让 Endpoint 开始接受该 ALPN 的是 `spawn()`。**

> **libp2p 对照**：libp2p 没有等价的 spawn——Swarm 是你自己 `loop { swarm.select_next_some().await }` 驱动的，**你拥有事件循环**。iroh 的 Router **自己持有 tokio task**，你只拿到一个句柄——控制反转方向相反。libp2p 里「注册协议」是把 Behaviour 塞进 `#[derive(NetworkBehaviour)]` 的 struct 字段（编译期确定）；iroh 是运行时往 BTreeMap 里 insert。

## `set_alpns` 是覆盖式的

```rust
// endpoint.rs:959-975
/// Sets the list of accepted ALPN protocols.
///
/// This will only affect new incoming connections.
/// Note that this *overrides* the current list of ALPNs.
///
/// If the endpoint is closed, this method will log a warning and ignore
/// the request to set new ALPNs.
pub fn set_alpns(&self, alpns: Vec<Vec<u8>>) {
    if self.is_closed() {
        warn!("Attempting to set ALPNs for a closed endpoint. Ignoring.");
        return;
    }
    ...
}
```

**推论：**
1. **混用 `Endpoint::builder().alpns()` 和 Router 是错的**——ALPN 只该在一个地方声明。`.alpns(vec![X]).bind()` 后再 `Router::builder(ep).accept(Y, h).spawn()`，X 被静默丢弃，只剩 Y。
2. **空 Router**（`Router::builder(ep).spawn()`，一个 accept 都没调）会 `set_alpns(vec![])`，即把 Endpoint 的入站 ALPN **清空**。测试 `test_shutdown`（`protocol.rs:677-690`）就是这么建的。
3. endpoint 已 closed 时 `set_alpns` 只 warn 不报错，`spawn()` 依然返回一个「永远收不到连接」的 Router。

> libp2p 无对应陷阱：协议名由各 Behaviour 自己通过 `ConnectionHandler::listen_protocol()` 上报，Swarm 做**并集**，不存在「后注册的覆盖先注册的」这种全局列表语义。

## ProtocolHandler：3 个方法，只有 accept 必须实现

```rust
// protocol.rs:228-287
pub trait ProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    fn on_accepting(&self, accepting: Accepting)
        -> impl Future<Output = Result<Connection, AcceptError>> + Send
    { async move { let conn = accepting.await?; Ok(conn) } }        // 默认：直接 await

    fn accept(&self, connection: Connection)
        -> impl Future<Output = Result<(), AcceptError>> + Send;    // 无默认实现

    fn shutdown(&self) -> impl Future<Output = ()> + Send
    { async move {} }                                               // 默认：空
}
```

全部走 **RPITIT**（`impl Future`）而非 `#[async_trait]`，所以实现时直接写 `async fn accept(&self, ...)`。

**三个坑：**
- **`&self` 而非 `&mut self`**——handler 内部状态必须自己用 `Arc<Mutex<..>>`/原子量包起来。官方 `screening-connection.rs` 就是 `struct ScreenedEcho { conn_attempt_count: Arc<AtomicU64> }`
- **`Debug` 是 supertrait**——忘了 `#[derive(Debug)]` 编译不过
- **签名变过**：CHANGELOG 显示这是 #3320 的 breaking change（"Make ProtocolHandler use async functions"），#3010 移除了旧的 Arc 要求。**网上 iroh 0.x 时代的老代码/教程签名对不上。**

> **本质区别**：libp2p 的 `NetworkBehaviour` / `ConnectionHandler` 是 **poll-based 状态机**（`fn poll(&mut self, cx) -> Poll<ToSwarm<..>>`），拿 `&mut self`、不能用 async/await，你得手写状态机把每个 await 点拆成状态字段。iroh 的 `ProtocolHandler` 是 `async fn accept(&self, conn)`，可以线性顺序地写「读→处理→写→关」。**这是从「状态机编程」到「普通 async 编程」的转变，也是 iroh 心智负担远低于 libp2p 的主要原因。**

## DynProtocolHandler：类型擦除层（3 个方法，别以为擦掉了两个）

`ProtocolHandler` 因为用了 RPITIT 所以**不是 dyn-safe**。iroh 靠一个平行的 `DynProtocolHandler` trait 做类型擦除：

```rust
// protocol.rs:323-352 —— 注意是 3 个方法，两个带默认实现
pub trait DynProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    fn on_accepting(&self, accepting: Accepting)
        -> Pin<Box<dyn Future<Output = Result<Connection, AcceptError>> + Send + '_>>
    { Box::pin(async { accepting.await }) }                  // :333-341

    fn accept(&self, connection: Connection)
        -> Pin<Box<dyn Future<Output = Result<(), AcceptError>> + Send + '_>>;   // :344-347

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
    { Box::pin(async {}) }                                   // :350-352
}

// protocol.rs:355-373 —— blanket impl，三个方法全部转发
impl<P: ProtocolHandler> DynProtocolHandler for P { ... }

// protocol.rs:317-321
impl<T: ProtocolHandler> From<T> for Box<dyn DynProtocolHandler> {
    fn from(value: T) -> Self { Box::new(value) }
}

// protocol.rs:289-315 —— Arc<T> / Box<T> 的转发 impl
impl<T: ProtocolHandler> ProtocolHandler for Arc<T> { /* 转发到 self.as_ref() */ }
impl<T: ProtocolHandler> ProtocolHandler for Box<T> { /* 转发到 self.as_ref() */ }
```

三方法结构很重要：正是它让 `shutdown` 钩子和 `on_accepting` 拦截**能穿透 `Box<dyn>`**。

**何时需要显式 Box**：运行时按配置挑 handler（feature flag 决定挂哪个实现）时必须标注成 `Box<dyn DynProtocolHandler>`——直接写 `if cond { A } else { B }` 类型对不上。`DynProtocolHandler` 是 pub 的（CHANGELOG #3366 "Expose DynProtocolHandler"）就是为了这个场景。

`Arc<T>`/`Box<T>` 的转发 impl 意味着 handler 内部不必再自己套 Arc 才能共享给外部代码。

> **libp2p 对照**：libp2p 的组合是**编译期**的（`#[derive(NetworkBehaviour)]` 静态组合 + 生成聚合 Event 枚举，无类型擦除）。iroh 是运行时 BTreeMap + `Box<dyn>`，代价是每次 accept 多一次 `Box::pin` 分配，收益是可以运行时增减协议集合（虽然 Router 本身构建后不可变）。

## ALPN 两层分发，粒度是「整条连接」

```rust
// protocol.rs:625-661
async fn handle_connection(incoming: Incoming, protocols: Arc<ProtocolMap>) {
    let mut accepting = match incoming.accept() {
        Ok(conn) => conn,
        Err(err) => { warn!("Ignoring connection: accepting failed: {err:#}"); return; }
    };
    let alpn = match accepting.alpn().await {
        Ok(alpn) => alpn,
        Err(err) => { warn!("Ignoring connection: invalid handshake: {err:#}"); return; }   // :636
    };
    tracing::Span::current().record("alpn", ...);                                          // :640

    let Some(handler) = protocols.get(&alpn) else {
        warn!("Ignoring connection: unsupported ALPN protocol"); return;                   // :643
    };
    match handler.on_accepting(accepting).await {
        Ok(connection) => {
            tracing::Span::current().record("remote", ...);                                // :648-651
            if let Err(err) = handler.accept(connection).await {
                warn!("Handling incoming connection ended with error: {err}");
            }
        }
        Err(err) => { warn!("Accepting incoming connection ended with error: {err}"); }    // :656-659
    }
}

// protocol.rs:377-388 —— 精确匹配，无前缀/通配
pub(crate) struct ProtocolMap(BTreeMap<Vec<u8>, Box<dyn DynProtocolHandler>>);
```

**第一层**在 TLS/QUIC 握手里：`set_alpns` 生成的 rustls server config 只接受列表内的 ALPN，不匹配的连接在**握手阶段**就失败。

> ⚠️ 别说成「根本到不了 Router」：`Incoming` 照样从 `endpoint.accept()`（`:557`）出来，照样过 `incoming_filter`、照样被 `join_set.spawn` 出一个带 `router.accept` span 的 task（`:587-592`）；TLS 拒绝是在 task 内部 `handle_connection` 的 `accepting.alpn().await` 处才显形。准确说法是「**到不了 handler / 到不了 ALPN 查表那一层**」。

**第二层**在 Router 进程内：从握手数据里读出协商结果，拿 `Vec<u8>` 去 BTreeMap 精确查表。

**ALPN 匹配是整串字节精确相等**，没有前缀匹配、没有版本协商回退、没有通配 handler。协议升级（`/myproto/1` → `/myproto/2`）必须两个 ALPN 都 `.accept()` 注册。

连接侧可以用 `ConnectOptions::with_additional_alpns` 一次提议多个 ALPN，由接受侧决定最终协议（`endpoint.rs:1786-1809`：「The ALPN identifier order on the connect side doesn't matter, since it's the accept side that determines the protocol.」）。

> **这是与 libp2p 最本质的区别。** libp2p 用 multistream-select 在**每条 substream** 上协商协议名（`libp2p-swarm-0.47.1/src/connection.rs:581` `dialer_select_proto` / `:618` `listener_select_proto`；`lib.rs:1461` 明写「The subtream upgrade protocol is the multistream-select protocol」），一条连接可以并发跑 identify + kad + gossipsub + 自定义协议。iroh 用 TLS ALPN 在**连接建立时**协商一次，一条 QUIC 连接**终身绑定一个 ALPN → 一个 handler**；要同时跑两个协议就得开两条连接（复用同一个 Endpoint 和同一次打洞的路径，但是两个 Connection）。
>
> **也是「流为什么在 libp2p 里贵」的第二个原因**：每条流一次 multistream-select 往返。iroh 一条连接一次 ALPN，之后开流纯本地。所以 **libp2p 里「合并多个逻辑通道到一条流」是优化，iroh 里这么做是反优化**。
>
> iroh 侧 ALPN 是连接级静态信息：`StaticInfo { endpoint_id, alpn }`（`connection.rs:750-755`），`pub fn alpn(&self) -> &[u8] { &self.data.info.alpn }`（`:1115-1117`）读的是握手时就固化的字段，**无网络交互**。

### ⚠️ 同一 ALPN 注册两次 = 静默覆盖

`ProtocolMap::insert` 就是 `BTreeMap::insert`（`protocol.rs:376-388`）。ALPN 常量拼错或两个库恰好用了同一个 ALPN 字节串时，症状是「**其中一个协议神秘失灵**」而不是启动报错——因为 `alpns()` 取的是 BTreeMap 的 keys，被覆盖的那个 handler 连 ALPN 都不会重复出现在 `set_alpns` 列表里，**从外部完全观察不到冲突发生过**。

反过来，同一个 handler 注册到多个 ALPN 完全合法（`.accept(V1, h.clone()).accept(V2, h)`），这是做协议版本兼容的正规姿势。

## 一个 Endpoint 挂多个协议

```rust
// iroh-examples/tauri-todos/src-tauri/src/iroh.rs:27-49
let endpoint = iroh::Endpoint::builder(presets::N0).secret_key(key).bind().await?;

let gossip = Gossip::builder().spawn(endpoint.clone());
let blobs = FsStore::load(&path).await?;
let docs = Docs::persistent(path).spawn(endpoint.clone(), (*blobs).clone(), gossip.clone()).await?;

let router = iroh::protocol::Router::builder(endpoint.clone())
    .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
    .accept(GOSSIP_ALPN, gossip)
    .accept(DOCS_ALPN, docs.clone())
    .spawn();
```

**分层注意**：gossip / docs 这些库的 handler 自己也要拿 `endpoint.clone()` 去主动发起出站连接，所以 **Endpoint 先建 → clone 给各协议 → 最后才交给 Router::builder**。Router 并不「拥有」Endpoint 的独占权（Endpoint 是 Clone 的），它只是 set_alpns + 跑 accept 循环。

> 对应 libp2p 的 `#[derive(NetworkBehaviour)] struct Behaviour { gossipsub, kad, custom }`。差别：libp2p 的组合产物是一个聚合 Event 枚举，你得在单一 match 里处理所有协议的事件；iroh 的多协议之间**完全解耦**，各自的 `accept()` 在各自的 task 里跑，**没有任何中心事件枚举需要 match**。

## AcceptError：5 变体，`#[non_exhaustive]`

```rust
// protocol.rs:113-162
#[stack_error(derive, add_meta, from_sources, std_sources)]
#[non_exhaustive]
pub enum AcceptError {
    #[error(transparent)] Connecting { source: ConnectingError },
    #[error(transparent)] Connection { source: ConnectionError },
    #[error(transparent)] MissingRemoteEndpointId { source: RemoteEndpointIdError },
    #[error("Not allowed.")] NotAllowed {},
    #[error(transparent)] User { source: AnyError },
}

impl AcceptError {
    #[track_caller] pub fn from_err<T: std::error::Error + Send + Sync + 'static>(value: T) -> Self
    #[track_caller] pub fn from_boxed(value: Box<dyn std::error::Error + Send + Sync>) -> Self
}

impl From<std::io::Error>     for AcceptError { fn from(err) -> Self { Self::from_err(err) } }
impl From<quic::ClosedStream> for AcceptError { fn from(err) -> Self { Self::from_err(err) } }
```

**三个坑：**
1. `#[non_exhaustive]`——下游 match 必须带 `_` 分支
2. **`?` 的行为不直观**：`connection.accept_bi()` 的 `ConnectionError` 走 `Connection` 变体，但 `tokio::io::copy` 的 `io::Error` 和 `send.finish()` 的 `ClosedStream` **都会被塞进 `User` 变体**——看错误信息时区分不出「是 IO 挂了还是业务逻辑挂了」
3. **不是所有错误类型都有 From**——`examples/search.rs` 里 `read_to_end` / `String::from_utf8` / `write_all` 都得显式 `.map_err(AcceptError::from_err)`

（CHANGELOG.md:796 显示它在 #3339 从 `ProtocolError` 改名而来；`from_boxed` 是 #3620 后加的，CHANGELOG.md:382。）

> **libp2p 对照**：libp2p 的 request_response 里错误分 `InboundFailure` / `OutboundFailure`，通过 Behaviour 的 Event 流报给你，在事件循环里集中处理。⚠️ 该枚举的字段随版本变过——libp2p-request-response **0.29.0**（随 libp2p 0.56.0）的变体是 `InboundFailure { peer, connection_id, request_id, error }`，**有 `connection_id`**，照抄网上旧签名会编译不过。
>
> iroh 的 `AcceptError` 是 handler 自己 async fn 的返回值，就近 `?` 传播，不经过任何事件总线——**但也因此没人替你重试或统计，Router 只 `warn!` 一行就丢了**。

### `NotAllowed`：纯语义标记，对端收不到任何原因

```rust
// iroh/iroh/examples/screening-connection.rs:102-119
impl ProtocolHandler for ScreenedEcho {
    /// `on_accepting` allows us to intercept a connection as it's being formed,
    /// which is the right place to cut off a connection as early as possible.
    async fn on_accepting(&self, accepting: Accepting) -> Result<Connection, AcceptError> {
        let count = self.conn_attempt_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count.is_multiple_of(2) {
            println!("rejecting connection");
            return Err(e!(AcceptError::NotAllowed));   // ← n0_error 的 e! 宏
        }
        let conn = accepting.await?;
        Ok(conn)
    }
}
```

**从 Router 的角度，`on_accepting` 和 `accept` 返回任何 Err 的处理是完全一样的：`warn!` 一行然后 drop。`NotAllowed` 不会给对端发任何专门的错误码或原因**——不发 CONNECTION_REFUSED、不 close with code，对端只看到连接被断。

要让对端知道「被拒绝且原因是什么」，得自己 `await` 出 Connection 再 `connection.close(code, reason)`，或改用 `RouterBuilder::incoming_filter` 的 `Reject`（那个会发真的 CONNECTION_REFUSED）。

注意变体写作 `NotAllowed {}`（带空花括号），构造要用 `e!(AcceptError::NotAllowed)`，不是 `AcceptError::NotAllowed`。

> libp2p 的连接门禁在 `handle_established_inbound_connection` / `handle_pending_inbound_connection` 里返回 `Err(ConnectionDenied)`，同样静默拒绝。但那是 **Behaviour 级**（对整个 peer 的所有协议生效），iroh 的 `on_accepting` 是 **per-ALPN handler 级**。iroh 要做全局门禁得用 `EndpointHooks`（`endpoint/hooks.rs:68` 的 `before_connect` / `after_handshake`）或 Router 的 `incoming_filter`。

### accept 返回 Err 不发错误码——文档要求你自己 close

```
// protocol.rs:258-272
/// The returned future runs on a freshly spawned tokio task so it can be long-running. Once
/// `accept()` returns, the connection is dropped. This means that it will be closed
/// if there are no other clones of the connection.  If there is a protocol error, you
/// can use [`Connection::close`] to send an error code to the remote peer. Returning
/// an `Err<AcceptError>` will also drop the connection and log a warning, but no
/// dedicated error code will be sent to the peer, so it's recommended to explicitly
/// close the connection within your accept handler.
```

**echo 范例结尾的 `connection.closed().await;` 不是可选的礼貌**——它是在等对端先关，否则 `accept()` 一返回就 drop connection，可能把还没 flush 出去的响应数据截断。`echo-no-router.rs:108-110` 注释写得很直白：「Wait until the remote closes the connection, which it does once it received the response.」

> libp2p 的 request_response 帮你管了响应生命周期（塞进 `ResponseChannel`，Behaviour 负责发完并关流）；iroh 里 **Connection 的 drop 时机直接决定线路行为，「什么时候 return」是协议正确性的一部分**。

## 三层门禁：选对层

| 层 | API | 时机 | 能看到什么 |
|----|-----|------|-----------|
| ① 握手前 | `RouterBuilder::incoming_filter` | `Incoming` 刚出来，spawn 之前，**内联在 accept 循环主线** | 只有 `&Incoming`。**拿不到 ALPN、拿不到对端 EndpointId** |
| ② 握手后 | `EndpointHooks::after_handshake`（`endpoint/hooks.rs:68`） | 握手完成 | 对端身份 |
| ③ 协议级 | `ProtocolHandler::on_accepting` | ALPN 查表后 | 该 ALPN 的 `Accepting` |

### incoming_filter 四选一

```rust
// protocol.rs:214-215
pub type IncomingFilter =
    Arc<dyn Fn(&crate::endpoint::Incoming) -> IncomingFilterOutcome + Send + Sync + 'static>;

// protocol.rs:562-585 —— 内联在 select! 的 accept 分支里，spawn 之前
match filter(&incoming) {
    IncomingFilterOutcome::Accept => {}
    IncomingFilterOutcome::Retry  => { /* incoming.retry() → 发 QUIC RETRY 包 */ continue; }
    IncomingFilterOutcome::Reject => { incoming.refuse(); continue; }   // 对端立刻收到 CONNECTION_REFUSED
    IncomingFilterOutcome::Ignore => { incoming.ignore(); continue; }   // 不回任何包，对端超时
}
```

**三点：**
1. **filter 是同步闭包**（`Fn` 不是 async），跑在 accept 循环主线上——**里面做慢事会拖住整个 Router 的 accept 吞吐**
2. 想在这一层看 ALPN 只能 `Incoming::decrypt()` 解 Initial 包偷看 ClientHello（~1200 字节的 clone + 解密）。官方明说贵（`protocol.rs:465-468`）：「Note that `decrypt()` is relatively expensive, so filters should reject based on cheaper signals (e.g. remote address) first.」
3. **`Retry` 的语义随连接类型而变**（`protocol.rs:170-208` 写得非常细）：直连 UDP 上它是真的源地址验证（防伪造源地址的放大攻击）；走 relay 时没有源地址可验，它只是给客户端**强加一个 relay 往返 + 重发 ClientHello 的成本**

> libp2p 的 `handle_pending_inbound_connection` 大致对应这一层，但它拿不到 QUIC RETRY 这种传输层原语，也没有 `Ignore`（静默丢弃）——因为 libp2p 的 transport 抽象把 QUIC 细节藏起来了。iroh 直接坐在 QUIC 上，把 RETRY / refuse / ignore 原样暴露，**DoS 防护手段更细**。

## Router vs 手写 accept 循环：真实差异在「所有权」

官方给了同一个 echo 协议的两个版本：`examples/echo.rs`（Router）与 `examples/echo-no-router.rs`（手写）。

```rust
// echo.rs:71-78
async fn start_accept_side() -> Result<Router> {
    let endpoint = Endpoint::bind(presets::N0).await?;
    let router = Router::builder(endpoint).accept(ALPN, Echo).spawn();
    Ok(router)
}

// echo-no-router.rs:69-90 —— 手写版必须自己做：alpns() 声明 + accept 循环 + per-conn spawn + task 生命周期
let endpoint = Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]).bind().await?;
tokio::spawn({
    let endpoint = endpoint.clone();
    async move {
        // In a more serious environment, we recommend avoiding `tokio::spawn` and use either a
        // `TaskTracker` or `JoinSet` instead to make sure you're not accidentally leaking tasks.
        while let Some(incoming) = endpoint.accept().await {
            tokio::spawn(async move { let connection = incoming.await.anyerr()?; ... });
        }
    }
});
```

**对 echo 这种形状的协议二者等价**（echo-no-router 自己的注释就承认 `tokio::spawn` 是简化写法、生产该用 TaskTracker/JoinSet——而那正是 Router 已经写好的）。

**但存在真实的能力差异**：`incoming_filter` 的签名是 `Arc<dyn Fn(&Incoming) -> ...>`（`protocol.rs:214-215`）——**只给借用，拿不到 `Incoming` 的所有权**。所以「把 Incoming 塞进自己的连接池/延迟队列」这类模式 **Router 结构上做不到**，只能手写 accept 循环。

**判据**：默认用 Router。手写只在以下情况：
- 需要 **Incoming 的所有权**（转交给自己的连接池/限流器）
- 需要 0-RTT 的完整 Incoming 控制（`examples/0rtt.rs:145-148` 就是手写 `endpoint.accept()` + `incoming.accept()` + `accepting.into_0rtt()`）
- 要接管 task 调度策略

手写版的关闭靠 `endpoint.close()` 让 `accept()` 返回 `None` 来间接终止循环。

> libp2p 没有这个二选一——你永远得自己写事件循环。iroh 的 Router 大致相当于给你预制了一个 swarm 事件循环 + 分发表，而 `endpoint.accept()` 才是接近 libp2p 心智（自己 poll、自己 spawn）的那条路。

## ⚠️ 一个 handler panic 会掀掉整个 Router 并关掉共享 Endpoint

```rust
// protocol.rs:533-546
Some(res) = join_set.join_next() => {
    match res {
        Err(outer) => {
            if outer.is_panic() {
                error!("Task panicked: {outer:?}");
                break;                      // ← 掀掉整个 accept 循环
            } else if outer.is_cancelled() { trace!("Task cancelled: {outer:?}"); }
            else { error!("Task failed: {outer:?}"); break; }
        }
        ...
    }
},

// protocol.rs:597-602 —— break 之后无条件跑的收尾
protocols.shutdown().await;
handler_cancel_token.cancel();
// Now we close the endpoint. This will force-close all connections that are not yet closed.
endpoint.close().await;                      // ← 关掉被所有协议共享的 Endpoint
```

**一个协议 handler 里的 panic 会连带杀死同 Router 上的其它所有协议，以及经由该 Endpoint 建立的出站连接。**

**而且文档与实现对不上**：`Router::shutdown` 的文档（`protocol.rs:426-428`）声称「If some ProtocolHandler panicked in the accept loop, this will propagate that panic into the result here.」——但代码里**没有任何 `resume_unwind`**：`run_loop_fut` 只 `error!` 一行然后 break，自身正常结束，所以 `task.await?` 返回 `Ok(())`。

（已确认 `n0_future::task::JoinSet` 在非 wasm 下就是 `tokio::task::JoinSet` 的 re-export——`n0-future-0.3.2/src/task.rs:7`——所以 `join_next` 是 tokio 标准语义。）

**实践结论：handler 里千万别 unwrap/expect/索引越界。一次 panic = 整个节点静默下线，且只在日志里留一行 `error!`。**

> **libp2p 对照**：libp2p 里事件循环是你自己的，behaviour 的 poll 是同步调用——panic 直接把你的主循环 task 炸掉，至少你能在自己的 loop 外面 catch/重启，**故障边界是显式的**。iroh 的 Router 把 task 藏在内部，panic 被 JoinSet 捕获后 Router 选择「自杀式关停」，从外部看只是 endpoint 悄悄 closed、`Router::shutdown()` 还返回 Ok——**故障边界隐式且不易察觉**。

## 关闭编排：严格四步

```rust
// protocol.rs:597-612
protocols.shutdown().await;      // ① 并发调用所有 handler 的 shutdown() 并等全部完成
handler_cancel_token.cancel();   // ② 取消仍在跑的 accept future
endpoint.close().await;          // ③ 强关所有还没关的连接
join_set.abort_all();            // ④ 兜底（应该是 noop）
while let Some(res) = join_set.join_next().await { ... }

// protocol.rs:395-401 —— 并发而非串行
pub(crate) async fn shutdown(&self) {
    let handlers = self.0.values().map(|p| p.shutdown());
    join_all(handlers).await;
}
```

trait 文档明写这个契约（`protocol.rs:270-272`）：「the futures returned by `Self::accept` will be aborted **after** the future returned from `ProtocolHandler::shutdown` completes.」

**契约有测试兜底**：`test_graceful_shutdown`（`protocol.rs:1008-1058`）里 `TestProtocol::accept` 把 Connection 存进 Vec 就返回，`shutdown()` 里 sleep 100ms 再逐个 `conn.close(42u32.into(), b"shutdown")`，最后断言对端收到的正是 `ApplicationClosed { error_code: 42, reason: "shutdown" }`。

**这说明「在 `accept()` 里存下 Connection、在 `shutdown()` 里统一优雅关闭」是官方认可的模式。**

**反过来：如果你的 handler 不实现 `shutdown()`（默认空 impl），`Router::shutdown` 会立刻走到 `endpoint.close()`，你的连接就是被强关的，对端收不到任何 application close code。**

> **libp2p 对照**（常见误传，请勿写成「libp2p 没有关闭编排」）：libp2p **有** `ConnectionHandler::poll_close`（`libp2p-swarm-0.47.1/src/handler.rs:179`），文档明写「When a connection is being shut down, we will first poll this function to completion. Following that, the physical connection will be shut down.」——这就是一个被 Swarm 驱动至完成的优雅关闭钩子，语义上正对应 `ProtocolHandler::shutdown`。另有 `Swarm::close_connection`（`lib.rs:658`）/ `disconnect_peer_id`（`lib.rs:637`）。
>
> **libp2p 真正缺的是「全局的、可 await 的 Swarm 级 `shutdown()`」**（确实不存在），不是「没有关闭编排」。

## Router 的其它语义

**Clone + 幂等 shutdown**（`protocol.rs:96-102, 415-446`）：

```rust
#[derive(Clone, Debug)]
pub struct Router {
    endpoint: Endpoint,
    task: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    cancel_token: CancellationToken,
}

pub fn is_shutdown(&self) -> bool { self.cancel_token.is_cancelled() }

pub async fn shutdown(&self) -> Result<(), JoinError> {
    if self.is_shutdown() { return Ok(()); }
    self.cancel_token.cancel();
    let task = self.task.lock().expect("poisoned").take();
    if let Some(task) = task { task.await?; }
    Ok(())
}
```

`run_loop_fut` 开头挂了 `let _cancel_guard = cancel_token.clone().drop_guard();`（`protocol.rs:520-522`），所以循环无论因何退出（收到 cancel / endpoint 被关 / handler panic）都会把 token 置为 cancelled。

**推论：**
- **`is_shutdown() == true` 不等于「有人调过 shutdown()」**，它只表示 accept 循环已经不在跑了——handler panic 或 endpoint 被别处 close 掉都会让它变 true。想探测「节点是不是意外挂了」可以轮询这个
- `shutdown()` 里 `.take()` 了 task，**只有第一个调用者真正 await 到 task 结束**；并发调用的其它方因 `is_shutdown()` 已 true 立刻返回 Ok——即「shutdown 返回」不严格等于「关闭序列已完成」

**per-connection task + span**（`protocol.rs:587-592`）：

```rust
let token = handler_cancel_token.child_token();
let span = info_span!("router.accept", me=%endpoint.id().fmt_short(), remote=Empty, alpn=Empty);
join_set.spawn(async move {
    token.run_until_cancelled(handle_connection(incoming, protocols)).await
}.instrument(span));
```

**一个 handler 阻塞/慢速不会影响其它连接或其它协议的 accept。** 这是 iroh 相对 libp2p 的核心工程优势——libp2p 所有 Behaviour 共享 Swarm 的单一 poll 循环，任何一个 behaviour 的 poll 里做了阻塞或重活，整个 swarm（含 ping/kad/所有连接）一起卡住（这也是为什么 libp2p 强制 poll-based 而非 async fn）。代价是上面说的：**panic 的爆炸半径反而更大**。

> **`join_set` 无上限**——Router 不做并发连接数限流。要限流得自己在 `incoming_filter` 或 `on_accepting` 里做（这正是 `screening-connection.rs` 的用意，头注释：「Maybe it's ... serving too many connections」）。

### 调试提示（注意一条常见的错误建议）

开 `RUST_LOG=iroh=debug` 后每条连接都有 `router.accept` span，带 `me` / `remote` / `alpn` 三个字段。

⚠️ **但 ALPN 不匹配时你看不到对端提议的 ALPN**：rustls 在握手阶段就发 `no_application_protocol` alert，`Accepting::alpn(&mut self)`（`connection.rs:558`）委托给 `alpn_from_noq_connecting`（`:279`），其 `conn.handshake_data().await?` 直接返回 Err → 走的是 `protocol.rs:636` 的 `warn!("Ignoring connection: invalid handshake: {err:#}")`，**不是** `:643` 的 "unsupported ALPN protocol"。而 span 的 alpn 字段在 `:640` 才 record，`:636` 的失败发生在它之前，**所以 alpn 字段仍是 Empty**。

（`:643` 那个分支在纯 Router 用法下基本走不到——因为 `set_alpns` 的列表就是从同一个 ProtocolMap 的 keys 生成的，除非你在 `spawn()` 之后又手动调了 `endpoint.set_alpns()`。）

`remote` 字段只在 `on_accepting` 成功之后才 record，所以被 `on_accepting` 拒掉的连接日志里 `remote` 是空的。

## 0-RTT：on_accepting 的第二个用途

```
// protocol.rs:229-247
/// However, this method can also be used to implement the accept side of a
/// 0-RTT connection.
/// `ProtocolHandler::on_accepting` allows you to take over the connection
/// state machine early in the handshake processes, by calling [`Accepting::into_0rtt`].
///
/// When working with 0-RTT, you may want to implement all of your protocol
/// logic in `on_accepting`. This is fine because `on_accepting` can handle
/// long-running processes. In this case, the [`ProtocolHandler::accept`] method
/// can simply return `Ok(())`.
```

**两边签名不对称，别照抄**：
- 接受侧 `Accepting::into_0rtt()` → `IncomingZeroRttConnection`（**不会失败**，`connection.rs:620-624` 里 `.expect("incoming connections can always be converted to 0-RTT")`）
- 连接侧 `Connecting::into_0rtt()` → `Result<OutgoingZeroRttConnection, Connecting>`（可能被拒，把 Connecting 还给你）

**安全提醒是硬性的**：0-RTT 数据可被重放，**绝不能用来触发非幂等操作**（`connection.rs:605-608`）。

官方 `0rtt.rs` 例子走的是手写 `endpoint.accept()` 而不是 Router——说明 Router + 0-RTT 这条路虽然文档支持，但**缺可抄的完整范例**。

> libp2p 的 QUIC transport 目前不把 0-RTT 暴露给上层——transport 抽象吃掉了这个能力。iroh 因为不做 transport 抽象、直接暴露 QUIC 语义，0-RTT / RETRY / ALPN 这些都能用上。**这是「薄封装」的收益面。**

## 没有 Codec：你拿到的是裸流

```rust
// iroh/iroh/examples/search.rs:129-161 —— 官方范例里的「协议」就是这样手搓的
async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
    let (mut send, mut recv) = connection.accept_bi().await?;
    // 手工限长防 OOM
    let query_bytes = recv.read_to_end(64).await.map_err(AcceptError::from_err)?;
    let query = String::from_utf8(query_bytes).map_err(AcceptError::from_err)?;
    let num_matches = self.query_local(&query).await;
    // 手工编码
    send.write_all(&num_matches.to_le_bytes()).await.map_err(AcceptError::from_err)?;
    send.finish()?;
    connection.closed().await;
    Ok(())
}
```

**「一次请求 = 一条 bi 流」这个约定是范例自己定的，不是 iroh 的规定**——iroh 层面你想开多少条流、单向还是双向、怎么复用，完全自定。`read_to_end(n)` 的 `n` 是防 OOM 的硬性限长，**必须自己给**。

`iroh_blobs` 之类的官方协议 crate 会把自己的 framing 封好并直接 `impl ProtocolHandler`（`sendme/src/main.rs:725-727`：`.accept(iroh_blobs::ALPN, blobs.clone())`），所以「用现成协议」很省事，「**自定义协议则完全从零**」。

> **这是与 request_response 模型的本质区别。** libp2p 的 `request_response::Behaviour<C: Codec>` 给你一整套：Codec trait、自动的 outbound 流管理、RequestId 关联、ResponseChannel、超时、失败分类，以及 `swarm.behaviour_mut().send_request(&peer, req)` 这种类型化 API。**iroh 这一层完全没有**——你拿到的是 tokio `AsyncRead`/`AsyncWrite`。
>
> 所以 libp2p 里「定义 Request/Response 类型 + 一个 Codec 就能跑」的东西，迁到 iroh 要**自己实现 framing 与请求关联**（官方范例 `search.rs` 的「一次请求 = 一条 bi 流」只是范例自定的约定，不是 iroh 的规定；`read_to_end(n)` 的防 OOM 限长也必须自己给）。反过来，iroh 也没有「Codec 必须适配 Behaviour 的 poll 语义」这个约束，**长连接流式传输（大文件分块）写起来自然得多**。
