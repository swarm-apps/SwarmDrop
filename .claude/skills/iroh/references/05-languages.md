# FFI 与绑定：uniffi / napi / C

iroh-ffi 1.1.0 · uniffi 0.31.1 · napi-rs 3 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/iroh-ffi/`

**一句话**：uniffi 0.31 桥 Swift / Kotlin / Python 三语言；**JS 走完全独立的 napi-rs 3 通道，不是 uniffi**；**完全不支持 wasm**；**不暴露 blobs、不暴露 discovery**。

## 绑定矩阵

| 目标语言 | 库 | 机制 | 成熟度 |
|---|---|---|---|
| Swift / Kotlin / Python | **iroh-ffi** | uniffi 0.31.1 | production |
| Node.js | **iroh-ffi/iroh-js** = npm `@number0/iroh` | napi-rs 3 | production |
| C / C++ / Go / 嵌入式 | **iroh-c-ffi** | safer-ffi，**全同步阻塞** | production（打折） |
| **React Native** | ❌ **不存在** | — | — |
| 浏览器 / wasm | iroh 本体 + wasm-bindgen | 见 [06-wasm-browser.md](06-wasm-browser.md) | production |
| ⚠️ 顶层 `iroh-js/` 目录 | ❌ **墓碑**（HTTP-RPC 客户端骨架，2023-12-07） | — | **abandoned** |

> ⚠️ **24 个仓平铺时 `iroh-js` 这个顶层目录名会骗人**：活的 JS 绑定在 `iroh-ffi/iroh-js/`（npm `@number0/iroh`）；顶层 `iroh-js/` 是 2.5 年前的墓碑（npm `@n0computer/iroh`，靠调已下线的 api.iroh.network）。详见 [index-ecosystem-map.md](index-ecosystem-map.md)。

---

# 第一部分：iroh-ffi

## 基础事实

```toml
# iroh-ffi/Cargo.toml
[lib]
name = "iroh_ffi"
crate-type = ["staticlib", "cdylib"]

[[bin]]
name = "uniffi-bindgen"
path = "uniffi-bindgen.rs"

[dependencies]
uniffi = { version = "0.31.1", features = ["cli", "tokio"] }

[build-dependencies]
uniffi = { version = "0.31.1", features = ["build"] }
```

```rust
// src/lib.rs:21 —— 纯 proc-macro 模式，无任何 .udl 文件
uniffi::setup_scaffolding!();
```

Rust edition 2024，rust-version 1.91，Cargo.lock 解析到 uniffi **0.31.2**。

**两个 crate-type 都要**：`staticlib` 供 iOS xcframework 链接，`cdylib` 供 JNA/Android/Python 加载，且 uniffi-bindgen 必须读 debug cdylib 才能提取元数据。crate 名 `iroh-ffi` 但 lib name 是 `iroh_ffi`。

- **成熟度**：**production**（HEAD `fa6c6d6` **2026-07-16**，PR 号已到 #274；9 个 GitHub workflow；已发布到 PyPI/`iroh` + Maven Central/`computer.iroh:iroh`+`iroh-android` + SwiftPM + npm/`@number0/iroh`）
  - ⚠️ **版本号不是承载证据**：`Cargo.toml:5` 是 `publish = false` —— 1.1.0 **不是 crates.io release**，无 semver-registry 保证（它走 npm/maven/cocoapods）。production 判定靠 HEAD 日期与它作为官方绑定的角色
- **入口**：`iroh-ffi/src/endpoint.rs`（**1086 行 = src/ 全部 2897 行的 37.5%** —— Endpoint / Connection / Stream 全挤在这一个文件）；再读 `DEVELOPERS.md` 的 "Translating the iroh API into bindings" 一节（n0 自己的桥接规约）

## 它暴露什么

**只有 iroh 1.0 的裸网络层**：Endpoint、Connection、SendStream/RecvStream、datagram、Incoming/Accepting/Connecting 手写 accept 循环、EndpointId/EndpointAddr/EndpointTicket、SecretKey/Signature、RelayMap/RelayMode 自定义 relay、multipath 快照、iroh-services 客户端、三个 watcher 回调。

## 两个硬伤

### 1. 不暴露 blobs / docs / gossip —— 不是「还没做」

`DEVELOPERS.md` 的 `Scope` 一节：*"`docs`, `gossip`, and `blobs` are intentionally **not** exposed (they are not 1.0 APIs)."*
README.md 同样口径：*"higher-level protocols not yet at 1.0 (`iroh-blobs`, `iroh-docs`, `iroh-gossip`) are out of scope"*

实证两条：
- `grep -rn 'blobs|iroh_blobs|gossip|docs::' src/ iroh-js/src/` **零命中**
- `grep '^name = "iroh-blobs"' Cargo.lock` **零命中** —— **连传递依赖都没有**

`[dependencies]` 只有 iroh / iroh-base / iroh-metrics / iroh-relay / iroh-services / iroh-tickets。

**这掐死了「用 iroh-ffi 从宿主语言直接用 iroh-blobs」这条路**：想从 iroh 生态白嫖 chunk/续传/校验，只能是 iroh-blobs（bao Merkle + 原生 range 请求），而它恰好是官方明说不进 FFI 的那部分。**iroh-blobs 只能作为 Rust crate 用，绝无 FFI 捷径。**

### 2. 没有任何 discovery / address_lookup setter —— 自定义发现机制从 FFI 完全够不着

`#[uniffi::export] impl EndpointBuilder`（`src/endpoint.rs:52-119`）导出的方法穷举：`new` / `apply_n0` / `apply_minimal` / `apply_n0_disable_relay` / `secret_key` / `alpns` / `relay_mode` / `bind_addr` / `bind`。

**没有 address_lookup、没有 discovery。** `grep -rn 'address_lookup|AddressLookup|discovery|mdns|pkarr' src/` 只命中 **3 处文档注释**（relay.rs:14、endpoint.rs:62、endpoint.rs:156），**零处 API**。

逃生舱也无用：`#[uniffi::export(with_foreign)] pub trait Preset { fn apply(&self, builder: Arc<EndpointBuilder>); }`（:130-133）—— 外语言实现的 Preset 拿到的 `Arc<EndpointBuilder>` 上**只有上面那 9 个方法**。

**对照组**：`iroh-c-ffi/src/endpoint.rs:55` `pub enum DiscoveryConfig { None, DNS, Mdns, All }` —— **C 绑定反而做到了**，可作为「该怎么暴露」的形状参考。

