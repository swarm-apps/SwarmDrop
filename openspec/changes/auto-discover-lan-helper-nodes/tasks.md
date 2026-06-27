## 1. swarm-p2p-core 基础设施能力

- [ ] 1.1 在 `libs/core` 新增 `RelayLimits` / `InfrastructureMode` / `LanHelperConfig` 配置类型，并为 `NodeConfig` 增加 builder 方法和默认值测试
- [ ] 1.2 在 `CoreBehaviour` 中新增 `relay_server: Toggle<relay::Behaviour>`，保持现有 `relay_client` 语义不变
- [ ] 1.3 在节点启动流程中根据 `InfrastructureMode::LanHelper` 启用 Kad Server 和 Relay Server
- [ ] 1.4 在 event loop 中过滤并公告可用私有 LAN 地址，避免公告 `0.0.0.0`、`::`、loopback 和不可路由地址
- [ ] 1.5 新增 relay server 事件处理和必要的 `NodeEvent` 状态事件，用于上层展示协助节点运行状态
- [ ] 1.6 为 LAN Helper 默认关闭、开启后启用 relay server、relay limits 生效、无可用 LAN 地址时降级等行为补单元/集成测试

## 2. 运行时 infrastructure peer 注册

- [ ] 2.1 扩展 `NodeEvent::IdentifyReceived`，暴露对端 listen addrs / protocols 或等价的 Identify 元信息
- [ ] 2.2 新增 `AddInfrastructurePeerCommand` 和 `NetClient::add_infrastructure_peer(...)`
- [ ] 2.3 在 command 中完成 Swarm 地址表注册、Kad 地址注册、dial、pending relay reservation 记录
- [ ] 2.4 复用连接建立后申请 `p2p-circuit` reservation 的时序，支持运行时发现的 relay server
- [ ] 2.5 为动态注册 KadServer/RelayServer 候选、重复注册去重、未启用 relay client 时不申请 reservation 补测试

## 3. crates/core 自动候选管理

- [ ] 3.1 扩展 `OsInfo` agent_version 编解码，支持 `caps=lan-helper` 并保持旧 agent_version 兼容
- [ ] 3.2 新增 `BootstrapCandidate`、`BootstrapCandidateSource`、`CandidateRoles`、`CandidateHealth` 等候选模型
- [ ] 3.3 实现 `BootstrapCandidateManager`，合并内置公网节点、用户自定义节点和 mDNS LAN Helper 候选
- [ ] 3.4 在 core 事件循环中根据 `PeersDiscovered` + `IdentifyReceived` 识别 LAN Helper，并调用 `add_infrastructure_peer`
- [ ] 3.5 在发现新的可用候选后触发 `client.bootstrap()` 或等价路由刷新，并处理失败候选的健康状态
- [ ] 3.6 扩展 `NetworkStatus`，暴露发现模式、候选来源、LAN Helper 数量、bootstrap/relay 当前来源等字段
- [ ] 3.7 为 OsInfo capability 解析、候选池去重/优先级、LAN Only 模式、候选失败回退补测试

## 4. 桌面端设置与状态展示

- [ ] 4.1 扩展 `preferences-store`，新增发现模式、自动发现局域网协助节点、本设备提供局域网协助能力等设置并持久化
- [ ] 4.2 调整网络启动参数，把发现设置和局域网协助开关传入 Rust core
- [ ] 4.3 将设置页“引导节点”调整为网络发现设置，默认展示自动发现/局域网协助开关，自定义 Multiaddr 移入高级区域
- [ ] 4.4 在网络状态 UI 展示公网引导状态、局域网协助节点数量、中继预约状态和当前候选来源
- [ ] 4.5 当修改需要重启网络节点的设置时，显示明确提示并复用 stop/start 重启流程
- [ ] 4.6 补充前端 store 和设置页交互测试；如涉及 Lingui 文案，运行 i18n extract

## 5. 验证与回归

- [ ] 5.1 增加三节点集成测试：A/B 普通节点，C 开启 LAN Helper，A/B 自动发现并通过 C 完成 Kad bootstrap
- [ ] 5.2 增加 relay reservation 集成测试：普通节点运行时发现 LAN Helper 后可申请 reservation
- [ ] 5.3 增加 LAN Only 模式测试：不连接内置公网 bootstrap，但仍可使用 mDNS 发现的 LAN Helper
- [ ] 5.4 运行 `cargo fmt --manifest-path libs/core/Cargo.toml`
- [ ] 5.5 运行 `cargo test --manifest-path libs/core/Cargo.toml`
- [ ] 5.6 运行 `cargo clippy --manifest-path libs/core/Cargo.toml --all-targets -- -D warnings`
- [ ] 5.7 运行 `cargo check --manifest-path src-tauri/Cargo.toml`
- [ ] 5.8 按最终改动范围运行前端类型检查和必要的 UI/组件测试
