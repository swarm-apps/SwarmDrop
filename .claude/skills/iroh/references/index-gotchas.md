# 所有 critical 坑（症状 → 根因 → 正确做法）

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

**本文按「你先撞到什么」组织。** 每条都给到源码位置，可自行核对。

> **这是正交索引层，不是官方分区。** 你先撞到的是报错、挂死、静默失灵，此时**根本不知道它属于哪个分区** ——
> 所以本文按「症状」而不是按领域组织。每个字母分区下的条目**跨多个官方分区**，这正是它不能被打散的原因。
>
> 按能力查库 → [index-ecosystem-map.md](index-ecosystem-map.md)；官方没讲的地基 → [index-foundations.md](index-foundations.md)。

| 分区 | 覆盖 | 官方 home |
|---|---|---|
| **A** | Endpoint / 启动（crypto_provider、preset、BindError、身份、默认监听） | [02-connecting.md](02-connecting.md)（A5 另牵 [07](07-configuration.md)） |
| **B** | 关闭（close vs Drop vs abort、Clone 语义） | [02-connecting.md](02-connecting.md) |
| **C** | 流（懒创建死锁、100 并发上限、方向反了的限制、内存） | [03a-using-quic.md](03a-using-quic.md) |
| **D** | Router / 协议（must_use、ALPN 覆盖、panic 连坐、关闭编排、AcceptError、0-RTT） | [03b-writing-a-protocol.md](03b-writing-a-protocol.md)（D12–D14 → [02](02-connecting.md) 的 Hooks） |
| **E** | Watcher（initialized 的两个陷阱、抖动、有损采样、缺失 API） | [index-foundations.md](index-foundations.md) + [02-connecting.md](02-connecting.md) |
| **F** | Relay（QAD 端口、`--dev`、scheme、Disabled 语义、双 RelayConfig、stale 文档、版本兼容） | [01](01-concepts.md) 心智 / [07](07-configuration.md) 配置 / [08](08-deployment.md) 安全 |
| **G** | 发现 / 隐私（改名、无 mDNS、user_data 泄漏、BEP44 误解、DNS 属性） | [02-connecting.md](02-connecting.md) + [08-deployment.md](08-deployment.md) |
| **H** | Feature / 跨平台（default-features 连带杀伤、wasm、ring/clang、tls-\*、apple datapath） | [07-configuration.md](07-configuration.md) + [06-wasm-browser.md](06-wasm-browser.md) |
| **I** | iroh-blobs / 传输（ImportMode、push 默认禁用、FsStore 布局、panic #233、自建 runtime） | [03c-blobs.md](03c-blobs.md) |
| **J** | 高层协议 / 地基（gossip 不是 presence、消息不认证作者） | [02-connecting.md](02-connecting.md) 的 Gossip 一节 |
| **K** | Ticket / 编码（wire 判别符、跨版本断代、读源码的行号陷阱） | [01-concepts.md](01-concepts.md) 的 Tickets 一节 |


## A. Endpoint / 启动

### A1. `BindError::InvalidCryptoProvider`，装了 `install_default()` 也没用

**症状**：`bind()` 立刻返回 `BindError::InvalidCryptoProvider`；照 libp2p 习惯加了 `rustls::crypto::ring::default_provider().install_default()` 仍然报同样的错。

**根因**：**iroh 全程用 `builder_with_provider` 显式传参，从不查 rustls 进程默认 provider。** iroh crate 里 rustls config 的**全部**构造入口只有两处（`tls.rs:81` / `:109`），都是 `builder_with_provider`；全 workspace 搜 `install_default|get_default|process_default` 在 iroh crate **零命中**（只命中 relay/dns 服务端二进制）。

**`install_default()` 对 iroh Endpoint 是纯 no-op。这不是 bug，是设计。**

**正确做法**：
- 有 `tls-ring` 或 `tls-aws-lc-rs` feature（default 有）→ 用 `presets::N0` / `presets::Minimal`
- 无 → `Endpoint::builder(presets::Empty).crypto_provider(Arc::new(...))` 手动喂

> **别写反的对比**：libp2p-tls **也不读**进程默认——它把 ring **硬编码在内部**（`libp2p-tls-0.6.2/src/lib.rs:48,75` 是 `rustls::crypto::ring::default_provider()`，一个**构造函数**，不是 `CryptoProvider::get_default()`），随后同样走 `builder_with_provider`。真实差异是「libp2p 硬编码不给选 vs iroh 要求显式注入」。

### A2. 同一个 `InvalidCryptoProvider` 有两个语义

**症状**：明明设了 provider，还是 `InvalidCryptoProvider`。

**根因**：两个触发点错误变体**完全相同**（错误信息刻意写成 "Missing or **incompatible** rustls crypto provider configured"，`socket.rs:863-864`）：

| 位置 | 含义 |
|------|------|
| `endpoint.rs:228-230` | **没设**（`self.crypto_provider.ok_or_else(...)`）|
| `endpoint.rs:232-235` | **设了但不兼容**——`RustlsTokenKey::new` 在 provider 里找不到任何 TLS 1.3 cipher suite（`tls/misc.rs:25-41`：`.filter_map(|suite| suite.tls13()).next()?`）|

**自定义裁剪版 provider 若把 TLS 1.3 suite 剔光，会得到和「忘了设」一模一样的错误。**

**另有第三条相关路径**：provider 缺 `TLS13_AES_128_GCM_SHA256` 时报的是 `BindError::TlsConfigError → TlsConfigError::CryptoProviderNoInitialCipherSuite`（`tls.rs:130-135`）。

### A3. `presets::Empty` 编译通过，运行必炸

**症状**：`Endpoint::bind(presets::Empty)` 编译没问题，跑起来必失败。

**根因**：`Empty::apply` 是**恒等函数**（`presets.rs:36-43`，原样返回 builder，一个字段都不设），`crypto_provider` 留在 `None`。文档写死了：「This doesn't set mandatory builder options, so using this in `Endpoint::bind(presets::Empty)` **will always fail**.」

失败是**瞬时**的（在 `bind()` 最前面，早于任何 socket/TLS 工作），**不会先绑端口、无网络副作用**。

**正确做法**：想要「最小但保证能跑」用 `Minimal`。`Empty` 只在自带 provider 时用（官方唯一范例：`examples/pq-only-key-exchange.rs`）。

> **libp2p 对照**：libp2p 的等价错误在 `SwarmBuilder` 的类型状态机里是**编译错误**（少一步 `with_tcp` 就编译不过）；iroh 把这个约束下放成了运行时 `Option` 检查。`bench/src/iroh.rs:37` 的注释原话就是「// allow building, but fail at runtime」。

### A4. `presets::Staging` 找不到

**症状**：照教程写 `presets::Staging` 编译不过。

**根因**：**它不存在。** `grep -rn "presets::Staging|pub struct Staging"` 全仓零输出。`presets.rs` 全文只有 4 个 preset：`Empty`(37) / `Minimal`(59) / `N0`(113) / `N0DisableRelay`(175)。

**Staging 只是 `RelayMode` 的枚举变体**（`endpoint.rs:1920-1934`）。

**正确做法**：`RelayMode::Staging`，或设环境变量（见 A5）。

### A5. 生产程序静默连到 staging relay

**症状**：线上连的是 staging 基础设施，代码里明明没写。

**根因**：`presets::N0` 走 `default_relay_mode()`（`presets.rs:136`），而它读环境变量：

```rust
// endpoint.rs:1966-1984
pub const ENV_FORCE_STAGING_RELAYS: &str = "IROH_FORCE_STAGING_RELAYS";
pub fn force_staging_infra() -> bool {
    matches!(std::env::var(ENV_FORCE_STAGING_RELAYS), Ok(value) if !value.is_empty())
}
pub fn default_relay_mode() -> RelayMode {
    match force_staging_infra() { true => RelayMode::Staging, false => RelayMode::Default }
}
```

**任意非空值即触发**，且**同一个变量还会同时切换 pkarr relay 端点**（`pkarr.rs` 的 `n0_dns()`）——relay map 和 DNS/pkarr 端点一起改。

CI/容器里误传这个变量很难查。文档明说 staging「might have incompatible changes deployed」。

> ⚠️ **一条 stale 文档**：`presets.rs:103-105` / `pkarr.rs:520-522` 写着「When in tests, or when the `test-utils` feature is enabled...」——**与代码不符**。`force_staging_infra` 全树只有一处定义（`endpoint.rs:1970-1972`），纯读环境变量，**无 `#[cfg(test)]` / `#[cfg(feature="test-utils")]` 分支**。

### A6. 每次重启身份都变

**症状**：EndpointId 每次启动都不一样，对端认不出来。

**根因**：`bind()` 第一步就是 `let secret_key = self.secret_key.unwrap_or_else(SecretKey::generate);`（`endpoint.rs:226`）——**不给就每次随机生成新身份**。

**正确做法**：持久化身份必须显式 `.secret_key(key)`。

### A7. 一次 `BindError::AddressLookup` 附带一条看起来像 bug 的 error 日志

**症状**：bind 失败时日志里同时出现 `Endpoint dropped without calling Endpoint::close. Aborting ungracefully.`，以为是关闭逻辑出问题。

**根因**：`bind()` 里 address lookup 服务构造失败时（`endpoint.rs:303` 的 `?`），`ep` 已存在（`:293-295`）且未 close，early-return 会 drop 掉它 → 触发 `Drop for EndpointInner`（`socket.rs:220-230`）→ 打出那条 `error!`。

**正确做法**：排查 bind 失败时**别被这条日志带偏**，看真正的 `BindError` 变体。

> 这是从两处源码推出的代码路径，不是由某个测试断言的行为。

### A8. `BindError::InvalidTransportConfig` 名字看不出含义

**根因**：它的**唯一含义是「配了超过一个 relay transport」**（`socket.rs:906` 注释「Currently we only support a single relay transport」，抛出点 `:908`）。

**另注**：`BindError` 是 `#[non_exhaustive]`（`socket.rs:844-870`），下游 match 必须带 `_` 分支；用的是 `n0_error` 的 `#[stack_error(derive, add_meta)]`，**不是 thiserror**——`e!`/`bail!`/`anyerr!` 都是 n0_error 宏。

### A9. 「Empty preset」不代表不监听

**症状**：以为 `presets::Empty` / `Minimal` 是纯本地的，结果已经在所有网卡上开了 UDP 端口。

**根因**：`Builder::empty()` 的**底座**预置了 IPv4 + IPv6 两个 UDP transport（`endpoint.rs:191-197`，非 wasm）。「Empty」只指 preset 不写任何字段，**不代表 builder 是空的**——仍会绑 `0.0.0.0` 和 `[::]`。（IPv6 那个允许绑失败而不报错。）