> ⚠️ **易误读的地方**：仓根有个 `support-matrix.yaml`（**不在 README 里**；该文件头部自述用途是喂文档站落地页 `scripts/docs/gen_landing.py`）。它 :33 写着 `- name: Custom relays (full RelayMap)` / `support: { swift: true, kotlin: true, python: true, js: true }` —— **全 true**，很容易误读成「配置能力齐全」。
>
> **更隐蔽的是**：该 matrix 总共只列 **5 个 feature**（Endpoints / Protocols / Connections & multipath / Custom relays / iroh-services），**根本没有 discovery 那一行** —— 不是标成 false，是压根不提。


> **❌ 完全不支持 wasm**，且「同一份 uniffi FFI 喂移动端和 Web」机制上走不通
> → [06-wasm-browser.md](06-wasm-browser.md)。


## 何时用 iroh-ffi

1. 想看 n0 官方怎么用 uniffi 0.31 处理 async / 长生命周期对象 / 回调 / Android JNI / Apple 部署目标 → **直接抄这份代码**
2. 真要在 Swift/Kotlin/Python 里开一条**裸 iroh QUIC 连接**，且业务逻辑写在宿主语言里
3. 校准自己的 uniffi 写法是否跑偏

## 何时不该当依赖

四条，每条都能 grep 证实：
1. 它是独立 cdylib（`crate-type = ["staticlib","cdylib"]`）+ 自己的 `uniffi::setup_scaffolding!()` —— **一个 megazord 装不下第二份 scaffolding**（两份各自注册符号）
2. 只吐 Kotlin/Swift/Python，**不吐 RN Turbo Module**（uniffi_bindgen 0.31.1 内置 backend 只有 kotlin/python/ruby/swift）
3. 不暴露 blobs
4. 不暴露 discovery

**通常的正确做法**：**iroh 作为普通 crate 进你自己的 core，FFI 边界只走你自己的业务语义。**

---

# 第二部分：uniffi 机制要点（抄作业清单）

## async 怎么过 FFI 边界 —— 是真异步

Rust 侧返回 **Handle + rust_future_new**，外语言侧变成原生异步（Kotlin `suspend fun` / Swift `async` / Python `await`）。不是回调地狱，也不是阻塞。

生成的形状（`uniffi_macros-0.31.1/src/export/scaffolding.rs:279-298`）：

```rust
#[unsafe(no_mangle)] pub extern "C" fn #ffi_ident(...) -> ::uniffi::Handle {
    let uniffi_lifted_args = (#lift_closure)();
    ::uniffi::rust_future_new::<_, #return_ty, _>(async move { ... }, crate::UniFfiTag)
}
```

**约束**：`uniffi_core-0.31.1/src/ffi/rustfuture/mod.rs:48-49` 注明 *"The `Send` bound is required because the Foreign code may call the `rust_future_*` methods from different threads."* —— 所有跨 FFI 的 future **必须 Send**，任何 !Send 的东西（比如某些 SQLite guard 跨 await）都过不去。

这是 uniffi 对 iroh-c-ffi(block_on) 和裸 JNI 的核心优势。

### 每个方法手写注解

```rust
// src/endpoint.rs:293
#[uniffi::constructor(async_runtime = "tokio")]
pub async fn bind(options: EndpointOptions) -> Result<Self, IrohError> {

// src/endpoint.rs:384
#[uniffi::method(async_runtime = "tokio")]
pub async fn connect(&self, addr: &EndpointAddr, alpn: &[u8]) -> Result<Connection, IrohError> {
```

**注解覆盖率的规律（精确、无一遗漏）：**
- `endpoint.rs` 46 个 async fn 对 34 个注解（差额 12 = 2 个 trait 方法定义 + 2 个 ProtocolWrapper impl + 8 个 `#[tokio::test]`）
- `accept.rs` 12 个 async fn 对 12 个注解，**全覆盖**
- **`#[uniffi::export]` 的 Object 方法必须加 `async_runtime`**
- **`with_foreign` 的外语言实现回调 trait 不加**（`watch.rs` 4 个 async fn 只有 1 个注解，那 1 个是 `WatchHandle::stop`）——它们靠 `#[async_trait::async_trait]`

## ⚠️ `async_runtime = "tokio"` 的真身是 async_compat::Compat

三级链条：
1. `uniffi_macros-0.31.1/src/export/scaffolding.rs:273-274` —— `future_expr = quote! { ::uniffi::deps::async_compat::Compat::new(#future_expr) }`，仅当 `matches!(ar, Some(AsyncRuntime::Tokio(_)))`
2. `uniffi_core-0.31.1/Cargo.toml:40` —— `tokio = ["dep:async-compat"]`；`lib.rs:77` `#[cfg(feature = "tokio")] pub use async_compat;`
3. `async-compat-0.2.5/src/lib.rs:456-469` —— `fn get_runtime_handle() -> Handle { tokio::runtime::Handle::try_current().unwrap_or_else(|_| TOKIO1.handle().clone()) }`，而 `static TOKIO1: Lazy<Runtime> = Lazy::new(|| { thread::Builder::new().name("async-compat/tokio-1".into()).spawn(...); tokio::runtime::Builder::new_current_thread().enable_all().build()... });` —— **new_current_thread**，由一个名叫 `async-compat/tokio-1` 的**专用单线程**驱动

**⚠️ 精确的行为**：`impl<T: Future> Future for Compat<T>`（`async-compat-0.2.5/src/lib.rs:325-332`）是：

```rust
fn poll(...) { let _guard = get_runtime_handle().enter(); self.get_pin_mut().poll(cx) }
```

—— 它**进入 TOKIO1 的上下文、然后在当前线程 poll**，并不把 future 搬到 TOKIO1 的线程上。

**准确表述**：
- 导出 async fn 的**函数体**由 uniffi 的 rust_future 机制在**外语言调用线程**上轮询（带着 TOKIO1 的上下文）
- 真正被关进 TOKIO1 那**一根线程**的是 `tokio::spawn` 出去的任务 + tokio 的 IO/timer driver

**对 iroh 的推论**：**iroh 的 QUIC actor 正是 `tokio::spawn` 出去的，会全落到那一根线程上。**

> ⚠️ **别信这个绕法**：「在 core 构造时显式建一个 multi-thread Runtime 并持有 Handle 就能绕开 TOKIO1」—— **机制不成立**。`Handle::try_current()` 读的是**当前线程的线程本地上下文**；把 Runtime/Handle 存成字段不会让轮询线程进入该 runtime 的上下文，`try_current()` 依旧 Err、依旧回落 TOKIO1。
>
> **要真正绕开**，必须让轮询线程进入上下文（每次调用里 `handle.enter()`）或把活儿显式 `handle.spawn(...)` 出去。**「持有 Handle」不够。**

## ⚠️ 最核心的阻抗失配：uniffi 只能 `&self`，永远拿不到 `self`

**所有 Rust 里 move-once 语义的类型都被迫退化成 `Mutex<Option<T>>` + `.take()` 手动模拟。move 检查从编译期保证降级为运行期字符串错误。**

