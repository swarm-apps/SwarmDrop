# generalize-node-assembly 设计决策

基于 2026-07-20 三端+core 装配架构测绘（workflow `map-node-assembly`，5 agent 源码级、
全部论断带 file:line）。前提：用户已拍板「web 包一层 core / 装配拆成可注入积木 / web 走完整配对」。

## D1：共享层落 crates/core，不新建 crate

两条硬证据否决「新开 `swarmdrop-node` 装配 crate」：

1. **core 已是组合根**：`runtime.rs:49` `start_node` 就是「怎么拼成 SwarmDrop 节点」的唯一权威处，net/transfer/host 是它调用的可复用积木。新建装配 crate 只会掏空 core + 加一跳，收益为负。
2. **core 已 wasm-aware**：`core/Cargo.toml:40-44` 有 wasm target-deps（getrandom/uuid js feature），`:11-12` 注释明写 wasm 上走 n0-future，production 不依赖 sea-orm/storage-sql（`:54-55` 仅 dev-dep 给 e2e）。编到 wasm 的地基已铺，不是从零。

## D2：`EndpointProfile`——可注入积木（非单函数分支）

用户在「拆成可注入积木」与「start_node 加 config 参数内部长分支」之间选了前者（平台中立、单一职责、正交）。形态：

```rust
/// 把 build_endpoint 里 Native-hardcoded 的四处收成可注入策略。
pub struct EndpointProfile {
    preset: Preset,                         // presets::Native | presets::Browser
    address_lookup: Option<LookupBuilderFn>,// Native=OnlineRecordLookup；Browser=None/轻量
    register_infra: bool,                    // Native=true（跑 bootstrap_node_addrs 循环）；Browser=false
    relay_server: Option<RelayServerConfig>, // Native+lan_helper=Some；Browser=None（是 relay_client）
}

impl EndpointProfile {
    pub fn native(network_config: &NetworkRuntimeConfig) -> Self { /* 现状原样 */ }
    pub fn browser() -> Self { /* Browser preset + relay_client，无 infra/relay_server */ }
}
```

`build_endpoint(secret_key, os_info, &network_config, &profile)`：DHT server_mode 仍由 `provide_lan_helper` 决定（Browser 恒 client），preset/address_lookup/relay_server 从 profile 取，infra 注册循环按 `profile.register_infra` 跳过。

> 影响面：`start_node` 签名 +1 参（`profile`）、`os_info` 从内构改为入参。牵动**两个**调用点（desktop `lifecycle.rs:64`、mobile `network.rs:214`），逐行对称改动，行为不变。

**实现精化（2026-07-20 落地）**：上面把 profile 画成「存 preset 值的 struct」，但 `Builder::preset(self, preset: impl Preset)`（`crates/net/src/endpoint/builder.rs:60`）是**立即 `preset.apply(self)`**——preset 是「一串 setter 打包」而非可存储的值，`Box<dyn Preset>` 也因 `apply(self)` 取 self 而不可 dyn。故落地为**枚举判别**，`build_endpoint` 在 `match profile` 里调对应 `.preset(presets::Native|Browser)` 并按分支挂/不挂 `address_lookup`：

```rust
pub enum EndpointProfile { Native, Browser }          // Copy
impl EndpointProfile { fn registers_infra(self) -> bool { matches!(self, Self::Native) } }
// build_endpoint: match profile { Native => .preset(Native).address_lookup(..), Browser => .preset(Browser) }
// relay_server 仅 registers_infra() && provide_lan_helper；infra 循环仅 registers_infra()
```

枚举同样满足「可注入积木」的取舍（调用方注入 profile 判别），且比 struct-of-closures 更贴合 preset 的立即-apply 语义。回归锚点 `runtime::tests::both_profiles_bind` 证两形态均 bind 成功。

## D3：`os_info` 提为入参——wasm env 探测恒 unknown

现状 `start_node` 用 `OsInfo { name: device_name, ..OsInfo::default() }`（`runtime.rs:60`），`OsInfo::default()` 走 env 探测——**wasm 下恒 `unknown`**。web 已有 `web_os_info()`（`node.rs:345`）绕开。泛化后 `os_info: OsInfo` 提为 `start_node` 入参：native 端传 env 探测值，web 传 `web_os_info()`。

