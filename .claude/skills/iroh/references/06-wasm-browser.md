# wasm / 浏览器：能跑，但没有直连

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

## 一句话结论：网络层不是卡点，存储层才是

浏览器里 iroh **真能用**：QUIC + E2E 加密不打折，既能主动连也能被动 accept，**pkarr 寻址仍在**。代价是**永远走 relay**（IP transport 在 wasm 下整个 `mod ip` 都不编译）+ 约 800~900KB brotli 包体。

真正的卡点是 **iroh-blobs 在浏览器只有纯内存 MemStore**（全仓 0 处 OPFS/IndexedDB/File System Access）。所以：**小文件/文本/剪贴板同步现在就能做；要收大文件，必须自己实现一个 OPFS 后端的 store，或接受「Web 端只发不收大文件」。**

---

# 第一部分：三个真正的浏览器样例（不是四个）

| 样例 | 内容 | 在 CI wasm 列表？ | 该抄什么 |
|---|---|---|---|
| **browser-echo** | iroh + 自定义 ALPN 的 echo 协议 | ✅ | **三层拆分模板**（共享 core / wasm 壳 / CLI 壳）+ `into_js_readable_stream` 那 6 行 |
| **browser-chat** | iroh + iroh-gossip + React + Vite | ✅ | **wasm-pack + Vite 集成路径** + React 侧消费事件流 |
| **browser-blobs** | iroh + iroh-blobs 最小 demo | ❌ 只在 deploy 时构建 | 只取 API 用法。它最大的信息量是**反向的**：划出了 iroh-blobs 在浏览器的天花板 |

> ⚠️ **`dumbpipe-web` 不是浏览器/wasm 例子** —— 把它算进来会得出错误结论。
>
> 四路证据：① `Cargo.toml` 的 package name 是 **`reverse-proxy`**（与目录名不符）；② 依赖 hyper 1.0 + hyper-util + `tokio { features = ["full"] }`（wasm 下不可能）+ dumbpipe 0.39，**零 wasm-bindgen 系依赖**；③ 目录里只有 `src/main.rs` 和 `src/quinn_endpoint.rs`，没有 public/、pkg/、package.json、.cargo/config.toml；④ `Makefile.toml:7-11` 的 `[tasks.deploy]` 与 `deploy.yml:7-15` 的 paths **只列 browser-echo/chat/blobs**，dumbpipe-web 只在 `ci.yml:18` 的 `RS_EXAMPLES_LIST`（原生构建）里。
>
> README 首句：*"This forwards http requests to dumbpipe"* —— **名字里的 web 指「给 dumbpipe 加个 HTTP 前端」，不是「跑在 web 里」**。它真正可复用的点（`quinn_endpoint.rs` 的 AsyncRead/AsyncWrite 缝合）见 [03c-blobs.md](03c-blobs.md)。


> **成熟度判定（三个样例哪个能抄、哪个别当依赖）与「自己写浏览器 store 的两道墙」（issue #84/#86/#207）**
> → [index-ecosystem-map.md](index-ecosystem-map.md)。


## 共享架构范式

**平台中立核心 + wasm 薄壳 + 复用同一核心的 CLI，且浏览器端与 CLI 端跑同一 ALPN 协议、可互连。**

```
// browser-echo/README.md
* The shared code between WASM and CLI lives in [`src/node.rs`](src/node.rs).
* A WASM-only wrapper around the `EchoNode` defined there lives in [`src/wasm.rs`](src/wasm.rs).
  It mostly is concerned with converting types to browser-supported equivalents.
* The CLI binary lives in [`src/bin/cli.rs`](src/bin/cli.rs)

// browser-chat/README.md
* [**shared**](shared) is a Rust library that exposes a `ChatNode`
* [**cli**](cli) uses that library
* [**browser-wasm**](browser-wasm) is a wrapper around the shared library that uses wasm-bindgen
* [**frontend**](frontend) is the web app.
```

**关键：「shared 核心」的边界不是靠 cfg 划的，而是靠 crate/模块划的**——`node.rs` / shared crate 里**一行 wasm cfg 都没有**（三个共享核心 grep `cfg(` 计数均为 0），全部平台差异被 n0-future 和 iroh 自身吸收掉了。

> **libp2p 对照**：libp2p 的 wasm 适配要按 target 换 transport 栈（`SwarmBuilder` 的 `with_tokio`/`with_wasm_bindgen` + transport 组合——**注意是在 transport 层，不是 behaviour 层**）。iroh 这里核心代码对 wasm **完全无感知**，`Endpoint::builder(presets::N0)` 在两端是同一行。

### browser-chat 的仓库结构（别数错）

`browser-chat/Cargo.toml`：`members = ["shared", "cli", "browser-wasm"]`、`default-members = ["cli"]` —— **3 个 Rust crate**。

`frontend` **不是 workspace member**，是独立 JS 包（`frontend/package.json`，靠 `"chat-browser": "file:../browser-wasm/pkg"` 消费 wasm 产物）。**Rust 侧和 JS 侧是两套包管理边界。**


# 第二部分：网络层 —— 能用，但永远 relay-only

## ⚠️ IP transport 是**编译期**不存在，不是运行时禁用

```rust
// iroh/iroh/build.rs
wasm_browser: { all(target_family = "wasm", target_os = "unknown") },

// iroh/iroh/src/socket/transports.rs:30-33
pub(crate) mod custom;
#[cfg(not(wasm_browser))]
mod ip;                             // ← 整个模块不编译
mod relay;

// transports.rs:52-63
pub(crate) struct Transports {
    #[cfg(not(wasm_browser))]
    ip: IpTransports,               // ← 浏览器下这个字段根本不存在
    relay: Vec<RelayTransport>,
    custom: Vec<Box<dyn CustomEndpoint>>,   // ← custom 在浏览器仍然在！
    ...
}

// transports.rs:99-118 —— TransportConfig::Ip 变体本身也被 gate
pub(crate) enum TransportConfig {
    #[cfg(not(wasm_browser))]
    Ip { config: ip::Config, is_user_defined: bool },
    Relay { relay_map: RelayMap, is_user_defined: bool },
    Custom(Arc<dyn CustomTransport>),
}
```

（`transports.rs` 里 `wasm_browser` 共出现 38 次。另有专门的 `#[cfg(wasm_browser)] local_addrs_watch()`（:403-416）只 join custom + relay 两路。）

**佐证**：`Builder::empty()` 的 `transports` vec（`endpoint.rs:191-197`）在 wasm 下**是空的**——不配 relay 就完全没有 transport。

**iroh 的打洞是 QUIC NAT-traversal + multipath**（`endpoint/quic.rs:537` `max_remote_nat_traversal_addresses`），**依赖直连 UDP path；IP transport 不存在时无从谈起**。

**浏览器下砍掉的**：IP transport（无 UDP → **无直连、无打洞**）、DNS 解析、portmapper、QAD 探测、本机网卡枚举。

证据：
- `iroh/iroh/Cargo.toml` 的 `[target.'cfg(not(all(target_family = "wasm", target_os = "unknown")))'.dependencies]` 段**独占** hickory-resolver（DNS）和 portmapper（UPnP/PCP/NAT-PMP）
- `iroh/iroh/src/net_report.rs:43-44` `#[cfg(not(wasm_browser))] use self::reportgen::{QadProbeReport, SocketState};`
- `iroh/iroh/src/socket.rs:39-43` `#[cfg(not(wasm_browser))] use netwatch::{...};`
- `iroh/iroh/src/endpoint.rs:141-142` `#[cfg(not(wasm_browser))] dns_resolver: Option<DnsResolver>,`

