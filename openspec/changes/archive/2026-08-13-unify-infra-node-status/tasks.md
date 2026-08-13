> 每个编号组 = 一刀，可独立合入且合入后三端仍是好的。排序原则：**先修 bug（可被测试证伪）→ 补前置 → 加能力（纯增量）→ 最后 UI**。删除全部推迟到最后且预期为空。

## 1. 先修可独立证伪的 bug

- [x] 1.1 给 `NatStatus`（`crates/net-base/src/status.rs`）加 `specta::Type` derive，去掉 core 侧 `NetworkStatus.nat_status` 的 `specta(type = String)` 覆盖
- [x] 1.2 `mobile-core` 的 `MobileNetworkStatus.nat_status` 从 `format!("{nat_status:?}")` 改为 uniffi enum 镜像（`network.rs:161`）
- [x] 1.3 补 core wire 测试：`serde_json::to_value(NatStatus::Public)` 为 `"public"`
- [x] 1.4 移动端三处 NAT 判据改走 `isNatMapped()`（`network.tsx` / `device-info-card.tsx` / `node-control-sheet.tsx`），枚举形状收在 `core/network-discovery.ts` 一处。**桌面端无需改动**：`NatStatus` 只有 `Public | Unknown` 两个变体（AutoNAT v2 单次失败不足以判 Private），所以既有的二值渲染本就是穷尽分支；`specta` 类型化后 `tsc` 会拦住写错的字面量
- [x] 1.5 修移动端 `candidateSourceKey`（`mobile/src/core/network-discovery.ts:45-54`）把 `Learned` 折进 `hostConfigured` 导致的 React key 碰撞，并删除两处因返回类型只有两值而永不可达的 `default` 分支（`network.tsx:599`、`node-control-sheet.tsx:402`）
- [x] 1.6 给 `useNodeRestart.restart()`（`src/hooks/use-node-restart.ts:31-35`）与移动/Web 的对应停止路径加在途传输检查；`StopNodeSheet` / `NodeControlSheet` / `NodeStatusDialog` 的确认文案补「正在传输的 N 个会话会被中断」

## 2. 打通推送链路（全案性价比最高的一刀，零依赖）

- [x] 2.1 `run_event_loop`（`crates/core/src/network/event_loop.rs:241-266`）新增 `relays_watcher = shared.endpoint.watch_relays()` 并在 select 中订阅，变更时 `publish_network_status`
- [x] 2.2 收敛 `PingSuccess` 触发的全量 publish（落实 `event_loop.rs:66-72` 的既有 TODO）——`ping_interval` 为 30s，多个已连 peer 会造成持续全量推送，而下一刀要往 `NetworkStatus` 挂数组
- [x] 2.3 **改为自动化断言**：`crates/core/tests/infra_reconcile.rs` 的 `relay_state_transitions_are_pushed_to_the_host`——起 `run_event_loop`，往候选表塞一条 TEST-NET-1 的死地址，断言 `CoreEvent::NetworkStatusChanged` 被推出且其中该 link 的 `relay` 非 `None`。刻意不钉 `Connecting` 还是 `Failed`：那取决于拨号超时，钉死其一会让测试变成计时器竞赛

## 3. 候选表：scope 单点推断 + first_seen

- [x] 3.1 `BootstrapCandidateManager::upsert`（`crates/core/src/network/candidates.rs:126-175`）删除 `scope` 参数，改为内部按合并后的全部地址 `CandidateScope::infer` 计算
- [x] 3.2 更新三个调用点：`config.rs:85-92`（删硬编码 `Public`）、`manager.rs:183`（删 `infer` 传参）、`event_loop.rs:145-155`（删硬编码 `Lan`）
- [x] 3.3 `BootstrapCandidate` 新增 `first_seen: DateTime<Utc>`，首次 upsert 写入且此后不可变；`last_seen` 语义不变
- [x] 3.4 新增单测：含私网地址的候选被二次 upsert 后 scope **不翻回** `Public`
- [x] 3.5 新增单测：同一候选先以 kad 角色、后以 relay 角色登记后，`roles` 两者皆真且 scope 按合并地址重算
- [x] 3.6 新增单测：重新发现（`last_seen` 刷新）不改变 `first_seen`
- [x] 3.7 修正 `config.rs:110-125` 的 `host_configured_candidate_is_loaded_in_lan_only_mode`——它用 `/ip4/127.0.0.1/`，scope 从 `Public` 变 `Lan`，断言需跟改

