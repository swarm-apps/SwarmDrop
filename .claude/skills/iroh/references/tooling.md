# 工具与诊断 —— iroh-doctor / iroh-experiments / Iroh Services

**iroh 1.0.2 · 调研 2026-07-17 · 源码 `/Volumes/yexiyue/iroh-study/`**

> API 用法 → `/iroh` skill。这里讲**排障工具的能用与不能用、以及锁定风险的真实边界**。

## 一句话结论

- **iroh-doctor 值得装**，且对「中国网络下 relay 可达性」是当前生态里最直接的答案。**但它是排障 CLI，不是可嵌入的库**
- **iroh-experiments 五个子项目全部落后**（4 个停在 iroh 1.0.0-rc.1、1 个停在 0.35），README 自称 "very low level and unpolished"，CI **不跑测试**
- **锁定风险不成立**：开源 iroh 对 iroh-services **零依赖、零 phone-home**，iroh-relay 自带 server binary 可自建。iroh-services 纯粹是**选配的可观测性 SaaS**

## iroh-doctor

- **成熟度**：**production**
- **依据**：
  - HEAD `c6abce7` 2026-06-24 `ci: add semver check (#82)`（约 3 周前）；version 0.101.0；依赖 `iroh = { version = "1.0.0", features = ["metrics", "unstable-net-report"] }`（`Cargo.toml:23`），Cargo.lock 锁在 1.0.0（与当前 1.0.2 semver 兼容）
  - README 提供 `cargo install iroh-doctor`；CI 含 semver check
  - ⚠️ **两点保留**：① 它是**独立的 0.x 版本线**，不在 iroh 1.0 的 semver 承诺内；② 它消费 `unstable-net-report`，而 `iroh/iroh/src/lib.rs:294-301` 明写 *"This API is unstable and gated behind the `unstable-net-report` feature. **It is not covered by semantic versioning guarantees and may change in any release without a major version bump.**"* —— **作为一次性诊断 CLI 这两点无所谓，但别把它的 API 编进产品**
  - ⚠️ 本地为 shallow clone（`git rev-parse --is-shallow-repository` = true，`git log` 仅 1 条），**无法**据此判断提交频率或 issue 活跃度
- **入口**：`iroh-doctor/src/doctor.rs`（`Commands` enum 在 :61）

### 7 个子命令

`report` / `accept` / `connect` / `port-map-probe` / `port-map` / `relay-urls` / `swarm-client`

> ⚠️ **README 的命令列表已过时**：文档里列的 `plot` 子命令在代码中**不存在**（全仓 `grep -rn "Plot" src/` **零命中**），而实际存在的 `swarm-client` 未被列出；README 也漏掉了 `--service-node` / `--ssh-key` 两个全局选项（`src/main.rs` 的 Cli struct：`service_node: Option<EndpointId>`（`#[clap(long, requires("ssh_key"))]`）与 `ssh_key: Option<PathBuf>`）。
>
> **以 `--help` 与源码的 `Commands` enum 为准。**

### 迁移评估的第一步

```sh
# 1) 网络环境总览：逐 relay 延迟 + captive portal + NAT 映射行为
#    无 config 文件时自动使用 n0 默认 4 个 relay（含 AP 的 aps1-1）
iroh-doctor report

# 2) 直接对每个 relay 测 connect 与 Ping/Pong 延迟（5 轮）
iroh-doctor relay-urls --count 5
#    输出形如：Node https://aps1-1.relay.n0.iroh.link./
#              Connect: 231ms
#              Latency: 198ms
#    失败则：   Connection Error: "..."

# 3) 两端实测连通性 / 吞吐 / relay→直连 切换
iroh-doctor accept --secret-key local                  # A 机，打印 endpoint-id
iroh-doctor connect <endpoint-id> --relay-url <url>    # B 机
#   → 输出 "Connection with <id> changed: Ip(..)/Relay(..) (after ..)"
```

**`report` 打出的 `Report` 结构**（`iroh/iroh/src/net_report/report.rs:18`，第 19-37 行公开字段）：

