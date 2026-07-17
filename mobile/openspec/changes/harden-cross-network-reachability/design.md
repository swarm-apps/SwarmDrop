## Context

跨网关拓扑调查（wf_bd61fd08，6 agent+逐环核实）确认的现状：

- **reservation 一次性**：`candidates.upsert` 同 peer+同地址返回 changed=false 跳过重注册（`crates/core/src/network/event_loop.rs:180` 门禁 + `candidates.rs:110-141`）；`bootstrap_peers` 首次 identify 后 remove（`libs/core/src/runtime/event_loop.rs:399`）；`ListenerClosed` 仅 warn（`libs event_loop.rs:460`）。libp2p 侧 reservation 严格绑定单条连接、断即 listener 永久关闭（`libp2p-relay-0.21.1/src/priv_client/transport.rs:366`），库层不重建。A 挂起恢复（地址不变）后 B 跨网永久不可达，直到进程重启（tcp/0 端口变化才意外触发 changed=true）。
- **B 的记录地址全废**：`get_addrs()`=listeners+external（`get_listen_addrs.rs:28`），LanOnly 下 = 私网监听 + 锚定 A 私网 IP 的一跳 circuit + 非法二跳 circuit（A 的 external 含其经 BOOT 的 circuit 地址，拼出多跳被 libp2p 硬拒 `transport.rs:276`）。C 无一可拨；C 拨一跳 circuit 需先与 A 直连，而 C→A 只能经 BOOT relayed + DCUtR 打洞——可靠性天花板=打洞成功率。
- **已核实的反直觉**：reservation 存续期间 B↔A 连接由 relay 协议双侧自动保活（client `keep_alive=reservation.is_some()`，server 侧有 reservation 即保活）且 30min 自动续约——「空闲回收拖垮 reservation」不成立，真实因果是「连接断→reservation 死→无人重建」。
- **限额**：`RelayLimits` 默认 `max_circuit_duration=30min`、`max_circuit_bytes=64MiB`（`libs/core/src/config.rs:24-28`），走中继的大文件传输必被掐。
- **LanOnly 语义极窄**：仅不加载内置公网引导（`crates/core/src/network/config.rs:67-70`），mdns/relay_client/dcutr/autonat 全开，不阻止经 A 学到公网节点后外拨（B 的 put_record 本就落到 BOOT）。
- 既有正确模式：`presence::Supervisor`（0.7.6）= 声明式收敛（期望状态 vs 实际状态 + tick + 退避），挂 `NetManager` 的 spawn+CancellationToken。

四项决策已与用户拍板：LanOnly 语义做成设置项（默认允许公网 reservation）；限额 bytes 无限+duration 12h；OnlineRecord 结构化+过滤；范围含 P2 探测编排与 P5 UI 提示。

## Goals / Non-Goals

**Goals:**
- 基础设施链路（连接/kad/reservation）获得与已配对 peer 同级的收敛保障：断线自动重建，A 挂起恢复后 B 的跨网可达性自动恢复。
- C→B 方向获得可靠直拨路径：默认配置下 B 在学到的公网中继上直接 reservation，C 一跳拨通、零打洞依赖；A 从必经枢纽退化为发现网关+兜底中继（爆炸半径缩小）。
- 在线记录成为「可用的可达性声明」而非 listeners 快照；announce 对世界变化即时响应。
- 走中继的传输不再被 64MiB/30min 限额掐断。
- 可达状态对用户可见（Trusted Doorstep：控制通过可见性）。

**Non-Goals:**
- 不做 kad 记录持久化（A 重启丢记录，≤150s 由 announce 补回，可接受；留 follow-up）。
- 不做 BOOT 的 identify→kad 接线（P6；直连 reservation 后「记录只落 A」场景重要性大降）。
- 不改设备列表 presence 的二元展示（0.7.6 决策维持）；「不可直达」提示只在本机网络状态区。
- 不做嵌套/多跳中继（libp2p 不支持，也不该支持）。
- 严格隔离模式（public_reachability=off）下的打洞失败兜底不追求 100%——该模式的可靠性天花板如实呈现在文案中。

## Decisions

### D1: 收敛模式推广 —— infra::Supervisor 作为家族第二员，候选表是唯一期望状态源