**正确做法**：要改用 `clear_ip_transports()` + `bind_addr()`（`endpoint.rs:353-358` 有官方示例）。

> libp2p 里不 `listen_on` 就完全不监听；iroh 默认行为更激进。

---

## B. 关闭

### B1. 对端把「已成功传完」的连接判成超时失败

**症状**：数据明明全传完了，对端报连接超时/失败。

**根因**：**漏了 `endpoint.close().await`。** 文档（`endpoint.rs:1690-1699`）：

> Not doing so will make connections that were still open while closing the endpoint time out on the remote end. **Thus remote ends will assume connections to have failed even if all application data was transmitted successfully.**

`close()` 做了 **4 件 `Drop`/`abort()` 做不到的事**（二者共享另外四步：`at_close_start.cancel()` / `address_lookup().clear()` / `at_endpoint_closed.cancel()` / `closed.store(true)`）：

| # | 动作 | 位置 |
|---|------|------|
| ① | `noq_endpoint().close(0u16.into(), b"")` 排 CONNECTION_CLOSE 帧 | `socket.rs:1146` |
| ② | **`wait_all_draining().await`——冲帧并等对端 ACK**（`:1156` 注释「usually 3 seconds」）| `socket.rs:1164` |
| ③ | 100ms 优雅收尾 actor task | `socket.rs:1174` |
| ④ | `runtime.shutdown().await` 而非 `runtime.abort()` | `:1192` vs `:1228` |

②上方有一段罕见的「祖训注释」（`socket.rs:1148-1153`）：这行被删过两次（PR #1753、#3165）、加回过两次，「So before removing this call, please consider carefully.」

**正确做法**：**必须 `await`**。QUIC 的 ACK 在 user-land，不像 TCP 由内核在 Time-Wait 期间处理。

### B2. 日志里出现 `Endpoint dropped without calling Endpoint::close`

**根因**：`Drop for EndpointInner`（**`socket.rs:220`，不在 endpoint.rs**）打 `error!` 后 `abort()` 硬砍。

**这条 `error!` 是 iroh 少见的用 error 级别喊话的地方——看到它 = 你漏了 close()，不是 iroh 出错。**

⚠️ **它走 tracing——app 没装 subscriber 就什么都看不到，只表现为对端莫名超时。**

### B3. 给 `close()` 套 timeout 会两头落空

**症状**：`timeout(d, ep.close()).await` 超时后，既没优雅关闭也没 abort，只留一条误导性 error 日志。

**根因**：两个早退条件不同：

| | 早退条件 | 位置 |
|---|---------|------|
| `Drop` | `is_closed()`（**完全关闭**）| `socket.rs:222` |
| `abort()` | `is_closed() \|\| is_closing()` | `socket.rs:1214` |

而 `is_closing()` 的置位点是 `close()` 的**第一步**（`socket.rs:1140`）。所以 close 开始后 future 被取消 → Drop 照样打 `error!` → `abort()` 因 `is_closing()==true` 立刻 return，**什么都不做**。

**正确做法**：**要么完整 await `close()`，要么干脆别调直接 drop（至少 abort 会真的执行）。**

### B4. Endpoint 静默常驻，`error!` 也不打

**症状**：以为 drop 了，实际 endpoint 还活着。

**根因**：`Endpoint` 是 `Clone`（`endpoint.rs:897-900`，`Arc<EndpointInner>`），**`Drop` 实现在 `EndpointInner` 上**（`socket.rs:205`）——**只在最后一个克隆落地时才跑**。把 Endpoint clone 进 `tokio::spawn`（examples 里到处这么写），只要有一个克隆活着就不触发。

**正确做法**：`close()` 取 `&self`，**任意一个克隆调用即可**关闭所有克隆共享的 endpoint。（UDP socket 本身要等所有克隆都 drop 才真正释放。）

---

## C. 流

### C1. `open_bi()` 之后 read 永久挂死

**症状**：A `open_bi()` 后等 B 先说话，B 卡在 `accept_bi()` 上，双方永久挂起。

**根因**：**流是懒创建的。** `open_bi()` 不发任何网络包（纯本地记账），**对端完全不知道流存在**。三处文档警告：

```
// iroh connection.rs:839-841
/// Data must be sent on a stream before the respective accept call at the peer
/// will yield a [`RecvStream`].

// iroh lib.rs:151-156
//! To keep streams cheap, they are lazily created on the network ... The sender
//! **must** send data on the stream before the receiver's accept_bi call will return.

// noq-1.0.1/src/connection.rs:357-362
/// Calling [`open_bi()`] then waiting on the [`RecvStream`] without writing
/// anything to [`SendStream`] will never succeed.
```

**「便宜」和「懒创建」是同一枚硬币的两面。**

**正确做法**：**协议设计上必须保证「开流方先写」。** dumbpipe 就是靠发一个 5 字节 HANDSHAKE 常量破这个局。

> **从 libp2p 迁过来最容易踩的就是这条**——libp2p 开流会立刻走 multistream-select 协商往返，对端马上收到事件，原本「开流即通知」的假设在 iroh 下不成立。

### C2. 并发流卡在第 101 条

**症状**：开到 100 条流后就不动了，也不报错。

**根因**：默认 bi/uni 各 **100** 条并发上限（`noq-proto-1.0.1/src/config/transport.rs:552-560`），iroh 应用面**未覆盖**（`quic.rs:151-162` 只动 keepalive/multipath/NAT）。超限是 `Poll::Pending` **背压排队**，不是错误（`noq connection.rs` 的 `poll_open`：只有 `state.error` 存在才 `Err`）。

⚠️ **文档里那句 2^62 "essentially unlimited"（`connection.rs:863-865`）说的是生命周期累计总数，不是并发数——很容易读串。**

**正确做法**：要 >100 并发流必须显式调 `max_concurrent_bidi_streams()`——**但方向和你想的相反**，见 C3。

### C3. 调大了 `max_concurrent_*_streams`，限制没生效

**症状**：改了自己的 config 想开 500 条流，还是卡在 100。

**根因**：**你设的限制是「对端能向你开多少」，不是你能开多少。** 文档措辞（`quic.rs:169-176`）：「Maximum number of **incoming** bidirectional streams... Must be nonzero **for the peer to open** any bidirectional streams.」

**正确做法**：**必须让对端把它的 `max_concurrent_bidi_streams` 调到 500。** 双方都是 iroh 默认时，双向各 100。

> **libp2p 对照（方向搞反会导致限制完全不生效）**：yamux 的 `set_max_num_streams` 是**本地对称限制**（`self.streams.len() >= self.config.max_num_streams` 直接卡自己出站），语义**正好相反**——**libp2p 卡自己，QUIC 卡对端**。
>
> 运行期版本（`connection.rs:1084-1108`）取 `&self` 且可随时调，不用重建连接。

### C4. 内存被多条流吃爆

**症状**：单条连接吃到 100+ MB。

**根因**：worst-case ≈ `max_concurrent_bidi_streams × stream_receive_window` = **100 × 1.25MB = 125MB/连接**。而 **`receive_window` 默认是 `VarInt::MAX`（无限）**，所以文档那句「upper bound proportional to `receive_window`」在**默认配置下不构成任何保护**。

默认值推导（`noq-proto/src/config/transport.rs:544-558`）：`STREAM_RWND = 12500*1000/1000*100 = 1_250_000`。

**对文件传输类应用（同时开多条大流、可能来不及读）这是真实风险。**

**正确做法**：调小 `stream_receive_window`，或显式设 `receive_window` 做聚合兜底。官方告诫（`quic.rs:216-222`）：「Setting this smaller than `receive_window` helps ensure that a single stream doesn't monopolize receive buffers」。

> **libp2p 对照**：yamux **强制**你先算内存——`max_connection_receive_window` 必须 ≥ 256KiB × `max_num_streams`，否则 assert 直接 panic（`yamux-0.13.10/src/lib.rs:129-134`）。QUIC 不强制，**更自由也更容易踩**。

### C5. 从 libp2p 迁来的限流逻辑是净负债

**根因对照**：

| | 超限行为 | 默认上限 |
|---|---------|---------|
| **yamux 出站** | `Err(ConnectionError::TooManyStreams)` + `log::error!`——**连接级错误，整条连接不可用**（`yamux-0.13.10/src/connection.rs:489-491`）| 512（`lib.rs:90`）|
| **yamux 入站** | `Action::Terminate(Frame::internal_error()/protocol_error())`——**当场终止整条连接**（`connection.rs:622-624` / `:725-727`）| 同上 |
| **QUIC** | `Poll::Pending` 排队 | 100 |

**libp2p 下「流是稀缺资源、要多路复用管理」是被迫的——不管理就打死连接。iroh 下「无脑多开」最坏只是变慢。**

⚠️ **反直觉**：yamux 默认 512 比 QUIC 默认 100 更宽——**但 yamux 超限是致命的。「数字更大」反而更危险。**

**正确做法**：原有的限流/流池逻辑**可以直接移除**。

### C6. 「不需要锁」不等于「无锁」

**根因**：`Connection` 是 Clone 且 `open_bi` 只要 `&self`，**调用方**不必自己加锁。但 noq 内部每条连接有一把互斥锁——`poll_open` 第一行就是 `conn.lock_without_waking("poll_open")`（`noq-1.0.1/src/connection.rs:1000`），`ConnectionRef` 本身即 `Arc<Mutex<State>>` 语义。

**准确表述是「调用方不必自己加锁」，不是「无锁」。**

---

## D. Router / 协议

### D1. 服务端起来了，但没人能连上

**可能根因 1**：**`Router::spawn()` 的返回值被丢弃。** 它带 `#[must_use]`，且 task 是 `AbortOnDropHandle`——`Router::builder(ep).accept(..).spawn();`（不存返回值）会让 accept 循环**立刻被 abort**。结构体文档明写「When dropped, this will abort listening the tasks, so make sure to store it.」

**可能根因 2**：**混用了 `Endpoint::builder().alpns()` 和 Router。** `spawn()` 里的 `set_alpns` 是**覆盖式**的（`endpoint.rs:959-975` doc：「Note that this **overrides** the current list of ALPNs.」）——`.alpns(vec![X]).bind()` 后再 `Router...accept(Y, h).spawn()`，X 被静默丢弃。

**可能根因 3**：**空 Router**（一个 `.accept()` 都没调）会 `set_alpns(vec![])`，把入站 ALPN **清空**。

