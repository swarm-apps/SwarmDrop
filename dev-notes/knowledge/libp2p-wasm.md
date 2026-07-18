# libp2p 与 Web 端（wasm）

## 概览

2026-07-17 调研「rust-libp2p 能否编到 wasm，让 Web 端复用同一个 Rust 核心包」，
以及备选的「Web 用 js-libp2p、原生用 rust-libp2p，靠协议互操作」。

方法：54 个 agent 源码级调研（48 条结论，**对抗性核验推翻 13 条 / 27%**）+ 16 个本地编译探针。
源码快照：`rust-libp2p@93c5059`（2026-07-13）、`js-libp2p@fb9e8a7`、`libp2p/specs`。
**编译探针的结论优先于 agent 的源码阅读** —— 本文件里凡标「实测」的都是真跑过 `cargo check/build`。

存储侧的结论在 [storage-abstraction.md](storage-abstraction.md)，两份不重叠。

> **当前状态：未决策。** 本文件是调研记录，不是已批准的路线。
> 相关背景见 [为什么 SwarmDrop 暂不迁移 Iroh](../why-libp2p-not-iroh.md)（其论据 3 「Web 端更需要协议
> 互操作」方向正确，但推理需按本文件第一节修正）。

---

## 总判决

### 第一道门不是「js 还是 rust」，是「浏览器根本进不来」

**今天浏览器对我们的网络零可达入口**，且这一条与 js/rust 选型**完全正交** —— 任何 Web 方案都得先过。
它排在所有其他讨论之前。

- `libs/core/src/config.rs:180-183` —— 生产环境只 listen **裸 TCP**（`/ip4/0.0.0.0/tcp/0` +
  `/ip6/::/tcp/0`）。全仓 `with_listen_addrs` 的调用者**全是测试**，生产路径从不覆盖默认值。
  `.with_quic()`（`libs/core/src/runtime/node.rs:50`）只给出站拨号能力 —— 没有 `/udp/../quic-v1`
  listen 地址，QUIC 从不监听。
- `crates/core/src/network/config.rs:17` —— bootstrap 是裸 IP `/ip4/47.115.172.218/tcp/4001`。

浏览器拨不了裸 TCP，也拨不了裸 QUIC。