| 字段 | 含义 |
|---|---|
| `udp_v4` / `udp_v6: bool` | UDP 通不通 |
| `mapping_varies_by_dest_ipv4` / `_ipv6: Option<bool>` | NAT 是否按目的地变映射 |
| `preferred_relay: Option<RelayUrl>` | 选出的 home relay |
| **`relay_latency: RelayLatencies`** | doc: *"The measured latency to each relay, keyed by relay URL"* |
| `global_v4` / `global_v6` | 发现的公网地址 |
| **`captive_portal: Option<bool>`** | 是否被劫持 |

`iroh-doctor/src/commands/report.rs:48-51` 即 `let mut stream = endpoint.net_report().stream(); while let Some(report) = stream.next().await { println!("{report:#?}"); }`。

**对中国网络排查确实有用，且是最省事的入口**：一条命令同时得到「UDP 通不通 / 各 relay 延迟 / 是否被劫持 / NAT 是否按目的地变映射」。`captive_portal` 对国内酒店、校园网场景尤其有价值。

### 换成自建 relay

配置文件路径由 `iroh-doctor/src/config.rs` 的 `iroh_config_root()` 决定，**它是分平台的**：

| 平台 | 路径 |
|---|---|
| Linux | `$XDG_CONFIG_HOME` 或 `$HOME/.config/iroh` |
| **macOS** | **`$HOME/Library/Application Support/iroh`** |
| Windows | `{FOLDERID_RoamingAppData}/iroh` |

文件名 `CONFIG_FILE_NAME = "iroh.config.toml"`（`config.rs:18`）。也可用 `IROH_CONFIG_DIR` 环境变量无条件覆盖（`config.rs:14`），或 **`--config <PATH>`（跨平台最稳）**。

```toml
[[relay_nodes]]
url = "https://relay.example.com"
```

对应 `iroh-relay/src/relay_map.rs:232-246` 的 `RelayConfig { url, quic(serde default), auth_token(optional) }`。

⚠️ `NodeConfig` 用 `#[serde(default, deny_unknown_fields)]`（`config.rs:24`）—— **字段名写错会直接报错**（这是好事）。

`impl Default for NodeConfig`（`config.rs:36-51`）用 `iroh::endpoint::default_relay_mode().relay_map()` 填充 relay_nodes；`NodeConfig::load`（:59 起）在配置文件不存在时回落到 `Self::default()` —— **所以不写 config 也能直接跑，不是「必须先写 config」**。

### ⚠️ 三个「看起来能用，其实不能」

#### 1. NAT 类型分类器从未被任何 CLI 命令调用

`iroh-doctor/src/nat_classifier.rs` 里有 Easy/Medium/Hard 分类器，但：

全仓 grep `classify_nat_type|nat_classifier|NatType` 排除 nat_classifier.rs 自身后，**唯一命中是 `src/lib.rs:6` 的 `pub mod nat_classifier;`** —— 即除了模块声明，**无任何调用点**。`src/doctor.rs` 的 `pub async fn run`（739 行起）逐个 match 七个子命令，**无一处触及 NAT 分类**。`classify_nat_type` 仅在 nat_classifier.rs 自己的 `#[cfg(test)] mod tests` 里被调用。

**「iroh-doctor 能告诉你 NAT 类型」对 CLI 用户来说是错的。**

#### 2. 即使接上，也永远不会返回 Easy

`iroh-doctor/src/swarm/net_report_ext.rs:13/16` 两字段的 doc 明写 *"Whether the NAT mapping varies by destination PORT for IPv4 (**not implemented**)"*；`from_base_report`（:22-27）把 `mapping_varies_by_dest_port_ipv4/_ipv6` **恒置为 `None`**。生产路径上构造 ExtendedNetworkReport 的两处（`swarm/client.rs:92` 与 :123）都走 from_base_report。

而 `nat_classifier.rs:76-84` 的 match 表中，**Easy 唯一入口是 `(Some(false), Some(false))`** —— port 项恒为 None 时只能落到 `(Some(false), None) => Medium` 或 `(Some(true), None) => Hard`。只有单测（:116/:130）手动赋值 `Some(..)`。

**抄这张 match 表时必须自己补齐「同一 NAT 对不同目的端口是否给出不同映射」的探测**，否则最好的网络也只会被判成 Medium，**NAT 画像数据会系统性偏悲观**。

