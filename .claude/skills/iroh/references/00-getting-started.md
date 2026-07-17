# Getting Started：从零到跑通两端

对应官方 [What is iroh?](https://docs.iroh.computer/what-is-iroh) / [Quickstart](https://docs.iroh.computer/quickstart) / [Connect two endpoints](https://docs.iroh.computer/connect-two-endpoints)。

> **本文所有代码都逐字取自 `iroh/iroh/examples/`，并已在本机实测编译 + 运行通过**（iroh 1.0.2，rustc 1.97.0，macOS/darwin 25.5.0，2026-07-17）。实测结论标注为「✅ 实测」。

---

## iroh 是什么

**一句话**：iroh 把「连接的标识」从 `IP:端口` 换成 **Ed25519 公钥** —— 你说「连这个公钥」，iroh 负责找到并维持一条到它的最快通路（优先直连打洞，打不通退回 relay 中继），你拿到的是一条标准 QUIC 连接。

拆成三条技术实质：

| 维度 | 事实 | 依据 |
|---|---|---|
| **寻址** | `EndpointId` **就是 32 字节 Ed25519 公钥本身**（不是哈希、不是 DHT key）。加密是 TLS，但**没有 client/server 证书链**——每个 endpoint 一把 `SecretKey`，对应 `PublicKey` 即身份。因此 EndpointId 是建连的**必需参数**，不是可选元数据 | `iroh/iroh/src/lib.rs:81-94` |
| **连通性** | 先经 home relay 建连，随即尝试直连打洞（hole punching）；成功后 **relay 退出链路**；打不通则**全程 relay 兜底**。这个切换对上层 QUIC 流**透明** | `iroh/iroh/src/lib.rs:64-78`, `:97-117` |
| **你拿到什么** | 标准 QUIC 的 `Connection` + 流：认证加密、并发流、无队头阻塞。底层是 **noq**（n0 自家的 quinn 分支） | `iroh/README.md:40-43` |

官方自述（原文）：

> "iroh is a library to establish direct connectivity between peers. It exposes an interface to [QUIC] connections and streams to the user, while implementing direct connectivity using [hole punching] complemented by relay servers under the hood." —— `iroh/iroh/src/lib.rs:3-5`

> "Iroh gives you an API for dialing by public key. You say "connect to that phone", iroh will find & maintain the fastest connection for you, regardless of where it is." —— `iroh/README.md:31-32`（README 的 "What is iroh?" 段）

> "a modular networking stack written in Rust... a lightweight native library meant to be embedded directly into your application" —— <https://docs.iroh.computer/what-is-iroh>

### 它**不**解决什么（先知道边界，少走弯路）

| 你可能以为它有 | 事实 |
|---|---|
| DHT / 路由表 / 迭代查找 | ❌ **没有**。解析 = 一次 HTTP GET 或 DNS TXT 查询 → [`02-connecting.md`](02-connecting.md) |
| 局域网 mDNS 发现 | ❌ 核心 crate 里**不含**，要外挂 crate（不是开 feature）→ [`02-connecting.md`](02-connecting.md) |
| 类型化 Request/Response + 超时 + 重试 | ❌ 只给**裸双向流**，framing/序列化/超时全自理 → [`03-protocols.md`](03-protocols.md) |
| 「短码 → 地址」rendezvous | ❌ 生态里不存在，寻址原语只有 pubkey→addr → [`01-concepts.md`](01-concepts.md) |

> 从 libp2p 迁过来的，**先读 SKILL.md 的「与 libp2p 的心智差异」10 条**再动手，否则会成片踩空。

---

## 最短路径：4 步跑通

### 步骤 1 —— 建项目

```bash
cargo new iroh-hello && cd iroh-hello
```

### 步骤 2 —— Cargo.toml（最小依赖，✅ 实测编译通过）

```toml
[package]
name = "iroh-hello"
version = "0.1.0"
edition = "2024"
rust-version = "1.91"          # iroh 1.0.2 的 MSRV，抬不动

[dependencies]
iroh = "1.0.2"                                                        # 默认 features 即可
n0-error = "1.0.0"                                                    # 默认 features 即可
tokio = { version = "1", features = ["rt-multi-thread", "macros", "io-util"] }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }   # 只为 echo.rs 的 fmt::init()
```

**每条依赖为什么在这**：

| 依赖 | 作用 | 依据 |
|---|---|---|
| `iroh` **默认 features** | `default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]`。**`tls-ring` 是关键**——三个 preset 全被 `#[cfg(with_crypto_provider)]` 门控，关掉它 `presets::N0` 直接消失 | `iroh/iroh/Cargo.toml:148`；`presets.rs:57,111,173` |
| `n0-error` **默认 features** | examples 用的 `n0_error::Result` / `AnyError` / `StdResultExt::anyerr` **都不需要任何 feature**（`default = []`）。iroh 自己 dev-deps 里开的 `features = ["anyhow"]` **只为 anyhow 互操作**，这里用不上 | `n0-error/Cargo.toml:25-26`；`src/lib.rs:90`；`src/ext.rs:71` |
| `tokio` `rt-multi-thread` + `macros` | `#[tokio::main]` 需要 | — |
| `tokio` `io-util` | `echo.rs` 里的 `tokio::io::copy` 需要 | `examples/echo.rs:99` |
| `tracing-subscriber` `env-filter` | 对齐 iroh 自己 dev-deps 的写法 | `iroh/iroh/Cargo.toml:102` |

> ⚠️ **别写 `default-features = false`**。✅ 实测后果：`error[E0425]: cannot find value 'N0' in module 'presets'`。详见下方[坑 2](#坑-2presets-选错3-种死法)。

### 步骤 3 —— src/main.rs

把 `iroh/iroh/examples/echo.rs` **逐字**拷进去（见下方[完整代码](#最小可运行两端echors逐字)）。

### 步骤 4 —— 跑

```bash
cargo run
```

✅ **实测输出**（首次编译约 37s）：

```
accepted connection from 523dea6102590c0ff27efc848833ff84ec2263629c0b233a437f5cc1ebb63bc0
Copied over 13 byte(s)
```

> 注意：echo.rs 是**单进程内建两个 Endpoint**（一个 accept、一个 connect），所以不用交换地址就能跑通。**真要两台机器**，见[两个进程/两台机器](#两个进程两台机器listenrs--connectrs)。

---

## 最小可运行两端：`echo.rs`（逐字）

**来源**：`/Volumes/yexiyue/iroh-study/iroh/iroh/examples/echo.rs`（iroh 1.0.2）。这是 **Router 版最小例**——推荐的默认形状。

```rust
//! Very basic example to showcase how to use iroh's APIs.
//!
//! This example implements a simple protocol that echos any data sent to it in the first stream.
//!
//! ## Usage
//!
//!     cargo run --example echo

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
const ALPN: &[u8] = b"iroh-example/echo/0";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let router = start_accept_side().await?;

    // wait for the endpoint to be online
    router.endpoint().online().await;

    connect_side(router.endpoint().addr()).await?;

    // This makes sure the endpoint in the router is closed properly and connections close gracefully
    router.shutdown().await.anyerr()?;

    Ok(())
}

async fn connect_side(addr: EndpointAddr) -> Result<()> {
    let endpoint = Endpoint::bind(presets::N0).await?;

    // Open a connection to the accepting endpoint
    let conn = endpoint.connect(addr, ALPN).await?;

    // Open a bidirectional QUIC stream
    let (mut send, mut recv) = conn.open_bi().await.anyerr()?;

    // Send some data to be echoed
    send.write_all(b"Hello, world!").await.anyerr()?;

    // Signal the end of data for this particular stream
    send.finish().anyerr()?;

    // Receive the echo, but limit reading up to maximum 1000 bytes
    let response = recv.read_to_end(1000).await.anyerr()?;
    assert_eq!(&response, b"Hello, world!");

    // Explicitly close the whole connection.
    conn.close(0u32.into(), b"bye!");

    // The above call only queues a close message to be sent (see how it's not async!).
    // We need to actually call this to make sure this message is sent out.
    endpoint.close().await;
    // If we don't call this, but continue using the endpoint, we then the queued
    // close call will eventually be picked up and sent.
    // But always try to wait for endpoint.close().await to go through before dropping
    // the endpoint to ensure any queued messages are sent through and connections are
    // closed gracefully.
    Ok(())
}

async fn start_accept_side() -> Result<Router> {
    let endpoint = Endpoint::bind(presets::N0).await?;

    // Build our protocol handler and add our protocol, identified by its ALPN, and spawn the endpoint.
    let router = Router::builder(endpoint).accept(ALPN, Echo).spawn();

    Ok(router)
}

#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    /// The `accept` method is called for each incoming connection for our ALPN.
    ///
    /// The returned future runs on a newly spawned tokio task, so it can run as long as
    /// the connection lasts.
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // We can get the remote's endpoint id from the connection.
        let endpoint_id = connection.remote_id();
        println!("accepted connection from {endpoint_id}");

        // Our protocol is a simple request-response protocol, so we expect the
        // connecting peer to open a single bi-directional stream.
        let (mut send, mut recv) = connection.accept_bi().await?;

        // Echo any bytes received back directly.
        // This will keep copying until the sender signals the end of data on the stream.
        let bytes_sent = tokio::io::copy(&mut recv, &mut send).await?;
        println!("Copied over {bytes_sent} byte(s)");

        // By calling `finish` on the send stream we signal that we will not send anything
        // further, which makes the receive stream on the other end terminate.
        send.finish()?;

        // Wait until the remote closes the connection, which it does once it
        // received the response.
        connection.closed().await;

        Ok(())
    }
}
```

### 每一步在做什么

| 代码 | 实际发生了什么 | 深入 |
|---|---|---|
| `Endpoint::bind(presets::N0)` | 绑 UDP socket + 起后台 actor + 装好 n0 的三件套（pkarr 发布 / pkarr 解析 / DNS 查找）+ 默认 relay。**返回即 socket 已绑好**，失败直接 `Err` | [`02-connecting.md`](02-connecting.md) |
| `Router::builder(ep).accept(ALPN, Echo)` | 只是往 `BTreeMap` 里 insert，**没起任何东西** | [`03-protocols.md`](03-protocols.md) |
| `.spawn()` | **这里才** `set_alpns` + 起 accept 循环。`#[must_use]` | `protocol.rs:500-511` |
| `router.endpoint().online().await` | 等**至少一个 relay 完成握手**（仅仅选出 relay URL 不算）。**无超时** | `endpoint.rs:1308-1319` |
| `router.endpoint().addr()` | `EndpointAddr` = `id` + `BTreeSet<TransportAddr>`（relay URL + 各 IP） | `iroh-base/src/endpoint_addr.rs:41-47` |
| `endpoint.connect(addr, ALPN)` | QUIC 握手，ALPN 在**握手时协商一次**，连接终身绑定它 | [`03-protocols.md`](03-protocols.md) |
| `conn.open_bi()` → `write_all` → `finish` | 开双向流。**必须先写数据**，否则对端 `accept_bi()` 永不返回 | [坑 4](#坑-4open_bi-了对端却不醒) |
| `conn.close(...)` | **不是 async**！只把 close 消息**排进队** | [坑 3](#坑-3忘记-close) |
| `endpoint.close().await` / `router.shutdown().await` | **真正把队列冲出去**，优雅关连接 | [坑 3](#坑-3忘记-close) |

---

## 对照组：`echo-no-router.rs`（手写 accept loop，逐字）

**来源**：`/Volumes/yexiyue/iroh-study/iroh/iroh/examples/echo-no-router.rs`。同样的协议，**不用 Router** —— 用来看清 Router 到底替你做了什么。

```rust
//! Very basic example showing how to implement a basic echo protocol,
//! without using the `Router` API. (For the router version, check out the echo.rs example.)
//!
//! The echo protocol echos any data sent to it in the first stream.
//!
//! ## Running the Example
//!
//!     cargo run --example echo-no-router

use iroh::{Endpoint, EndpointAddr, endpoint::presets};
use n0_error::{AnyError as Error, Result, StdResultExt};

/// Each protocol is identified by its ALPN string.
///
/// The ALPN, or application-layer protocol negotiation, is exchanged in the connection handshake,
/// and the connection is aborted unless both endpoints pass the same bytestring.
const ALPN: &[u8] = b"iroh-example/echo/0";

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = start_accept_side().await?;

    // wait for the endpoint to be online
    endpoint.online().await;

    connect_side(endpoint.addr()).await?;

    // This makes sure the endpoint is closed properly and connections close gracefully
    // and will indirectly close the tasks spawned by `start_accept_side`.
    endpoint.close().await;

    Ok(())
}

async fn connect_side(addr: EndpointAddr) -> Result<()> {
    let endpoint = Endpoint::bind(presets::N0).await?;

    // Open a connection to the accepting endpoint
    let conn = endpoint.connect(addr, ALPN).await?;

    // Open a bidirectional QUIC stream
    let (mut send, mut recv) = conn.open_bi().await.anyerr()?;

    // Send some data to be echoed
    send.write_all(b"Hello, world!").await.anyerr()?;

    // Signal the end of data for this particular stream
    send.finish().anyerr()?;

    // Receive the echo, but limit reading up to maximum 1000 bytes
    let response = recv.read_to_end(1000).await.anyerr()?;
    assert_eq!(&response, b"Hello, world!");

    // Explicitly close the whole connection.
    conn.close(0u32.into(), b"bye!");

    // The above call only queues a close message to be sent (see how it's not async!).
    // We need to actually call this to make sure this message is sent out.
    endpoint.close().await;
    // If we don't call this, but continue using the endpoint, then the queued
    // close call will eventually be picked up and sent.
    // But always try to wait for endpoint.close().await to go through before dropping
    // the endpoint to ensure any queued messages are sent through and connections are
    // closed gracefully.

    Ok(())
}

async fn start_accept_side() -> Result<Endpoint> {
    let endpoint = Endpoint::builder(presets::N0)
        // The accept side needs to opt-in to the protocols it accepts,
        // as any connection attempts that can't be found with a matching ALPN
        // will be rejected.
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;

    // spawn a task so that `start_accept_side` returns immediately and we can continue in main().
    tokio::spawn({
        let endpoint = endpoint.clone();
        async move {
            // This task won't leak, because we call `endpoint.close()` in `main()`,
            // which causes `endpoint.accept().await` to return `None`.
            // In a more serious environment, we recommend avoiding `tokio::spawn` and use either a `TaskTracker` or
            // `JoinSet` instead to make sure you're not accidentally leaking tasks.
            while let Some(incoming) = endpoint.accept().await {
                // spawn a task for each incoming connection, so we can serve multiple connections asynchronously
                tokio::spawn(async move {
                    let connection = incoming.await.anyerr()?;

                    // We can get the remote's endpoint id from the connection.
                    let endpoint_id = connection.remote_id();
                    println!("accepted connection from {endpoint_id}");

                    // Our protocol is a simple request-response protocol, so we expect the
                    // connecting peer to open a single bi-directional stream.
                    let (mut send, mut recv) = connection.accept_bi().await.anyerr()?;

                    // Echo any bytes received back directly.
                    // This will keep copying until the sender signals the end of data on the stream.
                    let bytes_sent = tokio::io::copy(&mut recv, &mut send).await.anyerr()?;
                    println!("Copied over {bytes_sent} byte(s)");

                    // By calling `finish` on the send stream we signal that we will not send anything
                    // further, which makes the receive stream on the other end terminate.
                    send.finish().anyerr()?;

                    // Wait until the remote closes the connection, which it does once it
                    // received the response.
                    connection.closed().await;

                    Ok::<_, Error>(())
                });
            }

            Ok::<_, Error>(())
        }
    });

    Ok(endpoint)
}
```

### 两版逐行对照 —— Router 帮你做的 4 件事

| | `echo.rs`（Router） | `echo-no-router.rs`（手写） |
|---|---|---|
| **声明 ALPN** | `.accept(ALPN, Echo)` 一处 | `.alpns(vec![ALPN.to_vec()])` 在 builder，**和 handler 分离在两处**（容易写漏/写错） |
| **accept 循环** | `.spawn()` 内建 | 自己 `while let Some(incoming) = endpoint.accept().await` |
| **每连接派发** | 自动 spawn 到新 task | 自己 `tokio::spawn` |
| **关闭编排** | `router.shutdown().await` 会**并发调用并等待所有 handler 的 shutdown** | `endpoint.close().await` 间接让 `accept()` 返回 `None` 退出循环 |

> **`echo-no-router.rs:84-85` 的官方自我提醒**（原文）：*"In a more serious environment, we recommend avoiding `tokio::spawn` and use either a `TaskTracker` or `JoinSet` instead to make sure you're not accidentally leaking tasks."* —— 手写版的任务生命周期是**你的责任**。
>
> **选型建议**：默认用 Router。只有在需要「在 `ProtocolHandler` trait 之外做连接级决策」时才手写——但那通常应该用 `on_accepting` 或 `incoming_filter`，见 [`03-protocols.md`](03-protocols.md)。

---

## 两个进程 / 两台机器：`listen.rs` + `connect.rs`

`echo.rs` 是单进程双 Endpoint，**不构成真正的跨机验证**。真要看两端连上，用这一对（都在 `iroh/iroh/examples/`，需要额外依赖 `clap` 和 `tracing`）：

```bash
# 终端 A（在 iroh 仓库根目录）
cargo run --example listen
# 它会打印出终端 B 该跑的完整命令

# 终端 B —— 直接抄 A 打印的那行
cargo run --example connect -- --endpoint-id <ID> --addrs "<IP:PORT> ..." --relay-url <URL>
```

✅ **实测**（2026-07-17，两端均在本机，走 n0 默认 relay `https://usw1-1.relay.n0.iroh.link./`）：

```
# listen 端输出
received: 54060acc6b56a86a0fa97b9597084737725484addaf84b905b408d4b56acf49c is saying 'hello!'

# connect 端输出
received: hi! you connected to b7c35330c19414a83a21fc20e153151104c6ea1cd5d999d80369481c18a65d9d. bye bye
```

**这对例子暴露的关键事实**：`connect.rs:73-78` 手工拼 `EndpointAddr`：

```rust
let addrs = args
    .addrs
    .into_iter()
    .map(TransportAddr::Ip)
    .chain(std::iter::once(TransportAddr::Relay(args.relay_url)));
let addr = EndpointAddr::from_parts(args.endpoint_id, addrs);
```

也就是说 —— **地址怎么从 A 传到 B，是你的问题，iroh 不管**。这里靠人肉复制命令行。生产里的答案是 **ticket**（`iroh-tickets 1.0.0` 的 `EndpointTicket::new(addr)`，`iroh-tickets/src/endpoint.rs:82`，带 `Display` + `FromStr`），官方 [Connect two endpoints](https://docs.iroh.computer/connect-two-endpoints) 教程用的就是它 → [`01-concepts.md`](01-concepts.md)。

---

## 新手最容易卡住的 8 个地方

### 坑 1：照抄 README 的 quickstart，编译不过

**iroh 仓库 README 的 "Getting Started" 代码是过期的**，对 1.0.2 编不过。三处硬伤：

| README 写的 | 1.0.2 的事实 | 依据 |
|---|---|---|
| `Endpoint::bind().await?` | `bind` **必须传 preset**：`pub async fn bind(preset: impl Preset)` | `README.md:62`（connect 侧）与 `:87`（accept 侧）；`endpoint.rs:955` |
| `.spawn()` + `.await?` | `spawn()` **不是 async**，直接返回 `Router` | `README.md:91-92`；`protocol.rs:501` |
| `.accept(ALPN.to_vec(), Arc::new(Echo))` | `accept` 收 `impl Into<Box<dyn DynProtocolHandler>>`，**直接传 `Echo`**，不用 `Arc` | `README.md:90`；`protocol.rs:484-488` |

> ✅ **正确做法：以 `iroh/iroh/examples/` 为准，不要以 README 为准。** README 自己在 `:113` 也说「The full example code with more comments can be found at [`echo.rs`]」。
>
> 官方 [Quickstart](https://docs.iroh.computer/quickstart) 页面**不展示任何代码**，只让你 `git clone https://github.com/n0-computer/iroh-ping` 然后 `cargo run --example quickstart receiver`。（其中 `IROH_SERVICES_API_SECRET` 那步是 **optional**，只为看 metrics dashboard，不跑核心流程也行。）

### 坑 2：presets 选错（3 种死法）

| 写法 | 症状 | 依据 |
|---|---|---|
| `presets::Empty` | **编译通过，运行必炸**。✅ 实测报错原文：`Missing or incompatible rustls crypto provider configured` | `presets.rs:26-43`（doc 明写 *"using this in Endpoint::bind(presets::Empty) will always fail"*）；`endpoint.rs:228-230` |
| `iroh = { default-features = false }` | ✅ 实测：`error[E0425]: cannot find value 'N0' in module 'presets'`。连带关掉 `tls-ring` → `with_crypto_provider` 为假 → **三个 preset 整个消失** | `Cargo.toml:148,158`；`presets.rs:57,111,173` |
| `presets::Minimal` + `online()` | **永久挂死，无日志无报错**。Minimal **不配 relay**，而 `online()` 的 doc 明写 *"If no relays are configured, this will pend forever."* | `endpoint.rs:1314`；`presets.rs:45-79` |

**怎么选**：

- **默认用 `presets::N0`** —— 但要知道它**隐含对外网络行为**：把你的地址发布到 n0 的 `dns.iroh.link`。
- **`presets::Minimal`** 适合「孤岛 / 局域网直连 / 自建全套基础设施」。✅ 实测：`Minimal` 两端 + `endpoint.addr()` 直连，**不需要 relay、不需要 WAN、不要调 `online()`**，本机可通。
- `presets::Staging` **不存在**（Staging 只是 `RelayMode` 的枚举变体）。

→ 逐行拆解见 [`02-connecting.md`](02-connecting.md)

### 坑 3：忘记 close

`conn.close()` **不是 async** —— 它**只把 close 消息排进队**。真正把队列冲出去的是 `endpoint.close().await`。

echo.rs 自己用 11 行注释强调这件事（`echo.rs:57-67`，原文）：

> "The above call only queues a close message to be sent (see how it's not async!). We need to actually call this to make sure this message is sent out. ... **always try to wait for `endpoint.close().await` to go through before dropping the endpoint** to ensure any queued messages are sent through and connections are closed gracefully."

**不调的后果**：Drop 里只 `abort()` 硬砍 → **对端把「已经成功传完」的连接判成超时失败**。这是 libp2p 用户最容易踩的一条（drop 掉 Swarm 就完事，iroh 不行）。

→ `close()` vs Drop 的 4 点差异见 [`02-connecting.md`](02-connecting.md)；给 `close()` 套 timeout 会两头落空，见 [`index-gotchas.md` B3](index-gotchas.md)

### 坑 4：`open_bi()` 了，对端却不醒

**流是懒创建的**。`open_bi()` 在本地立刻返回，但**在你写第一个字节之前，网络上什么都没发生**，对端的 `accept_bi()` 就永不返回。

rustdoc 里的正式警告（`lib.rs:152-156`，原文）：

> "To keep streams cheap, they are lazily created on the network: only once a sender starts sending data on the stream will the receiver become aware of a stream. This means only calling `Connection::open_bi` is not sufficient for the corresponding call to `Connection::accept_bi` to return. The sender **must** send data on the stream before the receiver's `Connection::accept_bi` call will return."

✅ **实测复现**（connect 端 `open_bi()` 后故意不写，4s 后才 `write_all`）：

```
[connect 端] open_bi() 已返回，故意不写数据，等 4s
[accept 端] accept_bi 3s 内未返回 —— 对端没醒     ← 挂住了
[connect 端] 现在 write_all
[accept 端] 写数据后 accept_bi 返回了              ← 立刻醒
```

> **诊断口诀**：「连上了，但对端 handler 好像没被调用」→ **99% 是你 `open_bi()` 后没写数据**。

→ [`03-protocols.md`](03-protocols.md)

### 坑 5：`online()` 没有超时，离线时永久挂死

`online()` 的 doc 三条硬事实（`endpoint.rs:1308-1319`）：

1. 「online」= **至少一个 relay 完成握手**。*"Merely selecting a relay URL is not sufficient."*
2. *"If no relays are configured, this will pend forever."*
3. *"This has no timeout, so if that is needed, you need to wrap it in a timeout."*

✅ **实测可用的推荐写法**（`iroh::NET_REPORT_TIMEOUT` 是**裸 `u64`**，值为 **5**，不是 `Duration`）：

```rust
let ep = Endpoint::bind(presets::N0).await?;
let t = std::time::Duration::from_secs(iroh::NET_REPORT_TIMEOUT);   // = 5s
match tokio::time::timeout(t, ep.online()).await {
    Ok(()) => { /* 已连上 relay */ }
    Err(_) => { /* 离线/无 WAN —— endpoint 仍可用于局域网直连 */ }
}
```

依据：`lib.rs:292`（`pub use net_report::{NetReportConfig, TIMEOUT as NET_REPORT_TIMEOUT}`）+ `net_report/defaults.rs:14`（`pub const TIMEOUT: u64 = 5`）；`endpoint.rs:1316-1319` 推荐「用接近 `NET_REPORT_TIMEOUT` 秒的超时」。

> **写离线优先的 app**（局域网 / 无 WAN）：doc 明确建议 *"defer any calls to `online` as long as possible, or avoid calling `online` entirely"*（`endpoint.rs:1325-1330`）。

### 坑 6：`Router::spawn()` 的返回值不能丢

```rust
Router::builder(endpoint).accept(ALPN, Echo).spawn();   // ← 返回值被丢弃 = 立刻 abort
```

`#[must_use]` 的原文就是答案（`protocol.rs:500`）：

> `#[must_use = "Router aborts when dropped, use Router::shutdown to shut the router down cleanly"]`

**症状**：服务端「起来了」但没人能连上，且**无任何报错**。

### 坑 7：`EndpointAddr` 没有 `Display`

✅ 实测：`println!("{}", ep.addr())` → `error[E0277]`。

`EndpointAddr` 只 derive 了 `Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash`（`iroh-base/src/endpoint_addr.rs:41`）—— **没有 Display**。

| 类型 | `{}` | `{:?}` |
|---|---|---|
| `EndpointId` / `PublicKey` | ✅ | ✅ |
| `TransportAddr` | ✅（`endpoint_addr.rs:81`） | ✅ |
| **`EndpointAddr`** | ❌ | ✅ |
| `EndpointTicket` | ✅（`iroh-tickets/src/endpoint.rs:28-29`，derive_more，编码成可粘贴字符串） | ✅ |

> **要把地址给人看/传出去，用 `EndpointTicket` 而不是 `{:?}` 一个 `EndpointAddr`。**

### 坑 8：`Echo` 忘了 `#[derive(Debug)]`

`ProtocolHandler` 的 supertrait 是 `Send + Sync + std::fmt::Debug + 'static`（`protocol.rs:228`）——**漏了 `Debug` 直接编译不过**，报错指向 trait bound 而不是你的结构体，容易看懵。

echo.rs 的 `#[derive(Debug, Clone)]`（`echo.rs:80`）不是装饰，是硬要求（`Clone` 不是）。

---

## 跑通之后往哪走

| 你接下来想做 | 去读 |
|---|---|
| 搞懂 preset / bind / close 的全部细节 | [`02-connecting.md`](02-connecting.md) |
| 写自己的协议（framing、背压、并发流、关闭编排） | [`03-protocols.md`](03-protocols.md) |
| 把地址编码成邀请链接 / 分享码 | [`01-concepts.md`](01-concepts.md) |
| 局域网发现 / 摆脱 n0 的 DNS | [`02-connecting.md`](02-connecting.md) |
| 自建 relay | [`07-configuration.md`](07-configuration.md) |
| 传文件（先读这条：多半要 bao-tree 而不是 iroh-blobs） | [`03c-blobs.md`](03c-blobs.md) |
| 撞到怪症状 | [`index-gotchas.md`](index-gotchas.md) |
| 从 libp2p 迁过来 | SKILL.md 的「与 libp2p 的心智差异」10 条 |

### 官方例子清单（`iroh/iroh/examples/`，17 个）

按「新手该看的顺序」：

| 例子 | 看点 | `required-features` |
|---|---|---|
| **`echo.rs`** | Router 版最小例 —— **从这里开始** | 无 |
| **`echo-no-router.rs`** | 手写 accept loop 对照 | 无 |
| `listen.rs` / `connect.rs` | **真·两进程**，手工拼 `EndpointAddr` | 无 |
| `listen-unreliable.rs` / `connect-unreliable.rs` | QUIC datagram（不可靠传输） | 无 |
| `search.rs` | 稍完整的自定义协议 | 无 |
| `transfer.rs` | 最大的例子（44 KB），文件传输 | 无 |
| `0rtt.rs` | 0-RTT 建连 | 无 |
| `incoming-filter.rs` / `screening-connection.rs` / `auth-hook.rs` | 三层入站门禁 | 无 |
| `monitor-connections.rs` / `remote-info.rs` | 连接状态观测 | 无 |
| `custom-transport.rs` | 自定义传输 | `test-utils`, `unstable-custom-transports` |
| `pq-only-key-exchange.rs` / `prefer-pq-key-exchange.rs` | 后量子密钥交换 | **`tls-aws-lc-rs`** |

依据：`iroh/iroh/Cargo.toml:200-251` 共 **13 个 `[[example]]` 段**，逐条列了 `required-features`；其中只有 3 个非空（`custom-transport` / `pq-only-key-exchange` / `prefer-pq-key-exchange`）。

> **17 个文件 vs 13 个声明**：`auth-hook.rs` / `monitor-connections.rs` / `remote-info.rs` / `screening-connection.rs` 这 4 个**没有 `[[example]]` 段**，走 cargo 自动发现，因此同样零额外 feature。
>
> ⚠️ 两个 pq 例子需要 `tls-aws-lc-rs`；`custom-transport.rs` 需要 `unstable-custom-transports`（**不受 semver 保护**）。**其余 14 个零额外 feature**，直接 `cargo run --example <name>` 即可。
