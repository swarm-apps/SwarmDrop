# iroh 迁移评估

## 概览

2026-07 对 [iroh](https://github.com/n0-computer/iroh) 1.0.2 生态做了两轮源码级调研（48 个 agent，
源码快照 `/Volumes/yexiyue/iroh-study`，覆盖 24 个仓），评估 SwarmDrop 从 libp2p 迁移到 iroh 的可行性与代价。

产出分两处，**职责不重叠**：

| 位置 | 内容 |
|---|---|
| `/iroh` skill | 通用 iroh 知识，两类问题一并回答：**选型**（某个 crate 是什么、成熟吗、代价是什么 → `ecosystem-map.md`）+ **写码**（Endpoint/Router/Watcher/流原语/ALPN/feature flags） |
| **本文件** | **只记对 SwarmDrop 的结论**——我们的代码现状、我们的取舍、我们的待办 |

**通用 iroh 知识一律查 `/iroh` skill，不在这里复制。** 本文件的每条结论都指明依据（我们的
`file:line`，或 skill 里的哪一节），因为三个月后没人记得当时凭什么这么判。

> **当前状态：评估结论，尚未决策迁移。** 除「n0-future 替换」标为「现在做」外，其余都是
> 迁移发生时才生效的判断。带 ⚠️ 标注的条目是**建议/待评审**，不是从源码验证出的事实。

调研中驳回了 42 条 claim、修正了 167 条。被推翻的旧认知单列一节 —— **那一节是本文件最重要的部分**，
因为它防的不是「不知道」，是「知道了错的还很确信」。

---

## 总判决

### 一句话选型：要 bao-tree，暂时不要 iroh-blobs

- **bao-tree** 是纯算法 crate，**不依赖 iroh 网络栈**，精准补上我们唯一真实缺失的能力（逐块验签），
  代价约 0.39% 存储，可增量塞进现有传输层、不动协议骨架 → **低风险、可回退**。
- **iroh-blobs** 是整套内容寻址 blob store，能力远超我们的需求，且 README 自述非生产质量
  + 三条结构性冲突（见下「push/offer 模型」「加密冲突」「Web 端无持久化」）+ 已验证的 panic 路径
  → **留到 1.0 且 issue #233/#207 关闭后再评估**。

**依据**：bao-tree/iroh-blobs 的客观属性见 `/iroh` 的 `blobs-and-file-transfer.md`。此处是**我们的**
判决 —— 「能力远超需求」的「需求」是我们的需求，换个要做多源下载或跨会话去重的项目结论会相反。

### 最低风险迁移路径 = dumbpipe 形状，不是 sendme 形状

保留 `crates/core/src/transfer/` 全部 6789 行，只把 libp2p Stream 换成 iroh 的
`SendStream`/`RecvStream`、ProtocolName 换成 ALPN。可先验证 NAT 穿透率与 uniffi/Tauri 集成，
再决定是否深入 iroh-blobs。

**对照**：sendme 形状要吞下 iroh-blobs 全栈（非生产质量）。dumbpipe 形状的代价是
chunk/续传/校验全自理 —— **但这些我们本来就有**。

**依据**：两个官方样本的形状对比见 `/iroh` 的 `blobs-and-file-transfer.md`。

---

## ❌ 被推翻的旧认知

**这一节记的是调研中被证伪的说法。重新捡起其中任何一条都会导致错误决策。**

### 「我们已经在用 blake3 了，所以不缺 iroh-blobs 的校验能力」—— 错

**用的是同一个哈希函数，但不是同一个东西。** verified streaming 的价值不在哈希算法，
而在 **outboard 这棵 Merkle 树**。

依据（实测）：

```
grep -rniE "bao|outboard|merkle" crates/core/src/   →  0 命中
```

我们的 blake3 是**扁平整文件 hasher**：
- `transfer/flow/prepare.rs:40` 逐 chunk `hasher.update(&chunk)` 汇入单个 hasher，`:76` 一次 `finalize()`
- `transfer/wire/data_frame.rs:102` 的 `manifest_digest()` 同样是扁平 hash
- `transfer/wire/crypto.rs:77` 的 `blake3::derive_key("swarmdrop-transfer-nonce-v1", ..)` 是 nonce KDF，
  根本不是文件 hasher
- `database/inbox.rs:507` 是第四处，与传输校验无关

**后果**：没有 outboard，收到的每个 chunk 在文件收完前都无法验证，
**续传只能建立在「信任对端」之上 —— 我们现在正是如此。** 这是整份调研里唯一被认定为
「真正的能力差」的一条，也是引入 bao-tree 的核心动机。

### 「我们是线性 offset checkpoint，迁 iroh 前要先改 range set」—— 错

**我们早就是 range set 模型。** 这条常被写反。

依据：
- `crates/entity/src/transfer_file.rs:27-35` —— `completed_chunks: Vec<u8>`（bitmap）+
  `completed_ranges: String`，doc 明写「**新数据面以 range 为 checkpoint 事实源**；
  bitmap 仅作为旧拉取实现和过渡适配」
- `crates/core/src/database/ops.rs:167` —— `update_file_checkpoint_ranges(.., completed_ranges: &[(u64,u64)], ..)`
- `crates/core/src/transfer/flow/resume/plan.rs:56` —— `build_fetch_plan(manifest, checkpoint)`
  是「清单减去已有」的**集合减法**

**`build_fetch_plan` 与 iroh 的 `local.missing() = root_requested - &self.bitfield.ranges`
是同一个心智模型。** 所以：迁 iroh-blobs 在续传上**换不来新能力**，只换来「range 可验证」——
而那正是 bao-tree 单独就能给的。

（`plan.rs:29-31` 确有一条 `transferred_bytes` 单连续 range 的过渡 fallback，仅在
`parse_completed_ranges` 为空时生效，不构成「线性单点模型」的定性。）

### 「push 模型必须两端各自维护 session + checkpoint，会有不一致」—— 错

**我们的发送端根本不写 checkpoint。**

依据（实测）：

```
grep -rn "update_file_checkpoint_ranges" crates/ src-tauri/src/
  → 定义 database/ops.rs:167
  → 测试 tests/e2e_transfer.rs:1156
  → 唯一调用点 transfer/actor/receiver.rs:457

grep -rnE "checkpoint|completed_ranges|completed_chunks" crates/core/src/transfer/actor/sender.rs
  → 0 命中
```

entity doc（`transfer_file.rs:30`）也写死「仅接收方使用，发送方为空 vec」。

**所以「发送端认为发到 5MB、接收端只落盘 3MB」这个不一致场景在本仓不存在**，
评估 push vs pull 时不要把它算进 push 模型的成本。

### 「迁 iroh 能省掉几千行 transfer 代码」—— 错

别被「1184 行（sendme）vs 6789 行（我们）」这种对比带节奏。**sendme 的对应物是
iroh-blobs 的 23457 行，不是 0。** 而且 sendme：

- **没有** offer/accept 门控（EventMask 只用 Notify —— 任何人拿到 ticket 即可下载）
- **没有** 收件箱、没有历史、没有多会话并发管理
- **不是** push 模型

**差别不在总量，在这些代码是 n0 维护还是我们维护。**

依据：`find crates/core/src/transfer -name '*.rs' | xargs wc -l` → 23 文件 6789 行（已复核）。

### 「SwarmDrop 手写的 relay/DCUtR 编排在 iroh 下全部可删」—— 错，那个编排不存在

DCUtR 是 libp2p 的 behaviour，我们只是**开关**它（`libs/core/src/config.rs:234`
的 `with_dcutr(bool)`，生产配置在 `crates/core/src/network/config.rs:92` 一行
`.with_dcutr(true)`）。仅有的相关代码是**观察**而非编排：

- `crates/core/src/device_manager.rs:156` —— 收到 `NodeEvent::HolePunchSucceeded` 时置
  `entry.hole_punched = true`
- 该字段的全部消费点是 `:195` / `:234` → `connection_info(..., hole_punched)`，
  用途只是把连接类型标成 `ConnectionType::Dcutr`（`crates/core/src/device.rs:360`）给 UI 显示

**迁移的实际影响是「把 ConnectionType 的显示层从 `hole_punched` 标志改写成读 `conn.paths()`
的 `is_relay()`」—— 是移植，不是删除。净收益比直觉小得多。**

> 📌 修正：调研初稿称 `with_dcutr` 「仅见于 tests」，实测不成立 —— 生产
> `network/config.rs:92` 就在用。但结论（无编排可删）不受影响。

### 「libp2p 缺了要 install_default()，iroh 才要注入 crypto provider」—— 错

**libp2p-tls 也不读进程默认 provider。** 核对 libp2p-tls 0.6.2（我们锁定的版本）：

```rust
:48  let mut provider = rustls::crypto::ring::default_provider();   // ← 构造函数，不是 get_default()
:56  ClientConfig::builder_with_provider(provider.into())
:75  let mut provider = rustls::crypto::ring::default_provider();
:83  ServerConfig::builder_with_provider(provider.into())
```

**真实差异**：libp2p-tls 把 ring **硬编码在内部**（我们根本无从提供 provider，也无从换
aws-lc-rs），iroh 则要求经 builder 显式注入。两者都不读进程默认。

**对我们的迁移含义**：现在这层是焊死的、上层无从注入；迁 iroh 后 provider 变成**必填项**，
`crypto_provider()` 漏了会在 `bind()` 直接 `InvalidCryptoProvider`。

### 「留在 libp2p 就能给 Web 端直连」—— 对我们的技术栈不成立

iroh 在浏览器里 100% 走 relay（wasm 下 `mod ip` 整个不编译，没有直连、没有打洞）。
由此容易反推「那不迁不就有 Web 直连了」。**不成立**：

- **js-libp2p** 有 `@libp2p/webrtc` 可做 browser-to-browser，但**仍需 circuit-relay 做 SDP 信令**
- **WebTransport 在浏览器不能 listen**，物理上做不了 browser↔browser 直连
- **我们用的是 rust-libp2p**（`libs/core/Cargo.toml:7` `libp2p = { version = "0.56.0" }`），
  rust-libp2p 的 webrtc 是 **browser→server 的 webrtc-direct，没有 browser-to-browser**

**所以「Web 端中继成本无法回避」在 iroh 侧成立，而「libp2p 能给你 Web 直连」这条退路
对我们的技术栈根本不存在** —— 它不构成留在 libp2p 的理由。

### 「iroh-blobs 有 2GB OOM / issue #84 #90 零 PR」—— 查无实据

调研未能证实。**别把它当作否决 iroh-blobs 的理由** —— 真正的否决理由是下面那三条结构性冲突，
它们各自都站得住，不需要靠这条。

（Web 端要盯的是 **issue #86**（可插拔持久化后端 PR，未合，updated 2026-05-27）
和 **#207**（WASM + irpc 组合仍坏），**不是 #90** —— #90 对应的 PR #187 已于 2025-11-06 合并，
那只带来了「能编译」。）

---

## 传输层

### 真正的能力差只有一个：Merkle 树 / 逐块验签

见上「被推翻的旧认知」第一条。**这是 iroh-blobs / bao-tree 相对我们最实质的差距，
而不是省几行代码。**

### 引 bao-tree 的落地路径

**⚠️ 建议方案，未实施。**

- 引 `bao-tree`（`default-features = false, features = ["validate"]`）
- 把 256 KiB 无校验 chunk（`crates/core/src/transfer/mod.rs:22` `CHUNK_SIZE`）换成
  **16 KiB 可验证 chunk group**
- outboard 存进现有 SQLite checkpoint 表（bao-tree 的 `Outboard` trait 支持自定义后端）

**配套要先拍的板**：发送端对已完整的文件建 outboard → `PreOrderOutboard` 即可；
接收端边收边写、文件在增长 → `PostOrderOutboard` 的 Stable/Unstable 语义能避免每次追加
都重写整个 outboard，**直接关系到续传时 outboard 的写放大**。

**相关文件**：`crates/core/src/transfer/mod.rs`（CHUNK_SIZE）、
`crates/core/src/transfer/flow/prepare.rs`、`crates/core/src/database/ops.rs`

### XChaCha20-Poly1305 迁 iroh 后是净负债

**⚠️ 这是迁移建议（待评审），不是从 iroh 源码验证出的事实。**

`crates/core/src/transfer/wire/crypto.rs` 共 244 行，其中 **156 行是测试**（`#[cfg(test)] mod tests`
起于第 89 行），生产代码仅约 **88 行**。核实过它的性质：

- 模块文档：「每次传输生成独立的 256-bit 对称密钥，传输结束后销毁」，密钥仅存内存、**从不落库**
  （落盘文件是明文）→ **纯在途加密，不承担 at-rest 职责**
- nonce 由 BLAKE3 从 `(session_id, file_id, chunk_index)` 确定性派生
- **密钥由接收方生成**（`flow/receive.rs:151`、`flow/resume/mod.rs:76`），随即在
  `receive.rs:155-157` 经 `AppResponse::Transfer(TransferResponse::OfferResult { key: Some(key), .. })`
  **沿同一条 libp2p Noise 信道回传给发送方**

**关键推论**：密钥与密文走同一条加密信道，对网络攻击者**零增益**；libp2p circuit relay v2 只转发
Noise 密文，中继本就看不到明文。迁 iroh 后同理 —— iroh relay 转发的是不透明密文
（`iroh/iroh/src/lib.rs:99-103`：「Since endpoints only send encrypted traffic, the Relay servers
can not decode any traffic」），且应用层加密与 relay 是否 TLS 完全正交。

→ **它是传输层加密、在 Noise / QUIC-TLS 之下冗余，迁 iroh 后可整块删除。**
（在 libp2p 下这层也已经是冗余的；iroh 只是让这件事更明显。）

**相关文件**：`crates/core/src/transfer/wire/crypto.rs`、`crates/core/src/transfer/flow/receive.rs`

### push/offer 模型与 iroh-blobs 是架构级冲突，不是配置问题

我们是 push/offer（发方 offer → 收方 accept → 发方推 chunk）。iroh-blobs 是 **pull 模型**。
它的 push API 虽公开（`remote.rs:573 execute_push`），但：

- `EventMask::DEFAULT` 里 `push: RequestMode::Disabled` —— **默认即禁用**
- 官方明确拒绝提供开启 push 的便捷常量（`events.rs:200-203`）
- 内层文档自述 *"Note that many nodes will reject push requests. Also, this is an experimental
  feature for now."*
- sendme 完全未用 push

**→ 若迁 iroh-blobs，我们会落在它最不成熟的分支上。** 要么把产品模型改成 pull（发方只发 ticket、
收方拉），要么绕开 iroh-blobs 走 dumbpipe 形状。

### 加密冲突：内容寻址对我们价值归零

若走 iroh-blobs，加密与内容寻址二选一，**两条都不通**：

- **(A) 先加密再入库** → hash 变成密文哈希。我们的 nonce 基于
  `(session_id, file_id, chunk_index)`（`wire/crypto.rs:12`）**每接收方不同** → 同一文件对不同
  接收方是不同 blob → **去重/复用价值归零**
- **(B) 存明文靠 QUIC TLS** → blob 磁盘明文、hash 即凭证

（iroh-blobs 全库零加密原语：`grep -rniE "encrypt|chacha|aead|cipher" src/` → 0 匹配。）

**这条比性能/体积都更该先拍板。**

### 逐项迁移影响矩阵

| 我们的模块 | iroh 对应物 | 结论 |
|-----------|------------|------|
| `transfer/wire/crypto.rs`（XChaCha20-Poly1305） | QUIC/TLS + Ed25519 endpoint key | **纯冗余，可整块删除**（⚠️ 待评审，见上） |
| checkpoint 相关的 SQLite 表 | FsStore 的 `<hash>.bitfield` / redb inline | 可删——**但「传输历史/收件箱」是产品语义、不是续传状态，那张表要留** |
| `transfer/flow/prepare.rs` 的扁平 blake3 | bao-tree outboard | **能力升级**（获得逐块验签），不只是省代码 |
| `transfer/wire/data_frame.rs` 的 manifest | `Collection`（本身也是 content-addressed blob） | 清单和数据用同一套传输与校验，不需单独设计 metadata 消息 |
| `host.rs:214` `FileAccess` trait 的 sink 半区（`:223-241`，桌面实现 `src-tauri/src/host/file_sink/path_ops.rs`，走 `.part` 再 finalize） | iroh-blobs store + export | **我们 1x 磁盘（同盘 rename 不产生第二份数据），sendme 用法是 2x** —— 这是我们优于 sendme 的一点 |
| 无 | `api::downloader` 多源并行下载（`SplitStrategy::Split`，`src/api/downloader.rs:317-320`） | **iroh-blobs 白送但 sendme 没吃的能力**。对「一份文件发给多台设备」有实质价值；在 libp2p 下要自造相当于重写一个 BitTorrent 调度器 |

### 「增量同步」有歧义，写文档要写清

iroh-blobs 的「增量」= **同一 hash 未下完的部分补齐**（bitfield 集合差），
**不是** 文件 v1→v2 之间只传改动量 —— 未找到任何 rsync 式跨版本 delta 同步 API，
内容寻址下**文件一改 hash 全变**。

**→ 对「传一个每天改一点的大文件」这种场景，iroh-blobs 每次都是全新 hash、全量重传，
不比我们现在强。** 别把它当引入 iroh-blobs 的理由。

### iroh-blobs 的鉴权档位：我们该选 InterceptLog

sendme 只用了观测档位（`main.rs:713-718`：`connected: ConnectMode::Notify` +
`get: RequestMode::NotifyLog`）—— 那是产品决定（ticket 即凭据，谁拿到谁能下），不是能力缺失。

**直接照抄 sendme 会得到一个「任何人只要知道 hash 且能连上就能拉走文件」的节点。**

我们要做配对（已配对设备才能收）+ UI 要进度事件 → **应该用 `RequestMode::InterceptLog`**
（既拦截又给详细传输事件）。档位枚举见 `/iroh` 的 `blobs-and-file-transfer.md`。

### E2E 加密 ≠ 授权 —— 配对层不会被 QUIC-TLS 吃掉

iroh 自己划的分工（`iroh/iroh/src/lib.rs:81-94`）：

> When accepting connections the peer's [`EndpointId`] is authenticated. **However it is up to
> the application to decide if a particular peer is allowed to connect or not.**

**我们的配对逻辑对应的是后者（授权），不会被 TLS 取代。**
别因为「TLS 已经加密了」就以为配对可以砍。

### iroh 没有 Codec 层 —— framing 与请求关联要自己写

我们现在是 `swarm_p2p_core::start::<AppRequest, AppResponse>()`，定义
`AppRequest`/`AppResponse` + Codec 就能跑，白得 Codec trait、自动出站流管理、RequestId 关联、
ResponseChannel、超时、失败分类。

**iroh 这一层完全没有 —— 拿到的是 tokio `AsyncRead`/`AsyncWrite` 裸流。** 迁过去要自己实现
framing 与请求关联（官方范例 `search.rs` 的「一次请求 = 一条 bi 流」只是范例自定的约定，
不是 iroh 的规定；`read_to_end(n)` 的防 OOM 限长也必须自己给）。

**反过来是净收益**：iroh 也没有「Codec 必须适配 Behaviour 的 poll 语义」这个约束，
**长连接流式传输（大文件分块）写起来自然得多** —— 这正是我们的主场景。

### 流不再是稀缺资源：限流/流池逻辑是可以直接移除的净负债

| | 超限行为 | 后果 |
|---|---|---|
| **yamux（libp2p）** | 出站 `Poll::Ready(Err(ConnectionError::TooManyStreams))`；入站直接 `Action::Terminate` | **致命** —— 整条连接不可用 |
| **noq（iroh/QUIC）** | `Poll::Pending`（挂起等 `stream_budget_available`） | **背压** —— 最坏只是变慢 |

**libp2p 下「流是稀缺资源、要多路复用管理」是被迫的**（不管理就打死连接）；
iroh 下同样情形只是排队。**→ 原先的限流/流池逻辑在 iroh 下属于可以直接移除的净负债。**

反直觉：yamux 默认 512 比 QUIC 默认 100 更宽 —— 但 yamux 超限是致命的。
**「数字更大」反而更危险。** 详细行号见 `/iroh` 的 `streams-and-protocol.md`。

---

## 配对 / 寻址 / ticket

### 我们现有 6 位配对码的可枚举洞（当前 libp2p 实现）

- 码空间由 `crates/core/src/pairing/code.rs:8-9` 限定：`CHARSET = b"0123456789"`、
  `CODE_LENGTH = 6` → 恰好 **10^6**
- DHT key **可预计算**（`crates/core/src/dht_key.rs:4-16`）：

  ```rust
  const NS_SHARE_CODE: &[u8] = b"/swarmdrop/share-code/";
  fn dht_key(namespace, id) -> RecordKey { sha2::Sha256::digest([namespace, id].concat())... }
  pub fn share_code_key(code: &str) -> RecordKey { dht_key(NS_SHARE_CODE, code.as_bytes()) }
  ```

- 记录内容 `code.rs:46-55` `ShareCodeRecord { os_info, created_at, expires_at, listen_addrs }`，
  由 `pairing/manager.rs:84-89` 用 `put_json_record(..)` **明文 JSON** 发布到公共 DHT

**精确化范围（别把结论说过头）**：`share_code_key` 记录只在 `manager.rs:76-89 generate_code()`
时写入，且有 TTL（默认 300s）。所以扫描能收割的是「**当前正在显示配对码的设备**」，
**不是所有在线设备**（在线状态用另一个 namespace `online_key(peer_id_bytes)`，
键是 32+ 字节 peer id，**不可短码枚举**）。

**准确表述**：攻击者持续扫描可以**捕获每一次进行中的配对尝试并劫持它** ——
因为 6 位码是 `request_pairing` 的唯一秘密。

**ticket 方案的对比**：「码」就是 32 字节 ed25519 公钥（`EndpointId`），2^256 空间不可枚举，
且**根本不需要往任何公共存储写记录**。

> **换句话说：ticket 变长的那 57+ 个字符，买的正是「不可枚举 + 不用公开广播自己的地址和 OS」。**

### 决策：放弃人念短码，改二维码 + 链接 + 剪贴板

iroh 的寻址原语只有 pubkey→addr（pkarr / DNS / mainline），**没有 code→record 这一层**
（`n0-mainline/README.md`：*"The main purpose for which iroh uses n0-mainline is endpoint address
lookup via BEP_0044."* —— 即便用到 Mainline DHT，也只是按公钥查地址）。

若坚持保留 6 位短码，就得继续自维护一个 rendezvous 服务（且要修掉上面的可枚举问题：
加长/加盐/限速）—— **这与「迁 iroh 以减少自维护基础设施」这个动机直接冲突**。

**务实结论**：放弃人念短码，改**二维码 + 链接 + 剪贴板**三件套（正是 sendme 做的 ——
`main.rs:793-867` 有专门的剪贴板支持，feature 名就叫 `clipboard`）。ticket 最短 63 字符，
是 6 位码的 10 倍以上，电话报码彻底不可能；184 字符仍远在 QR 容量内。

**→ 后人不要以为可以把 6 位码 UX 平移到 iroh 上。**

### ticket 落地决策清单（KIND 必须先拍板）

| 决策点 | 建议 |
|---|---|
| **KIND 取值** | ⚠️ **必须先拍板再发版** —— KIND 会烤进每一个发出去的链接，改了就**废掉所有存量 invite**。若 URL scheme 是 `swarmdrop://`，KIND 取 `"swarmdrop"` 会得到冗余的 `swarmdrop://swarmdrop<base32>`；**建议 KIND = `"invite"`** → `swarmdrop://invite<base32>`，前端 `startsWith("invite")` 校验。桌面 share-target 的 File Association 入口同理可复用这个前缀判定 |
| **一次性 + 过期** | payload 里放 nonce + expires_at，`decode_bytes` 里用 `ParseError::verification_failed` 拒过期；**重放必须服务端记 nonce 已用集合** |
| **长度预算** | 160~230 字符。二维码 + 剪贴板 + 链接三件套，放弃人念 |
| **版本兼容** | 单变体 enum 留位，老变体永不删。iroh 自己都破坏过一次 |
| **地址存法** | 直接存 `EndpointAddr`，别拆字段 |
| **online() 超时** | 照 sendme：超时就拒绝生成 invite（除非确定配了 pkarr publisher） |
| **FFI 形状** | 抄 `iroh-ffi/src/ticket.rs:12-52`：`#[derive(Debug, uniffi::Object)]` + `#[uniffi::export(Display)]` + 两个 constructor（`from_addr` / `from_string`）。ticket 是不可变值类型，Object 包裹比 Record 更省心（避免每个字段都过 FFI 类型映射）。注意 `iroh-ffi/Cargo.toml:5` 是 `publish = false` —— **抄形状即可，不必依赖它** |

### 「6 位码 + DHT」在 iroh 生态无对应物，迁移后必须自建

iroh 的模型是自包含 ticket（`EndpointAddr` + `BlobFormat` + `Hash`）。短票的代价钉死在两处：
发送端选最短的 `Id` 就**必须**发布 pkarr（sendme `main.rs:660-662`），接收端 ticket 里没地址
就**必须**开 DNS 查询（`main.rs:1016-1018`）—— **想要短码就必须依赖 n0 的 DNS/pkarr 基础设施
做地址发现**。

我们的 6 位码 rendezvous 层迁移后要么用 pkarr 自定义记录自建，要么保留现有 DHT。

**这是 sendme/dumbpipe 两个官方样本都覆盖不到的空白区**（两者都是「谁拿到 ticket 谁就能连」）。

### 不要条件反射去找 DHT Provider 的对等物

**content-discovery 对我们只当读物，不引入。** 判据：SwarmDrop 的模型是「已配对设备之间点对点
发送」—— 收发双方的 EndpointId 在**配对阶段就已互知**，根本不需要内容发现层。

iroh-experiments 的 content-discovery 解决的是「谁有这个 HashAndFormat」（tracker 式），
功能上对应 libp2p 的 DHT Provider 语义 —— **我们从 libp2p 迁过来时容易本能地去找它的替代品，
但我们的配对模型让这一层根本不存在。**

（它本身也是 experimental：依赖停在 `iroh = "1.0.0-rc.1"`，CI **零 `cargo test`**。
唯一值得借鉴的是它的 `SignedAnnounce`（ed25519 签名防冒名宣告）设计 —— 真要做多源分发再回头看。）

### `presets::N0` 不含 mDNS —— 我们的对标场景默认完全不工作

`presets::N0` = Minimal + PkarrPublisher::n0_dns() + PkarrResolver::n0_dns() +
DnsAddressLookup::n0_dns() + default_relay_mode()，**核心不含 mDNS**。局域网发现必须额外加
`iroh-mdns-address-lookup` 依赖（**不是开 feature** —— 0.x 时代的 `discovery-local-network`
feature 已被移除）。

**→ 在 `presets::N0` 下，LocalSend 对标场景（我们的核心定位）完全不工作。**

**验收清单必须有一条**：拔网线 / 断公网，两台机器还能不能互相发现。

**配套**：`iroh-mdns-address-lookup` 成熟度 beta —— 自身 0.4.0 pre-1.0，且核心功能压在 alpha 依赖
`swarm-discovery 0.6.0-alpha.2` 上（第三方作者 rkuhn，最后提交 2026-04-15）。
**局域网是我们的核心场景，要预留 fallback**（例如保留自有的配对码 + relay 兜底路径），
不能把主场景单点押在一个 beta crate 的 alpha 依赖上。
⚠️ 别漏注册那一步：`endpoint.address_lookup().unwrap().add(mdns.clone())`。

**好消息**：局域网下 mDNS 发布的是直连 IP，连接直接起在 IP path 上、**全程不碰 relay** ——
这个主场景不受 QAD/7842 被墙影响（见下「relay 与基建」）。

### DHT 定位窗口会从「300s」退化成「永久」

| | 我们现在（libp2p） | 迁 iroh 后 |
|---|---|---|
| 键 | SHA256(6 位码)，TTL 300s | SHA1(EndpointId)，salt 恒为 None |
| 窗口 | **有限**（只在显示配对码时） | **永久**（`REPUBLISH_DELAY = 3600` 每小时重发） |
| 鉴权 | 无（但码会过期） | 无 ACL，记录只签名不加密 |

**EndpointId 本身就是一个长期有效的定位能力（bearer capability）。**

**缓解手段**：只在「可被发现」开关打开时才 add DHT lookup。

### user_data 必须留空（设备名红线）

**设备名放进 `user_data` = 全球明文可读且长期缓存。**

依据：`EndpointData` 可发布字段只有 addrs + user_data；`EndpointData::apply_filter`
在过滤后**显式把 user_data 重新挂回**，`AddrFilter` 的函数签名里根本没有 user_data ——
**任何层都剥不掉**；pkarr GET 无鉴权，知道 EndpointId 就能拉到完整记录。
`PkarrPublisher` 自带的 `AddrFilter::relay_only()` 默认不外泄 IP，**但对 user_data 完全不设防**。

**正确做法**：设备名（`preferences-store` 的 `deviceName`）只在配对建立后、**E2E 加密的应用层
协议里**交换。

**这是 code review 要卡的红线。** 与已有的「发布到公共 DHT 的记录不得携带设备信息」
（见 rust-backend.md 的 `OnlineRecord` 条目）是同一条原则 —— **换 iroh 不自动解决这类问题。**

### 公共 WiFi：mDNS 与 DHT 的默认过滤器完全相反

- **DHT** 默认 `relay_only()` —— 不外泄 IP
- **mDNS** 默认 `AddrFilter::default()` = **恒等过滤器，不过滤** —— 广播全部本地 IP + relay + user_data

移动端会在咖啡厅/机场跑。公共 WiFi 上任何人被动嗅 5353 就能收集 EndpointId + 全部内网 IP +
relay URL + user_data。

**建议**：`AddrFilter::ip_only()` + `service_name("swarmdrop")` 隔离。
（不能无脑 `relay_only()` —— 局域网直连本来就需要 IP。）

### 「附近设备」列表：合并 subscribe + resolve 两路

`subscribe()` **只推送被动发现的设备**，被 `resolve()` 显式解析到的不会推给 subscriber
（源码里 `if !resolved { subscribers.send(...) }`）。

→ 既用 subscribe 维护「附近设备」列表、又对已配对设备调 `endpoint.connect()`（内部触发 resolve）时，
**已配对设备可能不出现在 subscribe 流里，UI 列表缺项**。迁移时 `network-store` 的 peer map 要合并两路来源。

**顺带**：`advertise(false)` 零成本实现「隐身模式」（我能发现别人、别人发现不了我）。

### presence 是自研资产，不是待替换的负债

`crates/core/src/presence/mod.rs:1-16` 的模块文档已写明它是「唯一的 presence 大脑：消费连接/ping
事件 + 定时器，维护 per-paired-peer 状态机，并承担在线宣告（DHT OnlineRecord）与连接保活白名单的
全部职责」。状态机：Connected → Probing（退避重拨，宽限期内 UI 维持在线）→ Unreachable（低频 DHT
查在线记录 + 重拨），共 **927 行**（实测复核）。

**这是「对已知的、点名的对端做定向存活探测 + 宽限期消抖」；gossip 是「在成员未知的大群里扩散
消息」—— 两者正交。** 别拿 gossip 的 NeighborUp/Down 当 presence：那两个事件限定于「swarm
membership layer 的 direct neighbor」，且 active view 中掉线的槽位会被 passive set 里的**随机**
节点填补、默认 `shuffle_interval = 60s` 自发轮换 —— **NeighborDown 可能只是视图轮换，不是存活判定**
（依据见 `/iroh` 的 `protocols-gossip-docs-rpc.md`）。

**迁移动作**：这套状态机**保留**，只把底层 libp2p 连接事件换成 iroh 的 Watcher/Connection 事件。

**相关文件**：`crates/core/src/presence/mod.rs`、`crates/core/src/presence/supervisor.rs`

---

## relay 与基建

### 必须自建 relay —— 成本与今天维护 bootstrap 等价，不构成迁移否决项

- **中国大陆无第三方托管**：n0 官方 4 个 relay 中最近的是 `aps1-1.relay.n0.iroh.link.`
  （标签 Asia-Pacific，落地机房不在大陆 —— ⚠️ 属推断，需实测坐实）
- **iroh-relay 开源且自带 server binary**（`iroh/iroh-relay/Cargo.toml:176-179` 定义
  `[[bin]] name = "iroh-relay"` + `required-features = ["server"]`），仓根有 `docker/Dockerfile`；
  iroh-relay 与 iroh-dns-server **均在开源侧**
- **我们已有自建 bootstrap**（`47.115.172.218`，阿里云，TCP + QUIC）→ **复用同一台机器跑
  iroh-relay 即可**

**→ relay 不可达不是迁 iroh 的否决项**，它退化成一次自建部署 ——
与我们今天维护 bootstrap 节点是同一件事、同一份运维成本。基础设施主权与 libp2p 时代持平。

### 自建 relay 的部署取舍

| 决策点 | 我们的选择 | 理由 |
|---|---|---|
| **证书** | `cert_mode = "Reloading"` + 外部 acme.sh DNS-01 签证书 | 避开 LE 的 443 入站验证、拿到真 TLS（Web 端前提）、换证书不重启不打断在途传输 |
| **QAD** | `enable_quic_addr_discovery = true` | 直连率 |
| **拓扑** | **先只部署一台** | 0.13.0 已 `Remove derp meshing`，多 relay 不分摊转发压力。将来加第二台靠 `insert_relay` 运行时下发 |
| **preset** | **固定用 `presets::Minimal`** | 用 `presets::N0` + `relay_mode(RelayMode::Custom(..))` 会**静默保留**三项 n0 基础设施（PkarrPublisher/PkarrResolver/DnsAddressLookup 全指向 dns.iroh.link）—— `relay_mode()` 只改 transports、**不碰 address_lookup** |

**防火墙放行**：TCP 80、TCP 443、**UDP 7842**、（内网）TCP 9090。

> ⚠️ **放成 3478 = QAD 永远超时**。官方 Dockerfile 的 `EXPOSE 80 443 3478/udp 9090` 是 STUN 时代
> 遗留；README 写的 7824 是 7842 的数字转置。**只开 443 = 直连率显著劣化、带宽账单上涨却看不出原因。**

### 成本护栏：`access.http` callout 接现有配对体系

- **`access.http` callout** —— relay 每来一个连接就 POST 我们的服务，回 200 + 文本 `true` 才放行
  （`main.rs:329-333` 严格判等）。EndpointId 不可伪造（relay handshake 已先鉴权）→
  **可以按「是否是已配对设备」动态放行，天然对接我们现有的配对体系。**
- **`limits.client.rx.bytes_per_second`** —— 唯一的限流开关

> ⚠️ `accept_conn_limit` / `accept_conn_burst` 是**死配置**，服务端从不读取
> （`server.rs:485-500` 的 TODO 明写 not currently implemented）。别指望它。
>
> ⚠️ **实现鉴权服务时 header 是 `X-Iroh-NodeId`，不是 rustdoc 说的 `X-Iroh-Endpoint-Id`**
> （`main.rs:36` 常量名叫 `X_IROH_ENDPOINT_ID` 但线上字面量是 `X-Iroh-NodeId`）——
> 照 rustdoc 取 header 会拿到 None 然后**拒绝掉每一个连接**。

### relay 配额：迁 iroh 不丢功能

我们现有 `libs/core/src/config.rs:16` 的 `max_circuit_bytes` 已设成 `u64::MAX`
（`:31` 注释「自己的设备给自己转发，流量上限没有意义」）→ **流量配额本来就没在用，迁 iroh 不丢功能。**

但 iroh-relay 连 `max_reservations: 16` / `max_circuits: 8` / `max_circuits_per_peer: 2`
（`libs/core/src/config.rs:22-26`）这类**连接数配额**都没有等价物 ——
只能靠 relay 前面的 nginx/iptables 或云厂商限速。

### 双手机必然走 relay —— 带宽按「全量中转」预算

Hard×Hard（双方都在 CGNAT 后）是**物理事实，iroh 也解决不了**：官方 NAT 矩阵里
`nat_hard_x_hard()` 就是被 `#[ignore = "not yet passing (and likely can't without port guessing)"]`
标掉的三个之一。**中国移动网络普遍是 CGNAT（= Hard）。**

**→ 自建 relay 的带宽成本要按「移动端之间全量中转」预算，不能假设打洞能省掉。**
relay 中转 1GB = ingress 1GB + egress 1GB（1 进 1 出、无压缩无去重），
且默认**完全不限流**、限流只限「客户端→relay」方向。9090 端口 Prometheus 的
`bytes_sent`/`bytes_recv` 就是账单曲线。

反之手机 ↔ 有公网/UPnP 的桌面（Hard×None / Hard×Easiest）在矩阵里是 CI 必过的，能直连。

### portmapper 保持默认开启

**不加 `default-features = false`，也不设 `PortmapperConfig::Disabled`。**
它对国内家宽直连率有实质价值（能把家用路由器造成的 NAT 变成可直连）。

**但救不了移动网络的 CGNAT** —— UPnP/PCP/NAT-PMP 是向本地 CPE 路由器申请映射，
运营商侧 CGNAT 在家用路由器之上、不响应 UPnP。**收益范围限定在家宽直连率，不能顺延到移动网络。**

**配套产品动作**：macOS 首次启动时 UPnP 的 SSDP multicast 会弹防火墙授权对话框，
**需在 onboarding 里预先解释**，否则用户会误以为是恶意行为。

### 身份唯一性：暴露面只有「导入导出身份」

**通用风险**：同一 EndpointId 的两条连接连同一台 relay 时，新连接会把旧的顶成 inactive ——
inactive 方**仍能发但收不到任何东西**（`metrics.rs:89-96` 有原文说明，行为在 `clients.rs:85-130`）。
表现为「一台设备收不到任何东西但发得出去」，**极难排查**。

**对我们的暴露面**：`secret-store` 用 Stronghold 存 Ed25519 keypair（**本地**加密 vault，
无跨设备同步机制）→ 要小心的现实路径**只有「用户导入导出身份」**。
（网络切换后的重连瞬间也会短暂出现两条连接。）

---

## 移动端 / FFI

### iroh-ffi 是参考实现，不是依赖项（架构主线）

**我们桥的是自己的业务 core（transfer/SQLite/配对）。iroh 的 Endpoint 应留在 `crates/core` 里
当普通 Rust 依赖，永不过 FFI 边界。**

把 Endpoint/Connection 暴露到 TS 再用 TS 拼 chunk 循环，是纯粹的性能与复杂度自残 ——
**这不是「感觉对」，是有数**（量化依据来自 iroh-ffi 官方绑定源码）：

- 数据进出 uniffi 全是 `Vec<u8>` 拷贝
- `RecvStream::read(size_limit)` **按上限全量 malloc**（`read(1_000_000)` 无论实到几字节都先分配 1MB）
- SendStream/RecvStream 是 `Arc<Mutex<..>>`（tokio async Mutex）且 Clone → 多次调用在同一把锁上排队

假设在 TS 里写 chunk 循环：每个 chunk 至少两次拷贝（Rust `Vec<u8>` → RustBuffer → JS ArrayBuffer）
+ 一次按上限的全量 malloc + 一次 tokio Mutex 争用，还要在 JS 里做加密和 range-set 记账。

**结论：数据留在 Rust、FFI 只过业务语义（sendFile/pause/resume/进度事件）。**

（别人可能就是想在宿主语言里开裸 QUIC 连接 —— 那正是 iroh-ffi 的正当用法。这条只对我们成立。）

### iOS 部署目标：迁移前必须实测 17.0 能否活

`mobile/app.json:38` 现在是 `"deploymentTarget": "17.0"`，而 n0 官方在 `make_swift.sh` 里把 floor
设到 **17.5**（`IPHONEOS_DEPLOYMENT_TARGET="17.5"` / `MACOSX_DEPLOYMENT_TARGET="14.5"`），
根因是 iroh 的 netdev 调 `nw_path_is_ultra_constrained`（iOS 17 / macOS 14 起）。

17.0 < 17.5 会不会真链接失败，取决于 `nw_path_is_ultra_constrained` 的实际可用版本
（Apple 注释说 iOS 17，n0 可能是保守取整）—— **必须在迁移前实测**，
否则失败模式是「**CI 过、真机 17.0 装上就崩**」。

⚠️ 这是硬成本，且**和用不用 iroh-ffi 无关** —— 符号来自 iroh → netwatch → netdev 的传递依赖，
把 iroh 塞进自己的 core 一样吃。

**要补的 framework**：Network + SystemConfiguration，CoreWLAN 只 macOS 要（Tauri 桌面端也会碰到）。
失败模式极隐蔽：**链接期才在消费者工程报 undefined symbol，自己 build .a 时一切正常。**

**相关文件**：`mobile/app.json`

### Android JNI context 注入是必做项

迁 iroh 后**必然**要复制 iroh-ffi `src/android_init.rs` 那段手写 JNI：iroh 的 DNS resolver 通过
`ndk_context` 读 `LinkProperties.getDnsServers()`，必须在构造任何 Endpoint **之前**注入进程的
JavaVM + Application context（必须 `std::mem::forget(global_ref)` 永久泄漏 global ref）。

- **libp2p 时代不需要它**（DHT/bootstrap 是 IP 直连，不依赖系统 DNS）
- **iroh 需要**（address_lookup 走 pkarr/DNS）
- 缺了这段，Android 上解析会**静默失败** —— 表现是「能连自建 relay 但解析不出对端」，极难 debug

**落地**：加一个 `cfg(target_os="android")` 模块（手写 JNI，**uniffi 表达不了**）+ 一个 Kotlin object；
RN 场景下调用时机在 `ReactApplication.onCreate` 或 TurboModule 初始化，**务必早于任何 `Endpoint::bind`**。

### 移动端 mDNS 是前置 spike —— 风险最高、信息最少

**建议把「iOS/Android 真机 mDNS 能否收到包」作为前置 spike，早于任何大规模重构。**

依据（全是空白）：

- 全部 24 个仓 grep `multicast-networking|MulticastLock|CHANGE_WIFI_MULTICAST|com.apple.developer.networking.multicast`
  → **零命中**
- iroh-ffi / iroh-js grep `mdns` → **零命中**，**官方 FFI 绑定压根不暴露 mDNS**
  （`grep -rn 'address_lookup|AddressLookup|discovery|mdns|pkarr' src/` 只命中 3 处文档注释、零处 API）
- swarm-discovery 走 socket2 的 `join_multicast_v4/v6` 监听 224.0.0.251 / ff02::fb:5353
  → OS 层多播限制必然适用
- iOS multicast entitlement **需向 Apple 单独申请**；Android 需 `CHANGE_WIFI_MULTICAST_STATE`
  + 运行期 `MulticastLock`

**→ 我们要在移动端暴露局域网发现，必须自己写绑定，无先例可抄。**
（对照组：`iroh-c-ffi/src/endpoint.rs:55` 的 `pub enum DiscoveryConfig { None, DNS, Mdns, All }` ——
C 绑定反而做到了，可作为「该怎么暴露」的形状参考。）

**这条直接决定我们 iroh 迁移里程碑的第一步** —— 局域网是核心场景，这块塌了整个迁移的价值就要重算。

### uniffi 版本对齐：我们的写法没跑偏，但别把 iroh-ffi 当依赖

iroh-ffi `uniffi = "0.31.1"` vs `mobile/packages/swarmdrop-core/rust/mobile-core/Cargo.toml`
`uniffi = "0.31.0"`（实测复核）—— **只差一个 patch**。三个模式逐一对上：
`#[uniffi::method/export(async_runtime = "tokio")]`、`#[uniffi::export(with_foreign)]`、
`#[derive(uniffi::Object)]`。**写法没跑偏，可放心继续。**

**正因为同构，把 iroh-ffi 当依赖塞进 megazord 反而会撞车**：iroh-ffi 是独立 cdylib
（`crate-type = ["staticlib","cdylib"]`）+ 自己的 `uniffi::setup_scaffolding!()`，
**一个 megazord 装不下第二份 scaffolding**（两份各自注册符号）。

### `NetClient` 已适配 uniffi；builder / 一次性 handle 才会中招

凡是 `fn foo(self)` 消耗自身的 API（builder、oneshot handle、Incoming 类），过 uniffi **必然**要走
`Mutex<Option<T>>` 包装，且**要自己补运行期测试 —— 编译器不再替你把关**。

**libp2p 对照**：libp2p 的 Swarm/Transport 大量用 move + `&mut self`（`poll_next` 取 `&mut self`），
过不了 uniffi 的 `&self` 门槛。**但我们的 `NetClient` 已经是 Clone + 内部 channel 的形态**
（`libs/core/src/client/mod.rs:31` `command_tx: mpsc::Sender<Command<Req, Resp>>`，
`:41` 显式 `impl Clone for NetClient` —— 实测复核）→ **反而比 iroh 的 builder 类更适配 uniffi。**

**iroh-ffi 在这里踩的坑我们在 NetClient 上天然绕开了，但在 builder / 一次性 handle 上会同样中招。**

### `dyn EventBus` 已经等价于 iroh-ffi 的 `with_foreign` 回调 trait

我们的事件分层（见 rust-backend.md「文档漂移已修」条目）中，`&dyn EventBus` 在结构上
**已经等价于** iroh-ffi 的 `with_foreign` 回调 trait —— **要桥的东西比想象的少。**

**顺带值得抄的**：iroh 的 `WatchHandle` + `AbortOnDropHandle` 取消语义 ——
它把「外语言对象被 GC → Rust 后台任务自动 abort」这条链路接通了。

### 错误模型：抄 iroh-ffi 的形状，别透传层级

我们「不要给共享 crate 加 uniffi derive」这条纪律与 n0 的做法**完全同源**：FFI 边界要有自己的
稳定错误分类，不透传内部层级（n0 原话：*"intentionally coarser than the upstream Rust error
types... without leaking the internal `iroh` / `n0-error` error hierarchy"*）。

我们的 `AppError` 是 thiserror enum（`crates/core/src/error.rs:7`）。迁 iroh 后需吸收一批新的 iroh
错误类型（BindError / ConnectError / ConnectionError / AlpnError / …）→
**直接抄 iroh-ffi 的 `from_iroh_err!` 宏 + 粗粒度 kind 枚举的形状**：
`#[non_exhaustive]` + `Copy` 的 14 值 `IrohErrorKind`，用 **`uniffi::Object` 而非 `uniffi::Error`**
承载，附 `message()` / `debug_message()` 分离。代价是外语言拿不到 enum 的穷尽匹配。

**同理适用于 libp2p**：libp2p 的错误类型层级极深（TransportError / DialError / SwarmEvent 里的
各种 Err），**全量映射到 uniffi enum 不现实。**

### 单 ubrn 通道吃三端 vs n0 的双通道

| | 我们 | n0 官方 |
|---|---|---|
| 通道 | **一条 uniffi（ubrn）吃 iOS/Android/JS 三端** | uniffi（Swift/Kotlin/Python）+ napi（Node）**两条并行** |
| 代价 | **没有官方先例可抄** | —— |

事实依据：iroh-ffi 全仓 **0 处** react-native / turbo module / ubrn 引用；
n0 在 uniffi 这条线上**完全没碰 wasm**（所以「ubrn 能出 wasm」这件事在 iroh 官方实践里
**没有任何先例可参考**）；ubrn 走 JSI/Turbo Module，**不吃官方 Kotlin 路线那套 JNA 依赖**
—— 这是我们和官方路线的实质分野。

**我们的路线更省，但也意味着这一段全靠自己。**

---

## Web 端

### Web 端一上马，现有明文 relay 基建直接出局

浏览器下 iroh 是 **100% relay 流量**（wasm 下 `mod ip` 整个不编译 → 无 UDP → 无直连、无打洞），
且 https 页面**不能连 `ws://`**（mixed-content 会被拦截）。

1. **relay 必须真 TLS** —— 现有裸 HTTP bootstrap（`47.115.172.218:4001`）+ `tauri.conf.json` 里
   为 dogfood 更新服务器开的 `dangerousInsecureTransportProtocol: true` 这套在浏览器**行不通**。
   自建 relay 必须支持 wss 且证书要被浏览器信任。**→ Web 端上线前 relay 必须先上正经 TLS 域名。**
2. **relay 容量要重估** —— 按 Web 用户数重新算：浏览器**永远没有直连兜底**，
   每个字节都是账单（桌面/移动端有打洞兜底，Web 没有）。
3. **relay auth token 会降级进 URL query** —— 浏览器 WebSocket API 不能设自定义 header，
   token 只能进 URL → 会出现在 relay access log / 浏览器 history 里，**需要短时效 token**。
   （原生路径走 `Authorization: Bearer` header + TLS key export 认证 `KeyMaterialClientAuth`，
   浏览器两者都没有。）

**→ 别以为现有 relay 直接给 Web 端用就行。**

### 最乐观的一条：Web 端 = 再加一个 host adapter

`crates/core` 里的 protocol / pairing 层如果只依赖 iroh Endpoint + n0-future，
**理论上不用为 wasm 改一行**。

证据：iroh-gossip 零 wasm 适配代码就能跑（对 `iroh-gossip/src/` grep
`wasm_browser|target_family = "wasm"` → 0 命中，Cargo.toml grep `wasm` → 0 命中）；
官方三个浏览器例子的 shared 核心里 `cfg(` 计数**均为 0**。

真正需要 cfg 分支的是有平台副作用的部分 —— **数据库、文件读写、keychain**。

**→ 这跟我们现有的 host adapter 分层思路一致：Web 端相当于再加一个 adapter 实现，
而不是重写业务逻辑。** 这条直接影响 Web 端工作量估算。

### Web 端能做什么 / 不能做什么

- **能做**：小文件 / 文本 / 剪贴板同步 —— **现在就能做**。网络层（QUIC + E2E + accept + pkarr 寻址）全在。
- **不能做**：**收大文件**。MemStore 是唯一选项，整个文件驻留 wasm 线性内存 + 至少双份拷贝
  （`get_bytes` 成 Bytes 再 `copy_from` 进 Uint8Array，加上 MemStore 里那份）。
  要落盘只能自己写 store，但 iroh-blobs 的 `Store` 是 **irpc actor 接口而非公开可实现的 trait**
  —— **这条路成本很高，不是「实现个 trait」那么简单**。

**若基于 iroh-blobs，刷新页面 = 传输进度全丢**（浏览器端只有 MemStore、无任何持久化后端、
无 fs-store → 无 `.bitfield` 落盘）。

**→ 反过来说：自研 transfer 层在 Web 上反而是优势 —— 我们可以自己接 OPFS。**

**上游状态**：盯 **issue #86**（可插拔持久化后端 PR，未合）与 **#207**（WASM + irpc 仍坏），
**不是 #90**。

### 跨到 wasm 会丢掉桌面端已有的类型安全

桌面端用 tauri-specta 自动生成 `bindings.ts`，Rust 类型改了前端立刻编译报错。
**跨到 wasm 这条链路上这个保障没了**：

- wasm-bindgen 只为**导出的 struct/fn** 生成 .d.ts，经 serde-wasm-bindgen 转出去的 JsValue
  在 TS 侧就是 `any`
- 生成的 .d.ts 里 stream 只有裸 `ReadableStream`，没有泛型参数
- 官方 browser-chat 就是**手写整个事件 union**（`iroh.ts:293` 注释 *"types used in chat-browser,
  for now they are defined manually here"*）再靠 `as ReadableStreamDefaultReader<ChatEvent>` 强转
  —— Rust 侧改了字段 TS 侧**静默不报**

**→ 若认真做 Web 端，需自己补一个类型生成步骤**（specta 直接对 wasm 边界导出 TS，或 ts-rs），
否则等于**把桌面端已有的类型安全在 Web 端主动放弃**。

---

## 地基库（n0-future / n0-watcher / n0-error）

### n0-future 替换：现在做

**这是本文件唯一一条「现在做」。** native 端**类型等价**、零风险，desktop 与 uniffi 移动端
**默认运行时行为不变**，**不需要回归网络/传输逻辑**。

把 `crates/core` 里的 `tokio::spawn` / `tokio::time::*` 换成 `n0_future::task::*` /
`n0_future::time::*`。native 上 n0-future 原样 re-export tokio
（`#[cfg(not(wasm_browser))] pub use tokio::spawn;`）。

⚠️ **但别说「逐字节等价」** —— n0-future 在 native target 下**无条件启用 tokio 的 `test-util`
feature**，经 feature unification 把可 mock 时钟代码路径编进生产包。实测本仓当前状态：

```
grep -c "test-util" Cargo.lock                                    → 0（首次引入）
grep -cE '^name = "n0-(future|watcher|error)"' Cargo.lock          → 0（三个库均未引入）
workspace Cargo.toml:44  tokio = "1.49.0"                          （未指定 feature）
```

默认 `start_paused=false`，运行时行为不变，但**二进制必然不同**。

**工作量已盘清（实测复核）**：`crates/core/src` 下 `tokio::spawn` **19 处** + `tokio::time::`
**9 处** = **28 处**，分布在 **8 个文件**：

```
network/event_loop.rs    transfer/manager.rs      transfer/wire/data_plane.rs
transfer/actor/receiver.rs   transfer/flow/send.rs    transfer/flow/receive.rs
infra/supervisor.rs      presence/supervisor.rs
```

**不用动的部分（已盘清）**：

- **`select!` 与 `tokio::sync` 保持原样**：7 处 `tokio::select!`（`infra/supervisor.rs:220`、
  `network/event_loop.rs:326`、`transfer/manager.rs:214`、`transfer/wire/data_plane.rs:33`、
  `transfer/actor/sender.rs:223`、`transfer/actor/receiver.rs:269`、`presence/supervisor.rs:502`）
  **全部不改**；1 处 `tokio::sync`（`transfer/actor/receiver.rs:15`）同理 ——
  `tokio::select!` 是纯宏、`tokio::sync` 是纯用户态原语，两者 wasm 可用，**n0-future 压根不提供替代**。
- **`spawn_blocking` / `tokio::fs` / `tokio::io`：零处**（实测）—— 这块干净，无阻碍
  （这三者是 n0-future 明确无法上 wasm 的部分）。

**要单独盯的点**：`infra/supervisor.rs:9` 与 `presence/supervisor.rs` 的 `use tokio::time::Instant;`
把 `Instant` 用作**结构体字段**（`supervisor.rs:37 next_attempt_at: Instant`）与**公开签名**
（`:161 pub fn tick(&self, now: Instant)`）。native 下换成 `n0_future::time::Instant` 是同一个类型、
零风险，**但真上 wasm 时它会变成 `web_time::Instant`** —— 届时这两个签名是类型边界。

**顺带的好运气**：因 libp2p 的影响，我们的 transfer 层早已站在 **futures 生态而非 tokio::io**
（`transfer/wire/data_frame.rs:8` 是 `use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};`，
`transfer/actor/receiver.rs:11` 与 `sender.rs:12` 是 `use futures::io::AsyncReadExt;`）。
而 n0-future 根部 re-export 的 io/Stream 用的**正是 futures 生态的 trait**
（`n0_future::io::AsyncRead` 就是 `futures_io::AsyncRead`）→ **我们的 AsyncRead/AsyncWrite 代码
在 n0-future 下一行不用改**，`data_frame.rs` 的编解码可原样复用，也降低了后续接 iroh 流的摩擦。

⚠️ **它只管 task 和 time** —— SQLite 收件箱、`file_source`/`file_sink`、`host/keychain`
在浏览器仍需另找出路，**那才是 Web 端真正的工作量**。
⚠️ 浏览器端 `spawn_local` 是单线程的：`tokio::sync` 原语是 wasm 安全的，
但任何真正需要多线程的假设都不成立。

**相关文件**：上列 8 个文件、`Cargo.toml`（workspace tokio）

### n0-future 替换的 ROI 完全押在 Web 端

**移动端（uniffi）编译到 aarch64-apple-ios / aarch64-linux-android 时 target_family 不是 wasm，
100% 走 `pub use tokio::*` —— 从这次替换中一点好处都拿不到。**

**若 Web 端不做，替换的收益是 0**，只剩 import 变更的噪音。
**它是一张为 Web 端提前买的期权** —— 这是排期决策，不是技术判断。

### JoinSet 陷阱：我们目前不受影响

`crates/core` 目前**未用 JoinSet**（上述 8 个文件用的都是裸 `tokio::spawn`），
所以不受 n0-future wasm 版 JoinSet 缺陷影响。

**但若借机把裸 spawn 重构成 JoinSet 管理，要避开 `select! { join_next(), ... => spawn() }`
这个模式** —— wasm 下新 spawn 的任务可能永不被 join 出来（作者自己标注的 TODO 级缺陷）。

### n0-watcher 的 Vec 陷阱落点：relay 健康度展示

若做 relay 健康度展示，`ep.home_relay_status().initialized()` **必踩**：
Value 是 `Vec<RelayStatus>`，`Nullable<Vec<T>>::into_option` 是 `pop()` →
**只返回最后一个 relay**，且类型推断成 `RelayStatus`，**review 时看不出问题**；
单 relay 场景下恰好正确，**接第二个 relay 才暴露**。

**用 `.updated()` / `.stream()` / `.get()`。** 机制详解见 `/iroh` 的 `foundations.md`（n0-watcher 全解）。

### n0-error 不迁（已定案）

两条独立理由，各自都足够：

1. **location 生产默认不采集** —— n0-error 只在 `RUST_BACKTRACE=1|full` 或
   `RUST_ERROR_LOCATION=1` 时才采集 location（OnceLock 缓存）。我们的 Tauri 桌面包 / RN 移动包
   **不会带这些 env** → 在真实用户机器上拿不到任何 location，**迁移成本全付、收益为零**。
2. **`#[derive(Debug)]` 冲突** —— n0-error 的 derive 自己生成 Debug，再写 `#[derive(Debug)]`
   会 **E0119**；而 `crates/core/src/error.rs:7` 与 `src-tauri/src/error.rs:28` 正是
   `#[derive(Debug, Error)]`（实测复核）。

**→ iroh 错误直接用 thiserror `#[from]` 包住即可**（iroh 的 stack_error 类型同时实现了
`std::error::Error`，source 链完整）。

### iroh 错误接入 AppError 的落地写法

在 `crates/core/src/error.rs` 里加：

```rust
#[error("P2P error: {0}")]
Iroh(#[from] iroh::endpoint::ConnectError),
```

—— 与现有 `P2p(#[from] swarm_p2p_core::Error)` **完全同构，零心智负担**。

**唯一例外**：想读 iroh 错误的 `.meta()` location 或用 `.report()` 打日志时才需要加
`n0-error = "1.0"` —— 建议届时**只加到需要的 crate、不要全局铺开**
（iroh 没有 re-export `n0_error`，下游非必须引入）。

（往 AppError variant 里插字段**不会**因 derive(Serialize) 编译失败 ——
我们的 Serialize 是手写的，见 rust-backend.md「AppError 的 Serialize 是手写的」条目。）

---

## 生态选型否决清单

**这一节的用途是防止日后重复调研同一批死路。**

### iroh-docs 不该用

四条理由：

1. 点对点**一次性传输**用不上持续同步语义
2. 已有 SeaORM 2.0 + SQLite，引 redb 等于并存两个嵌入式 DB + 两套事务/备份/迁移心智
   （iroh-docs 默认 feature 还同时编入 **redb 4.1 与 3.1 两个大版本**），**移动端二进制体积尤其敏感**
3. 「ID 即读权限、内容明文」（`Capability::Read(NamespaceId)`，而 NamespaceId 同时是 replica 的
   **公开唯一标识**）与我们的 **E2E 定位冲突**；套 E2E 就丧失 key_prefix 查询并劣化 range-based
   set reconciliation
4. 强制拖入 gossip + blobs 全栈（`Builder::spawn` 强制 endpoint/blobs/gossip 三参数，须注册三个 ALPN）

**重估触发条件**：只有转向「收件箱/文件索引在我的多设备间自动持续同步」这个**另一个产品**时才重估。

### irpc 现在别上

它主打的「消灭巨型 enum + 回调 channel 样板」**在我们这里无痛点可解**：

```
grep -rnE "mpsc::|oneshot::" crates/core/src src-tauri/src   →  0 命中（实测复核，83 个 .rs）
```

我们的实际状态模型是**共享态**（`transfer/manager.rs:16` `use std::sync::{Arc, Mutex};`、
`:19` `use dashmap::{DashMap, DashSet};`、`:144-149` 一组 `Arc<dyn EventBus>` /
`Arc<DatabaseConnection>` 字段）。

**→ 引入 irpc 不是简化，是「先把架构改成 actor 再用它来简化」—— 净增复杂度。**
（对一个已经是 actor 模型的项目，结论相反。）

**顺带**：iroh 核心全部 7 个 Cargo.toml grep `irpc` **零命中** —— ProtocolHandler + cheap streams
才是 iroh 一等公民，**迁移路径上没有「必须先学 irpc」这一步**。

**later-if**：若将来做「桌面 app ↔ headless daemon」或「MCP server ↔ node」的控制面，
那才是 irpc 的场景（iroh-blobs 的 noq rpc —— 「CLI/app ↔ 本机 blobs daemon」的进程边界 RPC ——
正是这个先例）。`no_rpc` 模式（`#[rpc_requests(message = ..., no_rpc, no_spans)]`，
关掉 rpc feature 后依赖只剩 serde + tokio + tokio-util）是零网络代价的入场方式，
且日后要开远端时不用改调用方代码。

### gossip：不是 presence，且用它要自己签名

- **不是 presence** —— 见上「presence 是自研资产」
- **若真要用，隐性成本是自己套一层 Ed25519 签名**（`src/stores/secret-store.ts` 已有密钥对可复用）
  —— **任何知道 32 字节 TopicId 的人都能伪造消息**。iroh-gossip 协议层无 author / signature 字段，
  官方 `examples/chat.rs` 也是自行定义 `struct SignedMessage { from, data, signature }` 并手工
  `verify_and_decode`，明确把作者鉴权列为应用层责任。**这是隐性成本，不是开箱能力。**

### CRDT / iroh-automerge 与我们无关

**我们传的是不可变文件字节，不是可并发编辑的结构化文档 —— CRDT 的合并语义在这里
没有任何东西可合并。**

（因此 iroh-automerge / iroh-automerge-repo 这两个示例对我们只有
「ProtocolHandler + bi-stream 帧化范本」的参考价值，**没有能力价值**。）

### 替代传输全部不碰

| 路线 | 结论 |
|---|---|
| **custom transport** | **不受 semver 保护**（`unstable-custom-transports` 是空 feature gate，文档明写 "not covered by semantic versioning guarantees and may change in any release without a major version bump" —— patch 版本即可 break）+ 默认 PathSelector 会把 custom transport **无条件抬到 Primary tier**（接了不换 selector = 强制所有流量走它）。**当前无任何需求值得付这个代价。** |
| **Tor**（`iroh-tor-transport`） | 依赖外部 Tor daemon 的控制端口（torut + tokio-socks，**无 arti**）→ **iOS/Android 根本跑不了 daemon** → 跨端能力不对等。**架构级排除，不是性能取舍。** |
| **Nym**（`iroh-nym-transport`） | README 自己把 "Bulk data transfer" 与 "High-throughput file sync" 列在 **"Not suitable for"**；~15-20 KiB/s 意味着传 1GB 约 **15 小时**。与文件传输定位不可调和 —— **连「可选隐私模式」都不做。** |
| **BLE** | iroh 生态**零实现**（TRANSPORTS.md 只占了个 id、repo 列为空）。要做只能自己从零写 GATT 分片/MTU/双端权限，**工作量以人月计**；且发现层已由 `iroh-mdns-address-lookup` 覆盖 —— **没有做 BLE 的理由。** |

### h3-iroh：later-if，知道它存在，暂不投入

**场景**：若想把本机的 MCP server 暴露成「跨网络可达、无需公网 IP、E2E 加密」的 HTTP 服务
（让远端 AI agent 通过 iroh 直连本机 MCP），h3-iroh 是 n0 生态里**唯一的现成路子** ——
它把 iroh 的 QUIC 连接接到 h3 crate 上，带 axum feature 可以让现成的 axum app 通过 iroh
（含 relay 穿透）对外服务。这与我们「设备间数据通道（人 + Agent）」的定位对得上。

**但先别投入**：experimental，version 0.1.0，`src/` 只有 lib.rs + axum.rs 两个文件，
依赖停在 `iroh = "1.0.0-rc.1"` 与 `h3 = "0.0.8"`（**h3 上游自身仍是 0.0.x，API 随时会变**），
受 iroh-experiments 仓 README 免责覆盖，**CI 不跑测试**。

⚠️ **别把它当 Web 端方案**：浏览器不能直接说 iroh 协议，**h3-iroh 的两端都得是 iroh 节点。**

### `unstable-net-report` 别编进产品

`endpoint.net_report()` 依赖的 `unstable_net_report` 模块（`iroh/iroh/src/lib.rs:294-301`）
doc 明写无 semver 保证，feature 默认不开（`iroh/iroh/Cargo.toml:164`），
导出的 `Report` 本身还带 `#[non_exhaustive]`。

**若要在 app 内做网络诊断上报（例如给用户看「当前 relay 延迟」），必须包在自己的薄适配层后面，
别让 `NetReport` / `RelayLatencies` / `Probe` 类型渗进 `crates/core` 的公开接口或 uniffi 桥** ——
否则 iroh 一次 **patch** 升级就可能同时震到桌面（`src-tauri`）与移动端（`mobile-core`）**两条版本线**，
而它们共享同一份 `crates/core` path 依赖。（与 rust-backend.md 的 crates/core ↔ src-tauri 边界同源。）

同理，iroh-doctor 的 `nat_classifier.rs` 那张 Easy/Medium/Hard match 表可以抄，但抄了就得自己基于
`endpoint.net_report()` 补齐「同一 NAT 对不同目的端口是否给出不同映射」的探测 —— 也是同一个 feature 的代价。

### Iroh Services：不会被锁定，且我们本来就不该用

**锁定风险不成立**（证据）：开源 iroh 对 iroh-services **零依赖、零 phone-home** ——
对 `iroh/Cargo.toml`、`iroh-relay/Cargo.toml`、`iroh-base/Cargo.toml` grep
`iroh-services|iroh_services` 零命中；全仓 grep `services\.iroh\.computer|api_secret` 零命中。
必须**显式构造 client + 显式传凭证**才会有数据外发。

**我们的取舍**：Iroh Services（API Keys / Billing / Managed Relay / Metrics）是
**可观测性/遥测面，不是传输面** —— 能力是 `push_metrics`（周期推送设备指标到 services.iroh.computer）、
`net_diagnostics(send: bool)`、云端注册 endpoint 名字。
**这与 SwarmDrop 的 E2E 隐私定位直接冲突。不用它没有任何功能损失** ——
用开源版缺的只是云端 dashboard，不缺任何传输能力。

⚠️ **真正不开源的只有 iroh-doctor `swarm-client` 的 coordinator**（n0des 闭源后端，
ALPN `n0/n0des-doctor/1`）→ 「组织一群国内节点跑分布式连通性矩阵测试」这个诱人用法**我们用不了**。
要做国内多点连通率统计只能自己写：用 doctor 的 accept/connect 两两对测 + 自建调度。

### 值得抄的现成参考

- **`iroh-examples/tauri-todos`** —— 509 行 Tauri 应用，`src-tauri/src/{lib,main,ipc,iroh,state,todos}.rs`
  结构与我们**同构**，依赖 tauri ^2 + iroh 1.0.0 + iroh-docs 0.101。
  **它甚至和我们用同样的方式规避 Windows lib/bin 命名冲突**：`Cargo.toml` 里
  `name = "tauri_todomvc_lib"`，注释与我们用 `swarmdrop_lib` 的理由**逐字相同**。
  **即便最终不采用 docs/gossip，它 `src/iroh.rs:20-49` 的 Endpoint 装配与 Router 注册那段可直接对照**
  （`load_secret_key` → `Endpoint::builder(presets::N0).secret_key(key).bind()` →
  `Gossip::builder().spawn()` → `FsStore::load(&path)` → `Docs::persistent(path).spawn(..)` →
  `Router::builder().accept(BLOBS_ALPN/GOSSIP_ALPN/DOCS_ALPN).spawn()`）。
- **`iroh-examples/browser-chat`** —— 技术栈（React + Vite + shadcn）与我们前端几乎一致
  （架构：shared(Rust lib) + cli + browser-wasm(wasm-bindgen) + frontend）→
  **我们做 Web 端时最值得照抄的样板。**

---

## 迁移前置实测清单

**顺序有意义：网络先于代码。**

### 1️⃣ 国内网络实测 —— 迁移决策前第一件该做的事

在真实的中国移动 / 联通 / 电信 / **校园网**环境下跑：

```sh
iroh-doctor report
iroh-doctor relay-urls --count 5
```

看三件事：① QAD 能否穿过 **UDP/7842**；② `global_v4` 是否有值；
③ n0 的 `aps1-1` relay 的延迟与可达性。

**这比任何架构推演都有说服力** —— **iroh 迁移的成败在国内网络下先由 relay 可达性决定，
不是由代码结构决定。** 用数据决定 relay 拓扑（是否只跑自建、要不要保留 aps1-1 作兜底）。

**执行注意（都是会让人白跑一趟的坑）**：

- macOS 上 iroh-doctor 的 config 在 `~/Library/Application Support/iroh/iroh.config.toml`，
  **不是** `~/.config/iroh/`；跨平台最稳用 `--config <PATH>`。不写 config 也能直接跑
  （回落到 n0 默认 4 个 relay，含 aps1-1）。
- `relay-urls` 的 connect 与 Ping/Pong 超时**硬编码 2 秒**且 CLI 无超时参数（只有 `--count`）。
  国内 → aps1-1 在丢包时 RTT 冲破 2s 并不罕见 → **别只看通过/失败**，必须交叉验证 `report`
  输出里的 `relay_latency`，否则会得出「n0 relay 在中国完全不可用」这个**过强**的结论。
- `report` 是订阅 Watcher stream 的**持续输出**，每 5 分钟刷新一次，**不会自行退出**。
  若组织多个用户各跑一遍回传数据，别直接 `iroh-doctor report > out.txt` 等它结束 ——
  加 timeout 或只取首份报告（首份很快出）。
- `report` 一条命令即同时给出「UDP 通不通 / 各 relay 延迟 / 是否被劫持（captive_portal）/
  NAT 是否按目的地变映射」，`captive_portal` 对国内酒店、校园网场景尤其有价值。

### 2️⃣ 移动端 mDNS spike

见上「移动端 mDNS 是前置 spike」。**早于任何大规模重构。**

### 3️⃣ iOS 17.0 部署目标实测

见上「iOS 部署目标」。失败模式是「CI 过、真机装上就崩」。

### 4️⃣ （随时可做，与迁移解耦）n0-future 替换

见上「n0-future 替换：现在做」。
