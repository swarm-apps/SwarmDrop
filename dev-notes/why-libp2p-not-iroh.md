# 为什么 SwarmDrop 暂不迁移 Iroh

> 状态：已决策  
> 日期：2026-07-17  
> 决策：继续使用 libp2p 作为网络协议栈，同时学习 Iroh 的架构与 API 设计，重构 SwarmDrop 自己的网络内核。

## 背景

SwarmDrop 最初使用 libp2p，并不是因为产品必须绑定某个框架，而是因为项目本身也承担了学习和实践 P2P 协议的目标。随着项目从 Demo 发展成可用应用，现有网络层已经覆盖：

- mDNS 局域网发现；
- Kademlia DHT；
- Circuit Relay 与 DCUtR；
- AutoNAT；
- Request-Response 与独立数据流；
- 设备配对、在线状态和文件传输；
- 桌面端、移动端共享的 Rust 核心。

Iroh 提供了更现代、更聚焦的 Rust API，并将身份、寻址、QUIC、打洞和 Relay fallback 组合成统一的 `Endpoint`。它让我们看到：P2P 网络内核不应该把底层事件循环、连接状态和协议细节暴露给每一个应用宿主。

因此，我们曾评估使用 Iroh 替换 libp2p。但调研后的结论不是“Iroh 不好”，而是：**直接迁移不能解决 SwarmDrop 当前最重要的问题，反而会牺牲一部分已经拥有的开放性、协议能力和跨端路线。**

## 决策

SwarmDrop 暂不从 libp2p 迁移到 Iroh。

我们继续使用 libp2p 提供传输、发现、路由、中继和浏览器互操作能力，同时重新设计 `swarm-p2p-core` / `crates/core` 的上层接口，使其具备接近 Iroh 的使用体验：

> 保留 libp2p 的开放协议生态，学习 Iroh 的架构边界和 API 表达。

这不是维持现状。底层技术选型保持不变，但网络内核的职责、接口和产品协议仍需要重构。

## 为什么不直接迁移 Iroh

### 1. Iroh 不能一对一替换 libp2p

libp2p 是一组可组合的开放协议，Iroh 更接近一套围绕 QUIC Endpoint 构建的完整网络运行时。两者抽象层次不同。

迁移后并不会自动获得 SwarmDrop 完整的产品网络层。以下能力仍需自行保留或重新实现：

- 设备配对和信任关系；
- 邀请授权、过期和撤销；
- 在线状态与已配对设备发现；
- 文件 Offer/Accept、进度、取消和续传；
- 未配对设备的访问控制；
- Web、CLI、桌面和移动宿主之间的统一协议。

换句话说，迁移主要改变连接运行时和 API，不会替代 SwarmDrop 的产品协议。

### 2. SwarmDrop 需要的协议组合，libp2p 已经具备

当前项目已经拥有并实际使用 mDNS、Kademlia、Relay、DCUtR、Request-Response 和流协议。桌面设备还可以在完全局域网模式中充当帮助节点，为其他设备提供 DHT 与 Relay。

迁移 Iroh 意味着：

- 放弃已有的 Kademlia 模型，重新设计跨网络发现；
- 将 libp2p Relay/DCUtR 切换为 Iroh 自己的 Relay 与寻址体系；
- 更换 PeerId、地址和连接协议，造成 wire protocol 不兼容；
- 桌面端和移动端必须同步切换，旧版本之间无法继续通信；
- 已有配对身份不能被静默视为新的 Iroh 信任关系。

这些都是完整迁移成本，而不是替换几个 API 调用。

### 3. Web 端更需要协议互操作，而不是统一使用同一个 Rust 库

浏览器是 SwarmDrop 的重要方向：用户可能不愿安装应用，只想打开分享链接就与已配对设备传输文件。

浏览器不能像原生应用一样自由使用 UDP socket。Iroh 的浏览器实现因此不能等价复用原生端的 QUIC 打洞路径，通常需要经过 Relay；其生态中的浏览器持久化和大文件传输能力也不是 SwarmDrop 可以直接依赖的终局方案。

libp2p 则同时存在 Rust 和 JavaScript 实现，并定义了 WebRTC、WebRTC Direct、Circuit Relay 等互操作协议。Web 端不必强求复用原生 Rust 网络运行时，而可以：

- 原生端继续运行 rust-libp2p；
- Web 端使用 js-libp2p；
- 双方通过标准 libp2p 协议互操作；
- 浏览器到原生设备优先使用 WebRTC Direct；
- 无法直连时再通过 Circuit Relay；
- 大文件落盘、续传和授权继续由 SwarmDrop 自有传输协议负责。

这种“共享协议和类型，不强求共享运行时”的方式更符合浏览器平台约束。

### 4. libp2p 的开放性更符合项目长期目标

Iroh 的核心、Relay 和发现组件是开源的，也允许自托管，因此不能简单地说 Iroh 是封闭方案。但两者的开放形态不同：