> **注意范围**：这道门锁的是**公网**入口。局域网是另一条完全独立、且门槛低得多的路 ——
> LAN helper 是我们自己的节点，加什么 listener 都行。见下方「[局域网路线](#局域网路线web-端最可行的起点)」。
> **别把本节的悲观结论套到局域网上。**

给浏览器开公网的门只有两个选项：

| 开门方式 | crate 成熟度 | 代价 |
|---|---|---|
| `/wss` | `libp2p-websocket 0.46.0` ✅ 稳定 | **需要域名 + CA 证书** |
| `/webrtc-direct` | crates.io 最新是 `0.9.0-alpha.1`，**实测跑不通**，必须用 git master 的 `0.10.0-alpha` ❌ | 免域名免证书，但**已发布版本是坏的**（见下），且**不在 libp2p facade 里**（`libp2p/Cargo.toml` 只有 `webrtc-websys`，原生 webrtc 要直接依赖） |

`specs/webrtc/webrtc-direct.md:16-19` 讲明了它存在的理由：

> **No need for trusted TLS certificates.** Enable browsers to connect to public server nodes
> without those server nodes providing a TLS certificate within the browser's trustchain.
> Note that we can not do this today with our Websocket transport as the browser requires the
> remote to have a trusted TLS certificate.

它的地址格式 `/ip4/1.2.3.4/udp/1234/webrtc-direct/certhash/<hash>` —— **裸 IP + 自签证书哈希**。

### `/webrtc-direct` 的不可替代性，精确地落在「用户自建**公网** relay + 浏览器」这一格

**先把话说准**：webrtc-direct 不是所有场景都必需 —— 局域网用 `ws://` 就够（私有 IP 豁免
mixed content，见「局域网路线」一节）。它不可替代的地方只有一处，但那一处很要命。

对**浏览器**客户端（原生客户端不受此约束）：

| relay 在哪 | `ws://` | `wss://` | `webrtc-direct` |
|---|---|---|---|
| **私有 IP**（LAN helper）| ✅ **实测通过** 私有 IP 字面量豁免 mixed content | ❌ CA 不给私有 IP 签证书，物理上不可能 | ✅ **实测通过** certhash |
| **公网裸 IP**（用户自建）| ❌ **实测被拦** mixed content（豁免只给私有 IP / `.local`）| ❌ **必须域名 + CA 证书** | ✅ certhash，裸 IP 就行（推理，见下） |

**已由 [`spike/webrtc-direct-https`](../../spike/webrtc-direct-https/) 实测坐实**（2026-07-17，Chrome）：

2×2 矩阵，同一份 wasm、同一个 libp2p 节点，唯一变量是页面协议：

| 页面 origin | → `webrtc-direct` | → `ws://` 私有 IP |
|---|---|---|
| `http://192.168.50.105:8080` | ✅ RTT 300µs | ✅ RTT 500µs |
| **`https://192.168.50.105:8443`** | **✅ RTT 500µs** | **✅ RTT 200µs** |

关键的**对照实验**（不做就分不清「豁免」与「测试工具关了 mixed content」）——
从同一 https 页面 `fetch`：`http://192.168.50.105:8080/`（私有 IP）**放行**，
`http://neverssl.com/`（公网）**被拦**。⇒ mixed content 确实生效，豁免确实专属私有 IP。

右下角那格（公网裸 IP + webrtc-direct）未直接实测，但其前提「WebRTC 不受 mixed content 约束」
已在左下角坐实 —— WebRTC 根本不走浏览器的 HTTP 栈，与目标 IP 是公是私无关。

**右下角那一格就是全部理由。** `crates/core/src/network/config.rs:26` 的
`custom_bootstrap_nodes` 允许用户自填任意节点，而 `why-libp2p-not-iroh.md:250` 把
「自托管基础设施」列为**一等模式**。于是：

- 不加 webrtc-direct ⇒ **每个想让自己的 relay 服务浏览器的用户，都得买域名 + 配
  Let's Encrypt**。自托管对 Web 端直接劝退。
- 加了 ⇒ VPS 上一个裸 IP 就够。

**配置面已经就绪**：`parse_multiaddrs`（`config.rs:59-71`）只要求地址含 `/p2p/<peer-id>`，
**不限制传输协议** —— 用户今天粘贴 `/ip4/1.2.3.4/udp/4001/webrtc-direct/certhash/<h>/p2p/<id>`
就能正常解析，只是拨号时因未注册 transport 而失败。**加 webrtc-direct 是纯 transport 层改动，
配置与 UI 零改动。**

### 代价：crates.io 上的 webrtc-direct **是坏的**，今天要用必须吃 git 依赖

**这比「它是 alpha」严重一档。** `libp2p-webrtc 0.9.0-alpha.1`（crates.io 最新）跑不通
webrtc-direct：浏览器 ICE 能打通（服务端确实收到 `IncomingConnection`），但握手死在
`data channel opening took longer than 10 seconds`。

[`spike/webrtc-direct-https`](../../spike/webrtc-direct-https/) 逐变量隔离后锁定是版本
（两端都降到与官方 `examples/browser-webrtc` 完全相同的配置仍失败，**同一份代码换 git master
立刻通**，RTT 400µs）。上游 CHANGELOG 点名了修复 ——
`transports/webrtc/CHANGELOG.md` 的 `0.10.0-alpha` 第一条：

> Update webrtc-rs to `v0.17` and **fix libp2p noise data channel negotiation**.
> See [PR 6429](https://github.com/libp2p/rust-libp2p/pull/6429)

⇒ **要 webrtc-direct 就得 `git = "https://github.com/libp2p/rust-libp2p", rev = "..."`**，
且**整个 libp2p facade 一起切**（webrtc-websys 从 facade 来，版本必须配对）。
等 `0.10.0-alpha` 发布才能回 crates.io。

切 git master 还会连带撞上（spike 已踩，见其 README）：
- **0.57 删掉了 `wasm-bindgen` feature**（改为按 `cfg(target_family="wasm")` 自动生效）——
  下方「libp2p 有一等公民的 `wasm-bindgen` feature」那条**只对 0.56 成立**
- master 的 `libp2p-swarm` 把 `wasm-bindgen-futures` **精确 pin 成 `=0.4.58`**，自己写 `"0.4"`
  会解析到 0.4.76 然后 cargo 无解

**这是真正要决策的风险点**，不是「alpha 心理障碍」。

### 这一格也是 iroh 与 libp2p 在自托管上的真实差距

**不要说「iroh 做不到这些」——那是错的。** iroh 的 relay **可以跑明文 HTTP**
（`RelayUrl` 不做 scheme 校验，`http://` 客户端自动降级到 `ws://`；见 `/iroh` skill 的
`07-configuration.md:443,455` 与 `01-concepts.md:307`）。所以**局域网那条路 iroh 一样能走**，
差别只是 iroh 永远经 relay、libp2p 可 browser→设备直连 —— 而局域网内绕一跳几乎无成本，
**不构成选型理由**。

真实差距只在右下角那一格，根因是**证书信任模型**：

- iroh 自建 relay 的推荐方案是「自签 TLS + 把自己的 CA 钉进去」（`CaTlsConfig`，
  `07-configuration.md:459`）。**这对原生客户端有效，对浏览器无效** ——
  浏览器只认系统/浏览器信任库，**没有任何 API 能让网页钉自定义 CA**。
- libp2p 的 certhash 把信任从 CA 体系搬进了 multiaddr 本身。

⇒ **iroh 让自建公网 relay 的用户必须有域名；libp2p 不必。**

**这条论据不在 [`why-libp2p-not-iroh.md`](../why-libp2p-not-iroh.md) 里，且比它的论据 3 更硬**
——它不依赖「Web 端能否直连」（两边都不能），只依赖证书信任模型。但它**只在「让用户自建」
这个产品前提下成立**：若只跑我们自己的公共 relay，买个域名即可，差别归零。

### 「Web 端永久 relay」的杀伤力被自建 relay 拆掉了

下一节会说明：Web 端在任何方案下都拿不到「浏览器 ↔ NAT 后设备」的直连，只能中继。
**这条结论本身没变，但它的后果变了。**

原本的反对理由是「所有 Web 流量压在我们的公共 relay 上 = 带宽成本」。但既然
`custom_bootstrap_nodes` 允许用户自填、且自托管是一等模式，那么：

**中继成本可以落在用户自己的 relay 上。** 「永久 relay」从「我们付不起的架构缺陷」
降级为「用户自行选择的部署形态」—— 与 LocalSend 类工具的用户预期一致。

⇒ **不要再把「Web 只能 relay」当作否决 Web 端的理由。** 真正要决策的是「吃不吃
webrtc-direct 的 alpha 风险」。

### 无论选哪条，Web 端都拿不到「浏览器 ↔ NAT 后设备」直连

这是本次调研对产品形态影响最大的一条：

| 场景 | 单核心包（rust wasm） | 两套实现（js-libp2p） |
|---|---|---|
| Web ↔ **公网**桌面 直连 | ✅ 桌面加 `/webrtc-direct` 监听 | ✅ 同 |
| Web ↔ **NAT 后**桌面 直连 | ❌ | ❌ **也不行** |
| Web ↔ Web 直连 | ❌ | ✅（非我们的场景） |

第二行的原因见下节「被推翻的旧认知」第 4 条：js-libp2p 的 `/webrtc` 要工作，**对端也得说
`/webrtc-signaling/0.0.1`**，而 rust-libp2p 零实现。换 js-libp2p 不自动换来直连。

### 与 Web 解耦、无论如何都该做的两件事

1. **`entity` 解绑 sea-orm runtime** —— 见 [storage-abstraction.md](storage-abstraction.md) 的「第 0 步」。
   实测零风险。
2. **`tokio` → `n0-future`**（36 处）—— 见下方「tokio 在 wasm 上」。原生侧类型等价
   （但**不是零成本**，见那一节的 ⚠️）。落地细节以 [iroh-migration.md](iroh-migration.md)
   的「n0-future 替换：现在做」为准。

这两件事是「wasm-ready」的前置，**不是「Web 上线」的前置**，做了不欠债。

---

## 局域网路线：Web 端最可行的起点

**跨网 Web 端的每一个难点，局域网都绕开了。** 而这恰好命中 SwarmDrop「跨网版 LocalSend」
定位里 **LocalSend 的那一半**。

| 跨网 Web 端的难点 | 局域网 |
|---|---|
| 公网 bootstrap 是裸 TCP，浏览器进不来 | **绕开** —— LAN helper 是我们自己的节点，加什么 listener 都行 |
| 浏览器无法 listen | **绕开** —— 浏览器主动拨 LAN helper |
| 需要 `/webrtc-signaling/0.0.1`（rust 零实现）| **绕开** —— 局域网内不需要打洞 |
| 需要域名 + CA 证书 | **绕开** —— `ws://` 私有 IP 豁免 mixed content（**实测**），局域网连 webrtc-direct 都不必 |

### 零件已经齐了

`InfrastructureMode::LanHelper`（`libs/core/src/config.rs:38-60`）已经提供：

- **强制 Kad Server 模式** —— `runtime/behaviour.rs:135-139`
- **Relay Server + 资源限额** —— `LanHelperConfig::relay_limits`
- **`announce_private_addrs`** —— 把私有 LAN 地址登记为可公告地址，供 relay reservation
  返回（`config.rs:53`）。没有这个，客户端会以 `NoAddressesInReservation` 拒绝。

**缺的只有一样：浏览器能拨的 listener。** 现在只监听裸 TCP。

### LAN helper 的真正价值是「发现锚点」，不是「中继」

局域网里每台设备都能监听，浏览器本可以直接拨目标。**浏览器真正做不到的是发现** ——
它没有 mDNS（发不了 UDP 多播，这是硬约束）。所以：

```text
浏览器 ──手动配置一个地址──▶ LAN helper
                              ├─ Kad Server：查到同网段其他设备的地址
                              └─ Relay Server：目标不能监听时兜底
```

这个区分直接决定工作量：

| 方案 | 谁要加浏览器可拨 listener | 流量 |
|---|---|---|
| 只当发现锚点 | **每台设备**都要 | 浏览器 ↔ 目标 直连 |
| **兼当中继（推荐起点）** | **只有 LAN helper** | 浏览器 → LAN helper → 目标 |

推荐第二行：工作量小一个数量级，代价是绕一跳 —— **而那一跳在局域网内，几乎无成本**。

### 浏览器侧的两道平台门（2026-07 的事实，会变）

**① mixed content —— 私有 IP 字面量豁免**

直觉上「HTTPS 页面拨 `ws://192.168.1.5` 必被 mixed content 掐掉」。
**但 Chrome 官方文档列出的豁免条件第一条就是**：

> The request hostname is a private IP literal (e.g., `192.168.0.1`).

> ✅ **已实测确认**（[`spike/webrtc-direct-https`](../../spike/webrtc-direct-https/)，2026-07-17，Chrome）：
> `https://192.168.50.105:8443` 页面拨 `ws://192.168.50.105:53185` **通，RTT 200µs**。
> 且做了对照 —— 同页面 fetch 公网 `http://neverssl.com/` **被拦**，证明 mixed content 确实生效、
> 豁免确实专属私有 IP，不是测试工具的假象。

⇒ **局域网场景下 `libp2p-websocket 0.46.0`（稳定）就够用，不必吃 webrtc-direct 的 alpha
（而且那个 alpha 的已发布版本还是坏的，见总判决）。** 这让局域网路线的成本大幅低于跨网。

**② Local Network Access（LNA）—— 今天不拦，但明确说了要拦**

Chrome 138 opt-in，**Chrome 142（2025-10-28）正式上线**，限制公网站点访问私有 IP
（`10.x` / `172.16.x` / `192.168.x` / `127.0.0.1` / `.local`）。

官方原文（<https://developer.chrome.com/blog/local-network-access>）：

> WebSockets (crbug.com/421156866), WebTransport (crbug.com/421216834), and WebRTC
> (crbug.com/421223919) connections to the local network are **not yet gated** on the LNA
> permission.

紧接着：

> we plan to ship Local Network Access for WebSockets, WebTransport, and WebRTC
> connections **soon**.

⇒ **今天能用，未来一定会弹权限提示。** 不是阻断，是用户点「允许」——
对 LocalSend 类工具用户本来就预期要授权，**但产品上要提前设计引导，不能等它突然冒出来**。
企业侧有 `LocalNetworkAccessAllowedForUrls` 策略可预授权。

### 页面必须是 HTTPS —— 别图省事让 LAN helper 托管网页

看起来「LAN helper 直接起个 HTTP 服务托管页面」最省事（`http://192.168.1.5:8080`），
**但那样页面不是 secure context，会丢掉 `crypto.subtle` 和 OPFS** ——
而 OPFS 正是 Web 端落盘与断点续传的地基（见 [storage-abstraction.md](storage-abstraction.md)）。

⇒ **页面照常从 `https://` 加载，只有 P2P 连接打到局域网 IP。** 这正好落进上面两道门的场景。

> ⚠️ **2026-07 实测坐实了这条，且踩了坑**：Web 壳端到端调试时用 `http://192.168.50.105:8080`（私网 IP over http）测，finalize 落盘**静默永久挂死**。浏览器探针一句话定位：
> ```
> isSecureContext: false   navigator.storage: undefined   crypto.subtle: undefined
> ```
> `write_opfs` 调 `navigator.storage.getDirectory()` 打到 **undefined**，web-sys 绑定的 `JsFuture` 永久 pending（不 resolve 不 reject）——**最坏的失败模式，无错误无超时**。换 `http://127.0.0.1` / `http://localhost`（**secure context 即使是 http**）立即通过，逐字节一致。
>
> **secure context 白名单**：`https://*`、`http://localhost`、`http://127.0.0.1`、`file://`。私网 IP over http **不在内**。
> **两条工程后果**：① 生产 Web 端必须 https 部署（自签 + 用户信任，或正式证书）；② `OpfsFileAccess` 这类碰 Web 平台 API 的端口实现，**构造时必须预检 `isSecureContext` / API 存在性并明确报错**，绝不能让 undefined 的 JsFuture 永久 pending（给每个 OPFS await 套 timeout 兜底）。
> ③ 连接不受影响——libp2p 的 Noise/blake3 在 wasm 用自带实现，**不依赖 `crypto.subtle`**，所以 http 私网 IP 下连接照通、只有落盘炸。这让根因更隐蔽（网络全绿、只有存储挂）。

### 实测进度

| # | 项 | 状态 |
|---|---|---|
| 1 | `ws://` 私有 IP 从 HTTPS 页面通不通 | ✅ **通**（[spike](../../spike/webrtc-direct-https/)，RTT 200µs；含 fetch 对照证明豁免专属私有 IP）|
| 2 | LNA 权限提示今天会不会弹 | ⬜ **未测** —— spike 的页面本身就在私有 IP 上（local→local，不触发）。要测得把页面挂到真公网 HTTPS origin |
| 3 | 浏览器经 LAN helper 的 relay 连第三台设备 | ✅ **通**（2026-07 Web 壳实测，`crates/web`）：浏览器 reserve circuit → 对端拨 circuit 地址被动接收；且**浏览器↔浏览器经 helper circuit 双向文件传输逐字节一致**（见下方「单核心包实证」）|
| 4 | certhash 能否跨重启稳定 | ✅ **通**（内核 `webrtc.rs` 测试 + 冒烟壳实测：同一持久化证书两次 bind 的 certhash 一致）|
| 5 | Safari / Firefox | ⬜ 未测。以上全是 Chrome / Chromium |
| 6 | **rust-wasm 单核心包端到端** | ✅ **通**（2026-07 里程碑）：浏览器跑与桌面**字面同一份** `swarmdrop-transfer`（offer 门控 / 256KiB 分块 / fetch_plan 续传 / **bao 逐块 Merkle 验签**），OPFS 落盘 716800 字节逐字节一致。**攻克代价见下节「五道运行时门」** |

---

## rust-wasm 单核心包的五道运行时门（编译期完全看不见）

> **这是本路线最硬的一手经验**（2026-07 Web 壳落地，十一轮真实浏览器实测剥出）。
> 核心教训:**「native 测试全绿 + 五 crate wasm 编译全过 + 控制面全通」= 零保证**。
> wasm 单线程 + Web 平台的运行时语义有五类陷阱,`cargo test`/`check-wasm` 一个都拦不到,
> 只能真实浏览器逐层剥(门 5 是自动化基准暴露的)。按我们踩到的顺序:

**门 1 — `std::time` 直接 panic**。`std::time::Instant::now()` 在 wasm 是
`time not implemented on this platform` 运行时 panic（不是编译错）。transfer 里 5 处
`Instant` 曾把 prepare 直接炸掉。**修**:一律 `n0_future::time::Instant`（native=tokio,
wasm=web_time）。**排查信号**:功能在某个用到计时的路径静默失败,console 有
`time not implemented` panic。

**门 2 — `futures::AsyncReadExt::split()` 的 reader half 在 wasm 不被唤醒**。
数据面两端 `channel.split()` + 并发读写,在 wasm 单线程下 reader half 收到字节后不唤醒读端
（native 多线程掩盖）。**判据**:能工作的路径(RPC/offer)全是**整条流顺序 read/write,从不 split**;
唯独卡住的路径 split 了。**修**:去掉 split,顺序读写（本就不重叠时 split 纯属多余）。

**门 3 — accepted 流跨任务 move 导致 lost-wakeup**。流在任务 A（Router handler）读了首帧,
再 move 给独立 spawn 的任务 B——B 首次 poll 前,muxer 已把后续帧的 wake 打给 A 的旧 waker,
B 注册新 waker 时事件已消耗,发送端不再有新字节 → **永久 Pending**。native 多线程时序掩盖。
**判据**:同一条流跨了任务边界(一个任务读、另一个任务接手)。**修**:**accepted 流不跨任务**——
在读首帧的同一任务里 await 到流生命周期结束（iroh「形状 A:在 accept 里跑完」)。
这也是更干净的架构。

**门 4 — Web 平台 API 的 secure-context gating**。见上「页面必须是 HTTPS」节:
`navigator.storage`(OPFS)/`crypto.subtle` 在非 secure context(http 私网 IP)**整个不存在**,
web-sys 绑定打到 undefined 的 `JsFuture` **永久 pending**。**判据**:碰 Web 平台 API 的路径静默挂死,
`isSecureContext` 探针一句定位。**修**:构造时预检 + 明确报错 + 每个 JS await 套 timeout。

**门 5 — web-time `Instant` 的原点是页面加载,`Instant - Duration` 开局即下溢 panic**。
门 1 换到 `n0_future::time::Instant`(wasm=web-time)后还有一层:native `Instant` 原点是系统启动
(uptime 几乎总够减),web-time 原点是 `performance.now()` = **页面导航时刻**——页面开了不足
N 秒就跑到 `now - N秒窗口`(如 progress 的滑动窗口 `now - SPEED_WINDOW`)时 `checked_sub`
为 None,web-time 直接 `expect` panic(`RuntimeError: unreachable`,console 里
`panicked at web-time-.../instant.rs`)。**为什么一直没炸**:人工实测时页面开了很久才点传输;
自动化 bench 秒开秒传,第一个 chunk 就炸——**时间原点类 bug 只有自动化才能稳定暴露**。
**修**:所有 `Instant - Duration` 一律 `checked_sub` 并处理 None(None = 窗口尚未填满,通常
直接跳过修剪)。**排查信号**:发送/接收在传输启动瞬间 panic `unreachable`,栈指向 web-time。

**共同的方法论**（这套调试值得复用):
- **穷举锚点 > 逐个假设**:卡点稳定后,在可疑路径**每个 await 前后**铺 `info!` 锚点,一轮实测
  「最后一条锚点」= 精确病灶。逐个假设验证会来回拉扯(我们前几轮就是)。