#### 3. `report` 的三个 flag 是装饰性的

`--quic-ipv4` / `--quic-ipv6` / `--https` **只被 println 打印，从未传给 endpoint**。

`iroh-doctor/src/commands/report.rs` 中这三个变量的全部命中集中在：:11-13（函数参数）、:16-19（全 false 则全置 true）、:22-29（`if quic_ipv4 { println!("quic ipv4") }` 等）。第 33-36 行构造 endpoint 时**只调了 `.relay_mode(...)`**：

```rust
Endpoint::builder(presets::N0).relay_mode(RelayMode::Custom(relay_map.clone())).bind().await?
```

—— **没有任何 `.net_report_config(...)`**（而该 setter 确实存在于 `iroh/iroh/src/endpoint.rs:798`）。故 report 恒以 `NetReportConfig::default()`（https_probes=true, captive_portal_check=true）运行。

**含义**：**别以为 `--https` 能单独隔离 HTTPS 探测来判断「是不是 QUIC/UDP 被封」** —— flag 无效，你拿到的永远是全量探测结果。要区分只能读 report 输出里 relay_latency 各条目的来源，或自己写代码传 `NetReportConfig{https_probes:false}`。

### ⚠️ relay-urls 的超时硬编码 2 秒

`iroh-doctor/src/commands/relay_urls.rs`（全文 153 行）中 `tokio::time::timeout(Duration::from_secs(2), client_builder.connect())` 用于连接；`ping()` 函数内同样 `Duration::from_secs(2)` 等 Pong。**CLI 侧 RelayUrls 只暴露 `--count`（`doctor.rs` 中 `#[clap(long, default_value_t = 5)] count: usize`），无超时参数。**

**国内 → aps1-1 的 RTT 在丢包时冲破 2s 并不罕见。** 用 relay-urls 得国内 relay 结论时，**别只看通过/失败**，要交叉验证 `report` 里的 relay_latency，否则可能得出「n0 relay 在中国完全不可用」这个**过强**的结论。

### ⚠️ report 是持续输出，不会自行退出

`report.rs:48-51` 订阅 net_report 的 Watcher stream 循环打印，循环结束后才 `endpoint.close().await`。完整报告间隔见 `iroh/iroh/src/net_report.rs:132` `const FULL_REPORT_INTERVAL: Duration = Duration::from_secs(5 * 60);`。

**脚本化采集国内网络数据时（例如让多个用户跑一遍回传），别直接 `iroh-doctor report > out.txt` 就等它结束** —— 需要加 timeout 或取首份报告。首份报告很快出，后续每 5 分钟刷新一次。

### ⚠️ swarm-client 依赖不开源的 coordinator

**这是 doctor 里唯一真正锁定 n0 的部分。**

`iroh-doctor/src/swarm/rpc.rs` **只定义了客户端**：`pub(super) type DoctorServiceClient = irpc::Client<DoctorProtocol>;`（:206）与 `pub struct DoctorClient`（:19），`DoctorClient::with_ssh_key`（:35）通过 `rcan::Rcan` 能力票据认证后 `.connect(coordinator_addr, N0DES_DOCTOR_ALPN)`（:70）。

`DoctorProtocol` 的**八个 RPC**（:459-476，Auth/Register/GetAssignments/CreateTestRun/ReportResult/GetNodeInfo/MarkTestStarted/GetTestRunStatus）**没有任何服务端 handler 实现**。仓内唯一的 ProtocolHandler 是 `swarm/runner.rs:35-37` 的 `SwarmProtocolHandler`，它注册的是节点间测试用的 `DOCTOR_SWARM_ALPN`（:247），**不是 coordinator**。

CLI 也强制要求 `--ssh-key` 与 `--coordinator <EndpointId>`（doctor.rs 中均为 `required = true`）。

ALPN 是 `n0/n0des-doctor/1` —— 即 **n0des 产品的闭源后端**。

**「组织一群国内节点跑分布式连通性矩阵测试」这个诱人用法用不了。** 要做国内多点连通率统计，只能自己写：doctor 的 accept/connect 两两对测 + 自建调度。

### 关键背景：HTTPS 探测是 QUIC 被封网络下的唯一手段

