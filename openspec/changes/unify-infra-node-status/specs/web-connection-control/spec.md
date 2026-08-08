## MODIFIED Requirements

### Requirement: connect 支持标准 AbortSignal 取消

`WebNode.connect(addr, opts?)` SHALL 接受可选的标准 `AbortSignal`（`opts.signal`）；signal 触发时 Promise SHALL 立即以 abort 语义 reject。API SHALL 不自造 `timeoutMs` 类参数——超时组合由调用方经 `AbortSignal.timeout()` / `AbortSignal.any()` 表达。abort 不承诺撤回在途拨号（文档 SHALL 明示"abort ≠ 撤回拨号"），但 SHALL 保证无常驻意图残留。

该方法 SHALL NOT 用于判定引导节点或中继的可达性，文档 SHALL 明示这一点。理由有三：它会把候选地址永久写入地址簿且无失败回滚；对已连接的节点它直接返回既有连接快照（因而对已连上的内置节点永远返回成功，一个不可能失败的测试比没有测试更坏）；它测的是直连链路，而中继的实际用法是 reservation，两条链路不同。

引导节点的可达性 SHALL 由「登记意图 + 观测收敛结果」回答（见 `infra 意图以声明式集合管理`）。

#### Scenario: 调用方主动取消 connect

- **WHEN** `connect` 等待期间调用方 abort 传入的 signal
- **THEN** Promise 立即以 abort 语义 reject，后续该次调用不产生任何状态副作用

#### Scenario: 用平台原语表达超时

- **WHEN** 调用方以 `connect(addr, { signal: AbortSignal.timeout(5000) })` 调用且地址不可达
- **THEN** Promise 在约 5 秒时以 abort 语义 reject

#### Scenario: 不作为可达性判据

- **WHEN** 界面需要判断某条引导节点是否可用
- **THEN** 判据来自该条 `InfraLink` 的状态，而非 `connect` 的返回值

### Requirement: relay 意图以声明式集合管理

`WebNode` SHALL 以命令/查询分离的形式管理**基础设施意图**（不限于中继角色），命名统一为 `infra_*`：

- 命令：`infra_ensure(addr)` 登记意图、`infra_drop(id)` 撤销意图——二者 SHALL 同步返回、幂等，且 `infra_drop` SHALL 真正撤销内核常驻意图（联动 `remove_infrastructure_peer`），而非仅停止等待。`infra_ensure` SHALL 以 kad + relay 双角色登记（当前 `relays_ensure` 只登记中继角色，使浏览器登记的节点不进 kad 路由表）。
- 查询：`infra_links()` SHALL 返回全量 `InfraLink` 快照（含意图侧的来源/角色/scope 与观测侧的连接与 reservation 状态、失败原因、circuit 地址）；状态变化 SHALL 经 `infra_changed` 流推送。快照 SHALL NOT 包含重试轮数或下次重试时刻。

`relays_ensure` / `relays_drop` / `relays_state` / `relays_changed` SHALL 更名为上述形式。

#### Scenario: ensure 立即返回，状态经订阅到达

- **WHEN** 前端调用 `infra_ensure(addr)`
- **THEN** 调用同步返回；reservation 建立后经 `infra_changed` 与 `infra_links()` 快照可观测到 `active` 状态及 circuit 地址

#### Scenario: drop 停止后台重试

- **WHEN** 某 helper 处于失败退避循环中，前端调用 `infra_drop(id)`
- **THEN** 内核不再对该 helper 重试，其条目从 `infra_links()` 快照中消失

#### Scenario: reservation 掉线可观测

- **WHEN** 已建立的 reservation 因 relay 断线失效
- **THEN** 前端经 `infra_changed` 观测到状态离开 `active`，无需重新发起调用

#### Scenario: 失败快照含原因不含轮数

- **WHEN** 某意图处于失败退避中，前端读取 `infra_links()`
- **THEN** 该条目的 `relay` 为 `failed` 且携带 `lastError`，不存在 `attempts` 字段

#### Scenario: 登记同时进 kad 路由表

- **WHEN** 前端调用 `infra_ensure(addr)`
- **THEN** 该节点以 kad + relay 双角色登记

### Requirement: 提供可取消的"等待首次 Active"便捷方法

`WebNode` SHALL 提供 `infra_until_active(id, opts?)`：等待指定意图首次进入 `active` 并 resolve 出 circuit 地址；接受可选 `AbortSignal`；观察到 `failed` 状态时 SHALL 立即 reject（携带失败原因），而非等待内核退避重试。该方法 SHALL 仅是状态订阅之上的便捷封装，不改变意图生命周期（reject/abort 不隐式撤销意图）。

#### Scenario: 等待成功

- **WHEN** `infra_ensure` 后调用 `infra_until_active(id)`，reservation 随后建立
- **THEN** Promise resolve 出该意图的 circuit 地址

#### Scenario: 失败快速反馈

- **WHEN** `infra_until_active(id)` 等待期间该意图进入 `failed`
- **THEN** Promise 立即 reject 并携带失败原因；意图仍保留，是否 `infra_drop` 由调用方决定

## ADDED Requirements

### Requirement: Web 端持久化基础设施意图清单

Web 端 SHALL 持久化用户对基础设施清单的修改，使刷新页面后不丢失。

持久化 SHALL 存储 **custom（用户新增）与 removed（用户撤销的内置项）两个集合**，SHALL NOT 存储合并后的最终清单——后者会在新版本更换内置地址时把老用户永久压在旧地址上。

存储载体 SHALL 是 localStorage 偏好存储，SHALL NOT 是 IndexedDB——它是本机设置而非运行时状态。

节点启动序列 SHALL 在装配阶段回放该清单为运行时意图。

#### Scenario: 刷新后自定义节点仍在

- **WHEN** 用户添加一条自定义节点后刷新页面
- **THEN** 该节点被重新登记为运行时意图

#### Scenario: 刷新后已撤销的内置项不复活

- **WHEN** 用户撤销一条内置节点后刷新页面
- **THEN** 该节点不被重新登记

#### Scenario: 内置清单更新可达老用户

- **WHEN** 新版本更换了内置节点地址且用户未曾撤销过内置项
- **THEN** 用户获得新地址