- Iroh 主要由一个团队围绕一套 Rust 实现和默认基础设施推进；
- libp2p 有独立协议规范和多语言实现；
- libp2p 的传输、发现、路由和中继可以分别替换；
- 节点可以完全使用自建的 DHT、Relay 和引导网络运行；
- 浏览器与原生应用可以通过协议互操作，而不依赖同一家实现。

SwarmDrop 不只追求“现在能连上”，还希望理解协议、控制部署边界，并逐步支持 Web、CLI、局域网帮助节点，甚至探索新的传输方式。libp2p 的协议优先和可组合特性更匹配这个方向。

### 5. API 更好，不等于应该更换协议栈

Iroh 最有吸引力的地方，是它让常见连接流程变得简单：

- 一个 `Endpoint` 统一身份、监听和连接；
- 使用 EndpointId 表达远端身份；
- Router/ProtocolHandler 按协议分发连接；
- Ticket 携带建立连接需要的信息；
- Relay fallback、地址更新和路径选择由运行时封装；
- 应用主要面对连接和流，而不是持续轮询庞大的 Swarm 事件枚举。

这些优点主要属于**架构与 API 设计**，并非只有使用 Iroh 才能获得。SwarmDrop 可以在 libp2p 上建立自己的应用层封装，把复杂事件循环收进内核，同时保留底层协议生态。

## 我们要向 Iroh 学什么

### 1. Endpoint，而不是裸 Swarm

业务层不应直接驱动 `Swarm::select_next_some()`，也不应理解每一种 Behaviour 事件。

新的网络入口应接近：

```rust
let endpoint = Endpoint::builder(config)
    .identity(identity)
    .protocol(control_protocol)
    .protocol(transfer_protocol)
    .bind()
    .await?;

endpoint.connect(device_id).await?;
endpoint.shutdown().await?;
```

`Endpoint` 内部拥有 libp2p Swarm 和事件循环，对外只暴露稳定的命令、状态与领域事件。

### 2. 按协议路由，而不是巨型事件分支

参考 Iroh 的 Router/ALPN 思路，为 SwarmDrop 定义清晰的协议边界：

- control：配对、授权、设备信息与能力协商；
- transfer：文件 Offer/Accept、数据流、续传和取消；
- presence：已配对设备在线状态；
- diagnostics：连通性和路径诊断。

在 libp2p 中可以分别映射为 request-response 协议、stream protocol 或专用 Behaviour，但这些实现细节不能泄漏到产品层。

### 3. Ticket 体验，而不是绑定 Iroh Ticket 格式

将当前 6 位配对码升级为 SwarmDrop 自己的签名邀请凭证 `PairInvite`：

- 使用高熵随机 ID，避免短码枚举；
- 包含邀请方身份和候选地址；
- 包含签发时间、过期时间、用途和协议版本；
- 使用设备长期密钥签名；
- 支持一次性使用和撤销；
- 同一内容编码为二维码、分享链接和 CLI 参数。

表现形式可以是：

```text
https://swarmdrop.app/pair#<encoded-invite>
swarmdrop://pair/<encoded-invite>
swarmdrop pair <invite>
```

Ticket 是产品协议，不应与某个网络库的类型绑定。未来即使更换传输层，邀请链接仍可保持兼容。

### 4. 连接与授权分离

建立加密连接只证明“正在与某个密钥对应的节点通信”，不代表该节点获得了访问权限。

新内核必须在建立连接后、处理业务请求前完成应用层授权：

1. 验证远端 PeerId；
2. 查询本地配对关系；
3. 验证邀请用途、过期时间和一次性状态；
4. 协商协议版本与设备能力；
5. 未授权时立即拒绝业务协议并断开连接。

这与 Iroh Endpoint Hooks 所强调的连接门控思想一致，但最终规则属于 SwarmDrop。

### 5. 简单的连接路径模型

产品层只需要理解少数稳定状态：

- `Local`：局域网直连；
- `Direct`：跨网络直连；
- `Relayed`：经中继转发；
- `Offline`：当前不可达。

mDNS、Kademlia、AutoNAT、DCUtR、WebRTC 或具体 Multiaddr 都留在网络适配层。UI 和业务代码不应根据某个 Behaviour 事件推导产品状态。

### 6. 默认内建网络诊断

参考 `iroh-doctor`，SwarmDrop 应提供统一诊断能力：

- UDP、TCP、WebSocket/WebRTC 可达性；
- NAT 类型和公网地址；
- 引导节点与 Relay 延迟；
- mDNS 是否工作；
- 当前连接使用直连还是中继；
- 可复制的脱敏诊断报告。

诊断模块通过 SwarmDrop 自己的类型输出，不把 libp2p 内部类型暴露给 Tauri、React Native、CLI 或 Web。

## 目标架构

