# Phase 2: 设备配对

> **范围**: 仅桌面端 (Windows / macOS / Linux)
>
> **设计文档**: [配对功能实现设计](../pre-refactor-design/pairing-implementation.md) — 架构、数据结构、流程、代码示例
>
> **总体规划**: [implementation-roadmap.md](implementation-roadmap.md)

## 目标

实现设备间的配对机制：配对码生成/解析、DHT 发布/查询、局域网直连配对。

本阶段仅针对桌面端之间的配对。移动端配对将在 Phase 4 使用二维码 + HTTP 方案实现。

## 前置条件

- [x] Phase 1 网络连接已完成
- [x] libp2p Swarm 正常运行（mDNS、DHT、Relay、DCUtR）
- [x] libs/core Kademlia 命令全套实现（provide、record、closest_peers）
- [x] libs/core Request-Response 实现（send_request、send_response）
- [x] 设备身份管理（Ed25519 密钥对生成、持久化、Stronghold 加密存储）

## 任务清单

### Step 1：消息类型与基础设施

| 状态 | 任务 | 说明 |
|------|------|------|
| ⬜ | 定义 `AppRequest` / `AppResponse` | `src-tauri/src/protocol.rs`，可扩展的请求/响应枚举 |
| ⬜ | 替换 `NetClient<(), ()>` | 改为 `NetClient<AppRequest, AppResponse>` |
| ⬜ | 扩展前端 `NodeEvent` 类型 | 处理 `inboundRequest` 事件 |
| ⬜ | 验证编译和基础功能 | mDNS 发现、连接不受影响 |

### Step 2：配对码生成

| 状态 | 任务 | 说明 |
|------|------|------|
| ⬜ | 配对码生成 | 6 位纯数字码，`pairing/code.rs` |
| ⬜ | 配对码验证 | 格式校验、过期判断 |
| ⬜ | DHT Key 命名空间 | `pairing/dht_key.rs`，`SHA256(namespace + id)` 防冲突 |
| ⬜ | 单元测试 | 生成格式、字符集、过期、Key 一致性 |

### Step 3：配对管理器

| 状态 | 任务 | 说明 |
|------|------|------|
| ⬜ | `generate_code` | 生成码 + `get_addrs` + `putRecord`（含可达地址） |
| ⬜ | `get_device_info` | `getRecord` + 解析 `ShareCodeRecord` |
| ⬜ | `request_pairing` | `add_peer_addrs` + `dial` + `send_request`（含可选 addrs） |
| ⬜ | `on_inbound_pairing` | 暂存请求，通知前端弹窗 |
| ⬜ | `handle_pairing_request` | 通过 `send_response(pending_id)` 回复 |
| ⬜ | `announce_online` | 节点启动时 `putRecord(online_key, OnlineRecord)` |
| ⬜ | `announce_offline` | 节点关闭时 `removeRecord(online_key)` |
| ⬜ | 过期清理 | 定时器清理过期配对码和入站请求 |

### Step 4：Tauri 命令注册

| 状态 | 任务 | 说明 |
|------|------|------|
| ⬜ | `commands/pairing.rs` | 封装 6 个 Tauri 命令 |
| ⬜ | 注册到 `invoke_handler` | `lib.rs` 中注册 |
| ⬜ | 初始化 PairingManager | 在 `start` 命令中创建并存入 Tauri State |

### Step 5：前端集成

| 状态 | 任务 | 说明 |
|------|------|------|
| ⬜ | `commands/pairing.ts` | invoke 封装 |
| ⬜ | `stores/pairing-store.ts` | 配对流程状态管理 |
| ⬜ | 修改 `network-store.ts` | 路由 `inboundRequest` 到 pairing-store |
| ⬜ | 配对码显示组件 | 大字体 + 倒计时 |
| ⬜ | 配对码输入组件 | 自动大写 + 格式化 |
| ⬜ | 配对请求弹窗 | 显示设备名，接受/拒绝 |

## 验收标准

- [ ] 能生成 6 位配对码并显示倒计时
- [ ] 另一台设备输入配对码能成功配对
- [ ] 配对码过期后提示"已过期"
- [ ] 局域网设备能通过附近列表直接配对
- [ ] 配对需要双方确认
- [ ] 已配对设备信息持久化到 Stronghold

## 依赖关系

- 依赖 Phase 1 的网络连接能力
- 为 Phase 3 文件传输提供已配对的连接
