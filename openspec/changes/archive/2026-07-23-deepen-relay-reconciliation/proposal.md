# deepen-relay-reconciliation

## Why

`connect-abort-and-relay-intent`（已归档）的 simplify 审查确认了三条架构级裂缝，当时因超出收尾范围而跳过。推敲后发现它们是**同一个问题的三个面**：net 层的 `RelayState` 承载了机制层无法自洽维护的信息（重试轮数——其语义由上层退避策略定义），而 supervisor 的收敛环**只有正向没有反向**（注销靠命令式三连 + tick 内 re-check 特例"收窄"竞态而非闭合）。

三条裂缝的现症：

1. **轮数双主人**：actor 的 `infra_relay_peers: HashMap<PeerId, u32>` 与 supervisor 的 `RelayLinkState.attempts` 各记各的——identify 重建路径靠 `.max(1)` 凑数、`event_loop` 的 LAN helper 即时注册会造成轮数漂移、两处归零规则（"Accepted 归零"）需人工对齐。
2. **注销绕过收敛环**：`remove_relay_intent` 是删候选 + 删 links + `RemoveInfraPeer` 的命令式三连；在途 tick 任务与注销的竞态靠 spawn 内候选表 re-check 缓解，re-check 通过后到 `AddInfraPeer` 送达 actor 前的窗口依然存在——注释自己承认只是"收窄到微秒级"。
3. **失败边沿只在采样轨**：`until_active` 在 watch 上等 `Failed`，可能被下一轮 `Connecting` 覆盖合并（浏览器实测第 3 轮才观察到）。

趁 Web 端仍无存量用户、上一 change 刚归档，一次把机制/策略边界修正到位。

## What Changes

- **crates/net（净删）** **BREAKING**：`RelayState` 撤掉轮数——`Connecting`（无字段）/ `Active { circuit_addr }` / `Failed { last_error }`。机制层只报告可自证的事实；`infra_relay_peers` 回归 `HashSet`，actor 的计数、归零、`.max(1)` 补丁全部删除。
- **crates/core**：
  - `InfraSupervisor::tick` 增加**反向收敛规则**：`watch_relays` 有条目而候选表已无该 peer → 发 `remove_infrastructure_peer`（幂等，直到条目消失）。期望状态模型补全为双向：候选有 → 收敛到有；候选无 → 收敛到无。
  - 删除 tick spawn 内的候选表 re-check——在途 add 即使复活内核登记，watch 重现条目后下一轮 tick 差集必然清理，**终态一致由环保证**而非靠收窄窗口。
  - `remove_relay_intent` 简化：删候选 + 删 links + 保留一次直接 `remove_infrastructure_peer` 作为**低延迟快路径**（环是竞态兜底，快路径管响应速度，二者幂等叠加）。
- **crates/web** **BREAKING**：`RelayInfoJson` 删除 `attempts` 字段（轮数是策略层内账，对 UI 无决策价值）；`until_active` 的失败文案去轮数。
- **明确否决**（记录在 design）：不新增 `RelayAttemptFailed` 事件轨——评估后无真实消费者（supervisor 退避不需要它，web 走采样 + 30s 契约兜底，残余风险仅为极端情况下错误文案降级为"超时"），为它铺 core→web 新数据通道属过度设计。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `infra-peer-lifecycle`: ① RelayState 不再携带尝试轮数——机制层只报告事实（连接中/活跃+地址/失败+原因），重试记账归策略层唯一所有；② 注销的"不复活"保证从"调用方按正确顺序清理 + re-check 收窄"升级为"supervisor 反向收敛环闭合"——内核状态与候选表的差集在有限轮 tick 内必然清零。
- `web-connection-control`: relay 状态快照不再包含 `attempts` 字段（BREAKING，无存量调用方）。

## Impact

- **代码**：`crates/net/src/{endpoint,actor}.rs`（净删~40 行）、`crates/core/src/infra/supervisor.rs`（+反向环，−re-check）、`crates/core/src/network/manager.rs`、`crates/web/src/{types,node}.rs`、`docs/app/try/page.tsx`（如有 attempts 展示）、wasm 产物与 TS 类型重新生成。
- **测试**：`relay_circuit.rs` 断言适配（`Failed { last_error }` 形状）；supervisor 新增"复活条目被反向环清理"用例；既有 `removed_candidate_is_not_converged` 语义不变。
- **风险面**：反向环的误拆边界——只对"候选表**完全没有**该 peer"触发（用户 drop / 候选被显式移除），`public_reachability` 降级等"候选在但不再想要 reservation"的场景**不**触发（维持现状，记为已知负债）。实现前需验证候选表无自动过期清出逻辑（mDNS 静默期误清候选会连带拆 reservation）。
