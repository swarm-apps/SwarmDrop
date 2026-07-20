# 写你自己的协议（Write your own Protocol）

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`
官方页：<https://docs.iroh.computer/protocols/writing-a-protocol.md> · <https://docs.iroh.computer/concepts/protocols.md>

> 这一页面向**「我要从零写一个协议」这个动作**：模板 → ALPN → 错误处理 → 六个设计决策 → 上线 checklist。
>
> **QUIC 原语本身**（open/accept、finish/reset/stop、背压、优先级、datagram、关闭时机）
> → [03a-using-quic.md](03a-using-quic.md)。**三层入站门禁** → [02-connecting.md](02-connecting.md)。

---

# 完整模板（1.0.2 可编译，直接抄）

这是 `iroh/iroh/examples/echo.rs` 的全文骨架（113 行，本地已通读）。**它和官方页上的版本有 4 处不一样**，下一节逐条列。

```rust
use iroh::{
    Endpoint, EndpointAddr,
    endpoint::{Connection, presets},
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_error::{Result, StdResultExt};

/// Each protocol is identified by its ALPN string.
///
/// The ALPN, or application-layer protocol negotiation, is exchanged in the connection handshake,
/// and the connection is aborted unless both endpoints pass the same bytestring.
const ALPN: &[u8] = b"iroh-example/echo/0";                    // ① ALPN 常量

#[derive(Debug, Clone)]                                        // ② Debug 是 supertrait，必须有
struct Echo;

impl ProtocolHandler for Echo {                                // ③ 实现 accept
    /// The `accept` method is called for each incoming connection for our ALPN.
    ///
    /// The returned future runs on a newly spawned tokio task, so it can run as long as
    /// the connection lasts.
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let endpoint_id = connection.remote_id();
        println!("accepted connection from {endpoint_id}");

        // 一问一答：期待对端开一条 bi 流
        let (mut send, mut recv) = connection.accept_bi().await?;

        // 把收到的字节原样回传，直到对端 finish
        let bytes_sent = tokio::io::copy(&mut recv, &mut send).await?;
        println!("Copied over {bytes_sent} byte(s)");

        // 声明「我不再发了」，让对端的 recv 流终止
        send.finish()?;

        // 等对端先关连接——它读完响应就会关
        connection.closed().await;

        Ok(())
    }
}

async fn start_accept_side() -> Result<Router> {               // ④ Router 注册
    let endpoint = Endpoint::bind(presets::N0).await?;
    let router = Router::builder(endpoint).accept(ALPN, Echo).spawn();
    Ok(router)
}

async fn connect_side(addr: EndpointAddr) -> Result<()> {      // ⑤ 连接侧：accept 的镜像
    let endpoint = Endpoint::bind(presets::N0).await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    let (mut send, mut recv) = conn.open_bi().await.anyerr()?;
    send.write_all(b"Hello, world!").await.anyerr()?;          // ⚠️ 开流方必须先写！
    send.finish().anyerr()?;

    let response = recv.read_to_end(1000).await.anyerr()?;     // ⚠️ 限长必须给
    assert_eq!(&response, b"Hello, world!");

    conn.close(0u32.into(), b"bye!");                          // 最后读数据的一侧负责 close
    endpoint.close().await;                                   // close() 只入队，这句才真发出去
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let router = start_accept_side().await?;

    router.endpoint().online().await;                          // ⑥ 等上线（官方页没有这句！）
    connect_side(router.endpoint().addr()).await?;             // ⑦ addr() 非 async！

    router.shutdown().await.anyerr()?;
    Ok(())
}
```

**七个位置的对应源码**：① `echo.rs:20` ② `:80-81` ③ `:83-112` ④ `:71-78`（`:75` 是 `Router::builder(endpoint).accept(ALPN, Echo).spawn()`）⑤ `:38-69` ⑥ `:28` ⑦ `:30`。

## 镜像关系（官方页讲得最好的一段）

> The connecting side is going to be the mirror image of the accepting side:
> * An `accept_bi` corresponds to an `open_bi`,
> * when data is received, the other side sends data,
> * when one side waits for `connection.closed()`, the other calls `connection.close()`.

**写协议时把两侧并排放在一个文件里，逐行对照着写。** 任何一侧多一个 `await` 少一个 `finish`，症状都是 30 秒后超时（`max_idle_timeout` 默认 30_000ms，`noq-proto-1.0.1/src/config/transport.rs:554`）。

---

# 官方 writing-a-protocol 页 vs 1.0.2：4 处不一致

| 官方页 | 仓内 `echo.rs`（1.0.2） | 后果 |
|---|---|---|
| `let endpoint_addr = router.endpoint().addr().await?;` | `router.endpoint().addr()`（`echo.rs:30`）——`pub fn addr(&self) -> EndpointAddr`（`iroh/iroh/src/endpoint.rs:1196`），**非 async** | 编译错误：`EndpointAddr` 上没有 `.await` |
| 无 | `router.endpoint().online().await;`（`echo.rs:28`，`endpoint.rs:1355`）——**官方页整段缺失** | 不等上线就取 `addr()`，拿到的地址可能还没有 relay/直连信息，连接侧偶发连不上 |
| `use anyhow::Result` 风格，`Endpoint::bind(...).await?` 直接 `?` | `n0_error::{Result, StdResultExt}`，跨库错误要 `.anyerr()`（`echo.rs:14, :45, :48, :51, :54`） | 照抄会遇到一串 `?` 转换失败 |
| `#[derive(Debug, Clone)] struct Echo;`＋`impl iroh::protocol::ProtocolHandler for Echo` | 一致 | — |