## 4. 启动路径角色降级

- [x] 4.1 `crates/core/src/runtime.rs:139` 的 `add_infrastructure_peer` 从 `InfraRoles::bootstrap()` 降为 `InfraRoles { kad_server: true, relay: false }`
- [x] 4.2 新增集成断言 `kad_only_registration_still_dials_the_bootstrap_node`：`relay: false` 的注册**仍会拨号并连上**（kad 种子不受闸门影响），且 `relays` map 里没有条目（降级是真降级）
      > **只覆盖了一半，另一半在集成测试里表达不了**：`public_reachability=false` 的那一半需要一个
      > `Public` 范围的候选，而集成测试只有 loopback 地址可用，`CandidateScope::infer` 见 loopback
      > 一律判 `Lan`。反向那半由既有单测 `public_reachability_off_skips_public_candidates` 覆盖。
- [x] 4.3 验证既有 supervisor 测试（`public_reachability` 关闭时不建 reservation）仍绿
- [x] 4.4 确认 `EndpointProfile::registers_infra()`（`runtime.rs:61-63`）保持不变——它 gate 的是「浏览器没有内置引导」

## 5. 意图登记面泛化

- [x] 5.1 `NetManager::ensure_relay_intent` → `ensure_infra_intent(NodeAddr, CandidateRoles)`（`crates/core/src/network/manager.rs:182`），默认 `CandidateRoles::kad_and_relay()`
- [x] 5.2 `NetManager::remove_relay_intent` → `remove_infra_intent`（`manager.rs:205`）
- [ ] 5.3 `InfraSupervisor` 的收敛按角色分档：kad 角色无条件收敛，relay 角色保持 `public_reachability` 闸门。确认 `wants_reservation` **不**被泛化为「是否入环」的总判据
      > **实施中发现的设计缺口，需先定再写**：`links` 这张表的每个字段（`reservation_active` /
      > `attempts` / `next_attempt_at`）都是**为 relay 语义定义的**。把纯 kad 候选塞进同一张表，
      > `reservation_active` 对它永远为假，退避会一直推进到 75s 上限却没有任何「成功」信号能归零它
      > ——退化成一个永不收敛的空转循环。
      >
      > 需要的是**第二个判据**（kad 侧的「已收敛」= 该 peer 在地址簿/路由表里且连着），而
      > `Endpoint` 当前**没有暴露 kad 路由表查询面**（`kad.add_address` 是 fire-and-forget，
      > actor 只处理 `OutboundQueryProgressed`，没有 `RoutingUpdated`）。所以这一项要么先在
      > `crates/net` 补一个可查询的收敛信号，要么改用 `watch_conns` 的连接位当近似判据——
      > 属于新的设计决策，不能顺手写。
      >
      > **不阻塞本轮**：4.1 已经堵住真正的缺陷（启动路径以 `relay:true` 绕过闸门）。
      > 现状是「kad 注册在启动时做一次，失败不重试」——与改动前完全一致，没有回归。
