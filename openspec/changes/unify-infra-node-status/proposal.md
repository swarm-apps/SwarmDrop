## Why

三端（桌面 / 移动 / Web）对「引导节点连上了没有」这件事给出三种不同答案，且只有 Web 端能说清「哪一条连不上、为什么」。表面是 UI 不一致，根因是**建模对象选错**：「引导节点」被当成实体，而它实际是**本机与某远端之间的一段基础设施关系**，由两个正交能力（DHT 路由种子 / circuit 中继）构成。证据是同一概念在**五个写入点用了四种角色组合**（`runtime.rs:139` / `config.rs:85-92` / `manager.rs:186-189` / `event_loop.rs:145-155` / `supervisor.rs:138-158`）——每个作者都在替一个不存在的默认值做决定。

后果是一条完整的信息损失链：内核有逐条三态与失败原因（`RelayState{Connecting|Active|Failed{last_error}}`），但 `run_event_loop` 至今没订阅 `watch_relays`（`event_loop.rs:241-266` 只订了 addrs 与 nat），所以 `Connecting` 与 `Failed{last_error}` **从未进过原生端的推送循环**；即便进了，`build_network_status` 的 `active_relay_peers()`（`manager.rs:295-303`）也会 `filter(Active)` 把它们丢掉。用户看到的是绿色「在线 · 可接收」，而实际一条引导节点都没连上。

同时暴露三个已存在的数据面缺陷：`CandidateHealth` 在四条路径上不回写，导致**本机在公共 DHT 上发布失效的 relay hint**（`presence/supervisor.rs:452-471`）；`public_reachability=false` 时启动仍以 `relay:true` 注册公网 bootstrap，**绕过了闸门**（`runtime.rs:139`）；候选 `scope` 被三个调用方用三种拼法覆写，使收敛环时进时出（`candidates.rs:151-154`）。

完整调研与架构决策（含被否决的 16 个方案）见 [`dev-notes/research/2026-08-tri-platform-network-status.md`](../../../dev-notes/research/2026-08-tri-platform-network-status.md)。

## What Changes

**领域模型**
- 新增读模型 `InfraLink`（`crates/core/src/infra/link.rs`）：一段基础设施关系，零存储，按需 join 候选表（意图）+ `watch_conns`（连接事实）+ `watch_relays`（reservation 事实）。角色用 `Option<RelayLinkState>` 表达「是否承担」，`InfraExclusion` 显式区分「被设置拦下」与「故障」。
- **BREAKING（core 内部）**：删除 `CandidateHealth` 与 `mark_connected` / `mark_relay_ready` / `mark_failed`；`presence` 的 `relay_hints()` 改读 `watch_relays()` 的 `Active`。
- `BootstrapCandidate` 新增 `first_seen`；`scope` 不再由调用方传，改由 `upsert` 内部按合并后的全部地址 `CandidateScope::infer` 单点推断。
- `InfraSupervisor.links` 新增 `ever_active` 单调位（宽限期的唯一开关，非轮数）。

**配置模型**
- `ensure_relay_intent` → `ensure_infra_intent(NodeAddr, CandidateRoles)`；`remove_relay_intent` → `remove_infra_intent`。桌面/移动补上对应 IPC——「加引导节点需重启」不是内核限制，是 IPC 缺口。
- `runtime.rs:139` 的启动注册从 `InfraRoles::bootstrap()` 降为 `{ kad_server: true, relay: false }`，relay 角色交给 supervisor 按 `public_reachability` 闸门收敛。**保留** `NetworkRuntimeConfig.bootstrap_nodes`（降为 seed 语义）——删掉它会使 `public_reachability=false` 时 kad 路由表拿不到任何公网种子。
- Web 新增 relay 清单持久化（localStorage，存 custom + removed 两个集合而非 merged 快照）。

**推送链路**
- `run_event_loop` 订阅 `watch_relays`（三行），并收敛 `PingSuccess` 触发的全量 publish（`event_loop.rs:66-72` 的既有 TODO 升级为必做前置）。
- `NetworkStatus` **只增不删**：新增 `infra_links: Vec<InfraLink>`；既有 7 个派生标量中的 5 个内部改为从它派生，线上契约不动（其中 `relay_ready` 是 MCP agent 面 schema、`lan_helper_count` / `candidate_sources` 是 e2e 断言载体）。

**呈现（三端统一的是信息模型与状态语义，不是像素）**
- 节点生命周期轴与网络健康度轴拆开：常驻位显示 `status !== running ? 生命周期文案 : 健康度后果句`。
- 两层披露：结论层（状态点+词 · 一句后果句 · 已配对 N·在线 M · 至多一个 CTA）+ 诊断层（引导节点逐条 + 本机真值）。
- 桌面删除 `showExtra = windowHeight >= 700` 的 7 个门控点（`stop-node-sheet.tsx:195`），`StartNodeSheet` / `StopNodeSheet` 合并为随状态切换动作的单面。
- 移动 `NetworkHint` 重写（gate 运行态，修「节点没起来」被归因成「公网引导未连接」）；删除本端独有的「网络状况 良好/受限」合成。
- Web 删除「测试连通性」按钮 UI 入口（保留 `WebNode.connect` 导出）；修 pill 在全部 relay failed 时仍显绿。
- **新产品规则**：只有 `HostConfigured` 来源的 link 提供「移除」入口。自动来源（mDNS / Learned）不给——`remove_infrastructure_peer` 会断开全部连接，在「既是已配对设备又是 LAN Helper」的重叠节点上会掐断在途传输，且下次 identify 会原样复活。
- 契约：`DESIGN.md` 新增 `### Node Status Contract (cross-platform)`，与实现同 PR 落地。