**另外**：官方页的 `Router::builder(endpoint).spawn()`（第一段还没 `.accept()` 时）**不是「拒绝所有连接」那么无害**——它会 `set_alpns(vec![])`，把 Endpoint 的入站 ALPN **清空**。详见下方「`set_alpns` 是覆盖式的」。

> **总评**：`writing-a-protocol` 页的**结构与设计论述可靠**（镜像关系、General Guidance 三条），**代码要按上表修**。`using-quic` 页的代码问题更大（见 [03a-using-quic.md](03a-using-quic.md) 开头的表）。

---

# Router 与 ALPN 分发：机制层
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

# ALPN：命名约定与版本策略

## 官方指引（`concepts/protocols.md`，原文）

> Protocols use "Application-Layer Protocol Negotiation" (ALPN) strings to identify themselves. When an endpoint accepts a connection, it uses the ALPN to route to the correct protocol handler.
>
> For example, `iroh-blobs` uses the ALPN `/iroh-bytes/VERSION`.
>
> You can also use multiple ALPN identifiers for version negotiation. A connecting peer might offer `/my-protocol/2` and `/my-protocol/1`, letting the accepting peer respond with whichever version it supports.

## 真实世界调查：**没有统一约定**

| 协议 | ALPN 字节串 | 源码位置 |
|---|---|---|
| iroh-blobs | `/iroh-bytes/4` | `iroh-blobs/src/protocol.rs:406` |
| iroh-docs | `/iroh-sync/1` | `iroh-docs/src/net.rs:18` |
| iroh-gossip | `/iroh-gossip/1` | `iroh-gossip/src/net.rs:45` |
| echo 范例 | `iroh-example/echo/0` | `iroh/iroh/examples/echo.rs:20` |
| search 范例 | `iroh-example/text-search/0` | `iroh/iroh/examples/search.rs:70` |
| transfer 范例 | `n0/iroh/transfer/example/1` | `iroh/iroh/examples/transfer.rs:68` |
| dumbpipe | `DUMBPIPEV0` | `dumbpipe/src/lib.rs:5` |

**观察到的规律（不是强制规范）：**

1. **唯一真正的共性是「名字 + 版本号」**——连 `DUMBPIPEV0` 都把版本号焊在名字尾巴上。
2. n0 自家的**库**（blobs/docs/gossip）用 `/name/N`（前导斜杠、无前导斜杠的都有人用，范例就没有）。
3. **crate 名 ≠ ALPN 名**：`iroh-blobs` 的 ALPN 是 `/iroh-bytes/4`（历史名），版本已经到 **4**。**ALPN 是线上契约，改名/改版本 = 断老客户端**，所以它跟 crate 名解耦是对的。
4. 范例用 `iroh-example/` 前缀避免和真实协议撞车。

## 判据

```rust
// 推荐形状：反向域名或项目名做前缀，斜杠分段，末尾版本号
const ALPN: &[u8] = b"/myapp/filesync/1";
```

- **必须带版本号**。ALPN 匹配是**整串字节精确相等**——没有前缀匹配、没有通配、没有自动回退（`ProtocolMap` 就是 `BTreeMap<Vec<u8>, _>`，`iroh/iroh/src/protocol.rs:377-388`）。没版本号 = 以后没法演进。
- **加个项目独有前缀**。同一 ALPN 注册两次是**静默覆盖**（`BTreeMap::insert` 语义），症状是「其中一个协议神秘失灵」而不是启动报错——两个库撞 ALPN 时你查不出来。详见下方「同一 ALPN 注册两次 = 静默覆盖」。
- **别用 crate 名当 ALPN**，crate 会改名，ALPN 不能。

## 版本演进：注册多个 ALPN

同一个 handler 注册到多个 ALPN 是合法且正规的做法：

```rust
const ALPN_V1: &[u8] = b"/myapp/filesync/1";
const ALPN_V2: &[u8] = b"/myapp/filesync/2";

let router = Router::builder(endpoint)
    .accept(ALPN_V2, handler.clone())      // 新版
    .accept(ALPN_V1, handler)              // 兼容老客户端
    .spawn();
```

handler 内部靠 `connection.alpn()` 分流（`iroh/iroh/src/endpoint/connection.rs:1115`，返回 `&[u8]`，**读的是握手时固化的字段，无网络交互**）：

```rust
async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
    match connection.alpn() {
        ALPN_V2 => self.handle_v2(connection).await,
        ALPN_V1 => self.handle_v1(connection).await,
        _ => unreachable!("router 只会把注册过的 ALPN 派进来"),
    }
}
```

连接侧可以用 `ConnectOptions::with_additional_alpns` 一次提议多个，**由接受侧决定最终协议**（`endpoint.rs:1786-1809`：「The ALPN identifier order on the connect side doesn't matter, since it's the accept side that determines the protocol.」）。

> ⚠️ **一条连接终身绑定一个 ALPN**。这跟 libp2p 的 multistream-select（每条 substream 各自协商协议名）根本不同——iroh 下要在一条连接里同时跑两个协议是**做不到的**，得开两条连接（复用同一个 Endpoint 和同一次打洞的路径）。详见下方「ALPN 两层分发，粒度是「整条连接」」。