- **对照实验切分**:小文件 vs 大文件(切帧大小/流控假设)、换 origin(切 secure context)、
  native e2e vs 浏览器(切 wasm 特有 vs 逻辑)。
- **浏览器探针直插平台层**:绕开整个 rust 栈,`evaluate` 直接调 `navigator.storage.getDirectory()`
  /`window.isSecureContext`,一刀切分「环境 vs 代码」。门 4 就是这么一句定位的。
- **每层修复让卡点前移一步**是正确收敛的信号(offer 不通→首帧拉不到→拉到首帧→3 块全过→finalize)。

## ❌ 被推翻的旧认知

**这一节记的是本次调研中被证伪的说法。重新捡起其中任何一条都会导致错误决策。**

### 「libp2p 有 wasm transport 而 iroh 没有，所以浏览器直连这件事上两者是两个世界」—— 错

> 🔁 **这条在 2026-07 的 iroh 调研里已经写过了**，见 [iroh-migration.md](iroh-migration.md) 的
> 「**「留在 libp2p 就能给 Web 端直连」—— 对我们的技术栈不成立**」。本次调研有人**没先读知识库**，
> 从「rust-libp2p 有三个 websys transport 而 iroh 没有」这个真事实出发，重新推出了错误结论，
> 并在编译探针连续通过后更加确信。
>
> **教训**：`libp2p-wasm-*` crate 的存在是事实，「所以浏览器能直连」是幻觉 —— 中间隔着
> listen 能力和信令协议两道门。**编译通过对这两道门零信息量。**
> 动 P2P 选型 / Web 端前先跑 `/dev-workflow`（CLAUDE.md 的硬要求），别靠临场推理。