中招的四个类型：`Incoming` / `Accepting` / `Connecting` / `EndpointBuilder`。

```rust
// src/accept.rs:99-121
#[derive(uniffi::Object)]
pub struct Incoming(Mutex<Option<iroh::endpoint::Incoming>>);

#[uniffi::export]
impl Incoming {
    #[uniffi::method(async_runtime = "tokio")]
    pub async fn accept(&self) -> Result<Accepting, IrohError> {
        let inner = self.0.lock().await.take()
            .ok_or_else(|| anyhow::anyhow!("Incoming has already been consumed"))?;
        ...
    }
}
```

**官方把这个降级当成必须跨每种语言测试的一等契约**——「already consumed」断言出现在 **5 处**（Rust + 全部四种绑定语言）：

| 位置 | 测试名 |
|------|--------|
| `src/endpoint.rs:921` | `test_builder_bind_consumes`（断言 :928） |
| `kotlin/lib/src/test/kotlin/computer/iroh/EndpointTest.kt:33` | `builderBindConsumes`（:41） |
| `python/endpoint_test.py:34` | `test_builder_bind_consumes`（:41） |
| `IrohLib/Tests/IrohLibTests/IrohLibTests.swift:139` | `testBuilderBindConsumes`（:148） |
| `iroh-js/test/endpoint.mjs:47` | `/already consumed/`（:52） |

连独立 napi 通道的 `iroh-js/src/endpoint.rs` 都把 Incoming/Accepting/Connecting 各自复刻了一遍「already consumed」。

**通则**：凡是 `fn foo(self)` 消耗自身的 API（builder、oneshot handle、Incoming 类），过 uniffi **必然**要走这套 `Mutex<Option<T>>` 包装，且**要自己补运行期测试——编译器不再替你把关**。

> **libp2p 对照**：libp2p 的 Swarm/Transport 大量用 move + `&mut self`（`poll_next` 取 `&mut self`），过不了 uniffi 的 `&self` 门槛。**「Clone + 内部 channel」形态的客户端类型反而天然适配 uniffi**，而 builder / 一次性 handle 一定会中招。

### ⚠️ 反面教材：EndpointBuilder 消费后 panic 而非 Result

```rust
// src/endpoint.rs:34-41
fn map<F>(&self, f: F) {
    let mut guard = self.inner.lock().unwrap();
    let builder = guard.take().expect("EndpointBuilder consumed");   // ← .expect()
    *guard = Some(f(builder));
}
```

走 `map()` 的导出方法有 6 个：`apply_n0`(:63) / `apply_minimal`(:68) / `apply_n0_disable_relay`(:73) / `secret_key`(:83) / `alpns`(:89) / `relay_mode`(:95)。

而另一条路 `take_inner()`（:43-49）返回 `Err(anyhow!("EndpointBuilder already consumed"))`，被 `bind_addr`(:101) 和 `bind`(:115) 使用。

**于是**：bind() 后再 bind() → 干净的 IrohError（有测试覆盖）；bind() 后再 alpns() → **panic**。panic 会被 uniffi 的 catch_unwind 兜住（`uniffi_core-0.31.1/src/ffi/rustcalls.rs` 有 3 处），所以不是 abort，但外语言侧拿到的是一个不带 IrohErrorKind 的 panic 型异常。**仓库测试只覆盖了 bind() 那条。**

**更值钱的间接教训**：这是「**`uniffi::Object` 包 `Option<T>` 做一次性消费语义**」这个模式的通病。同样的形状在 iroh-ffi 里还有 **3 处**（Incoming/Accepting/Connecting，`accept.rs:100/197/234` —— 那 3 个是**正确**的 Result 写法）。

**自己写一次性对象时：要么统一走 Result，要么统一 panic，别一半一半。**

## ⚠️ `tokio::sync::Mutex` 会把同步 getter 传染成 async

```rust
// src/accept.rs:164-192 —— 三个纯读字段的 getter 全被迫 async
#[uniffi::method(async_runtime = "tokio")]
pub async fn local_addr(&self) -> Result<IncomingLocalAddr, IrohError> {
    let guard = self.0.lock().await;                  // ← tokio Mutex
    let inner = guard.as_ref().ok_or_else(|| anyhow::anyhow!("Incoming has already been consumed"))?;
    Ok(inner.local_addr().into())                     // 纯读取
}

// 对比：EndpointBuilder 用 std::sync::Mutex，于是 getter 保持同步
// src/endpoint.rs:24
inner: std::sync::Mutex<Option<iroh::endpoint::Builder>>,
// src/endpoint.rs:63
pub fn apply_n0(&self) { self.map(|b| presets::N0.apply(b)); }    // ← 同步
```

**选 `std::sync::Mutex` 还是 `tokio::sync::Mutex` 会直接决定外语言 API 是不是 suspend。** 官方在 EndpointBuilder 上用 std Mutex 保住了同步 API，在 Incoming 上用 tokio Mutex 导致三个 getter 无谓地 async。

**这是个可以主动优化的点：不跨 await 持锁的地方用 `std::sync::Mutex`。**

## 对象生命周期：Arc 引用计数 + Cleaner 兜底，不是 GC

生成的 Kotlin（`iroh_ffi.kt:3579-3582`）里每个 Object 都 `open class Xxx : Disposable, AutoCloseable, XxxInterface`，持 `wasDestroyed: AtomicBoolean` + `callCounter: AtomicLong(1)`（:3608-3609），destroy() 用 CAS + decrementAndGet 到 0 才 `cleanable.clean()`（:3611-3620）。构造时 `this.cleanable = UniffiLib.CLEANER.register(this, UniffiCleanAction(handle))`（:3589）。

`iroh_ffi.kt:3480-3553` 的长注释给了完整契约，关键三句：
- *"Astute readers may observe that this all sounds very similar to the way that Rust's `Arc<T>` works"*
- 回收顺序：**1. 手动 destroy() → 2. 不可达且 Cleaner 线程排到 → 3. 进程退出**
- 风险：*"if the Rust object's drop method is slow, and/or there are many objects to cleanup, and it's on a low end Android device, then the cleaner thread may be starved, and the app will leak memory"*

**含义**：**不是 GC**，Cleaner 只是安全网。若你的主对象持 DB 连接 + 网络句柄、drop 可能不快，正好落在注释警告的「slow drop + 低端 Android」区间 —— **应显式释放而非等 Cleaner**。

**且内存释放 ≠ 网络关停：必须显式 `close()`。**

```rust
// src/endpoint.rs:392 —— 两条分支
#[uniffi::method(async_runtime = "tokio")]
pub async fn close(&self) -> Result<(), IrohError> {
    if let Some(router) = &self.router { router.shutdown().await?; }
    else { self.inner.close().await; }
    Ok(())
}
```