---

# ProtocolHandler 与错误处理

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

## `?` 的行为不直观：什么时候要 `map_err`

`AcceptError` 只对少数类型有 `From`。看官方两个范例的**差异**就懂了：

**echo.rs 全程裸 `?`**（`:95-104`）——因为它只碰到有 `From` 的类型：

```rust
let (mut send, mut recv) = connection.accept_bi().await?;         // ConnectionError → From ✓
let bytes_sent = tokio::io::copy(&mut recv, &mut send).await?;    // io::Error → From ✓
send.finish()?;                                                   // ClosedStream → From ✓
```

**search.rs 三处必须 `map_err`**（`:139-150`）：

```rust
let query_bytes = recv.read_to_end(64).await.map_err(AcceptError::from_err)?;   // ReadToEndError ✗
let query = String::from_utf8(query_bytes).map_err(AcceptError::from_err)?;     // FromUtf8Error ✗
send.write_all(&num_matches.to_le_bytes()).await
    .map_err(AcceptError::from_err)?;                                           // WriteError ✗
send.finish()?;                                                                 // ClosedStream ✓
```

**规则**：

| 类型 | `?` 直接可用？ |
|---|---|
| `ConnectionError`（`accept_bi` / `open_bi`） | ✓ 走 `AcceptError::Connection` |
| `std::io::Error`（`tokio::io::copy` 等） | ✓ 走 `AcceptError::User`（经 `From<io::Error>`） |
| `quic::ClosedStream`（`finish` / `reset`） | ✓ 走 `AcceptError::User` |
| `ReadToEndError` / `ReadError` / `WriteError` / 你的业务错误 | ✗ **要 `.map_err(AcceptError::from_err)`** |

> ⚠️ **`io::Error` 和 `ClosedStream` 都被塞进 `User` 变体**——看日志时区分不出「是 IO 挂了还是业务逻辑挂了」。要可诊断就自己定义错误枚举再 `from_err`。iroh-gossip 就是这么做的：自建 `WriteError`/`ReadError`（`iroh-gossip/src/net/util.rs:34-56`），在 `ProtocolHandler::accept` 边界上一次性 `.map_err(AcceptError::from_err)`（`iroh-gossip/src/net.rs:130-135`）。

## 返回 `Err` **不会**给对端发错误码

trait 文档明写（`protocol.rs:258-272`，节选）：

> Returning an `Err<AcceptError>` will also drop the connection and log a warning, but **no dedicated error code will be sent to the peer**, so it's recommended to explicitly close the connection within your accept handler.

**Router 对任何 `Err` 的处理都是 `warn!` 一行然后 drop——没人替你重试、没人统计。** 想让对端知道原因，自己 `connection.close(code, reason)`。

---

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

# 设计要点 1：流的粒度（本页最重要的决策）

## 前提：「流很便宜」是写进 rustdoc 的合同，不是社区说法

iroh 与 noq 两层文档共 **7 处**明确表述，**唯一的开销来源是流控，不是流本身**。
逐字引文与出处（`lib.rs:136-148` / `connection.rs:817-830` / `endpoint.rs:875` / noq `connection.rs:321,333`）
→ [03a-using-quic.md](03a-using-quic.md) 的「「流很便宜」是写进 rustdoc 的合同」一节。

**本节只用这个前提推设计结论**，不重复引文。

## 从这些文档能推出的四条收益

多开流你**白拿**：

1. **零额外开销**——「does not incur any extra overhead compared to sending data on an existing stream」。开流不发包、不往返，纯本地记账。
2. **无队头阻塞**——「packet loss on one stream will not delay other streams」。一条流丢包不拖累别的流。
3. **独立取消粒度**——每条流可以单独 `reset`/`stop`，QUIC 立刻停止为它重传（见 [03a-using-quic.md](03a-using-quic.md)）。
4. **可交错并发**——「streams can be interleaved and will not block each other... regardless of how long they last」。

## 那什么时候该复用一条流？

**只有一个理由：顺序。** QUIC 只保证**单条流内**字节严格有序（「The data sent in each stream is delivered strictly ordered」），**跨流无序**。

两个次要理由：

- **并发额度**：默认 `max_concurrent_bidi_streams: 100`（`noq-proto-1.0.1/src/config/transport.rs:552-553`）。超了是 `Poll::Pending` **静默排队**，不报错——长生命周期的「一消息一流」超过 100 就会莫名卡住。
- **内存**：每条流一个 `stream_receive_window`（默认 1.25MB），worst-case 100 × 1.25MB = 125MB/连接（推导见 [03a-using-quic.md](03a-using-quic.md)）。

## 判据：**流的边界应该画在「顺序域」的边界上**

这不是我编的，是 iroh-gossip 的实际选择：**per-topic 一条 uni 流**——同一 topic 内的消息需要有序，跨 topic 不需要，所以 topic 就是顺序域，流边界就画在那儿。