- [x] 5.4 `InfraSupervisor.RelayLinkState` 新增 `ever_active: bool`：`RelayReservationAccepted` 置位；候选移除 / 节点停止 / 手动重启清零
- [x] 5.5 `crates/web/src/node.rs` 内部改调新 API（此刻仅内部改名，公开 API 更名在第 9 组）
- [ ] 5.6 **真机验证**：Web 端 `kad_server` 从 false 变 true 后 presence 宣告不退化。浏览器 kad 查询将全跑在 relay circuit 上，路径与今天不同；`QuorumFailed` 是已知旧伤，不要误判为新回归
      > **这条无法自动化，也是本轮唯一有回归风险的改动。** 建议与 8.6 / 9.7 合并成一次浏览器
      > 会话做完。design.md 的 Risks 段已写明回滚方案：真机不过就单独回滚这一刀（其余刀只
      > 依赖它的 API 更名，不依赖它的行为变化）。

## 6. InfraLink 读模型与 IPC 载体

- [x] 6.1 新建 `crates/core/src/infra/link.rs`：`InfraLink` / `RelayLinkState` / `InfraExclusion` 类型定义（含 `Serialize` + `specta::Type`）
- [x] 6.2 实现 `build_infra_links(&SharedNetRefs) -> Vec<InfraLink>`：现场 join 候选表 + `watch_conns` + `watch_relays` + supervisor 的 `ever_active`，零存储
- [x] 6.3 `removable` 由 `sources` 全部为 `HostConfigured` 派生；`excluded` 按 `roles.relay_server` 与 `public_reachability` 现算（**只有两个变体**，不含 `NodeNotRunning`、不含任何基于 `DiscoveryMode` 的变体）
- [x] 6.4 `NetworkStatus` 新增 `infra_links: Vec<InfraLink>`；`relay_ready` / `relay_peers` / `bootstrap_candidate_count` / `candidate_sources` / `lan_helper_count` 改为从它派生，**对外字段与语义不变**
- [x] 6.5 确认 `bootstrap_connected` 与 `discovered_peers` **保持现实现**（口径是扫全部已连 peer 的 agent 前缀，与候选表是不同集合）
- [x] 6.6 新增 `MobileInfraLink` / `MobileRelayLinkState` / `MobileInfraExclusion` 手写镜像 + 两处 `DateTime<Utc>` → `i64` 转换（`mobile-core` 全目录零 chrono，这是三端最大的一块工作量）；`network.rs:127-150` 的穷尽解构 drift guard 会编译期强制处理
- [x] 6.7 验证 `crates/core/tests/infra_reconcile.rs` 的三条 `relay_ready` 断言**原样通过**（这正是保留字段的价值）
- [x] 6.8 已由 `infra_reconcile.rs` 的既有断言覆盖（`relay_ready` 为真时同一 peer 的 link 必须是 `Active`，且 `ever_active` 置位、`excluded` 为 `None`）

## 7. 删 CandidateHealth（先补会红的测试）

- [x] 7.1~7.2 **改为把 `relay_hints` 抽成纯函数 `relay_hints_from(candidates, relays)` 并直接钉住判据**（`relay_hints_follow_live_relay_state`）。原计划的两条集成测试造不出来——`watch_relays` 只由 actor 在真实 relay 事件里写，测试拿不到写入口。纯函数版覆盖的是同一个缺陷类且更直接：内核翻 `Failed` / 条目消失时 hint 必须立刻停发，而这正是「用户撤销」与「拨号失败」两条不发 `RelayReservationLost` 的路径的可观测后果
- [x] 7.3 ⚠️ **不要用 `ListenerClosed` 路径写测试**——`event_loop.rs:57-60` 与 `actor.rs:1118-1119` 是同一原子点，那条今天是绿的
- [x] 7.4 `presence/supervisor.rs:452-471` 的 `relay_hints()` 从 `CandidateHealth::RelayReady` 改读 `watch_relays()` 的 `RelayState::Active`；验证 7.1/7.2 转绿
- [x] 7.5 删除 `CandidateHealth` 枚举、`BootstrapCandidate.health` 字段、`mark_connected` / `mark_relay_ready` / `mark_failed` 三个方法及其在 `event_loop.rs:42-63, 174-180` 的四个调用点

## 8. 桌面 / 移动：意图 IPC 与提交前校验