**是同一个世界。** rust-libp2p 编到 wasm，Web 端**全程走 relay，永久**，与 iroh 完全一致。

依据（三条，均已复核）：

1. 三个 websys transport 的 `listen_on` **全部无条件返回 `Err(MultiaddrNotSupported)`** ——
   无 cfg 分支、无 feature gate：
   - `transports/webrtc-websys/src/transport.rs:57-63`
   - `transports/websocket-websys/src/lib.rs:83-89`
   - `transports/webtransport-websys/src/transport.rs:56-62`
2. `transports/webrtc-websys/src/transport.rs:74-76` —— `dial` 直接拒绝 `role.is_listener()`，
   那正是 **DCUtR 打洞的 listener 侧**。⇒ 浏览器永远无法把 relay 连接升级成直连。
3. `/webrtc-signaling/0.0.1`（浏览器间直连协议）在 rust-libp2p 全仓 **0 命中**。

唯一的生机是「能收文件」：`protocols/relay/src/priv_client/transport.rs:131` 的 `listen_on`
**不走内层 transport** —— 它解析 relayed multiaddr 后发 `ListenReq` 让 behaviour **拨出**到
relay 做预留。所以浏览器虽开不了监听，仍能靠 circuit relay 被动可达。

### 「libp2p 的协议栈编过了 wasm，所以单核心包这条路成立」—— 编得过 ≠ 用得了