> ⚠️ **精确表述**：relay **不是**浏览器下唯一的 transport——`pub(crate) mod custom;`（`transports.rs:30`）和 `custom: Vec<Box<dyn CustomEndpoint>>`（`:57`）都**没有** wasm_browser gate（`socket/transports/custom.rs` 里 wasm_browser 出现 0 次）。准确说法是「**relay 是浏览器下唯一的内建 transport；custom transport（feature `unstable-custom-transports`）在浏览器仍可注册**」。自己实现 WebRTC/WebTransport 理论可行，是 relay-only 的**唯一逃生口** —— 高成本高回报的可选项，不是现成能力。

> ⚠️ **「无 mDNS」的归因要说清楚**：mDNS **根本不在 iroh crate 里**，跟 `#[cfg(not(wasm_browser))] mod ip;` 无关（`address_lookup.rs:46-50` 明写它在独立 crate 里）。所以「Web 端没有 mDNS」成立（浏览器无 UDP 多播），但**它不是 wasm gate 砍掉的，而是它在任何平台上都得额外引 crate**。

> **libp2p 对照（这条容易写过头）**：
> - **js-libp2p** 有 `@libp2p/webrtc` 可做 browser-to-browser 直连，但**仍需 circuit-relay 做 SDP 信令**
> - **WebTransport 在浏览器不能 listen**，物理上做不了 browser↔browser 直连
> - **rust-libp2p 的 webrtc 是 browser→server 的 webrtc-direct，没有 browser-to-browser**
>
> **所以「Web 端 iroh 中继成本无法回避」在 iroh 侧成立；但若你的栈是 rust-libp2p，「留在 libp2p 就能给 Web 直连」同样不成立。**

## ⚠️ 常见误传：pkarr 在浏览器里是可用的

**被编译掉的只有 `DnsAddressLookup`（裸 DNS 查询），不是整个发现机制。**

```rust
// iroh/iroh/src/endpoint/presets.rs:116-139 —— N0 preset 无条件加 pkarr 两件
builder = Minimal.apply(builder);                            // 设 crypto_provider
builder = builder.address_lookup(PkarrPublisher::n0_dns());  // ← 无 cfg，浏览器可用
// Resolve using HTTPS requests to our DNS server's /pkarr path.
builder = builder.address_lookup(PkarrResolver::n0_dns());   // ← 无 cfg，浏览器可用
// Additionally resolve using DNS queries outside browsers.
#[cfg(not(wasm_browser))]
{ builder = builder.address_lookup(DnsAddressLookup::n0_dns()); }   // ← 只有这个被 gate
builder.relay_mode(default_relay_mode())
```

pkarr 走 HTTPS 打 `N0_DNS_PKARR_RELAY_PROD = "https://dns.iroh.link/pkarr"`（`pkarr.rs:127`），**浏览器完全可用**。`address_lookup.rs:120-127` `#[cfg(not(wasm_browser))] pub mod dns;` 而 `pub mod memory; pub mod pkarr;` **无 gate**。

**决定性反证**：**browser-echo 既没有 ticket 也没有 MemoryLookup**（grep `address_lookup|MemoryLookup|Ticket` 对 `browser-echo/src/` **零命中**），它在浏览器里靠一个**裸 32 字节 endpoint id** 就能 connect（`node.rs:163` `endpoint.connect(endpoint_id, Echo::ALPN)`，main.js 从表单读 endpoint-id）。**若浏览器端发现机制真的不可用，browser-echo 这个例子根本跑不起来。**

**「浏览器没有 discovery」是错的直觉。** Web 端的节点寻址不用另起炉灶，pkarr over HTTPS 直接可用，跟桌面/移动端同一套 EndpointId 体系。

### 但 DNS resolver 确实没了

```rust
// iroh/iroh/src/lib.rs:287
#[cfg(not(wasm_browser))] pub use iroh_dns::dns;      // ← 浏览器里 iroh::dns 模块不存在
// endpoint.rs 对应
#[cfg(wasm_browser)]      use crate::address_lookup::PkarrResolver;
#[cfg(not(wasm_browser))] use crate::dns::DnsResolver;
```

**写跨平台代码时引用 `iroh::dns` 会在 wasm 编译时报 unresolved。** `util.rs` 的 `reqwest_client_builder` 也有两个签名：非 wasm 收 `(tls_client_config, dns_resolver)`，wasm 版**不收参数**。

**浏览器下 N0 preset 少一层 DNS 冗余，解析只剩 pkarr HTTPS 一条腿。**

### ⚠️ 另一条常见误传：「自定义 relay 协议在浏览器不可用」

**`RelayProtoNotAvailable`（`iroh-relay/src/client.rs:101`，"The relay protocol is not available in browsers"）是死代码**——`grep -rn RelayProtoNotAvailable` 扫全 workspace **只有这一处声明，从未被构造**。

**1.0.2 里根本不存在「自定义 relay 协议 vs WebSocket」的分叉**：

| target | 实现 |
|--------|------|
| native | `tokio_websockets`（`client.rs:302` `ClientBuilder::new()`）|
| browser | `ws_stream_wasm`（`client.rs:415` `WsMeta::connect`）|

**两端都是 WebSocket**，且协商同一个 `ProtocolVersion`（`http.rs:50`，只有 V1/V2，作为 WS subprotocol）。全库 grep `enum Protocol\b|Protocol::Relay|Protocol::Websocket` 零命中。该变体是 pre-1.0 iroh 曾有 HTTP-upgrade 自定义 relay transport 的残留。

## relay 走 WebSocket，auth token 降级成 URL query

`iroh/iroh-relay/Cargo.toml:102-103` wasm-in-browser 段：`ws_stream_wasm = { version = "0.7.4", default-features = false }`（非 wasm 段用 `tokio-websockets = "0.13"`）。

```rust
// iroh-relay/src/client.rs:386-420 —— #[cfg(wasm_browser)] pub async fn connect(&self)
if let Some(token) = self.auth_token.as_ref() {
    dial_url.query_pairs_mut().append_pair(AUTH_TOKEN_URL_QUERY_PARAM, token);
}
ws_stream_wasm::WsMeta::connect(dial_url.as_str(), Some(ProtocolVersion::all().collect())).await
```

对比原生路径（:320-327）：`builder.add_header(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}"))...)`。原生还多一层 `KeyMaterialClientAuth::new(&self.secret_key, &stream)`（**:328**，TLS key export 认证），**浏览器没有**（会自动回退到 challenge-response）。

**两个后果**：
1. 浏览器 WebSocket API **不能设自定义 header**（web 平台硬限制），所以 token 只能进 URL → 会出现在 relay 的 access log / 浏览器 history 里，**需要短时效 token**
2. 自建 relay 必须支持 **wss 且证书要被浏览器信任** —— **裸 HTTP relay 这套在浏览器行不通**（https 页面不能连 `ws://`，mixed-content 会被拦截）。**Web 端一上马，明文 relay 方案直接出局**