**双层引用计数**：`SendStream`/`RecvStream` 是 `#[derive(Clone, uniffi::Object)] pub struct SendStream(Arc<Mutex<endpoint::SendStream>>)`——uniffi Object 外面一层 Arc（uniffi 管）+ 里面一层 `Arc<Mutex>`（自己管），因为 `BiStream::send()`/`recv()` 要能多次返回同一个流的句柄。

## ⚠️ Kotlin 名字冲突：async `close()` 撞 AutoCloseable

```toml
# iroh-ffi/uniffi.toml
# `Endpoint::close()` collides with `java.lang.AutoCloseable.close()` which
# uniffi-kotlin implements (non-suspend) on every object. Our close() is async,
# so the JVM sees conflicting overloads. Rename only for the Kotlin binding;
# all other languages keep `close()`.
[bindings.kotlin.rename]
"Endpoint.close" = "shutdown"
```

DEVELOPERS.md 把这条升格为通则：

> Mirror upstream names. Where a name collides with a host-language builtin (e.g. Kotlin `AutoCloseable.close()`), rename **only** for that binding via `uniffi.toml`'s `[bindings.kotlin.rename]`, **not globally**.

**任何 async 的 `close()` 在 Kotlin 侧都会炸。** 测试印证：`EndpointTest.kt` 全篇用 `ep.shutdown()`，Rust 测试用 `ep.close()`。

另注：`public` 也是 Kotlin 关键字，uniffi 会反引号转义（`secret.\`public\`()`），Swift 也一样。

> 若用生成 TS/C++ 而非 Kotlin 的工具链（如 uniffi-bindgen-react-native），AutoCloseable 撞名这个**具体**坑不吃；但「一个 async close() 撞上宿主语言内建方法」这个**类**的坑要记着 —— 加 close()/dispose() 前先查工具链在宿主侧注入了什么。

## 回调：`with_foreign` + `async_trait`

```rust
// src/watch.rs:19-23
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait AddrChangeCallback: Send + Sync + 'static {
    async fn on_change(&self, addr: Arc<EndpointAddr>) -> Result<(), CallbackError>;
}

// src/error.rs:140-161 —— 回调错误必须实现 From<UnexpectedUniFFICallbackError>
#[derive(Debug, thiserror::Error, PartialEq, Eq, uniffi::Error)]
pub enum CallbackError {
    #[error("Callback failed")]
    Error,
}
impl From<uniffi::UnexpectedUniFFICallbackError> for CallbackError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self { CallbackError::Error }
}
```

**回调 trait 的 async 方法不加 `async_runtime = "tokio"`**（那是给 Rust 侧导出方法用的），而是靠 `#[async_trait::async_trait]`。

**同步/异步外语言 trait 可以混用**：`Preset` trait 是 `with_foreign` 但**非** async（`fn apply(&self, builder: Arc<EndpointBuilder>)`）。

DEVELOPERS.md 把它立成规约：*"Methods that emit progress/events take a callback trait (`#[uniffi::export(with_foreign)]` on the uniffi side, `ThreadsafeFunction` on the napi side). See the watcher callbacks in `src/watch.rs`."*

## Stream/Watcher → 回调 trait + WatchHandle

```rust
// src/watch.rs 头部注释点破动机
//! `iroh::Endpoint` exposes a few values that change over time via the
//! `n0_watcher::Watcher` trait (`watch_addr`, `home_relay_status`, etc.). That
//! trait doesn't map naturally to uniffi, so the FFI exposes the same data via
//! callback traits: register a callback and get back a [`WatchHandle`] that
//! aborts the underlying task when dropped (or when [`WatchHandle::stop`] is called).

#[derive(uniffi::Object)]
pub struct WatchHandle { task: Mutex<Option<AbortOnDropHandle<()>>> }

pub(crate) fn spawn_watch_addr(endpoint: iroh::Endpoint, cb: Arc<dyn AddrChangeCallback>) -> WatchHandle {
    let task = n0_future::task::spawn(async move {
        let mut stream = endpoint.watch_addr().stream();
        while let Some(addr) = stream.next().await {
            if let Err(err) = cb.on_change(Arc::new(addr.into())).await {
                tracing::warn!("addr change callback error: {err:?}");
                break;                    // ← 回调返 Err 就退出 watch 循环
            }
        }
    });
    WatchHandle::new(AbortOnDropHandle::new(task))
}
```

**三个可直接落地的设计点**：

1. **回调失败即 break 退出 watch 循环**（:72-75）—— 检查你的事件回调有没有这个语义。**宿主侧一次异常就永久失联，是想要的吗？**
2. **AbortOnDropHandle + WatchHandle** 这个「返回一个句柄，drop 或 stop() 即注销」的模式比全局事件总线更可控 —— 它把「**外语言对象被 GC → Rust 后台任务自动 abort**」这条链路接通了
3. `WatchHandle::stop` 用 **`tokio::sync::Mutex`（async 锁）而非 std Mutex**（`watch.rs:14`）
   > ⚠️ 注意：`spawn_watch_addr`（:64-79）本身**根本没有任何锁**，所以它不是「先脱离锁再回调」的范例 —— 别照这个说法去理解

**`watch_addr` 本身是同步 uniffi 方法（无 async_runtime 注解）却内部 `n0_future::task::spawn`**——依赖调用时有环境 runtime。而 `WatchHandle::stop` 反倒是 async，只因为它要 lock 一个 tokio Mutex。

## 错误处理：Object 而非 Enum

```rust
// src/error.rs:6-47
/// Stable high-level error categories exposed across the FFI boundary.
/// These are intentionally coarser than the upstream Rust error types. They
/// give foreign bindings a stable taxonomy for `errors.Is`-style handling
/// without leaking the internal `iroh` / `n0-error` error hierarchy.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum IrohErrorKind {
    InvalidInput, Bind, Connect, Connection, Alpn, KeyParsing,
    TicketParsing, Relay, Stream, Datagram, Callback, Closed, Timeout, Internal,
}   // ← 14 个值

#[derive(Debug, thiserror::Error, uniffi::Object)]      // ← Object，不是 uniffi::Error！
#[error("{message}")]
#[uniffi::export(Debug)]
pub struct IrohError { kind: IrohErrorKind, message: String, debug_message: String }

// src/error.rs:108-138 —— 上游类型 → 稳定分类的宏映射（把 16 个上游类型压到 14 个 kind）
from_iroh_err! {
    iroh::endpoint::BindError => IrohErrorKind::Bind,
    iroh::endpoint::ConnectError => IrohErrorKind::Connect,
    ...
}
```

