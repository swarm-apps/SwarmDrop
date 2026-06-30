## Why

当前 SwarmDrop 依赖内置公网 bootstrap/relay 节点和用户手动填写 Multiaddr 来获得跨网络发现与中继能力。这个模型可以工作，但对用户不够自然：局域网内已经运行的桌面端无法自动成为协助节点，其他设备也无法自动把它纳入 bootstrap/relay 候选，只能继续依赖公网节点或手填地址。

本变更让桌面端可以选择性提供“局域网协助节点”能力，并让其他设备自动发现、识别、使用这些候选节点，从而把“设置中继节点地址”从默认路径降级为高级手动兜底。

## What Changes

- 新增“局域网协助节点”模式：用户可在桌面端手动开启，使本设备在局域网内提供 Kad Server 和 Relay Server 能力。
- 新增自动候选发现：系统从内置公网节点、用户自定义节点、mDNS 发现到的局域网协助节点等来源合并 bootstrap/relay 候选。
- `swarm-p2p-core` 支持运行时注册 infrastructure peer，而不只在启动时消费 `NodeConfig.bootstrap_peers`。
- mDNS + Identify 识别到局域网协助节点后，客户端自动将其加入 Kad 路由表、尝试 bootstrap，并在需要时申请 relay reservation。
- 设置页将“引导节点地址”调整为高级配置，并新增自动发现/局域网协助相关开关和状态展示。
- 网络状态暴露当前候选来源、已连接 bootstrap、relay reservation、局域网协助节点数量等信息。
- 保留用户手动添加自定义引导节点能力，作为高级兜底；不移除内置公网 bootstrap。

## Capabilities

### New Capabilities

- `lan-helper-node`: 桌面端作为局域网协助节点提供受限 Kad Server 与 Relay Server 能力，并向同网段设备声明该能力。
- `bootstrap-candidate-discovery`: 自动收集、合并、筛选和运行时注册 bootstrap/relay 候选节点，使普通用户无需手动填写中继节点地址。

### Modified Capabilities

（无现有主 specs 需要修改）

## Impact

- **swarm-p2p-core**: `NodeConfig`、`CoreBehaviour`、`EventLoop`、`NetClient` 命令、`NodeEvent`；新增 relay server Toggle、运行时 infrastructure peer 注册、helper 能力识别所需事件信息。
- **crates/core**: 网络配置与运行时启动参数、候选节点管理、网络状态投影、设备 Identify 分类。
- **桌面前端**: 设置页网络区域、preferences store、network store、网络状态展示。
- **测试**: core 三节点集成测试、relay server 限额测试、mDNS/Identify 自动发现测试、设置持久化与状态展示测试。
- **安全/资源**: 普通桌面端默认不开放 relay server；开启后限制为局域网优先、严格配额、可见状态，并提示流量和电量消耗。
