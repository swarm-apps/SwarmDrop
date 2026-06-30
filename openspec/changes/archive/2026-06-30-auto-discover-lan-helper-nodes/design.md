## Context

当前桌面端的 bootstrap/relay 配置由两部分组成：`crates/core/src/network/config.rs` 内置公网节点，以及设置页持久化的 `customBootstrapNodes`。节点启动时一次性把这些地址传入 `NodeConfig.bootstrap_peers`，`swarm-p2p-core` 连接这些 peer 后再申请 relay reservation。运行期间通过 mDNS 发现的 peer 会被 dial，Identify 成功后会加入 Kad/Swarm 地址表，但不会被识别为 infrastructure peer，也不会触发 relay reservation 或候选状态更新。

`libs/bootstrap` 已经证明了“Kad Server + Relay Server + AutoNAT Server”的公网基础设施形态。桌面 App 需要的是更轻的“局域网协助节点”：让同网段设备自动发现一台常开桌面端，并把它作为局域网内的 Kad/Relay 协助节点使用。它不能替代公网 bootstrap，因为跨网络发现仍需要一个已知发现根。

## Goals / Non-Goals

**Goals:**

- 普通用户无需手填 Multiaddr，也能自动使用同局域网内已开启协助模式的桌面端。
- 桌面端可显式开启“局域网协助节点”模式，提供受限 Kad Server 与 Relay Server 能力。
- 客户端能从多个来源合并候选：内置公网节点、用户自定义节点、mDNS 发现到的 LAN Helper。
- `swarm-p2p-core` 支持运行时注册 infrastructure peer，而不是只依赖启动时配置。
- 网络状态能展示自动发现到的协助节点、当前 bootstrap/relay 就绪状态和候选来源。
- 手动自定义引导节点保留为高级兜底能力。

**Non-Goals:**

- 不把普通桌面端默认变成 relay server。
- 不让 LAN Helper 默认提供公网 bootstrap 服务。
- 不在第一版加入 AutoNAT Server 到 LAN Helper；AutoNAT Server 继续保留给公网 bootstrap 节点形态。
- 不实现中心化远程节点目录或云端动态下发节点清单。
- 不移除内置公网 bootstrap 节点。

## Decisions

### 1. 用独立 Infrastructure 配置表达 helper/server 能力

在 `swarm-p2p-core::NodeConfig` 中新增 infrastructure 配置，而不是复用 `enable_relay_client` 或继续增加散落 bool。

建议结构：

```rust
pub enum InfrastructureMode {
    Off,
    LanHelper(LanHelperConfig),
}

pub struct LanHelperConfig {
    pub enable_kad_server: bool,
    pub relay_limits: RelayLimits,
    pub announce_private_addrs: bool,
}
```

`CoreBehaviour` 新增 `relay_server: Toggle<relay::Behaviour>`。现有 `relay_client: Toggle<relay::client::Behaviour>` 保持不变。一个节点可以同时作为 relay client 和 relay server，但两个语义必须分开：client 是“使用别人中继”，server 是“允许别人通过我中继”。

备选方案是复用 `kad_server_mode` 和新增 `enable_relay_server` 两个 bool。它更简单，但会让“桌面端基础设施模式”的安全边界、默认限额和 UI 语义分散，后续扩展 AutoNAT Server 或 PublicBootstrap 会变得凌乱。

### 2. LAN Helper 默认只声明局域网能力

LAN Helper 应默认绑定当前节点监听地址，但只向候选系统声明可用的私有网段地址，不主动注册公网 external address。relay reservation 响应需要可用的 relay 地址，因此 helper 运行时必须把可达的私有 LAN 地址加入 advertised/external address 集合，且必须过滤 `0.0.0.0`、`::`、loopback 和不可路由地址。

如果未来支持“公开协助节点”，应另开 PublicBootstrap/Profile，而不是扩大 LAN Helper 的默认行为。

### 3. 通过 SwarmDrop agent capabilities 识别 LAN Helper

`swarm-p2p-core` 不理解 SwarmDrop 业务语义，只负责把 Identify 信息完整暴露出来。`crates/core` 扩展 `OsInfo::to_agent_version()`，在用户开启协助模式时写入能力标记，例如：

```text
swarmdrop/0.5.3; caps=lan-helper; os=...; platform=...; arch=...; host=...
```

`OsInfo::from_agent_version()` 解析 `caps` 字段。这样 mDNS + Identify 之后，app 层可以判断对端是否是 LAN Helper，而无需依赖 libp2p 内部协议名。

备选方案是通过 Identify 的 `protocols` 列表推断 relay server 支持的 libp2p 协议。该方案更“底层”，但依赖 libp2p 内部协议名，且无法区分“普通 relay server”与“SwarmDrop 局域网协助节点”的产品语义。

### 4. 引入 BootstrapCandidateManager 合并候选来源