`iroh/iroh/src/net_report.rs:89-100` `NetReportConfig::https_probes` 的 doc：

> *"HTTPS latency probes perform an empty HTTPS GET request to each configured relay server and measure latency. They are performed in addition to the QUIC address discovery (QAD) probes. **In networks that do not allow QUIC traffic, they are the only way to detect relay latencies and thus the preferred relay.** Disabling them is harmless on networks that do allow QUIC traffic, but will completely prevent finding the home relay on networks that do block QUIC."*

:123-129 Default 为 `https_probes=true`；`minimal()`（:114-120）则把它关掉。

**⚠️ 迁移风险点**：国内运营商对 UDP/QUIC 常有限速或阻断。若为了省事用 `NetReportConfig::minimal()` 或某些 preset 关掉 https_probes，**在封 QUIC 的网络下将完全选不出 home relay**。**默认值是安全的，别乱关。**

## iroh-experiments —— 读，别依赖

- **仓 README 自述**：*"This is for experiments with iroh by the n0 team. Things in here can be very low level and unpolished."* + *"Some of the things in this repo might make it to iroh-examples or even into iroh itself, most will not."*
- **依赖全部落后**：`.github/workflows/ci.yml:16` `RS_EXAMPLES_LIST: "content-discovery,iroh-pkarr-naming-system,iroh-s3-bao-store,iroh-dag-sync,h3-iroh"`
  - **4 个停在 `iroh = "1.0.0-rc.1"`**：content-discovery(`Cargo.toml:29`)、h3-iroh(:18)、iroh-dag-sync(:10)、iroh-pkarr-naming-system(:12)
  - **第 5 个更早**：iroh-s3-bao-store(:24) 是 `iroh = "0.35"`
  - 而 `iroh/iroh/Cargo.toml:3` 是 **1.0.2**
- **CI 只跑 check/fmt/clippy，零 `cargo test`**：三个 step 分别只跑 `cargo check --all-features` / `cargo fmt -- --check` / `cargo clippy` —— **无 `cargo test`**。`ci.yml:15` `MSRV: "1.75"`（远低于 iroh 主仓的 Rust 2024）
- 仓库 HEAD `b66d8b8` 2026-06-01，标题即 `feat: update to iroh@1.0.0-rc.1 (#47)`

**连 n0 自己都只保证它能编译过。任何从这里抄的代码都要按 1.0.2 重新校对 API。**

### content-discovery

- **成熟度**：**experimental**
- **是什么**：tracker 式全局内容发现。三个 crate —— `iroh-content-discovery`（协议类型 + client，ALPN `n0/tracker/1`）、`iroh-content-tracker`（tracker 服务端）、`iroh-content-discovery-cli`
- **语义**：「谁有这个 HashAndFormat」。peer 用 `SignedAnnounce`（ed25519 签名，**防冒名宣告**）向 tracker 宣告自己持有某内容，`AnnounceKind` 区分 Partial / Complete；他人向 tracker Query 得到持有者 EndpointId 列表。支持 0-RTT 宣告（`announce_conn_0rtt`）与并行多 tracker 宣告（`announce_all`）
- **入口**：`iroh-experiments/content-discovery/iroh-content-discovery/src/protocol.rs`（:15 `pub const ALPN: &[u8] = b"n0/tracker/1";`）

```rust
// client.rs:96 —— 向单个 tracker 宣告
pub async fn announce(
    endpoint: &Endpoint,
    endpoint_id: EndpointId,          // tracker 的 EndpointId
    signed_announce: SignedAnnounce,  // ed25519 签名
) -> Result<()>;

// client.rs:71 —— 并行向多个 tracker 宣告
pub fn announce_all(
    endpoint: Endpoint,
    trackers: impl IntoIterator<Item = EndpointId>,
    signed_announce: SignedAnnounce,
    announce_parallelism: usize,
) -> impl Stream<Item = (EndpointId, Result<()>)>;
```

**功能上对应 libp2p 的 DHT Provider 语义，但用的是「中心化 tracker」而非 DHT。**

#### ⚠️ 关于「它有没有 pkarr」—— 一个典型的改名陷阱

