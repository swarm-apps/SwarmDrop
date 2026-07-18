# Phase 1: 网络连接 ✅

> **状态**: 已完成
>
> **范围**: 仅桌面端 (Windows / macOS / Linux)
>
> **参考**: [implementation-roadmap.md](implementation-roadmap.md) 查看整体规划

## 目标

建立 libp2p 网络基础设施，实现设备间的网络连接能力，包括局域网发现和跨网络连接。

**注意**：本阶段仅针对桌面端。移动端将在 Phase 4 使用 HTTP 方案实现，不依赖 libp2p。

## 前置条件

- Tauri v2 项目脚手架已完成
- 基础 UI 框架已搭建

## 核心任务

### 1.1 libp2p Swarm 初始化

**目标**：创建 libp2p Swarm 实例，配置基础传输层

**技术细节**：
- 传输层：TCP + Noise（加密）+ Yamux（多路复用）
- 生成本地密钥对（Ed25519）作为设备身份
- 创建 PeerId

**实现步骤**：
1. 添加 libp2p 依赖到 `Cargo.toml`
2. 创建 `src-tauri/src/network/mod.rs` 模块
3. 实现 `SwarmManager` 结构体
4. 配置 Transport：`tcp::tokio::Transport` + `noise` + `yamux`
5. 创建 Swarm 并绑定本地端口

**Rust 依赖**：
```toml
libp2p = { version = "0.54", features = [
    "tokio",
    "tcp",
    "noise",
    "yamux",
    "macros",
    "identify",
] }
tokio = { version = "1", features = ["full"] }
```

**验收标准**：
- [x] Swarm 能成功启动并监听端口
- [x] 能生成并持久化本地 PeerId
- [x] 日志显示 Swarm 运行状态

---

### 1.2 mDNS 局域网发现

**目标**：自动发现同一局域网内的 SwarmDrop 设备

**技术细节**：
- 使用 `libp2p-mdns` 协议
- 服务名：`_swarmdrop._p2p._local`
- 发现事件通知前端

**实现步骤**：
1. 添加 `mdns` feature 到 libp2p
2. 配置 mDNS Behaviour
3. 处理 `MdnsEvent::Discovered` 事件
4. 维护已发现设备列表
5. 通过 Tauri Event 通知前端

**Tauri 命令**：
```rust
#[tauri::command]
fn get_nearby_devices() -> Vec<DeviceInfo>;

#[tauri::command]
fn set_discoverable(enabled: bool);
```

**前端事件**：
```typescript
listen('device-discovered', (event) => { ... });
listen('device-lost', (event) => { ... });
```

**验收标准**：
- [x] 同一局域网两台设备能互相发现
- [ ] 设备离线后能正确移除
- [ ] 隐身模式能阻止被发现

---

### 1.3 Identify 协议

**目标**：交换设备元信息（名称、版本、支持的协议）

**技术细节**：
- 使用 `libp2p-identify` 协议
- 交换信息：协议版本、Agent 版本、监听地址

**实现步骤**：
1. 添加 `identify` feature
2. 配置 Identify Behaviour
3. 处理 `IdentifyEvent::Received` 事件
4. 存储对方的 Identify 信息

**验收标准**：
- [x] 连接后能获取对方设备信息
- [ ] 显示对方的设备名称和版本

---

### 1.4 Kademlia DHT（跨网络发现）

**目标**：加入 DHT 网络，支持跨网络的 Peer 发现

**技术细节**：
- 使用 `libp2p-kad` 协议
- 连接公共引导节点
- 支持 Provider 发布和查询

**实现步骤**：
1. 添加 `kad` feature
2. 配置 Kademlia Behaviour
3. 实现引导节点连接逻辑
4. 实现 `PUT_PROVIDER` 和 `GET_PROVIDERS`
5. 处理 DHT 事件

**引导节点配置**：
```rust
const BOOTSTRAP_NODES: &[&str] = &[
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    // 可添加自建引导节点
];
```

**Tauri 命令**：
```rust
#[tauri::command]
async fn bootstrap_dht() -> Result<(), String>;

#[tauri::command]
fn get_dht_status() -> DhtStatus;
```

**验收标准**：
- [x] 能成功连接引导节点
- [x] DHT 路由表能填充
- [ ] 能发布和查询 Provider

---

### 1.5 Relay 协议（NAT 穿透兜底）

**目标**：当直连失败时，通过 Relay 节点中继流量

**技术细节**：
- 使用 `libp2p-relay` 协议
- 支持作为 Relay Client
- 自动选择可用的 Relay 节点

**实现步骤**：
1. 添加 `relay` feature
2. 配置 Relay Client Behaviour
3. 实现 Relay 节点发现和选择
4. 建立 Relay 连接

**验收标准**：
- [x] 能通过 Relay 连接到 NAT 后的设备
- [ ] Relay 连接稳定可用

---

### 1.6 DCUtR（NAT 打洞）

**目标**：尝试直接连接，减少对 Relay 的依赖

**技术细节**：
- 使用 `libp2p-dcutr` 协议
- 先通过 Relay 连接，再尝试打洞升级

**实现步骤**：
1. 添加 `dcutr` feature
2. 配置 DCUtR Behaviour
3. 处理打洞成功/失败事件

**验收标准**：
- [x] 部分 NAT 场景能成功打洞
- [x] 打洞失败自动降级到 Relay

---

### 1.7 连接管理

**目标**：管理所有 Peer 连接的生命周期

**技术细节**：
- 连接池管理
- 自动重连
- 连接状态通知

**实现步骤**：
1. 实现 `ConnectionManager` 结构体
2. 跟踪连接状态变化
3. 实现连接超时和重连逻辑
4. 通知前端连接状态

**Tauri 命令**：
```rust
#[tauri::command]
async fn connect_to_peer(peer_id: String) -> Result<(), String>;

#[tauri::command]
async fn disconnect_peer(peer_id: String);

#[tauri::command]
fn get_connected_peers() -> Vec<PeerInfo>;
```

**验收标准**：
- [ ] 能主动连接指定 Peer
- [ ] 连接断开能自动重连
- [ ] 前端能实时显示连接状态

---

## 项目结构

```
src-tauri/src/
├── lib.rs
├── main.rs
└── network/
    ├── mod.rs              # 模块导出
    ├── swarm.rs            # Swarm 初始化和管理
    ├── behaviour.rs        # 组合 NetworkBehaviour
    ├── discovery.rs        # mDNS + DHT 发现
    ├── relay.rs            # Relay + DCUtR
    ├── connection.rs       # 连接管理
    └── types.rs            # 共享类型定义
```

---

## 测试计划

### 单元测试

- Swarm 初始化测试
- 事件处理测试

### 集成测试

- 两台设备局域网互联
- 跨网络连接（需要两个不同网络的测试设备）
- NAT 穿透测试

### 测试工具

- `libp2p-swarm-test` 用于模拟 Swarm
- Docker 容器模拟不同网络环境

---

## 风险与对策

| 风险 | 对策 |
|------|------|
| NAT 穿透成功率低 | Relay 兜底，记录穿透统计 |
| 引导节点不稳定 | 支持多引导节点，可配置自建节点 |
| 防火墙阻止连接 | 使用常见端口，提供防火墙配置指南 |

---

## 阶段产出

- [ ] `network` 模块完整实现
- [ ] 局域网设备发现功能
- [ ] 跨网络连接能力
- [ ] 连接状态管理
- [ ] 基础 Tauri 命令接口

## 依赖的后续阶段

- Phase 2 将基于本阶段的连接能力实现配对
- Phase 3 将基于连接发送文件数据