## 浏览器下 home relay 的选路退化

```rust
// iroh/iroh/src/net_report/probes.rs:20-33
pub enum Probe {
    Https,
    #[cfg(not(wasm_browser))] QadIpv4,     // QUIC Address Discovery
    #[cfg(not(wasm_browser))] QadIpv6,
}

// iroh/iroh/src/net_report/reportgen.rs:815-864
async fn run_https_probe(...) -> Result<HttpsProbeReport, MeasureHttpsLatencyError> {
    let url = relay.join(RELAY_PROBE_PATH)?;          // iroh-relay/src/http.rs:14-15: RELAY_PROBE_PATH = "/ping"
    let start = Instant::now();
    let response = client.request(reqwest::Method::GET, url).send().await...;
    let latency = start.elapsed();
}
```

`net_report/options.rs` 的 wasm 分支 doc：「Options for running probes (in browsers). **Only HTTPS probes are supported in browsers.**」

连带 `net_report/report.rs` 的 `RelayLatencies` 在 wasm 下只有 `https: BTreeMap<RelayUrl, Duration>`，`ipv4`/`ipv6` 字段消失；`reportgen.rs` 的 `SocketState`（装 QuicClient + DnsResolver）整个 `#[cfg(not(wasm_browser))]`，注释写「Factored out so it can be disabled easily in browsers.」

⚠️ **浏览器里 home relay 完全由一次 HTTP `GET /ping` 的往返时间决定**——会被 CORS 预检、浏览器连接池、Service Worker 干扰。**自建 relay 若不给 `/ping` 返回正常响应，浏览器端 relay_latency 为空 → preferred_relay 永远 None → home relay 选不出来。**

> libp2p 的 autonat/identify 观测外部地址；iroh 用 QAD（QUIC 层拿对端观测到的地址）+ relay。浏览器里 QAD 没了，**外部地址只能靠 relay 视角**。

## wasm 下的其它退化

### watch_addr 永远没有 direct address

```rust
// iroh/iroh/src/endpoint.rs:1286-1306
/// When compiled to Wasm, this function returns a watcher that initializes
/// with an [`EndpointAddr`] that only contains a relay URL, but no direct addresses,
/// as there are no APIs for directly using sockets in browsers.
#[cfg(wasm_browser)]
pub fn watch_addr(&self) -> impl n0_watcher::Watcher<Value = EndpointAddr> + use<> {
    let watch_relay = self.inner.home_relay();
    let endpoint_id = self.id();
    watch_relay.map(move |mut relays| {
        EndpointAddr::from_parts(endpoint_id, relays.into_iter().map(TransportAddr::Relay))
    })
}
// 对比 native 版 :1269-1284 会 chain 上 ip_addrs()
```


### wasm + `RelayMode::Disabled` 的真实失效模式

⚠️ **常见误传**：「浏览器里 `watch_addr()` 会一直 pending 直到 home relay 选出来；Disabled + wasm 会静默挂死」——**这是错的**。

`watch_addr()` 返回 `impl Watcher<Value = EndpointAddr>`，而 `initialized()` 有 `W: Nullable<T>` 约束，**`EndpointAddr` 不实现 `Nullable`，所以在 `watch_addr()` 的返回值上根本调不到 `initialized()`**，谈不上「永远 pending」。

**真实失效模式是「静默产出空地址」**：wasm 版 `watch_addr()`（`endpoint.rs:1297`）只 map `home_relay()`，而 `home_relay()`（`socket.rs:488`）是 `local_addrs_watch.map(filter_map(Addr::Relay))`——Disabled 下过滤出空 Vec，`get()` 立刻返回一个 `EndpointAddr::from_parts(endpoint_id, [])`，即**零地址的 EndpointAddr**。不报错、不 pending。

**危害其实更隐蔽**：你拿到的是个看起来合法、实际没人能拨通的 EndpointAddr，把它塞进 ticket 分发出去也不会有任何报错。

**真正会「等」的是 `Endpoint::online()`**，Disabled 下确实永不返回——但那是 `online()` 的语义，不是 `watch_addr()`。


### Runtime 任务管理全部退化

```rust
// iroh/iroh/src/runtime.rs
/// No-op on wasm. There is no task tracker to close or wait on.
#[cfg(wasm_browser)] pub(crate) async fn shutdown(&self) {}
/// No-op on wasm. There is no task tracker or cancellation to perform.
#[cfg(wasm_browser)] pub(crate) fn abort(&self) {}

impl noq::Runtime for Runtime {
    #[cfg(wasm_browser)]
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        wasm_bindgen_futures::spawn_local(future);
    }
```

**浏览器里 `endpoint.close()` 这类优雅关闭不会真的等任务结束，spawn 出去的 task 也无法被 cancel（spawn_local 后就脱管）。别在 wasm 上依赖 shutdown 的时序语义做清理。**

⚠️ **浏览器端 `spawn_local` 是单线程的**：`tokio::sync` 原语能用（纯用户态，wasm 安全），但**任何真正需要多线程的假设都不成立**。注意 shared 里仍直接依赖 `tokio = { default-features = false, features = ["sync"] }`——**tokio 的 sync 原语是 wasm 安全的，full 不是**。

## ✅ 好消息：iroh-gossip 零 wasm 适配代码就能跑

对 `iroh-gossip/src/` grep `wasm_browser|target_family = "wasm"` → **0 命中**；对 `iroh-gossip/Cargo.toml` grep `wasm` → **0 命中**（**没有任何 wasm target-specific dependencies 段**，与 iroh / iroh-blobs 形成对比）。全仓 grep `tokio::net|std::fs|std::net::TcpListener` 只命中 `src/bin/sim.rs`（一个 CLI 模拟器 bin，不是 lib）。

browser-chat 就是用 `iroh-gossip = { version = "0.101", default-features = false, features = ["net"] }` 直接编进 wasm。

**这说明**：**基于 iroh Endpoint 的上层协议默认就是 wasm 兼容的**，只要不碰 `std::fs` / `tokio::net`。**真正需要 cfg 分支的是有平台副作用的部分 —— 数据库、文件读写、keychain。**

> 这也是对「上层协议要不要为 Web 重写」的最乐观答案：**若你的协议/业务层只依赖 iroh Endpoint + n0-future，理论上不用为 wasm 改一行**（官方三个浏览器例子的 shared 核心里 `cfg(` 计数均为 0）。


---

# 第三部分：存储层 —— 这才是卡点

## iroh-blobs 在浏览器只有 MemStore

`browser-blobs/src/node.rs:25` `let store = iroh_blobs::store::mem::MemStore::default();`
README：*"For now, only the in-memory store works in the browser, so there is no persistence."*

对 iroh-blobs 全仓 grep `opfs|indexeddb|idb|origin.?private|FileSystemDirectoryHandle|localStorage`（`--include='*.rs' --include='*.toml' --include='*.md'`）→ **0 命中**。

`iroh-blobs/src/store/mod.rs:8` `#[cfg(feature = "fs-store")] pub mod fs;` —— 可选 store 只有 fs / mem / readonly_mem；`browser-blobs/Cargo.toml:21` 用 `default-features = false` 关掉 fs-store，**浏览器只剩 mem**。

