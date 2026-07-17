# webrtc-direct × HTTPS spike

验证 [`dev-notes/knowledge/libp2p-wasm.md`](../../dev-notes/knowledge/libp2p-wasm.md) 那张表的**地基**：

> **「WebRTC 不受 mixed content 约束」** —— 写文件时只有间接证据，没有规范原文。
> 而「用户自建 relay 免域名、裸 IP 就能服务浏览器」的整个架构论据都压在它上面。

放 `spike/` 而非 `crates/`：根 `Cargo.toml` 里 `exclude = ["spike"]`。同 `spike/iroh-web`。

## 结论（2026-07-17，Chrome via agent-browser 0.26）

### 1. 两条 mixed content 断言 —— **都成立** ✅

2×2 矩阵，同一份 wasm、同一个 libp2p 节点，**唯一变量是页面协议**：

| 页面 origin | → `webrtc-direct`（裸 IP + certhash） | → `ws://`（私有 IP） |
|---|---|---|
| `http://192.168.50.105:8080` | ✅ RTT 300µs | ✅ RTT 500µs |
| **`https://192.168.50.105:8443`** | **✅ RTT 500µs** | **✅ RTT 200µs** |

**对照实验**（必须做，否则分不清「豁免」与「测试工具把 mixed content 关了」）——
从同一个 https 页面 `fetch`：

| 目标 | 结果 |
|---|---|
| `http://192.168.50.105:8080/`（私有 IP） | **放行** |
| `http://neverssl.com/`（公网） | **被拦**（`TypeError: Failed to fetch`） |

公网被拦 ⇒ mixed content **确实在生效**，`--ignore-https-errors` 没把它关掉
（它只跳过证书校验，origin 仍是 `https://`）。私有 IP 放行 ⇒ **豁免是真的**。

⇒ 知识库那张表的两条断言均获实证：
- **WebRTC 不受 mixed content 约束**（https 页面直拨裸 IP 通）
- **私有 IP 字面量豁免 mixed content**（https 页面 `ws://` 私有 IP 通，且对照证明豁免专属私有 IP）

### 2. 【意外·更重要】crates.io 上的 webrtc-direct 是坏的 ❌

**`libp2p-webrtc 0.9.0-alpha.1`（crates.io 最新）跑不通 webrtc-direct。**

症状：浏览器 ICE 能打通（服务端确实收到 `IncomingConnection`），但握手死在
`data channel opening took longer than 10 seconds`，拨号端报
`Failed to negotiate transport protocol(s): Timeout has been reached`。

隔离过程（每次只动一个变量）：

| 步骤 | 结果 | 排除了什么 |
|---|---|---|
| 官方 `examples/browser-webrtc`（master） | ✅ RTT 400µs | 环境/浏览器没问题 |
| 我的客户端只装 webrtc（去掉 `or_transport`） | ❌ 仍超时 | 不是 transport 组合 |
| 服务端也只装 webrtc（`SPIKE_WEBRTC_ONLY=1`，去掉 dns/ws） | ❌ 仍超时 | 不是我的配置 |
| **同一份代码换 git master** | **✅ RTT 400µs** | **⇒ 就是版本** |

上游 CHANGELOG 点名了修复 —— `transports/webrtc/CHANGELOG.md` 的 `0.10.0-alpha` 第一条：