**可能根因 4**：endpoint 已 closed 时 `set_alpns` **只 warn 不报错**，`spawn()` 依然返回一个「永远收不到连接」的 Router。

**正确做法**：**ALPN 只该在一个地方声明**（用 Router 就别用 `.alpns()`），且必须存住 Router。

### D2. 某个 ALPN 神秘失灵，无任何报错

**根因**：`ProtocolMap::insert` 就是 `BTreeMap::insert`（`protocol.rs:376-388`）——**同一 ALPN 注册两次，后者静默覆盖前者**。ALPN 常量拼错或两个库恰好用了同一个字节串时就会这样。

**从外部完全观察不到冲突发生过**——因为 `alpns()` 取的是 BTreeMap 的 keys，被覆盖的那个 handler 连 ALPN 都不会重复出现在 `set_alpns` 列表里。

**正确做法**：同一 handler 注册到多个 ALPN 是合法的（`.accept(V1, h.clone()).accept(V2, h)`），这是做版本兼容的正规姿势。

### D3. 整个节点静默下线，`Router::shutdown()` 还返回 Ok

**症状**：所有协议都不工作了，出站连接也断了，日志里只有一行 `error!`。

**根因**：**任一 handler task panic → 掀掉整个 accept 循环 → 执行完整关闭序列，包括 `endpoint.close()`**（`protocol.rs:533-546` 的 `break` + `:597-602`）：

```rust
if outer.is_panic() { error!("Task panicked: {outer:?}"); break; }
...
protocols.shutdown().await;
handler_cancel_token.cancel();
// Now we close the endpoint. This will force-close all connections that are not yet closed.
endpoint.close().await;      // ← 连坐所有协议 + 该 Endpoint 的所有出站连接
```

**而且文档与实现对不上**：`Router::shutdown` 文档（`protocol.rs:426-428`）声称会把 panic 传播到返回值——**但代码里没有任何 `resume_unwind`**，`run_loop_fut` 只 `error!` 一行然后 break、自身正常结束，所以 `task.await?` 返回 `Ok(())`。

**正确做法**：**handler 里千万别 unwrap/expect/索引越界。一次 panic = 整个节点静默下线。**

> **libp2p 对照**：libp2p 里事件循环是你自己的，panic 直接炸你的主循环 task，**故障边界是显式的**、你能 catch/重启。iroh 的 Router 把 task 藏在内部，从外部看只是 endpoint 悄悄 closed——**故障边界隐式且不易察觉**。

### D4. `is_shutdown() == true` 但没人调过 shutdown

**根因**：`run_loop_fut` 开头挂了 `let _cancel_guard = cancel_token.clone().drop_guard();`（`protocol.rs:520-522`），**循环无论因何退出**（收到 cancel / endpoint 被别处关 / handler panic）**都会把 token 置为 cancelled**。

**`is_shutdown()` 只表示 accept 循环已经不在跑了。** 反过来可以利用：**轮询它可以探测「节点是不是意外挂了」**。

另注：`shutdown()` 里 `.take()` 了 task，**只有第一个调用者真正 await 到关闭序列结束**；并发调用的其它方因 `is_shutdown()` 已 true 立刻返回 Ok。

### D5. handler 返回 Err，对端收不到任何原因

**根因**：文档（`protocol.rs:258-272`）：

> Returning an `Err<AcceptError>` will also drop the connection and log a warning, but **no dedicated error code will be sent to the peer**, so it's recommended to explicitly close the connection within your accept handler.

**`AcceptError::NotAllowed` 也一样**——它是纯语义标记，Router 不做任何特殊处理（不发 CONNECTION_REFUSED、不 close with code），对端只看到连接被断。

**正确做法**：
- 要让对端知道原因 → 自己 `connection.close(code, reason)`
- 要真的发 CONNECTION_REFUSED → 用 `RouterBuilder::incoming_filter` 的 `Reject`

注意变体写作 `NotAllowed {}`（带空花括号），构造要用 `e!(AcceptError::NotAllowed)`。

### D6. 响应数据被截断

**根因**：**`accept()` 一 return，Connection 就被 drop**（若无其它克隆则连接关闭），可能把还没 flush 出去的响应截断。

**正确做法**：echo 范例结尾的 `connection.closed().await;` **不是可选的礼貌**——`echo-no-router.rs:108-110` 注释：「Wait until the remote closes the connection, which it does once it received the response.」

**「什么时候 return」是协议正确性的一部分。**

> libp2p 的 request_response 帮你管了响应生命周期（塞进 `ResponseChannel`）；iroh 里 Connection 的 drop 时机直接决定线路行为。

### D7. `?` 传上来的错误分不清是 IO 还是业务

**根因**：`AcceptError` 有两个手写 `From`（`protocol.rs:113-162`），**都映射到 `User` 变体而非专门变体**：

```rust
impl From<std::io::Error>     for AcceptError { fn from(err) -> Self { Self::from_err(err) } }
impl From<quic::ClosedStream> for AcceptError { fn from(err) -> Self { Self::from_err(err) } }
```

所以 `connection.accept_bi()` 的 `ConnectionError` 走 `Connection` 变体，但 `tokio::io::copy` 的 `io::Error` 和 `send.finish()` 的 `ClosedStream` **都被塞进 `User`**。

另注：`AcceptError` 是 `#[non_exhaustive]`；**不是所有错误类型都有 From**——`examples/search.rs` 里 `read_to_end` / `String::from_utf8` / `write_all` 都得显式 `.map_err(AcceptError::from_err)`。

### D8. handler 里想改自己的状态，编译不过

**根因**：`ProtocolHandler` 三个方法**都取 `&self`**（`protocol.rs:228-287`），不是 `&mut self`。

**正确做法**：内部状态用 `Arc<Mutex<..>>`/原子量。官方 `screening-connection.rs` 就是 `struct ScreenedEcho { conn_attempt_count: Arc<AtomicU64> }`。

**另一个常见编译错**：`Debug` 是 supertrait（`Send + Sync + Debug + 'static`），**忘了 `#[derive(Debug)]` 编译不过**。

**还有一个版本坑**：签名变过（CHANGELOG #3320 "Make ProtocolHandler use async functions"，#3010 移除旧的 Arc 要求）——**网上 iroh 0.x 时代的教程签名对不上**。

### D9. 运行时按配置挑 handler，类型对不上

**根因**：`ProtocolHandler` 用了 RPITIT，**不是 dyn-safe**。

**正确做法**：显式标注成 `Box<dyn DynProtocolHandler>`——直接写 `if cond { A } else { B }` 类型对不上。`DynProtocolHandler` 是 pub 的（CHANGELOG #3366 "Expose DynProtocolHandler"）就是为了这个场景。

它有 **3 个方法**（`on_accepting` :333-341 默认 / `accept` :344-347 无默认 / `shutdown` :350-352 默认），blanket impl 在 `:355-373` 三个方法全部转发——**类型擦除后没有丢失 shutdown 钩子和 on_accepting 拦截**。

### D10. handler 的 shutdown() 没被调用 / 对端收不到 close code

**根因**：Router 的关闭编排是严格四步（`protocol.rs:597-612`），**① `protocols.shutdown().await` 并发调用并等待所有 handler 的 shutdown 跑完，之后才 ② cancel accept future ③ `endpoint.close()` ④ abort**。

**如果你的 handler 不实现 `shutdown()`（默认空 impl），`Router::shutdown` 会立刻走到 `endpoint.close()`，连接被强关，对端收不到任何 application close code。**

**正确做法**：官方认可的模式是「在 `accept()` 里存下 Connection、在 `shutdown()` 里统一优雅关闭」——有测试兜底（`test_graceful_shutdown`，`protocol.rs:1008-1058`，断言对端收到 `ApplicationClosed { error_code: 42, reason: "shutdown" }`）。

> ⚠️ **别写「libp2p 没有关闭编排」**：libp2p **有** `ConnectionHandler::poll_close`（`libp2p-swarm-0.47.1/src/handler.rs:179`，doc：「we will first poll this function to completion. Following that, the physical connection will be shut down.」），另有 `Swarm::close_connection`(`lib.rs:658`) / `disconnect_peer_id`(`lib.rs:637`)。**libp2p 真正缺的是全局的、可 await 的 Swarm 级 `shutdown()`。**

### D11. ALPN 不匹配时，日志里看不到对端提议的 ALPN

**症状**：想调试对端用了什么 ALPN，`router.accept` span 里 alpn 字段是空的。

**根因**：**rustls 在握手阶段就发 `no_application_protocol` alert**。`Accepting::alpn(&mut self)`（`connection.rs:558`）委托给 `alpn_from_noq_connecting`（`:279`），其 `conn.handshake_data().await?` 直接返回 Err → 走的是 `protocol.rs:636` 的 `warn!("Ignoring connection: invalid handshake: {err:#}")`，**不是 `:643` 的 "unsupported ALPN protocol"**。

而 span 的 alpn 字段在 `:640` 才 record，`:636` 的失败发生在它之前——**所以 alpn 字段仍是 Empty**。

（`:643` 那个分支在纯 Router 用法下基本走不到，除非你在 `spawn()` 之后又手动调了 `endpoint.set_alpns()`。）

**另注**：`remote` 字段只在 `on_accepting` 成功之后才 record，被 `on_accepting` 拒掉的连接日志里 remote 是空的。

**也别说「ALPN 不匹配的连接根本到不了 Router」**：`Incoming` 照样出来、照样过 filter、照样被 spawn 出带 span 的 task，TLS 拒绝是在 task 内部才显形。准确说法是「**到不了 handler / 到不了 ALPN 查表那一层**」。

### D12. incoming_filter 拖慢了整个 Router

**根因**：filter 是**同步闭包**（`Arc<dyn Fn(&Incoming) -> IncomingFilterOutcome>`，`protocol.rs:214-215`），**内联跑在 accept 循环主线上**（`:562-585`，spawn 之前）。

**里面做慢事会拖住整个 Router 的 accept 吞吐。** 想在这层看 ALPN 只能 `Incoming::decrypt()` 解 Initial 包（~1200 字节的 clone + 解密），官方明说贵（`:465-468`）：「should reject based on cheaper signals (e.g. remote address) first」。

**另注 `Retry` 的语义随连接类型而变**（`:170-208`）：直连 UDP 上是真的源地址验证；**走 relay 时没有源地址可验，它只是给客户端强加一个 relay 往返 + 重发 ClientHello 的成本**。

### D13. 想把 Incoming 交给自己的连接池，Router 做不到

**根因**：`incoming_filter` **只给借用 `&Incoming`，拿不到所有权**。

