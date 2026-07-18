# 可插拔扩展点：async trait 的「人体工学」与「可存储」怎么兼得

> 这篇讲一个在内核里复用了三处的范式：让用户写线性的 `async fn`，内部却能把它擦成 `Box<dyn>` 存进表里。ProtocolHandler、RpcService、AddressLookup 都是它的实例。这也是 iroh 扩展点的同款手法。

## 矛盾：好写的 trait 存不下，存得下的 trait 难写

我们希望扩展点的作者写起来是线性的 async：

```rust
async fn accept(&self, stream: P2pStream) -> Result<(), AcceptError> {
    let req = read_frame(&mut stream).await?;   // 想读就读
    let resp = decide(req).await;               // 想 await 就 await
    write_frame(&mut stream, &resp).await?;      // 想写就写
    Ok(())
}
```

在 Rust 里，trait 方法里写 `async fn` 等价于返回 `impl Future`（RPITIT，return-position impl trait in trait）。它好写，但有个硬伤：

> **RPITIT 不是 object-safe。** 带 `impl Future` 返回的 trait 不能 `Box<dyn Trait>`。

而 Router 恰恰需要把不同协议的 handler 存进一张 `BTreeMap<ProtocolId, Box<dyn ...>>`——**它必须能被擦成同一种 boxed 类型**。好写和可存储，直接冲突。

## 解法：并排放一个「Dyn 孪生 trait」

范式就是给人体工学 trait 配一个**手动返回 `Pin<Box<dyn Future>>` 的孪生 trait**，再用一条 blanket impl 把前者自动桥接到后者。以 ProtocolHandler 为例（[`crates/net/src/router.rs`](../../../crates/net/src/router.rs)）：

```rust
// ① 人体工学 trait：用户实现这个，写 async fn
pub trait ProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    fn accept(&self, stream: P2pStream) -> impl Future<Output = Result<(), AcceptError>> + Send;
    fn shutdown(&self) -> impl Future<Output = ()> + Send { async {} }
}

// ② Dyn 孪生 trait：object-safe，返回手动 box 的 future
pub(crate) trait DynProtocolHandler: Send + Sync + std::fmt::Debug + 'static {
    fn accept(&self, stream: P2pStream) -> BoxedFuture<'_, Result<(), AcceptError>>;
    fn shutdown(&self) -> BoxedFuture<'_, ()>;
}

// ③ blanket impl：任何 ProtocolHandler 自动是 DynProtocolHandler
impl<T: ProtocolHandler> DynProtocolHandler for T {
    fn accept(&self, stream: P2pStream) -> BoxedFuture<'_, Result<(), AcceptError>> {
        Box::pin(ProtocolHandler::accept(self, stream))   // 把 impl Future 装进 Box
    }
    fn shutdown(&self) -> BoxedFuture<'_, ()> {
        Box::pin(ProtocolHandler::shutdown(self))
    }
}
```

三步各司其职：

```mermaid
graph LR
    U["用户类型 T<br/>impl ProtocolHandler<br/>（写 async fn accept）"]
    B["blanket impl<br/>impl&lt;T: ProtocolHandler&gt;<br/>DynProtocolHandler for T"]
    D["Box&lt;dyn DynProtocolHandler&gt;<br/>（Router 表里存的东西）"]
    U -->|"Box::new(handler) 自动 unsize"| B
    B -->|"Box::pin 擦成 boxed future"| D
    style U fill:#51cf66,color:#fff
    style D fill:#4dabf7,color:#fff
```

- **人体工学 trait**：用户唯一要碰的，写普通 async；
- **Dyn 孪生 trait**：object-safe，可以 `Box<dyn>`，只在内核内部出现（`pub(crate)`）；
- **blanket impl**：`impl<T: ProtocolHandler> DynProtocolHandler for T`，让「实现了好写的那个」自动获得「存得下的那个」，用户零感知。

注册处 `Box::new(handler)` 时，Rust 的 unsize coercion 会自动把 `T` 收进 `Box<dyn DynProtocolHandler>`——用户从头到尾只见过 `ProtocolHandler`。这和 iroh `protocol.rs` 里 `impl<P: ProtocolHandler> DynProtocolHandler for P` 是一字不差的同款手法（iroh 额外提供了 `From<T> for Box<dyn DynProtocolHandler>`，因为它的 `accept` 签名收 `impl Into<Box<...>>`；我们的签名直接收 `impl ProtocolHandler`，靠 unsize coercion 就够了，省掉那道 `From`）。