**契约红线**：`agent_version` 必须走 `os_info.to_agent_version()`（`"swarmdrop/{ver}; os=…"`）——桌面 `DeviceManager` 用 `AGENT_PREFIX` 过滤设备列表，前缀不对 Web 节点会在对端设备列表里**隐身**（`node.rs` 注释「曾踩过 `swarmdrop-web/0.1` 被整个滤掉」）。泛化不得破坏此契约。

## D4：web 走完整配对——为什么，以及它如何简化泛化

用户选「补真正的 invite 配对 + 持久化」而非维持 demo 级。副作用是**装配收敛**：

| 装配层 | 桌面/移动 | web（选完整配对后） |
|---|---|---|
| NetManager（pairing/devices/presence） | 装 | **同装**（复用） |
| Router 协议集 | 3 协议 | **3 协议**（复用，含 pairing） |
| EndpointProfile | native | **browser**（唯一真分叉） |
| TransferStore | SQL（闭包注入） | Memory/OPFS（闭包注入） |
| PairingStore | SQL+keychain | **IndexedDB/OPFS（新写）** |
| os_info / 身份来源 | env / keychain | web_os_info / localStorage+OPFS |

即：web 不再是「第三个 2 协议无配对的异形」，NetManager+router 层直接复用，泛化只需在 **endpoint 轴**（profile）+ **端口后端**（本就注入）动刀。合成 `WebPeerDirectory`（`node.rs:122-125`）退役，换真 `PairingManager`——它此前故意给 `Some` 绕过 `incoming.rs` 对未配对设备的 `NotPaired` 硬拒，现在有了真配对记录，安全边界与桌面对齐。

## D5：web `PairingStore` 落点——IndexedDB vs OPFS

三端第一个「不是 SQL、不是 keychain」的配对持久化。两个候选：

| | IndexedDB | OPFS |
|---|---|---|
| 结构化查询 | ✅ 原生 KV+索引，适合按 NodeId 查信任设备 | ❌ 纯文件，要自己序列化整表 |
| 已在用 | web 身份已用 localStorage/OPFS | OPFS 已用于落盘（`OpfsFileAccess`） |
| 事务/并发 | ✅ 事务 | ⚠️ 手动 |
| 倾向 | **推荐**：配对记录是结构化 KV，IndexedDB 天然契合 | 若想复用现有 OPFS 栈少引一套 API 则次选 |

存的是 `PairedDeviceInfo` 集合（与桌面/移动同 schema，供 `start_node` 的 `paired_devices` 入参与 NetManager 持久化副作用消费）。InboxStore（当前 no-op）不在本 change——它是传输落盘域、非节点装配路径。

## D6：手抄 endpoint 副本——只消 web 那份，e2e 那份是测试正当代码（实现修订）

原判断「e2e 手抄了一份 build_endpoint、泛化后可删副本」在落地时被证**不准**。读 `crates/core/tests/e2e_transfer.rs` 的 `test_endpoint`（`:76`）：它只 listen `127.0.0.1`、关 mDNS、关 relay_client、DHT server_mode on——这是**为测试隔离刻意调过的端点配置**（关 mDNS 是「两个本机节点不能靠 mDNS 自动发现、否则互相串扰状态」，见其注释），**不是** `build_endpoint`（Native 形态：0.0.0.0 + mDNS on）的手抄。而且 e2e 的 `spawn_node` **已经复用了 `build_router`**（`:154`）——真正会漂移的**协议注册**早已共享。

强行把 e2e 塞进 `EndpointProfile` 有害无益：要么引入 `EndpointProfile::Test` 变体把测试专用配置（listen 127.0.0.1、关 mDNS）漏进生产枚举，要么让 e2e 走 Native profile 从而开启 mDNS、令并行测试互相串扰。故**保留 e2e 的 `test_endpoint` 原样**。

真正因「build_endpoint 从没参数化」而手抄的是 **web 那份**（`crates/web/src/node.rs:97` 直接 `Endpoint::builder()` 拼 Browser 装配）——Phase 3 让 web 包 core 后即消除。这才是本 change 收口的那处存量重复。

## D7：与 B 轨 webrtc 的接缝

B 轨（`webrtc-direct-reachability`）要给 `build_endpoint` 注入持久化 webrtc 证书。本 change 把 `build_endpoint` 参数化后，证书自然作为 `EndpointProfile`（或 `NetworkRuntimeConfig`）的一个字段注入，**给 B 轨留了干净的缝**——两轨正交并行，本轨先定形态，B 轨少 plumb 一次。非硬依赖，纯锦上添花。
