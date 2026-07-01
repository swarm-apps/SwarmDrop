## Context

当前引导节点地址硬编码在 `src-tauri/src/network/config.rs` 的 `BOOTSTRAP_NODES` 常量中。`start` 命令调用 `create_node_config(agent_version)` 时不接受外部参数。前端 `NetworkStatus` 已有 `relayReady`、`natStatus` 字段但桌面端未展示。移动端有 `NetworkStatusBar` 但只显示连接数。

## Goals / Non-Goals

**Goals:**
- 用户可在设置页查看默认引导节点、添加/删除自定义引导节点
- 自定义节点列表持久化到 `preferences-store`
- 后端 `start` 命令接受自定义引导节点参数，与默认节点合并
- 修改引导节点列表后提供「重启节点」操作（前端 stopNetwork + startNetwork）
- `NetworkStatus` 新增引导节点连接状态
- 桌面端设备页顶部新增 `NetworkStatusBar`
- `NetworkStatusBar` 展示引导节点连接、中继就绪、NAT 穿透三项状态

**Non-Goals:**
- 不允许删除/修改默认引导节点（只能添加额外的）
- 不做引导节点健康检查或连通性测试
- 不修改 `swarm-p2p-core` 库

## Decisions

### 1. 自定义引导节点的存储位置 → `preferences-store`

引导节点列表属于用户偏好设置，不含敏感信息，存储在 `preferences-store`（tauri-plugin-store）中。

字段: `customBootstrapNodes: string[]`（Multiaddr 字符串数组）

**备选方案:** 存在 secret-store（Stronghold），但引导节点非敏感数据，不需要加密保护。

### 2. 传参方式 → `start` 命令新增可选参数

`start(pairedDevices, customBootstrapNodes?)` — 前端将 `preferences-store` 中的列表传入，后端在 `create_node_config` 中与默认节点合并。

**备选方案:** 后端从文件读取，但违反了「前端驱动」的架构风格。

### 3. 引导节点连接状态 → `NetworkStatus` 新增 `bootstrapConnected` 布尔字段

事件循环中通过 `ConnectionEstablished` 事件检查 peer_id 是否为引导节点来更新此状态。简单且够用。

**备选方案:** 每个引导节点单独追踪（Map<PeerId, bool>），但过度复杂，当前只有一个引导节点。

### 4. 重启节点 → 前端 `stopNetwork` + `startNetwork` 顺序调用

不在后端实现 restart 命令。前端在设置页修改引导节点列表后显示「需要重启节点」提示，用户点击后依次调用 stop/start。

### 5. 桌面端状态栏 → 复用 `NetworkStatusBar` 组件

在 `DesktopDevicesView` header 区域下方插入 `NetworkStatusBar`。增强该组件使其同时展示：引导节点连接状态、中继就绪状态、NAT 穿透状态。移动端和桌面端共用同一组件。

## Risks / Trade-offs

- **[引导节点地址格式验证]** → 前端添加时做基础 Multiaddr 格式校验（包含 `/p2p/` 且可解析），拒绝无效地址
- **[重启节点丢失连接]** → 重启会断开所有连接，但这是用户主动操作，可接受。提示文案说明影响
- **[bootstrapConnected 粒度粗]** → 只有一个布尔值表示"至少一个引导节点已连接"，未来多引导节点场景可能不够。但当前足够，后续可扩展