注意是 **`uniffi::Object` 而非 `uniffi::Error`** —— 所以外语言侧是个带方法的对象：`message()` / `kind()` / `debug_message()` / `is_kind()`（:73-95），并在 Swift 侧 `error as? IrohError` 捕获（`IrohLibTests.swift:26-47` 有验证）。

这条路子比 `uniffi::Error` enum 更灵活（能带方法、能演进），**代价是外语言拿不到 enum 的穷尽匹配**。`#[non_exhaustive]` + `Copy` 的 kind enum 是 FFI 稳定性的关键设计。

**可抄的两点**：① `from_iroh_err!` 宏 + 「粗粒度 kind 枚举 + `#[non_exhaustive]`」的形状；② `message()` / `debug_message()` 分离（用户可读 vs `{:?}` 全量）。

> **为什么值得抄**：libp2p 与 iroh 的错误类型层级都极深（TransportError / DialError / BindError / ConnectError / ConnectionError / AlpnError…），**全量映射到 uniffi enum 不现实**。FFI 边界要有自己的稳定错误分类，不透传内部层级。

## ⚠️ 数据进出全是 Vec<u8> 拷贝 —— 「文件数据绝不能过 FFI」的量化证据

`iroh-ffi/src/endpoint.rs:811-819`：

```rust
pub async fn read(&self, size_limit: u32) -> Result<Vec<u8>, IrohError> {
    let mut buf = vec![0u8; size_limit as _];   // ← 按上限全量分配！
    let mut r = self.0.lock().await;
    let res = r.read(&mut buf).await?;
    let len = res.unwrap_or(0);
    buf.truncate(len);
    Ok(buf)
}
```

**`read(1_000_000)` 无论实到几字节都先 malloc 1MB。** `read_exact`(:823-828) / `read_to_end`(:832-836) 同样 u32 上限（read_to_end 因此**无法**读超过 4GiB-1 的流）。

SendStream/RecvStream 是 `Arc<Mutex<endpoint::SendStream>>`（:727、:800，**tokio async Mutex**）且 Clone，所以 `BiStream::send()`（:716-718）每次返回一个克隆，**多次调用会在同一把锁上排队**。

DEVELOPERS.md 把这个取舍写成了规约：
- *"`PathBuf` → `String`; `Bytes` / `[u8]` → `Vec<u8>`"*
- *"Anything streaming should read from / write to explicit buffers, or expose a `size` accessor so callers can decide how to handle the data"*
- *"Methods returning a `Stream` (e.g. a `list`) should return a `Vec`. **Add a comment warning that everything is loaded into memory.**"*

**算一笔账**：假设在宿主语言里写 chunk 循环 —— 每个 chunk 至少两次拷贝（Rust `Vec<u8>` → RustBuffer → JS ArrayBuffer / Kotlin ByteArray）+ 一次按上限的全量 malloc + 一次 tokio Mutex 争用，还要在宿主语言里做加密和 range-set 记账。

**结论：数据留在 Rust、FFI 只过业务语义（sendFile / pause / resume / 进度事件）—— 这不是「感觉对」，是有数。**

## 类型映射：为 Kotlin 人体工学做的有损妥协

```rust
// src/endpoint.rs:248-251
/// Flat snapshot of the headline numbers from `noq::ConnectionStats`.
/// Counters are `i64` (not `u64`) so Kotlin sees `Long`, not `ULong`.
#[derive(Debug, uniffi::Record)]
pub struct ConnectionStats { pub udp_tx_datagrams: i64, ... }

// src/endpoint.rs:595-601
/// Close the connection immediately with the given application error code.
/// Signed for Kotlin/Swift ergonomics; negative values are rejected.
pub fn close(&self, error_code: i64, reason: &[u8]) -> Result<(), IrohError> {
    let unsigned = u64::try_from(error_code)
        .map_err(|_| IrohError::invalid_input("error_code must be >= 0"))?;

// src/endpoint.rs:368 —— metrics 饱和转换
MetricValue::Counter(v) => u32::try_from(v).unwrap_or(u32::MAX),
```

即便做了 i64 妥协，生成代码里仍有 unsigned 类型泄漏——`kotlin/lib/build.gradle.kts:52` 必须 `optIn.add("kotlin.ExperimentalUnsignedTypes")`。

# 第三部分：Android

## ⚠️ `android = true` 被刻意注释禁用

```toml
[bindings.kotlin]
package_name = "computer.iroh"
cdylib_name = "iroh_ffi"
# android = true   # enabling this injects androidx.annotation.RequiresApi
#                  # and Build.VERSION_CODES.UPSIDE_DOWN_CAKE into iroh_ffi.kt,
#                  # which can't compile under the pure JVM java-library plugin.
#                  # Re-enable once kotlin/lib is split into a JVM module + a
#                  # com.android.library AAR module. PR #237's JNI context init
#                  # (IrohAndroid + src/android_init.rs) works without it.
```

**这是「uniffi 的 Kotlin 后端假设你要么纯 JVM、要么纯 Android，同一份生成代码两边通吃很别扭」的直接证据。** 官方的取舍：生成一份 JVM-safe 的 .kt，Android 特有的东西（JNI init）自己在 AAR 模块里手写。

## ⚠️ 必须手写裸 JNI 注入 Application context

`iroh-ffi/src/android_init.rs` 全文 39 行，模块注释直说：

> *"iroh's DNS resolver reads `LinkProperties.getDnsServers()` through `ndk_context`, which must be initialized with the process's JavaVM and `Application` context **before any `Endpoint` is constructed**"*

```rust
#[unsafe(no_mangle)]
pub extern "system" fn Java_computer_iroh_IrohAndroid_installAndroidContext<'local>(
    mut env: jni::EnvUnowned<'local>,
    _class: jni::objects::JClass<'local>,
    context: jni::objects::JObject<'local>,
) {
    INIT.call_once(|| {                      // :12-14, :22 —— static INIT: Once 保证幂等
        env.with_env(|env| -> jni::errors::Result<()> {
            let java_vm = env.get_java_vm()?;
            let global_ref = env.new_global_ref(&context)?;
            unsafe {
                ndk_context::initialize_android_context(
                    java_vm.get_raw() as *mut std::ffi::c_void,
                    global_ref.as_obj().as_raw() as *mut std::ffi::c_void,
                );
            }
            // Keep the global ref alive forever; ndk_context holds the raw
            // pointer and expects it to stay valid for the rest of the process.
            std::mem::forget(global_ref);     // :34
            ...
```

```toml
[target.'cfg(target_os = "android")'.dependencies]
ndk-context = "0.1"
jni = "0.22"
```

Kotlin 侧：`object IrohAndroid { init { System.loadLibrary("iroh_ffi") } ; @JvmStatic external fun installAndroidContext(context: Context) }`。