```rust
// iroh-gossip/src/net/util.rs:198-205
pub(crate) struct SendLoop {
    conn: Connection,
    streams: HashMap<TopicId, SendStream>,       // ← 一个 topic 一条流，长期持有
    buffer: Vec<u8>,
    max_message_size: usize,
    finishing: JoinSet<()>,
    send_rx: mpsc::Receiver<ProtoMessage>,
}

// :274-293 —— 按 topic 懒开流，开流后先写 header 标明自己是哪个 topic
pub async fn write_message(&mut self, message: &ProtoMessage) -> Result<(), WriteError> {
    let ProtoMessage { topic, message } = message;
    let topic_id = *topic;
    let mut entry = match self.streams.entry(topic_id) {
        hash_map::Entry::Occupied(entry) => entry,
        hash_map::Entry::Vacant(entry) => {
            let mut stream = self.conn.open_uni().await?;
            let header = StreamHeader { topic_id };
            header.write(&mut stream, &mut self.buffer, self.max_message_size).await?;
            debug!(topic=%topic_id.fmt_short(), "stream opened");
            entry.insert_entry(stream)
        }
    };
    let stream = entry.get_mut();
    write_frame(stream, message, &mut self.buffer, self.max_message_size).await?;
    ...
}
```

接收侧对称——accept 到一条 uni 流，先读 header 知道 topic，之后这条流上的所有帧都属于该 topic（`iroh-gossip/src/net/util.rs:169-179` 的 `RecvStreamState::new`）。

## 真实实现的完整光谱

| 实现 | 流的粒度 | 顺序域 | 源码 |
|---|---|---|---|
| `echo.rs` | **1 条 bi 流 / 连接**（用完连接就关） | 整条连接 | `echo.rs:95` |
| `dumbpipe` | **1 条 bi 流 / 本地 socket 连接**（一条 iroh 连接上开多条） | 每个本地连接 | `dumbpipe/src/main.rs:807-811` |
| `transfer.rs` | **1 条 bi 流 / 请求**（`loop { accept_bi }`） | 每个请求 | `transfer.rs:660-665, :805` |
| `iroh-gossip` | **1 条 uni 流 / topic**（长生命周期 + framing） | 每个 topic | `net/util.rs:198-205, :274-293` |
| MoQ（官方页举例，非本仓） | **1 条 uni 流 / 视频帧** | 每一帧 | 官方 using-quic 页 |

**这五档从「一条流管所有」到「一帧一条流」，全部是合理设计**——差别只在顺序域有多大。

## 决策表

| 问题 | 答案 → 做法 |
|---|---|
| 这些消息之间需要严格全序吗？ | **要** → 同一条流 + framing<br>**不要** → 一条消息一条流 |
| 需要单独取消某条消息吗？ | **要** → 一条消息一条流（才能 `reset`/`stop`） |
| 消息之间丢包应该互相拖累吗？ | **不该** → 分流（消灭队头阻塞） |
| 同时在飞的流会超过 100 吗？（且是长生命周期） | **会** → 要么合流，要么让**对端**调大 `max_concurrent_*_streams` |
| 单条流可能堆积很大而应用来不及读吗？ | **会** → 算一下 `并发流数 × stream_receive_window` |

> **从 libp2p 迁过来的人注意**：libp2p 每开一条 substream 都要一次 multistream-select 往返，所以那边「把多个逻辑通道合并到一条流」是**优化**。**iroh 下这么做是反优化**——你白白放弃了无队头阻塞和独立取消，换来的只是本来就不需要的顺序保证。这是最常见的迁移误判。

---

# 设计要点 2：连接的生命周期 = `accept()` 的生命周期

trait 文档（`iroh/iroh/src/protocol.rs:258-272`，节选）：

> The returned future runs on a freshly spawned tokio task so it can be long-running. **Once `accept()` returns, the connection is dropped. This means that it will be closed if there are no other clones of the connection.**

**这句话里藏着两种完全不同的协议形状。**

## 形状 A：在 `accept()` 里跑完整个连接（echo / search / blobs）

`accept()` 的 future **就是**这条连接的一生。**要保持连接就别 return。**

```rust
async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
    let (mut send, mut recv) = connection.accept_bi().await?;
    tokio::io::copy(&mut recv, &mut send).await?;
    send.finish()?;
    connection.closed().await;        // ← 不 await 这句就 return，响应可能被截断
    Ok(())
}
```

**长连接版**就是套个 loop（官方页 General Guidance 的第一条建议）：

> **Re-use connections**: The version of the echo protocol above simply closes the connection after having echo-ed one stream. This is needlessly wasteful... Instead, you could put a loop around `connection.accept_bi()` to accept multiple streams to echo on for the same connection. In practice, protocols often re-use the same connection for performance.

即 `transfer.rs:660-665` 那个形状。

## 形状 B：`accept()` 立刻返回，把 Connection 的 clone 交给长生命周期 actor（gossip）

gossip 利用的正是文档里那个逃生口——**"if there are no other clones"**：

```rust
// iroh-gossip/src/net.rs:129-142
impl ProtocolHandler for Gossip {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.handle_connection(connection)          // ← 塞给 actor 后立刻返回
            .await
            .map_err(AcceptError::from_err)?;
        Ok(())
    }

    async fn shutdown(&self) {
        if let Err(err) = self.shutdown().await {   // ← 调的是固有方法（inherent 优先于 trait）
            warn!("error while shutting down gossip: {err:#}");
        }
    }
}

// :246-252 —— Connection 被 move 进 channel，actor 端持有它 → 连接不会被关
pub async fn handle_connection(&self, conn: Connection) -> Result<(), Error> {
    self.inner.local_tx.send(LocalActorMessage::HandleConnection(conn)).await?;
    Ok(())
}
```