> Update webrtc-rs to `v0.17` and **fix libp2p noise data channel negotiation**.
> See [PR 6429](https://github.com/libp2p/rust-libp2p/pull/6429)

**这比「webrtc-direct 是 alpha」严重一档**：不是「不稳」，是**已发布版本根本不通**。
今天要用就必须吃 git 依赖，等 `0.10.0-alpha` 发布才能回 crates.io。

## 跑

需要 `wasm-pack`、`rustup target add wasm32-unknown-unknown`，macOS 还要 `brew install llvm`（见坑 3）。

```bash
wasm-pack build --target web --out-dir static/pkg --out-name webrtc_direct_https_spike
cargo run                      # 打印两个 origin 与两条 multiaddr
```

然后浏览器开打印出来的地址。**先跑 `http://` 那个拿基线**，再跑 `https://`（自签证书告警点「继续」，
点过之后 origin 仍是 `https://`，mixed content 照常生效，不影响测试）。

环境变量：

| 变量 | 作用 |
|---|---|
| `SPIKE_LAN_IP` | 强制网卡。开发机常挂 Tailscale（`utun*`）/ 网桥（`bridge*`），**挑错网卡会得到假阴性** |
| `SPIKE_WEBRTC_ONLY=1` | 服务端只装 webrtc，对齐官方例子，用于隔离 |

## 踩过的坑

1. **`libp2p 0.56` 的 `websocket` feature 不开 `dns` 编不过** —— builder 的 websocket phase
   无条件引用 `libp2p_dns`，报 `cannot find module or crate libp2p_dns` +
   `no variant named Dns`。上游 feature bug。
2. **SwarmBuilder 的 phase 链有序**：`Provider → Tcp → Quic → OtherTransport → Dns → Websocket
   → Behaviour`。`with_other_transport(..)` 之后必须先 `.with_dns()` 才拿得到 `.with_websocket()`
   （`libp2p-0.56.0/src/builder/phase/other_transport.rs:82`），且后者是 **async**。
3. **Apple clang 没有 WebAssembly backend** —— `ring` 编不到 wasm。`brew install llvm` +
   `.cargo/config.toml` 指 `CC_wasm32_unknown_unknown`。同 `spike/iroh-web` 坑 1。
4. **rustls 的 `install_default()` 在这里是必须的，但在 iroh 那边无效** —— 依赖树里 ring 与
   aws-lc-rs 并存，axum-server 会在 unwrap 处 panic。**同一个报错、相反的解**：
   iroh 读的是 builder 字段（`spike/iroh-web` 坑 3），axum-server/rustls 读的是进程默认。
   别把两边经验互相套用。
5. **`or_transport` 要摊平两次** —— 两侧 Output 类型必须一致（各自 `.map` 成 `StreamMuxerBox`），
   而且摊平后 `OrTransport::Output` 仍是 `future::Either<A,B>`
   （`libp2p-core-0.43.2/src/transport/choice.rs:51`），同类型也不自动塌缩，要再 `.into_inner()`。
6. **wasm-pack 会往 `--out-dir` 塞一个内容为 `*` 的 `.gitignore`** —— 直接吐在 `static/` 会把
   `index.html` 一起吞掉（`git check-ignore` 实测）。故产物进 `static/pkg/`。
7. **`Multiaddr` 的 `Debug` 不带引号** —— 用 `{:?}` 生成 js 会得到
   `export const X = /ip4/...;`，无效语法，页面 import 直接炸。要 `"{}"`。
8. **libp2p 0.57（master）删掉了 `wasm-bindgen` feature** —— 改为按
   `cfg(target_family="wasm")` 自动生效。0.56 的写法搬到 master 会报
   `libp2p does not have that feature`。
9. **master 的 `libp2p-swarm` 把 `wasm-bindgen-futures` 精确 pin 成 `=0.4.58`** ——
   自己写 `"0.4"` 会解析到 0.4.76 然后 cargo 无解。跟着钉。
10. **`--ignore-https-errors` 不影响 mixed content** —— 它只跳过证书校验。上面的 fetch
    对照实验已证明（公网 http 仍被拦）。**但这一条必须每次实测，不能假设。**

## 没测的

- **Chrome LNA（Local Network Access）的权限提示**。Chrome 142 起 LNA 限制公网站点访问私有 IP，
  但官方明确 WebSocket/WebTransport/WebRTC **尚未纳入**（会「soon」纳入）。
  本 spike 的页面 origin 本身就在私有 IP 上（`https://192.168.50.105`），属 local→local，
  **不触发 LNA**。要测 LNA 得把页面挂到真正的公网 HTTPS origin 上。
- **Safari / Firefox**。以上全部是 Chrome 的行为。
- **公网裸 IP 的 webrtc-direct**（知识库表格右下角那一格）。本 spike 只验了私有 IP。
  但既然「WebRTC 不受 mixed content 约束」已成立，公网那格的推理前提已经坐实。