**这是 Router 与手写 accept 循环之间真实的能力差异**（不只是工效差异）。需要所有权转移、需要 0-RTT 的完整 Incoming 控制（`examples/0rtt.rs:145-148`）、或要接管 task 调度策略时，必须手写。

**其余情况默认用 Router**——`echo-no-router.rs` 自己的注释就承认 `tokio::spawn` 是简化写法、生产该用 TaskTracker/JoinSet，**而那正是 Router 已经写好的**。

### D14. Router 不限流

**根因**：`join_set` **无上限**。要限流得自己在 `incoming_filter` 或 `on_accepting` 里做（这正是 `screening-connection.rs` 的用意）。

### D15. 0-RTT 两边签名不对称，照抄会编译不过

| 侧 | 签名 | 会失败吗 |
|----|------|---------|
| 接受侧 `Accepting::into_0rtt()` | `-> IncomingZeroRttConnection` | **不会**（`connection.rs:620-624` 里 `.expect("incoming connections can always be converted to 0-RTT")`）|
| 连接侧 `Connecting::into_0rtt()` | `-> Result<OutgoingZeroRttConnection, Connecting>` | 会（被拒时把 Connecting 还给你）|

**安全提醒是硬性的**：0-RTT 数据**可被重放**，绝不能用来触发非幂等操作（`connection.rs:605-608`）。

官方 `0rtt.rs` 例子走的是手写 `endpoint.accept()` 而不是 Router——**Router + 0-RTT 这条路虽然文档支持，但缺可抄的完整范例**。

---

## E. Watcher

### E1. `home_relay_status().initialized()` 返回了，但根本没连上

**症状**：等 `initialized()` 返回后就去连人，失败。

**根因**：链条闭合于三处：

1. **`Nullable<Vec<T>>::into_option` 就是 `self.pop()`**（`n0-watcher lib.rs:109-125`）——**非空即返回**，且返回的是**最后一个元素**（其余静默丢弃）
2. **RelayActor 在拨号之前就发布 `Connecting`**（`actor.rs:1138-1145`），`Connected` 要等 `run_dialing()` 返回（`:364-372`）
3. Vec 序列是 `[]` → `[Connecting]` → `[Connected]`，`initialized()` 在中间就返回

`Endpoint::online` 的文档（`endpoint.rs:1310-1312`）写死了：「**Merely selecting a relay URL is not sufficient.**」

> ⚠️ **别写「必然」**：`InitializedFut` 的 `initial` 在 future **创建时同步求值**（`lib.rs:312`），若此时已连上则直接返回 `[Connected]`。准确说法：**`initialized()` 在它观测到的第一个非空值处返回，对「是否已连上」不提供任何保证。**（这个准确版反而更有力。）

**正确做法**：用 `Endpoint::online()` + timeout。**判据极简：Value 是 `Option<T>` 才用 `initialized()`，是 `Vec<T>` 就别用。**

**全仓佐证**：`grep -rn "\.initialized()"` 整个 iroh-study **只有 1 处命中**——`endpoint.rs:1428` 的 `net_report()` 文档示例（那里 Value 是货真价实的 `Option<NetReport>`）。所有真实的「等上线」点（sendme / dumbpipe / examples / tests）**无一例外用 `online()`**。

### E2. `initialized().await` 静默僵死，无日志无错误

**根因**：Watchable 被 drop 后，`InitializedFut::poll` 返回 `Poll::Pending` 且**永不再被唤醒**（`n0-watcher lib.rs:703-718`）：

```rust
if ready!(this.watcher.poll_updated(cx)).is_err() {
    // The value will never be initialized
    return Poll::Pending;          // ← 不是 Err
}
```

**因为 Output 是裸 `T` 而非 `Result`，它在类型上就没有表达失败的能力。**

对照：`updated()` 返回 `Err(Disconnected)`，`stream()` 以 `None` 正常结束。

**正确做法**：**任何 `initialized().await` 都必须外包 timeout。**

### E3. 换 relay 时 `is_connected()` 抖动，误报"掉线"

**根因**：切 home 时**无条件先 publish `Connecting`**（`actor.rs:1142-1144`），哪怕目标 relay 的 ActiveRelayActor 早已 Connected（注释：「If an `ActiveRelayActor` already exists for this URL it will republish its actual status...」）。

**所以换 relay 时必然看到一次 Connected → Connecting → Connected，即使连接从未断过。**

**正确做法**：别把 `is_connected()==false` 当成掉线告警。

### E4. Watcher 丢了中间状态

**根因**：**Watcher 是 last-value-wins 的有损采样，不是事件流。** 模块文档（`n0-watcher lib.rs:3-5`）：「The aim is to always be aware of the **last** value, not to observe *every* value change.」

- `set()` **只在值真的变了时才唤醒**（`lib.rs:147-172`，值相同则 `Err(value)` 且不通知）——**没有心跳语义**
- 中间值会被跳过（`lib.rs:213-218` 明说）

**绝不能用 Watcher 做「统计状态变化次数」或「捕获每一次 Connecting→Connected 翻转」。**

> **这是与 libp2p 最根本的心智差异。** `SwarmEvent` 流是**无损、有序、必达**（不 poll 就在队列里等）；Watcher 是**不 poll 就被覆盖**。模块文档 `lib.rs:63-70` 自己对比过 `tokio::broadcast`（不丢）和 `tokio::watch`（最新值语义）——Watcher 属于后者。

**一个反向利用**：`RelayConnectionState` 的 `PartialEq` 对 `Disconnected.last_error` 用 **`Arc::ptr_eq` 按指针比较**（`actor.rs:925-939`），注释：「each new failure produces a fresh allocation, so the watcher fires on every new error」——**故意绕过「值不变不通知」。别指望「同样的错误不会重复通知」。**

### E5. `close()` 后 watcher stream 不终止

**根因**：**`close()` 不断开 watcher，只有最后一个 Endpoint 克隆 drop 才会。** 三个公开 Watcher API 的文档都重复了这段警告（`endpoint.rs:1235-1266` / `:1380-1383` / `:1412-1415`）——说明这是踩过的坑。

**正确做法**：配合 `Endpoint::closed()` 用 `run_until`（官方示例就在那段 doc 里）。

### E6. `.is_empty()` 判断「relay 没开」会误判

**根因**：`home_relay_status()` 的空 Vec 有**两种截然不同、无法区分**的含义：(a) 压根没配 relay（`RelayMode::Disabled`），(b) 配了但 net_report 还没跑出 preferred_relay。

公开文档措辞很准（`endpoint.rs:1374-1376`）：「is empty when no relays are configured **or before the endpoint has selected a home relay**」——注意是「**selected**」而非「connected」。

**正确做法**：想等「上线」直接用 `Endpoint::online()`。

### E7. `peek()` 拿到旧值

**根因**：`peek()` 返回的是「**这个 watcher 上次抓到的值**」，不是「Watchable 当前的值」（`n0-watcher lib.rs:255-275` doc：「Calling this won't update the latest value, unlike `get` or `update`」）。

**正确做法**：要最新值必须 `get()`/`update()`，代价是持 `&mut`。

**另注**：`get()` 在上游已 drop 时会**返回最后一次抓到的旧值而不报错**（`lib.rs:243-247`），n0-watcher 自己的测试 `test_updated_then_disconnect_then_get` 固化了这个行为。

### E8. `stream().next()` 立刻返回

**根因**：`stream()` 的首个 item 是「**当前值**」而非「一次变化」（`lib.rs:330-362`：`initial: Some(self.get())`）——跟 `updated()`（必等下一次变化）**语义相反**。

**正确做法**：想要纯粹的变化通知用 `stream_updates_only()`。

### E9. `watch_addr()` 里有 relay 地址 ≠ 能被拨到

**根因**：relay transport 的 `local_addr_watch()`（`socket/transports/relay.rs:180-185`）把任意 `Option<RelayStatus>` 无差别映射成 `Option<(RelayUrl, EndpointId)>`——**完全不看 state**。而 status 在 `Connecting` 阶段就已经是 `Some`。

**另注 `Endpoint::addr()` 就是 `self.watch_addr().get()`**（`endpoint.rs:1196-1198`）——一个**可能残缺的瞬时快照**，早期调用会拿到不完整的 EndpointAddr。iroh 文档也提醒先 `online()`（`endpoint.rs:1189-1192`）。

### E10. `iroh::direct_addrs()` 找不到

**根因**：**1.0.2 没有这个 API。** 公开返回 Watcher 的只有三个：`watch_addr()` / `home_relay_status()` / `net_report()`（后者 feature 门控）。

`direct_addrs` / `direct_addresses` / `conn_type` 在公开 API 里**都不存在**。历史：`CHANGELOG.md:1717`（local_endpoints → direct_addresses）、`:631`（#3505 Make direct_addresses always be initialised）、`:441`（**Remove Endpoint::conn_type #3647**）。

**正确做法**：直连地址通过 `watch_addr()` 返回的 EndpointAddr 里的 `TransportAddr::Ip(..)` 间接看。

### E11. 照 `socket.rs` 的 ip_addrs 文档写 `.initialized()` 编译不过

**根因**：**`socket.rs:446-460` 的文档已过期**——它写着「the current item in this Watcher will be [`None`]... use [`Watcher::initialized`]」，但字段实际类型是 `Watchable<BTreeSet<DirectAddr>>`（`socket.rs:2029`），而 **`Nullable` 根本没为 `BTreeSet` 实现**。

已实测：rustc 报 `E0277: the trait bound BTreeSet<u8>: Nullable<_> is not satisfied`，并自动列出只有 `Option<T>`/`Vec<T>` 实现 `Nullable`。

（疑似 #3505 的遗留，但**因果归因未经 git blame 证实**。）

---

## F. Relay

### F1. 自建 relay 每次 net_report 白等 3 秒

**根因**：**两边默认值相反**：

| 侧 | 默认 |
|----|------|
| 客户端 `RelayConfig::from(RelayUrl)` | `quic: Some(RelayQuicConfig::default())` → **QAD 开在 7842/udp**（`relay_map.rs:272-310`，`DEFAULT_RELAY_QUIC_PORT = 7842`）|
| 服务端 `Config` | `enable_quic_addr_discovery: false`（`main.rs:350-364`）|

`FromIterator<RelayUrl>` 走的也是这条（`relay_map.rs:203-217`，注释明说会带 default QUIC 端口）。

**症状**：`RelayMode::custom(["http://myrelay".parse()?])` 时客户端白白往 7842/udp 发探测、超时失败（不致命，只是每次 net_report 浪费 3s PROBES_TIMEOUT）。