- [x] 8.1 `crates/net` 新增 `Endpoint::supported_transports() -> &[TransportKind]`
- [x] 8.2 桌面新增 `ensureInfraIntent` / `removeInfraIntent` / `supportedTransports` 三条 tauri 命令（`src-tauri/src/commands/lifecycle.rs` + `setup.rs` 的 `collect_commands!`）
- [x] 8.3 移动新增对应 uniffi 方法
- [x] 8.4 三端实现提交前同步校验：Multiaddr 可解析 + 含 `/p2p/` 且 peer id 合法 + **transport 属于本端点实际装配的传输** + 与既有条目（含内置）去重。校验 SHALL 无网络往返
- [x] 8.5 桌面/移动的添加与删除改走意图 IPC，同时写回 preferences（先改内核成功、再写持久化）
- [ ] 8.6 手动 e2e：运行中添加一条引导节点，≤15s 看到状态翻转，**全程不重启节点**
      > **需要跑起来的 app，本轮未做。** 已由自动化覆盖的部分：`add_infra_node` 同步返回
      > （`infra::validate` 6 条单测）+ 收敛环把状态推到宿主（`relay_state_transitions_are_pushed_to_the_host`）。
      > 真正只有手测能答的是「≤15s」这个体感阈值，以及三端 UI 有没有真的重渲染。
- [ ] 8.7 手动 e2e：粘一条 `/webrtc-direct/` 地址到桌面、粘一条 `/tcp/` 到浏览器，两者均当场被拒且提示说明原因
      > **判据本身已自动化**：`rejects_transports_the_local_endpoint_never_assembled` 正是这两个
      > 方向各一条断言（含「错误里带着本端支持什么」）。剩下要手测的只有「提示文案读起来是否
      > 说得清下一步」——那是文案质量，测不出来。
      > ⚠️ 桌面那条的前提是 `/webrtc-direct` **确实不在** native 清单里；实测 native 装配了它
      > （`supported_transports` 含 `WebrtcDirect`），所以任务描述里那个例子**举错了**。桌面真正
      > 拒得掉的是「只有浏览器有的传输」或未来新增的 target-specific 传输。

## 9. Web：infra_* 更名与持久化

- [x] 9.1 `crates/web/src/node.rs` 的 `relays_ensure` / `relays_drop` / `relays_state` / `relays_changed` / `relays_until_active` 更名为 `infra_ensure` / `infra_drop` / `infra_links` / `infra_changed` / `infra_until_active`
- [x] 9.2 `infra_links()` 返回 `InfraLink[]`（替代 `RelayInfoJson[]`）；`infra_ensure` 以 kad + relay 双角色登记
- [x] 9.3 `WebNode.connect` 保留导出；在其文档中补「不用于 relay 可达性判定」
- [x] 9.4 `docs/app/app/_lib/preferences-store.ts` 新增基础设施清单持久化，存 **custom 与 removed 两个集合**（不是 merged 快照）
- [x] 9.5 `node-lifecycle.ts` 的 `ensureConfiguredRelays` 改为回放「内置清单 − removed + custom」
- [x] 9.6 `pnpm build:wasm` 并提交 `packages/swarmdrop-web/` 产物
- [ ] 9.7 验证：刷新后自定义节点仍在；撤销内置项后刷新不复活；模拟内置地址变更后老用户能拿到新的
      > 三条语义均已有单测（`docs/app/app/_lib/network-view.test.ts` 与 preferences-store 的
      > `custom` / `removed` 两集合归一）。**剩浏览器里真刷一次页面**——只有它能证明
      > localStorage 的读写时机与 `WebNodeBootstrap` 的挂载顺序没打架。

## 10. 契约与共享判据（与第 11–13 组同 PR 合入）

