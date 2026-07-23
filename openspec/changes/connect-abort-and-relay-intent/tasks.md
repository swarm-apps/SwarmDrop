# Tasks — connect-abort-and-relay-intent

## 1. 前置验证（不阻塞后续任务的 API 形状）

- [x] 1.1 在 `libs/` pin 的 libp2p master 上验证 `Swarm::close_connection(ConnectionId)` 对 pending dial 是否生效，结论记入 design.md（决定 `remove_infrastructure_peer` 是"立刻断"还是"失败后不再试"）——结论：`close_connection` 仅 established；改用 `disconnect_peer_id`（`Pool::disconnect` 会 abort pending），"立刻断"可行

## 2. crates/net — 机制层

- [x] 2.1 `RelayState` 三态化：`Connecting { attempt }` / `Active { circuit_addr }` / `Failed { attempts, last_error, next_retry_at }`（`endpoint.rs`），并适配 `watch_relays` 类型
- [x] 2.2 actor 失败翻转：`OutgoingConnectionError`（infra relay peer、全地址耗尽）→ `Failed`；`ListenerClosed`（reservation 失效）→ `Failed`；identify/重试 → `Connecting`；reservation 接受 → `Active` 并写入内核拼装的 circuit 地址
- [x] 2.3 新增 `ActorMessage::RemoveInfraPeer` + `Endpoint::remove_infrastructure_peer(node)`：摘 `infra_relay_peers`、清地址簿、`kad` 路由表移除、关闭对应 circuit listener（`relay_listeners` 反查）、删 `watch_relays` 条目
- [x] 2.4 （视 1.1 结论）remove 时对该 peer 的 pending/active 连接调用 `close_connection`
- [x] 2.5 `EndpointProfile::Browser` 的 `connect_timeout` 默认下调至 15s（builder/config）
- [x] 2.6 `crates/net/tests/relay_circuit.rs` 新增用例：不可达 helper 进入 `Failed`（含错误信息）、注销后无重试、注销活跃 reservation 关闭 listener、`Active` 携带 circuit 地址

## 3. crates/core — 策略层清理

- [x] 3.1 `NetManager`/`InfraSupervisor` 暴露注销入口：清 `links` 条目 + `BootstrapCandidateManager` 候选条目 + 调用 `endpoint.remove_infrastructure_peer`
- [x] 3.2 supervisor 单测：注销后 tick 不再对该候选发起收敛；学习型候选可在重新 identify 后重新纳管
- [x] 3.3 适配 `RelayState` 形状变更的 native 消费方（`network-status` 状态镜像层），`cargo check --workspace` 全绿（覆盖 mobile-core）

## 4. crates/web — JS API 重塑

- [x] 4.1 wasm 侧 `AbortSignal` 工具：`web_sys::AbortSignal` → future（一次性监听，drop 时 remove_event_listener，单点封装）
- [x] 4.2 `WebNode.connect(addr, opts?)` 接受 `opts.signal`，abort 立即 reject；文档注明"abort ≠ 撤回拨号"
- [x] 4.3 删除 `WebNode.reserve()`；新增 `relays_ensure(addr)` / `relays_drop(id)`（同步幂等命令，drop 联动 core 注销入口）
- [x] 4.4 新增 `relays_state()` 快照（id/状态/circuit 地址/尝试次数的 TS 类型化返回）+ `relay-changed` 事件推送（接 `watch_relays` 变更）
- [x] 4.5 新增 `relays_until_active(id, opts?)`：等首次 `active` resolve 出 circuit 地址；`failed` 立即 reject（意图保留）；支持 `opts.signal`
- [x] 4.6 同步 `docs/packages/swarmdrop-web/client.js` Worker 方法表镜像（逐一核对新增/删除的方法）——核实 client.js/worker.js 在 develop 与 feat/web-75-base 均不存在（README 描述的 Worker 模式尚未落地），无镜像可改
- [x] 4.7 specta/TS 类型导出与 `README.md`「遗留 / 取舍」更新（移除"无内建超时"记录，补新 API 契约说明）

## 5. 验证与收尾

- [x] 5.1 wasm 构建 + 浏览器手测：不可达地址下 connect ≤15s reject、`relays_ensure` 后 `failed` 可观测、`relays_drop` 后控制台无重拨日志——实测通过（connect 黑洞地址 11s reject；until_active 对不可达 helper 携轮数+错误 reject；drop 后静置 36s 拨号计数不变）；AbortSignal JS 路径经编译+类型检查覆盖，未单独浏览器实测（/try 页无 signal 入口）
- [x] 5.2 `cargo clippy` / `cargo test`（workspace）/ `cargo test -p swarmdrop-net` 全绿——clippy workspace 0 警告、net+core 16 套测试全过、check-wasm --clippy 绿
- [x] 5.3 在 issue #84 回帖关联本 change 与新 API 契约；标注 `feat/web-75-base` 分支的跟进项（删 `with-timeout.ts`、连接面板与 #77 切新 API）——https://github.com/swarm-apps/SwarmDrop/issues/84#issuecomment-5055624874
