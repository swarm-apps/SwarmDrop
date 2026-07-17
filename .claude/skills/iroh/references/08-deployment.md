# Deployment：基础设施 · 指标 · 安全与隐私

iroh 1.0.2 · 调研日期 2026-07-17 · 源码快照 `/Volumes/yexiyue/iroh-study/`（24 个仓）

对应官方 [Deployment](https://docs.iroh.computer/deployment/) 分区：Dedicated Infrastructure /
Custom Metrics / Security & Privacy。

> **relay 怎么配** → [07-configuration.md](07-configuration.md)。**relay 是什么** → [01-concepts.md](01-concepts.md)。
> **托管 relay / 云端 dashboard（选配 SaaS）** → [09-iroh-services.md](09-iroh-services.md)。
> **排障（iroh-doctor）** → [10-about-and-policy.md](10-about-and-policy.md) 的 Troubleshooting 一节。

---

# 1. Dedicated Infrastructure

### ✅ iroh-relay 开源且自带 server binary

- `iroh/iroh-relay/Cargo.toml:176-179` 定义 `[[bin]] name = "iroh-relay"` / `path = "src/main.rs"` / `required-features = ["server"]`
- `iroh-relay/src/` 下同时有 `main.rs`、`server.rs` 与 `server/` 目录
- iroh 仓根有 `docker/Dockerfile` 与 `docker/README.md`
- iroh 仓 crate 列表为 iroh、iroh-base、iroh-dns、**iroh-dns-server**、**iroh-relay** —— **relay 与 DNS server 两块基础设施均在开源侧**

**自建 relay 是一等公民路径，不是 hack。**

### 多 relay 不分摊压力：mesh 已死

`CHANGELOG.md:2062`「Remove derp meshing (#2079)」位于 0.13.0 段（2024-03-25）；现源码 `grep -rni mesh iroh-relay/src/ iroh/src/` **零命中**。

**结论：先只部署一台 relay。** 多 relay 只在「用户地理分布广、想各自就近接入」时才有意义，且要求两端都能连到对方的 home relay。单 relay 下所有人 home relay 相同，行为最可预测。将来加第二台靠 `insert_relay` 运行时下发即可。


> **容量规划的输入**：官方 16 组 NAT 矩阵里 **Hard×Hard 必走 relay**（移动网络普遍 CGNAT = Hard）
> —— 自建 relay 的带宽要按「移动端之间全量中转」预算。矩阵详情 → [01-concepts.md](01-concepts.md)。
> **端口**：必须同时放通 **443/tcp 与 7842/udp**，只开 443 会显著劣化直连率、带宽账单上涨却看不出原因。


### relay 中转 1GB = ingress 1GB + egress 1GB

1 进 1 出，**无压缩无去重**。`metrics.rs:13-24` 两个独立计数器：`bytes_sent` 与 `bytes_recv`。

### 默认完全不限流，且限流只限「客户端→relay」

```
// streams.rs:326-331
/// Rate limiter for reading from a [RelayedStream].
/// The writes to the sink are not rate limited.
```

代码印证：:553-605 `impl AsyncRead for RateLimited` 里有 bucket 消费，而 :608-630 `impl AsyncWrite for RateLimited` 三个方法**全是直穿**，**无 bucket**。

默认无限流：`server.rs:487-500` `#[derive(Debug, Default)] pub struct Limits { pub client_rx: Option<ClientRateLimit>, ... }` → `client_rx: None`。

桶参数（`streams.rs:417-430`）：`max_burst_bytes` 默认 = `bytes_per_second / 10`，refill 周期 100ms。

**唯一的限流开关**：

```toml
[limits.client.rx]
bytes_per_second = 5_000_000
max_burst_bytes  = 500_000          # 不写则默认 = bytes_per_second/10
```

**没有任何按连接/按会话的字节总量上限。**

### ⚠️ 陷阱：`accept_conn_limit` / `accept_conn_burst` 是死配置

TOML 能解析、类型能通过、`build_relay_config` 会赋值（`main.rs:754-755`），但**服务端从不读取**：

```rust
// server.rs:485-500
/// Rate limits.
// TODO: accept_conn_limit and accept_conn_burst are not currently implemented.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Limits {
    pub client_rx: Option<ClientRateLimit>,
    /// Rate limit for accepting new connections. Unlimited if not set.
    /// Not currently implemented, setting this has no effect.
    pub accept_conn_limit: Option<f64>,
    /// Not currently implemented, setting this has no effect.
    pub accept_conn_burst: Option<usize>,
}
```

下游 `server.rs:741-743` **只消费 `relay_config.limits.client_rx`**。

**按官方 TOML 字段名配了 `accept_conn_limit` 就以为挡住了连接洪水 —— 那是纸糊的。** 连接数配额只能靠 relay 前面的 nginx/iptables 或云厂商限速。

> **libp2p 对照**：libp2p circuit relay 有 `max_reservations` / `max_circuits` / `max_circuits_per_peer` / `max_circuit_bytes` 这类配额。**iroh-relay 一个连接数配额的等价物都没有**，只有 `limits.client.rx.bytes_per_second` 这一个字节速率开关。

---

# 2. Custom Metrics

### 可观测：Prometheus

`server.rs:700-710` metrics server 挂在 `config.metrics_addr`，默认 `[::]:9090`。**9090 的 `bytes_sent`/`bytes_recv` 直接接 Prometheus 就是你的账单曲线。**

> 云端聚合 / dashboard / 按量计费 → [09-iroh-services.md](09-iroh-services.md)。

---

# 3. Security & Privacy

## 3.1 发布出去的记录里到底有什么

### 1. iroh 的记录里根本没有 hostname 字段

`enum IrohAttr`（`iroh/iroh-dns/src/attrs.rs:82-89`）**只有三个变体**：`Relay` / `Addr` / `UserData`。所以「向公共 DHT 广播主机名」这类泄露在 iroh 模型下**不可能由 iroh 自己引入**。

`grep -rn "hostname|host_name|gethostname|whoami|username|device_name"` 覆盖 `iroh-dns/src/` 与 `iroh/src/address_lookup*`：**发布路径零命中**（仅 dns.rs 两处是「解析 URL 里的主机名」，与发布无关）。

mDNS 侧同理：SRV 的 target 是 `{base32-id}-{port}.local.`（`swarm-discovery/src/sender.rs:181`），**不是 `gethostname()`** —— 对 swarm-discovery 全仓 grep `hostname|gethostname|host_name` **零命中**。

**结论**：泄露面被收窄到唯一一个你完全可控的字段 —— `user_data`。

> **libp2p 对照**：libp2p 的 **Identify 协议默认就会向每个连上的 peer 广播 agent_version / protocol_version / listen_addrs**（很多项目正是在这里泄露主机名或内网地址）；iroh **没有 Identify 等价物**，元数据面默认是空的——**默认更保守**。

### 2. user_data 是 endpoint 全局的，且 AddrFilter 永远剥不掉它

```rust
// iroh-dns/src/endpoint_info.rs:70-76 —— 可发布字段只有两个
pub struct EndpointData {
    addrs: Vec<TransportAddr>,
    user_data: Option<UserData>,
}
// iroh/iroh/src/endpoint.rs:205
address_lookup_user_data: Default::default(),   // == None

// endpoint.rs:631-642 / :1661 —— 唯一注入点，需显式调用
pub fn user_data_for_address_lookup(mut self, user_data: UserData) -> Self { ... }
pub fn set_user_data_for_address_lookup(&self, user_data: Option<UserData>)
```

类型 `pub struct UserData(String)` 上限 **245 字节**（`endpoint_info.rs:314`），文档明说 *"Iroh does not keep track of or examine the user-defined data"*。

**关键**：`EndpointData::apply_filter`（`endpoint_info.rs:189-199`）在过滤后**显式把 user_data 重新挂回**：

```rust
pub fn apply_filter(&self, filter: &AddrFilter) -> Cow<'_, Self> {
    match self.filtered_addrs(filter) {
        Cow::Borrowed(_) => Cow::Borrowed(self),
        Cow::Owned(addrs) => {
            let mut data = EndpointData::new(addrs);
            data.set_user_data(self.user_data.clone());   // ← :195 user_data 被原样带过去
            Cow::Owned(data)
        }
    }
}

// endpoint_info.rs:229-230 —— filter 的签名里根本看不到 user_data
type AddrFilterFn = dyn Fn(&Vec<TransportAddr>) -> Cow<'_, Vec<TransportAddr>> + Send + Sync + 'static;
```

> **AddrFilter 在任何层都不可能剥掉 user_data。** 这是最容易误判的一条：以为「加了 relay_only 就安全了」。真相是 filter 的函数签名里根本没有 user_data，无从过滤。
>
> **要不发布 user_data，唯一办法是不设它（默认即不设）或 `set_user_data_for_address_lookup(None)`。防线在 user_data 那一侧，不在 AddrFilter。**
>
> 想做到「mDNS 广播设备名、DHT 不广播」，唯一办法是**自己包一层 AddressLookup** —— 这不是推断，是源码级事实。

**任何往 user_data 里塞可识别信息（设备名/用户名/机器码）的代码，都会被公开发布到 dns.iroh.link 且全球可无鉴权 GET。这是唯一需要 code review 卡住的 API。**

### 4. ⚠️ mDNS 与 DHT 的默认过滤策略**完全相反**

| | 默认 filter | 后果 |
|---|---|---|
| **DHT** | `AddrFilter::relay_only()`（lib.rs:169，注释明写 *"This avoids leaking IP addresses to the public DHT"*） | 不泄 IP |
| **mDNS** | `AddrFilter::default()`（lib.rs:**173**）| **不过滤** —— 广播全部本地 IP + relay + user_data |

`AddrFilter::default()` 就是恒等过滤器：`#[derive(Clone, Default)] pub struct AddrFilter(Option<Arc<AddrFilterFn>>)`（`endpoint_info.rs:242-243`），Default 即 `None`，apply 走 `None => Cow::Borrowed(addrs)` 原样返回（:279-284），Debug 甚至直接打印 `"identity"`（:250）。mDNS 模块文档也自认（lib.rs:42-43）*"By default, MdnsAddressLookup publishes all addresses it receives: direct IP addresses and up to one RelayUrl"*。

**同时装两个时，你以为设过 filter 了，其实只有一半生效。**

**公共 WiFi 上，默认配置下链路内任何人被动嗅 5353 就能收集：EndpointId + 全部内网 IP + relay URL + user_data。** 局域网直连本来就需要 IP，所以不能无脑 `relay_only()` —— 缓解手段是 `AddrFilter::ip_only()` + `service_name("你的应用名")` 隔离 + user_data 留空。

> **libp2p 对照**：libp2p Kademlia 的 provider/peer record 会把 listen_addrs（常含 192.168.\*/10.\* 内网地址）**原样进 DHT**，且没有内建的「只发 relay 不发 IP」开关；iroh 把地址发布做成一等公民的可插拔 `AddrFilter`，且默认最小化。

### 5. DHT 键 = SHA1(EndpointId)，无 salt、无 ACL，记录只签名不加密

键的算法（`n0-mainline/src/common/mutable.rs:46-58`）：

```rust
pub fn target_from_key(public_key: &[u8; 32], salt: Option<&[u8]>) -> Id {
    let mut encoded = vec![]; encoded.extend(public_key);
    if let Some(salt) = salt { encoded.extend(salt); }
    let mut hasher = Sha1::new(); hasher.update(&encoded); /* ... */
}
```

iroh 侧调用时 **salt 恒为 None**（查：`iroh-mainline-address-lookup/src/lib.rs:116`；发布：lib.rs:44-52，最后一参亦 None，且传的 `packet.encoded_packet()` 是**明文 DNS 包**）。

**含义**：任何知道你 EndpointId 的人都能算出 target 并查到你的 relay（乃至 IP，若 unfiltered）。**EndpointId 本身就是一个长期有效的定位能力（bearer capability）**。

若你的产品是配对模型（双方长期持有对方 EndpointId），这意味着**一次配对 = 永久授予对方（以及任何窃取到该 ID 的人）定位你的能力**，只要你开着 DHT 发布（每小时 republish，**窗口是永久**）。**缓解手段**：只在「可被发现」开关打开时才 add DHT lookup。

### 6. AddrFilter 挡不住「你的 IP 暴露给谁」

DHT publish/lookup 是**裸 UDP**（`n0-mainline/Cargo.toml` 依赖 `noq-udp` + tokio `net`）。默认 bootstrap 是硬编码的公共 BT 基础设施（`n0-mainline/src/actor/config.rs:3-8`）：

```rust
pub const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881", "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401", "relay.pkarr.org:6881",
];
```

所以源 IP 必然暴露给这 4 个节点 + 迭代查询沿途所有节点；lookup 还额外泄露「你在找哪个 EndpointId」。BEP42 的存在（`n0-mainline/src/common/id.rs:84-107`，DHT 节点 ID 由 IP 派生）进一步说明这一层与 IP 强绑定。

**「E2E 加密」容易被理解成「没人知道我在跟谁传」—— 开 DHT 后这条不成立。** 观察者（跑几个 DHT 节点即可，成本极低）能看到「IP a.b.c.d 在查 EndpointId X」，交集分析可还原社交图谱。

相比之下 pkarr 只把这些暴露给 n0 一家（走 HTTPS）—— **不是「DHT 更私密」，而是「换了个信任对象」**。

## 3.2 relay 的信任模型

### relay 服务器没有 iroh 身份

```rust
// iroh-relay/src/server.rs:105-119 / :121-147 —— 全无 secret_key 字段
pub struct ServerConfig { pub relay: Option<RelayConfig>, pub quic: Option<QuicConfig>, pub metrics_addr: ... }
pub struct RelayConfig { pub http_bind_addr: SocketAddr, pub tls: Option<TlsConfig>,
                         pub limits: Limits, pub key_cache_capacity: Option<usize>,
                         pub access: Arc<dyn DynAccessControl> }
```

**relay 是「按 EndpointId 转发密文」的哑管道，没身份也无需身份。** 客户端身份由 relay 的 handshake 校验（ServerChallenge → ClientAuth），**不是 mutual 的**。所以自建 relay 的信任模型 = 「相信它不做流量分析」，而不是「相信它的公钥」。**纯 HTTP 自建 relay 的 URL 可被中间人劫持**（虽然 payload 仍是端到端加密的）。

> ⚠️ **精确表述**：**服务端侧文件**（`server.rs` / `server/http_server.rs` / `main.rs`）的 SecretKey 命中全部落在 `#[cfg(test)] mod tests` 内。但**客户端侧**（`client.rs:328`、`protos/handshake.rs:225/254/342`、`client/conn.rs:90`）在非测试代码里正常使用 SecretKey 签 challenge——因为 iroh-relay 这个 crate 同时装着 relay 客户端和服务端。**别说成「SecretKey 在 iroh-relay 里只出现在测试代码」。**

> **libp2p 对照**：libp2p 的 relay 是个完整 libp2p 节点，有 PeerId，客户端 dial 时 multiaddr 里带 `/p2p/<relay-peer-id>` 且会做身份校验。**iroh relay 只是 URL，没有公钥钉扎**。运维心智也完全不同：libp2p relay 是 libp2p 身份 + noise 加密，**没有 web PKI / Let's Encrypt 那套**；iroh relay 是个 HTTP 服务器，TLS 是 web 那一套。

### 无 TLS 时客户端认证静默降级

```rust
// iroh-relay/src/protos/handshake.rs:251-269
impl KeyMaterialClientAuth {
    /// Generates a client's authentication ... by using TLS keying material instead of a received challenge.
    pub(crate) fn new(secret_key: &SecretKey, io: &impl ExportKeyingMaterial) -> Option<Self> {
        let key_material = io.export_keying_material(...)?;   // 无 TLS → None
        ...
    }
}
// handshake.rs:340-378 —— 拿不到 keying material 就走 challenge-response（多一个 RTT）
```

两条路径见 `protos/handshake.rs:8-25`：TLS keying material 走 RFC 5705 省一个 RTT，但 *"it relies on the keying material extraction feature of TLS, which is not available in browsers"*；否则回退 ServerChallenge/ClientAuth 签名挑战。

**这个降级是静默的、只有一行 debug 日志**（`client.rs:328-335`）。**纯 HTTP 自建 relay 能连上、能用，只是每次建连多一个 RTT。别以为「没配 TLS 但连上了」说明 auth 被跳过了——auth 一直在做，只是换了路子。**

## 3.3 谁能用我的 relay：准入控制

### 准入控制（四档，1.0.0 已 GA）

```rust
// main.rs:158-197
enum AccessConfig {
    Everyone,                          // default
    Allowlist(Vec<EndpointId>),
    Denylist(Vec<EndpointId>),
    Http(HttpAccessConfig),
    #[serde(rename = "shared_token")]
    SharedToken(Vec<String>),
}
```

TOML 形状（`main.rs:829-909` 的测试）：`access = "everyone"` / `access.allowlist = [...]` / `access.http.url = "..."` / `access.shared_token = ["token-a", "token-b"]`。

**EndpointId 不可伪造**（`server.rs:226-231` doc）：*"The relay handshake authenticates this id before the access hook is invoked. The client proves possession of the secret key for this public key by either signing keying material exported from the TLS session or a challenge issued by the server."*

环境变量覆盖：`main.rs:38-40` `IROH_RELAY_HTTP_BEARER_TOKEN` / `IROH_RELAY_ACCESS_TOKEN`（:231-241 env 版单 token 覆盖整个列表，空 token 启动即失败）。

**吊销限制**（`README.md:84`）：*"**Note:** this shared token does not support revocation other than updating the config and restarting the service."* 需要动态吊销就自己实现 `AccessControl`（`server.rs:285-305` trait，含 `on_connect` + `on_disconnect(endpoint_id, connection_id)`），完整可运行范例见 `iroh/iroh-relay/tests/runtime_auth.rs`。

#### ⚠️⚠️ access.http 的 header 名：文档和代码互相矛盾

| | 值 |
|---|---|
| **代码实际发出的** | `X-Iroh-NodeId` —— `main.rs:36` `const X_IROH_ENDPOINT_ID: &str = "X-Iroh-NodeId";`，`main.rs:319` `.header(X_IROH_ENDPOINT_ID, endpoint_id.to_string())` |
| **rustdoc 说的** | `X-Iroh-Endpoint-Id` —— `main.rs:168-170` `AccessConfig::Http` 的 rustdoc |

**照 rustdoc 去实现鉴权服务、按 `X-Iroh-Endpoint-Id` 取 header 的人，会拿到 None 然后拒绝掉每一个连接** —— 而且因为 relay 侧只会打 warn、鉴权服务侧看起来「工作正常」，**这是个排查成本极高的坑**。

（注意常量**名**叫 ENDPOINT_ID 但线上 header **字面量**是 X-Iroh-NodeId —— 双重迷惑。）

callout 语义：relay 每来一个连接就 POST 你的服务，你回 200 + 文本 `true` 才放行（`main.rs:329-333` 严格判等 `text == "true"`）。


## 3.4 iroh-blobs 零加密原语 —— 与应用层 E2E 加密正面冲突

#### 1. 全库零加密原语 —— 与应用层 E2E 加密正面冲突

`grep -rniE "encrypt|chacha|aead|xsalsa|cipher" src/` 在整个 src/ 下 **0 匹配**。落盘就是裸文件：`store/fs/options.rs:28-42` 的 `data_path()` = `{hash}.data`、`outboard_path()` = `{hash}.obao4`。

鉴权唯一钩子是 provider events（见下文第三部分）。但 `BlobsProtocol::new(store: &Store, events: Option<EventSender>)`（`net_protocol.rs:72-79`）传 None 时 `events.unwrap_or(EventSender::DEFAULT)` —— **默认不拦截**，README 示例正是传 None。

**若你的应用层已有自己的加密，必须二选一，没有中间路线**：

- **(A) 先加密再入库** → hash 变成密文哈希；若 nonce/密钥按接收方派生，同一文件对不同接收方是不同 blob → **内容寻址的去重/复用价值归零**，只剩一个笨重的 store
- **(B) 存明文靠 QUIC TLS** → blob 在磁盘上是明文、**hash 即凭证**（谁有 hash + 能连上就能拉，除非自己接 Intercept 写鉴权）

**这条比性能/体积都更该先拍板。**

> 段末「鉴权唯一钩子是 provider events」的完整档位表 → [02-connecting.md](02-connecting.md) 的 Endpoint Hooks 一节。

## 3.5 fast-apple-datapath 的私有 API 风险（被高估了）

#### 真相：私有符号是 dlsym 动态解析的，且从没人调用过

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

## 3.6 会不会 phone-home

#### ✅ 开源 iroh 与它零耦合 —— 锁定风险不成立

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

#### 结论

**Iroh Services（API Keys / Billing / Managed Relay / Metrics）是纯选配 SaaS 观测面，边界清晰地落在「一个需要显式构造 + 显式凭证的 client crate」上。用开源版缺的只是云端 dashboard，不缺任何传输能力。**

**真正不开源的是 iroh-doctor `swarm-client` 的 coordinator（n0des 后端）。**

> **完整的锁定边界分析**（开源/闭源边界、默认配置的静默依赖、三档脱钩方案、脱钩检查清单）
> → [09-iroh-services.md](09-iroh-services.md)。

## 3.7 构建产物里的绝对路径

`--remap-path-prefix` 是 Rust-only 的；`ring` 这类依赖经 build.rs + `cc` crate 编 C 源码，
需要额外的 `-ffile-prefix-map`。**没有这一步，你的 `.a` 里会嵌着构建机的绝对路径 —— 既泄露又不可复现。**
可抄的完整 RUSTFLAGS/CFLAGS 组合 → [05-languages.md](05-languages.md) 的「可复现构建」一节。