wasm 下 `import_path` 直接返回错误（`store/mem.rs:758-764`，带 `#[cfg(wasm_browser)]`）：*"import_path is not supported in the browser"* —— **这是「浏览器没有文件系统」最直接的一手证据**。

## 取回 blob：整块拷贝 + 硬编码 1MiB 阈值

```javascript
// iroh-examples/browser-blobs/public/main.js
const size = await blobs.complete_size(hash);
if (size < 1024 * 1024) {              // ← 1MiB 以上干脆不读
  const data = await blobs.get(hash);
  const text = new TextDecoder().decode(data);
}
```

```rust
// browser-blobs/src/wasm.rs
pub async fn get(&self, hash: String) -> Result<Uint8Array, JsError> {
    let bytes = self.0.blobs.get_bytes(hash).await?;   // ① 整个 blob 读进 wasm 线性内存
    Ok(bytes_to_uint8array(&bytes))
}
pub fn bytes_to_uint8array(bytes: &[u8]) -> Uint8Array {
    let array = Uint8Array::new_with_length(bytes.len() as u32);
    array.copy_from(bytes);                            // ② 再 copy 一份到 JS 堆
    array
}
// src/bin/cli.rs 同样的守卫：if size < 1024 * 1024 { ... }
```

**两层放大：峰值 = 文件大小 × 2**（**加上 MemStore 里那份，一个文件在内存里至少两三份**）。**官方对此的处理是 Rust 端和 JS 端都硬编码「1MiB 以上干脆不读」，没有提供流式出口。**

**这是「Web 端能不能收大文件」的决定性否定证据。**

> ⚠️ **别写「2GB 直接 OOM」** —— 源码只说「受可用内存限制」（`mem.rs:1-9`），**没给任何数字**；`gh search issues --repo n0-computer/iroh-blobs "2GB OR OOM OR 'out of memory'"` 返回**空**；`gh issue list --search "wasm OR browser OR OOM OR memory" --state all` 命中的 6 个 issue（#90/#84/#207/#203/#67/#233）无一提及。而 wasm32 的**架构硬上限是 4GiB**（`2^16 pages × 64KiB`，非 2GB）。**保留机制描述，删掉数字。真要给数字必须自己压测。**


对照 `:767` 的 `#[cfg(not(wasm_browser))]` 真实现。**浏览器下只能走 `import_bytes`（`mem.rs:755`）全量入内存。**

> ⚠️ **常见误传纠正 1**：「必须关 default features，否则 **redb** 编不过」——**redb 编 wasm 完全没问题**。实测：新建 crate 仅依赖 redb 4.1.0，`cargo check --target wasm32-unknown-unknown` → Finished，零报错。机理是 redb 自带 fallback 后端（`src/tree_store/page_store/file_backend/mod.rs`：`#[cfg(not(any(windows, unix, target_os = "wasi")))] pub use fallback::FileBackend;`，wasm32-unknown-unknown 正好落进这条分支）。
>
> **「必须关 default features」结论成立，但报错不在 redb 而在 `mio`**：iroh-blobs 默认 features 编 wasm32 时 48 个 error 全在 `mio v1.2.2`（E0308/E0599，UdpSocket/IoSource），经 tokio 由 bao-tree/iroh-io/irpc/iroh-relay 等多路拉入。**归因给 redb 会被一条 `cargo check` 打脸。**

> ⚠️ **常见误传纠正 2**：「iroh-blobs 没有 wasm cfg、靠 feature flag 做跨平台」—— 它**有** build.rs（:7 定义 `wasm_browser: { all(target_family = "wasm", target_os = "unknown") }`，Cargo.toml:79 依赖 cfg_aliases 0.2.1），src/ 下有 3 处 `cfg(wasm_browser)`（`store/util.rs:76`、`store/mem.rs:758`、`:767`）。**字面 grep `target_family` 会零命中 —— 这是 cfg alias 造成的间接层，别被骗。**

**关于内存上限**：源码只说「受可用内存限制」（`mem.rs:1-9`：*"Being a memory store, this store has to import all data into memory before it can serve it. So the amount of data you can serve is limited by your available memory."*），**没给任何数字**。

> ⚠️ **别写「2GB 直接 OOM」** —— 这个具体数字**在仓库和 issue 里均无出处**（`gh search issues --repo n0-computer/iroh-blobs "2GB OR OOM"` 返回空）。而 wasm32 的**架构硬上限是 4GiB**（`2^16 pages × 64KiB`，非 2GB）。**正确表述**：「mem store 全量入内存，容量受可用内存限制；wasm32 寻址硬上限 4GiB，实际远低于此（mem store 同时存 data + outboard 两份，import 期还有额外拷贝）」。真要给数字必须自己压测。

> **自己写浏览器 store 的两道墙**（#84/#86 未合 → 无持久化；#207 → wasm + irpc 组合仍坏；
> `Store` 是 struct 不是 trait，自定义意味着实现 irpc actor）→ [index-ecosystem-map.md](index-ecosystem-map.md)。
>
> **FsStore 的落盘布局**（`.data`/`.obao4`/`.sizes4`/`.bitfield` + redb）→ [03c-blobs.md](03c-blobs.md)。
> **后果链**：浏览器无 fs-store → 无 `.bitfield` 落盘 → **刷新即从零开始**。

## iroh-docs 在浏览器下丧失持久化

### 浏览器可行性

| | wasm CI | 备注 |
|---|---|---|
| **iroh-gossip** | ✅ `ci.yaml:150` job "Build wasm32"，:169 `cargo build --target wasm32-unknown-unknown`（**未加** --no-default-features，即默认 net+metrics 可用），:175 断言无 `import "env"` | 有真实浏览器实跑参考 `iroh-examples/browser-chat`（README:3-5 含线上 demo 链接），其 Cargo.toml 用 `iroh-gossip = { version = "0.101", default-features = false, features = ["net"] }`；架构为 shared(Rust lib) + cli + browser-wasm(wasm-bindgen) + frontend(TypeScript/React/Vite/shadcn) |
| **iroh-docs** | ⚠️ `ci.yaml:282` job "Build & test wasm32"，:310 为 `--no-default-features` → default 的 metrics/rpc/fs-store/redb-v2-migration **全部关闭** | **浏览器里丧失持久化**，只能内存态 |


---

# 第四部分：iroh-ffi 与 wasm —— 绕开它

## ❌ 完全不支持 wasm

`support-matrix.yaml` 全部语言只有四种：swift / kotlin / python / js。**零 wasm 构建目标、零 wasm 文档、CI 无 wasm job。** napi 的 11 个 target 也**全是原生**。

> ⚠️ **举证方法要小心**：`grep -rni "wasm" . --exclude-dir=.git` 有 108 处命中，但**不是「全部在 Cargo.lock」**。实测分布：Cargo.lock **32** + `iroh-js/yarn.lock` **74** + `iroh-js/index.js` **1** + `.yarn/releases/yarn-4.4.0.cjs` **1**。yarn.lock 那 74 处是 `@napi-rs/cli` 的工具链依赖（`@napi-rs/lzma-wasm32-wasi` 等），不是 iroh 的传递依赖。
>
> 更关键：`iroh-js/index.js:551` **存在一条 WASI 加载分支**（`require('@number0/iroh-wasm32-wasi')`，由 `NAPI_RS_FORCE_WASI` 触发）。它是 napi-rs 生成的样板且**实为死代码**（package.json 的 11 个 target 无 wasm32-wasi、optionalDependencies 无该包、仓内 find 不到任何 `*wasi*` 文件）。
>
> **所以「零 wasm 目标/文档/CI job」这个结论成立，但别用那条 grep 当判据。**

