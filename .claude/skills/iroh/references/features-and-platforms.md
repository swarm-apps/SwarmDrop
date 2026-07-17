# Feature Flags 与跨平台约束

iroh 1.0.2 · 调研日期 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`

**核心心智**：iroh 的 feature 面比 libp2p 小得多——**只有 10 个 feature，且没有任何 discovery-\* feature**。跨平台**不是靠 feature 裁剪**，而是靠 build.rs 的两个 cfg alias + Cargo.toml 里的 **target-gated 依赖**。

## 全部 10 个 feature

```toml
# iroh/iroh/Cargo.toml
[features]
default = ["metrics", "fast-apple-datapath", "portmapper", "tls-ring"]
portmapper = ["dep:portmapper"]
metrics = ["iroh-metrics/metrics", "iroh-relay/metrics"]
test-utils = ["iroh-relay/test-utils", "iroh-relay/server", "dep:axum"]
# Enables fetching TLS trust anchors from the operating system
platform-verifier = ["iroh-relay/platform-verifier"]
qlog = ["noq/qlog"]
# Use private Apple APIs to send multiple packets in a single syscall.
fast-apple-datapath = ["noq/fast-apple-datapath"]
# Use ring as the crypto backend.
tls-ring = ["noq/ring", "iroh-relay/tls-ring", "iroh-dns/tls-ring"]
# Use aws-lc-rs as the crypto backend, unless `ring` is also enabled.
tls-aws-lc-rs = ["noq/aws-lc-rs", "iroh-relay/tls-aws-lc-rs", "iroh-dns/tls-aws-lc-rs"]
# Unstable: Custom transport API (may change without notice)
unstable-custom-transports = []
# Unstable: API to access an endpoint's NetReport (may change without notice)
unstable-net-report = []
```

> ⚠️ **`discovery-*` 系列 feature 在 1.0.2 不存在。** grep 整个 workspace 的 `*.toml`，"discovery" 只在 `iroh-dns/Cargo.toml` 的 description 文案里出现一次。**别照搬 0.x 的 `discovery-local-network` / `discovery-pkarr-dht`**（历史脉络：`CHANGELOG.md:1495` 记录 0.23.0 时代曾有 `discovery-local-network`，1.0 已废除该模式）。

> **libp2p 对照**：libp2p 习惯是「一个能力 = 一个 feature」（mdns/kad/relay/dcutr 全是 feature）。**iroh 反过来：能力靠 trait 对象 + builder 注入，feature 只用来选 crypto backend / 平台加速 / 可选副作用（metrics、portmapper）。不要用 libp2p 的 feature 心智去找 iroh 的开关。**

## 两个 cfg alias

```rust
// iroh/iroh/build.rs
use cfg_aliases::cfg_aliases;
fn main() {
    cfg_aliases! {
        wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
        with_crypto_provider: { any(feature = "tls-ring", feature = "tls-aws-lc-rs") }
    }
}
```

⚠️ **`wasm_browser` 的判据含 `target_os = "unknown"`，所以只命中 `wasm32-unknown-unknown`。`wasm32-wasip1`/`wasip2` 不走 wasm_browser 分支，会去走 `not(wasm_browser)` 的 IP transport 路径。别把「wasm」和「wasm_browser」当同义词。**

`with_crypto_provider` 详见 [endpoint-and-presets.md](endpoint-and-presets.md)——**它 gate 的是 `Minimal`(57/59) / `N0`(111/113) / `N0DisableRelay`(173/175) 三个 preset 类型的存在性**。在 `iroh/src` 下 41 处命中里非测试只有 7 处（presets.rs 6 + `tls.rs:24-25`）。

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

> ⚠️ **但它不是编译硬门槛**（实测）：browser-blobs 无该 cfg 直接 build 成功；browser-echo 用 `RUSTFLAGS=""` 清掉后同样成功。**只验证了「能编过」，未验证运行时熵来源** —— 保留它是无害且更保险的。详见 [wasm-and-browser.md](wasm-and-browser.md)。

另：`[lib] crate-type = ["lib", "cdylib"]`，注释「We need "cdylib" to actually generate .wasm files」，且因 rust-lang/cargo#12260 无法做成 target-dependent。

## ⚠️ `default-features = false` 的连带杀伤

**关掉 default 会连带关掉 `tls-ring` → `with_crypto_provider` 为假 → `presets::N0` / `Minimal` / `N0DisableRelay` 三个类型整个从 API 消失。**

而 crypto provider 缺失是**运行时**错误（`endpoint.rs:228-230` 的 `ok_or_else`），**代码照样编过，跑起来 `bind()` 才炸**。

**无 tls-* feature 时必须**：

```rust
Endpoint::builder(presets::Empty)
    .crypto_provider(Arc::new(/* 你自己的 provider */))