**协议层确实 100% 编过，卡死的是 transport 层。** 这两件事必须分开说。

实测（探针 4，`cargo build --target wasm32-unknown-unknown`）两端依赖树差集：

| 只在 wasm | 只在原生 |
|---|---|
| `webrtc-websys` `webrtc-utils` `websocket-websys` `webtransport-websys` | `mdns` `quic` `tcp` `tls` |

其余 18 个 crate 完全一致 —— `libp2p-kad` / `gossipsub` / `relay` / `dcutr` / `identify` /
`ping` / `request-response` / `stream` / `autonat` / `noise` / `yamux` 全部编过 wasm32。

**但编过的是 behaviour 层。** 上一条已说明 transport 层做不了 listen、做不了打洞。
引用「编译通过」论证「Web 可行」是本次调研最容易踩的逻辑滑坡。

### 「官方 CI 有 wasm `--all-features` job 且是绿的，所以 libp2p 全功能支持 wasm」—— 错，这个绿有毒

CI 确实存在且每个 PR 都跑（`rust-libp2p/.github/workflows/ci.yml:123-153`，`cross` job，
命令 `cargo check --package libp2p --all-features --target=wasm32-unknown-unknown`）。

**但 `--all-features` 在 wasm32 上名不副实**：`libp2p/Cargo.toml:126-135` 用
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` 把 dns/mdns/memory-connection-limits/
quic/tcp/tls/uds/upnp/websocket 共 9 个 crate 从 wasm 依赖图里摘掉了 —— **feature 被接受，
dep 根本不进图，于是「跳过」被记成了「通过」**。

引用这条 CI 作为传输层的证据是错的。

### 「Web 换成 js-libp2p 就能有浏览器 ↔ NAT 后桌面的直连」—— 错，rust 侧没有信令协议

`/webrtc` 浏览器间直连**在 libp2p 协议层面已定稿**（`specs/webrtc/webrtc.md`，
Candidate Recommendation / Active，r0 2023-04-12），且 `webrtc.md:15` 明确覆盖我们的场景：

> Note that _A_ and/or _B_ may as well be **non-browser nodes behind NATs** and/or firewalls.

机制（`webrtc.md:24` 起）：B 把 `/webrtc` 追加到 relayed multiaddr 上通告 → A 经 relay 建流跑
`/webrtc-signaling/0.0.1` 换 SDP → **打洞成功后是真直连，relay 只承担信令**（`webrtc.md:47`
成功后关掉信令流）。所以「浏览器不能 listen」不构成 b2b 障碍。

**但它要求两端都实现该协议**：
- js-libp2p ✅ 完整实现（`packages/transport-webrtc/src/constants.ts:89` 定义 `SIGNALING_PROTOCOL`；
  `private-to-private/` 下有 listener / initiate-connection / signaling-stream-handler；
  `index.ts:354` 同时导出 `webRTC` 与 `webRTCDirect`）
- rust-libp2p ❌ 零实现（全仓 grep `webrtc-signaling` 无命中；`transports/webrtc/src/lib.rs:21-22`
  自述 "WebRTC protocol **without a signaling server**"；`ROADMAP.md:58`/`:99` 的
  browser-to-browser 锚点悬空、章节不存在）

我们的桌面/移动端是 rust-libp2p。**要这条路，得自己在 rust 侧把 `/webrtc-signaling/0.0.1`
实现出来**（好消息：有定稿 spec + js 参考实现可对照移植）。

> ⚠️ 易误读：`ROADMAP.md:99` 那行 `Connectivity | Done | Q4/2022` 属于
> `### WebRTC support (browser-to-server)` 章节，"browser-to-browser" 只出现在 Dependents 列。
> 单看 grep 会误读成「b2b 已 Done」，实际相反。

### 「编 wasm 要裁掉 tcp/quic/mdns 这些 feature」—— 错，feature 与 target 正交

`libp2p/Cargo.toml:126-135`（已发布的 0.56 同样，见 `libp2p-0.56.0/Cargo.toml:282-315`）已经把
tcp/quic/mdns/dns/tls 放在 `cfg(not(target_arch = "wasm32"))` 下。**feature 照开，wasm 上自动消失。**

⇒ **不需要为两个 target 维护两套 feature 列表**。实测（探针 2-4）：`libs/core` 现有的 17 个
feature 原样不动，只在 wasm target 上追加 `wasm-bindgen` + 三个 websys transport 即可。

### 「sea-orm / SQLite 在浏览器里能用」—— 错