在 `crates/core` 新增候选管理层，统一管理：

```rust
enum BootstrapCandidateSource {
    BuiltInPublic,
    UserCustom,
    MdnsLanHelper,
}

struct BootstrapCandidate {
    peer_id: PeerId,
    addrs: Vec<Multiaddr>,
    source: BootstrapCandidateSource,
    roles: CandidateRoles,
    scope: CandidateScope,
    last_seen: DateTime<Utc>,
    health: CandidateHealth,
}
```

启动时加载内置公网节点和用户自定义节点；运行时从 `IdentifyReceived` + `PeersDiscovered` 识别 LAN Helper 并加入候选池。候选池负责去重、状态投影和决定何时向 core 注册。

### 5. `swarm-p2p-core` 提供运行时 infrastructure peer 注册命令

当前 relay reservation 只在启动时通过 `connect_bootstrap_peers()` 的 `bootstrap_peers` map 触发。需要新增命令，使 app 层在运行时发现 LAN Helper 后可以注册：

```rust
client.add_infrastructure_peer(peer_id, addrs, roles).await?;
```

该命令在 event loop 内完成：

- 把地址加入 Swarm 地址表。
- 把地址加入 Kad 地址表。
- dial 该 peer。
- 如果角色包含 RelayServer 且 relay client 已启用，则记录 pending relay reservation。
- 连接建立后触发 `listen_on(<relay_addr>/p2p-circuit)` 申请 reservation。

这样保留现有“连接建立后再申请 reservation”的正确时序，不回退到曾经会失败的“dial 前立即 listen_on(p2p-circuit)”模式。

### 6. 自动模式以候选优先级和健康状态驱动

默认发现模式为 Auto：

1. 内置公网节点始终作为跨网络发现根。
2. 用户自定义节点加入同一候选池，优先级高于内置节点。
3. mDNS 发现到的 LAN Helper 加入局域网候选，优先用于同网段协助和 relay reservation。
4. 如果用户选择 LAN Only，则不连接内置公网节点，只使用 mDNS 与 LAN Helper；跨网络 DHT/分享码能力会降级。

健康状态不需要第一版做复杂探测。初版以连接状态、Identify 时间、RelayReservationAccepted 事件作为健康信号即可。

### 7. 设置页从“地址列表”改为“发现策略 + 高级地址”

设置页应把“引导节点地址”收进高级区域，默认展示更面向用户的开关：

- 自动使用公网引导节点。
- 自动发现局域网协助节点。
- 本设备作为局域网协助节点。
- 高级：自定义引导节点地址。

网络状态显示：

- 公网引导：已连接/未连接。
- 局域网协助节点：发现数量。
- 中继预约：已就绪/未就绪。
- 当前候选来源：内置/自定义/局域网协助。

## Risks / Trade-offs

- **[用户误以为 LAN Helper 可以替代公网节点]** → UI 文案必须说明 LAN Helper 只自动覆盖同局域网；跨网络仍需要公网或自定义 bootstrap。
- **[桌面端被误开成开放 relay]** → 默认关闭；开启时提示流量/电量消耗；默认使用保守 relay limits；状态页显示当前 reservation/circuit/转发量。
- **[relay reservation 响应没有可用地址]** → LAN Helper 必须注册可达私有地址作为 advertised/external address，并过滤不可路由地址。
- **[mDNS 发现噪声或并行测试串扰]** → 候选必须通过 Identify 的 SwarmDrop protocol version 和 `caps=lan-helper` 双重确认，不只靠 mDNS peer。
- **[运行时动态注册改变 event loop 复杂度]** → 把逻辑封装为单个 infrastructure peer command，复用现有 bootstrap peer 连接后 reservation 时序。
- **[手动地址与自动候选冲突]** → 候选池按 peer_id 去重，来源保留集合；用户自定义来源优先级高于内置和自动发现。

## Migration Plan

1. 默认配置保持现状：继续连接内置公网 bootstrap，LAN Helper 默认关闭。
2. 新增 preferences 字段使用默认值迁移：`discoveryMode = "auto"`、`provideLanHelper = false`。
3. 旧的 `customBootstrapNodes` 保留，迁移到高级设置区域，不改变数据格式。
4. 先实现 core 能力和集成测试，再接入 UI 开关；未开启 LAN Helper 时行为应与当前版本一致。
5. 如果发现 LAN Helper 有兼容问题，可通过 UI/配置关闭自动发现，仍回退到内置公网 + 自定义地址。

## Open Questions

- 是否需要在第一版实现 relay server 的连接来源过滤，还是仅用 opt-in、LAN 地址公告和严格限额控制风险？
- LAN Helper 的 `caps` 是否只放在 agent_version，还是后续引入专门的 capabilities request-response？
- 是否需要为“只使用局域网”模式提供更强提示，例如禁用分享码跨网络功能并展示原因？