**正确做法**：显式构造 `RelayConfig::new(url, None)` 再 `RelayMap::from_iter`。

（「QUIC 在手机键盘上打出来是 7842」是个好记忆点。）

### F2. `--dev` + QAD 是死路

**根因**：QAD 的 TLS 检查（`main.rs:585-587` 的 `bail_any!("TLS must be configured in order to spawn a QUIC endpoint")`）在 `if cli.dev`（`:588`）**之前**执行 → `--dev` + QAD + 无 TLS 直接 bail；就算配了 TLS，dev 又会把它 `dangerous_http_only = true` 关掉。

**正确做法**：本地调 relay 老实关掉 QAD。

### F3. scheme 拼错，错误延后到 TLS 握手才炸

**根因**：`_ => "wss"` 是**通配兜底不是白名单**（`client.rs:267-281`）：

```rust
dial_url.set_scheme(match self.url.scheme() { "http" => "ws", "ws" => "ws", _ => "wss" })
```

**`RelayUrl` 不做 scheme 校验**（`iroh-base/src/relay_url.rs:21-44`，只是 `Arc<Url>` newtype）。写 `htp://`、`foo://` 不会报错，被静默当成 wss 去连，**报错信息完全不指向 scheme 拼错**。

**另一个坑**：`RELAY_PATH` 是 `set_path` **覆盖**写入的——**你 URL 里带的任何 path 都会被丢弃**。

### F4. `RelayMode::Disabled.relay_map().is_empty()` 判断 relay 是否启用是错的

**根因**：两条路径语义不同：

```rust
// endpoint.rs:1936-1944 —— 这条返回空 map
RelayMode::Disabled => RelayMap::empty(),
// endpoint.rs:153-170 —— 但 Builder 走的是这条
impl From<RelayMode> for Option<TransportConfig> { RelayMode::Disabled => None, ... }
// endpoint.rs:557-575 —— None 分支直接 retain 掉整个 Relay transport
None => { self.transports.retain(|t| !matches!(t, TransportConfig::Relay { .. })); }
```

**`Builder::relay_mode(Disabled)` 是把 Relay transport 整个移除，不是给个空 map。**

### F5. `N0DisableRelay` ≠ 离线

**根因**：它是 `N0.apply(builder).relay_mode(RelayMode::Disabled)`（`presets.rs:177-184`）——**先跑完整 N0 再关 relay**。净效果是没有 relay transport，**但 address lookup 服务一个不少，它依然联网发布地址**。

### F6. 两个同名的 `RelayConfig`

| 类型 | 位置 | 语义 |
|------|------|------|
| `relay_map::RelayConfig` | `iroh-relay/src/relay_map.rs:232` | **客户端侧**。被 iroh 顶层 re-export（`iroh/src/lib.rs:290`），**`iroh::RelayConfig` 是这个** |
| `server::RelayConfig` | `iroh-relay/src/server.rs:127` | **服务端侧**。需 `use iroh_relay::server::RelayConfig` |

**字段完全不同。同时 import 必须 alias。文档/示例里看到 `RelayConfig` 一定要先看 use 语句。**

### F7. `home_relay_status()` 健康 ≠ 所有 relay 连接健康

**根因**：**home relay ≠ 唯一连的 relay。** 给对端发包时若对端 home 在另一个 relay，本地会再开一条到那个 relay 的连接（`actor.rs:1166-1211`，除非某条已有连接已知道该对端）。非 home 的连接 60s 无写入就关（`RELAY_INACTIVE_CLEANUP_TIME`），**home relay 的 actor 永不退出**（`actor.rs:6-9`）。

### F8. 纯 HTTP 自建 relay 能连上，以为 auth 被跳过了

**根因**：**auth 一直在做，只是静默降级换了路子。** `KeyMaterialClientAuth::new` 依赖 TLS keying material，导出不到就返回 `None`（`protos/handshake.rs:251-269`），然后走 `ServerChallenge → ClientAuth` 的**多一轮握手**（`:340-378`）。

**降级只有一行 debug 日志**（`client.rs:328-335`）。纯 HTTP 能用，只是每次建连多一个 RTT。

### F9. 三处会骗人的 stale 文档

| 位置 | 骗人内容 | 真相 |
|------|---------|------|
| **`iroh/iroh/src/lib.rs:97-110`** | 「initiated as normal HTTP 1.1 connections using **TLS**. Once connected the transport is **upgraded to a plain TCP connection using a custom protocol**」 | 实际是 **WebSocket**，且 **TLS 非强制**。**别引用 lib.rs 的 Relay Servers 段落写文档** |
| `iroh-relay/src/http.rs:11-13` | 「over websockets **and a custom upgrade protocol**」 | 无第二种 upgrade 分支 |
| `iroh-relay/src/server/http_server.rs:329` | 「(**and "/derp" for backwards compatibility**)」 | 无任何 `/derp` 路由常量 |

后两条是 0.91 删旧路径时漏改的注释。

**还有一个 stale 死代码**：`iroh-relay/src/client.rs:101` 的 `RelayProtoNotAvailable`（"The relay protocol is not available in browsers"）**从未被构造**（全 workspace 只有这一处声明），是 pre-1.0 的残留。**1.0.2 里 native 与 browser 都是 WebSocket，不存在这个分叉。**

### F10. relay 服务器没有公钥可钉扎

**根因**：`ServerConfig` / `server::RelayConfig` **全无 secret_key 字段**。relay 是「按 EndpointId 转发密文」的哑管道。客户端身份由 handshake 校验（ServerChallenge → ClientAuth），**不是 mutual 的**。

**信任模型 = 「相信它不做流量分析」，不是「相信它的公钥」。纯 HTTP 自建 relay 的 URL 可被中间人劫持**（虽然 payload 仍是端到端加密的）。

> ⚠️ **别说「SecretKey 在 iroh-relay 里只出现在测试代码」**——服务端侧文件（`server.rs` / `http_server.rs` / `main.rs`）确实如此，但**客户端侧**（`client.rs:328`、`protos/handshake.rs:225/254/342`、`client/conn.rs:90`）在非测试代码里正常用 SecretKey 签 challenge——iroh-relay 这个 crate **同时装着客户端和服务端**。

### F11. 跨 0.91 / 0.98 的兼容性

**两层版本线，别混为一谈**：
- **0.91**：「传输壳子只能是 WS」——同时改了 handshake 协议和帧格式（两条 breaking：#3389 / #3331）。**跨 0.91 的 relay 客户端/服务端不互通**
- **0.98**：「WS 里跑的帧格式是 v2」——`ProtocolVersion` 走 `Sec-WebSocket-Protocol` 子协议头（`http.rs:25-68`）

**已核实**：**新服务端对老客户端明确硬拒**（`http_server.rs:603` `expect_header(&req, SEC_WEBSOCKET_PROTOCOL)?` 强制要求，不带就 MissingHeader）。

⚠️ **反方向（新客户端 → 老服务端）本仓无法核实。** 只能确定：服务端不回显该头 → `protocol_version_str` 为 None → `BadVersionHeader` 硬失败（`client.rs:345-355`）。**别写「老服务端会回落 v1，能用」——那只在老服务端主动回显 `iroh-relay-v1` 时才成立。**

---

## G. 发现 / 隐私

### G1. `iroh::discovery` / `Discovery` trait 找不到

**根因**：**0.96.0 的 breaking rename**（`CHANGELOG.md:487`，PR #3853）：`Discovery` → `AddressLookup`，`discovery/` → `address_lookup/`。builder 方法是 **`.address_lookup()`**，不是 `.discovery()`。

**docs.rs 上的老版本教程、绝大多数博客仍写 `Discovery`。**