> **注意 `shutdown()` 里的 `self.shutdown()`**：Gossip 有一个固有的 `pub async fn shutdown(&self) -> Result<(), Error>`（`net.rs:258-266`），**Rust 的方法解析里固有方法优先于 trait 方法**，所以这不是无限递归。这个写法很紧凑但容易看懵，抄的时候心里有数。

**形状 B 的代价**：连接的所有权跑到了 actor 里，`accept()` 的返回值不再代表连接结局，**错误也不会经过 `AcceptError` 报出来**——gossip 自己在 actor 里处理（`net.rs:564-576`）。选它的唯一理由是**协议本身是 peer-to-peer 的、连接要参与全局状态机**（gossip 要把所有 peer 连接放一起做 membership）。

## 判据

| 你的协议 | 形状 |
|---|---|
| 请求-响应型（client/server 味道）：blobs、sendme、dumbpipe、irpc | **A**——在 `accept()` 里跑完 |
| 连接要参与全局状态机、要和别的连接互相影响：gossip、docs | **B**——交给 actor |

官方 `concepts/protocols.md` 也是这么分的（原文）：

> Sendme, dumbpipe, iroh-blobs and all irpc based protocols have clear client and server roles at the application level.
> Gossip and docs are examples of peer-to-peer protocols.

---

# 设计要点 3：开流方必须先写（否则死锁）

**这是从 libp2p 迁过来必踩的第一个坑**，也是「流为什么便宜」的代价面。

```
// iroh/iroh/src/lib.rs:151-156（warning 块）
//! To keep streams cheap, they are lazily created on the network: only once a sender starts
//! sending data on the stream will the receiver become aware of a stream.  This means only
//! calling [`Connection::open_bi`] is not sufficient for the corresponding call to
//! [`Connection::accept_bi`] to return.  The sender **must** send data on the stream before
//! the receiver's [`Connection::accept_bi`] call will return.
```

**`open_bi()` 不发任何网络包（纯本地记账），对端完全不知道这条流存在。** A `open_bi()` 后等 B 先说话 → B 卡在 `accept_bi()` 上 → 永久死锁。

## dumbpipe 的解法：一个 5 字节的握手常量

dumbpipe 是「双向裸管道」，**它没有天然的「谁先说话」**（可能两边都在等 stdin），所以专门造了个常量来破局：

```rust
// dumbpipe/src/lib.rs:1-11
/// The ALPN for dumbpipe.
///
/// It is basically just passing data through 1:1, except that the connecting
/// side will send a fixed size handshake to make sure the stream is created.
pub const ALPN: &[u8] = b"DUMBPIPEV0";

/// The handshake to send when connecting.
///
/// The side that calls open_bi() first must send this handshake, the side that
/// calls accept_bi() must consume it.
pub const HANDSHAKE: [u8; 5] = *b"hello";
```

连接侧（`dumbpipe/src/main.rs:441-443`，注释一针见血）：

```rust
// the connecting side must write first. we don't know if there will be something
// on stdin, so just write a handshake.
s.write_all(&dumbpipe::HANDSHAKE).await.anyerr()?;
```

接受侧（`dumbpipe/src/main.rs:402-407`）：

```rust
if !args.common.is_custom_alpn() {
    // read the handshake and verify it
    let mut buf = [0u8; dumbpipe::HANDSHAKE.len()];
    r.read_exact(&mut buf).await.anyerr()?;
    ensure_any!(buf == dumbpipe::HANDSHAKE, "invalid handshake");
}
```

## 判据

| 你的协议 | 做法 |
|---|---|
| 开流方天然先发（请求-响应） | **不用管**——请求本身就是第一份数据。echo/search/transfer 都是这样 |
| 开流方可能长时间无话可说（管道、订阅、推送） | **发个固定握手**（dumbpipe 式），或**把方向反过来**让有话说的一方开流 |
| 一条流要标明「我是干嘛的」 | **先写一个 header 帧**——gossip 的 `StreamHeader { topic_id }` 就是一石二鸟：既破了懒创建的局，又标了 topic（`iroh-gossip/src/net/util.rs:58-61, :282-287`） |

> **gossip 的 `StreamHeader` 是这条设计的最佳实践**：你多半本来就需要在流开头说明「这条流是什么」，那就顺手用它当唤醒帧，不用额外造 HANDSHAKE 常量。

---

# 设计要点 4：framing —— 没有 Codec，自己写

**iroh 层面没有任何 framing/Codec 抽象。** 你拿到的是 `AsyncRead`/`AsyncWrite`。`iroh_blobs` 之类的官方协议 crate 把 framing 封在自己 crate 里并直接 `impl ProtocolHandler`，所以「用现成协议」很省事，「**自定义协议则完全从零**」。

官方 `using-quic` 页推荐 `tokio-util` 的 `LengthDelimitedCodec`（类型上可行——`RecvStream`/`SendStream` 实现了 tokio 的 `AsyncRead`/`AsyncWrite`，`noq-1.0.1/src/recv_stream.rs:588`、`send_stream.rs:329`，无 feature gate）。**但 n0 自己一个都没这么用。** 仓内两份真实实现都是手写 `u32` 长度前缀 + postcard：