新建 `crates/core/src/infra/`：`InfraSupervisor` 与 `presence::Supervisor` 同构（事件折叠 + 1s tick + 退避），独立 tokio 任务，同挂 `NetManager` 的 CancellationToken，随 `run_event_loop` 拉起。期望状态定义：对候选表（`BootstrapCandidateManager`）中每个候选，维持「连接存活 + kad 已接线 +（relay 角色且 public_reachability 允许时）reservation 存活」。

- 候选表升级为期望状态唯一 source of truth：`upsert` 的 changed 返回值只用于「地址更新时刷新 kad 地址簿」，不再门禁任何维持动作；`mark_failed/mark_connected` 记账供退避决策。
- **删除三层一次性门禁**：`maybe_register_lan_helper` 的 changed 门禁（`crates event_loop.rs:180`）改为无条件 upsert+交给 Supervisor 收敛；`bootstrap_peers.remove`（libs `event_loop.rs:399`）语义改为「已触发过首次 reservation 请求」的记账而非唯一通道（重建走幂等原语）；`ListenerClosed` 上抛事件（D2）。
- 弃选「并入 presence::Supervisor」：关注对象（配对 peer vs 基础设施）与期望状态语义不同，合并会让两个状态机互相渗透；共享的只是模式不是代码。
- 弃选「libs 层自动重建 reservation」：机制/策略分层——libs 不知道"该不该重建"（候选可能已被用户移除），只提供幂等原语与事件。

### D2: libs/core 机制 —— RelayReservationLost 事件 + 幂等 reservation 原语

- event loop 维护 `listener_id → relay_peer` 映射（`request_relay_reservations` 时登记）；`ListenerClosed/ListenerError` 命中映射时上抛 `NodeEvent::RelayReservationLost { relay_peer_id }`（与既有 `RelayReservationAccepted` 成对）。
- 新命令 `EnsureRelayReservationCommand { peer_id, addrs }`：若该 peer 已有活跃 reservation（accepted 且 listener 未关）→ no-op 返回；否则执行 listen_on(addr/p2p-circuit)。活跃状态由上述映射派生，不新增状态源。
- 弃选「业务层记 reservation 状态」：listener 生命周期只有 libs 看得见，状态放 libs、事件通知业务是唯一不撕裂的分法。

### D3: 设置语义拆分 —— discovery 与 reachability 解耦

`NetworkRuntimeConfig` 新增 `public_reachability: bool`（默认 true）。语义矩阵：

| discovery_mode | public_reachability | 行为 |
|---|---|---|
| Auto | true（默认） | 现状 + 学到的中继也纳管 |
| LanOnly | true | 不连内置引导；经 LAN Helper 学到的公网中继照常连接并 reservation（用户拓扑的目标态） |
| LanOnly | false | 严格局域网：不对公网中继发起连接/reservation，跨网可达仅剩 A 转发+打洞（文案如实告知） |
| Auto | false | 连引导做 DHT 但不做公网 reservation（可被发现、不可被中继直达） |

- InfraSupervisor 据此决定 reservation 期望：`false` 时对「非 LAN scope」候选跳过 relay 角色维持。
- 弃选三态枚举替换 discovery_mode：两个正交维度硬压一个枚举会产生 4 缺 1 的表达力问题，且 discovery_mode 已有前端语义。

### D4: 学习型候选 —— 公网中继经 identify 自动纳管

IdentifyReceived 且 `OsInfo::is_bootstrap_agent(agent_version)` → 以 `BootstrapCandidateSource::Learned`（新枚举值）upsert 候选（roles=kad_and_relay，scope=Public），地址取 identify 的公网可拨地址。B 在 LanOnly 下经 A 的 kad/identify 认识 BOOT 的那一刻即进入收敛清单 → 连接 + reservation 自动建立。受 D3 开关约束。

### D5: OnlineRecord 结构化（BREAKING，无兼容负担）

```rust
OnlineRecord {
    os_info,
    direct_addrs: Vec<Multiaddr>,   // listeners+external，剔除 loopback/unspecified/多跳 circuit；私网地址保留（跨子网 LAN 重探可用，跨网拨快速失败无害）
    relay_addrs: Vec<Multiaddr>,    // 合法一跳 circuit 地址
    relays: Vec<RelayHint { peer_id, addrs }>,  // 每个活跃 reservation 的 relay 及其可达地址，C 先修 relay 直连再拨 circuit 的输入
    timestamp,
}
```
- relays hint 上限 3 个（防 record 膨胀）；relay 的"可达地址"取该候选在本机候选表/地址簿中的公网优先地址。
- announce 触发改为「地址集或 reservation 集变化 → 立即重发」+150s 周期兜底；首发失败带 10s 退避重试一次（治 LanOnly 启动 150s 空窗）。