顶层 README 说 content-discovery 含 *"[pkarr] integration for finding trackers"*，而**字面 grep `pkarr` 在该目录零命中** —— 很容易得出「README 与代码不符」的结论。**那是错的。**

**pkarr 机制以改名后的 crate 完整存在且已接线**：
- `content-discovery/Cargo.toml:32` `iroh-mainline-address-lookup = "0.3"`
- `iroh-content-tracker/src/main.rs:53` `DhtAddressLookup::builder().secret_key(key).build()?`（发布自身地址）
- `iroh-content-discovery-cli/src/main.rs:88` `DhtAddressLookup::builder().no_publish().build()?` + `.address_lookup(address_lookup)`（查询侧解析）
- `iroh-content-tracker/Cargo.toml:22` 与 `iroh-content-discovery-cli/Cargo.toml:13` 均有 `iroh-mainline-address-lookup = { workspace = true }`
- Cargo.lock 内 iroh-mainline-address-lookup 依赖 n0-mainline，而 `/Volumes/yexiyue/iroh-study/n0-mainline/README.md` 自述为 *"an iroh-flavored fork of [nuhvi/dht]"*、*"endpoint address lookup via BEP_0044"* —— **nuhvi 即 pkarr.org 作者，BEP_0044 ed25519 签名可变记录就是 pkarr 的机制本体**

**n0 把 pkarr 换成自家 n0-mainline、并把 discovery 改名 address_lookup，所以字符串消失而功能仍在。**

> 📌 **这是本次调研里最典型的「旧版本 API 名称」陷阱**：只查一个子 crate 的 Cargo.toml（恰好是三个里唯一没有该依赖的那个）+ 字面 grep 关键词 = 得出反向结论。**iroh 生态凡是 grep 不到 `discovery` / `pkarr` 的地方，先想想是不是改名了。**

**真正成立的限制**：announce/query **确实要求显式传入 tracker 的 EndpointId**（README 示例即 `--tracker b223f67b...` 十六进制 EndpointId），不存在「枚举有哪些 tracker」的发现层。

**但 DhtAddressLookup 让 tracker 仅凭公钥即可被拨号、无需静态地址或 relay 配置 —— 这是降低而非提高自建 tracker 的运维成本。**

### iroh-pkarr-naming-system

- **成熟度**：**experimental**
- **依据**：version 0.2.0；依赖 `iroh = "1.0.0-rc.1"` / `iroh-blobs = "0.102"` / `pkarr = { version = "5", features = ["dht"] }`；受仓 README 免责覆盖；CI 不跑测试
- **是什么**：IPNS 的极简复刻 —— 把一个 iroh blake3 content hash 发布到 ed25519 公钥名下，通过 pkarr + BitTorrent mainline DHT 存取，之后可按公钥查最新 hash。即「**可变指针指向不可变内容**」
- ⚠️ **与 iroh 内建的 address_lookup 不同**：那里 pkarr 用于发布**节点地址**；这里 pkarr 被用来发布**内容 hash**
- **何时可能有用**：想做「稳定分享链接，内容可更新」（一个长期有效的分享码指向最新版文件）—— 这是 n0 生态里唯一的可变命名参考
- **别用于设备地址发现** —— 那是 iroh 内建 address_lookup 的活，已在正式版里

### h3-iroh

- **成熟度**：**experimental**
- **依据**：version 0.1.0；依赖 `iroh = "1.0.0-rc.1"` / `h3 = "0.0.8"`（**h3 上游自身仍是 0.0.x**）；受仓 README 免责覆盖；CI 不跑测试；`src/` 只有 lib.rs + axum.rs 两个文件
- **是什么**：把 iroh 的 QUIC 连接接到 h3 crate 上，在 iroh 连接之上跑 HTTP/3。带 axum feature，可以让现成的 axum app 通过 iroh（含 relay 穿透）对外服务。examples/ 里有 client.rs / server.rs / server-axum.rs
- **何时可能有用**：想把一个本地 HTTP 服务暴露成「跨网络可达、无需公网 IP、E2E 加密」的服务 —— **属于「值得知道，暂不投入」**
- **别把它当 Web 端方案**：浏览器不能直接说 iroh 协议，**h3-iroh 的两端都得是 iroh 节点**