**三重坑：**
1. **uniffi 桥不了这个，必须手写 JNI 符号**
2. **必须 `std::mem::forget(global_ref)` 永久泄漏 global ref**，否则 ndk_context 持有的裸指针悬垂
3. **App 必须在 `Application.onCreate`（RN 场景是 `ReactApplication.onCreate` 或 TurboModule 初始化）手动调一次 `installAndroidContext`，务必早于任何 `Endpoint::bind`**

> **libp2p 对照 —— 这一条是 iroh 独有的新增成本**：libp2p 时代**不需要它**（DHT/bootstrap 是 IP 直连，不依赖系统 DNS）；**iroh 的 address_lookup 走 pkarr/DNS，Android 上没有这段注入，解析会静默失败** —— 表现是「能连自建 relay 但解析不出对端」，**极难 debug**。

## 产物：4 ABI + 两个独立 Maven 制品

```toml
# Makefile.toml:167-181
[tasks.kotlin-android]
command -v cargo-ndk >/dev/null 2>&1 || cargo install --version 3.5.4 cargo-ndk --locked
cargo ndk -o ./kotlin/android/src/main/jniLibs \
  --manifest-path ./Cargo.toml \
  -t armeabi-v7a -t arm64-v8a -t x86 -t x86_64 \
  build --release
```

```
# Makefile.toml:287-288 —— 两条分发路径
#   - kotlin/lib/src/main/resources/<jna-platform>/<lib>  → JVM JAR; JNA
#     discovers libs at <jna-platform>/<libname>.<ext> from classpath.
#   - kotlin/android/src/main/jniLibs/<abi>/libiroh_ffi.so → AAR; AGP packages
#     these into consumer APKs at lib/<abi>/.
```

cargo-ndk 钉死 **3.5.4**，NDK **r23**（CI 用 `arqu/setup-ndk@main`）。`kotlin/android/build.gradle.kts`：compileSdk 34 / **minSdk 24** / JVM 17。

**minSdk 的坑**（`kotlin/android/build.gradle.kts:18`）：「cargo-ndk 的默认 API level (21) 低于 minSdk 24，所以 .so 能在 minSdk 宿主上加载」——**故意让 NDK API level 低于 minSdk，反过来会炸。**

### ⚠️ JNA 重复类冲突

```kotlin
// kotlin/android/build.gradle.kts
dependencies {
    // Excluding JNA from this transitive: the JVM JAR depends on
    // net.java.dev.jna:jna:5.15.0 (the JAR variant, fine for desktop); the
    // AAR variant — which carries libjnidispatch.so per ABI — is added
    // explicitly below. Without the exclude, AGP errors on duplicate
    // com.sun.jna.* classes from the JAR + AAR.
    api(project(":lib")) { exclude(group = "net.java.dev.jna", module = "jna") }
    api("net.java.dev.jna:jna:5.15.0@aar")
```

**这是 uniffi-kotlin 走 JNA 而非直接 JNI 带来的连锁坑**：Android 上你不只要打包自己的 .so，还要打包 JNA 自己的 `libjnidispatch.so`。

> 走 JSI/Turbo Module 的工具链（uniffi-bindgen-react-native）不吃这套 JNA 依赖 —— **这是与官方 Kotlin 路线的实质分野**。但同类冲突要警惕。

# 第四部分：Apple

## 产物：xcframework 但用扁平 `-library` 形态

```bash
# make_swift.sh
# Apple-blessed shape: each slice ships a flat `lib<name>.a` + `Headers/`
# directory containing the uniffi-generated FFI header, an `Export.h`
# umbrella, and a `module.modulemap` declaring the Swift-visible module name (`Iroh`).
#
# Replaces the historical "checked-in framework skeleton + cp binaries
# into it" pattern, which forced per-Xcode-major hand-fixes of the
# bundle layout (see iroh-ffi#247: Xcode 27 rejected the shallow
# Info.plist layout iOS-style bundles use). With -library there is no
# .framework directory at all — just lib.a + headers — so that whole
# class of "bundle layout doesn't match the platform Apple expects" bug disappears.
```

**5 个 build target**：`aarch64-apple-ios` / `aarch64-apple-ios-sim` / `x86_64-apple-ios` / `aarch64-apple-darwin` / `aarch64-apple-ios-macabi`（Mac Catalyst）；两个 sim arch 用 `lipo -create`(:110) 合成 fat lib；再 `xcodebuild -create-xcframework` 出 **4 个 slice**。有 `cargo make verify-swift-xcframework` 专门守这个形状（`Makefile.toml:243-272`）。

**iroh-ffi#247 是真实事故**：Xcode 27 拒绝了手工维护的 `.framework` bundle 的 shallow Info.plist 布局。**教训：让 xcodebuild 自己从 .a 的 Mach-O header 推断 platform/arch/simulator 并生成 Info.plist，别手写 framework 骨架。**

（`make_swift.sh` 还得 sed 把 uniffi 生成的 `iroh_ffiFFI` 模块名重写成消费者 import 的 `Iroh`。）

## ⚠️ 硬性部署目标下限 iOS 17.5 / macOS 14.5 + 三个必链 framework

```bash
# make_swift.sh
# Apple deployment-target floors. iroh's netdev calls
# `nw_path_is_ultra_constrained` (iOS 17 / macOS 14); rustc's default
# `*-apple-ios` floor and the unset macOS floor produce undefined-symbol
# link errors at xcframework-consumption time on older SDKs.
export IPHONEOS_DEPLOYMENT_TARGET="17.5"
export MACOSX_DEPLOYMENT_TARGET="14.5"
```

```swift
// Package.swift:48-51
platforms: [ .iOS("17.5"), .macOS("14.5"), .macCatalyst("17.5") ],

// Package.swift:66-72
linkerSettings: [
  .linkedFramework("SystemConfiguration"),
  // iroh's netdev uses Network.framework for interface enumeration
  // (the nw_* / nw_path_monitor_* symbols) on Apple platforms.
  .linkedFramework("Network"),
  // iroh's netwatch queries WiFi interfaces via CoreWLAN on macOS.
  .linkedFramework("CoreWLAN", .when(platforms: [.macOS]))
]),
```

**已独立核实**：`iroh-ffi/Cargo.lock` 里 netdev = 0.45.0，而 `netdev-0.45.0/src/os/ios/network.rs:137` 确有 `fn nw_path_is_ultra_constrained(path: nw_path_t) -> bool;`（extern C 声明，:206 处调用）。依赖链：Cargo.lock 中 netwatch 0.19.1 的 dependencies 含 netdev，`iroh/Cargo.toml:40` `netwatch = "0.19.1"`。

**⚠️ 这是硬成本，且和用不用 iroh-ffi 无关** —— 符号来自 **iroh → netwatch → netdev 的传递依赖**，把 iroh 塞进自己的 core 一样吃。**Tauri 桌面端也会碰到 CoreWLAN 这条。**