### D6: 探测编排的两个时间尺度

- **Probing（15s 宽限）不变**：只做地址簿直拨——宽限期语义是"短抖动无感"，不塞多步动作。
- **Unreachable 低频重探升级**：get_record → 并发拨 direct+relay_addrs → 全失败且 relays hint 非空 → 逐个 hint：dial(relay) 确保直连（触发 DCUtR 自升级）→ 再拨目标 circuit 地址。失败原因分级 tracing（无记录/地址不可拨/relay 不可达），替代 `let _` 静默。
- 弃选「宽限期内做多步修链」：会撑大宽限期语义，且链路级故障本来就该走慢循环。

### D7: 限额与 UI

- `RelayLimits` 默认：`max_circuit_bytes = u64::MAX`、`max_circuit_duration = 12h`（防僵尸电路永久占位；被掐后 Probing 15s 宽限内重拨 circuit 无感重建）、reservation/circuits 数量维持现状。
- `NetworkStatus` 加 `public_reachable: bool`（= 存在活跃的公网 scope reservation 或已确认公网直达地址）；桌面网络状态卡与移动端状态区展示"公网可达/仅局域网可达"；LanOnly 设置文案改写（"不主动连接公网引导节点；仍可经局域网协作节点被跨网访问，除非关闭公网可达性"）。
- 移动端镜像：`MobileNetworkStatus`/`MobileNetworkRuntimeConfig` 加字段——穷尽解构 drift guard 编译报错逼同步；**本次需重生成 bindings**（`build:android`/`build:ios`/`prepare` 三连，见 rust-bridge 知识库）。

## Risks / Trade-offs

- [严格隔离模式可靠性天花板仍是打洞成功率] → 如实呈现：该模式文案说明"跨网访问可能不可用"；默认模式已绕开此路径。
- [reservation 重建风暴（候选不可达时反复 listen_on）] → InfraSupervisor 用与 presence 同款退避（2/5/10s→75s 低频），mark_failed 记账；listen_on 失败无副作用。
- [Learned 候选可能学到陌生 bootstrap agent] → 仅信任 `is_bootstrap_agent` + 公网地址可拨者；候选数量上限沿用现状；用户自定义引导仍最高优先。
- [OnlineRecord 结构变更使新旧版本互不识别] → 无存量兼容负担（用户偏好：不留兼容层）；两端同轮发版。
- [A 的 kad MemoryStore 单点未解] → 接受 ≤150s 记录空窗（announce 周期兜底）；持久化留 follow-up。
- [归档顺序约束] → `fix-presence-idle-offline` 必须先归档，否则本 change 的 paired-device-presence MODIFIED delta 无主体可改。
- [移动端 bindings 重生成链路长] → 走知识库既有轻量路径；FFI 面变化点集中在两个镜像 struct。

## Migration Plan

1. 桌面仓 libs/core：事件+原语+限额 → 单测。
2. 桌面仓 crates/core：infra::Supervisor + 门禁删除 + record 结构化 + announce 事件驱动 + probe 编排 + 设置字段 → 单测/集成测试（reservation 断线重建）。
3. src-tauri + 桌面前端：设置项、状态展示、文案。
4. 桌面全绿后推 develop 取 rev。
5. 移动仓：bump + 镜像字段 + bindings 重生成 + 设置/状态 UI。
6. 真机拓扑验证（用户环境：A 手机 LAN Helper + B LanOnly 电脑 + C 跨网设备）。
7. 回滚：单 change 原子提交链，revert 回 0.7.6 行为。

## Open Questions

- InfraSupervisor 对 Learned 候选的退避上限与淘汰策略（连续失败多久降级/移除）——实现期按 presence 手感定，spec 只约束"持续收敛"。
- RelayHint 中 relay 可达地址的选取排序（公网 confirmed > 用户自定义 > 私网）——实现期定。