见 [storage-abstraction.md](storage-abstraction.md)。一句话：`libsqlite3-sys` 死在
`fatal error: 'stdio.h' file not found` —— wasm32-unknown-unknown **连 libc 都没有**。

---

## 编译实证

所有探针在 `/private/tmp/.../scratchpad/` 下，均可复现。

| 探针 | 目标 | 结果 |
|---|---|---|
| 1-4 | libp2p 全套 feature → wasm32 | ✅ build 通过；behaviour 层 18 crate 全编过 |
| 5 | `libs/core` 原样 → wasm32 | ❌ 7 error |
| 12 | `libs/core` + n0-future | ❌ 4 error（纯 sed 换 import 掉 3 个） |
| 13-14 | `libs/core` + n0-future + transport cfg 分叉 | ✅ **0 error** |
| — | 同一份代码 → 原生 | ✅ 0 error |

**`libs/core` 距离编到 wasm 只有 7 处改动**（1518 行 crate）：

| 错误 | 数量 | 位置 | 解法 |
|---|---|---|---|
| `tokio::time` 找不到 | 3 | `pending_map.rs:9`、`client/data_channel_open.rs:123`、`client/req_resp.rs:46` | 换 `n0_future::time`（纯 import 替换）|
| `libp2p::mdns` | 2 | `runtime/behaviour.rs:5`、`runtime/event_loop.rs:349` | cfg |
| `libp2p::tcp` | 1 | `runtime/node.rs:2` | cfg |
| `SwarmBuilder::with_tokio` | 1 | `runtime/node.rs:44` | 换 `.with_wasm_bindgen()` |

`tokio::sync::mpsc` **原样编过** —— wasm 上 tokio 的 `sync` 可用，只有 `net`/`time` 不行，
切分粒度可控。

---

## wasm 编译的坑（文档都不会告诉你）

### libp2p **0.56** 有一等公民的 `wasm-bindgen` feature（0.57 已删）

> ⚠️ **仅限 0.56**。master（0.57）**移除了这个 feature**，改为按 `cfg(target_family="wasm")`
> 自动生效。0.56 的写法搬到 master 会报 `libp2p does not have that feature`（spike 实测）。

`libp2p-0.56.0/Cargo.toml:141`：

```toml
wasm-bindgen = [
    "futures-timer/wasm-bindgen",
    "getrandom/js",
    "libp2p-swarm/wasm-bindgen",
    "libp2p-gossipsub?/wasm-bindgen",
]
```

一把解决 getrandom/js、swarm 的 `WasmBindgenExecutor`（`libp2p-swarm-0.47.1/src/executor.rs:49-53`
= `wasm_bindgen_futures::spawn_local`）、gossipsub 的定时器。

### 两个 getrandom 版本并存，各要各的开关，libp2p 只管了 0.2

| 版本 | 来源 | 开关 |
|---|---|---|
| 0.2 | libp2p facade 直接依赖 + `rand_core 0.6`（snow / ed25519-dalek 链路）| `js` feature —— **libp2p 的 `wasm-bindgen` 已传递** |
| 0.3 | `rand 0.9` 链路 | `wasm_js` feature **加** `--cfg getrandom_backend="wasm_js"` rustflag —— **必须自己直接依赖并开启** |

两个报错长得不一样（0.2 说 `you may need to enable the "js" feature`，0.3 说
`The "wasm_js" backend requires the wasm_js feature`），修好一个会以为完事了。

**正确做法**（wasm target 段）：

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
libp2p = { version = "0.56.0", features = [
    "wasm-bindgen", "webrtc-websys", "websocket-websys", "webtransport-websys",
] }
getrandom = { version = "0.3", features = ["wasm_js"] }
```

配合 `.cargo/config.toml`：

```toml
[target.wasm32-unknown-unknown]
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
```

### 没有 `with_websocket_websys()` 这种便捷方法

`SwarmBuilder` 在 wasm 上只有 `.with_wasm_bindgen()`（`libp2p-0.56.0/src/builder/phase/provider.rs:41`）。
**websys transport 一律走 `with_other_transport` + 手动 upgrade/authenticate/multiplex。**

权威写法照抄官方 `interop-tests/src/arch.rs:245-260`（那是真跑 CI 的组合）：

```rust
#[cfg(target_arch = "wasm32")]
let builder = SwarmBuilder::with_existing_identity(keypair)
    .with_wasm_bindgen()
    .with_other_transport(|local_key| {
        Ok(libp2p::websocket_websys::Transport::default()
            .upgrade(libp2p::core::upgrade::Version::V1Lazy)
            .authenticate(noise::Config::new(local_key)?)
            .multiplex(yamux::Config::default()))
    })?;
