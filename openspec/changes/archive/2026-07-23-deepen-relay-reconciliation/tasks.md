# Tasks — deepen-relay-reconciliation

## 1. crates/net — 轮数撤出机制层（净删）

- [x] 1.1 `RelayState` 改为 `Connecting`（无字段）/ `Active { circuit_addr }` / `Failed { last_error }`（`endpoint.rs`），文档同步"轮数归策略层"的边界说明
- [x] 1.2 `infra_relay_peers` 回归 `HashSet<PeerId>`；删除 `ensure_relay` 的递增、`ReservationReqAccepted` 的归零、`set_relay_connecting` 的 `.unwrap_or(1).max(1)` 自取——`set_relay_connecting` 退化为无参写入
- [x] 1.3 `set_relay_failed` 去掉 attempts 查表，保留 `relay_listeners` guard 与相等去重不变
- [x] 1.4 `relay_circuit.rs` 断言适配：`Failed { last_error }` 形状、`unreachable_helper_enters_failed` 去掉 attempts 断言；全套用例回归绿

## 2. crates/core — 反向收敛环

- [x] 2.1 `InfraSupervisor::tick` 增加反向规则：`endpoint.watch_relays().with(读 key 集合)` 与候选表快照做差集，对"内核有、候选无"的 peer spawn `remove_infrastructure_peer`（幂等，条目消失前每轮重发）
- [x] 2.2 删除 tick spawn 内的候选表 re-check（含注释）——竞态闭合责任移交反向环
- [x] 2.3 `remove_relay_intent` 注释更新：直接注销调用标注为"低延迟快路径"，环为竞态兜底，二者幂等叠加；`InfraSupervisor::remove` 的文档同步
- [x] 2.4 supervisor 单测：新增"watch 有条目而候选表无 → tick 发出注销"（用真实 endpoint 登记后删候选，断言有限轮内 watch 条目消失）；`removed_candidate_is_not_converged` / `removed_candidate_can_be_relearned` 回归不变
- [x] 2.5 supervisor 失败重试的 tracing 输出带 `links.attempts` 轮数（补偿状态侧轮数移除后的诊断面）——tick 的"第 N 次尝试" debug 已有，另在 ReservationLost info 补了轮数

## 3. crates/web — 快照瘦身

- [x] 3.1 `RelayInfoJson` 删除 `attempts` 字段（`types.rs`）；`relay_info_json` 投影适配；`until_active` 失败文案去轮数（`node.rs`）
- [x] 3.2 specta 导出回归（`cargo test -p swarmdrop-web --features specta`）+ wasm 产物与 `.d.ts` 重新生成；`docs/app/try/page.tsx` 如有 attempts 展示一并清理

## 4. 验证与收尾

- [x] 4.1 `cargo clippy --workspace` / `bash scripts/check-wasm.sh --clippy` / `cargo test -p swarmdrop-net -p swarmdrop-core` 全绿；docs `pnpm exec tsc --noEmit` 绿
- [x] 4.2 浏览器冒烟（/try 页）：ensure 不可达 helper → failed 可观测（无轮数、含错误）；drop 后无重拨；与前序 change 的实测行为对齐——实测通过：failed 含真实握手错误无轮数；drop 后静置 36s 拨号计数 5→5 零新增；反向环未误触发（快路径成功时环静默，符合设计）
- [x] 4.3 知识库更新：net-kernel.md 补"轮数归属策略层 + 反向收敛环"条目；确认 rust-backend/toolchain 无需增改