```

详见 [endpoint-and-presets.md](endpoint-and-presets.md)。

## tls-ring vs tls-aws-lc-rs

**两者都开时，preset 路径选 ring**（`presets.rs:66-75` 的两个 `#[cfg]` 分支）。Cargo.toml 注释：`# Use aws-lc-rs as the crypto backend, unless "ring" is also enabled.`

> ⚠️ **但别说成「aws-lc-rs 白编一遍还不生效」——那是错的。** `Builder::crypto_provider` 是公开 setter 且在 preset 之后**覆盖**（`endpoint.rs:761` 直接赋值），显式传 aws-lc-rs 时 ring 完全不参与。官方两个 example 就是这么干的：
>
> ```rust
> // examples/pq-only-key-exchange.rs —— 用 Empty，preset 压根不设 provider
> Endpoint::builder(presets::Empty).crypto_provider(pq)
> // examples/prefer-pq-key-exchange.rs:42/73 —— 用 N0，然后覆盖
> Endpoint::builder(presets::N0).crypto_provider(pq)
> ```
>
> `pq-only-key-exchange.rs` 头注释逐字写死：「With iroh's default features still on, both `ring` and `aws-lc-rs` get linked. **That's harmless — we wire the aws-lc-rs provider in directly via `Builder::crypto_provider`.**」
>
> **所以「想真用 aws-lc-rs 必须 `default-features = false`」也是错的。** ring 只是被一起链接进来（多编一份，二进制变大）。`default-features = false` 只在你想真正把 ring 从依赖树里去掉（体积/合规诉求）时才必要。

## ⚠️ fast-apple-datapath：默认开着，但实为死代码

**iroh 自身源码里零实现**——`grep` `iroh/src` 结果为空，Cargo.toml 那一行是唯一出现处，纯透传到 `noq/fast-apple-datapath` → `noq-udp`。

```toml
# iroh 侧
fast-apple-datapath = ["noq/fast-apple-datapath"]
# noq-1.0.1/Cargo.toml
fast-apple-datapath = ["udp/fast-apple-datapath"]
```

```rust
// noq-udp-1.0.1/build.rs
apple: {
    any(target_os = "macos", target_os = "ios", target_os = "tvos",
        target_os = "visionos", target_os = "watchos")
},
apple_fast: { all(apple, feature = "fast-apple-datapath") },
apple_slow: { all(apple, not(feature = "fast-apple-datapath")) },
```

**作用域只在 apple 系，其他平台完全惰性——这解释了为什么 Android CI 和 wasm CI 都懒得关它。**