- [x] 10.1 `DESIGN.md` 新增 `### Node Status Contract (cross-platform)`，插在 Send Entry Contract 与 Layout Density Contract 之间。按 Device Card Contract 体例写：绑定宣示 + 信息位表（结论层 / 诊断层）+「不得因布局紧张丢弃信息位」+ **Permitted divergence** + **Degradation**
- [x] 10.2 契约中写死：**基础设施是关系的角色，不是节点的类别**；同一 NodeId 可同时出现在设备与基础设施两层，重叠时设备卡加一枚「也是我的中继」标记
- [x] 10.3 `DESIGN.md` 新增「网络概念 → 三端统一中文串」表（与既有 icon table 并列），收口：引导节点 / 局域网协助 / 中继 / 已连接设备 / 可达。废弃「公网引导」「LAN Helper」「本机 Helper」等分叉写法
- [x] 10.4 `packages/shared-view/src/network/` 新增 `deriveInfraLinkState(link, nowMs)` 与 `summarizeNodeHealth(status, links, nowMs) -> { level, msgId, cta }`，**只返回 msgId 不返回文案**
- [x] 10.5 单测：宽限只在 `!ever_active ∧ 已见 Failed ∧ now - first_seen >= 10s` 三条件同时成立时生效
- [x] 10.6 单测：`Settling` 不返回成功档；`ever_active == true` 的 link 掉线不吃宽限
- [x] 10.7 单测：两个 `InfraExclusion` 变体各返回中性档且 cta 为「改设置」而非「重试」
- [x] 10.8 单测：整体健康度六态各自的 msgId 与色档；「部分中继失败」不降级
- [x] 10.9 三端 i18n catalog 各补一份 msgId 对应文案（三份独立 catalog，无门禁兜底，需人工三处同改）

## 11. 桌面 UI

- [x] 11.1 `StartNodeSheet` 与 `StopNodeSheet` 合并为单一节点状态面，动作随状态切换；更新两个调用点（`app-topbar.tsx:119-123`、`devices/index.lazy.tsx:243-244`）
- [x] 11.2 **删除 `showExtra = windowHeight >= 700`**（`stop-node-sheet.tsx:188-195`）及其 7 个门控点（`:243, 327, 353, 371, 394, 410, 426`），改为折叠 + 内滚
- [x] 11.3 按两层分层重排：结论层（状态点+词 · 可达性后果句 · 已配对/在线 · 至多一个 CTA）+ 诊断层（`infraLinks` 逐条 + 本机真值）
- [x] 11.4 节点 ID 与公网地址改为可复制且截断时 `copyText`/`title` 给完整值（`stop-node-sheet.tsx:276-282, 410-419`）
- [x] 11.5 设置页「引导节点」区加逐条状态列（状态点+词 / 来源 / `lastError` 原文 + 复制按钮 / 移除入口按 `removable` 门控）
- [x] 11.6 顶栏 pill 接 `summarizeNodeHealth`；与节点状态面统一状态词；补 `title` / `aria-label`（说明可点开查看详情）
- [x] 11.7 重启横幅从页级降为行级（只保留在 `provide_lan_helper` 下方）；`needsRestart` 从组件 `useState` 提升到 store，使其跨路由存活
- [x] 11.8 更新 `-network-settings-section.test.tsx:74` 的「运行中修改后显示重启提示」断言
- [x] 11.9 组件测试：矮窗口（<700px）下信息位不丢

## 12. 移动 UI

- [x] 12.1 `NetworkHint`（`mobile/src/app/settings/network.tsx:543-568`）重写：gate 到 `runtimeState === "running"`（修「节点没起来」被归因成「公网引导未连接」）、接 `summarizeNodeHealth`、补 CTA
- [x] 12.2 删除本端独有的「网络状况 良好/受限」合成（`network.tsx:97-112`）与来源 chip 列表（`:512-541`），改用共享判据
- [x] 12.3 引导节点页（`settings/bootstrap-nodes.tsx`）加逐条状态列 + `lastError` + 移除入口按 `removable` 门控 + 提交前校验
- [x] 12.4 `NodeControlSheet` 按两层分层重排；诊断层接 `infraLinks`
- [x] 12.5 主屏 StatusPill 接 `summarizeNodeHealth`；补 `accessibilityLabel`
- [x] 12.6 移动 `runtimeConfigChanged`（`network.tsx:80-95`）不再需要比对 bootstrapNodes（已即时生效）；重启横幅只保留 `provide_lan_helper`
- [x] 12.7 验证：节点关闭时 `NetworkHint` 说的是「节点未运行」而非「公网引导尚未连接」