### iroh-dag-sync

- **成熟度**：**experimental**
- **依据**：version 0.1.0；依赖 `iroh = "1.0.0-rc.1"` / `iroh-blobs = "0.102"` / `iroh-gossip = "0.100"` / `iroh-car = "0.5"` / `redb = "4.1"` / ipld-core / cid / serde_ipld_dagcbor；受仓 README 免责覆盖；CI 不跑测试；README 自称 *"Example how to use iroh protocols"*
- **是什么**：在 iroh-blobs 与 IPFS 之间搭桥 —— 同步「非 BLAKE3 CID」的 IPFS DAG（unixfs 目录、深层 DAG），用 redb 存 DAG 结构、iroh-blobs 存原始数据，支持从 .car 文件导入
- **唯一可借鉴处**：它展示了 iroh-blobs + 自定义 traversal + redb 索引怎么组合
- **别因为「也要传目录」就来抄** —— 它的复杂度全在 IPFS CID 兼容与多哈希函数上。直接看 iroh-blobs 的 collection 抽象即可

### iroh-s3-bao-store

- **成熟度**：**abandoned**
- **依据**：**README 有显式 NOTE**：*"This crate is currently pinned to `iroh@0.35` / `iroh-blobs@0.35`. The store API in iroh-blobs 0.102 is structured differently, and porting the 'outboard in memory, data stays remote' behaviour properly hasn't been done yet — **left for future work**."* Cargo.toml 佐证：`iroh = "0.35"` / `iroh-blobs = "0.35"`（同仓其余项目已到 1.0.0-rc.1 / blobs 0.102，它落后约 67 个 minor）；version 0.1.0。仍留在 CI 的 RS_EXAMPLES_LIST 里，故只是「**停止移植**」而非删除
- **是什么**：把数据留在 S3/HTTP 远端、只在内存里算 bao outboard 的 iroh-blobs store 实现
- **别用**。仅在想理解「outboard 与数据可以分离存放」这一 bao 特性时扫一眼它的 README 概念说明。**学 bao/outboard 请直接读 `bao-tree/src/lib.rs:1-204`**（见 `blobs-and-bao.md`）

## Iroh Services —— 锁定风险的真实边界

- **成熟度**：**production**（选配）
- **依据**：
  - `iroh-doctor/Cargo.lock` 中 iroh-services version = 1.0.0、`source = "registry+https://github.com/rust-lang/crates.io-index"`、带 checksum —— **已在 crates.io 正式发布 1.0.0**
  - iroh-ffi（HEAD 2026-07-16）以 `iroh-services = { version = "1.0.0", default-features = false }` 依赖它，并配有 Rust + Python 双份测试
  - ⚠️ **它是 library crate**（`iroh-ffi/src/services.rs:8` `use iroh_services::{Client, ClientBuilder};`、:65 `impl From<iroh_services::net_diagnostics::DiagnosticsReport>`），**不是 binary crate**
  - ⚠️ **源码未克隆到 iroh-study，本次未审计** —— 但**不是「不可审计」**：crates.io 的 .crate 包按定义即源码分发（cargo 必须从源码编译），docs.rs / `cargo vendor` 均可拿到
- **入口**（本地唯一可读的证据）：`iroh-ffi/src/services.rs`

### 它是什么（能力范围经 FFI 绑定确认）

- **它是可观测性/遥测面，不是传输面。**
- 能力：`ping` / `name` / `set_name`（云端注册 endpoint 名字）/ `push_metrics`（按 metrics_interval 周期推送指标）/ `net_diagnostics(send: bool)`（跑本地网络诊断，**可选**上传云端存档）
- 凭证三选一：`api_secret`（`services1...` 编码 ticket）、`IROH_SERVICES_API_SECRET` 环境变量、或 `ssh_key_pem`（node operator / project owner 全权限）
- doc comment 直言：*"Binding for `iroh-services` — push metrics to services.iroh.computer"*

### ✅ 开源 iroh 与它零耦合 —— 锁定风险不成立

- 对 `iroh/Cargo.toml`、`iroh-relay/Cargo.toml`、`iroh-base/Cargo.toml` grep `iroh-services|iroh_services` → **零命中**
- 对全仓 `*.rs`/`*.toml`/`*.md` grep `services\.iroh\.computer|iroh\.computer/services|api_secret` → **零命中**