**顺带清掉的相邻债**
- 移动 NAT 恒显示「未知」（`network.rs:161` 的 `format!("{nat_status:?}")` 产出 `"Public"`，三处 UI 判 `=== "public"`）。
- MCP `get_network_status` 的 `status` 硬编码 `"running"`（`mcp/tools.rs:224`）与 `nat_status` 的 Debug 格式，并加 `infraLinks`。
- 移动 `candidateSourceKey` 把 `Learned` 与 `HostConfigured` 折成同值造成的 React key 碰撞（`network-discovery.ts:45-54`）。
- 托盘状态只反映「`start()` 返回过 Ok」（`lifecycle.rs:126/145`），永不反映降级。
- `useNodeRestart.restart()` 无在途传输防护，会静默掐断传输。

**明确不做**
- 不删 `DiscoveryMode`（它确实零行为效果，但有整条 spec SHALL + 两端开关 + 两条 e2e，单独立 change）。**本轮禁止基于它写任何新逻辑。**
- 不把 `public_reachability` / `auto_discover_lan_helpers` 改成运行时可写（重启横幅从页级降为行级即可）。
- 不下发 `attempts` / `next_attempt_at`（`infra-peer-lifecycle` 与 `web-connection-control` 双重 SHALL NOT）。
- 不改 `connected_peers` 口径（LAN Helper 是 SwarmDrop agent，重叠是正确的；改口径会让用户的另一台电脑因为帮了忙而从设备页消失）。

## Capabilities

### New Capabilities
- `infra-link-status`: 基础设施链路的 per-link 读模型（`InfraLink`）、单链路与整体健康度两张状态机、宽限期判据、三条 IPC 投影与平台退化规则。

### Modified Capabilities
- `network-status`: 新增 `infra_links`；既有标量改为内部派生但保留；MCP 投影纳入同一事实源。该 spec 已严重过时（仍描述 `src-tauri/src/commands/mod.rs` 与三变体 `NatStatus`），一并重写。
- `network-status-display`: 生命周期轴与健康度轴拆开；两层披露；「不得因布局紧张丢弃信息位」；桌面 `NetworkStatusBar` 的既有 SHALL 与现行 `AppTopBar` 形态对齐。
- `bootstrap-node-settings`: 「修改引导节点后需重启节点」→「经 intent 即时生效」；`start` 的自定义清单降为 seed 语义；新增提交前同步校验（含 transport 匹配本端能力）。
- `bootstrap-candidate-discovery`: 删除「bootstrap 失败 → 标记候选健康状态」（改由 `watch_relays` 表达）；`scope` 由合并后地址单点推断；网络状态暴露 `infra_links`。
- `infra-peer-lifecycle`: 收敛按角色分档（kad 无条件、relay 受 `public_reachability` 闸门）；`ever_active` 的定义与清除条件。
- `web-connection-control`: `relays_*` → `infra_*` 更名；快照形状改为 `InfraLink`；补「`connect` 不用于 relay 可达性判定」；新增 relay 清单持久化。
- `node-control-sheets`: 桌面启停合并为单面；删除视口高度门控；入口描述从「侧边栏底部」修正为 `AppTopBar`（桌面端已无侧边栏）。

## Impact

**Rust**：`crates/net`（新增 `Endpoint::supported_transports()`；`NatStatus` 加 `specta::Type`）、`crates/core`（`infra/link.rs` 新增，`network/candidates.rs` / `network/manager.rs` / `network/config.rs` / `network/event_loop.rs` / `infra/supervisor.rs` / `presence/supervisor.rs` / `runtime.rs` 改）、`crates/web`（`relays_*` → `infra_*`）、`src-tauri`（新增两条 IPC 命令、MCP 投影、托盘状态源）、`mobile-core`（新增 `MobileInfraLink` / `MobileRelayLinkState` / `MobileInfraExclusion` 手写镜像 + chrono→i64 转换——该目录零 chrono，这是三端里最大的一块工作量）。

**前端**：桌面 `src/components/network/` 与 `src/routes/_app/settings/`、移动 `mobile/src/app/settings/` 与 `mobile/src/components/`、Web `docs/app/app/_components/` 与 `_lib/`；`packages/shared-view` 新增 `deriveInfraLinkState` / `summarizeNodeHealth`（只返回 msgId 不返回文案）。

**codegen 连锁**：specta 自动重生成（消费方只有 `stop-node-sheet.tsx` 的 9 个字段引用）；uniffi 的穷尽解构 drift guard（`network.rs:127-150`）会编译期强制处理每个变更；wasm 需 `pnpm build:wasm` 并提交 `packages/swarmdrop-web/` 产物。

**测试**：`crates/core/tests/infra_reconcile.rs` 三条 `relay_ready` 断言应原样通过（保留字段的价值）；`config.rs:110-125` 的 scope 断言需跟改；`src/routes/_app/settings/-network-settings-section.test.tsx:74` 的「重启提示」断言需跟改。

**文档**：`DESIGN.md` 新增契约 + 「网络概念 → 三端统一中文串」表；`CLAUDE.md:390` 的「三端信息分层一致」断言需更正（已证伪）；删除 `docs/app/app/_components/connection-panel.tsx:391-394` 的错误注释（「桌面 bootstrap 只在启动时用一次」——`InfraRoles::bootstrap()` 含 `relay: true`）。

**风险**：刀 4（`ensure_infra_intent` 给 Web 补上 `kad_server`）改变 Web 的 kad 查询路径，需真机验证 presence 宣告不退化。