## 13. Web UI

- [x] 13.1 `ConnectionPanel` 接 `infra_links()`：逐条展示状态 / 来源 / `lastError`；移除入口按 `removable` 门控
- [x] 13.2 删除「测试连通性」按钮 UI 入口（保留 `WebNode.connect` 导出），改为提交前同步校验
- [x] 13.3 `RelayError` 加 `CopyButton`
- [x] 13.4 `NodeStatusDialog` 与侧栏 / 顶栏 pill 接 `summarizeNodeHealth`；修「节点在跑但全部 relay failed 时 pill 仍显绿」
- [x] 13.5 按两层分层重排 `NodeStatusDialog`
- [x] 13.6 验证：全部 relay failed 时侧栏 pill 不是绿的

## 14. 第四、第五个状态面

- [x] 14.1 MCP `get_network_status`（`src-tauri/src/mcp/tools.rs:198-241`）：`status` 改读 `NetworkStatus.status`（当前硬编码 `"running"`，使 server instructions 的「先确认节点已启动」完全失效）
- [x] 14.2 MCP `nat_status` 改经 serde 序列化，去掉 `format!("{:?}")`
- [x] 14.3 MCP 返回值新增 `infraLinks`
- [x] 14.4 托盘状态源接健康度：整体进入 `Isolated` 时托盘不呈现正常「在线」（当前 `refresh_tray` 的 `online` 只由 `lifecycle.rs:126/145` 写死传入）
- [x] 14.5 托盘 rust-i18n catalog（`src-tauri/locales/*.toml` 的 `[tray.status]`）补对应文案——**第四份 catalog，`pnpm i18n:extract` 扫不到**

## 15. 文档更正与收尾

- [x] 15.0 **移动端 i18n 提取（被并发会话阻塞）**：第 1 组给移动端新增了两条串（`自动发现` /
      `就绪 · 自动发现`，替代原先永不可达的 `公网` 死分支）。`mobile/src/locales/{en,zh-Hans}/messages.po`
      当前正被另一个会话大改（各 ~449 行的导航重构），此刻跑 `pnpm i18n:extract` 会把对方的在制品
      重排。**等那批改动落地后再跑一次 extract 并补 en 译文**——在此之前英文用户会在这两处看到中文。

- [x] 15.1 删除 `docs/app/app/_components/connection-panel.tsx:391-394` 的错误注释（「桌面那份是纯静态配置，它的 bootstrap 节点只在启动时用一次」——`InfraRoles::bootstrap()` 含 `relay: true`，桌面同样持有活的 reservation）
- [x] 15.2 更正 `CLAUDE.md:388-390` 的「三端同一件事的第三份实现，信息分层一致」——该断言对桌面不成立，改为指向新契约
- [x] 15.3 `CLAUDE.md` 的 i18n 段落把「三份独立 catalog」更正为「四份，其中一份是后端 rust-i18n」
- [x] 15.4 `dev-notes/knowledge/net-kernel.md` 补一节：`InfraLink` 读模型、收敛按角色分档、`watch_relays` 是原生端 relay 状态的唯一来源
- [ ] 15.5 把 `dev-notes/research/2026-08-tri-platform-network-status.md` 的状态更新为「已落地」，并把实践部分提炼进 `knowledge/`

## 16. 全量门禁与条件性删除