```

（需 `use libp2p::core::transport::Transport as _;` 才能调 `.upgrade()`。）

### Apple clang 没有 WebAssembly backend

`ring`（`tls-ring` / sea-orm 的 `runtime-tokio-rustls` 都会带来）要把 C 编到 wasm 必挂。
同 `spike/iroh-web` 的坑 1：`brew install llvm`，并在 `.cargo/config.toml` 显式指定：

```toml
[env]
CC_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/clang"
AR_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/llvm-ar"
```

### `webrtc-websys` 在 Web Worker 里会 panic

`transports/webrtc-websys/src/transport.rs:116` 的 `web_sys::window().expect(...)` ——
想把传输放进 Worker 避免阻塞主线程这条路不通。**只有 `websocket-websys` 支持 Worker**
（它有 `web_context.rs` 做 window/worker 分支）。

### `/private/tmp` 下跑原生 cargo 会被 Gatekeeper 拦

macOS 拒绝 dlopen `/private/tmp` 下的 proc-macro dylib
（`library load disallowed by system policy`）。表现为莫名其妙的 rustc exit 101。
scratchpad 里做原生构建实验时用 `CARGO_TARGET_DIR` 指到别处。

### OPFS 接收落盘：流式 positioned write 的正确姿势（主线程版）

接收侧大文件落盘用「`createWritable` 句柄常驻 + 每 chunk positioned write + `close` 提交」，
不要整文件内存缓冲（demo 初版就是这么 OOM 的）。

**正确做法**：
- 开句柄用 `create_writable_with_options(FileSystemCreateWritableOptions)`：
  `keep_existing_data=false` 打开即截断（全新文件）、`true` 保留已有字节（断点续传——
  positioned write 只覆盖写到的 range）。feature：`FileSystemCreateWritableOptions`
- 每 chunk 用 `write_with_write_params(WriteParams)`（WHATWG `{type:"write", position, data}`）
  **单次 Promise 完成 seek+write**；别手写 `seek()` + `write()` 两次往返（热路径调度开销翻倍）。
  feature：`WriteParams` + `WriteCommandType`（`type` 是 spec required 字段，
  `WriteParams::new(WriteCommandType::Write)`；position 是 `Option<f64>`）
- `close()` 才提交落盘（writable 是 staging 语义）；取消/失败直接 drop 句柄 = 丢弃未提交写入，
  正是想要的行为
- SendWrapper 兜 Send 有**两种合法裹法**（模块 doc 已写明）：短路径在 scope 内取 Promise 即丢、
  只让 `SendWrapper<JsFuture>` 跨 await；多步 helper（如 open_writable：建目录链→取文件句柄→
  createWritable）则**整段 async fn future 裹 SendWrapper**，内部 !Send 句柄随包一起被兜

**不要做**：
- 别用 `SyncAccessHandle`——Worker-only，与 webrtc-websys 主线程约束冲突（见上一条 panic 条目）；
  ws-only + Worker 的 bundle 才轮到它
- 别依赖 staging 数据活过页面刷新——`close` 前的写入刷新即丢，`keep_existing_data` 只保护
  已 close 的字节（所以跨刷新续传不在主线程版范围内）

**相关文件**：`crates/web/src/file_access.rs`

---

## tokio 在 wasm 上：用 n0-future

**结论：换。理由与 iroh 无关。**

`n0-future` 是 n0（iroh 组织）维护的通用垫片，**不拖 iroh 进来**。关键在它的设计：

```rust
// n0-future-0.3.2/src/time.rs:3-10
#[cfg(not(wasm_browser))]
pub use tokio::time::{interval, interval_at, sleep, sleep_until, timeout,
                      Duration, Instant, Interval, MissedTickBehavior, Sleep, Timeout};