## Send 约束按 target 分叉

`BoxedFuture` 是个按平台分叉的类型别名：

```rust
#[cfg(not(wasm_browser))]
type BoxedFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(wasm_browser)]
type BoxedFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + 'a>>;
```

native 是多线程运行时，future 必须 `Send`；wasm 是单线程，加 `Send` 反而会把碰 JS 类型的 handler 挡在门外。这个差异被这一个 `cfg` 别名吸收掉，扩展点作者和内核其它代码都不用感知——这正是内核「平台差异全部下沉、业务层零 cfg」约束的一个缩影。

## 同一个范式，三处形态不同

范式的**内核**是「人体工学 trait + 一条把它擦成可存储 boxed 形态的路径」。但「擦成什么」取决于人体工学 trait 本身是否 object-safe，于是三个扩展点长得不完全一样：

**ProtocolHandler**（[02](02-router-protocol-handler.md)）——RPITIT，不 object-safe，走完整的「Dyn 孪生 + blanket impl」，如上。

**RpcService**（[05](05-typed-rpc-on-streams.md)）——也是 RPITIT async fn，但它不另立 Dyn 孪生，而是用一个**适配器结构体** `RpcHandler` 实现 `ProtocolHandler`，复用后者已经建好的 Dyn 机制（[`crates/net/src/rpc.rs`](../../../crates/net/src/rpc.rs)）：

```rust
pub trait RpcService<Req, Resp>: Send + Sync + 'static {
    fn handle(&self, from: NodeId, req: Req)
        -> impl Future<Output = Result<Resp, AcceptError>> + Send;
}

// RpcHandler<Req, Resp, S> 把一个 RpcService 适配成 ProtocolHandler
impl<Req, Resp, S: RpcService<Req, Resp>> ProtocolHandler for RpcHandler<Req, Resp, S> { .. }
```

也就是说，`RpcService` 的可存储性是「借道」`ProtocolHandler` 拿到的——范式可以叠罗汉。

**AddressLookup**（[06](06-address-lookup-dht.md)）——它的方法 `resolve` 返回的是 `Option<BoxStream<...>>` 而不是 `impl Future`，本身就已经 object-safe，**不需要 Dyn 孪生**，直接 `Box<dyn AddressLookup>` 存起来。它对范式的贡献在另一头——用 blanket impl 让「不需要 Endpoint 的 lookup」自动就是 Builder（[`crates/net/src/lookup.rs`](../../../crates/net/src/lookup.rs)）：

```rust
pub trait AddressLookupBuilder: Send + Sync + 'static {
    fn into_address_lookup(self: Box<Self>, endpoint: &Endpoint)
        -> Result<Box<dyn AddressLookup>, LookupError>;
}
// 不依赖 Endpoint 的 lookup 自动就是 Builder
impl<T: AddressLookup> AddressLookupBuilder for T {
    fn into_address_lookup(self: Box<Self>, _endpoint: &Endpoint)
        -> Result<Box<dyn AddressLookup>, LookupError> { Ok(self) }
}
```

那些**需要** Endpoint 才能构造的 lookup（比如基于 DHT 的在线宣告解析），走一个闭包适配器 `LookupBuilderFn` 延迟构造——这套「bind 后回填」的鸡生蛋问题留到 [06](06-address-lookup-dht.md) 讲。

## 收束：认这个形状

三个扩展点摆在一起，共同的骨架一目了然：

| 扩展点 | 人体工学 trait | 怎么变可存储 |
|---|---|---|
| `ProtocolHandler` | `async fn accept` (RPITIT) | Dyn 孪生 + blanket impl |
| `RpcService` | `async fn handle` (RPITIT) | 适配器 `RpcHandler` → 复用 ProtocolHandler 的 Dyn |
| `AddressLookup` | `resolve` 返回 `BoxStream`（已 object-safe） | 直接 `Box<dyn>`；另配 Builder blanket + 闭包适配器 |

以后要往内核再加一个可插拔点，照这个形状套就行：**先写给用户看的 async trait，再想清楚它 object-safe 不 object-safe，据此决定要不要一个 Dyn 孪生**。用户永远只碰好写的那一面，内核在幕后把它擦成能进表的形态。

下一篇把这个范式最有代表性的用例展开——控制面的 typed RPC，看它怎么让旧栈的 `pending_id`/`PendingMap` 整套机制凭空消失：[05 — 裸流上的 typed RPC](05-typed-rpc-on-streams.md)。
