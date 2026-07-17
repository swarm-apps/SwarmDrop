# iroh-web spike (#60)

验证 **iroh 能否在真实浏览器里跑**。这是 M3 闸门的一环，不是产品代码。

放在 `spike/` 而非 `crates/`：`crates/web` 是 #72 定的生产位置，本 spike 要能整目录删。
根 `Cargo.toml` 里 `exclude = ["spike"]` —— 它 wasm-only，进 workspace 会被桌面 target
白编一遍，且自带的 `[profile.release]` 会被 workspace root 静默忽略。

依赖与 features 抄自 iroh 官方 [browser-echo](https://github.com/n0-computer/iroh-examples/tree/main/browser-echo)。

## 结论（2026-07-17）

| | 结果 |
|---|---|
| 编译到 `wasm32-unknown-unknown` | ✅ |
| 真实 Chrome 加载 wasm | ✅ 15 ms |
| 浏览器里建 `Endpoint` | ✅ 17 ms |
| 连上 relay (`aps1-1.relay.n0.iroh.link`) | ✅ 2.6 s |
| console error | ✅ 无 |
| **wasm 体积** | **2005 KB 裸 / 849 KB gzip** + 45 KB js glue |

> ⚠️ **relay 那条不能作为「中国可用」的证据**：验证时本机出口 IP 在东京，
> 量的是「东京→新加坡」。#62 的国内三网实测仍必须另做。

## 跑

需要 `brew install llvm`（见下方坑 1）与 `rustup target add wasm32-unknown-unknown`。

```bash
wasm-pack build --target web --weak-refs --release
npx http-server -p 8420 --cors -c-1 .
open http://127.0.0.1:8420/index.html
```

`--weak-refs` 不能省：靠 JS FinalizationRegistry 自动回收 Rust 对象。
iroh 的 `Endpoint`/`Connection` 是持有资源的长生命周期对象，不开就得手动 `.free()`，
漏一个就泄漏。

## 踩过的坑（文档都不会告诉你）

1. **Apple clang 没有 WebAssembly backend** —— `clang -print-targets` 一条 wasm 都没有。
   `ring`（`tls-ring` 带来的）要把 C 编到 wasm，必挂在
   `No available targets are compatible with triple "wasm32-unknown-unknown"`。
   解：`brew install llvm`，并在 `.cargo/config.toml` 显式指
   `CC_wasm32_unknown_unknown` / `AR_wasm32_unknown_unknown`。Linux 的发行版 clang 通常自带。
2. **`getrandom` 需要 rustflags** `--cfg getrandom_backend="wasm_js"`，少了编不过且报错不指向这里。
3. **`presets::Empty` 连必需项都不设** —— 它的 `apply()` 直接返回原 builder，
   连 `crypto_provider` 都不给，`bind()` 必报 `InvalidCryptoProvider`。要用 `Minimal`。
   而且那个 provider 走的是 **builder 字段**，不是 rustls 的进程默认 ——
   `rustls::crypto::ring::default_provider().install_default()` 对它**无效**。
4. **`Watcher::initialized()` 返回得很早** —— 它等的是「Nullable 从空变为有值」，
   即 relay 一进列表就返回，此时握手还在进行（`is_connected()=false` 且 `last_error()=None`）。
   把「还在连」误报成「连不上」。要 `updated()` 轮到终态。
5. **`initialized()` 与 `updated()` 返回类型不对称** —— 前者把 `Vec<RelayStatus>` 解包成
   单个 `RelayStatus`，后者返回完整 `Vec`。
6. **`Endpoint` 必须显式 `close()`** —— iroh 在 `impl Drop for EndpointInner` 里
   故意 `tracing::error!("Endpoint dropped without calling Endpoint::close")`。
   浏览器里尤其要紧：不 close 就不向 relay 发优雅断开，对端只能等超时。

## 对规划的印证

- **`n0-future` 出现在官方浏览器示例的依赖里** → M2 (#57) 「用 n0-future 替换 core 的
  29 处 `tokio::spawn`/`time`」是 iroh 生态的既定路径，不是我们的臆测。
  同理官方示例的 `tokio = { default-features = false, features = ["sync"] }`
  印证了「wasm 上 tokio 只剩 sync 可用」。
- **wasm-pack 从 `Cargo.lock` 自动装匹配的 `wasm-bindgen-cli`**（实测装了 v0.2.126），
  故无需像官方示例那样硬 pin `="0.2.122"` —— 那个 pin 是**手工串 CLI** 链路的产物
  （`cargo build` 后自己调 `wasm-bindgen`，schema version 对不上直接报错）。
- **Web 走 wasm-bindgen、不经 UniFFI**，与 iroh 官方唯一推荐路径一致
  （*"keep building it using wasm-bindgen"* / *"we don't bundle iroh's Wasm build as an
  NPM package"* → 自写 application-specific wrapper crate）。故 #61 已关闭。
