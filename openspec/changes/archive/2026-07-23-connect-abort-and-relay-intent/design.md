# Design — connect-abort-and-relay-intent

## Context

现状（develop）：

- `Endpoint::connect`（`crates/net/src/endpoint.rs:141`）内建 `connect_timeout`（默认 30s），超时仅"弃等"——reply sender 留在 actor 的 `dials` 等待表，底层 dial 继续；无取消面。
- `WebNode::reserve`（`crates/web/src/node.rs:269`）= `ensure_relay_reservation`（发 `AddInfraPeer` 后**立即返回**的 fire-and-forget 登记）+ `watch_relays` 上的无限 loop 等 `Active`。helper 不可达 → 状态永停 `Connecting` → Promise 永不 settle。
- `infra_relay_peers`（`crates/net/src/actor.rs`）是只进不出的 `HashSet`，全仓无 remove 路径。
- `InfraSupervisor`（`crates/core/src/infra/supervisor.rs`）是标准 reconciler：期望状态（候选表）+ 1s tick + 退避收敛（2s→…→75s 封顶，永不放弃）——对内置 bootstrap 这是**正确行为**，问题只在意图不可撤销、失败不可观测。
- 前端（`feat/web-75-base` 分支）用 20s `Promise.race` 兜底（`with-timeout.ts`），是止血不是修复。

约束：Web 端无存量用户，**不需要兼容**；`RelayState` 的 native 消费方仅状态显示层。

## Goals / Non-Goals

**Goals:**

1. 任何 JS Promise 在有限时间内 settle（内核兜底，不依赖调用方传参）。
2. 每个"注册"有对称"注销"：`add_infrastructure_peer` ↔ `remove_infrastructure_peer`。
3. 状态机无隐藏状态：失败、退避、重试次数经 `watch_relays` 可观测。
4. JS API 只用平台原语（`AbortSignal`、事件），零自造概念（不发明 `timeoutMs`）。
5. 机制与策略分离："重试多久放弃"由调用方决定，内核只提供诚实状态。

**Non-Goals:**

- 不改 `InfraSupervisor` 的收敛策略（永不放弃对内置 bootstrap 是特性）。
- 不做 `feat/web-75-base` 分支的前端改造（该分支合并后跟进）。
- 不追求"abort 撤回在途 dial"——libp2p 无公开 API 时接受"失败后不再有重建意图"的等价语义。
- 不引入 IndexedDB 持久化等无关 Web 负债。

## Decisions

### D1：reserve 拆成命令与查询（CQS），而非加超时参数

`reserve()` 的本质是"注册常驻可达意图"（无终点的收敛过程），包装成一次性 RPC 是语义谎言——加超时只是让谎言 20s 后被戳穿，且 reservation 建立后掉线前端依然失明。拆开：

- **命令**（同步、幂等）：`relays_ensure(addr)` 登记意图；`relays_drop(id)` 撤销意图。
- **查询**：`relays_state()` 快照 + `relay-changed` 事件订阅。
- **糖**：`relays_until_active(id, signal?)` 覆盖"等首次 Active"的九成场景，`Failed` 时提前 reject 而非干等。

意图生命周期（ensure/drop）与单次等待的耐心（signal）从此解耦。

**备选：句柄式**（`reserve()` 返回带 `close()` 的 `Reservation` 对象）。否决理由：① JS 无可靠 RAII，忘 close 的句柄 = 泄漏的常驻意图；② reservation 生命周期是 app 级（Zustand store）而非调用点级——#77 的邀请在离开页面后仍需可达。声明式集合幂等、无悬空，与 store 心智同构。

### D2：取消用标准 AbortSignal，不自造 timeoutMs

`AbortSignal.timeout(ms)` / `AbortSignal.any([...])` 是平台已有词汇；API 语言好的标志是不重新发明宿主原语。wasm 侧经 `web_sys::AbortSignal` 监听 abort 事件转 future select。同时保留内核 `connect_timeout` 兜底（Browser profile 下调至 15s），保证 Goal 1 不依赖调用方自觉。

### D3：RelayState 三态化，circuit 地址随 Active 下发

```rust
pub enum RelayState {
    Connecting { attempt: u32 },
    Active { circuit_addr: Addr },
    Failed { attempts: u32, last_error: String, next_retry_at: Instant },
}
```