**iroh 内核本身是支持 wasm 的**（Cargo.lock 里 ws_stream_wasm / wasm-bindgen 就是证据；`iroh/Cargo.toml:16` 注释 *"We need 'cdylib' to actually generate .wasm files"*，:91 有 wasm target deps 段），但**官方 FFI 层刻意没往 wasm 走**。别把「iroh 支持 wasm」误读成「iroh-ffi 支持 wasm」。想要浏览器端得绕开 iroh-ffi 直接用 iroh + wasm-bindgen 自己写（见 [06-wasm-browser.md](06-wasm-browser.md)）。

**利好**：Web 端走 wasm-bindgen 时，**iroh 那半边不用你操心**，只需重写业务薄壳。

## 「同一份 uniffi FFI 喂移动端和 Web」不可行

不是没人做，是机制上走不通：

- **机制级证据**：`async-compat-0.2.5/src/lib.rs:460-463` 的 TOKIO1 初始化里有 `thread::Builder::new().name("async-compat/tokio-1".into()).spawn(...)` —— wasm32-unknown-unknown 无 threads 时这是死路。而如果每一个 `#[uniffi::export]` 都带 `async_runtime="tokio"`，就全中
- 旁证：`uniffi_core-0.31.1/Cargo.toml:41` 有 feature `wasm-unstable-single-threaded` —— **名字自带 unstable**；用它是为了在 wasm32 上摘掉 Future 的 Send bound（`ffi/rustfuture/mod.rs:42-53`）

> ⚠️ 常见的错误理由是「uniffi 不支持 wasm」—— **不准确**，uniffi 有 `wasm-unstable-single-threaded`。**真正的杀手是你自己的 async 模型**：`async_runtime="tokio"` → async-compat → `thread::spawn`。

---

# 第五部分：构建链路


| | browser-echo / browser-blobs | browser-chat |
|---|---|---|
| **工具** | 手工串 cargo + wasm-bindgen + wasm-opt | **wasm-pack** |
| **target** | `--target=web` | `-t bundler` |
| **消费** | `import init, { EchoNode } from "./wasm/browser_echo.js"; await init();` | `import { ChatNode } from "chat-browser"` —— **无需 init()** |
| **依赖声明** | — | `"chat-browser": "file:../browser-wasm/pkg"` |
| **Vite 配置** | — | `plugins: [react(), wasm(), topLevelAwait()]` |
| **前端** | vanilla JS，**无打包器** | React + Vite |

生成的 `.d.ts` 印证差异：`--target=web` 的 `browser_echo.d.ts` 有 `export default function __wbg_init(...)` 和 `initSync`（51-113 行）；`--target=bundler` 的 `chat_browser.d.ts` **完全没有这两个**。

```json
// browser-echo/package.json
"build:release": "... cargo build --target=wasm32-unknown-unknown --release
  && wasm-bindgen ./target/wasm32-unknown-unknown/release/browser_echo.wasm
       --out-dir=public/wasm --weak-refs --target=web
  && wasm-opt --enable-nontrapping-float-to-int --enable-bulk-memory -Os
       -o public/wasm/browser_echo_bg.wasm public/wasm/browser_echo_bg.wasm"
```

```json
// browser-chat/frontend/package.json
"build:wasm": "wasm-pack build ../browser-wasm --dev --weak-refs --reference-types -t bundler -d pkg",
"build:wasm:release": "wasm-pack build ../browser-wasm --release -t bundler -d pkg"
```

⚠️ **注意 `build:wasm:release` 丢掉了 dev 里的 `--weak-refs --reference-types`。**

**wasm-opt 的调用方式也不统一**：browser-echo 手动带 `--enable-nontrapping-float-to-int --enable-bulk-memory -Os`；browser-blobs 只有裸 `-Os`；browser-chat 靠 Cargo.toml 的 `[package.metadata.wasm-pack.profile.release] wasm-opt = [...]` 让 wasm-pack 代劳。

**用 Vite 就选 wasm-pack -t bundler 那条**（抄 browser-chat）。代价是 README 明说的两个 DX 坑：

```
"Note that you have to run `npm run build:wasm` *before* running `npm install`."
"Whenever you change something on the rust side, you need to rebuild the Wasm package with
 `npm run build:wasm`. Likely you will have to restart the Vite dev server afterwards, as the
 Wasm is not properly picked up by Vite's hot module reloader."
```

**顺序约束来自 `"chat-browser": "file:../browser-wasm/pkg"`——pkg 目录不存在时 `npm install` 直接失败。**

**「改 Rust 立刻见效」的开发体感在这里是打折的（HMR 不认 wasm，得重启 dev server），要提前告知团队。**

## 工具链版本必须精确一致

`browser-echo/Cargo.toml:33` `wasm-bindgen = "=0.2.122"`（**注意 `=` 前缀，精确锁版本**），browser-chat 同样。

**wasm-bindgen 的 crate 与 CLI 版本不匹配会直接报错，升级时两边必须同步动。**

**别让 wasm-pack 现场编 CLI**：它会自动执行 `cargo install --force wasm-bindgen-cli --version 0.2.122`，从源码编 walrus / wasmparser / wasm-bindgen-cli-support —— 慢且占盘（**实测因盘满直接 `No space left on device` 失败**）。

**CI 抄 `iroh-examples/.github/workflows/deploy.yml` 的做法**：`uses: taiki-e/install-action@v2 with: tool: wasm-bindgen,wasm-opt,wasm-pack,cargo-make` —— 走预编译二进制。本地同理，用 cargo binstall 或直接下 release 二进制。

另注 `iroh/iroh/Cargo.toml` 的 `[lib]` 声明 `crate-type = ["lib", "cdylib"]`，注释说明「We need "cdylib" to actually generate .wasm files」，且因 rust-lang/cargo#12260 无法做成 target-dependent。

## ⚠️ getrandom cfg：**不是编译硬门槛**（两份旧资料在此矛盾，以实测为准）

```toml
# browser-echo/.cargo/config.toml 全文（browser-chat 同）
[target.wasm32-unknown-unknown]
runner = "wasm-bindgen-test-runner"
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
```

**但 browser-blobs 没有这个文件**，而它 `Cargo.toml:19` 同样声明了 `getrandom = { version = "0.3", features = ["wasm_js"] }`。

**实测对照**：browser-blobs 无该 cfg 直接 build **成功**；browser-echo 用 `RUSTFLAGS=""` 显式清掉 cfg 后同样**成功**。两个例子的 Cargo.lock 里都同时存在 getrandom 0.2.17 / 0.3.4 / 0.4.2 三个版本，且三者在有无 cfg 两种情况下都照常参与编译。

> ⚠️ **所以「不加这个 rustflag 就编译失败/链接期报 getrandom 无 backend」这个说法与实测不符。**
>
> ⚠️ **但这只验证了「能编过」，没有验证运行时随机数是否真的正常** —— 未在浏览器里实跑。
>
> **建议**：**保留该 cfg 是无害且更保险的**（跟 n0 的 CI 一致，`ci.yml:111` 也显式带 `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'`）；但遇到 RUSTFLAGS 冲突时不必惊慌，它不是编译硬门槛。**密码学相关的东西不要仅凭「编过了」就放行，上线前需在真实浏览器验证 `SecretKey::generate` 的熵来源。**