// task.rs:5-9
#[cfg(not(wasm_browser))]
pub use tokio::spawn;
pub use tokio::task::{AbortHandle, Id, JoinError, JoinHandle, JoinSet};
```

**原生侧它就是 `pub use tokio::*`** —— 桌面/移动端跑的还是 tokio 原物，**类型等价、源码级零改动**。
wasm 侧才换成 API 兼容的重实现（`web-time` + `wasm-bindgen-futures` + `send_wrapper`）。

`wasm_browser` 是 build.rs 里的 cfg_alias（`= all(target_family = "wasm", target_os = "unknown")`），
**自动生效，不用手配 rustflag**。

> ⚠️ **别说「零成本」**。`n0-future-0.3.2/Cargo.toml:102-108` 在非 wasm target 下声明
> `tokio = { features = ["rt", "time", "macros", "test-util"] }` —— **`test-util` 是无条件开的**，
> 而 Cargo 的 feature unification 会把它传染给整个构建的 tokio。
> 「类型等价」成立，「零成本」不成立。同一警告见 [iroh-migration.md](iroh-migration.md) 的
> 「n0-future 替换：现在做」一节 —— 那条更早、更细，**动手前以它为准**。

### 为什么不是 futures-timer

libp2p 已经把 `futures-timer 3.0.4` / `gloo-timers 0.4.0` / `wasm-bindgen-futures` / `web-time`
带进 wasm 树了（零新依赖）。但 `futures-timer` 只给 `Delay`，**没有 `Interval`、没有
`MissedTickBehavior`**。

我们的 3 处 `interval` 里有 2 处调了 `set_missed_tick_behavior(MissedTickBehavior::Delay)`
（`crates/core/src/infra/supervisor.rs:218`、`presence/supervisor.rs:499`）—— tokio 独有 API。
n0-future 把它补齐了（`time.rs:249` 的 `MissedTickBehavior` enum + `:402` 的 setter），
这 2 处是纯 import 替换；走 futures-timer 则要手写重造。

### 用量盘点（迁移风险低于预期）

| API | 数量 | 备注 |
|---|---|---|
| `tokio::spawn` | 22 | **全部 fire-and-forget** —— 零 `JoinHandle`、零 `.abort()` |
| `tokio::time::timeout` | 5 | |
| `tokio::time::interval` | 3 | 其中 2 处用 `set_missed_tick_behavior` |
| `tokio::time::sleep` | 1 | |

合计 36 处（`libs/core` 8 + `crates/core` 28）。分布：`presence/supervisor.rs` 11、
`infra/supervisor.rs` 5、`transfer/wire/data_plane.rs` 3、`network/event_loop.rs` 3、其余 ≤2。

**22 处 spawn 全是 fire-and-forget** 这一点很关键 —— `wasm_bindgen_futures::spawn_local` 返回
`()`，没有 JoinHandle 语义，正好是 drop-in。

---

## 走 js-libp2p 的真实成本

这一维 8 条结论**全部通过对抗性核验，零推翻**。

### `request-response` 在整个 js-libp2p 仓零命中

我们的控制面协议（配对 + 传输协商 + 续传探测）在 Web 上必须手搓。
`@libp2p/fetch` 是写死语义的 key→value 协议（proto 仅 `bytes identifier` + `bytes data`），
`@libp2p/echo` 是 echo 协议 —— 都不是可插 codec 的通用抽象。

（收窄表述：js-libp2p 有贯穿约 20 个包的 request-response **惯用法**，但没有打包成命名抽象。）

### 更麻烦的是字节格式：`Uuid` 上线是 16 字节裸 bytes，不是字符串

`cbor4ii` 的 `is_human_readable` 在 ser/de 两侧都硬编码 `false`
（`cbor4ii-0.3.3/src/serde/ser.rs:278-280`、`de.rs:309-311`；**1.2.2 在同样行号仍是 false，
升级躲不掉**）。`uuid` 据此走 `serialize_bytes` 分支（`uuid-1.23.4/src/external/serde_support.rs:28`）。

我们的 `session_id: Uuid` 遍布 `crates/core/src/protocol.rs`（156/163/167/171/175/209/213/218）。
除 Uuid 外还有：`[u8;32]` 的自定义 `serialize_bytes`（`protocol.rs:225-235,237-260`）、
内部 tag 枚举 `tag="kind"`/`"type"`（`:153,197,263,270`）、snake_case 与 camelCase 混用
（`:101,130,186`）、`Vec<(u64,u64)>` 元组数组（`:125`）。

**TS 侧要逆向一个从未文档化的隐式格式，且 rust 侧改一行 `protocol.rs` 就可能悄悄破坏兼容。**

分帧本身反而简单：`rust-libp2p/protocols/request-response/src/cbor.rs:143-158` 写完 CBOR 直接
`write_all` 半关闭，`:117-128` 读端 `io.take(max).read_to_end` 读到 EOF —— 无长度前缀，
TS 里 10 行能复刻。**成本全在 body 的字节兼容性。**

### noise / yamux 不在 js-libp2p 仓内

是外部 ChainSafe 包 `@chainsafe/libp2p-noise ^17.0.0` / `@chainsafe/libp2p-yamux ^8.0.0`
（`packages/integration-tests/package.json:41,52`）。**libp2p 主包不依赖它们**
（`packages/libp2p/package.json` grep chainsafe 只有 is-ip/netmask），需使用者自行装配。
⇒ Web 端的加密/多路复用由第三方组织另仓维护，版本线独立，多一条兼容轴。

### 上游不帮我们守 js↔rust 回归

- `packages/interop`（`@libp2p/interop`）**只测 js↔go**：`src/index.ts:57` 写死
  `NodeType = 'js' | 'go'`，`src/utils/test-matrix.ts:10` 同样写死，整包 grep rust 零命中。
- js↔rust 互测在**外部 `libp2p/test-plans` 仓**编排；`js-libp2p/interop/` 与
  `rust-libp2p/interop-tests/` 只是各自的 docker 适配器。
- **该套件里 QUIC 被注释掉了**（`interop/test/fixtures/get-libp2p.ts:3`），js 侧 QUIC 还依赖外部
  `@chainsafe/libp2p-quic`。被验证过的组合实际只有 tcp×(yamux|mplex)×(noise|tls) + 浏览器系的
  ws/wss/webtransport/webrtc-direct。

### 浏览器永远只能是 DHT client

`packages/kad-dht` 能在浏览器跑（有 chrome/firefox/webworker 测试目标），但 server 模式仅在节点
被判定为公网可拨时才开。⇒ 浏览器能发起 put/get/getProviders，**不承担 DHT 存储**。

对照我们自己的注释 `libs/core/src/runtime/behaviour.rs:127-129`（「若 AutoNAT 未确认…停留在
Client 模式…导致 put_record 等操作因 QuorumFailed 失败」）—— 分享码的 `put_record(Quorum::One)`
依赖网络里有足够 server 节点应答，**Web 端只能寄生于现有 server 节点**。

### 工作量

约 **1 万行**生产级 Rust 要 TS 重写（仅 `transfer/` 就 5747 行），且 **2693 行 Rust 测试带不走**。

---

## rust-in-browser 的最强实证

`rust-libp2p/.github/workflows/interop-test.yml:12-44` —— matrix 含 `flavour: [chromium, native]`，
rust 编到 wasm32 → wasm-pack 打包 → 由原生 harness 经 chromedriver 拉起
`selenium/standalone-chrome:125.0` 在**真实 Chrome** 里执行，并经 `libp2p/test-plans` 的
`run-transport-interop-test` action 与全生态版本对拨（test-filter `chromium-rust-libp2p-head`）。

**这是「rust-libp2p 在浏览器里真跑起来并与 js-libp2p 互通」的最强证据。**

⚠️ 但它**只覆盖 ping + identify** —— 不含 kad/gossipsub/relay/dcutr/request-response。
协议层的浏览器互通仍是未验证区。

---

## 未决 / 待查

- ~~**WebRTC 是否真的不受 mixed content 约束**~~ —— ✅ **已由 [`spike/webrtc-direct-https`](../../spike/webrtc-direct-https/) 实证**（2026-07-17，Chrome）。
  连带证实了「私有 IP 字面量豁免 mixed content」。**同时挖出 crates.io 版本跑不通**（见总判决）。
- **Safari 对 WebTransport 的支持**：三个本地仓内零证据（`rust-libp2p` 全仓无 "safari"；
  `specs/webtransport/README.md` 全文无 Safari 且 revision 停在 r0, 2022-10-12）。
  需查 caniuse/MDN 等实时来源，**不要用训练数据补**。
- **Safari / Firefox 的 LNA 对等物**：上面 LNA 的事实全部来自 Chrome 文档。
  其余浏览器的私有 IP 策略未查。
- **中国大陆能否连上 relay**：`spike/iroh-web` 的 relay 实测出口 IP 在东京，量的是「东京→新加坡」，
  不构成「国内可用」证据。此问题对 libp2p relay 同样存在。
- **Web 端能接受「永久 relay」吗**：这是产品问题，决定要不要投入实现
  `/webrtc-signaling/0.0.1`。能接受 → 单核心包（rust wasm）已验证可行且比 js 少养一套实现。

## 相关文件

- `libs/core/src/config.rs:180-183` —— listen 地址默认值（Web 公网零入口的根因）
- `libs/core/src/config.rs:38-60` —— `InfrastructureMode::LanHelper` / `LanHelperConfig`
  （局域网路线的零件：Kad Server + Relay Server + `announce_private_addrs`）
- `libs/core/src/runtime/node.rs:43-53` —— transport 构建（wasm 需 cfg 分叉的主战场；
  加 webrtc-direct 也在这里）
- `libs/core/src/runtime/behaviour.rs:50,146` —— mdns（wasm 上需 cfg）
- `libs/core/src/runtime/behaviour.rs:135-139` —— LAN helper 强制 Kad Server 模式
- `crates/core/src/network/config.rs:15-19` —— bootstrap 裸 IP 地址
- `crates/core/src/network/config.rs:26` —— `custom_bootstrap_nodes`（用户自填节点，
  webrtc-direct 论据的产品前提）
- `crates/core/src/network/config.rs:59-71` —— `parse_multiaddrs`：**不限制传输协议**，
  加 webrtc-direct 无需改配置面
- `crates/core/src/protocol.rs` —— CBOR 控制面协议（js 重写的字节兼容性风险源）
- `spike/iroh-web/` —— 浏览器 wasm 的既有工程范例（wasm-pack + `--weak-refs` + llvm 配置）
- `/iroh` skill 的 `07-configuration.md:443,455,459` + `01-concepts.md:307` ——
  iroh relay 的 scheme/证书行为（「iroh 与 libp2p 自托管差距」一节的依据）
