# connect-abort-and-relay-intent

## Why

Issue #84（P1）：`WebNode.connect()/reserve()` 对不可达地址没有超时/取消能力——`connect()` 要等 30s 默认超时，`reserve()` 的 Promise 会**无限挂起**（watch loop 等一个永远不会到达的 `RelayState::Active`），前端只能各自发明 20s `withTimeout` 兜底，且内核在背后持续重试、无任何撤销口子。

根因不是"缺个超时参数"，而是三个结构性缺口：

1. **语义错配**：`reserve()` 把"注册常驻 relay 意图 + 后台收敛"（声明式、无终点）硬包装成一次性 RPC（Promise 假装有终点）。
2. **注销面缺失**：`add_infrastructure_peer` 没有对称的 remove——`infra_relay_peers` 只进不出，意图一旦登记永不可撤。
3. **状态机不诚实**：`RelayState` 只有 `Connecting | Active`，拨号失败后状态停在 `Connecting`，观察者分不清"正在连"和"连不上退避中"。

Web 端（#75-#83）尚在起步、无存量用户，现在矫正 API 形状零兼容成本；`#77`（配对）即将复用 `reserve()`，晚改一天多一份兜底负债。

## What Changes

- **crates/net（机制层）**：
  - `Endpoint` 新增 `remove_infrastructure_peer(node)`：从 `infra_relay_peers`、地址簿、kad 路由表摘除，关闭对应 circuit listener——补齐注册型 API 的对称注销面。
  - **BREAKING** `RelayState` 状态机诚实化：`Connecting { attempt }` / `Active { circuit_addr }` / `Failed { attempts, last_error, next_retry_at }`；拨号失败必须反映到状态，circuit 地址作为 `Active` 的属性由内核给出（不再让调用方拼字符串）。
  - Browser profile 的 `connect_timeout` 默认下调（30s → 15s），作为"任何 Promise 有限时间内 settle"的内核兜底不变量。
- **crates/core（策略层）**：`InfraSupervisor` 与候选表响应注销（清 `links` 与候选条目）；收敛逻辑本身不动。
- **crates/web（适配层）** **BREAKING**：
  - `connect(addr, opts?)` 接受标准 `AbortSignal`（不自造 `timeoutMs` 参数）。
  - **删除** `reserve()`，代之以声明式 relay 意图 API：`relays_ensure(addr)` / `relays_drop(id)`（命令，同步幂等）、`relays_state()`（查询快照）、`relay-changed` 事件（订阅）、`relays_until_active(id, signal?)`（等待首次 Active 的糖，可取消、`Failed` 时提前 reject）。
  - `client.js` Worker 镜像同步更新方法表。
- **docs 前端（`feat/web-75-base` 分支跟进）**：删除 `with-timeout.ts` 兜底，连接面板与 #77 配对流程改用新 API（在该分支合并后作为后续任务执行，本 change 只交付 crates 侧）。

## Capabilities

### New Capabilities

- `infra-peer-lifecycle`: 基础设施节点（relay helper / bootstrap）的登记-注销对称生命周期，以及 relay reservation 的诚实状态机（连接中/活跃/失败可观测，含退避信息）。覆盖 crates/net 的 Endpoint API 与 crates/core 的收敛清理。
- `web-connection-control`: 浏览器端连接控制的 JS API 契约——connect 的可取消性（AbortSignal）与有界性（内核兜底超时），relay 意图的声明式管理（ensure/drop/state/事件/until_active），"任何 Promise 有限时间内 settle"不变量。

### Modified Capabilities

（无——主 specs 中尚无 relay/infra/web-node 相关既有能力。）

## Impact

- **代码**：`crates/net/src/{endpoint,actor,config,event}.rs`、`crates/core/src/infra/supervisor.rs`、`crates/core/src/network/{manager,candidates}.rs`、`crates/web/src/{node,events}.rs`、`docs/packages/swarmdrop-web/client.js`（方法表镜像）。
- **API**：`RelayState` 枚举形状变更（native 端 `network-status` 显示层需随动）；`WebNode.reserve()` 删除（无存量调用方，仅 `feat/web-75-base` 分支的连接面板，随分支合并改造）。
- **测试**：`crates/net/tests/relay_circuit.rs` 增不可达 helper / 注销 / Failed 状态用例；`crates/core` supervisor 单测补注销清理。
- **协调**：与在途 change `webrtc-direct-reachability`（占用 `web-node` 能力名但 spec 为空）无实质重叠；web 前端改造依赖 `feat/web-75-base` 合并时序。
- **待验证风险**：libp2p `Swarm::close_connection` 对 pending dial 是否生效（决定注销是"立刻断"还是"失败后不再试"），实现前在 `libs/` pin 版本上花 10 分钟验证，不影响 API 形状。