**它是 rustflag 不是 feature —— 不会随 `cargo add iroh` 自动带过来。** 下游项目要用得自己在 `.cargo/config.toml` 或 `RUSTFLAGS` 里补。

## ⚠️ default features 在 wasm32 上是能编过的

**这条直接推翻一个常见前提。**

```yaml
# iroh/.github/workflows/ci.yml
wasm_test:
  name: Build & test wasm32 for browsers
  env:
    RUSTFLAGS: '--cfg getrandom_backend="wasm_js"'
  ...
    - name: wasm32 build (iroh-relay)
      run: cargo build --target wasm32-unknown-unknown -p iroh-relay

    - name: wasm32 build (iroh)
      run: cargo build --target wasm32-unknown-unknown -p iroh       # ← 没有 --no-default-features！

    # If the Wasm file contains any 'import "env"' declarations, then
    # some non-Wasm-compatible code made it into the final code.
    - name: Ensure no 'import "env"' in iroh Wasm
      run: |
        ! wasm-tools print --skeleton target/wasm32-unknown-unknown/debug/iroh.wasm | grep 'import "env"'

    - name: Run integration test in wasm
      run: cargo test -p iroh --test integration --target=wasm32-unknown-unknown
```

**iroh CI 带 default features（含 metrics + tls-ring + portmapper + fast-apple-datapath）直接编 wasm32，没有 `--no-default-features`。**

**「wasm 必须 default-features=false，因为 metrics 编不过」对 1.0.2 不成立**——metrics 的 wasm 支持在 **0.34.0** 就修好了（`CHANGELOG.md:885`，隶属 `## [0.34.0] - 2025-03-17`）：

