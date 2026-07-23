# Design — deepen-relay-reconciliation

## Context

前序 change（`2026-07-23-connect-abort-and-relay-intent`，已归档）落地了 relay 意图的声明式 API 与注销面，simplify 审查（4 agent 并行）确认了三条架构级裂缝并跳过。本 change 是对它们的推敲与收束——**两条真改，一条否决**。

现状裂缝的代码坐标：

- 轮数双主人：`crates/net/src/actor.rs` 的 `infra_relay_peers: HashMap<PeerId, u32>`（ensure_relay 递增、Accepted 归零、identify 路径 `.unwrap_or(1).max(1)` 凑数）vs `crates/core/src/infra/supervisor.rs` 的 `RelayLinkState.attempts`（驱动退避、Accepted/候选刷新归零）。
- 注销命令式三连：`NetManager::remove_relay_intent`（删候选 + `infra.remove` + `remove_infrastructure_peer`），tick spawn 内候选表 re-check（注释自认"收窄到微秒级"而非闭合）。
- 失败边沿采样：`until_active` 在 watch 上等 `Failed`，浏览器实测第 3 轮才观察到（wasm 单线程调度 + last-value-wins 覆盖）。

关键前置验证（本次 change 起草时完成）：

1. 候选表**只有显式 remove**（`remove_relay_intent` 一处生产调用），无任何自动过期/清出路径——"候选消失"当且仅当用户显式撤销。
2. 所有生产路径的 relay 登记都有候选表对应条目：内置/自定义 bootstrap 经 `create_candidate_manager` upsert（与 `bootstrap_node_addrs` 同一套 DiscoveryMode 过滤，LanOnly 两边同时跳过）、mDNS LAN helper 经 event_loop upsert、web 手动意图经 `ensure_relay_intent`。**不存在"endpoint 有 relay 登记而候选表无条目"的合法状态**。

## Goals / Non-Goals

**Goals:**

1. 机制层只报告可自证的事实——重试轮数（语义由上层退避策略定义）撤出 `RelayState`，supervisor 成为轮数唯一主人。net 侧为**净删**。
2. 收敛环补全反向规则：候选表无 → 内核状态收敛到无。注销竞态由环闭合（终态一致保证），不再靠 re-check 收窄窗口。
3. 净复杂度下降：删除的机制（actor 计数、re-check、`.max(1)` 补丁）多于新增的（一条差集规则）。

**Non-Goals:**

- 不新增失败边沿事件轨（见 D3 否决记录）。
- 不处理"候选在但 wants_reservation 降级"（`public_reachability` 关闭后清理既有 public reservation）——反向环只对"候选完全消失"触发，降级场景维持现状，记为已知负债。
- 不改 `remove_infrastructure_peer` 的 net 层语义（全拆：意图+地址簿+kad+listener）。

## Decisions

### D1：轮数撤出 `RelayState`，而非 `AddInfraPeer` 携带 attempt

```rust
pub enum RelayState {
    Connecting,                       // 无字段
    Active { circuit_addr: Addr },
    Failed { last_error: String },
}
```

曾考虑的备选：`AddInfraPeer { attempt }` 由策略盖章、机制回显。否决：把策略参数塞进机制 API，且 `ensure_relay_reservation` 糖 / event_loop LAN helper 注册 / runtime bootstrap 注册都得编造默认值——又是两套语义。撤出后 actor 的 `infra_relay_peers` 回归 `HashSet`，递增/归零/凑数逻辑全删，identify 路径与 LAN helper 注册的轮数漂移**连根消失**（没有轮数就没有漂移）。

**web 连带**：`RelayInfoJson` 删 `attempts` 字段。轮数对 UI 无决策价值（用户关心"连不上 + 为什么"），supervisor 内账经 tracing 可诊断。

### D2：反向收敛用「watch_relays vs 候选表」差集，而非 links 记账

tick 增加规则（伪码）：

```
desired  = 候选表快照中的 peer 集合
observed = endpoint.watch_relays() 中的 peer 集合（with() 零拷贝读 key）
for peer in observed - desired:
    spawn remove_infrastructure_peer(peer)   // 幂等，条目消失前每轮重发
```

选 watch（内核实际状态镜像）做 observed 的原因：**自愈性**。在途 `AddInfraPeer` 即使在注销后复活内核登记，watch 必然重现条目，下一轮 tick 差集必然发现并清理——终态一致由环保证，与事件到达顺序无关。备选"links 记账 + remove 确认后删账"否决：links 删除即失忆，复活场景无人发现，还得引入 epoch/确认协议——比差集贵得多。

连带简化：

- tick spawn 内的候选表 re-check **整段删除**（它防的竞态现在由环闭合）。
- `remove_relay_intent` 保留一次直接 `remove_infrastructure_peer` 调用作为**低延迟快路径**（用户 drop 后立即生效，不等 1s tick）；环是兜底，二者幂等叠加，注释写明分工。
- 判据安全性依赖前置验证 1/2（候选无自动清出、登记必有候选）——写入 spec scenario 锁定。

### D3：否决失败边沿事件轨（`RelayAttemptFailed`）

逐一排查潜在消费者后否决：supervisor 的退避是定时驱动（tick），失败即时记账不改变收敛行为；桌面 UI 的失败原因已在 `RelayState::Failed` 快照中；web `until_active` 走"采样 + 30s 契约兜底"，错过 Failed 窗口的实害仅是极端情况下错误文案从具体原因降级为"超时"（且下一轮 Failed 大概率被采到，实测 ~10s）。为此铺一条 net→core→web 的新数据通道属过度设计。**接受的取舍**：`until_active` 的错误信息质量不做保证，30s cap 为契约级兜底（前序 change 已文档化）。

### D4：BREAKING 的处置

`RelayState` 形状第二次变更（上一 change 刚三态化）。可接受：native 消费方仍只有 `active_relay_peers` 的 `matches!`（不受影响）与测试断言；web 无存量用户；两次变更间隔一天、未发版。**这也是现在做的理由**——拖到 Web 端（#75-#83）铺开后成本翻倍。

## Risks / Trade-offs

- [反向环误拆：候选表被意外清空会连带拆掉全部 reservation] → 前置验证确认候选表无自动清出路径；spec scenario 锁定"候选消失当且仅当显式撤销"作为判据前提；若未来引入候选过期机制，该 spec 会在 review 时拦截。
- [每轮 tick 多一次 watch_relays 读] → `with()` 零拷贝只读 key，1s 一次，可忽略。
- [`Failed` 无轮数后诊断信息变薄] → supervisor 的 links 内账仍有 attempts，失败时 tracing 带轮数输出，诊断走日志不走状态。
- [净层直接用户（绕过 core 的 `ensure_relay_reservation` 调用方）不受环保护] → 该场景没有 supervisor 在跑，也就没有反向环，行为与现状一致；net 层 API 语义不变。

## Migration Plan

单 PR：net 撤字段（净删）→ core 反向环 + 删 re-check → web 类型与产物重生成 → 测试适配。回滚 = revert。无数据迁移。

## Open Questions

（无——三条裂缝的处置均已定案：D1 改、D2 改、D3 否决。）