**其它 1.0 前夕的重命名**（跨版本抄代码必踩）：`NodeId` → `EndpointId`、`NodeAddr` → `EndpointAddr`、`EndpointMap/EndpointState` → `RemoteMap/RemoteState`(#3673)、`CustomAddr::as_vec` → `to_vec`(#4074)、`CaRootsConfig` → `CaTlsConfig`(#4300)。

### G2. 找不到 mDNS feature

**根因**：**iroh 1.0.2 里没有 mDNS——不是 feature 关着，是代码不在这个 crate 里。** `grep -rin mdns --include=*.rs iroh/src` 只命中 **3 行**（`address_lookup.rs:46/47/50`），全是文档链接。

内建的 AddressLookup 只有：`PkarrPublisher` / `PkarrResolver` / `DnsAddressLookup`(非 wasm) / `MemoryLookup`。

**正确做法**：**必须额外引入 `iroh-mdns-address-lookup` crate，不是开 feature。**（Mainline-DHT 同理：`iroh-mainline-address-lookup`。）

### G3. 别名为 `static` 的实现

**根因**：「静态/手动添加地址」这一角色由 **`MemoryLookup`** 承担。`ls iroh/iroh/src/address_lookup/` 只有 dns.rs / memory.rs / pkarr.rs。

⚠️ **「MemoryLookup 的旧名是 StaticProvider」本仓无法核实，别写**：`grep StaticProvider iroh/src/` 零命中；`grep -rni MemoryLookup` 在两个 CHANGELOG 零命中；仓是 shallow clone 无历史可追。

### G4. 只加了 `DnsAddressLookup`，别人查不到我

**根因**：**发布/解析职责分离**——`AddressLookup` trait 两个方法都有默认空实现（`address_lookup.rs:333-350`）。`PkarrPublisher` 只实现 `publish`；`DnsAddressLookup` / `PkarrResolver` 只实现 `resolve`；`MemoryLookup` 的 `publish` 是**显式空实现**（`memory.rs:219`）。

**DNS 侧没有发布通道，发布只能走 pkarr。**

### G5. 用了 `presets::Empty` / `clear_address_lookup()` 后 connect 只认显式地址

**根因**：未配置任何服务时**不是静默成功**，而是 stream 立刻吐 `AddressLookupFailed::NoServiceConfigured`（`address_lookup.rs:620-626`），且 `remote_state.rs:896-898` 对它**只打 trace 不打 debug**。

### G6. `publish()` 失败拿不到任何信号

**根因**：trait 契约就是 fire-and-forget（`address_lookup.rs:334-341`：「This is fire and forget, since the [`Endpoint`] can not wait for successful publishing.」）。**`publish()` 无返回值也不报错——失败只体现为 `warn!` 日志**（`pkarr.rs:391-397`）。

**正确做法**：想确认「我已可被发现」只能自己去 GET 一次。

> libp2p Kad 的 put_record 有 Quorum 语义、能通过事件拿到成功/失败与副本数。

### G7. ⚠️ `AddrFilter::relay_only()` 挡不住 user_data 泄漏

**这是最容易误判的一条。**

**根因**：**`AddrFilter` 只过滤地址，`apply_filter` 会原样保留 `user_data`**（`iroh-dns/src/endpoint_info.rs:189-199`）：

```rust
Cow::Owned(addrs) => {
    let mut data = EndpointData::new(addrs);
    data.set_user_data(self.user_data.clone());   // ← 原样带过去
    Cow::Owned(data)
}
```

**filter 的函数签名里根本看不到 user_data**（`endpoint_info.rs:229-230`），**无从过滤**。

**正确做法**：要不发布 user_data，**唯一办法是不设它（默认即不设）或 `set_user_data_for_address_lookup(None)`。防线在 user_data 那一侧，不在 AddrFilter。**

**最关键的一条**：iroh 全链路**从不**发布 hostname/用户名/设备名（发布路径 grep 零命中），可发布字段只有地址集 + 可选 `user_data`（默认 `None`）。**但只要有人调了 `user_data_for_address_lookup(设备名)`，设备名就会被公开发布到 dns.iroh.link，且全球可无鉴权 GET。这是唯一需要 code review 卡住的 API。**

> **libp2p 对照**：libp2p 的 **Identify 默认就广播 agent_version / listen_addrs**（很多项目正是在这里泄露主机名或内网地址）；iroh **没有 Identify 等价物**，元数据面默认是空的——**默认更保守**。

### G8. `PkarrPublisher` 默认 relay_only 的归属别搞错

**事实**：默认 `AddrFilter::relay_only()` 设在 **`PkarrPublisherBuilder::new`**（`pkarr.rs:163`，filter 在 `:168`），**不是 `PkarrPublisher::new`**——后者在 `pkarr.rs:298-330`，**私有且 filter 必须显式传入，本身没有默认值**。

**默认之所以是铁的**：唯一的公开入口 `PkarrPublisher::builder()`(:290) / `n0_dns()`(:332) 都**强制经过 builder**，外部无法绕过。运行时生效点在 `pkarr.rs:351`。

**过滤是两层且会叠加**：endpoint 级 addr_filter（默认 `None` = 不过滤，`endpoint.rs:206`）+ 每个服务自己的 filter。**别以为「builder 没设 filter = 会发 IP」——Publisher 自己会兜住。**

### G9. 地址多了塞不进 DNS 包

**根因**：pkarr 签名包格式 `<32 pubkey><64 sig><8 timestamp><DNS packet>`，**DNS 包上限 1000 字节、整包 1104 字节硬上限**（`iroh-dns/src/pkarr.rs:17-30`），超了 `SignedPacketBuildError::PacketTooLarge`（`:72-77`）。

这正是 `EndpointData::new` 文档说「地址顺序被保留，可为 lookup 服务编码优先级，**以防塞不进单个 DNS 包**」的原因（`endpoint_info.rs:86-89`）。

> iroh **不依赖外部 `pkarr` crate**（`grep '^name = "pkarr"' iroh/Cargo.lock` → NOT IN LOCKFILE），格式由 iroh-dns 基于 `simple-dns` 自己实现。

### G10. 「iroh 用 BEP44 存进 Mainline DHT」对默认配置是错的

**根因**：**默认走 HTTP PUT 到 n0 服务器，DHT 一步都不沾。** `grep -rni "bep44|bep 44|bep-44"` **全树 0 命中**。iroh 客户端默认路径无 DHT：`grep -rniE "kademlia|k-bucket|routing_table"` 在 `iroh/src/` 只命中 3 处且均指**操作系统 IP 路由表**；`grep -rni "bootstrap" iroh/src/` **零命中**。

> ⚠️ **但别说成「Mainline DHT 代码全在仓外」**：本仓 `iroh-dns-server` **有** mainline DHT fallback（`Cargo.toml:38` `mainline = "7"`；`src/store.rs:11` `use mainline::{Dht, DhtBuilder, MutableItem};`——**`MutableItem` 就是 BEP44 mutable item**，只是没写这个词；`config.rs:53` `pub mainline: Option<MainlineConfig>`，`:286` **默认 `None`**）。
>
> **准确表述**：iroh 自己不实现 BEP44、**客户端默认路径一步不沾 DHT**；BEP44 语义经 `mainline` crate 出现在**服务端可选 fallback** 里（默认关闭）。客户端侧的 Mainline lookup（`iroh-mainline-address-lookup`）在仓外。
>
> 同理 `pkarr.rs:121-123` 那句「it does not interact with the Mainline DHT」是**描述 n0 那台部署实例的配置，不是软件能力**——`pkarr.rs:9-20` 明说通用 pkarr relay「will usually perform the publishing to the Mainline DHT on behalf on the client」，**n0 是刻意的例外**。

### G11. DNS 属性只看 dns.rs 文档会漏

**根因**：`iroh/iroh/src/address_lookup/dns.rs:24-38` 的 doc **只列了 `relay=<url>` 一个属性**。三个属性的真正依据在 `iroh-dns/src/attrs.rs:78-89`：

```rust
#[strum(serialize_all = "kebab-case")]      // ← UserData → user-data 的来源
pub(crate) enum IrohAttr { Relay, Addr, UserData }
```

即 `relay=` / `addr=`（**Ip 与 Custom 共用**，`endpoint_info.rs:492`）/ `user-data=`。记录名 `_iroh`（`attrs.rs:20`）。

**另注**：DNS 查询是 staggered 并发（`DNS_STAGGERING_MS = &[200,300,600,1000,2000,3000]`），每次查询自身超时 3s，**整体最迟 6 秒放弃**（`dns.rs:18-22`）——**这个 6s 上限是首连失败时的关键时序常数**。

### G12. `TransportAddr` match 编译不过

**根因**：`#[non_exhaustive]`（`iroh-base/src/endpoint_addr.rs:54-62`），**必须带兜底分支**（iroh 自己在 `endpoint_info.rs:493` 就写了 `_ => {}`）。

---

## H. Feature / 跨平台

### H1. `default-features = false` 后 `presets::N0` 不存在了

**根因**：**连带关掉 `tls-ring` → `with_crypto_provider` 为假 → `Minimal`(57/59) / `N0`(111/113) / `N0DisableRelay`(173/175) 三个类型整个从 API 消失。**

而 crypto provider 缺失是**运行时**错误——**代码照样编过，跑起来 `bind()` 才炸**。

**正确做法**：`Endpoint::builder(presets::Empty).crypto_provider(Arc::new(...))`。

### H2. 以为 wasm 必须关 default features

**根因**：**iroh CI 带 default features 直接编 wasm32，没有 `--no-default-features`**（`.github/workflows/ci.yml` 的 `wasm_test` job）。「metrics 编不过 wasm」在 0.34.0 就修好了（`CHANGELOG.md:885`，#3206）。

**机制是 target-gated 依赖，不是 feature 裁剪**：`portmapper` 这个 dep 本身就声明在 `not(wasm)` 的 target 表里，所以 feature 的 `dep:portmapper` 在 wasm 上是空转。

**在 wasm 上开着 portmapper / fast-apple-datapath 是无害的，不需要为 wasm 单独维护 feature 列表。**

**但 rustflag 得自己补**：`--cfg getrandom_backend="wasm_js"`（`.cargo/config.toml` 或 `RUSTFLAGS`）——**是 rustflag 不是 feature，不会随 `cargo add iroh` 带过来**。

> ⚠️ **别说成「不加就编译失败」** —— **实测它不是编译硬门槛**：browser-blobs 根本没有 `.cargo/config.toml` 却 build 成功；browser-echo 用 `RUSTFLAGS=""` 清掉该 cfg 后同样成功。**但只验证了「能编过」，没验证运行时随机数是否正常** —— 保留该 cfg 是无害且更保险的（与 n0 CI 一致），密码学相关的东西别仅凭「编过了」就放行。详见 [06-wasm-browser.md](06-wasm-browser.md)。

### H3. macOS 上 ring 编不到 wasm32

**根因**：**本地 toolchain 问题，不是 iroh 约束。** Apple 系统 clang（Xcode CLT 自带）**不含 WebAssembly target/backend**，而 ring 的 build.rs 会把 `RING_SRCS` 里那批 .c 交给 cc crate 编到 wasm32。

**已独立验证**：`cargo tree --target wasm32-unknown-unknown -p iroh -i ring` 确认 ring v0.17.14 **进入 wasm32 依赖图**（经 noq-proto 与 rustls 两条路径）。而 **iroh CI 在 Linux 上带 tls-ring 编 wasm32 是通过的**——所以 ring→wasm 本身可行。

**修法是换编译器而不是关 feature**：

```bash
brew install llvm
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar
```

> ✅ **这条修法已实测有效**（macOS aarch64，2026-07-17）。触发链路是 `iroh` 的 `tls-ring` feature → ring 要编 C；报错信息指向 ring 而非 clang，**极易误判成「iroh 编不了 wasm」**。CI（ubuntu-latest）不受影响 —— 这正是它没被 n0 发现的原因，三个浏览器例子的 README 也只字未提 clang。
>
> ⚠️ **别说 ring build.rs「明确要求 clang-like」**：`ring-0.17.14/build.rs:594-603` 的语义是「**如果** `is_like_clang()` **才**追加 `-nostdlibinc`」，非 clang 只是不加标志，既不报错也不 gate（紧邻 TODO 还写着「Expand this to non-clang compilers」）。**wasm32 需要 LLVM/clang 是工具链事实，不来自这段 build.rs。**

**若改走 tls-aws-lc-rs 逃避，代价更大**：aws-lc-sys 的 C/汇编依赖更重。

> libp2p 默认也吃 ring（经 quinn/rustls），**同样的约束——不是 iroh 特有的坑**。

### H4. 两个 tls-* 都开了，aws-lc-rs 没生效

**根因**：**只在 preset 路径上 ring 优先**（`presets.rs:66-75` 的两个 `#[cfg]` 分支）。

⚠️ **但别说成「aws-lc-rs 白编一遍还不生效」，也别说「必须 `default-features = false`」——都是错的。** `Builder::crypto_provider` 是公开 setter 且在 preset 之后**覆盖**（`endpoint.rs:761` 直接赋值）。官方两个 example 就在 default features 全开下用 aws-lc-rs：

```rust
Endpoint::builder(presets::Empty).crypto_provider(pq)   // pq-only-key-exchange.rs
Endpoint::builder(presets::N0).crypto_provider(pq)      // prefer-pq-key-exchange.rs:42/73
```

`pq-only-key-exchange.rs` 头注释逐字：「With iroh's default features still on, both `ring` and `aws-lc-rs` get linked. **That's harmless** — we wire the aws-lc-rs provider in directly via `Builder::crypto_provider`.」

**ring 只是被一起链接（多编一份，二进制变大）。`default-features = false` 只在真想去掉 ring（体积/合规）时才必要。**

### H5. `wasm` ≠ `wasm_browser`

**根因**：`wasm_browser` 的判据含 `target_os = "unknown"`（`build.rs`），**只命中 `wasm32-unknown-unknown`**。`wasm32-wasip1`/`wasip2` **不走** wasm_browser 分支，会去走 `not(wasm_browser)` 的 IP transport 路径。

### H6. 浏览器里没有直连、没有打洞

**根因**：**IP transport 整个被编译掉**（`socket/transports.rs:30-33` `#[cfg(not(wasm_browser))] mod ip;`，`:52-63` 的字段门控，`:99-118` 的 `TransportConfig::Ip` 变体门控）。iroh 的打洞是 QUIC NAT-traversal + multipath（`quic.rs:537`），**依赖直连 UDP path**。

**佐证**：`Builder::empty()` 的 transports vec 在 wasm 下**是空的**——不配 relay 就完全没有 transport。

> ⚠️ **精确**：relay 是浏览器下唯一的**内建** transport；**custom transport 仍可注册**（`transports.rs:30`/`:57` 都没有 wasm_browser gate）。
>
> ⚠️ **libp2p 对照别写过头**：js-libp2p 的 browser-to-browser WebRTC **仍需 circuit-relay 做 SDP 信令**；WebTransport 浏览器**不能 listen**；而 **rust-libp2p 的 webrtc 是 browser→server 的 webrtc-direct，没有 browser-to-browser**。所以「留在 libp2p 就能给你 Web 直连」这个说法：对 js-libp2p 是半真（仍需 relay 做信令），**对 rust-libp2p 完全不成立**。

### H7. ⚠️ 常见误传：「pkarr / 发现机制在浏览器不可用」

**这是错的。** 被编译掉的**只有 `DnsAddressLookup`**（裸 DNS 查询）：

```rust
// presets.rs —— N0 preset
builder = builder.address_lookup(PkarrPublisher::n0_dns());     // ← 无 cfg
builder = builder.address_lookup(PkarrResolver::n0_dns());      // ← 无 cfg
// Additionally resolve using DNS queries outside browsers.
#[cfg(not(wasm_browser))]
{ builder = builder.address_lookup(DnsAddressLookup::n0_dns()); }   // ← 只有这个
```

pkarr 走 HTTPS 打 `https://dns.iroh.link/pkarr`（`pkarr.rs:127`），**浏览器完全可用**。

**决定性反证**：**browser-echo 既没有 ticket 也没有 MemoryLookup**（grep 零命中），靠**裸 32 字节 endpoint id** 就能 connect（`node.rs:163`）。**若发现机制真不可用，这个例子根本跑不起来。**

**确实没了的是**：`iroh::dns` 模块（`lib.rs:287` `#[cfg(not(wasm_browser))] pub use iroh_dns::dns;`）——**跨平台代码引用它会在 wasm 编译时报 unresolved**。浏览器下解析只剩 pkarr HTTPS 一条腿。

### H8. 浏览器里 home relay 选不出来

**根因**：**QAD 探测在 wasm 下不编译**（`net_report/probes.rs:20-33` 的 `QadIpv4`/`QadIpv6` 带 `#[cfg(not(wasm_browser))]`），**只剩 HTTPS 探测**——一次 `GET {relay}/ping` 计时（`RELAY_PROBE_PATH = "/ping"`）。

**会被 CORS 预检、浏览器连接池、Service Worker 干扰。自建 relay 若不给 `/ping` 返回正常响应，浏览器端 relay_latency 为空 → preferred_relay 永远 None → home relay 选不出来。**

### H9. wasm 下 `close()` 不等任务结束

**根因**：Runtime 任务管理全部退化（`runtime.rs`）：

```rust
/// No-op on wasm. There is no task tracker to close or wait on.
#[cfg(wasm_browser)] pub(crate) async fn shutdown(&self) {}
/// No-op on wasm. There is no task tracker or cancellation to perform.
#[cfg(wasm_browser)] pub(crate) fn abort(&self) {}
#[cfg(wasm_browser)] fn spawn(&self, future: ...) { wasm_bindgen_futures::spawn_local(future); }
```

**spawn 出去的 task 无法被 cancel（spawn_local 后就脱管）。别在 wasm 上依赖 shutdown 的时序语义做清理。**

**另注**：`spawn_local` 是**单线程**的——`tokio::sync::broadcast` / `async_channel` 能用，但**任何真正需要多线程的假设都不成立**。tokio 只能开 `features = ["sync"]`，full 不行。

### H10. `fast-apple-datapath` 的 App Store 风险被高估了

**事实链**：
1. **私有符号是运行时 `dlsym` 解析的，不是静态链接**（`noq-udp-1.0.1/src/unix.rs:653-661` 走 `libc::dlsym(libc::RTLD_DEFAULT, ...)`，`:626-631`/`:638-643` 用 `c"sendmsg_x"` / `c"recvmsg_x"` 字面量按名查找再 transmute）。**Mach-O 里没有 undefined import，只有字符串字面量**
2. **`set_apple_fast_path` 全链路无人调用**（grep `noq-1.0.1/src`、`noq-udp-1.0.1/src`、`iroh/src` 三处均无调用者，只有定义处 `:355` 和一处注释 `:1185`）→ `apple_fast_path` 恒为 `AtomicBool::new(false)`（`:199`）→ send/recv 在进入 `*_via_*msg_x` **之前**就分流到 `send_single`/`recv_single`——**dlsym 运行时根本不会被执行到**
3. 真要开是 `unsafe` 且 doc 明写「may crash on unsupported OS versions」

**准确表述：默认配置下这是「编进去的死代码 + 两个字符串常量」。静态扫描面是字符串匹配而非链接符号。**

**iOS 上仍可评估 `default-features = false` 去掉它，但理由是「去掉无用死代码 / 减小体积 / 消除字符串扫描面」，不是「规避已链接的私有 API」。** 记得补回 tls-ring（见 H1）。

**另注**：iroh 自身源码零实现（纯透传到 noq-udp），作用域只在 apple 系（`noq-udp/build.rs` 的 `apple_fast` alias）——**其他平台完全惰性，这就是为什么 Android CI 和 wasm CI 都懒得关它**。

### H11. iOS 无 CI 覆盖

**这是成熟度事实而非症状** —— `iroh/.github/workflows/ci.yml` grep 不到任何 iOS job（Android 有）。
正文 → [07-configuration.md](07-configuration.md) 的「iOS / Android：没有任何专属 feature」。

### H12. portmapper 在 wasm/未开 feature 时不报错，而是空壳

**根因**：**刻意的 API 稳定性设计**（`portmapper.rs`）——`PortmapperConfig` 无 cfg，跨平台代码可以无脑 `.portmapper_config(...)`。真实开关是 `all(not(wasm_browser), feature = "portmapper")` **双条件**。

> libp2p 的 `libp2p-upnp` 是独立 behaviour，不装就没有。iroh 是「**装了但可能是空壳**」。

**一个产品决策点**：`PortmapperConfig::Disabled` 的 doc 提到 **UPnP 的 SSDP multicast 会在 macOS 上弹防火墙授权对话框**。

---

## I. iroh-blobs / 传输

### I1. iroh-blobs 自述非生产质量，且退不回 0.35

**这是成熟度判定而非症状** → [index-ecosystem-map.md](index-ecosystem-map.md) 的 iroh-blobs 条目。

### I2. `ImportMode` 的默认是 `Copy` 不是 `TryReference`

**根因**：`proto.rs:629-644` 的 `#[default]` 标在 **`Copy`** 上（`:635-636` doc：「This mode will copy the file into the database before hashing. This is the **safe default**」）。

**sendme 是显式写 `mode: ImportMode::TryReference` 主动选入的**，且 `:642-643` 说明它**只是个 hint**：「Stores are allowed to **ignore this mode** and always copy the file」。

### I3. `EventMask::DEFAULT` 下 push 是 Disabled 不是 None

**根因**（安全相关，方向别搞反）：

```rust
// events.rs:189-198 —— :190 doc: All event notifications are fully disabled.
//                              Push requests are disabled by default.
DEFAULT = EventMask {
    connected: ConnectMode::None,
    get: RequestMode::None, get_many: RequestMode::None,
    push: RequestMode::Disabled,          // ← 不是 None！
    throttle: ThrottleMode::None, observe: ObserveMode::None,
}
```

`:202-204`：「If you want to enable push requests, **which can write to the local store**, you need to do it manually. Providing constants that have push enabled would risk misuse.」

**唯一能写入本地 store 的请求类型恰恰是默认拒绝的。**

### I4. 照抄 sendme 会得到一个谁都能拉走文件的节点

**根因**：**sendme 不鉴权是产品决定，不是能力缺失**——它的模型是「ticket 即凭据，谁拿到谁能下」。它只用了观测档位（`ConnectMode::Notify` + `RequestMode::NotifyLog`）。

**iroh-blobs 提供了拦截钩子**：`ConnectMode::Intercept` / `RequestMode::Intercept` / `InterceptLog` / `Disabled`。

**要做准入控制（只允许特定对端拉取）就必须换档位**：`Intercept` 可拒绝入站连接/单条请求；若既要拒绝、又要详细传输事件（进度 UI），用 `InterceptLog`。

**iroh 层面的分工写在 `iroh/iroh/src/lib.rs:81-94`**：

> When accepting connections the peer's [`EndpointId`] is authenticated. **However it is up to the application to decide if a particular peer is allowed to connect or not.**

**E2E 加密 ≠ 授权。**

### I5. FsStore 不是「每个 blob 四个文件」

**根因**：**四类存储位置按大小与 ImportMode 择一**：

| 位置 | 条件 |
|------|------|
| **Inline** | data/outboard **< 16KiB** 时内联进 redb（`fs/options.rs:71-78`；`fs.rs:22-23` 模块文档：「as well as **inlined data and outboard data for small files**」）|
| **External** | `ImportMode::TryReference` → `DataLocation::External(vec![path], size)`（`fs.rs:1042-1044`）——**store 里根本没有 `<hash>.data`**（sendme 发送端正是这个模式）|
| **NotNeeded** | 单 chunk group 的小 blob 无 outboard（`fs.rs:1065`）|
| **Owned** | 完整 sidecar 文件组 |

### I6. bitfield 与数据不是单一事务

**根因**：`write_batch`（`fs.rs:318-334`）是**两步**：`self.state.send_if_modified(...)` 改内存/文件状态，再 `self.global.db.update(...)` 更新 redb。

`fs.rs:50-57` 模块文档**明确承认不一致窗口**：「It is **not guaranteed that all write operations will make it to disk**. The on-disk store will be in a consistent state, but might miss some writes in the last seconds before shutdown」。

**准确说法：iroh 不是靠事务消灭不一致，而是靠内容寻址让 bitfield 可从 data+hash 重算，代价是下次启动的重校验延迟。**

### I7. 浏览器下 `import_path` 直接报错

**根因**（比「降级为拷贝」更狠）：

```rust
// iroh-blobs-0.103.0/src/store/mem.rs:758-764
#[cfg(wasm_browser)]
async fn import_path(cmd: ImportPathMsg) -> Result<ImportEntry> {
    Err(n0_error::anyerr!("import_path is not supported in the browser"))
}
```

**浏览器下只能走 `import_bytes`（`mem.rs:755`）全量入内存。**

### I8. 浏览器收不了大文件

**根因**：取回路径是**整块拷贝 + 两层放大**——`get_bytes` 先把整个 blob 读进 wasm 线性内存，`bytes_to_uint8array` 再 copy 一份到 JS 堆，**峰值 = 文件大小 × 2**。官方对此的处理是 **Rust 端和 JS 端都硬编码「1MiB 以上干脆不读」**，没有提供流式出口。

⚠️ **别写「2GB 直接 OOM」**——源码只说「受可用内存限制」（`mem.rs:1-9`），**没给任何数字**；wasm32 架构硬上限是 **4GiB**（非 2GB）。真要给数字必须自己压测。

### I9. 想自定义 store 后端

**这是能力边界而非症状**：0.103 里**没有可插拔的 store 后端 trait**，`Store` 是 struct（`api.rs:213`）。
→ [index-ecosystem-map.md](index-ecosystem-map.md) 的「不存在」清单 + [03c-blobs.md](03c-blobs.md)。

### I10. 「iroh-blobs 支持增量同步」有歧义

**这是能力边界而非症状**：它的「增量」= 同一 hash 未下完的部分补齐（bitfield 集合差），
**不是 rsync 式跨版本 delta** —— 内容寻址下文件一改 hash 全变。→ [03c-blobs.md](03c-blobs.md)。

### I11. fs store 在空闲驱逐时 panic（issue #233，未修）

**症状**：桌面端长时间运行 + 大量 blob handle 后，store worker task panic：`poisoned storage should not be used`。

**根因**（三步链条）：
1. `bao_file.rs:519-521` `pub fn take(&mut self) -> Self { std::mem::replace(self, BaoFileStorage::Poisoned) }` —— take 必然留下 Poisoned
2. `fs.rs:991-1008` `async fn persist(&self)` 的 `send_if_modified` 闭包**两条分支都不还原 guard**
3. `bao_file.rs:409-411` `BaoFileStorage::Poisoned => { panic!("poisoned storage should not be used") }`

唯一调用方是 `fs.rs:230-236` `EmParams::on_shutdown` → `state.persist().await`，而 `ShutdownCause::Idle`（*"the entity is idle"*）意味着**进程存活期间的空闲驱逐也会走这条路**。

> ⚠️ **精确化**：Poisoned **不是永久状态** —— entity manager recycle 时会 `Reset` 回 `Initial`（`bao_file.rs:532-535`）。它是 persist() 与 reset() 之间的**竞态窗口**。**危害仍在**：`Initial` 同样 panic，且 #233 的生产 trace 证明该窗口可达。

**这是 README「非生产质量」最具体的注脚。**

### I12. `FsStore::load` 会自建一个 multi_thread tokio runtime

**根因**（`fs.rs:1398-1409`）：

```rust
let rt = tokio::runtime::Builder::new_multi_thread()
    .thread_name_fn(|| format!("iroh-blob-store-{}", THREAD_NR.fetch_add(1, Ordering::Relaxed)))
    .enable_time().build()?;
```

**两个后果**：
1. 进程里多一组 `iroh-blob-store-*` 线程，非你所控
2. **关闭路径用 `tokio::task::block_in_place`**（`fs.rs:833-844` 的 `impl Drop for RtWrapper`）—— **它在 current_thread runtime 上会 panic**。若某处在单线程 runtime 里 drop 掉 store 就会炸

显式关停 API：`Store::shutdown()`(`api.rs:276`) / `wait_idle()`(`api.rs:293`)。

**这条在「能不能塞进现有架构」上比依赖体积重要得多。**

---

## J. 高层协议 / 地基

### J1. 拿 gossip 的 NeighborUp/Down 当「设备在线/离线」

**根因**：`src/api.rs:336-345` 的限定词是 **membership layer 的 direct neighbor**。而 `src/proto.rs:23-24`：*"If one of your active peers goes offline, its slot is filled with a **random** peer from the passive set."* + 默认 `shuffle_interval = 60s` 自发轮换。

**即：active view 是会自发轮换的部分视图，NeighborDown 可能只是视图轮换，不是存活判定。** 即使成员数很小时经验上接近，也是依赖实现细节的巧合，不是协议保证。

**正确做法**：「设备 X 现在在不在线」这种**定向、确定**的存活判定要自己做（对已知对端做定向探测 + 宽限期消抖），gossip 给的是「某个随机邻居进/出了我的部分视图」。

### J2. 以为 gossip 消息带作者身份

**根因**：`src/api.rs:361-372` 的 Message 只有 `content` / `scope` / `delivered_from` 三个字段，`:370` 注释明写 *"The endpoint that delivered the message. **This is not the same as the original author.**"* —— **无 author、无 signature 字段**。协议层零签名逻辑。

**任何知道 32 字节 TopicId 的人都能伪造消息。** 官方 `examples/chat.rs` 自己定义 `struct SignedMessage { from, data, signature }` 并手工 `verify_and_decode`。

> ⚠️ **措辞精确性**：说 gossip「不加密」是**失准**的 —— 它跑在 iroh Connection 上，**每一跳都是 QUIC/TLS 加密且端点已鉴权**。准确表述：**gossip 不提供跨多跳转发的端到端机密性与作者鉴权** —— 逐跳加密，但中间转发者可见明文。

### J3–J6. 高层协议的选型判断

以下四条**不是症状，是「该不该引入」的选型判断**，正文已收口到别处：

| | 结论 | 正文 |
|---|---|---|
| **J3** 只想要 blobs 却被 iroh-docs 拖进全栈 | `Builder::spawn` 强制 `(endpoint, blobs, gossip)` 三个参数，Router 须注册三个 ALPN；默认 feature 还会同时编入 redb 4.1 与 3.1 两个大版本 | [03d-docs-rpc-automerge.md](03d-docs-rpc-automerge.md) |
| **J4** 把 irpc 当 bulk data plane | 16 MiB 硬上限且无覆盖机制；n0 的分界线是「bulk 手写 ProtocolHandler，control/progress 才上 irpc」 | [03d-docs-rpc-automerge.md](03d-docs-rpc-automerge.md) |
| **J5** 以为 irpc 能接 Tauri IPC / uniffi | Non-goals 第一条逐字写着 "Cross language interop. This is for talking from rust to rust" —— **按设计就接不了** | [03d-docs-rpc-automerge.md](03d-docs-rpc-automerge.md) |
| **J6** 用了 quic-rpc | **abandoned**：绑死 iroh 0.35、外部依赖数 0、14 个月空窗 | [index-ecosystem-map.md](index-ecosystem-map.md) |

### J7. n0-error 的 location 在生产里永远是 None

**这是地基库的迁移成本判断而非症状** —— `backtrace_enabled()` 只认 `RUST_BACKTRACE=1|full` 或
`RUST_ERROR_LOCATION=1` 且用 OnceLock 缓存；打包分发的应用不会带这些 env。**且不必迁**。
→ [index-foundations.md](index-foundations.md)。

---

## K. Ticket / 编码

### K1. `TicketWireFormat::Variant1` 的 wire 判别符是 0x00，不是 0x01

`endpoint.rs:36-38`：

```rust
enum TicketWireFormat { Variant1(Variant1EndpointTicket) }
```

单变体 enum，postcard 按**位置**编号，故判别符 = 0。其自身测试向量（`endpoint.rs:203-207`）印证：`// variant` 对应 `"00"`。

对照 `iroh-blobs/src/ticket.rs:40-42` `enum TicketWireFormat { Variant0(Variant0BlobTicket) }`，测试向量（:227）写 `00 # discriminator for variant 0` —— **两者名字一个叫 Variant1 一个叫 Variant0，wire 上却都是 0x00**。

**照抄这个「单变体 enum」模式是对的**（它就是为留版本位而存在的，`iroh-blobs/src/ticket.rs:35-38` 注释：*"In the future we might have multiple variants (not versions, since they might be both equally valid), so this is a single variant enum to force postcard to add a discriminator"*），但务必**按位置而非名字理解判别符**：新增变体时它拿到的是 0x01。建议命名直接用 V0/V1 并配注释写明 wire 值。

### K2. wire format 跨版本断过 —— 连官方 README 都没跟上

`dumbpipe/README.md:47` 里印的真实 ticket（100 字符）用**当前 iroh-tickets 1.0.0 解不出来**：base32 解出 57 字节，但 postcard 反序列化失败（`Serde Deserialization Error`）。原因是首字节 = `0x20`（十进制 32，像是旧格式的 32 字节长度前缀），而 1.0.0 期望的判别符是 `0x00`。该 README 与 `dumbpipe/Cargo.toml:20` 声明的 `iroh-tickets = "1.0.0"` 不一致。

**两个教训**：
1. **ticket 字符串不是永久稳定的** —— n0 自己在 1.0 前就破坏过格式。若要「发出去的旧链接以后还能用」，必须自己扛版本兼容（单变体 enum 留位 + 老变体永不删）
2. **不要相信 iroh 文档里的示例 ticket 能跑** —— 以源码测试向量为准

### K3. 读 browser-chat 源码时被注释掉的死代码骗

`iroh-examples/browser-chat/browser-wasm/src/lib.rs:54` 的 `// let ticket = ChatTicket::new(topic);` 是注释掉的死代码；但 **:134 是 LIVE 代码**（`let mut ticket = ChatTicket::new(self.topic_id);`），真正的残留注释在 **:94**（`// ticket.bootstrap = [self.0.endpoint_id()]...`）。