> *(iroh)* Enable `netwatch::netmon::Monitor` and the `metrics` feature in Wasm ([#3206])

**如果你实测 default features 编不过 wasm，那是别的原因（大概率是 ring 的 C 编译，见下），不该归因到 metrics。**

### 机制：target-gated 依赖，不是 feature 裁剪

```toml
# iroh/iroh/Cargo.toml
# non-wasm-in-browser dependencies
[target.'cfg(not(all(target_family = "wasm", target_os = "unknown")))'.dependencies]
hickory-resolver = { version = "0.26.0", default-features = false }
portmapper = { version = "0.19.1", optional = true, default-features = false }
noq = { version = "1.0.1", default-features = false, features = ["runtime-tokio", "rustls"] }
tokio = { version = "1", features = ["io-util", "macros", "sync", "rt", "net", "fs", "io-std"] }

# wasm-in-browser dependencies
[target.'cfg(all(target_family = "wasm", target_os = "unknown"))'.dependencies]
wasm-bindgen-futures = "0.4"
# we don't use time directly, but need to enable it because x509_parser uses these in browsers
time = { version = "0.3", features = ["wasm-bindgen"] }
getrandom = { version = "0.4", features = ["wasm_js"] }
```

**`portmapper` 这个 dep 本身就声明在 `not(wasm)` 的 target 表里，所以 `portmapper` feature 的 `dep:portmapper` 在 wasm 上是空转。**

**关键心智：在 wasm 上开着 portmapper / fast-apple-datapath feature 是无害的，不需要为了 wasm 单独维护一份 feature 列表。**（注意 tokio 在 wasm 上没有 net/fs/io-std。）

> **libp2p 对照**：libp2p 需要你手动为 wasm 换 transport feature（如 `libp2p-websocket-websys` 替 tcp）；iroh 的等价物是**自动的**——同一份 feature 列表，靠 target cfg 换实现。

### 但 rustflag 得自己补

```toml
# iroh/.cargo/config.toml
[target.wasm32-unknown-unknown]
runner = "wasm-bindgen-test-runner"
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
```

**这是 rustflag，不是 feature——不会随 `cargo add iroh` 自动带过来。** 下游项目要用得自己在 `.cargo/config.toml` 或 `RUSTFLAGS` 里补。

> ⚠️ **但它不是编译硬门槛**（实测）：browser-blobs 无该 cfg 直接 build 成功；browser-echo 用 `RUSTFLAGS=""` 清掉后同样成功。**只验证了「能编过」，未验证运行时熵来源** —— 保留它是无害且更保险的。详见 [06-wasm-browser.md](06-wasm-browser.md)。

另：`[lib] crate-type = ["lib", "cdylib"]`，注释「We need "cdylib" to actually generate .wasm files」，且因 rust-lang/cargo#12260 无法做成 target-dependent。

## ⚠️ macOS 上的隐藏坑：ring 编不过

**这个坑没有任何文档提到，三个例子的 README 也没写。**

在 macOS 跑 README 里的原命令会报：

```
cargo:warning=error: unable to create target: 'No available targets are compatible with triple "wasm32-unknown-unknown"'
error occurred in cc-rs: command did not execute successfully: LC_ALL="C" "clang" ...
  "--target=wasm32-unknown-unknown" ... ring-0.17.14/crypto/curve25519/curve25519.c
```

**根因**：Apple 系统 clang（Xcode CLT 自带）**不支持 wasm32 target**（`clang --print-targets | grep -i wasm` 无输出）。触发链路是 `iroh` 的 `tls-ring` feature → ring 要编 C（`RING_SRCS` 里那批 .c：curve25519.c、aes_nohw.c、montgomery.c…，交给 cc crate）。


```rust
// ring-0.17.14/build.rs:594-603
const WASM32: &str = "wasm32";
// Allow cross-compiling without a target sysroot for these targets.
if (target.arch == WASM32)
    || (target.os == "linux" && target.env == "musl" && target.arch != X86_64)
{
    // TODO: Expand this to non-clang compilers in 0.17.0 if practical.
    if compiler.is_like_clang() {
        let _ = c.flag("-nostdlibinc");
        let _ = c.define("RING_CORE_NOSTDLIBINC", "1");
    }
}
```

**ring 对 wasm32 会实打实编译 C 源码**（`RING_SRCS` 里那批 .c：curve25519.c、aes_nohw.c、montgomery.c…），交给 cc crate（`Cargo.lock` 里 ring 的 build 依赖含 `cc`）。

**解法（实测有效）**：

```sh
brew install llvm     # Homebrew 的 clang 带 wasm 后端
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar
```

**这是本地 toolchain 问题，不是 iroh 约束**：
- **独立验证**：`cargo tree --target wasm32-unknown-unknown -p iroh -i ring --edges normal` 实测 **ring v0.17.14 确实进入 wasm32 依赖图**（经 noq-proto 与 rustls v0.23 两条路径，后者由 `iroh-dns/tls-ring = ["rustls/ring", ...]` 引入）
- **iroh CI 在 Linux 上带 default（含 tls-ring → noq/ring → ring）编 wasm32 是通过的**，所以 ring→wasm 本身可行。CI（ubuntu-latest）不受影响 —— **这正是它没被 n0 发现的原因**

**错误信息指向 ring 而非 clang，极易误判成「iroh 编不了 wasm」。**

> ⚠️ **一处措辞要收紧**：ring 的 `build.rs:594-603` **不是「明确要求 clang-like 编译器」**——它的语义是「**如果**编译器 `is_like_clang()`，**才**追加 `-nostdlibinc`」，非 clang 编译器只是不加这两个标志，build.rs 既不报错也不 gate（紧邻的 TODO 还写着「Expand this to non-clang compilers」）。**实践上 wasm32 仍需 LLVM/clang（因为只有它能产 wasm 后端目标码），但这个结论来自工具链事实，不来自这段 build.rs。**

**若改走 tls-aws-lc-rs 逃避，代价更大**：aws-lc-sys 的 C/汇编依赖更重。

## release profile 的 workspace 绕法

三例都用同一组值，但 browser-chat 因为是 workspace 被迫写进 `.cargo/config.toml`：

```toml
# browser-chat/browser-wasm/.cargo/config.toml
# we specify the profile here, because it is the only way to define
# different settings for a single crate in a workspace.
[profile.release]
codegen-units = 1
strip = "symbols"
debug = false
lto = true
opt-level = "z"
panic = 'abort'

[target.wasm32-unknown-unknown]
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
```

**Cargo 的 `[profile.*]` 只能在 workspace root 生效，成员 crate 的 `[profile.*]` 会被忽略（且告警）。给单个 crate 定制 profile 只能通过该 crate 的 `.cargo/config.toml`** —— 这是官方例子采用的绕法。**代价是从 workspace root 构建它时这份 profile 不生效。**

**⚠️ 已有根 workspace 的 monorepo 将来加 wasm crate 会正面撞上这个限制。**

browser-blobs **缺这段**，白白多付约 **39%** 的 gzip 体积（套上 echo 的 profile 重编：wasm-opt 后从 4510KB/gzip 1739KB 降到 3027KB/gzip ~1253KB，raw 省 1483KB、gzip 省约 486KB）。


# 第五部分：体积

> ⚠️ **可信度声明**：以下是**未经复现的单次测量**（本机 aarch64-darwin，wasm-bindgen 0.2.122 + binaryen 123，2026-07-17）。产物已不存在，无法重跑核验。算术自洽已审计（9 组增减百分比误差 <0.1pp）。**建议不要把这些数字当基线，而是在自己 CI 里加体积门禁实测。**

| 样例 | post-bindgen | gzip | brotli | wasm-opt -Os 后 raw | -Os 后 gzip | -Os 后 brotli |
|---|---|---|---|---|---|---|
| **browser-echo**（iroh + echo 协议） | 2794KB | 1028KB | 793KB | 2538KB | 1073KB | 829KB |
| **browser-chat**（iroh + gossip） | 3124KB | 1131KB | 868KB | 2825KB | 1185KB | 908KB |
| **browser-blobs**（iroh + blobs，套用 echo 的 profile） | 3348KB | 1214KB | 926KB | 3027KB | ~1253-1265KB | 968KB |

（压缩用 gzip -9 与 node zlib brotli quality 11。browser-blobs 那格 gzip 有两个互相矛盾的测量值，说明该数字不可靠 —— 更佐证上面的可信度声明。）

**两条结论**：

1. **iroh-blobs 的增量只有约 +133KB brotli**（793→926 pre-opt）。所以「Web 端要不要上 blobs」**不该由体积决定**，该由 MemStore 内存约束决定。
   > ⚠️ 「iroh 骨架 brotli 约 830KB」这个说法偏乐观 —— 那是 browser-echo -Os 后的 829KB，而该产物含 echo 协议 + tracing-subscriber(env-filter，拉进 regex) + wasm-streams + serde-wasm-bindgen + console_error_panic_hook。**首屏想优化，先砍 tracing-subscriber 的 env-filter，比砍 blobs 划算。**

2. **wasm-opt -Os 在已 strip + LTO 的构建上对传输体积是负收益**：raw 小约 9%，但 **gzip/brotli 大约 4.5%** —— 而真正走网络的是压缩后的字节。三个例子独立复现同一方向。
   - 唯一 wasm-opt 帮上忙的是 browser-blobs 的**原始配置**（gzip 1953→1739KB，-11%）—— 但那只是因为它**没有 `[profile.release]`、符号没 strip**，wasm-opt 顺手清了本该由 strip 干的活
   - → **照抄这条链路前自己实测再决定留不留 wasm-opt，别默认它有用。**（前提：本机单次测量、已 strip+LTO 的产物；换 binaryen 版本或 flag 组合可能不同）


# 第六部分：可抄的代码模式

## 三层拆分（browser-echo）

`src/node.rs` 共享 core（浏览器和 CLI **完全同一份代码，无 cfg 分支**）：

```rust
// src/node.rs:22-33
let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
    .alpns(vec![Echo::ALPN.to_vec()])
    .bind()
    .await?;
let router = Router::builder(endpoint).accept(Echo::ALPN, echo).spawn();
```

`src/wasm.rs` 只做类型转换，`src/bin/cli.rs` 是 CLI。

单 crate 方案靠 `[[bin]] required-features = ["cli"]` + `cli = ["tokio/full", "dep:clap"]` 把 tokio full/clap 挡在 wasm 构建之外；wasm 构建时用 `cargo build --lib`。

## 长生命周期对象 = newtype + 异步静态构造

```rust
// src/wasm.rs:29-35
#[wasm_bindgen]
pub struct EchoNode(node::EchoNode);

#[wasm_bindgen]
impl EchoNode {
    pub async fn spawn() -> Result<Self, JsError> {      // ← 异步构造器叫 spawn()
        Ok(Self(node::EchoNode::spawn().await.map_err(to_js_err)?))
    }
}
```

**newtype 是必需的而非风格选择**：wasm-bindgen 不能给外部 crate 的类型加 `#[wasm_bindgen]`，也不支持泛型/生命周期，所以 `wasm.rs` 的职责被压缩成「包一层 + 转类型」。

生成的 `.d.ts`（:11-19）印证：`export class EchoNode { private constructor(); free(): void; [Symbol.dispose](): void; ... static spawn(): Promise<EchoNode>; }` —— **构造函数是 private，只能走 static spawn()**。

`--weak-refs` 决定内存是自动回收还是必须手工 `free()`：生成的 `browser_echo.js:890-892` `const EchoNodeFinalization = (typeof FinalizationRegistry === 'undefined') ? { register: () => {}, unregister: () => {} } : new FinalizationRegistry(ptr => wasm.__wbg_echonode_free(ptr, 1));`，:78 `if (Symbol.dispose) EchoNode.prototype[Symbol.dispose] = EchoNode.prototype.free;`。

暴露面复杂度：browser-echo / browser-blobs 各只导出 1 个类型；**browser-chat 最复杂**，还导出 `Channel` / `ChannelSender`，用 `#[wasm_bindgen(getter)]` 暴露 sender/receiver，用 `serde_wasm_bindgen::from_value` 接收 JS options。

## Rust Stream → JS ReadableStream，只需 6 行

```rust
// browser-echo/src/wasm.rs:66-71 —— 抽成泛型 helper
fn into_js_readable_stream<T: Serialize>(
    stream: impl Stream<Item = T> + 'static,
) -> wasm_streams::readable::sys::ReadableStream {
    let stream = stream.map(|event| Ok(serde_wasm_bindgen::to_value(&event).unwrap()));
    ReadableStream::from_stream(stream).into_raw()
}

// browser-chat —— 内联版，额外把 Err 转成 JsValue 让 JS reader 抛错
let receiver = receiver.map(move |event| {
    event.map_err(|err| JsValue::from(&err.to_string()))
         .map(|event| serde_wasm_bindgen::to_value(&event).unwrap())
});
```

依赖 `wasm-streams = "0.5"` + `serde-wasm-bindgen = "0.6.5"`。`from_stream` 要求 `Item = Result<JsValue, JsValue>`。

⚠️ browser-echo helper 里的 `Ok(...)` 和 `.unwrap()` 意味着**序列化失败会直接 panic 到 JS**（靠 console_error_panic_hook 显形）。

**这不是强制范式**：browser-blobs 的 Cargo.toml 里根本没有 wasm-streams——它全用普通 async fn。只在需要推事件时才用。

> libp2p 事件通常是 Swarm 的单一 `select_next_some()` 事件循环；iroh 这里是**每个协议/连接自己产一路 Stream，再各自转成独立的 ReadableStream 交给 JS**。

## JS 侧消费：用 getReader()，别用 for-await

| | 写法 | 依赖 |
|---|---|---|
| browser-echo | `for await (const event of node.events())`（`public/main.js:32`、:52） | ReadableStream 的 async iterator（`Symbol.asyncIterator`）—— 较新的 web 平台特性，各浏览器支持不一（此为 web 平台常识，非本仓证据，需按目标浏览器验证） |
| **browser-chat** ✅ | `const reader = channel.receiver.getReader() as ReadableStreamDefaultReader<ChatEvent>; while (true) { const { done, value } = await reader.read(); if (done) break; ... }`（`iroh.ts:77-83`） | 只依赖标准 `getReader` |

**⚠️ locked 陷阱**：`browser-chat/browser-wasm/src/lib.rs:127-130` 的 `#[wasm_bindgen(getter)] pub fn receiver(&mut self) -> ChannelReceiver { self.receiver.clone() }` —— ChannelReceiver 是 JS 对象的 wrapper，**clone 复制的是句柄而非流本身**。**`ReadableStream` 同一时刻只能有一个 reader —— 多次调用 getter 拿到的是同一个底层流的引用，不是独立订阅。**

前端 `iroh.ts` 全仓只在 :77 调用了**一次** getReader()（grep 全 frontend 仅 1 处命中）。**若事件流被多个组件订阅，应像 iroh.ts 那样在一个 class/store 里单点消费再 fan-out**（它的 subscribers / peerSubscribers / neighborSubscribers 三个数组就是这个模式）。

对比 browser-echo 的 `events()` 每次调用都 `self.accept_events.subscribe()` 新建一路 broadcast 订阅 —— **那才是独立订阅**。

## 跨 JS 边界的事件类型约定

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectEvent {
    Connected,
    Sent { bytes_sent: u64 },
    Received { bytes_received: u64 },
    Closed { error: Option<String> },
}
```

⚠️ **外层 `rename_all="camelCase"` 只改 variant 名，字段名要在每个 variant 上再标一次**。browser-chat 的 `Event` 是逐个 variant 重复标注的；**browser-echo 的 `ConnectEvent` 反而漏标了**（`bytes_sent` 到 JS 仍是 snake_case），main.js 因为只做 `JSON.stringify` 展示所以没暴露问题。

## ⚠️ 没有从 Rust 自动生成 TS 类型

```ts
// browser-chat/frontend/src/lib/iroh.ts:293
// types used in chat-browser, for now they are defined manually here.
type JoinedEvent = { type: "joined", neighbors: string[] }
type MessageEvent = { type: "messageReceived", from: string, text: string, nickname: string, sentTimestamp: number }
...
type ChatEvent = JoinedEvent | MessageEvent | NeighborUpEvent | NeighborDownEvent | PresenceEvent | LaggedEvent

// 消费处靠断言接上（:77）
const reader = channel.receiver.getReader() as ReadableStreamDefaultReader<ChatEvent>
```

**wasm-bindgen 只为导出的 struct/fn 生成 .d.ts，经 serde-wasm-bindgen 转出去的 JsValue 在 TS 侧就是 `any`**——事件负载的类型安全需要自己补。

生成的 `.d.ts` 里 stream 只有裸 `ReadableStream`，**没有泛型参数**：
- `browser_echo.d.ts:15-17`：`connect(endpoint_id: string, payload: string): ReadableStream;` / `events(): ReadableStream;`
- `chat_browser.d.ts:17-18`：`ticket(opts: any): string;` / `readonly receiver: ReadableStream;`

**Rust 侧改了字段 TS 侧静默不报。** 若认真做 Web 端，**需自己补一个类型生成步骤**（specta 直接对 wasm 边界导出 TS，或 ts-rs）。

## 错误转换

```rust
// src/wasm.rs:61-64 —— 三处一字不差
fn to_js_err(err: impl Into<anyhow::Error>) -> JsError {
    let err: anyhow::Error = err.into();
    JsError::new(&err.to_string())
}
```

—— **把 anyhow::Error 拍成字符串**。

**若要在前端区分错误类型（如「对方拒收」vs「网络失败」），得自己设计一个可序列化的 error enum 走 serde_wasm_bindgen，而不是照抄 to_js_err。**

## 统一初始化：两条注释就是两个坑

```rust
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .with_writer(
            // To avoide trace events in the browser from showing their JS backtrace
            MakeConsoleWriter::default().map_trace_level_to(tracing::Level::DEBUG),
        )
        // If we don't do this in the browser, we get a runtime error.
        .without_time()
        .with_ansi(false)
        .init();
}
```

1. **`.without_time()` 不加会运行时报错**（wasm 里没有 `SystemTime::now` 的默认实现）
2. **tracing 的 TRACE 级别在浏览器 console 里会带 JS backtrace**，要用 `map_trace_level_to` 降级规避

（browser-chat 用 `LevelFilter::DEBUG`，echo/blobs 用 `TRACE`。）

---

# 附：寻址在浏览器下只剩 pkarr 一条腿

**DHT 与 mDNS 完全不可用** —— 不是降级，是编译/运行都没有。

- 正面：内置 pkarr 模块处处有 wasm 分支（`pkarr.rs:310/313`、:458/461 多处 `#[cfg(wasm_browser)]` 成对出现）—— 被刻意支持
- `DnsAddressLookup` 整模块被排除：`address_lookup.rs:120-126` `#[cfg(not(wasm_browser))] pub mod dns;`；presets 里同样跳过（`presets.rs:131-134`）
- 反面：对 `iroh-address-lookups` 全仓 grep `wasm|target_arch`（含 `*.rs` 与 `*.toml`）**零结果**；对 `swarm-discovery` 的 src/ 与 Cargo.toml grep `wasm` 亦**零结果** —— 两者根本没考虑过 wasm，底层分别是 noq-udp 与 socket2 UDP 多播

**Web 端必须依赖 pkarr relay**（n0 的 dns.iroh.link 或自建）。这意味着「无服务器」卖点在 Web 端天然打折 —— 桌面/移动可纯 DHT，Web 不行。**要么三端统一用 pkarr，要么接受三端能力不对等并在 UI 讲清楚 —— 这是产品决策，不只是技术细节。**
