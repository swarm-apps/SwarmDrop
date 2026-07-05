## 1. 桌面仓 libs/core（机制层）

- [x] 1.1 reservation 状态跟踪：event loop 登记 `listener_id → relay_peer` 映射（request_relay_reservations 处），派生"活跃 reservation 集"
- [x] 1.2 新增 `NodeEvent::RelayReservationLost { relay_peer_id }`：ListenerClosed/ListenerError 命中映射时上抛（替代仅 warn）；确认与 RelayReservationAccepted 成对
- [x] 1.3 新增幂等命令 `EnsureRelayReservationCommand { peer_id, addrs }` + client API：已有活跃 reservation 则 no-op，否则 listen_on circuit
- [x] 1.4 `RelayLimits` 默认调整：`max_circuit_bytes=u64::MAX`、`max_circuit_duration=12h`；`bootstrap_peers` 首次 remove 语义降级为记账（重建统一走幂等原语）
- [x] 1.5 libs 单测：reservation 丢失事件上抛、ensure 幂等（重复调用无副作用）、限额生效

## 2. 桌面仓 crates/core：infra::Supervisor（收敛层）

- [x] 2.1 新建 `crates/core/src/infra/` 模块：InfraSupervisor 骨架（事件折叠 + 1s tick + 退避，同 presence 模式），随 run_event_loop 拉起、挂 CancellationToken
- [x] 2.2 候选表升级为期望状态源：`BootstrapCandidateManager` 增加健康/退避记账查询；`maybe_register_lan_helper` 删 changed 门禁（upsert 无条件，维持动作交 Supervisor）
- [x] 2.3 收敛逻辑：对每个候选维持「连接存活（退避 dial）+ kad 接线 + reservation 存活（relay 角色且 public_reachability 允许时调 ensure_relay_reservation）」；消费 RelayReservationLost/PeerDisconnected 事件
- [x] 2.4 学习型候选：IdentifyReceived + `is_bootstrap_agent` → upsert（source=Learned，scope=Public，roles=kad_and_relay），受 public_reachability 约束
- [x] 2.5 `NetworkRuntimeConfig` 加 `public_reachability: bool`（默认 true）并贯穿 create_node_config/候选装配
- [x] 2.6 infra 单测：reservation 丢失→重建、候选不可达退避、Learned 候选纳管、public_reachability=false 时不做公网 reservation

## 3. 桌面仓 crates/core：presence 记录与探测升级

- [x] 3.1 `OnlineRecord` 结构化：direct_addrs（过滤 loopback/unspecified/多跳 circuit）+ relay_addrs（合法一跳 circuit）+ relays hint（≤3，含 relay PeerId+可达地址）；构建逻辑从裸 get_addrs 迁出
- [x] 3.2 announce 事件驱动：NewListenAddr/ExternalAddrConfirmed/ListenerClosed/ReservationAccepted/Lost → 立即重发（去抖 ~2s）；150s 周期降为兜底；首发失败 10s 退避重试一次
- [x] 3.3 `spawn_probe` 多步编排：get_record → 并发拨 direct+relay 地址 → 全败且有 hint → 逐个 hint 先 dial(relay) 再拨 circuit；失败原因分级 tracing（无记录/地址不可拨/relay 不可达）
- [x] 3.4 presence 单测更新：记录过滤规则、事件驱动补发、多步编排（hint 路径）

## 4. 桌面 host + UI

- [x] 4.1 `NetworkStatus` 加 `public_reachable: bool`（活跃公网 reservation 或已确认公网直达地址）；build_network_status 接线
- [x] 4.2 桌面设置：新增"公网可达性"开关；LanOnly 文案改写真实语义（"不主动连接公网引导；仍可经局域网协作节点被跨网访问，除非关闭公网可达性"）
- [x] 4.3 桌面网络状态区展示"公网可达/仅局域网可达"
- [x] 4.4 `cargo test` + clippy 全绿；桌面侧集成验证：模拟 reservation 断线（断 relay 连接）→ 自动重建 → announce 即时刷新
- [x] 4.5 桌面仓提交推 develop，记录 rev

## 5. 移动仓跟进

- [x] 5.1 bump swarmdrop-core rev；`MobileNetworkRuntimeConfig`/`MobileNetworkStatus` 镜像加字段（穷尽解构 drift guard 驱动）
- [x] 5.2 重生成 bindings：`build:android` + `build:ios` + `prepare` 三连（rust-bridge 知识库路径）；`pnpm typecheck` 全绿
- [x] 5.3 移动设置页加"公网可达性"开关 + 网络状态区"公网可达/仅局域网可达"展示 + LanOnly 文案同步；`pnpm lint`/i18n extract
- [x] 5.4 移动仓提交

## 6. 拓扑验证（用户环境）

- [ ] 6.1 目标态验证：A(LAN Helper 手机)+B(LanOnly 电脑,默认可达)+C(跨网)——B 自动获得公网中继 reservation，C 一跳直拨 B，presence 稳定
- [ ] 6.2 断线重建验证：A 挂起/杀掉再恢复 → B 的 reservation 与跨网可达自动恢复（无需重启节点）；大于 64MiB 文件走中继传输不被掐断
- [ ] 6.3 观察项：严格隔离模式（public_reachability=off）下 RelayHint+打洞路径的实际成功率，决定是否需要 follow-up