## 实现 A：iroh-gossip（可复用、带 max_message_size 兜底）

```rust
// iroh-gossip/src/net/util.rs:377-393
pub async fn write_frame<T: Serialize>(
    stream: &mut SendStream, message: &T, buffer: &mut Vec<u8>, max_message_size: usize,
) -> Result<(), WriteError> {
    let len = postcard::experimental::serialized_size(&message)?;
    if len >= max_message_size {
        return Err(e!(WriteError::TooLarge));
    }
    buffer.clear();
    buffer.resize(len, 0u8);
    let slice = postcard::to_slice(&message, buffer)?;
    stream.write_u32(len as u32).await?;                       // ← tokio AsyncWriteExt
    stream.write_all(slice).await.map_err(io::Error::other)?;
    Ok(())
}

// :354-375 —— 读侧：EOF 返回 None（不是错误），超限返回 TooLarge
/// Reads a length prefixed buffer.
///
/// Returns the frame as raw bytes.  If the end of the stream is reached before
/// the frame length starts, `None` is returned.
pub async fn read_lp(
    reader: &mut RecvStream, buffer: &mut BytesMut, max_message_size: usize,
) -> Result<Option<Bytes>, ReadError> {
    let size = match reader.read_u32().await {                 // ← tokio AsyncReadExt
        Ok(size) => size,
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),  // 对端 finish 了
        Err(err) => return Err(err.into()),
    };
    let size = usize::try_from(size).map_err(|_| e!(ReadError::TooLarge))?;
    if size > max_message_size {
        return Err(e!(ReadError::TooLarge));                   // ← 防 OOM，必须有
    }
    buffer.resize(size, 0u8);
    ...
}
```

`read_frame` 只是 `read_lp` + `postcard::from_bytes`（`:336-348`）。

## 实现 B：transfer.rs（更简，用 postcard 的 `MaxSize` 自动推上界）

```rust
// iroh/iroh/examples/transfer.rs:240-267
#[derive(Serialize, Deserialize, MaxSize, Debug, Clone)]
enum Request {
    Download(Length),
    Upload,
}

impl Request {
    async fn read(recv: &mut RecvStream) -> Result<Self> {
        let header_len = recv.read_u32().await.anyerr()? as usize;
        ensure_any!(
            header_len <= Self::POSTCARD_MAX_SIZE,             // ← 上界由 derive(MaxSize) 算出来
            "received invalid header length"
        );
        let mut buf = vec![0u8; header_len];
        recv.read_exact(&mut buf).await.anyerr()?;
        let request = postcard::from_bytes(&buf).std_context("failed to decode request")?;
        debug!("received request {request:?}");
        Ok(request)
    }

    async fn write(&self, send: &mut SendStream) -> Result<()> {
        debug!("sending request {self:?}");
        let buf = postcard::to_stdvec(&self).unwrap();
        send.write_u32(buf.len() as u32).await.anyerr()?;
        send.write_all(&buf).await.anyerr()?;
        Ok(())
    }
}
```

**`#[derive(MaxSize)]` + `Self::POSTCARD_MAX_SIZE` 这招很值得抄**——定长消息的上界让编译器算，不用手写魔数（对比 `search.rs:139` 手写的 `read_to_end(64)`）。

## framing 的三条铁律

1. **长度前缀必须校验上界**。`read_u32` 读回来的是**对端说的**长度——不校验就是 `vec![0u8; 4GB]`。gossip 用 `max_message_size`（`util.rs:365-367`），transfer 用 `POSTCARD_MAX_SIZE`（`transfer.rs:249-252`），`search.rs` 用 `read_to_end(64)` 的硬编码限长。**三份实现，三处限长，零例外。**
2. **EOF 不是错误**。对端 `finish()` 后 `read_u32` 返回 `UnexpectedEof`，这是**正常的流结束**，要转成 `Ok(None)`（`util.rs:361`）。当错误处理会让每条正常结束的流都打一行 error。
3. **framing 函数不是 cancel-safe**。gossip 在 `RecvStreamState::next` 上明确标注「This function is not cancellation-safe」（`util.rs:185-187`）；`read_to_end` 的 rustdoc 同样标注（`noq-1.0.1/src/recv_stream.rs:257`）。**别把半个 frame 的读取放进 `tokio::select!` 分支**——取消掉就丢了半个 frame，流状态永久错位。gossip 的解法是把 `state` move 进 future 再 move 回来（`util.rs:188-195`），取消了就整个 state 一起丢。

---

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

# 设计要点 5：状态放哪

`accept(&self, ...)` 取 **`&self`**——**handler 内部状态必须自己包 `Arc<Mutex<..>>` / 原子量**。

```rust
// iroh/iroh/examples/search.rs:118-122
#[derive(Debug, Clone)]
struct BlobSearch {
    endpoint: Endpoint,                        // Endpoint 是 Clone 的，handler 自己也能主动连出去
    blobs: Arc<Mutex<BTreeSet<String>>>,       // ← 共享状态
}
```

官方 `writing-a-protocol` 页对这一点的注解：

> The struct is actually empty, because the protocol is fully stateless.
> If we were building a protocol for a database, then this struct would contain a database connection or the database contents directly, so that all connections can access it.