**反向佐证：必须显式构造 + 显式凭证**

```rust
// iroh-ffi/src/services.rs —— 无凭证时直接报错
// "ServicesOptions requires one of api_secret, api_secret_from_env=true, or ssh_key_pem"
let mut builder: ClientBuilder = Client::builder(endpoint.raw());
builder = builder.api_secret_from_str(&secret)?;   // 或 .api_secret_from_env() / .ssh_key(&pem)
let inner = builder.build().await?;                // 指标此后才按 interval 自动推送

// iroh-doctor/src/doctor.rs:656-668 —— 只有传了 --service-node 才建
let rpc_client = if let Some(remote_node) = service_node {
    let client = iroh_services::Client::builder(&endpoint)      // :659
        .ssh_key_from_file(ssh_key_path).await?
        .remote(remote_node)
        .build().await?;
    Some(client)
} else { None };   // ← 不传就是 None，Endpoint 完全不受影响
```

**不构造 ServicesClient 就绝不会有任何数据外发。**

### ✅ iroh-relay 开源且自带 server binary

- `iroh/iroh-relay/Cargo.toml:176-179` 定义 `[[bin]] name = "iroh-relay"` / `path = "src/main.rs"` / `required-features = ["server"]`
- `iroh-relay/src/` 下同时有 `main.rs`、`server.rs` 与 `server/` 目录
- iroh 仓根有 `docker/Dockerfile` 与 `docker/README.md`
- iroh 仓 crate 列表为 iroh、iroh-base、iroh-dns、**iroh-dns-server**、**iroh-relay** —— **relay 与 DNS server 两块基础设施均在开源侧**

**自建 relay 是一等公民路径，不是 hack。**

### 结论

**Iroh Services（API Keys / Billing / Managed Relay / Metrics）是纯选配 SaaS 观测面，边界清晰地落在「一个需要显式构造 + 显式凭证的 client crate」上。用开源版缺的只是云端 dashboard，不缺任何传输能力。**

**真正不开源的是 iroh-doctor `swarm-client` 的 coordinator（n0des 后端）。**

## ⚠️ 应用内诊断的 semver 代价

`endpoint.net_report()` 所依赖的 `unstable_net_report` 模块（`iroh/iroh/src/lib.rs:294-301`）：

```rust
#[cfg(feature = "unstable-net-report")]
pub mod unstable_net_report {
    //! This API is unstable and gated behind the `unstable-net-report` feature.
    //! It is not covered by semantic versioning guarantees and may change in any
    //! release without a major version bump.
```

导出 `Probe`、`RelayLatencies`、`Report as NetReport`。`iroh/iroh/Cargo.toml:164` `unstable-net-report = []`（**默认不开**）。`Report` 本身还带 `#[non_exhaustive]`（`report.rs:17`）。

**若要在 app 内做网络诊断上报（例如给用户看「当前 relay 延迟」），就得开这个 feature，等于把一个无 semver 保证的 API 引入产品。**

**建议**：把它包在自己的**薄适配层**后面，别让 `NetReport` 类型渗进 core 的公开接口或 uniffi 桥。

## 方法论限制（诚实交代）

iroh-study 里的仓库是 **shallow clone（depth=1）**，因此任何基于 git log 的「提交频率 / 是否停更 / issue 活跃度」判断在本地都**无法成立**。

- `git rev-parse --is-shallow-repository` 在 iroh-doctor、iroh-experiments、iroh 三处均返回 `true`
- `git log --oneline | wc -l` 三处均为 **1**

可用的日期证据只有各仓 HEAD 单条 commit：iroh-doctor `c6abce7` = 2026-06-24、iroh-experiments `b66d8b8` = 2026-06-01、iroh = 2026-07-16。

**本 skill 中所有 maturity 判定均基于「HEAD 日期 + 版本号 + 依赖版本 + README 免责声明 + CI 配置」，未使用提交频率或 issue 活跃度。**

**iroh 主仓 HEAD 距调研仅 1 天、PR 编号已到 #4421，是本次唯一有直接证据表明高强度维护的仓。**