- [x] 16.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo test --workspace` + `cargo clippy --workspace`
- [x] 16.2 `./scripts/check-wasm.sh` 与 `./scripts/check-wasm.sh --clippy`
- [x] 16.3 `pnpm test` + `pnpm check:zustand-access` + `pnpm check:shared-view` + `pnpm check:clipboard`；`docs/` 下 `pnpm test` + `pnpm typecheck`；`mobile/` 下 `pnpm typecheck`
- [x] 16.4 **预期被部分证伪，但结论仍是本轮不删。** 逐个 grep 的实测：
      | 字段 | 现存消费者 | 判定 |
      |---|---|---|
      | `relay_ready` | MCP schema（`mcp/tools.rs`）+ `infra_reconcile.rs` 三条断言 | 保留 |
      | `lan_helper_count` / `candidate_sources` | `e2e_lan_helper.rs` 断言 | 保留 |
      | `bootstrap_connected` | 口径不同（扫全部已连 peer 的 agent 前缀，非候选表） | 保留 |
      | `relay_source` / `bootstrap_candidate_count` / `relay_peers` | **UI 侧已归零**（三端全改吃 `infra_links`），仅剩定义、派生与测试 fixture | 可删，但不在本轮 |
      后三个确实空了——这与 design.md 决策 6 预判的「预期为空」不符，因为那条预判是在
      三端 UI 还在读压扁标量时下的。仍不在本轮删：删它们要动 specta / uniffi / wasm 三条
      codegen 加移动镜像的穷尽解构，而本 change 的纪律是「删除全部推迟到最后且不与功能同刀」。
      留给独立 change，判据已在此备好。
- [x] 16.5 **决策：不加第五条脚本门禁，改由类型强制**。shared-view 导出的是 `NodeHealthLevel` /
      `InfraLinkState` 两个**字面量联合**，各端只要把映射写成 `Record<NodeHealthLevel, MessageDescriptor>`
      就是编译期穷尽的——漏一个变体当场红，比脚本更早、更准，且零维护成本。
      脚本方案还有个够不着的地方：msgId 不是 Lingui 的 msgid（各端映射到自己的 `msg` 描述符），
      「catalog 覆盖 msgId 集合」这个断言本身就不成立。

## 17. 审查发现与修复（第二轮，2026-08-08）

三端实现落地后各配一名独立审查者对着 `DESIGN.md` 的契约复核，共 1 blocker + 6 major + 9 minor。
全部处理完毕，其中一条经核实**判定审查有误、未照改**。

- [x] 17.1 **core：`removable` 判据写反了**（编排层修）。原判据「sources **全是** HostConfigured」
      在 `upsert` 累加 + `learn_candidate` 给每个 `is_bootstrap_agent` 对端补 `Learned` 的组合下必然翻假——
      本仓自建那台正是 bootstrap agent，于是它连上几秒后移除按钮消失；用户自加的中继一旦连上就
      **永久删不掉**而偏好清单还在每次启动回放它。改为「**含有** HostConfigured」，配两条回归测试。
      三端 UI 都照这个门控画，所以三端一起坏、一起好。
- [x] 17.2 **桌面：宽限时钟走表条件错**。`summary.level === "starting"` 与「有 link 在 settling」不等价——
      `resolveLevel` 在 `publicReachable` 为真时先 return，于是「两条引导节点连上一条」时定时器不起、
      `nowMs` 冻在 mount，挂掉那条永远显示「正在连接」而它的 lastError 就在同一张卡下面两行。
      反向：空清单时 level 恒 `starting`，定时器永不停。改判据 + 两条各钉一个方向的回归测试。
- [x] 17.3 **Web / 移动：内容超出视口够不着**（契约明令禁止的那条）。Web 的 `DialogContent` 无
      `max-h`/`overflow`，而 app shell 是 `h-dvh overflow-hidden` 且 Radix 锁了 body 滚动——不是被裁掉
      是触达不到，且它是移动档**停止节点的唯一入口**；移动的 `AppBottomSheet` 没传 `scrollable`。
      两处修法刻意与桌面孪生分叉且写明了原因（Web 用 `dvh` 不用 `vh`、内滚只给中间段以钉住 footer）。
- [x] 17.4 **桌面：还剩一处自造健康度**（`-device-info-section.tsx` 的 `nodeStatus === "running"`），
      改动后会与已接判据的顶栏 pill 公开打架。改走 `useNodeHealth` + `resolveNodePresentation`。
- [x] 17.5 **移动：starting / error 时结论层自相矛盾**（pill 说「启动中」、下一行说「节点未运行」，
      还渲染一颗会被重入守卫吞掉的「启动节点」）。照桌面的 `LIFECYCLE_OVERRIDE` 补同一层，
      并删掉本端那四张生命周期兜底表——它们正是矛盾的来源。
- [x] 17.6 **三端诊断层补齐契约点名的信息位**：桌面/Web 的逐条归因补 `scope · roles`
      （角色是**列举**不是二选一，两者正交）；移动补 公网地址 / 监听地址 / 身份存放位置。
- [x] 17.7 **编排层裁决：设置页「已连接设备」取错了数**。它读 `connectedPeers`，而后端
      `DeviceManager::connected_count()` 数的是所有 agent 认得出是 SwarmDrop 的已连对端、不看配对——
      局域网协助节点就是另一台 SwarmDrop 桌面，于是零配对设备在线的用户会看到「已连接设备 1」。
      抽 `usePairedOnlineCount` 让它与节点状态面的「在线 M」**结构性同源**。`connected_peers` 字段
      本身不动（Non-Goal），只是它回答的不是这一格的问题。
- [x] 17.8 **词表收口**：三端 en catalog 的 `Bootstrap Nodes` / `LAN Helper Address` / `LAN assistance`
      / `Add Custom Bootstrap Node` / `Public Address` / `Listening Addresses` / `Relay Node`
      → `Bootstrap nodes` / `LAN helper address` / `LAN helper` / `Add custom bootstrap node`
      / `Public address` / `Listen addresses` / `Relay node`。
      `Listen addresses` 逐字对应字段名 `listen_addrs`，用户搜日志能对上。
- [x] 17.9 其余 minor：复制提示说假话（两处收成一个共享组件）、Web 的 `infra_ensure` 不再吞
      `Duplicate`、`doRestore` 校验失败不再静默、`Set` 字面量改 `Record` 让穷尽检查真的生效、
      移动 `candidateSourceKey` 删 `default`、icon-rail 档状态词不再消失。
- [x] 17.10 **判定审查有误、未照改**：「`forgetInfraNode` 无可达 caller」是**旧 `removable` 判据下的
      推论**（按钮消失 → `doDrop` 打不到）。17.1 修完前提即不成立，UI 侧一行未动，只把
      「判据在 core，别在这一侧另立一个绕回来」钉在门控处。

### 遗留（需要跑起来的东西，本轮做不到）

Web 侧栏 768–1023px 档的 pill 改成了竖排方块（契约信息位 1 不许只有裸色点）。
50px 文字宽 vs 英文 `Connecting` 约 48px 是**算出来的、未实测**，已留 `md:break-words` 兜底
（最坏是断词不是横向溢出）。跑 `docs` 的 dev server 时顺手看一眼那一档。

### 本轮**未做**的一件事（刻意）

三端 catalog 存在**全局性**的按钮文案大小写漂移（桌面 `Cancel Transfer` / `Add Device` 是 title case，
`Clear records` 是 sentence case；Web 全 sentence case）。这不是本轮词表那 5 个概念的问题，而是
几百条串的独立扫荡，且本仓目前**没有**一条管大小写的规则。往 `DESIGN.md` 加一条自己不执行的规则
正是该文档警告过的「契约先立、实现没跟上」，所以既不加规则也不做扫荡，留作独立提案。