```text
┌─────────────────────────────────────────────────────────┐
│ Tauri / React Native / CLI-TUI / Web                    │
└──────────────────────────┬──────────────────────────────┘
                           │ 稳定的 Command / Event / Types
┌──────────────────────────▼──────────────────────────────┐
│ SwarmDrop Core                                          │
│ 配对 · 信任 · 授权 · Presence · Transfer · Diagnostics │
└──────────────────────────┬──────────────────────────────┘
                           │ Endpoint / Protocol 接口
┌──────────────────────────▼──────────────────────────────┐
│ SwarmDrop Network Runtime                               │
│ 隐藏事件循环 · 连接管理 · 协议路由 · 地址选择           │
└──────────────────────────┬──────────────────────────────┘
                           │ Transport Adapter
┌──────────────────────────▼──────────────────────────────┐
│ libp2p                                                  │
│ QUIC/TCP · WebRTC · Noise/TLS · mDNS · KAD · Relay     │
└─────────────────────────────────────────────────────────┘
```

关键边界是：

- 上层依赖 SwarmDrop 的领域接口，不依赖 `SwarmEvent`；
- 网络运行时可以使用 libp2p，但 libp2p 类型不穿透 FFI/IPC；
- Web 端可以使用不同实现，只要遵循相同的 SwarmDrop wire protocol；
- 邀请、授权和传输协议不绑定 PeerId 的具体序列化形式；
- 将来若再次评估 Iroh，只需替换网络适配层，而不是重写产品核心。

## 保留与重构范围

| 范围 | 决策 |
|---|---|
| libp2p QUIC/TCP、Noise/TLS | 保留 |
| mDNS、Kademlia、Circuit Relay、DCUtR、AutoNAT | 保留 |
| 当前 Swarm 事件循环 | 收进 Network Runtime，不再向业务层暴露 |
| `request_response` | 可继续用于短控制请求，但封装为 SwarmDrop control protocol |
| 文件数据流 | 保留独立 stream protocol，不通过巨型 RPC 消息传大文件 |
| 6 位 DHT 配对码 | 由签名、可过期的邀请链接替换 |
| Tauri commands | 保留为桌面宿主适配层，不承担网络核心职责 |
| 移动端 UniFFI | 绑定稳定的 Core API，不绑定 libp2p 内部类型 |
| Web 端 | 使用 js-libp2p/浏览器适配器，共享 wire schema 与生成的 TypeScript 类型 |
| CLI/TUI | 优先连接共享后台内核；受限设备可运行 headless node |
| Iroh | 作为设计参考和持续观察的备选网络适配器 |

## 这项决策不意味着什么

- 不意味着 libp2p 当前 API 已经足够好；恰恰相反，网络内核仍需大幅简化。
- 不意味着永远不使用 Iroh；未来可以重新评估，甚至作为实验性适配器并存。
- 不意味着所有平台必须运行相同的 libp2p 实现；我们追求 wire compatibility，而不是 binary reuse。
- 不意味着继续依赖公共 DHT 或公共 Relay；完全局域网和自托管基础设施仍是一等模式。
- 不意味着复制 Iroh 的所有 crate；我们只吸收与 SwarmDrop 问题匹配的设计。

## 重新评估 Iroh 的条件

只有出现以下变化之一，才值得重新启动整体迁移评估：

1. Iroh 提供成熟、稳定的浏览器直连能力，并能满足大文件持久化与续传；
2. Iroh 的多语言或 wire protocol 生态允许浏览器与原生实现独立互操作；
3. libp2p 的维护成本成为可量化的主要瓶颈，而不是单纯觉得 API 复杂；
4. SwarmDrop 不再需要 Kademlia、自建帮助节点或多传输协议组合；
5. Iroh 能带来经真实网络测试验证的显著连接成功率提升，足以覆盖身份、协议和版本迁移成本；
6. 网络适配层完成后，可以通过小范围实验接入 Iroh，而不影响产品协议和存量用户。

在此之前，不再以“API 更简洁”为理由推动 wire-incompatible 的整体迁移。

## 后续行动

1. 将现有网络事件循环封装为内部 Runtime；
2. 设计稳定的 `Endpoint`、`Connection`、`ProtocolHandler` 和领域事件接口；
3. 将六位配对码替换为签名邀请链接，并统一二维码、链接和 CLI 输入；
4. 将控制协议、Presence、文件数据流拆分为明确的协议模块；
5. 清理 Core/FFI/IPC 中泄漏的 libp2p 类型；
6. 为 Rust 与 TypeScript 生成同源 wire schema；
7. 做 js-libp2p WebRTC Direct 与 rust-libp2p 的互操作 PoC；
8. 增加类似 `iroh-doctor` 的跨端网络诊断模块；
9. 将旧的 Iroh 迁移文档视为调研材料，不再作为已批准的实施路线。

## 参考资料

- [Iroh FAQ](https://docs.iroh.computer/about/faq)
- [Iroh Public Relays](https://docs.iroh.computer/iroh-services/relays/public)
- [Iroh Services](https://docs.iroh.computer/iroh-services)
- [libp2p 协议与实现](https://github.com/libp2p/libp2p)
- [libp2p Specifications](https://github.com/libp2p/specs)
- [邀请链接与二维码配对设计](architecture/iroh-invite-link-pairing-design.md)
- [Iroh 跨平台架构调研](archive/recon-2026-07/iroh-cross-platform-context.md)
- [Iroh 迁移评估知识库](knowledge/iroh-migration.md)