**注意 `BlobSearch` 里存了 `endpoint`**——这是个关键的分层信号：**handler 不只是被动接受连接，它常常也要主动发起连接**（`search.rs:178` 的 `self.endpoint.connect(endpoint_id, ALPN)`）。所以装配顺序永远是：

```rust
// iroh-examples/tauri-todos/src-tauri/src/iroh.rs:27-49
let endpoint = iroh::Endpoint::builder(presets::N0).secret_key(key).bind().await?;   // ① 先建

let gossip = Gossip::builder().spawn(endpoint.clone());                              // ② clone 给各协议
let blobs = FsStore::load(&path).await?;
let docs = Docs::persistent(path).spawn(endpoint.clone(), (*blobs).clone(), gossip.clone()).await?;

let router = iroh::protocol::Router::builder(endpoint.clone())                       // ③ 最后交给 Router
    .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
    .accept(GOSSIP_ALPN, gossip)
    .accept(DOCS_ALPN, docs.clone())
    .spawn();
```

**Router 并不「拥有」Endpoint**（Endpoint 是 Clone 的），它只是 `set_alpns` + 跑 accept 循环。

`Arc<T>` / `Box<T>` 有转发的 `ProtocolHandler` impl（`iroh/iroh/src/protocol.rs:289-315`），所以 handler 内部不必再自己套 Arc 才能共享给外部代码。

---

# 设计要点 6：优雅关闭与 `shutdown()` 钩子

关闭的**规则**（谁 close、`endpoint.close().await`、`GRACEFUL_CLOSE` 错误码、`read_to_end(0)` 确认技巧）在 [03a-using-quic.md](03a-using-quic.md) 的「关闭连接」一节；Router 的**关闭编排**（四步序列、`protocols.shutdown()` 并发调用、契约测试）见下方。这里只讲写协议时该做的决定。

## 要不要实现 `shutdown()`？

`ProtocolHandler::shutdown()` 默认是空实现（`protocol.rs:284-287`）。**不实现的后果**：`Router::shutdown()` 会直接走到 `endpoint.close()`，你的连接是**被强关的**，对端收不到任何 application close code。

trait 契约（`protocol.rs:270-272`）：

> the futures returned by `Self::accept` will be aborted **after** the future returned from `ProtocolHandler::shutdown` completes.

**「在 `accept()` 里存下 Connection、在 `shutdown()` 里统一优雅关闭」是官方认可的模式**——`protocol.rs:1008-1058` 的 `test_graceful_shutdown` 就是为这个契约兜底的（断言对端收到 `ApplicationClosed { error_code: 42, reason: "shutdown" }`）。

## 判据

| 你的协议 | `shutdown()` |
|---|---|
| 形状 A、连接短命（一次请求就关） | **不用实现**——连接本来就自己关了 |
| 形状 A、长连接（连接池/订阅） | **实现**——在 `accept()` 里把 Connection 存进 `Arc<Mutex<Vec<Connection>>>`，`shutdown()` 里逐个 `close(code, reason)` |
| 形状 B（actor 持有连接） | **实现**——转发给 actor 的关闭流程，像 gossip 那样（`iroh-gossip/src/net.rs:137-141`） |

---

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

# 要不要用 Router？

官方页的态度（`writing-a-protocol` 的 Appendix "No router no problem"，原文）：

> The router can make writing code with iroh easier, but it's not required.
> Note that in this case, you don't even need to implement the `ProtocolHandler` trait. **The only reason it exists is to provide an interface between protocols and the `Router`.**

手写版就是把 `Router::builder(..).accept(..).spawn()` 换成 `.alpns(vec![ALPN.to_vec()])` + 自己的 accept 循环（`iroh/iroh/examples/echo-no-router.rs:69-90`）。**注意官方范例自己的注释**：

```rust
// In a more serious environment, we recommend avoiding `tokio::spawn` and use either a
// `TaskTracker` or `JoinSet` instead to make sure you're not accidentally leaking tasks.
```

——而那正是 Router 已经替你写好的东西。

**判据：默认用 Router。** 手写只在三种情况（详细论证见下方「Router vs 手写 accept 循环」）：

1. 需要 **`Incoming` 的所有权**（转交给自己的连接池/延迟队列）——`incoming_filter` 只给 `&Incoming`，**Router 结构上做不到**。
2. 需要 0-RTT 的完整 `Incoming` 控制（`examples/0rtt.rs:145-148`）。
3. 要接管 task 调度策略。

`transfer.rs:591` 就是手写派（`builder.alpns(vec![TRANSFER_ALPN.to_vec()]).bind()`）——因为它要精细控制每条连接的 span、stats 采集与生命周期。

> ⚠️ **别混用**：`Endpoint::builder().alpns(vec![X])` 之后再 `Router::builder(ep).accept(Y, h).spawn()`，X 会被**静默丢弃**（`set_alpns` 是覆盖式的）。ALPN 只该在一个地方声明。

---

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

# 附：最小协议的下限 —— dumbpipe 的 13 行 lib.rs
```rust
// dumbpipe/src/lib.rs —— 全文
/// The ALPN for dumbpipe.
///
/// It is basically just passing data through 1:1, except that the connecting
/// side will send a fixed size handshake to make sure the stream is created.
pub const ALPN: &[u8] = b"DUMBPIPEV0";

/// The handshake to send when connecting.
///
/// The side that calls open_bi() first must send this handshake, the side that
/// calls accept_bi() must consume it.
pub const HANDSHAKE: [u8; 5] = *b"hello";

pub use iroh_tickets::endpoint::EndpointTicket;
```