**失败模式极其隐蔽：链接期才在消费者工程里报 undefined symbol，你自己 build .a 时一切正常。**

（build.rs 里还额外为 apple target 往 pkg-config 里塞 `-framework SystemConfiguration`。）

> ⚠️ **17.5 是 n0 的保守取整**：Apple 注释说 `nw_path_is_ultra_constrained` 是 iOS 17 起可用，n0 设到 17.5。**若你的部署目标低于 17.5，会不会真链接失败取决于该符号的实际可用版本 —— 必须实测**，否则失败模式是「CI 过、真机装上就崩」。

# 第五部分：构建产物与可抄的 CI 实践

## bindgen 元数据来源

两边都先 `cargo build --lib` 出 **host debug** 产物，再 `cargo run --bin uniffi-bindgen generate --library $TARGET_DIR/debug/libiroh_ffi.{so,dylib}` —— **uniffi 是从 host 动态库的符号里读 FFI 表的**。

## ① 可复现构建（值得直接抄）

```sh
# make_swift.sh:30-40
RUSTFLAGS: 4 条 --remap-path-prefix (cargo registry / cargo git / 源码 checkout / rustup sysroot)
CFLAGS:    3 条 -ffile-prefix-map
# 注释（:34-35）: "--remap-path-prefix is Rust-only. Several deps (notably `ring`)
#                  compile bundled C sources via build.rs + the `cc` crate"
```

**没有这一步，你的 .a 里会嵌着构建机的绝对路径 —— 既泄露又不可复现。**

## ② 发布前消费者冒烟（这套思路值得整体抄）

```kotlin
// kotlin/android-smoke/src/androidTest/kotlin/computer/iroh/smoke/IrohSmokeTest.kt
/**
 * On-device smoke for `computer.iroh:iroh:<version>`. If `lib/<abi>/libiroh_ffi.so`
 * isn't merged into this consumer APK by AGP (issue #246), or it loads but a
 * uniffi symbol is missing, this test fails before the assertion.
 *
 * Stays entirely offline — `SecretKey.generate()` is local ed25519, no network.
 */
@RunWith(AndroidJUnit4::class)
class IrohSmokeTest {
    @Test
    fun secretKey_roundtripsThroughJni() {
        // `public` is a Kotlin keyword — uniffi emits it backtick-escaped.
        val secret = SecretKey.generate()
        val pub = secret.`public`()
        ...
```

```yaml
# release.yml:270 —— publish 强依赖冒烟通过
build-and-publish-kotlin:
  needs: [create-release, build-kotlin-cdylib, build-kotlin-android, verify-kotlin-android-consumer]
```

Makefile.toml 有三个这类任务：`verify-swift-xcframework` / `verify-kotlin-android-consumer` / `verify-kotlin-consumer`，且 `Makefile.toml:240` 注释写明动机 —— 抓的是 *"succeeds, artifact is broken, runtime crash on consumer device"*。

**把刚构建的 .so 塞进一个真的 consumer app，在 emulator（API 35 / x86_64 / google_apis）上跑 instrumented test。** 目的是抓 layout 检查抓不到的运行期问题：**AGP 没把 .so 从 JAR merge 进 APK（issue #246）、.so 加载了但 JNI 符号缺失、NDK API level 对 emulator 太高**。

**冒烟测试刻意选纯离线的 `SecretKey.generate()`**（本地 ed25519，不联网），因为目标是验证桥接链路而非网络。

（emulator job 必须跑在 ubuntu-latest 而非 self-hosted runner——「self-hosted linux runner 没开 KVM」，要手写 udev 规则开 `/dev/kvm`。）

**「build 成功但产物是坏的、到用户设备才崩」这个失败类别，只有消费者冒烟能抓。**

## 生成的绑定是检入仓库的

| 类别 | 是否检入 |
|------|---------|
| Kotlin `iroh_ffi.kt`（594KB） | ✅ 检入 |
| Swift `IrohLib.swift`（9975 行） | ✅ 检入 |
| `Iroh.xcframework/` | ❌ gitignore（`xcodebuild -create-xcframework -library` 从头生成） |
| `kotlin/android/src/main/jniLibs/` | ❌ gitignore（CI-staged） |

**分界线是「文本绑定检入、二进制产物不检入」。**

`Package.swift` 还做了个巧妙的双模解析：本地 checkout 若已 build 过（探测 `Iroh.xcframework/macos-arm64/libiroh_ffi.a` 存在）就用本地 xcframework，否则回落到 GitHub Release 的预编译 zip + 钉死的 checksum——**让源码消费者的绑定永远和源码一致**。

---

# 第六部分：两个警告

## ⚠️ 别把 iroh-ffi 的异步回调路径当作久经沙场的参考实现

DEVELOPERS.md 的明文要求：

> ## Testing
> When you add a piece of the API, add a test for it in **every** binding:
> `src/*.rs` (`#[cfg(test)]`), `python/*_test.py`, `iroh-js/test/*.mjs`,
> `kotlin/lib/src/test/kotlin/computer/iroh/*Test.kt`, and `IrohLib/Tests/IrohLibTests/IrohLibTests.swift`.

**但实测：`watch_addr` / `watch_home_relay` / `watch_network_change` / `watch_paths` / `watch_path_events` / `ProtocolCreator` / `ProtocolHandler` 这一整块，在 Rust 和四种绑定语言里全部零测试覆盖**（grep `watch|protocol` 覆盖四种绑定的测试文件 → 0 命中；`src/endpoint.rs` 的 `#[cfg(test)]` 模块正文 868-1100 行 → 0 命中，连 `test_connect_echo_roundtrip` 走的都是 `accept_next()` + Incoming 手动 accept 路径）。

**对比：同步的外语言 trait（`Preset`）在 4 处都测了**——Rust `endpoint.rs:874/883`、Kotlin `EndpointTest.kt:11/19`、Python `endpoint_test.py:12/20`、Swift `IrohLibTests.swift:117/125`。（JS 没有，但 JS 走 napi 通道、根本没有 Preset foreign trait 这个概念，属于不适用而非漏测。）

**这个反差让核心对比更锋利：同步 foreign trait 四语言全测，异步 foreign trait（AddrChangeCallback / ProtocolHandler）一个都没测。自己桥事件回调时，得自己建立信心，不能假设「n0 官方这么写就一定没问题」。**

## 一个由此暴露的 bug 候选

**传给 `ProtocolCreator::create()` 的 `Arc<Endpoint>` 是一个新造的、router 为 None 的包装对象**，与 `Endpoint::bind()` 返回给调用方的那个（持有 router 的）**不是同一个 uniffi 对象**：