（证据链跨仓：实现在 crates.io 的 `noq-udp-1.0.1`，本机路径 `~/.cargo/registry/src/index.crates.io-*/noq-udp-1.0.1/`，**不在 iroh-study 树内**。该 feature 是 0.x 后期才加的，`CHANGELOG.md:371`「Add fast-apple-datapath feature and enable it by default (#3724)」，隶属 `:365` 的 `## [0.96.0] - 2026-01-28`。）

### 真相：私有符号是 dlsym 动态解析的，且从没人调用过

```rust
// noq-udp-1.0.1/src/unix.rs:199
apple_fast_path: AtomicBool::new(false),        // ← 默认就是关的

// unix.rs:355
/// Enables Apple's fast UDP datapath using private `sendmsg_x`/`recvmsg_x` APIs.
/// Once enabled, this also updates [`max_gso_segments`] to allow batched sends.
///
/// # Safety
///
/// These APIs may crash on unsupported OS versions, so callers must verify
/// availability before enabling.
#[cfg(apple_fast)]
pub unsafe fn set_apple_fast_path(&self) {
    self.apple_fast_path.store(true, Ordering::Relaxed);
    self.max_gso_segments.store(BATCH_SIZE, Ordering::Relaxed);
}

// unix.rs:653-661 —— 运行时 dlsym，不是静态链接
fn resolve_symbol(...) { ... libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) ... }
// :626-631 / :638-643 —— 用 c"sendmsg_x" / c"recvmsg_x" 字面量按名查找再 transmute 成 fn 指针
```

**对 iOS 分发的准确评估**（这条容易被高估）：

1. **私有符号不是静态链接的**——Mach-O 里**没有 `sendmsg_x`/`recvmsg_x` 的 undefined import**，只有字符串字面量（且仅在 `apple_fast` 下编入）
2. **更关键：`set_apple_fast_path` 全链路无人调用**（grep `noq-1.0.1/src`、`noq-udp-1.0.1/src`、`iroh/src` 三处均无调用者，只有定义处 `unix.rs:355` 和一处注释 `unix.rs:1185`）。所以 `is_apple_fast_path_enabled()` **恒为 false**，`send`(:470-477) 与 `recv`(:250-262) 都在进入 `send_via_sendmsg_x`/`recv_via_recvmsg_x` **之前**就分流到 `send_single`/`recv_single`——**dlsym 在运行时根本不会被执行到**
3. 真要开，是 `unsafe` 且 doc 明写「may crash on unsupported OS versions」

**准确表述：默认配置下这是「编进去的死代码 + 两个字符串常量」。静态扫描面是字符串匹配而非链接符号，风险远低于「App 里静态链接了私有 API」这种说法暗示的程度。**

**iOS 上仍可评估 `default-features = false` 去掉它，但理由应该是「去掉无用死代码 / 减小体积 / 消除字符串扫描面」，而不是「规避已链接的私有 API」。** 记得补回 tls-ring（见上文连带杀伤）。

> libp2p/quinn 生态没有等价物；这是 n0 fork quinn 成 noq 之后自己加的 Apple 特化。

## iOS / Android：没有任何专属 feature

```yaml
# iroh/.github/workflows/ci.yml —— Android
- name: Build workspace test binaries
  if: matrix.target == 'x86_64-linux-android'
  env:
    ANDROID_NDK_HOME: ${{ steps.setup-ndk.outputs.ndk-path }}
    CARGO_NDK_TARGET: ${{ matrix.target }}
  run: |
    cargo ndk test --no-run -p iroh-base --all-features
    cargo ndk test --no-run -p iroh-dns --features tls-ring
    cargo ndk test --no-run -p iroh-relay --features tls-ring,metrics
    cargo ndk test --no-run -p iroh --features tls-ring,metrics,portmapper,test-utils
```

矩阵 target：`aarch64-linux-android` / `armv7-linux-androideabi` / `x86_64-linux-android`，NDK **r25c**，cargo-ndk **4.1.2**。

⚠️ **`--features X` 是叠加在 default 之上的**（没有 `--no-default-features`），所以这行实际生效的 feature = default ∪ {test-utils} = `metrics + fast-apple-datapath + portmapper + tls-ring + test-utils`。

**这份列表反映的是 n0 心目中「Android 该有的样子」（tls-ring + metrics + portmapper），但它并没有真的排除 fast-apple-datapath——靠的是它在非 apple 平台惰性。**

**iOS 在 ci.yml 里 grep 不到任何 job**，只有 noq-udp 的 apple alias 覆盖 `target_os = "ios"`。即 **iOS 无 CI 覆盖**。

## portmapper：no-op stub 而非报错

```rust
// iroh/iroh/src/portmapper.rs
//! Portmapper integration.
//!
//! Wraps the real [`portmapper`] crate when the `portmapper` feature is enabled,
//! or provides a no-op stub otherwise.

pub(crate) fn create_client(config: &PortmapperConfig) -> Client {
    match config {
        #[cfg(all(not(wasm_browser), feature = "portmapper"))]
        PortmapperConfig::Enabled {} => Client::Enabled(::portmapper::Client::default()),
        _ => {
            let (tx, rx) = watch::channel(None);
            Client::Disabled { _tx: tx, rx }
        }
    }
}
```

**这是刻意的 API 稳定性设计：`PortmapperConfig` 无 cfg，跨平台代码可以无脑 `.portmapper_config(...)`。真实开关是 `all(not(wasm_browser), feature = "portmapper")` 双条件。**

值得写进产品决策的点：`PortmapperConfig::Disabled` 的 doc 提到 **UPnP 的 SSDP multicast 会在 macOS 上弹防火墙授权对话框**——对桌面 app 分发是个真实的 UX 决策点。

> libp2p 的 `libp2p-upnp` 是独立 behaviour，不装就没有。iroh 是「**装了但可能是空壳**」。

## ⚠️ ring 编不到 wasm：是本地 toolchain 问题，不是 iroh 约束

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

**独立验证**：`cargo tree --target wasm32-unknown-unknown -p iroh -i ring --edges normal` 实测 **ring v0.17.14 确实进入 wasm32 依赖图**（经 noq-proto 与 rustls v0.23 两条路径，后者由 `iroh-dns/tls-ring = ["rustls/ring", ...]` 引入）。

**关键结论：iroh CI 在 Linux 上带 default（含 tls-ring → noq/ring → ring）编 wasm32 是通过的，所以 ring→wasm 本身可行。**

**在 macOS 失败的根因是 Apple 系统 clang（Xcode CLT 自带）不含 WebAssembly target/backend。**

**修法是换编译器而不是关 feature**：

```bash
brew install llvm     # Homebrew 的 clang 带 wasm 后端
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar
```

> ✅ **这条修法已实测有效**（macOS aarch64，2026-07-17）。详见 [wasm-and-browser.md](wasm-and-browser.md) 的「macOS 上的隐藏坑」。
>
> ⚠️ **另一处措辞要收紧**：build.rs 那段**不是「明确要求 clang-like 编译器」**——它的语义是「**如果**编译器 `is_like_clang()`，**才**追加 `-nostdlibinc`」，非 clang 编译器只是不加这两个标志，build.rs 既不报错也不 gate（紧邻的 TODO 还写着「Expand this to non-clang compilers」）。**实践上 wasm32 仍需 LLVM/clang（因为只有它能产 wasm 后端目标码），但这个结论来自工具链事实，不来自这段 build.rs。**

**若改走 tls-aws-lc-rs 逃避，代价更大**：aws-lc-sys 的 C/汇编依赖更重。

> **libp2p 对照**：libp2p 默认也吃 ring（经 quinn/rustls），**同样的 C 编译约束——这一点两边没差别，不是 iroh 特有的坑**。

## 各 target 速查

| target | IP transport | relay | QAD 探测 | DNS resolver | portmapper | 备注 |
|--------|-------------|-------|---------|-------------|-----------|------|
| linux/macos/windows | ✅ | ✅ | ✅ | ✅ | feature 决定 | 完整 |
| iOS / Android | ✅ | ✅ | ✅ | ✅ | feature 决定 | **iOS 无 CI**；Android 需手写 JNI 初始化（见 [ffi-and-bindings.md](ffi-and-bindings.md)）|
| wasm32-unknown-unknown | ❌ **整个编译掉** | ✅ WebSocket | ❌ 只剩 Https | ❌ 只剩 pkarr HTTPS | no-op stub | **无直连无打洞**；custom transport 仍可注册 |
| wasm32-wasip1/p2 | ✅（**不走 wasm_browser 分支**）| ✅ | ✅ | ✅ | — | 未见 CI 覆盖 |