**握手存在的唯一理由写在注释里**：QUIC 的 `open_bi()` 在真正写入字节前不会在对端触发 `accept_bi()`，所以连接方必须先写点什么。**这是个纯粹的 QUIC 语义补丁，不是身份/版本协商。**

894 行的 main.rs 几乎全是 stdio/TCP/Unix socket 转发与 clap。**整个「协议」= 一个 ALPN 字符串 + 一次 open_bi/accept_bi。**

> libp2p 里「定义一个协议」要写 NetworkBehaviour / ConnectionHandler / UpgradeInfo；iroh 里最小协议 = **一个 ALPN 字节串 + 一个 impl ProtocolHandler**（dumbpipe 连 ProtocolHandler 都没用，直接裸 accept 循环）。

**依赖面**：`[dependencies]` 实际有 **11 项**：clap、hex、iroh 1.0.0、iroh-tickets 1.0.0、noq 1.0.0、tokio(full)、tokio-util、tracing、tracing-subscriber、data-encoding、n0-error。准确说法是「**网络**依赖面极窄」—— 网络相关只有 iroh + iroh-tickets，其余是 CLI/日志/编码。

# 上线前 checklist

## 协议设计

- [ ] ALPN 带**版本号**、带**项目独有前缀**（撞 ALPN 是静默覆盖，查不出来）
- [ ] 流的边界画在**顺序域**边界上（问自己：这些消息之间真的需要全序吗？）
- [ ] **开流方先写**——请求-响应天然满足；管道/订阅型要发 header 帧或握手常量
- [ ] 长生命周期的并发流数 **< 100**（否则第 101 条静默 `Pending`），或已让**对端**调大限额
- [ ] 算过 `并发流数 × stream_receive_window`（默认 1.25MB/流）的内存上界

## framing

- [ ] 长度前缀**校验上界**（`max_message_size` / `POSTCARD_MAX_SIZE` / 硬编码限长，三选一，不能没有）
- [ ] `read_to_end(N)` 的 `N` 给了
- [ ] EOF（`UnexpectedEof`）当**正常结束**处理，不打 error
- [ ] 半个 frame 的读取**没有**放进 `tokio::select!`（不是 cancel-safe）

## 生命周期与关闭

- [ ] 形状 A：`accept()` 里 `connection.closed().await` 再 return（否则响应可能截断）
- [ ] 形状 B：Connection 的 clone 确实被 actor 持有了
- [ ] **最后读到应用数据的那一侧**负责 `conn.close(code, reason)`
- [ ] `close()` 之后有 `endpoint.close().await`（**带超时**，见 `transfer.rs:787-794`）
- [ ] 定义了 `GRACEFUL_CLOSE: VarInt`，用 `matches!(ApplicationClosed(f) if f.error_code == ..)` 区分正常/异常（否则日志全是 error）
- [ ] 长连接：`conn.closed()` 被并发监视（`race` / `select!`）
- [ ] 长连接协议实现了 `ProtocolHandler::shutdown()`
- [ ] 需要「对端确实收完了」时用**应用层 ACK**（反向流 `finish` + `read_to_end(0)`），不是 `stopped()`

## 健壮性

- [ ] handler 里**没有 `unwrap`/`expect`/索引越界**——一次 panic = 整个 Router 自杀 + 共享 Endpoint 被关，而且 `Router::shutdown()` 还返回 `Ok`（详见下方「一个 handler panic 会掀掉整个 Router」）
- [ ] 业务错误有自己的错误枚举，不是全塞 `AcceptError::User`
- [ ] `stopped()` / 任何等对端的 await **带超时**（gossip 用 5s，`net/util.rs:257`）
- [ ] 需要拒绝连接时选对了层（`incoming_filter` / `EndpointHooks::after_handshake` / `on_accepting`，三层对比见 [02-connecting.md](02-connecting.md) 的 Endpoint Hooks 一节）

## 可观测

- [ ] 用 `send.id().index()` 做 tracing 字段（`transfer.rs:690`）
- [ ] `RUST_LOG=iroh=debug` 下每条连接有 `router.accept` span（`me`/`remote`/`alpn`）
- [ ] 知道 ALPN 不匹配时日志是 `"Ignoring connection: invalid handshake"` 而**不是** `"unsupported ALPN protocol"`，且 span 的 `alpn` 字段是空的（原因见下方「调试提示」）

---

# 附：官方页的 General Guidance（原文，三条都对）

> * **Re-use connections**: ...Opening a QUIC stream is *really* cheap, as it doesn't need extra round-trips for the stream to get established, which is not the case for connections (unless in special circumstances when you're using the QUIC 0-RTT feature).
> * **Beware: QUIC streams are lazy**: Make sure that when you call `connection.open_bi()`, you *always send first* before you receive data. ...The other side that accepts the stream will know about it at the same time that it gets the first bits of data.
> * **Closing QUIC connections can be hard**: ...As a general rule of thumb: **The side to last read data should be the side to close a connection.** Also try to always wait for `Endpoint::close` before dropping your endpoint.

n0 的关闭连接长文：<https://www.iroh.computer/blog/closing-a-quic-connection>（官方页多处引用；本次未独立核实其内容）。