- 拨号失败（`OutgoingConnectionError`）与 reservation 被拒（`ListenerClosed`）必须翻转到 `Failed`；`Identify`/重试翻回 `Connecting`。
- circuit 地址由内核作为 `Active` 属性给出，删除 `WebNode::reserve` 里手拼 `{helper}/p2p-circuit/p2p/{self}` 的逻辑（单一事实源）。
- **Failed 粒度**：dial 失败是 per-address，RelayState 是 per-peer——取"该 peer 本轮全部候选地址耗尽"为 Failed 判据，`last_error` 记末次错误。避免多地址部分失败的状态抖动。

**备选：保持二态 + 另设错误事件轨道**。否决：状态与事件分家后，快照消费方（`relays_state()`）拿不到失败事实，watch 语义残缺。

### D4：remove_infrastructure_peer 是唯一取消原语

actor 新增 `RemoveInfraPeer` 消息：摘 `infra_relay_peers`、清地址簿、`kad.remove_peer`、`ListenOpts` 对应 circuit listener 关闭、`watch_relays` 删条目；core 侧同步清 `InfraSupervisor::links` 与候选表（学习型候选）。撤销后失败不再有任何重建路径——即使 libp2p 无 dial abort，效果等价于取消。

**已验证（pin 93c5059 源码实读）**：`Swarm::close_connection(ConnectionId)` 仅对 established 生效（`pool.get_established`），pending 返回 false；但 **`Swarm::disconnect_peer_id(PeerId)` → `Pool::disconnect` 会对该 peer 的 pending 连接调用 `connection.abort()`**（pool.rs 文档明示 "whether pending or established are closed asap"）。故 remove 用 `disconnect_peer_id` 可做到"立刻断"，含中止在途拨号。

### D5：connect 的 abort 语义诚实文档化

abort 后 swarm 在途拨号继续到自然失败（无常驻意图残留，地址簿记录无害），Promise 立即以 `AbortError` 语义 reject，actor 侧 waiter 随 reply sender drop 自然失效。文档明确"abort ≠ 撤回拨号"。

### D6：分层落点

| 层 | 改动 | 不动 |
|---|---|---|
| `crates/net` | remove 消息、RelayState 三态、Failed 翻转、Browser 默认超时 | dial/收敛核心路径 |
| `crates/core` | 注销清理（links/候选） | InfraSupervisor 收敛策略 |
| `crates/web` | connect+signal、relays 命名空间、删 reserve、client.js 镜像 | transfer/pairing 路径 |

## Risks / Trade-offs

- [`RelayState` 破坏性变更波及 native 状态显示] → 消费方仅 `network-status` 镜像层，随 change 一并适配；`cargo check --workspace` 覆盖 mobile-core。
- [Failed 判据（全地址耗尽）依赖 libp2p 错误事件的到达顺序] → 以 `OutgoingConnectionError`（peer 级，含全部地址结果）为判定点，不逐地址累计，规避顺序问题。
- [`relays_drop` 与 supervisor tick 竞态：drop 后 tick 又把候选拉回] → remove 语义定义为"撤销意图"，须同步清候选表条目；学习型候选（identify 自动纳管）可能再学回来——接受，因为"helper 真实可达且宣告自己"时重新纳管是正确行为。
- [client.js 手工镜像漏方法（历史踩过）] → tasks 中列为独立核对项；根治（Proxy 动态转发）留给 React UI 工程，不混入本 change。
- [wasm 侧 AbortSignal 监听的回调泄漏] → 用一次性监听 + future drop 时显式 remove_event_listener 封装成小工具函数，单点治理。

## Migration Plan

无存量兼容负担。合并顺序：本 change（crates 侧）→ `feat/web-75-base` 合并后删 `with-timeout.ts` 并切新 API（后续小 PR）。回滚 = revert 单 PR。

## Open Questions

- ~~`Swarm::close_connection` 对 pending dial 的行为~~ 已验证（见 D4）：`disconnect_peer_id` 可中止 pending 拨号，remove 采用"立刻断"语义。
- ~~`relays_until_active` 遇 `Failed` 的行为~~ 已定案：立即 reject 携带失败原因，意图保留、是否 drop 由调用方决定（把"要不要再等"还给调用方），已写入 `web-connection-control` spec。