```rust
// src/endpoint.rs:277-283 —— Endpoint::new 恒置 router: None
pub fn new(ep: endpoint::Endpoint) -> Self { Endpoint { inner: ep, router: None } }

// src/endpoint.rs:318-334 —— bind 内部
let endpoint_wrapper = Arc::new(Endpoint::new(endpoint.clone()));  // ← router: None
for (alpn, creator) in protocols {
    let handler = creator.create(endpoint_wrapper.clone());        // ← 发给外语言的是它
    router_builder = router_builder.accept(alpn, ProtocolWrapper { handler });
}
...
Ok(Endpoint { inner: endpoint, router })   // ← 返回给调用方的是这个
```

**后果**：在 ProtocolHandler 里对手上那个 Endpoint 调 `close()`，会走 `self.inner.close().await` 分支而**不是** `router.shutdown()`——即关掉 endpoint 但不关 router。

**这是 uniffi Object 身份与 Rust 值语义不一致的典型表现：同一个底层 `iroh::Endpoint` 在外语言侧呈现为两个不同对象。而这条路径无测试覆盖（见上）。**

# 第七部分：JS 是完全独立的第二条通道（napi）

```toml
# iroh-js/Cargo.toml
[package]
name = "number0_iroh"
[lib]
crate-type = ["cdylib"]
[dependencies]
napi = { version = "3", default-features = false, features = ["napi8", "error_anyhow", "async"] }
napi-derive = "3"
```

```json
// iroh-js/package.json —— 11 个 target，全原生，无 wasm、也无 darwin-x64
"napi": {
  "binaryName": "iroh",
  "targets": [
    "aarch64-apple-darwin", "aarch64-linux-android", "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl", "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl",
    "armv7-unknown-linux-gnueabihf", "armv7-unknown-linux-musleabihf", "armv7-linux-androideabi"
  ]
}
```

DEVELOPERS.md：「Python/Swift/Kotlin use [uniffi-rs]; JavaScript uses [napi-rs].」lock 解析到 napi **3.10.5**。`engines.node >= 20.3.0`。

## 回调机制与 uniffi 完全不同

napi 走 `ThreadsafeFunction`（`iroh-js/src/endpoint.rs:450/456/462`）：

```rust
#[napi] pub fn watch_addr(&self, callback: ThreadsafeFunction<EndpointAddr>) -> WatchHandle { ... }
#[napi] pub fn watch_home_relay(&self, callback: ThreadsafeFunction<Vec<String>>) -> WatchHandle { ... }
#[napi] pub fn watch_network_change(&self, callback: ThreadsafeFunction<()>) -> WatchHandle { ... }
```

**同一个 `watch_addr` 在两条通道上是两种写法**：uniffi 是 `Arc<dyn AddrChangeCallback>`，napi 是 `ThreadsafeFunction<EndpointAddr>`。**官方维护两份并行的绑定实现——这是 uniffi 覆盖不到 JS 生态的代价。**

## 不能用于浏览器，也不能用于 RN

它是 **Node 原生 addon**（.node 文件 + dlopen），**浏览器无 dlopen**。targets 里有 android，但那是「**Android 上跑 Node**」，不是 RN 的 JSI。

> ⚠️ **源码树不是一一对应**：`iroh-js/src/` 有 **9** 个文件（endpoint / key / lib / net / path / relay / services / ticket / watch），`src/` 有 **11** 个（多 accept / error / android_init）。Incoming/Accepting/Connecting 被折叠进了 `iroh-js/src/endpoint.rs`（见 `:445` 及 `:492-628` 的「already consumed」系列）。

# 第八部分：iroh-c-ffi

- **成熟度**：**production（打折）**
- **依据**：
  - version **0.101.0** —— **pre-1.0**，与 iroh 1.0.0 依赖版本脱节（「维护中但不承诺稳定」的信号）
  - **edition 2021** —— 比 iroh-ffi 的 edition 2024 落后一代
  - HEAD `277a462` 2026-06-25 `ci: add semver check (#71)` —— PR 号 71，且**刚加了 semver 检查**，说明在正经维护
  - 有 `.github/workflows/{ci.yml,release.yaml}` + `deny.toml`
  - 依赖 safer-ffi 0.1.13（features=["async-fn"]）、iroh 1.0.0、**iroh-mdns-address-lookup 0.4.0**
  - **判定 production 但要打折：版本号与 edition 都表明它是二等公民**
- **入口**：`iroh-c-ffi/src/endpoint.rs`（src/ 共 2419 行）；C 侧用法看同目录 `main.c` / `multi-thread-client.c` / `single-thread-server.c`

## 全同步阻塞

所有 async fn 用 `#[ffi_export(executor=tokio_executor)]` 包一层 block_on，**C 侧调用者线程被阻塞**：

```rust
// src/util.rs:5-10
pub(crate) static TOKIO_EXECUTOR: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().unwrap());
pub fn tokio_executor<F: std::future::Future>(future: F) -> F::Output {
    TOKIO_EXECUTOR.block_on(future)
}
```

生成的 C 头（`irohnet.h:636-644`）注释直说 *"Blocks the current thread."*。生成头文件：`cargo run --features headers --bin generate_headers`。

## 何时用 / 不用

**用**：宿主语言是 C/C++/Go/Zig，或目标是嵌入式；且能接受「一次调用阻塞一个线程」（仓里给了 `multi-thread-server.c` 示范线程池扛法）。

**也可作为「iroh 该怎么暴露 mDNS discovery」的参考 —— 它是唯一做到的官方绑定**（`src/endpoint.rs:55` `pub enum DiscoveryConfig { None, DNS, Mdns, All }`）。

**不用**：C ABI 手动管内存（`rust_buffer_alloc` / `rust_buffer_free` / `rust_free_string`，`util.rs:19-34`），比 uniffi 的 Cleaner 退化一代；且 block_on 模型和事件循环驱动的宿主（Node/RN）天生打架。


---

# 第九部分：ticket 过 FFI 边界的形状

抄 `iroh-ffi/src/ticket.rs:12-52`：`#[derive(Debug, uniffi::Object)]` + `#[uniffi::export(Display)]` + 两个 constructor（`from_addr` / `from_string`）。**ticket 是不可变值类型，用 Object 包裹比 Record 更省心**（避免每个字段都要过 FFI 类型映射）。⚠️ `iroh-ffi/Cargo.toml:5` 是 `publish = false`，它走 npm/maven/cocoapods 而非 crates.io —— **抄形状即可，不必依赖它**

> **官方 FFI 压根不暴露 mDNS** —— 要在 uniffi 桥里暴露局域网发现**必须自己写绑定，无先例可抄**；
> `iroh-c-ffi` 的 `DiscoveryConfig` 是唯一做到的官方绑定，可作形状参考
> → [02-connecting.md](02-connecting.md) 的 mDNS 一节。
