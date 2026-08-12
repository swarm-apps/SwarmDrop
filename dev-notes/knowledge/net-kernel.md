# 网络内核（swarmdrop-net）开发知识

> 覆盖 `crates/net-base` + `crates/net`（2026-07 重构产物，取代 `libs/` 的 swarm-p2p-core
> ——**该 submodule 已于 2026-07 从本仓删除**，`.gitmodules` 不存在，历史源在独立仓
> `swarm-apps/swarm-p2p`）。
> 架构设计依据见 `dev-notes/why-libp2p-not-iroh.md`；重构决策过程见当次 plan。
>
> **libp2p 依赖当前指向个人 fork**，不是官方 master——见下方「libp2p git pin 校准实录」。

## 架构速览（改内核前必读）

```
宿主(src-tauri/mobile-core/wasm壳)
  → swarmdrop-core(组合根 + 网络/配对/presence + SqlSessionStore + CoreEvent 聚合)
  → swarmdrop-transfer(传输域，经端口 trait 依赖倒置，双 target 可编)
  → swarmdrop-host(宿主端口层：FileAccess/EventBus/error/device 数据类型)
  → swarmdrop-net(内核) → swarmdrop-net-base(类型底座)
```

依赖倒置（2026-07 传输域独立 crate）：`swarmdrop-transfer` 不依赖 sea-orm / pairing /
network 模块，持久化走 `store::{SessionStore, InboxStore}`、配对目录走 `peer::PeerDirectory`、
事件发射走 `events::TransferEventSink`、生命周期清理走 `runtime::TransferRuntime`，均由
core 侧实现注入。`CoreEvent`/`EventBus`/`MemoryHost` 留在 core（`CoreEvent` 反向引用 transfer
wire 类型，下沉会成环）。`swarmdrop-host` + `swarmdrop-transfer` 已进 `check-wasm.sh`。

- **Endpoint 是 `Arc<Inner>` 门面**（Clone 廉价），单中枢 actor 是唯一 Swarm poll 点；
  快路径不经 actor：开流走 `libp2p_stream::Control`，状态读走 watch。
- **事件双轨制**：状态用 watch（last-value-wins 采样：addrs/nat/conns/relays），
  必达边沿用 bounded mpsc(256) 的 `NetEvent`——**不要混用**（用 watch 数事件会丢边沿，
  用事件流存状态会堆积）。
- **协议按 `ProtocolId` 路由到 stream 级**（尊重 multistream-select：一条连接多协议子流，
  与 iroh 的 per-connection ALPN 刻意不同）。`Rpc<Req,Resp>` 是「一流一问一答」helper，
  **handler 可在回复前 await 用户决策**——旧 pending_id/PendingMap 机制因此不存在。
- **libp2p 类型不出内核**：上层只见 net-base 的 NodeId/Addr/NodeAddr/ProtocolId。
  `#[doc(hidden)]` 的 `as_peer_id()/from_multiaddr()` 只供内核互转，业务层禁用。
- 扩展点四件套范式（ergonomic RPITIT trait + Dyn trait + blanket impl）：
  `ProtocolHandler`、`RpcService`、`AddressLookup`(+Builder 回填)。

### relay 意图的机制/策略分界（2026-07-23 定稿，deepen-relay-reconciliation）

- **`RelayState` 不携带重试轮数**：机制层只报告可自证事实（`Connecting` / `Active{circuit_addr}` /
  `Failed{last_error}`）。轮数语义由退避策略定义，唯一账本在 core 的 `InfraSupervisor.links`
  （诊断走 tracing，不下发状态）。别再往 RelayState 加策略派生字段——actor 无法自洽维护它
  （identify 重建、LAN helper 即时注册等路径会造成漂移，曾实证）。
- **收敛环是双向的**：tick 正向（候选有→内核有）+ 反向（`watch_relays` 有条目而候选表无该
  peer → 幂等发 `remove_infrastructure_peer`）。注销与在途注册的竞态由环的终态一致性闭合，
  **不要**在共享收敛路径上加 re-check/epoch 类特例。反向判据的前提：候选表只经显式撤销移除
  （无自动过期清出）且所有生产路径的 relay 登记均有候选条目——**引入候选自动清出机制前必须
  重新评估**（spec `infra-peer-lifecycle` 已锁定该前提）。
- `remove_infra_intent` 的直接注销调用是低延迟快路径，环是兜底，二者幂等叠加。

### ⚠️ `Connecting` / `Failed` 没有 NetEvent —— core 事件循环必须订阅 `watch_relays`（2026-08-08 修）

边沿轨只有 `RelayReservationAccepted` / `RelayReservationLost` 两个。`RelayState::Connecting`
与 `Failed{last_error}` **没有任何对应事件**，只存在于 watch 轨。

`run_event_loop` 此前只订了 `watch_addrs` 与 `watch_nat`，于是原生端**从来没观测到过**这两个
态：桌面/移动 UI 里「引导节点」永远只有一个聚合布尔，失败原因根本到不了前端。Web 端之所以是
三端唯一能逐条显示 relay 状态的，正因为它直接订 `relays_changed()` **绕过了 core 这一层**——
那不是它做得好，是另外两端的链路断了一截。

**正确做法**：任何依赖 relay 三态的上层，事件源必须是 `endpoint.watch_relays()`，不能靠
`NetEvent`。不会造成推送风暴：`set_relay_state` 用 `send_if_modified` 做了值相等去重，
supervisor 走 2s→75s 退避。

**相关文件**：`crates/core/src/network/event_loop.rs`（`run_event_loop` 的 select）

### 候选表只存意图，观测值一律现算（2026-08-08 定）

`BootstrapCandidateManager` 是**期望状态**的权威源（谁该在、什么地址、什么角色、什么来源）。
连接与 reservation 的**事实**分别在 `watch_conns` / `watch_relays`，由
`crates/core/src/infra/link.rs` 的 `build_infra_links` 现场 join 成 `InfraLink` 读模型（零存储）。

**不要**把观测值写回候选表。此前 `BootstrapCandidate.health` 就是这么做的，而四条路径不回写：

- `cancel_relay_reservation` 与 `handle_remove_infra_peer` **刻意不发** `RelayReservationLost`
  （免得上层把用户取消误判成需要自动恢复的故障）；
- `set_relay_failed` 的另外几条路径（含 `OutgoingConnectionError`）同样不发。

后果不在 UI 层：`presence/supervisor.rs` 的 `relay_hints()` 按那个位筛出 `RelayHint` 写进发布到
**公共 DHT** 的 `OnlineRecord`，对端拿去「先修 relay 直连、再拨 circuit」。于是本机持续发布一条
早已失效的中继路径，对端拨号必然失败，**日志无痕**。

判据现在是 `relay_hints_from(candidates, relays)` 这个纯函数（读 `watch_relays` 的 `Active`），
回归测试 `relay_hints_follow_live_relay_state` 钉住它。

### 引导节点的启动登记：kad 无条件，relay 就地过闸门（2026-08-09 定稿）

`runtime.rs` 注册 host 配置的引导节点时，`kad_server` 恒为真，`relay` 按
`public_reachability || CandidateScope::infer(addrs) == Lan` 现场算。

此前用 `InfraRoles::bootstrap()`（kad + relay），于是关掉「公网可达性」的用户**照样在启动时
建了公网 reservation**：`wants_reservation` 只管收敛环，管不到这条一次性注册。
中间还有一版改成写死 `relay: false`、把 relay 整个推迟给 `InfraSupervisor::tick`——闸门是堵上了，
但每个引导节点变成先注册一次再注册一次，首次可达性凭空晚一个 tick。**闸门查询要放在调用点，
不是把整件事推迟。**

⚠️ 判据必须与 `exclusion_for` 同一条（见下面 `CandidateScope` 那节）。写成全局开关一刀切，
私网引导节点会在启动时被拦掉、又被 tick 加回来，每次冷启动白拨一轮。

⚠️ 反过来也不能把这段删掉改走候选表。`wants_reservation` 要求 relay 角色 + tick 里是
`continue`，所以 `public_reachability=false` 时公网候选**一次 `add_infrastructure_peer` 都不会
发** → kad 路由表拿不到任何公网种子 → `dht.bootstrap()` 与在线记录发布全塌。
「别让我被动可达」与「跨网还能不能找到人」是两件事，不能让一个吃掉另一个。

### 候选 scope 由 `upsert` 单点推断，调用方不得指定（2026-08-08 修）

`upsert` 对 roles 是 `|=` 累加、对 scope 曾是**直接覆盖**，而三个调用方给三种拼法（启动路径
硬编码 `Public`、运行时意图用 `infer`、局域网协助路径硬编码 `Lan`）。于是一个既被用户手填
（含私网地址）又被 identify 认出的节点，scope 会在 `Lan`/`Public` 之间来回翻转——而
`wants_reservation` 直接吃它，该候选就在收敛环里时进时出。

现在 `upsert` 内部按**合并后的全部地址** `CandidateScope::infer`，签名里没有 scope 参数。
`infer` 自身的判据 2026-08-09 又翻转过一次，见下面单独那节——那次翻转修的是另一个 bug。

### 「本端拨得动这条地址吗」是内核事实，不是部署配置（2026-08-08 加）

`Endpoint::supported_transports() -> &[TransportKind]` 在 bind 时按 target + 配置算一次。
清单的定义**紧挨着 `build_swarm` 住在 `crates/net/src/transport.rs`**，并有一条双向护栏测试
（`supported_transports_match_the_assembled_stack`：既查"有"也查"无"）——只查其一会放过多报或少报。

- 多报一种：用户能配下一条**永远连不上**的引导节点，且没有任何错误提示；
- 少报一种：当场拒掉合法地址。

上层判据在 `crates/core/src/infra/validate.rs`，**三端共用一份**：可解析 → 含合法 `/p2p/` →
含可拨传输 → 该传输本端装配了 → 不是本机、不与既有条目重复。五条全部**无网络往返**。

两个容易写错的点：

- **circuit 地址取的是外层中继段的传输**（`/ip4/../tcp/../p2p/<relay>/p2p-circuit/p2p/<target>`
  → `Tcp`），而那正是本机要拨的那一跳；身份则取末位（目标）。`Addr` 的 `transport()` 与
  `p2p_node_id()` 各自已经是对的，别在上层再拼一遍。
- **去重按「同 peer **且**同地址」**，不是按 peer。同一节点的 TCP 与 QUIC 地址是同一条关系的
  两条路径，`upsert` 会合并；按 peer 去重会挡住用户补一条 QUIC 地址。

⚠️ **不要用 `Endpoint::connect` 当连通性探测原语**：它把候选地址永久写进 address_book
（无 TTL、无失败回滚，清理入口 `remove_infrastructure_peer` 会断连）；已连接时直接返回既有
连接快照，所以对内置节点**永远绿**；而且它走直连，relay 的实际用法是 reservation。
Web 端那颗「测试连通性」按钮就是这么变成一个不可能失败的测试的，已删。

**相关文件**：`crates/net/src/transport.rs`、`crates/core/src/infra/validate.rs`

### 并发拨号是**延迟**竞速，会系统性选出最慢的传输（2026-08-12 定）

`dial_concurrency_factor` 没设（libp2p 默认 8），而候选地址通常 ≤5 —— 等于**全部并发发出，
谁先建连谁赢**。问题是我们在意的是吞吐，而两者恰好反向：

| | 建连延迟 | 吞吐 |
|---|---|---|
| 中继 | **最低**（复用一条已建立的连接） | **最差** |
| webrtc-direct | 中（ICE + DTLS） | 72 MiB/s |
| WebTransport | 略高（多一次 QUIC 握手） | 322 MiB/s |
| 打洞 | **最高**（等 ICE 收敛数秒） | 好 |

所以「谁赢」必须由**层级**决定，不能交给竞速。判据收在 `Addr::dial_tier()`
（`crates/net-base`），三档：`DirectFast`（TCP/QUIC/WebTransport）→ `DirectSlow`
（webrtc-direct/打洞）→ `Relayed`，`Ord` 方向是**越小越好**。

**判定必须看 `/p2p-circuit` 的位置，不是有没有。** 三种地址都同时含 circuit 段与 WebRTC 段：

| 地址 | 真身 | 档 |
|---|---|---|
| `…/webrtc-direct/certhash/…/p2p/R/p2p-circuit/p2p/T` | **中继**，第一跳恰好是 webrtc-direct | `Relayed` |
| `…/p2p/R/p2p-circuit/webrtc/p2p/T` | **打洞**，circuit 只用于信令 | `DirectSlow` |
| `…/udp/…/webrtc-direct/certhash/…/p2p/T` | 直连 | `DirectSlow` |

第一行判错的后果最隐蔽：**浏览器的中继地址第一跳正是 webrtc-direct**，判成直连就会把
「换一条中继」当成升级完成。护栏是 `circuit_is_judged_by_position_not_presence`。

### 升级的判据是「有没有**更快的**直连」，不是「有没有直连」（2026-08-12 修）

`try_upgrade_to_lan` / `try_upgrade_to_direct` 此前的前置条件是 `only_relayed(peer)`。
后果：一旦升级到 webrtc-direct，`only_relayed` 变 false，**两条升级路径永久关闭**，
从此锁死在慢传输上——而 WebTransport 上线后这恰好成了常态。

现在统一走 `best_tier(peer)`（所有连接里最好的一档）+ `wants_upgrade_to(peer, target)`，
两条特例收敛成一条规则。`lan_candidates` 随之改成**只拨严格更优的那一档，且只拨那一档**：
层内可以继续竞速（同档差别不大），层间必须有序。

⚠️ **筛「本端拨得动吗」必须排在挑档之前。** 浏览器最快的一档（对端自报的 `/tcp`、
`/quic-v1`）恰好是它**唯一拨不动**的一档；先挑档再筛，浏览器会永远挑中拨不动的那条：
拨号立刻失败 → 在途标记清掉 → 5 分钟后 identify 再来一轮挑同一条，**永远升不上去**，
而每一步看起来都在正常工作。判据是 `transport::dialable_kind(kind, browser)`，
纯函数以便两个 target 都能在 native 上测。

### 升级必须**关掉劣档连接**，否则新流在两条连接间掷硬币（2026-08-12 修）

`libp2p-stream` 的 `Shared::sender()` 是这一行：

```rust
.choose(&mut rand::thread_rng())
```

**在该 peer 的所有连接里均匀随机挑一条。** 而升级会新建一条连接、旧的不会自动消失，于是：

> 只升级不关旧连接，等于把「走慢连接」的概率从 100% 降到 50%，而不是消除它。

这条**早于按档升级就存在**（中继 → 打洞升级同样如此），且 UI 会显示「直连」——
`best_conn` 按 `path_rank` 取最好的那条报给前端，于是**界面说直连、一半的流在走中继**：
本仓最不喜欢的那类形态，一半诚实一半撒谎。

现在 `prune_inferior_conns(peer)` 在两处触发：**连接建立后**（主路径——升级通常发生在
identify 首帧，此时用户还没开始传，一关就干净）与**每次 identify**（自愈，补上被挡掉的轮次）。

⚠️ **判据只能是「该 peer 一条流都没有」**（`StreamRegistry::is_idle`）。想关得更准就得知道
「哪条流在哪条连接上」——而 `libp2p-stream` 随机挑完**并不回报挑了哪条**，从我们这侧根本
观测不到，registry 也只按 `(peer, protocol)` 记账。所以宁可保守：有任何活跃流就整轮跳过，
**绝不打断正在传的数据**。

代价是「传输途中升级成功」那一格要等到下一轮 identify（~5 分钟）才收敛。**这个代价是对的**：
那条正在传的流本来就已经绑在旧连接上，早关晚关都救不了它，而早关会直接把它打断。

三条护栏：`inferior_conns_lists_everything_below_the_best_tier`（该关的都关）、
`inferior_conns_keeps_every_connection_in_the_best_tier`（只有一条时无论多差都不关——
关了就断线）、`is_idle_covers_both_directions_and_recovers_after_drop`（**入站也算活跃**，
只看出站会把正在接收的会话判成空闲）。

**相关文件**：`crates/net-base/src/addr.rs`（`DialTier`）、`crates/net/src/actor.rs`
（`best_tier` / `lan_candidates`）、`crates/net/src/transport.rs`（`dialable_kind`）

### 集成测试里的「没有传输层」：`impl IncomingTransferRuntime for ()`

`run_event_loop` 的泛型约束是 `IncomingTransferRuntime`，而 `NetManager<()>` 本来就是
既有集成测试的常规构造（`TransferRuntime for ()` 早就有）。所以 `()` 也实现了
`IncomingTransferRuntime`，全部入站请求以 `AppError::Transfer` **婉拒而不是 panic**。

写新的网络层集成测试时直接传 `()`，不要在测试文件里手写 5 个方法的 no-op 双——那份样板
会在 trait 每次改动时红一遍，而它表达的东西 `()` 已经表达了。

### `CandidateScope` 是「持有公网地址」，不是「不含私网地址」（2026-08-09 翻转）

`CandidateScope::infer` 判 `Public` 的条件是**任一非 circuit 的公网可路由地址**
（`CandidateScope::is_infra_public_addr`，`usable_public_addrs` 共用同一个谓词）。

**别翻回「任一私网地址即判 Lan」。** 那是原始写法，它有一个静默失效的开关：
`upsert` 按合并后的全部地址重算 scope，而地址表**只增不减**——自建 bootstrap 跑在同一
局域网、用户按内网地址把它加进来，随后 identify 并入它的公网地址，旧判据就让 scope
永久停在 `Lan`。而 `InfraSupervisor::exclusion_for` 正是拿这个位当公网闸门，于是关掉
「公网可达性」的用户照样在一台真·公网中继上建了 reservation，被跨网直达，**开关无声失效**。

翻过来之后 `Public` 是吸收态，方向是安全那一侧：地址只增不减，「见过公网地址」这个事实
本就不该被后来的私网地址抹掉。纯局域网 helper（只有私网/回环地址）仍判 `Lan`、仍不受公网
开关约束——「用户手点的本地 helper 不该被公网开关拦下」这条原意保留。

**这个位有两个消费方，必须同源**：`exclusion_for` 的闸门，和读模型下发给三端的
`scope`（shared-view / `node_health.rs` 拿它算 `configuredLanOnly`）。闸门另写一份地址
判据就会出现「被拦下了但 UI 说不出为什么」——用户看到红色的「找不到任何节点」，而真相
是他自己关的开关。`runtime.rs` 的启动注册同理复用 `infer`，不要在那里写第三份。

**相关文件**：`crates/core/src/network/candidates.rs`、`crates/core/src/infra/supervisor.rs`、
`crates/core/src/runtime.rs`

### relay 边沿事件**不推** `NetworkStatus`——那条轨由 `watch_relays` 独占

`actor.rs` 在 emit `RelayReservationAccepted` / `Lost` **之前**必然先写 `watch_relays`
（`set_relay_state` / `set_relay_failed` 就在 emit 上一行）。而 `run_event_loop` 已经订阅
了那条 watch，所以 core 的 `NetEvent` 分支里**不要**再 `publish_network_status`：两边都推
= 每次转换重建两遍全量状态（候选快照克隆 + 两张 watch 深拷贝，移动端还要多过一次 uniffi）。

更隐蔽的一半：reservation 每轮续期都发 `Accepted { renewal: true }`，而 watch 侧
`send_if_modified` 判值相等**不通知**（那是刻意的静音）。NetEvent 分支会把这份静音拆回来，
于是每个 relay 每个续期周期都在推一份一模一样的状态。

两条轨的到达顺序不定（`select!` 随机），但这不构成问题：watch 只承载 relay 三态，
`ever_active` 由 `infra.handle_event` 折叠，而 `deriveInfraLinkState` 先判 `relay == active`
才看 `everActive`——两者唯一同时生效的组合（曾活跃过、现已失败）里，`ever_active` 早在更早
那次 `Accepted` 就置位了。**加新的 relay 相关字段时要重新过一遍这个论证。**

**相关文件**：`crates/core/src/network/event_loop.rs`、`crates/net/src/actor.rs`

### 与旧栈（swarm-p2p-core）的关键差异

| 旧 | 新 | 原因 |
|---|---|---|
| `start::<Req,Resp>()` CborMessage 泛型贯穿 | 无泛型；协议注册在 Router | 业务类型不入网络层 |
| request_response behaviour + PendingMap | stream 上的 `Rpc` + handler 长 await | 三件套机制整体消失 |
| 巨型 NodeEvent 枚举直接进前端 | watch + 小 NetEvent；前端事件由 core 层 CoreEvent 组装 | 事件/状态分轨 |
| 命令责任链（trait 对象穿链） | 扁平 ActorMessage 枚举 + oneshot | 协议数固定，责任链的开闭收益换不回间接成本 |
| kad 路由表兼职地址簿 | actor 自维护 AddressBook | `Swarm::add_peer_address` 只是广播（见坑 3） |

## libp2p git pin 校准实录

> **pin 目标的时间线**：
> 1. 官方 master `93c5059`（2026-07-13 快照）—— 下方 6 条坑均在此 rev 上实读校准，结论仍适用
> 2. **当前：个人 fork `github.com/yexiyue/rust-libp2p`**。确切 rev 以根 `Cargo.toml` 为准，
>    **不要从本文抄 rev**——文档必然滞后于 Cargo.toml。
>
> **这是当前架构最大的单点依赖风险**：上游安全更新需自行 rebase。退出条件见下节，
> 那里写死了可判定的判据和验证命令。

**为什么是 git 不是 crates.io**：crates.io 的 libp2p-webrtc 0.9.0-alpha.1 的 webrtc-direct
实证跑不通，修复只在 master（PR 6429，已于 2026-05-22 合并但**尚未进任何 crates.io 发布版**）。
identity/multiaddr 不用跟 git——master 树自己解析到 crates.io（0.2.14 / 0.18.2），
net-base 用 crates.io 版本天然 unify。

**升级 rev 必须走独立 PR + 全量测试 + wasm check**，并同步 Cargo.lock。

### 临时 fork 集成策略 —— 含退出条件（校准于 2026-07-27）

Web 端在上游合并前需要 WebRTC DataChannel 回调生命周期修复与连接级消息上限协商，
因此 workspace 暂时 pin `yexiyue/rust-libp2p`。该分支以 `libp2p/rust-libp2p:master`
为基线；`Cargo.lock` 必须与精确 revision 一起提交。

#### fork 到底比上游多什么（2026-08-01 更新）

**四条补丁全部已提 PR，无一漏提。** 四个 PR 都还 OPEN。

| fork commit | 补丁 | 上游 PR | 状态（2026-07-31） |
|---|---|---|---|
| `db1bc23e` | `fix(webrtc-websys): defer data channel callback wakes` | [#6558](https://github.com/libp2p/rust-libp2p/pull/6558) | **OPEN** |
| `c7d37a8d` | `feat(webrtc): negotiate data channel message limits` | [#6560](https://github.com/libp2p/rust-libp2p/pull/6560) | **OPEN** |
| `9e3bcd9b` | `docs: add WebRTC message limit changelogs` | #6560（同 PR） | **OPEN** |
| `c4c2c167` + `989cb610` | separate / configure receive buffer limit | #6560 的 `5984c716`（**squash 成单 commit**） | **OPEN** |
| `262dea51` | `fix(relay): don't panic on circuit request without a matching reservation` | 我们提的 [#6570](https://github.com/libp2p/rust-libp2p/pull/6570) 已 **CLOSED**，改跟上游自己的 [#6472](https://github.com/libp2p/rust-libp2p/pull/6472) | **#6472 已 MERGED** |
| `d858435c` | `feat(identify): allow updating agent_version at runtime` | [#6576](https://github.com/libp2p/rust-libp2p/pull/6576)（PR 分支另起，见下） | **OPEN**（2026-08-01 提） |

末行那条**不是别人偷塞进来的**：它是本仓 `identify-agent-version-runtime-update` 变更的
产物，分支 `feat/identify-runtime-agent-version`，基线正是上一版 pin 的 `262dea51`
（刻意没 rebase 到上游 master，免得把上面几个 PR 的状态搅进这次变更、出问题时分不清是谁的锅）。
给 identify 的 `Behaviour` 加 `set_agent_version` + `InEvent::AgentVersionChanged`，
逐连接下发新值——**没有它，改设备名必须重启整个节点**（断开所有连接、中断进行中的传输），
`crates/net` 侧绕不过去：`agent_version` 在每条连接建立时就被 clone 进了该连接的 Handler，
只改 Behaviour 的 config 对已建立的连接无效。补丁按上游可接受的形态写（英文 doc、参数是裸
`String`、不带任何 SwarmDrop 语义、附 smoke 测试与 CHANGELOG 条目），
**2026-08-01 已提为 [#6576](https://github.com/libp2p/rust-libp2p/pull/6576)**。

###### 提 PR 时另起了一条分支，别把两条搞混

`262dea51` **不在上游 master 的历史上**（`git merge-base --is-ancestor` 判否）。直接从
`feat/identify-runtime-agent-version` 提 PR 会把 **9 个 commit** 一起带进去 ——
#6558 / #6560 两条 PR 的分支和 relay 修复全在里面。所以 PR 走的是另一条分支：

| 分支 | 基线 | 用途 |
|---|---|---|
| `feat/identify-runtime-agent-version` @ `d858435c` | `262dea51`（fork 线） | **本仓 Cargo.toml pin 的就是它** |
| `feat/identify-set-agent-version` @ `da9e151d` | 上游 master `3667c6c6` | 提给上游的 #6576（含 changelog 的 PR 链接 commit） |

两条内容等价、SHA 不同。**pin 的那条永远不要 force-push 或删除** —— `d858435c` 一旦变成
游离对象被 GitHub GC，`Cargo.lock` 就拉不到它，本仓构建当场断。要跟进 review 意见只改
PR 那条（上游禁止 force push，只能追加 commit）。

提交前在**上游最新 master 上**自测过：`cargo test -p libp2p-identify` 4 单测 + 9 smoke 全过
（含新增的 `runtime_agent_version_update`）、`cargo fmt --check` 干净、
`cargo clippy -p libp2p-identify --all-targets` 本 crate 零 warning。

##### relay panic（2026-07-28，线上实证）

**这条不是为 Web 端加的，是线上 relay 真的挂了才发现的**——公网 bootstrap 跑了 44 分钟后
进程退出，日志停在：

```
thread 'tokio-rt-worker' panicked at protocols/relay/src/behaviour.rs:719:21:
assertion `left == right` failed
  left: None     ← 该连接的 reservation 状态
 right: Active
```

成因：relay 处理 circuit 请求时从 `HashMap<ConnectionId, Reservation>` 里 **`.iter().next()`
取任意一条连接**，然后 `assert_eq!(*status, Active)`。而连接建立时一律以 `Reservation::None`
入表，只有 reserve 成功才翻成 `Active`。于是**目标节点只要多一条未 reserve 的连接**
（重连时旧连接尚未清理、或因别的原因又拨了一次 relay），HashMap 的随机迭代序就可能取到它，
整个 relay 进程随之退出——**可被远程触发**，源节点只需在那一刻发 circuit 请求。

还有个更安静的后果：即使断言侥幸通过，circuit 也可能被挂到一条并不持有 reservation 的连接上。

修法是找**持有 reservation 的那条**（`is_active()` 早就存在，只是这一处没用），找不到就落进
已有的 `NO_RESERVATION` 拒绝分支。

⚠️ **本仓 pin 到 `262dea51` 只是让新构建不再带这个 bug；已在跑的线上节点必须重新部署才生效。**

> **对账时别只比 commit SHA。** PR 分支上的 `5984c716` 是 fork master 那两个 receive-buffer
> commit 压缩后的形态——SHA 不同但内容等价（`poll_data_channel.rs` +70/-3 ≈ 两个 commit 的
> 净效果）。只跑 `compare master...fork` 看 SHA 差集，会把**已提 PR 的补丁误判成「未提」**
> （本文档 2026-07-27 就这么错过一次）。正确做法：`gh pr view <n> --json commits` 看 PR 实际
> 内容，或直接比对文件。
>
> 同理，**两个 PR 分支互不包含**：#6558 与 #6560 各自基于上游 master 独立开分支，拿 fork master
> 跟任一 PR 分支比，都会看到另一个 PR 的改动被列为「差异」——那不是遗漏。

**WebRTC 那批补丁里唯一未进 PR 的内容**：`misc/webrtc-utils/src/stream.rs` 里 3 行文档注释
（说明 transport-local 聚合接收缓冲为何与协商的单条消息上限相互独立）。纯注释、零功能影响，
上游合并后随 rebase 自然消失，**不构成退出阻塞**。

（identify 那条是另一回事——它是**有功能的**未提项，见上表末行与下节退出条件。）

#### 退出条件（两阶段，各自可判定）

**阶段 1 — 切回官方 git URL**：三个 PR 都进入 upstream master。

```bash
# 主判据：三个 PR 均为 MERGED —— 此时上游 master 已含全部所需修复
gh pr view 6558 --repo libp2p/rust-libp2p --json state --jq .state
gh pr view 6560 --repo libp2p/rust-libp2p --json state --jq .state
gh pr view 6472 --repo libp2p/rust-libp2p --json state --jq .state   # relay panic，已 MERGED
```

> relay panic 那条要查的是**上游自己的 #6472**，不是我们提的 #6570——后者源码与 #6472 逐字节
> 相同，2026-07-28 已按维护者要求关闭，**永远不会变成 MERGED**。照旧判据查 6570 会得到一个
> 永不满足的退出条件。

三个都 MERGED 后，把**五行** git 依赖（libp2p / -stream / -core / -swarm / -webrtc-utils）的
URL 换回 `libp2p/rust-libp2p`、rev 换成上游 master 上含这三个 PR 的 commit，跑全量测试 +
`./scripts/check-wasm.sh`。

> **identify 那条补丁是独立于上面三条的第四条，判定上要分开看。**
> 它**不进阶段 1 的判据**——三个 WebRTC/relay PR 合不合并，与它毫无关系，别因为它没提 PR 就
> 认为阶段 1 退不了。
>
> 但它**阻塞「删掉 fork pin」这个终局**：`set_agent_version` 只在 fork 上，pin 一删，
> `crates/net` 的 `Endpoint::set_agent_version` 直接编不过，改设备名就退回「必须重启整个节点」。
> 所以终局多一步 —— 等 [#6576](https://github.com/libp2p/rust-libp2p/pull/6576) 合并
> （2026-08-01 已提，见上表），或明确接受功能回退。**两条都没成之前，fork pin 不能删**。
>
> 判定：`gh pr view 6576 --repo libp2p/rust-libp2p --json state --jq .state`

```bash
# 辅助判据（查漏用，不是合并信号）：确认 fork 上没有漏提的自有补丁。
# ahead_by 不会因 PR 合并而自动归零 —— squash 与 merge commit 让 SHA 对不上，
# 要等 fork master 重置到上游后才归零。它回答「还有没有自有补丁」，不回答「能不能切」。
gh api repos/libp2p/rust-libp2p/compare/master...yexiyue:master --jq '.ahead_by'
```

**阶段 2 — 切回 crates.io**：上游发布含这些修复的版本。

```bash
# 当前（2026-07-27）：crates.io 仍是 0.56.0 / 0.9.0-alpha.1，都不含 webrtc-direct 修复
cargo search libp2p --limit 1
cargo search libp2p-webrtc --limit 1
```

判据：`cargo search` 显示的 `libp2p` ≥ **0.57.0**、`libp2p-webrtc` ≥ **0.10.0-alpha**、
`libp2p-stream` ≥ **0.5.0-alpha**（这三个正是 fork 树上的版本号，见 Cargo.lock）。
到那时把 git 依赖整体换成 crates.io 版本号，`libp2p-stream` / `libp2p-webrtc` 仍需与
`libp2p` 同期版本对齐。

> **复查节奏**：这三条命令跑一次不到 10 秒。建议每次要动 `crates/net` 时顺手跑一遍——
> 拖得越久，fork 与上游的 rebase 成本越高。

`max_message_size` 只限制单条编码后的 DataChannel 消息，以及发送端的背压高水位；浏览器
回调在 Rust task 再次 poll 前可能已累计多条合法消息，故 `webrtc-websys` 的累计读取缓冲
通过 `Config::with_max_read_buffer_size` 单独显式配置为 256 KiB。它是本地资源上限，不参与
协商；库会保证其不低于单条消息上限。两者不能混用，否则连续合法的 8 KiB 消息会被错误判定
为对端过载并重置 stream。数据面每个 target 都应调用 `flush()`；回调唤醒已由 #6558 延后，
不能再以跳过 `flush()` 规避回调重入。

**正确做法**：
- 每次更新先将 fork `master` 快进到上游，再重新合并仍未被上游接受的修复并跑 WebRTC/wasm 检查。
- 上游合并或发布可用版本后，切回官方 URL（或 crates.io）并删除 fork pin。

**不要做**：
- 不要在产品仓库直接 pin 已删除分支或孤立 commit；这样 lockfile 无法长期可靠复现。

### 坑 1：relay server 的 HOP 协议默认不广告（relay 0.22.0，PR 6154）

**行为变更**：HOP 协议广告默认 `Status::Disable` 且随 **external address** 自动开关。
私网 LanHelper 没有公网地址 → auto 模式**永远不会开 HOP** → reservation 请求在
multistream 层被静默拒绝（症状：`Listener: rejecting protocol .../hop`，无任何 relay 事件）。

**正确做法**：配置了 relay server 即显式 `server.set_status(Some(relay::Status::Enable))`
（`crates/net/src/behaviour/mod.rs` 已做，`tests/minimal_relay.rs` 固化对照）。

### 坑 2：reservation 应答必须携带 relay 自身 external 地址

server 无 external 地址时照样 accept reservation，但应答里 0 个地址——**client 侧**报
`NoAddressesInReservation` 直接关 circuit listener（server 日志还显示 accepted，极具迷惑性）。
所以 `announce_private_addrs` 承担双重职责：identify 广播 + reservation 应答地址，
且判定含 loopback（生产无害、测试必需）。

### 坑 3：`Swarm::add_peer_address` 不是地址簿

它只把 `NewExternalAddrOfPeer` 广播给各 behaviour——没有 behaviour 存储就没有任何效果；
dial 的候选地址来自 behaviour 的 `handle_pending_outbound_connection`。
**内核自维护 AddressBook**（`actor.rs`），不依赖 kad 兼职（旧栈的做法）。

### 坑 4：拨号在途时 `dial()` 报 `DialPeerConditionFalse`

并发 connect / infra dial 撞在途拨号时不是错误——挂进 connect 等待表共享结果
（ConnectionEstablished / OutgoingConnectionError 到达时统一应答）。

### 坑 5：circuit listen 前必须先与 relay 有活跃连接

`listen_on(<relay>/p2p-circuit)` 不会自己把连接建好。正确顺序（旧栈实证、新内核沿用）：
dial relay → identify 到达 → 才 listen circuit。内核的 `ensure_relay` 封装了这个时序
（未连接先拨号，identify 经 `infra_relay_peers` 幂等触发真正 listen）。

### 公网 Bootstrap + Relay 必须显式登记外部地址（2026-07）

公网节点的实际 listener 常绑定 `0.0.0.0` / `[::]`。这类地址不能直接作为
Circuit Relay reservation 的应答地址，否则客户端会以 `NoAddressesInReservation`
拒绝 reservation。`Swarm::add_external_address` 又不保证回发
`ExternalAddrConfirmed`，只依赖 watch 事件会让状态与实际 Swarm 配置分叉。

**正确做法**（2026-08-12 起，见下节；此处保留问题描述）：
- 组合根在 `Endpoint::bind()` 前经 `Builder::external_addrs()` 登记**不带 certhash 且恒定
  不变**的那几条（TCP / QUIC）；它们同时成为 `watch_addrs().external` 初值。
- 其余全部交给 `Builder::external_ip()`，由内核从监听地址映射。

**相关文件**：`crates/net/src/{endpoint/builder.rs,actor.rs,addrset.rs}`、
`crates/bootstrap/src/lib.rs`

### 公网地址跟着监听地址走，不从证书第二次派生（2026-08-12）

内核多了一条 external 来源：`Builder::external_ip(IpAddr)`。给了它之后，actor 持续维护
「当前监听集合中每条**非 circuit** 地址 ⇒ IP 段换成这个 IP」这一份，与宿主声明的
（`external_addrs`）、AutoNAT 确认的三者取并集下发。判据全在
`crates/net/src/addrset.rs` 的 `map_to_public_ip`（circuit 排除 + 改写后去重），
三条单元测试各守一条。

**这条机制取代了两样东西**，两样都不要加回来：

1. **bootstrap 里那个自制的地址跟踪任务**（watch 监听地址 → 筛 WebTransport → 改写 IP →
   `set_external_addrs(静态 ∪ 观测)`）。它做的正是内核现在做的事，但记了第二份账；而
   `set_external_addrs` 是整份替换，两个事实源不一致的症状是「某条地址悄悄不再被通告」，
   本机日志完全正常。bootstrap 现在只剩一个**纯日志**的 watch 循环。
2. **`swarmdrop_net::webrtc_direct_addr_from_pem()`**（已删除）。它是「从证书算 certhash」
   的第二条路径，与传输启动时实际使用的那条并行存在；漂移的症状是浏览器在 TLS 阶段被拒、
   日志毫无线索。映射版按定义不可能与传输不一致——certhash 直接取自监听地址本身。

**为什么 TCP/QUIC 仍然静态声明一次**：它们不用等监听结果就能算出来，而 bind 返回到第一批
`NewListenAddr` 抵达之间有个窗口，那期间来的 reservation 请求拿不到可拨地址会被客户端以
`NoAddressesInReservation` 整个拒掉（上一节踩过）。映射随后算出同样两条，重合去重，所以
这份预声明是纯保险。带 certhash 的两条不能这么办——静态算它们就得重新引入上面第 2 条。
`bootstrap` 有一条测试（`statically_declared_addresses_agree_with_the_public_ip_mapping`）
钉住「预声明的内容必须与映射结果一致」，否则就是凭空多通告一条从未监听过的地址。

**相关文件**：`crates/net/src/{addrset.rs,actor.rs,endpoint/builder.rs}`、
`crates/bootstrap/src/lib.rs`

### 公共基础设施地址由 Host 配置，核心只消费候选（2026-07-24）

`swarmdrop-core::NetworkRuntimeConfig` 不再内置公网 bootstrap/relay 地址；公共节点是各端
部署策略，桌面、移动和浏览器的可用 transport 不同，必须由各自 host 注入完整 multiaddr。

**正确做法**：
- 桌面端在 `src/lib/bootstrap-nodes.ts` 维护 TCP / QUIC / WebSocket 等可用地址，启动时与用户偏好合并。
- 移动端在 `mobile/src/core/bootstrap-nodes.ts` 维护 Android 可用的 TCP / QUIC 地址；当前不放 `/ws`。
- 浏览器在 `docs/app/app/_lib/relay-helpers.ts` 使用 WebRTC Direct 或 WSS helper；每项必须附带 `/p2p/<peer-id>`，WebRTC Direct 还必须带稳定的 `certhash`。
- 新公网 relay 同时承担 circuit relay 时，仍需按上一节登记其外部地址；客户端清单只解决“如何拨到它”，不替代服务器侧公告。

**不要做**：
- 不要把某一端可用的 `/ws` 或 `/webrtc-direct` 地址无差别下发给所有端；Android 当前无法拨 WebSocket，而浏览器不能拨 TCP/QUIC。

**相关文件**：`crates/core/src/network/config.rs`、`src/lib/bootstrap-nodes.ts`、`mobile/src/core/bootstrap-nodes.ts`、`docs/app/app/_lib/relay-helpers.ts`

### 每个 relay 只申请一份 reservation（2026-07-28 修）

`request_relay_reservation` 曾对地址簿里该 relay 的**每个地址各 listen 一次**，于是一台
通告 9 个地址的 LanHelper 就收到 9 份 reservation 请求。而配额是 **per-peer** 的
（`max_reservations_per_peer` 默认 4），多数请求以 `ResourceLimitExceeded` 被拒 →
listener 批量关闭 → reservation 反复丢失重建。公网 relay 的总配额（32）也曾被几个
测试端占满，实测时表现为「怎么都 reserve 不上」。

**一份就够**，三条依据：

1. 走到该函数时**必然已连上 relay**（两个调用点都在 `conns` / identify 之后，见坑 5 的
   时序），relay client 的 `ListenReq` 走「复用现有连接」分支；
2. 我们传的地址只用来拼那条要通告的 external 地址，**不参与建连**；
3. relay client 的 `reservation_addresses` 以 `ConnectionId` 为键——多份本就互相覆盖，
   最终生效的只有一份。

实测：请求数 13 → 2（两个 relay 各一份），`ResourceLimitExceeded` 归零。

### presence 的启动序列必然抢跑——失败必须短退避，不能等满一个周期（2026-07-29 修）

`PresenceSupervisor::run` 的装载把每台已配对设备排成 `next_probe_at: now`（立即重探），
而那一刻 relay reservation 还没建立、DHT 也没连上任何节点——**首探注定失败**。原先
`Unreachable` 分支无条件把下次排到 `probe_interval(75s) + jitter(≤15s)` 之后，
**排期与探测结果无关**，于是第二次机会在一个完整周期之后。

实测：浏览器刷新页面，已配对设备要 **89s** 才翻回在线。修成 2s 起步逐次翻倍、封顶回
基础周期后，三次刷新分别是 **6s / 3s / 4s**。

同一个启动序列还会让首次 `announce_online` 撞空（下面那条），但 announce 早就有
`announce_backoff`（2s 起步）——**只有重探漏了退避**。改动同构，见
`probe_backoff` 与回归守卫 `first_probe_failure_retries_fast`。

> 通用教训：**任何"启动时立即做一次"的动作，都要假设它跑在网络就绪之前**，
> 失败路径必须能快速重试。固定周期在这种场景下等价于「首次失败 = 一个周期的不可用」。

### ⚠️ 别把浏览器的 `QuorumFailed` 归咎于 kad client 模式（2026-07-29 证伪）

浏览器启动早期 `announce_online` 会报一次
`QuorumFailed { success: [], quorum: 1 }`。曾据此判断「浏览器无 AutoNAT →
kad 恒 `Mode::Client` → PutRecord 第二阶段拿不到 success」。**这个判断是错的**，
沿它排查会一路走进 libp2p-kad 源码而找不到问题。

两条实证否掉它：

1. native 端用 `server_mode: false`（Client 模式，与浏览器完全一致）对同一个 bootstrap
   做 `dht.put`，**成功**。client 模式 put 不了这个前提不成立。
2. 开 `libp2p_kad=trace` 看失败的那次查询，**一条 `Request to peer in query succeeded`
   都没有**——不是第二阶段拿不到 success，是**第一阶段就没有 peer 可问**。日志行序说明
   一切：失败的 `QueryId(0)` 出现在第 5 行，而 webrtc-direct 连接第 55 行才就绪。

真正的原因还是上面那条：**启动序列抢跑**。它只失败一次，`announce_backoff` 2s 后重试
即成功——硬证据是让 native 去 DHT 读浏览器的 presence 记录，读得到（1100 字节，
含正确的 circuit 地址）。

另注：浏览器拿到 relay reservation 后，circuit 地址会被 confirm 成 external address，
kad 随即 `Switching to server-mode`。所以浏览器**并非恒 Client**，这也是上述判断的
另一处事实错误。

### 坑 6：kad `Record.expires` 的类型按 target 分叉

native = `std::time::Instant`，wasm = web_time（与 `n0_future::time::Instant` 同源）——
写跨平台代码需 cfg 分支（`actor.rs` 的 DhtCommand::Put 有样例）。

### 坑 7：Android 上 hickory 读系统 DNS 走 JNI，两处入口都会炸（2026-07-20 实证）

master 的 libp2p-dns 依赖 hickory-resolver 0.26，其 `system_conf` 在 Android 上经
`ndk_context::android_context()` 读系统 DNS——RN/uniffi 宿主没有任何初始化入口，
`Endpoint::bind`（start）时直接报 `android context was not initialized`。**炸点有两处**：

1. `with_dns()` → `Transport::system`。修法：Android target 用
   `with_dns_config(公共 DNS, ResolverOpts::default())`（transport.rs 有
   `android_dns_config()`：AliDNS/DNSPod/Cloudflare/Google udp+tcp 四组）。
2. ~~`with_websocket()` 的宏展开硬编码 `Transport::system`~~ —— **随 WebSocket 整体移除
   而消失（2026-07-28）**，Android 与桌面的 transport 栈现已一致。上游缺口
   <https://github.com/libp2p/rust-libp2p/issues/6529> 对本项目不再有影响。

### Android 条件编译分支也必须通过 `-D warnings`（2026-07-24）

移动端 release 使用 `RUSTFLAGS=-D warnings`，而仅在桌面目标编译的分支会让 Android
的局部可变绑定变成硬错误。对于 listener 等平台差异，直接把 `#[cfg]` 放在 `vec![]`
元素上，避免先声明 `mut` 再在某个 target 中 `push()`。

**相关文件**：`crates/net/src/endpoint/presets.rs`

`NameServerConfig` 需要直接依赖 hickory-resolver（libp2p::dns 只
re-export ResolverConfig/ResolverOpts），版本必须与 libp2p-dns 同线（crates/net 的
android target 依赖表）。

### 坑 8：取消在途拨号要用 `disconnect_peer_id`，不是 `close_connection`

`Swarm::close_connection(ConnectionId)` 只对 **established** 连接生效（`pool.get_established`），
对 pending dial 返回 `false`——不能中断在途拨号。**`Swarm::disconnect_peer_id(PeerId)` →
`Pool::disconnect` 才会对该 peer 的 pending 连接调用 `connection.abort()`**（pool.rs 文档明示
"whether pending or established are closed asap"）。`remove_infrastructure_peer` 的"立刻断"
语义靠它实现（2026-07-23 pin 93c5059 源码实读，`actor.rs::handle_remove_infra_peer`）。

### 坑 9：watch 采样会跳过短暂中间态（事件双轨制的实证补充）

浏览器实测 `relays_until_active`：不可达 helper 第 1 轮 `Failed` 写入 watch 后，JS 侧消费者
经常在第 2-3 轮才观察到 Failed（wasm 单线程下 actor 与 JS future 抢调度，last-value-wins
覆盖中间值）。**依赖"看到每一次状态翻转"的逻辑必须走 `NetEvent` 边沿轨**；watch 只保证
最终收敛值可见。对 until_active 这类"等终态"逻辑无影响（Failed/Active 会持续存在直到下轮）。

### 连接路径由「谁先建成」定终身——所以升级必须主动发起（2026-08-03 修）

**症状**：同一个局域网里的两台已配对设备，连接徽标长期停在「中继」——不是显示错了，字节真的
在绕公网。

**成因链**（三条缺一不可，改任一条都能复现）：

1. presence 经 DHT 发现对端在线后立刻 `connect`，那一刻地址簿里往往只有 circuit 候选
   （mDNS 还没到，或对端平台压根收发不了组播），relay 于是先赢；
2. `handle_connect` 开头「已连接就返回当前快照」——之后再多的 `connect` 都不会重拨；
3. `try_upgrade_to_direct` 只在 identify 的 `listen_addrs` 里 `find(is_webrtc)`，
   **对端自报的私网地址被整个忽略**；mDNS `Discovered` 也只 `record_addr` + emit，不拨号。

**修法**：`actor.rs` 新增 `try_upgrade_to_lan`，两个地址来源汇进来——identify 的 `listen_addrs`
（**主路径，不依赖 mDNS**）与 mDNS `Discovered`（来得更早）。

四个要点，改这块前逐条对：

- **别只修 mDNS**。它在两个移动平台都要过平台的门（下一节），而 identify 自报的私网地址一个门
  都不用过。mDNS 只是「来得更早的那份」。
- **LAN 升级不做 `should_initiate` 定序，打洞继续做**。LAN 握手是毫秒级无信令，两端各拨最坏多
  一条 idle 回收的连接；定序则会让「只有一端拨得通」（防火墙拦入站 / 一端 mDNS 瞎了）彻底没救。
  打洞一次是数秒 ICE + 信令往返，那才值得定序。
- **两条路径的在途标记必须分开存**（`upgrading_lan` / `upgrading_direct`）。共用一个的话，跨网
  场景下「对端自报的私网地址必然拨不通 → 失败 → 下轮又先试 LAN」会把打洞永久锁死，而 identify
  默认 5 分钟才来一轮。
- **候选要排除 circuit**。局域网 helper 自己就监听在私网地址上，它派发的 circuit 地址前半段同样
  `is_private_lan()`——不排除就会把「换一条中继」当成「升级为直连」。
- **候选上限必须按传输分组，不能笼统 `take(N)`**。原生端同时监听 tcp / quic-v1 /
  webrtc-direct，各自再乘网卡数与 IPv4/IPv6，一台手机自报六条私网地址是常态；而
  webrtc-direct 是 listen 列表里**最后**注册的，笼统截断砍掉的正是它。
  **那一刀正好打死浏览器**：浏览器拨不了裸 TCP/QUIC，webrtc-direct 是它够到局域网内原生端的
  唯一路径。症状是「浏览器 ↔ 同网段的手机永远停在中继」，且没有任何报错可查
  （`lan_candidates` 按 `Addr::transport()` 分组，两条单测钉死）。

**相关文件**：`crates/net/src/actor.rs`（`try_upgrade_to_lan` / `only_relayed` /
`clear_upgrade_marks` / `is_lan_candidate`）

### mDNS 在 iOS / Android 上要过平台的门（2026-08-03）

内核一直是开着的（`presets::Native` → `.mdns(true)`，移动端同一 profile），**但两个平台各自会
把组播吃掉**，症状是「代码没问题、设备就是发现不了」：

| 平台 | 必需项 | 缺失的后果 |
|---|---|---|
| iOS | `NSLocalNetworkUsageDescription` | iOS 14+ 连权限弹窗都不出现，组播静默丢弃 |
| iOS | `NSBonjourServices` 含 `_p2p._udp` | libp2p mdns 的服务名，见其 `SERVICE_NAME` |
| Android | `CHANGE_WIFI_MULTICAST_STATE` + `MulticastLock` | Wi-Fi 芯片省电态直接在驱动层丢组播帧 |

Android 的锁在 `mobile/modules/lan-multicast`（expo module，`setReferenceCounted(false)`），
生命周期绑节点启停——持锁期间芯片不进省电态，节点停了还持着只是白耗电。

⚠️ **iOS 侧未验证的一层**：裸 socket 绑 5353 + 加入 `224.0.0.251` 组播组（libp2p-mdns 正是这么
做的）可能还需要 Apple 特批的 `com.apple.developer.networking.multicast` entitlement——经系统
Bonjour API 浏览不需要，裸 socket 大概率需要。**这不阻塞局域网直连**：上一节的 identify 升级路径
不碰组播，mDNS 只影响「多快发现」。

顺带修掉的地雷：`behaviour/mod.rs` 里 mDNS 构建曾是 `.expect("mDNS initialization failed")`——
把一个平台可选能力做成了启动硬前提，任何不给绑 5353 的环境会在节点启动时直接 panic。现已降级
为 warn + 无 mDNS 继续跑。

### 其余确认

- `with_wasm_bindgen()` 在 master 仍在（删的是 cargo feature，不是方法）。
- websocket phase 依赖 dns feature 的隐式耦合仍在（同开即可）。
- `NetworkBehaviour` derive 的 **cfg 字段**（mdns/autonat/dcutr）双 target 编译均过；
  但 native 行为只有 relay/kad/identify/ping 被测试实证，**mdns/autonat/dcutr 的
  运行时行为待真机冒烟确认**（mdns 在移动端另有平台门，见上面那一节）。
- ConnectionHandler 的关联类型（InboundOpenInfo 等）与 0.56 一致，keep_alive
  behaviour 近零改动移植。

## WebRTC 打洞传输接线（`crates/webrtc-p2p`，2026-07-28）

自研的打洞传输已接进内核。内核层面可关（`EndpointConfig.webrtc_p2p: Option<WebRtcP2pConfig>`，
经 `Builder::webrtc_p2p(..)` 开启），但 **core 的组合根对三端一律开启**
（`crates/core/src/runtime.rs`）——**打洞要两端都支持，只开浏览器等于没开**：
`web ↔ NAT 后的桌面/手机` 那一格照样全程中转，而那恰恰是自研它最想拿下的场景。
对原生端也不是冗余：dcutr 走 TCP/QUIC 直连，ICE 走 UDP + STUN 候选，覆盖的 NAT 类型不同。

与官方 `libp2p-webrtc`（webrtc-direct）是**两个传输、可共存**：那个要求目标地址已可达，
这个让双方都不可达的节点经 relay 换信令后打洞。

### WebSocket 已整体移除（2026-07-28）

客户端 transport、桌面 `/ws` listener、bootstrap 的 4002 端口一并砍掉。它唯一的活是
「同网浏览器直连桌面」，webrtc-direct 做得更好：不占 TCP 端口、私网公网同一条路径。

**三个副产品**：

1. **坑 7 只剩一处炸点**。原来 Android 的 JNI DNS 问题有两处（`with_dns` 与
   `with_websocket`），后者随 ws 消失——Android 与桌面的 transport 栈现在完全一致。
2. **前缀误匹配的隐患没了**。circuit 地址形如 `/ip4/…/tcp/…/ws/p2p/<relay>/p2p-circuit/…`，
   ws transport 只认前缀就照单全收、无视 circuit 段，会真的连上 relay 然后报
   `WrongPeerId`（实测踩过）。
3. **native 也能打洞了**。`with_websocket` 是 `ws.or_transport(已有)` 把自己排到最前，而
   builder 不允许在 websocket phase 之后再加 transport——它在时，native 无法把 webrtc-p2p
   排到 relay 前面。砍掉后两个 target 用同一套装配。

Worker 模式（曾是 ws-only，因 webrtc-websys 在 Worker 里必 panic）同时失去意义——
`docs/app/app` 从未真的 `new Worker`，那是 spike 遗留。

### ⚠️ 不能用 `with_relay_client`——relay 会抢走打洞地址（2026-07-28 浏览器实测）

打洞地址 `<relay>/p2p-circuit/webrtc/p2p/<target>` **含 `/p2p-circuit` 段**，而 relay client
transport 的 `parse_relayed_multiaddr` 只要求有 circuit 段——**circuit 之后的 `/webrtc` 被
塞进 `dst_addr` 而不报错**（`priv_client/transport.rs` 的 `p => { ... dst_addr.push(p) }`），
于是它照单全收。

谁先拿到取决于 `or_transport` 顺序，而 `with_relay_client` 内部写死
`relay_transport.or_transport(已有链)`——**relay 永远在最前**。`with_other_transport` 无论
注册多少次都在它后面，抢不过。

**症状极隐蔽**：reservation 正常、中转正常、传输正常，只是打洞路径**一次都没被调用过**，
且没有任何报错。实测靠在 webrtc-p2p 的 `listen_on` 里打日志才发现（那条日志因此保留）。

**正确做法**：跳过 builder 的 relay phase——自己 `libp2p::relay::client::new()` 造 transport，
用 `relay_first_webrtc()` 把 webrtc-p2p 放在 relay 前面，behaviour 经闭包外的
`let mut relay_behaviour = None` 回传，最后用 **单参数** `with_behaviour(|key| ...)`
（`RelayPhase` 的快捷方式，内部走 `without_relay()`）。多传一个 relay_client 参数就会走回
那条把 relay 排最前的路。

js-libp2p 没这个问题——它的 circuit transport filter 显式排除带 `/webrtc` 的地址。
rust-libp2p 缺这道排除，因为上游还没有 private-to-private 的 WebRTC 传输。

### listener 只在 `request_relay_reservation` 里挂，不跟随 reservation 事件

本机要能「被拨」，需要一个 `<relay>/p2p-circuit/webrtc/p2p/<本机>` 监听
（`Actor::ensure_webrtc_listener`），它才会进 `watch_addrs().listen` → `dialable()` → 邀请。

两条约束：

1. **不能进 `relay_listeners` 表**。它不是一份 reservation（webrtc-p2p transport 收下地址
   只是登记 listener，不向 relay 请求任何东西），混进去它的 `ListenerClosed` 会被误判成
   reservation 失效 → 误发 `RelayReservationLost`。故另存 `webrtc_listeners: PeerId → ListenerId`。
2. **不要挂在 `ReservationReqAccepted` 的处理路径上**。那条路径正是 relay client 更新自己
   `reservation_addresses` 表的时刻，在其中插入 `listen_on` 会扰动它的内部时序，把它的
   `expect("Relay connection exist")` 打成 panic——浏览器实测 2/2 必现，短路后 0/2。
   撤销同理，只在 `handle_remove_infra_peer` 做。
   代价是 reservation 掉线期间地址短暂不可达；幂等重试会拉回来，比让对端地址簿反复失效好。

### 浏览器侧排障：先收紧 tracing filter

Web 端曾是全局 `DEBUG`，libp2p 各层的日志（multistream 协商、每连接 poll、identify push…）
把浏览器 console 的行数上限冲爆——**自己的日志一条都看不到**。排 webrtc 打洞时因此误判过
「对端毫无响应」。现按 target 分层（`crates/web/src/lib.rs` 的 `Targets`），日志量降两个
数量级。**碰 Web 端排障先确认 filter，别对着被冲掉的日志下结论。**

> 反过来，**桌面与移动端的 filter 是太窄而不是太宽**：`swarmdrop=debug` 按前缀匹配
> 够不着 `webrtc_p2p`，两端曾经一条 webrtc-direct 日志都收不到。2026-08-10 已单列
> `webrtc_p2p=info`，见 [`rust-backend.md`](rust-backend.md) 的「tracing 的 target 是前缀匹配」。

### ⚠️ 浏览器接收侧**没有背压**——大文件的流控只能由应用层做（2026-08-06 修）

**症状**：桌面向浏览器发 20 MB，传到 12–22% 断，发送侧会话消失。小文件（3 MB 以下）
一路正常，所以很容易误判成偶发网络问题。

**实测现场**（浏览器 console）：

```
对端消息堆积超过读缓冲上限，重置子流 buffered=4194171 incoming=8190 max_read_buffer=4194304
```

精确撞在 4 MiB 上。`4 MiB / 20 MB ≈ 20%`，与观察到的中断点吻合。

**根因是 WebRTC 的 API 缺口，不是本仓的 bug**。背压链有三层，这条链路上只有第三层能补：

| 层 | 机制 | 实际 |
|---|---|---|
| SCTP 逐跳 | `a_rwnd` 收缩 → 发送端停发 | **被短路**：浏览器 `onmessage` 一触发就把字节交给 JS 并释放 SCTP 接收缓冲，窗口永不收缩 |
| 本地发送队列 | `SEND_BUFFER_LIMIT`（已配 4 MiB） | 防的是**本端 OOM**，管不到「已送达对端 JS 但未被消费」的量 |
| 应用层端到端 | 逐块 Ack | wire v2 删掉了 → **缺口在这一层** |

这是 W3C 承认了十余年的缺口：[2014 年的 public-webrtc 提案](https://lists.w3.org/Archives/Public/public-webrtc/2014Mar/0063.html)
就写明「底层 SCTP 完全支持背压，只是 `DataChannelInterface` 这层 API 不支持」，请求加
`setReadEnabled(bool)`，至今未采纳（[webrtc-pc#1732](https://github.com/w3c/webrtc-pc/issues/1732)
仍开着）。libp2p 的 WebRTC spec 也明确写着 message framing「is not concerned with
flow-control」——**显式把流控推给上层**。js-libp2p 同样只做了发送侧 `bufferedAmount`
背压，接收侧是无界 `Pushable` 队列。

**为什么只有一个方向坏**：native 侧 webrtc-rs 是**拉模型**（`dc.poll().await`），不读就真不读，
SCTP 窗口会收缩 → 浏览器→桌面天然有背压；桌面→浏览器是推模型，没有。

**修法**：`swarmdrop-transfer` 在数据面加信用窗口（`WINDOW_CHUNKS = 16`，即 4 MiB 在途）——
发送方每满一窗发一帧 `TransferDataFrame::Window` 并停下，接收方**把窗内每一块验签落盘完毕后**
回同款帧才放行。停等而非滑动：滑动要在写的同时读确认，与数据面「整流顺序读写、不 split」
的硬约束冲突；代价是每窗一个 RTT，20 MB 只停 5 次。

**两个数是绑死的**：`WINDOW_CHUNKS × CHUNK_SIZE`（4 MiB）必须显著小于 `webrtc-p2p` 的
`DEFAULT_MAX_READ_BUFFER`（16 MiB）。**调大窗口必须同时抬那个上限**，否则退回越限重置。
护栏是 `actor::sender::tests::sender_stops_after_one_window_until_peer_acks`——删掉窗口后
native↔native 照样跑得通（yamux/QUIC 顶着），只有浏览器会被撑爆，那是跑不进 CI 的失效模式，
这条断言是它唯一的机器守卫。

**别把 `MAX_BUFFERED_AMOUNT` 当成对端的在途上限。** 旧注释里就是这么推的（「对端可以合法地
让 1 MiB 数据在途」），**是错的**：那个常量约束的是本端往外发，与对端往里灌毫无关系；后者
在应用层窗口出现之前根本没有上限。这个错误推理正是 `DEFAULT_MAX_READ_BUFFER` 当初被定成
4 MiB 的原因。

#### ⚠️ 加一个帧 tag 就必须换协议名——`decode_frame` 对未知 tag 是**硬失败**

窗口帧最初是直接加进 `/swarmdrop/transfer-data/2` 的，`TRANSFER_DATA_VERSION` 也留在 2。
那样发出去，**已发布的 v0.12.0 会在 4 MiB 处被全部打断**：它的解码器读到 tag 7 返回
「未知 transfer-data frame tag」，接收端随即中止整个会话。小于一窗的文件照常成功，所以
症状是「小文件行、大文件到 20% 就断」——与本节开头那个 bug 一模一样，极容易误判成没修好。

**这不是理论风险**：桌面与移动是两条独立版本线，移动端的更新永远滞后，而「桌面发照片到
手机」正是最常走的路径。

**修法**（都在 `crates/transfer`）：

- `TRANSFER_DATA_PROTOCOL` 提到 `/swarmdrop/transfer-data/3`，同时保留
  `TRANSFER_DATA_PROTOCOL_V2` 常量；
- `core::runtime::build_router` 用**同一个 handler** 注册两个协议名——接收端的读循环对有没有
  窗口帧都成立（没有就一帧也读不到），差别只在发送端；
- 出站在 `wire::data_plane::open_data_stream` 里先拨 `/3`，**只有** `OpenError::UnsupportedProtocol`
  才退回 `/2`（其余错误换个协议名重试同样失败，只会把真错误换成更没信息量的第二条）；
- 发不发窗口帧由流自己带的协商结果决定（`WindowPacer::for_protocol` 读 `P2pStream::protocol()`），
  **不由调用方传参**——这个判断只该有一个来源。

**`TRANSFER_DATA_VERSION` 保持 2 是刻意的**：它校验的是「共有帧怎么编码」，而 v3 只是多认
一个 tag、既有帧编码逐字未变。更实际的一条——退回 `/2` 时 Hello 必须仍然写 2，跟着提就把
回退路径自己堵死了。能力协商交给 multistream-select，别在 payload 里再做一遍。

护栏：`unpaced_stream_emits_no_window_frames`（v2 链路上一帧都不许发）与
`pacer_only_paces_on_the_negotiated_v3_protocol`。

### ⚠️ 不关闭的 `PeerConnection` 会把 CPU 烧光（2026-08-06 修）

**症状**：桌面端「卡死」——webview 对任何 JS 都超时，看起来像前端挂了。实际是
**CPU 被吃光**：`ps` 显示 948%，webview 只是抢不到时间片。

**定位手法记一笔**（下次省半小时）：`sample <pid> 3 -mayDie -file out.txt`，然后读
`Sort by top of stack` 那一段。这次的结论一眼可见——前十几项全是
`PeerConnectionDriver::event_loop` / `RTCPeerConnection::poll_write` / `poll_timeout` /
`mach_absolute_time`。**注意 `pgrep` 拿到的第一个 pid 往往是 pnpm 的 wrapper**，
按 `ps aux | awk '$3 > 50'` 挑真正吃 CPU 的那个。

**根因**：`webrtc-rs` 的 `PeerConnection` 背后是一个独立的 driver 任务，只有 `close()`
之后才退出。上游的 `Drop` **只在 `dedicated_reactor` 模式下**设 shutdown 标志，并把
general-runtime（我们用的那条）注释为「detach harmlessly onto the application's own
worker pool」——**对已死的连接不成立**：detach 的 driver 不 park，而是在 `poll_timeout`
上空转。

泄漏有两条来源，**只堵一条没用**：

1. **握手失败**：`direct::upgrade` 的 `inbound`/`outbound` 在 `finish()` 之前有八个 `?`
   早退点。而拨号**会重试**（拨到非 webrtc-direct 的端口、certhash 不匹配、Noise 认证
   失败都很常见），泄漏按重试次数累积——这是主因。
2. **连接异常终止**：`StreamMuxer::poll_close` 只在 libp2p 走正常关闭流程时才被调到；
   对端掉线或 Swarm 直接丢弃连接时，muxer 是**直接被 drop** 的。

**修法**：`backend/native/managed.rs` 的 `ManagedPeerConnection` —— 一个 RAII 守卫，
`Deref` 到 `Arc<dyn PeerConnection>`，drop 时把 `close()` 派给 runtime。握手路径、打洞
backend 的 `State::Ready`、muxer 三处都用它持有连接，不变式收敛成一条：**只要连接被
`ManagedPeerConnection` 持有，它就一定会被关闭**。所有权移交用 `into_inner()` 解除守卫，
否则守卫会在函数返回时把刚建好的连接关掉。

**这条与 wasm 侧的 `Drop` 是同一个道理的两面**（见 `backend::wasm::muxer`），但后果不
对称：浏览器那边泄漏的是浏览器自己管的对象，native 这边泄漏的是**本进程的 CPU**。
所以 native 侧漏掉它的代价大得多，而它恰恰是后加的。

### 以本机为中转的 circuit 地址：拨不通、拨了还会挤掉真候选（2026-08-06）

**症状**：桌面向浏览器发 offer 反复失败，日志里是
`Dial error: Unexpected peer ID 12D3KooWRkj1…`（那串正是**桌面自己**）。

**成因**：浏览器与桌面同网直连后，向桌面申请了 circuit reservation（桌面是开着
relay server 的）。于是浏览器的可达地址里多出
`…/p2p/<桌面>/p2p-circuit/p2p/<浏览器>`——**桌面自己的每条 listener 地址各一条**
（tcp / quic / webrtc-direct 三份），再原样广告回桌面。桌面拿它去拨，第一跳拨的就是自己。

这类地址**永远拨不通，也永远不需要拨**：本机能当对端的中转，前提就是两者之间已经有一条
连接。留着它只会挤掉同批候选里真正可用的那些（relay hint 还有 `MAX_RELAY_HINTS` 上限，
自指那条会把名额占掉）。

**两处修复，各管一层**：

- `crates/net` 的 `record_addr`（**地址进簿的唯一入口**，四条路径 `AddAddrs` /
  `AddInfraPeer` / `Connect` 显式候选 / mDNS 的共同下游）丢弃 `is_relayed_by(addr, self)`
  的地址。判据落在**中转跳**而非末位：末位是自己时 libp2p 自己就拒
  （`DialError::LocalPeerId`），漏的正是中间这一跳。
- `crates/core` 的 `presence::supervisor::spawn_probe` 跳过 `hint.peer_id == 本机` 的
  relay hint——否则每轮固定浪费一次「先连 relay 再拨 circuit」，而第一步就是拨自己。

  ⚠️ **筛必须在 `take(MAX_RELAY_HINTS)` 之前**。第一版写成了循环体里 `continue`，于是
  自指的那条照样占掉三个名额之一——「真候选被挤掉」这个正要修的症状原样保留，只是换了个
  地方发生。同一处 `hint.addrs.is_empty()` 的跳过是**存量的同类错误**，一并挪进 `filter`。
  判据：`take(N)` 里的 N 表达的是「试几次」，那就只能截在**可试的**条目上。

**⚠️ 这条过滤的日志必须是 `trace` 不能是 `debug`。** 默认 filter 就是
`swarmdrop_net=debug`，而对端反复断连重连时它是**每秒上万条**的量级：实测一次 11 分钟的
重连风暴写了 **640 MB** 日志，99.9% 是这一条。

**顺带暴露的、尚未查清的问题**：那 640 MB 说明有个上游循环在以 ~4000 次/秒 的频率重放
同一批地址（每 250µs 一次，四条一组）。加过滤之前它完全静默（`entry.contains` 去重后什么
都不做），是这条日志第一次让它可见。过滤让它不再产生失败拨号，但**调用本身还在烧 CPU**，
触发点是「ping 连续失败 → 主动断连重探」。要查它请开 `swarmdrop_net=trace` 并从
`record_addr` 的四个调用方入手。

### 双层 circuit 地址：`listen_on` 当场拒，而错误在日志里是**空的**（2026-08-10）

上一条的同族问题，中转跳换成第三方就漏过来了。真机日志几十条刷屏、`error` 字段全是空：

```
relay circuit listen failed relay_addr=/ip4/192.168.50.105/udp/4001/quic-v1/p2p/12D3KooWCkaj…/p2p-circuit/p2p/12D3KooWMSUf…/p2p-circuit error=
```

**三件事叠在一起，任何一件单独发生都不难查：**

1. **地址来源**：`request_relay_reservation` 取 `address_book` 的 `first()` 当 circuit 基址，
   而簿里可以合法地混着「经第三方中转到该 relay」的地址——上一条的 `record_addr` 过滤只丢
   **中转跳是本机**的，中转跳是别人时按设计放行。撞上那条，`circuit_base` 见到已有 `P2p`
   就直接追加 `/p2p-circuit`，拼出双层。
2. **错误看不见**：libp2p relay client 以 `MultipleCircuitRelayProtocolsUnsupported` 拒收，
   这个判别码落在 `TransportError::Other` 上，而
   **`TransportError` 的 Display 对 `Other` 分支写的是空串**（`core/src/transport.rs`）。
   `%e` 因此渲染出一条没有错误内容的 warn。⚠️ **凡是 `listen_on` 的错误一律用 `?e` /
   `{e:?}`**——同一个坑当时在 `crates/net` 里有三处（reservation listen、`/webrtc` listen、
   `builder.rs` 的 `BindError::Listen { reason }`，最后那处会让 bind 失败**整句话没有原因**）。
3. **无退避**：`listen_on` 是**同步**失败，上层 `InfraSupervisor` 的 2s→75s 退避又被 mDNS
   刷新候选（`candidate_seen` 重置）反复清零，于是同一条注定失败的地址被秒级重放。

**修法（各管一层，缺一不可）：**

- `circuit_base` 改成返回 `Option`，输入已含 `/p2p-circuit` 时给 `None`——**判在拼装处，
  不留给 `listen_on` 去拒**。三个调用点（reservation listen / `/webrtc` listen / `circuit_addr_for`）
  统一走 `circuit_base_for`：取地址簿里第一条**能当基址**的，不是第一条。
  护栏测试 `circuit_base_never_nests` / `circuit_base_skips_circuit_entries_in_address_book`。
- `Actor::relay_retry` 是**同步失败**的重试闸门（档位与 `InfraSupervisor::rebuild_backoff`
  同一套）。它不是重试策略——策略仍在 core；它挡的是「输入没变、答案必然相同」的重放。
  地址簿真有新条目时整条清掉（新事实到达不必等退避），意图撤销时也清掉。
  ⚠️ 闸门短路时**必须把状态压回 `Failed`**：`ensure_relay` 在调它之前刚翻成 `Connecting`，
  不压回去，退避期内 UI 会一直显示「正在连接…」却给不出原因。

### DataChannel 的 `Connecting` 不是错误（wasm 侧）

`PeerConnection` 的 `connected`（DTLS 完成）**早于** DataChannel 的 `open`（SCTP 完成）。
muxer 一交出去上层就开始写，此刻必然还是 `Connecting`——把它当写错误会让刚建立的打洞连接
立刻刷屏报错，实际只差几十毫秒。正确做法是注册 waker 返回 `Pending`，并配 `onopen` 回调
唤醒（**没有 onopen 就永远等不到通知**）。native 侧无此问题：`webrtc-rs` 的 `send` 是
async，内部等 SCTP。

### ⚠️ JS 回调闭包：释放之前**必须先解绑**，唤醒**必须延后**（2026-08-04 修）

症状是浏览器 console 刷屏：

```
Uncaught Error: closure invoked recursively or after being dropped
    at RTCDataChannel.i (...)
```

它是 **JS 侧的 Uncaught，不会 panic 掉 wasm**——节点照常跑（relay reservation 也照样
accepted），所以极容易被当成网络问题查半天。真正发生的事：

1. libp2p 子流被 drop（日志里那句 `Stream dropped without graceful close, sending Reset`）；
2. `DropListener` 发完 Reset flag，最后一个 `PollDataChannel` 副本释放，wasm-bindgen
   回收 `Closure`；
3. 但 **JS 侧 `RTCDataChannel.onmessage` 还指着它**——浏览器事件队列里排着的、或对端随后
   发来的消息照样触发，于是抛错并丢消息。

**三条一起才算修好**（缺任何一条都会以不同形式复发）：

| # | 约束 | 漏掉的后果 |
|---|---|---|
| 1 | 闭包要活到目标对象不再产生事件 | 回调**静默**失效，无任何报错 |
| 2 | 释放之前先 `set_onxxx(None)` | 就是上面那个刷屏 |
| 3 | 唤醒 waker 要延后到回调栈之外（`spawn_local`），且**先释放 `RefCell` 借用** | 重入 executor：轻则同一个报错，重则 `already borrowed` panic |

`crates/webrtc-p2p/src/backend/wasm/callbacks.rs` 的 `JsCallbacks<T>` 把 1、2 收口成类型
保证；3 是 `data_channel.rs` 的 `defer_wake`。`crates/web` 侧同一条不变量由
`crates/web/src/js_guard.rs` 的 `JsGuard<T, C>` 承担（IndexedDB 的三个 handler 站点 +
`AbortSignal` 的监听器）。

两处都把 `detach` 做成**参数**而不是 target 类型上的 trait：解绑动作写在注册点旁边，
加一个 handler 时两行相邻。曾写成 trait（一份「这个 JS 类型的全部 handler」清单），
但那份清单在另一个文件里，注册点加了 handler 却忘记回来补清单，正是它要防的失效模式。

**这是同一个错误犯第二次。** 官方 `webrtc-websys` 里这三条正是本仓提的
[#6558](https://github.com/libp2p/rust-libp2p/pull/6558)（见上文 fork 台账），
2026-07-28 自研 `crates/webrtc-p2p` 重写 wasm 后端时只带过来第 1 条。
**以后凡是「自研替换掉一个曾打过补丁的上游实现」，先把那些补丁逐条对照过来**——
补丁在 fork 里躺着，不会自己跟着走。

> **同族的第三种失效方式：补丁还在原地，产物里却没有它。** 移动端 2026-08-10 实证：
> 一份 `pnpm patch` 打在 `expo-file-system` 上，`pnpm install` 全绿、`node_modules` 里的
> Kotlin 确实是打过补丁的，Android 构建吃的却是预编译 AAR ⇒ **补丁三次都没参与编译**，
> 还被反推出一条错误的「架构事实」。判据是**编译产物里的符号**（`javap`），不是源码。
> 见 [toolchain.md](toolchain.md) 的「pnpm patch 打在有预编译产物的原生依赖上会静默失效」。
> 三条放在一起是同一句话：**改动看起来在，不等于它进了产物。**

同批漏抄的还有**累计读缓冲上限**（#6560 的 receive buffer 部分）：`onmessage` 在 Rust task
再次 poll 之前能攒下多条各自合法的消息，没有上限就是浏览器 OOM，且它**不能拿单条消息
上限来代替**（连续合法的 8 KiB 消息会被误判成对端过载）。

⚠️ **但也不能照抄官方的 256 KiB**——那个值比本端自己的发送高水位
`MAX_BUFFERED_AMOUNT`（1 MiB）还小。对端可以合法地让 1 MiB 在途，而浏览器接收侧每读满
一个应用块（`CHUNK_SIZE` 也是 256 KiB）就要 await OPFS 落盘 + bao 校验，那一下停顿就够
越限——**正常的快速传输会被判成「对端过载」并重置子流**。现取
`max(4 MiB, max_message_size)`。定这类阈值时先看一眼对面允许多少在途。

##### 两种终态错误相对读缓冲的顺序**相反**，不能合并成一个字段

`errored`（`onerror`）与 `overloaded`（读缓冲越限）看着都是「通道废了」，一度合并成一个
`fatal`。但：

- **越限**丢过消息，缓冲里是**有洞的**字节流，接着解帧只会解出垃圾 → 必须**先于**缓冲报错；
- **onerror** 一个字节都没丢，缓冲是完整合法的前缀 → 必须**后于**缓冲报错。

合并后症状是常态路径出问题：发送方一关连接（SCTP ABORT → 浏览器 `error` 事件），接收侧
就把已收到但没读完的尾部连同 FIN flag 一起丢掉，**正常收尾被报成流被 reset**。
`poll_read` 的判定顺序现在是 `overloaded` → 缓冲 → `eof` → `errored`，四者顺序都是语义。

### ⚠️ `RTCPeerConnection` 必须显式 `close()`，drop 掉句柄不算数

浏览器不会回收一条还在跑 ICE/DTLS 的连接——Rust 侧 drop 只是撤掉一个引用。官方
`webrtc-websys` 的 `Connection` 为此有 `impl Drop`；自研版一度只在 `Muxer::poll_close`
里关，于是三条路径漏关、每次失败/异常终止都攒一条僵尸连接：

| 路径 | 谁负责关 |
|---|---|
| 连接正常存活期 | `muxer::Inner` 的 `Drop`（数据面是最后持有者，异常终止时 libp2p 不会调 `poll_close`） |
| 打洞信令失败 | `backend::wasm::Inner` 的 `Drop`，判据是 `Handover` 还在不在（已移交 = 连接归数据面，绝不能关） |
| direct 建连失败**或被取消** | `direct::PendingConnection` 的 `Drop`，成功时 `disarm()` |

最后一行是 RAII 而不是错误分支，这点**不能省成 `inspect_err`**：`Transport::dial` 返回的
`Upgrade` future 会被 libp2p 的 `ConcurrentDial` **直接丢弃**（同时拨的另一个地址先成功，
浏览器拨一个通告了多个 webrtc-direct 地址的 peer 就会走到），那条路径上错误分支根本不执行。
取消、`?` 早退、panic 三条路只有 RAII 一起覆盖得住。

第二条要特别小心：`WasmBackend` 的寿命**只到信令会话结束**（`Action::Connected` 后
`connection_keep_alive()` 立刻转 false，那条 relay 上的信令连接被关，handler → session →
backend 顺次 drop），而数据面还要活很久。同一个时序此前已经埋了一个更隐蔽的 bug：
`ondatachannel` 闭包连同它捕获的 `dc_tx` 留在 backend 里，backend 一死，muxer 的
`incoming` 立刻结束 → `poll_inbound` 报「连接已关闭」→ **刚建好的打洞连接自己塌了**。
现在回调随 `take_muxer` 一并移交，与数据面同寿。

### `FuturesUnordered` 空集时不注册 waker —— DropListener 会被晾着

`while let Poll::Ready(Some(_)) = drop_listeners.poll_next_unpin(cx) {}` 这个写法有个洞：
集合为空时 `poll_next` 返回 `Ready(None)` 且**不注册 waker**，循环以它收尾，本轮的 `cx`
就白给了。之后 push 进去的 listener 要等连接因别的事情被唤醒才轮得到 poll。

在 wasm 侧这不只是「晚一点」：`DropListener` 持着 `PollDataChannel` 的一份 clone，它不被
poll 完，`Rc` 就不归零——**JS 回调迟迟不解绑、Reset 也迟迟不发**，连接安静下来（子流都关完、
没有 identify/ping 活动）时尤其明显。故 `Ready(None)` 要自己把 waker 存下，push 之后主动
`wake()`（官方 `webrtc-websys` 的 `no_drop_listeners_waker` 同款）。

### webrtc-p2p 的 `Transport::listen_on` 必须唤醒 poll

`listen_on` / `remove_listener` 是外部**同步**调用，往 `pending` 队列塞事件时没有任何东西
会唤醒 poll（只有 `from_behaviour` 有消息才会）。少了这个唤醒，新监听地址要等到下一次因
别的原因被 poll 才通告得出去。故 poll 挂起时存 waker，`queue()` 时唤醒。

**本机 `/p2p` 段要自己补**——swarm 不代劳。relay client 也是自己补的
（`priv_client/handler.rs` 的 `.with(P2pCircuit).with(P2p(local_peer_id))`）；漏了地址仍能
listen 成功，但对端解析不出目标节点，拨不动。

### `classify_path` 对 `/webrtc` 的例外

打洞连接的远端地址天生带 circuit 段（**信令**确实经 relay），但数据面是直连、一个字节不过
中继。`is_circuit()` 会把它判成 `Relayed`，于是 `path_rank` 把真直连排到中转之下、UI 显示
也反了。`actor.rs` 的 `classify_path` 因此对含 `/webrtc` 的 circuit 地址返回 `Direct`
（`is_hole_punched`），单测钉死。

### 「是不是打洞」这一位归 `PathKind`，产品层不许反推（2026-08-12 修）

`PathKind` 曾只有三档，`Direct` 一档里同时躺着**打洞建立的直连**与**直接拨通对端地址的
直连**（公网 IP、Tailscale 之类的 mesh VPN 隧道）。而 `crates/core` 的
`path_to_connection` 把这一档一对一映射成 `ConnectionType::Dcutr`（DCUtR = 打洞），于是
**任何非私网非中继的连接在三端 UI 上都写着「打洞」**。真机形态：一条
`/ip4/100.112.160.47/udp/62829/quic-v1/webtransport/…` 的 Tailscale 直拨，徽标显示「打洞」，
点开的链路详情里写着 `WebTransport`。

现在 `PathKind` 有四档，`HolePunched` 单列，`classify_path` 里那个**本来就存在**的
`is_hole_punched` 分支直接返回它，`path_to_connection` 退回纯一对一。

#### ⚠️ 中途走过的弯路：用「传输是不是 WebRTC」反推打洞 —— 别再走回去

第一版修法是在 `path_to_connection` 里判 `TransportKind == Webrtc`。它当天是准的，
但**错在两处**：

1. **它依赖一条无人看守的谓词等价关系。** `is_hole_punched`（`net`）与
   `Addr::transport() == Webrtc`（`net-base`）今天恒等，但它们问的不是同一件事——前者问
   「这条链路是不是穿透来的」，后者问「字节跑在哪种传输上」。旁边还有个语义更宽的同名
   `Addr::is_webrtc`（含 `WebRTCDirect`）等着被人「统一」进来。真漂移了两侧都编得过、
   各自的测试都绿，只有 UI 自相矛盾。
2. **它看不见 libp2p 自己的 DCUtR。** `presets.rs` 的 `Native` 写着 `.dcutr(true)`，
   `runtime.rs` 的注释明说两套并存、「dcutr 走 TCP/QUIC 直连，ICE 走 UDP + STUN 候选，
   覆盖的 NAT 类型不同、互为补充」。一次成功的 DCUtR 打洞产出的是普通
   `/ip4/<公网>/udp/<port>/quic-v1`，按传输反推会把**真打洞判成「压根没打洞」**——
   与修复前的错误方向正好相反。

所以「**打洞只可能跑在 WebRTC 上**」这句话是**错的**，不要写进任何文档或注释（它一度进过
三处 rustdoc 与 DESIGN.md）。判据的唯一归属是内核的 `PathKind`。

#### 已知缺口：libp2p DCUtR 的打洞目前仍归 `Direct`

`classify_path` 只认地址（circuit 段之后有没有 `/webrtc`），而 DCUtR 的产物在地址上认不
出来。要认出它得接 `dcutr::Event::DirectConnectionUpgradeSucceeded` —— actor 目前把它落进
`other => debug!("behaviour event")`。这是**缺口不是判据**：修它是给那个事件加一个分支、
把对应连接标成 `HolePunched`，而不是回到按传输反推。

#### 其余不变量

- **`WebrtcDirect` 归 `Direct` 不归 `Dcutr`** —— 它拨裸 IP、免信令免穿透，与 `/webrtc`
  是两个传输（DESIGN.md 明令不得合并）。
- **`path_rank` 里 `Direct` 与 `HolePunched` 同分** —— 数据面质量相同，分档只为说清来路；
  给谁加分都会让 `best_conn` 在两条等价连接之间偏心。
- `infer_connection_type`（断连宽限期的地址回退推断）**永远推不出 `Dcutr`**，这是判据决定
  的不是疏漏：打洞地址含 circuit 段，在第一个分支就归了中继。要认出它得看「最后一个 circuit
  段之后还有没有传输段」（`dial_tier` 那套），而回退推断不值得重造一份易错的地址解析。

### Tailscale 的 `100.64.0.0/10` 在三个谓词里两否一是

`is_private_lan()` **false**（只认 RFC1918 + IPv6 ULA）、`is_public_routable()` **false**
（显式排除共享地址空间）、`is_shared_address_space()` **true**。漏掉第三条的地方，隧道地址
会**一档都不占**然后静默消失 —— `infer_connection_type` 此前就是这样，宽限期内 Tailscale
连接的徽标凭空不见。写任何「按地址性质分类」的分支时三条一起判。

#### ⚠️ 这只覆盖 IPv4 一半：Tailscale 的 IPv6 仍会被判成「局域网」

Tailscale 的 v6 段 `fd7a:115c:a1e0::/48` 落在 ULA（`fc00::/7`）里，`is_private_lan()` 对它
**为真** —— 于是同一条隧道走 v6 时 UI 显示「局域网」，与「隧道归 `Direct`」这条契约相反。

**没有跟着修，是因为 IPv4 那半有通用判据而 v6 那半没有**：`100.64/10` 是 CGNAT 标准段，
「不是真 LAN」这件事与用哪家 VPN 无关；而 ULA 本来就是给任何私有网络用的合法地址，
Tailscale 只是自选了其中一个 /48，从地址上无法与真局域网区分。要认它只能把那个产品前缀
写死进内核。

**并且改这条不只是改一个徽标**：`is_private_lan` 是 `is_lan_discovered` 的输入，而后者是
`PairingMethod::Direct` 的**唯一授权依据**（该模式没有配对码）。把 Tailscale v6 移出 LAN
等于同时改掉「谁能免码配对」，那是安全判据，要单独评估——可能是想要的（隧道确实不是同一
广播域），也可能打断现有用法。故留作待评估项，不夹带在一次 UI 修复里。

⚠️ 另一处**尚未处理**的后果：`dial_tier` 只看传输段不看 IP，所以隧道上的 WebTransport 和
真 LAN 的 WebTransport 同为 `DirectFast`。一旦连接落在隧道那条，`lan_candidates` 的
`*tier < current` 恒 false、`wants_upgrade_to(_, DirectSlow)` 也 false —— **LAN 升级与打洞
双双再也不会发起**，且 `prune_inferior_conns` 会把其它路径主动关掉。实测同网 20 MB/s 的
链路因此停在 3 MB/s。修它要给 `DialTier` 加一档，而那一档排在 `DirectSlow` 前还是后取决于
「Tailscale 走的是 direct 还是 DERP」——SwarmDrop 看不见这个状态，需要实测定档，故未动。

### 移动端加 `ConnectionType` 变体不会编译报错，另两端会

桌面与 Web 吃 specta/wasm-bindgen 生成的联合类型，`Record<ConnectionType, …>` 缺 key 当场
编译失败；移动端隔着 uniffi 的**字符串**，`normalizeConnectionKind` 的 `default` 分支把未知
值收成 `null`，表现是那种连接的设备卡上**整枚徽标消失**，静默。加变体时同改
`mobile-core/src/device.rs` 的字符串化、`CONNECTION_META` 与那个 `switch`。

### `OptionalTransport` 只有 `From<T>`，没有 `From<Option<T>>`

`OptionalTransport::from(opt)` 会包成 `OptionalTransport<Option<_>>`——那不是 `Transport`，
报错信息绕。用 `match { Some(t) => ::some(t), None => ::none() }`。另外
`with_other_transport` 闭包里没有 `?` 时错误类型推断不出来，要显式标注成
`Box<dyn Error + Send + Sync>`（`TryIntoTransport` 唯一认的 Result 形态）。

**相关文件**：`crates/net/src/{transport.rs,actor.rs,config.rs,behaviour/mod.rs}`、
`crates/core/src/runtime.rs`；决策与 spike 实测见
[`dev-notes/research/2026-07-webrtc-native-ice.md`](../research/2026-07-webrtc-native-ice.md)

## webrtc-direct 自研实现（`crates/webrtc-p2p`，2026-07-28）

同一个 crate 现在提供**两种模式**：打洞（上一节）与 direct。目标是完全替代官方
`libp2p-webrtc`，把 native 依赖树里的两套 WebRTC 栈（0.17 + 0.20）并成一套。

两条建连路径**刻意不复用**——差异全在安全语义上（谁的指纹可信、要不要 Noise、
ICE 是否 lite），摊平成一个带 role 参数的函数极易改错一边。分派点在
`swarm/transport.rs`，按 multiaddr 的协议段判别（`/webrtc` vs `/webrtc-direct`），
两个方向的反向断言由 `dispatches_by_address_family` 钉死。

端到端证据：`crates/webrtc-p2p/tests/direct_loopback.rs`（真绑端口、真 ICE-lite +
DTLS + SCTP、真 Noise、真开子流传字节，且验证一个端口服务多条连接）。

### ⚠️ rtc 0.20 的 `disable_certificate_fingerprint_verification` 是死代码

**这是 direct 服务端的阻塞项，已 pin fork 绕过。**

该 setting 有字段、有 setter，但**从未被传给 `RTCDtlsTransport`**——`start()` 一律装上
指纹校验回调。旁边的 `allow_insecure_verification_algorithm` 走完全相同的路径且接线
完整（`SettingEngine` → `internal.rs` → `RTCDtlsTransport::new` → `ConfigBuilder`），
一对比就能看出是 sans-io 重构时漏接的一环。

direct 的**服务端必须能关掉它**：它收不到真 offer，只能本地合成一份、`a=fingerprint`
填占位值（`Fingerprint::FF`），身份改由 DataChannel 之上的 Noise 握手认证（spec FAQ
第一条）。开关失效时 DTLS 在 Flight 4 报 `ErrNoMatchingCertificateFingerprint`。

修复见 <https://github.com/webrtc-rs/rtc/pull/137>，**已随 rtc 0.20.0 正式版发布**
（2026-07-31），本仓的 fork pin 已删除。这也是 `rtc` / `webrtc` **不得降回
`0.20.0-rc.*`** 的原因之一：rc 版里这个开关仍是死代码，direct 监听端直接起不来。

### 上游缺口台账（2026-07-28，pin 状态更新于 2026-08-04）

做 direct 期间踩到的上游问题都已提出去。**关键区分：只有下表标「阻塞」的两项影响本仓的
依赖 pin**，其余是反哺与待改进——看到「5 个上游 PR」不要以为退出条件从 3 个变成 5 个。

> **webrtc-rs 那五条已全部出清**（下表标「已 pin」/「阻塞」的 rtc·webrtc 条目）：
> 补丁 2026-07-29 合并进上游 master，2026-07-31 随 **0.20.0 正式版**发到 crates.io，
> 本仓的两条 `[patch.crates-io]` 于 2026-08-04 整段删除。表里的「已 pin」现在读作
> 「已在 0.20.0 里」。
>
> ⚠️ **当天下午两条 pin 又加回来了，但与这五个补丁无关**——是为了等
> [#853](https://github.com/webrtc-rs/webrtc/pull/853) 公开两个 helper，见下面
> 「webrtc 的 fork pin：删掉又加回来」。别把它读成「五个补丁又退回去了」。

| 仓 | 编号 | 内容 | 对本仓的意义 |
|---|---|---|---|
| webrtc-rs/rtc | [PR 137](https://github.com/webrtc-rs/rtc/pull/137) | `disable_certificate_fingerprint_verification` 是死代码 | **阻塞** — direct 服务端没它建不起来 |
| libp2p/rust-libp2p | [PR 6472](https://github.com/libp2p/rust-libp2p/pull/6472)（上游自己的） | relay circuit 无 reservation 时 panic | **阻塞** — 与 #6558/#6560 同属 git pin 退出条件 |
| webrtc-rs/**rtc** | [PR 140](https://github.com/webrtc-rs/rtc/pull/140) | `RTCDataChannelInit` 的 `ordered` 默认成 `false`（issue 139） | 反哺 — 本仓已在自己这侧显式传参，不进 pin |
| webrtc-rs/webrtc | [PR 825](https://github.com/webrtc-rs/webrtc/pull/825) | `on_data_channel` 把本端开的通道也报上来 | **已 pin**（见下）；muxer 的 `local_channels` 仍保留，它是不变式不是补丁 |
| webrtc-rs/**rtc** | [PR 138](https://github.com/webrtc-rs/rtc/pull/138) | `send()` 在通道 open 前/关闭后返回 `Ok` 但**静默丢数据**（issue 826） | **已 pin**；`data_channel::await_open` **无论如何都要留** |
| webrtc-rs/webrtc | [PR 828](https://github.com/webrtc-rs/webrtc/pull/828) | 加 `remote_certificate_fingerprint`（issue 827） | **已 pin**，`remote_fingerprint()` 收成一行 |
| webrtc-rs/webrtc | [PR 850](https://github.com/webrtc-rs/webrtc/pull/850) → [853](https://github.com/webrtc-rs/webrtc/pull/853) | `gro_recv_buf_len` / `is_retryable_socket_recv_error` 是 `pub(crate)`，实现自定义 `AsyncUdpSocket` 只能照源码抄 | **已拒绝（2026-08-04）** — driver 策略不属 socket 契约，`pub` 会冻结内部分配策略。上游改为把规则写进公开文档，本仓按文档在 `udp_mux.rs` 自持一份。两条 pin 随之删除 |
| libp2p/rust-libp2p | [PR 6571](https://github.com/libp2p/rust-libp2p/pull/6571) | `Fingerprint::from_sdp_format` | 纯反哺 — 合并后 `protocol/addr.rs` 的手写解析可删 |
| libp2p/rust-libp2p | [PR 6572](https://github.com/libp2p/rust-libp2p/pull/6572) | offer SDP 模板搬进 `libp2p-webrtc-utils` | 纯反哺 — 合并后 `native/direct/sdp.rs` 的模板副本可删 |

> #6571 / #6572 **基于上游 master 开分支，不在 fork 树上**——它们不进 `Cargo.toml` 的 pin，
> 也不进退出条件。它们合并只是让本仓能删掉两处副本。

> **我们的 relay panic PR #6570 已于 2026-07-28 关闭。** 维护者指出上游早有
> [#6472](https://github.com/libp2p/rust-libp2p/pull/6472)，实测两者**源码逐字节相同**
> （同一段 `find(|(_, status)| status.is_active())`），只有测试写法不同。
> ⚠️ **这不缩短退出条件**：#6472 至今仍是 OPEN，那一行只是从「我们的 PR」换成
> 「上游的 PR」，fork 上的补丁照旧要留着。

⚠️ **issue 826 是本仓踩过最贵的一个坑**：Noise 握手第一条消息在
`RTCPeerConnectionState::Connected` 时写出去就消失了，全链路零报错，表现为「握手莫名挂住」。
实测数据在 issue 正文里（三条消息只到一条）。修法是发首包前等 `OnOpen`。

根因后来钉死在 **rtc**（不在 webrtc）：`DataChannelHandler::handle_write` 确实返回了
`ErrDataChannelNotExisted`，但它跑在 pipeline 的 write pass 上，那里的错误只 `warn!` 不上抛：

```
send result: Ok(())
[WARN rtc::peer_connection::handler] DataChannelHandler.handle_write got error: data channel not existed
```

修复见 [rtc#138](https://github.com/webrtc-rs/rtc/pull/138)（把判据搬到 send 边界，
用与 handler 完全相同的条件，于是两者不可能不一致）。顺带查出两件 issue 里没写的：
**通道关闭后 send 同样返回 `Ok`**，且被拒的 send 还会错误累加 `outstanding_bytes`
——那些字节从没进过 SCTP，永远不会被释放，等于把发送窗口永久缩小一截。

> **就算 rtc#138 合并了，`await_open` 也不能删。** 它把「静默丢」变成「明确报错」，
> 不代表可以不等——发之前仍然必须等通道 open。

#### direct 的三个指纹从哪来——issue 827 只砸中其中一个

Noise prologue 绑定**双方**指纹（`libp2p-webrtc-noise:<client><server>`，
`libp2p-webrtc-utils/src/noise.rs`），两端算不出同一个 prologue 就握手失败。三个取值点：

| 谁要谁的 | 来源 | 位置 |
|---|---|---|
| 拨号端要**服务端**的 | multiaddr 的 certhash（参数传入） | 两端皆是，spec 设计 |
| wasm 拨号端要**自己**的 | `localDescription` 的 `a=fingerprint:` | `wasm/direct.rs::local_fingerprint` |
| **服务端要拨号端的** | `get_stats` 的 certificate 项 | `native/direct/upgrade.rs::remote_fingerprint` |

前两行不是绕法：certhash 本来就该从地址来；浏览器不给你直接读自己的证书指纹，
解析 `localDescription` 是唯一途径。**只有第三行是 issue 827 逼出来的**——
服务端在 direct 模式下收不到真 offer（自己合成、填 `Fingerprint::FF`），
拨号端的指纹只存在于 DTLS 握手里，官方 0.17 用的正是 `get_remote_certificate()`。

那一处曾靠 `cert.stats.id.starts_with("remote-certificate-")` 认远端项（id 前缀是 rtc 的
实现细节，无文档承诺稳定）。现已换成上游 API [`PR 828`](https://github.com/webrtc-rs/webrtc/pull/828)
的 `remote_certificate_fingerprint()`——库内部用 `Transport.remote_certificate_id` 反查，
不再赌前缀。

两道兜底都在：`tests/direct_loopback.rs` 跑真握手且断言 `accepted_peer == client_peer`
（指纹取错 prologue 就对不上，Noise 当场失败）；上游那侧的测试则交叉验证
「一端看到的 remote == 另一端的 local」，防的是取反了还静默通过——
拿本端指纹去做 pin 校验等于自己跟自己比，会接受任何 peer。

### webrtc 的 fork pin：删掉又加回来（2026-08-04）

时间线值得记清楚，因为**两次 pin 的理由完全不同**：

| 时间 | 状态 | 理由 |
|---|---|---|
| 2026-07-28 | pin fork | 五个功能补丁未合并 |
| 2026-08-04 上午 | **删除** | 补丁随 0.20.0 正式版进 crates.io |
| 2026-08-04 下午 | **重新 pin** | 等 [#850](https://github.com/webrtc-rs/webrtc/pull/850) 公开两个 helper |
| 2026-08-04 晚 | pin 不变，**PR 改投** | 维护者要求投 `v0.20.x`（无 breaking change，合并后他自行 merge 回 master）→ 重开为 [#853](https://github.com/webrtc-rs/webrtc/pull/853)，#850 CLOSED。集成分支**没动**（那条绝不 force-push），故 `Cargo.toml` 的 rev 不变 |
| 2026-08-04 深夜 | **再次删除，回 crates.io 0.20.0** | 上游拒绝公开那两个 helper（见下），pin 失去唯一理由 |

**终局：两条 pin 已删，webrtc / rtc 都走 crates.io `0.20.0`。** 下面那段「等 API」的推理
保留，因为结论被推翻的**方式**本身值得记：我们要的不是补丁而是「把内部函数提为 `pub`」，
而这类请求的成败取决于**它是否属于对方的公开契约**，与它对下游多有用无关。维护者的划界是：
`gro_recv_buf_len` / `is_retryable_socket_recv_error` 都由 **driver** 消费，而不是由
`AsyncUdpSocket` 的**实现者**消费，所以它们是 driver 策略、不是 socket 契约的一部分；
设成 `pub` 就等于把内部分配策略冻进 1.0 的兼容承诺。

**但他接住了真实需求**：指出我们这种「一个 UDP socket 多路复用给多个 PeerConnection」的用法
其实是在扮演 driver，本来就该自己拥有缓冲尺寸与错误分类；同时把两条规则补进了**公开文档**
（commit `ef8ba660`）——`poll_recv` 的 `# Errors` 列出五个 transient 变体，
`max_gro_segments` 的 `# Buffer sizing` 写死「合并段受路径 MTU 约束而非应用最大数据报」，
并点名「跨连接多路复用的共享 socket 要在自己的循环里负责同样的 MTU 上界」。

于是本仓按文档在 `udp_mux.rs` 自持一份（**不是照源码抄**），护栏是那个文件里的两条测试。
这比 pin 一个 fork 更可持续：文档是契约，源码不是。

第二次不是等修复，是**等一个新公开的 API**。`gro_recv_buf_len`（GRO 缓冲尺寸公式）
与 `is_retryable_socket_recv_error`（读错误分类）在上游是 `pub(crate)`，而
`crates/webrtc-p2p` 的 udp_mux 两个都要用。不 pin 就得在下游各抄一份，**而这两件事
都没有反馈回路**：缓冲算小了内核静默丢尾部段、判据漏一种就把公网监听端口永久关掉，
两者都不报错。本仓抄过一版，**两个都抄错了**（缓冲大 5.5 倍、错误集漏三个变体，
见上面两条坑）。宁可背一条 pin，也不要在下游维护这两份复制品。

- `rtc` → **官方** `webrtc-rs/rtc` 的 submodule commit `b47f82fe`，无自有补丁
- `webrtc` → fork 分支 `swarmdrop-integration-0.21` = 上游 master + #850 + 一行适配

⚠️ **两者都是未发布的 0.21.0**（crates.io 最高 0.20.0），故 `crates/webrtc-p2p` 的
版本号也写 0.21.0，由这两条 patch 提供。
⚠️ **`swarmdrop-integration-0.21` 不能 force-push**——commit 一游离就被 GC，构建当场断。
#850 若被要求改形态，**另开分支**重建，不要改写这条。

**同源约束**（两次 pin 都适用）：`webrtc` 与 `webrtc-p2p` 必须解析到**同一份** `rtc`，
否则两个 source id = 两个互不兼容的同名 crate，`webrtc` 返回的类型对不上
`use rtc::...` 的类型，直接编译失败。上游 master 用 `rtc = { version, path = "rtc" }`
指 submodule，而 **`[patch.crates-io]` 不作用于 path 依赖**，所以集成分支必须把 `path`
去掉——那就是「一行适配」，它**不能进上游**。

（发布到 crates.io 的 `webrtc 0.20.0` 声明的是 `rtc = "^0.20.0"`，发布流程自动剥掉了
`path`，所以走 crates.io 时天然同源、无需适配——这正是上午能删掉 pin 的前提。）

验证收敛的命令（应只有一行 rtc）：

```bash
cargo tree -p webrtc-p2p -i rtc
```

### webrtc 0.20 没有 UDPMux —— 改从 `Runtime::wrap_udp_socket` 注入

0.17 的 `UDPMux` / `UDPMuxWriter` / `UDPMuxConn` 体系在 0.20 整个消失
（`SettingEngine::set_udp_network` 在 rtc 里已是**注释掉的 TODO**）。

替代注入点更下层也更干净：`Runtime::wrap_udp_socket` 决定 `PeerConnection` 用哪个
socket。于是复用一个端口的做法变成「给每条连接发一个假 socket」——发包转给共享
socket，收包从自己的支路取。官方那 579 行 `udp_mux.rs` 里的 trait 适配层随之消失，
只剩真正的分流逻辑（约 250 行）。

**分流依据**：首包按 STUN `USERNAME` 里的 local ufrag（`<对端>:<本端>`，取**冒号前**
那一半），其余按源地址。

### ⚠️ 坑：udp_mux 必须自己拆 GRO —— 这是**旧代码一直漏做**的，不是升级引入的

2026-08-04 切到 crates.io 0.20.0 时，上游把 socket 原语从 `async fn recv_from` 换成了
quinn 风格的 `poll_recv(cx, bufs, meta)`（`recv_from` 降级成基于它的默认方法）。适配
过程中才发现 udp_mux 一直缺一件事：**按 `RecvMeta::stride` 拆开内核 GRO 合并的数据报**。

**因果别搞反**（这份文档里一度就记错了）：GRO 不是 0.20.0 才有的。旧 pin
（`webrtc@3d6391cd`）的 `wrap_udp_socket` 就已经调
`quinn_udp::UdpSocketState::new(...)`，那会在 socket 上**打开 UDP_GRO sockopt**；而
当时 `UdpMux` 读包走 `recv_from`，底层是裸的 `tokio::UdpSocket::recv_from`，
**不解析承载 stride 的 cmsg**。sockopt 开着，内核照样合并——只是 stride 信息被丢弃。

于是 **0.20.0 之前的 Linux 构建（含已发布的 v0.10.4）在这条路径上是有缺陷的**：
同一对端连发的数据报会被当成一个巨包投给支路（DTLS 记录层校验失败 / SCTP 解析失败，
两者都静默丢弃），且缓冲只有 8 KiB，超出的尾部段被内核直接丢掉。表现为「偶发的、
无日志的丢包」，极难归因。macOS / Windows 无 GRO，所以本机开发永远看不到。

换成 poll 式 API 只是让这个约束**显式**了（`recv_from` 的文档明写「不要在可能发生
GRO 的地方用它」）。现在 `UdpMux::poll` 是：`poll_recv` 收一批 → 按 `stride` 切成单个
数据报入队 → 逐个 dispatch。

⚠️ **本机 loopback 通常不触发 GRO**，`direct_loopback.rs` 那几条真链路测试走不到这条
分支——覆盖它的是 `udp_mux.rs` 里 `split_datagrams` 的单测。改那段逻辑时别指望集成
测试会红。

同批还有两处纯签名变动：`Runtime::spawn` 返回 `Box<dyn JoinHandle>`（不再是具体类型），
`spawn_reactor` 多了 `reactor_pool_size: usize` 首参；`Runtime` 另新增
`resolve_host` / `sleep` / `interval` / `block_on` 四个必需方法，`MuxedRuntime` 一律转交
`inner`（这层垫片只替换 UDP socket）。

### ⚠️ 坑：瞬时读错误的判据漏一种，公网监听端口就会被远程掀掉

`UdpMux::poll` 收到读错误时要判断「是某个对端的事」还是「端口废了」——后者会顺着
`UdpMuxEvent::Error` 冒到 `Transport::poll`，那里 `listeners.remove()` + `ListenerClosed`，
**4003 端口就此消失，进程还活着但再没人连得进来，且没有重试路径**。

判据必须与 webrtc-rs 的 `is_retryable_socket_recv_error` 一致：
`Interrupted | WouldBlock | ConnectionRefused | ConnectionReset | TimedOut`。

**最容易漏的是 `ConnectionRefused`**：ICMP port unreachable 在 **Linux 上是它**，
Windows 上才是 `ConnectionReset`。只认后者等于在 Linux 上留了个远程可触发的开关——
随便哪个对端关掉进程，回来的 ICMP 就能掀掉整个监听端口。本仓一度就是这样
（2026-08-04 修）。

### 坑：`Transport::poll` 里的读循环必须有 burst 上限

`UdpMux::poll` 的读循环开在 swarm 的 poll 线程上。没有上限的话，公网 4003 上一股持续
流量（或一个刷包的扫描器）就能把 `Transport::poll` 永久留在里面，**节点其余所有传输、
连接、behaviour 一起饿死**——而这是个未认证的输入源。

修法与 webrtc-rs 的 `MAX_UDP_RECV_BURST` 一致：读满 64 轮就 `cx.waker().wake_by_ref()`
后返回 `Pending`。自唤醒不能省——`pending` 队列里可能还有货，否则要等下一个数据报
才会被处理。

另有一条相关契约：`poll_recv` 返回 **`Ok(0)` 意思是「什么都没准备好」，不是「收到 0 条
消息」**（上游 `runtime/primitives.rs` 明写）。当成后者会转出一轮没有 waker 的空循环。

### 坑：GRO 缓冲按**段长**算，不是按单包上限算

`gro_recv_buf_len` 是 `max_gro.min(64) * 1500`，无 GRO 时退化成 `UDP_RECV_BUF_LEN`
（2000）。拿 8192（单包上限）当段长去乘会把缓冲算大 5 倍多（64 段时 512 KiB vs
94 KiB）——GRO 的段不可能超过一个路径 MTU。另外 `max_gro_segments()` 是
`AsyncUdpSocket` 实现给的值，**不能直接当分配乘数**，必须 clamp。

> **这两条坑（缓冲尺寸 + 错误判据）现在都不用自己实现了。** 本仓一度各抄一份、
> 两份都抄错，于是提了 [#850](https://github.com/webrtc-rs/webrtc/pull/850) 把上游
> 那两个 `pub(crate)` 提为 `pub`，`udp_mux.rs` 改为直接
> `use webrtc::runtime::{gro_recv_buf_len, is_retryable_socket_recv_error}`。
> **不要再在本仓重新实现它们**——留这两条记录只为解释「为什么它们值得一条 pin」。

### ⚠️ udp_mux 的支路丢包在生产里曾经**完全隐形**（2026-08-10 修）

`deliver()` 里支路满就丢包（`BRANCH_CAPACITY = 256`，见那个常量的注释：一条慢支路
卡住整个端口不可接受）。这个设计本身没问题——UDP 允许丢包，DTLS 与 SCTP 各有重传。

**问题在于它只留了一句 `tracing::debug!`，而桌面与移动的默认 filter 够不着
`webrtc_p2p` 这个 target**（`EnvFilter` 按前缀匹配，`swarmdrop=debug` 覆盖不到它，
详见 [`rust-backend.md`](rust-backend.md) 的同名条目）。于是这条路径在生产日志里
一条都不出现。

**为什么它要紧**：持续丢包会把 SCTP 的拥塞窗口压塌，表现为「吞吐从第一秒起就恒定在
一个远低于链路能力的值上、但传输仍能完成」。2026-08-10 观测到的浏览器→桌面恒定
3.3 MB/s 正是这个形状——3.3 MB/s ÷ 1 ms LAN RTT ⇒ 等效 cwnd ≈ 3 个 MTU。
它既不是 CPU 瓶颈的形状（那会随负载波动），也不是带宽瓶颈的形状。

现在按 **2 的幂次**限频打 `warn!`（第 1、2、4、8… 次）并带累计数。选 2 的幂次而不是
定频（每 N 次一条）：真丢包时日志行数只有 log₂ 级、不刷屏，而**第一次丢包一定被记下来**
——定频报告会把「只丢了几个」这种更值得警惕的情形整个吞掉。

⚠️ **别急着调大 `BRANCH_CAPACITY` 或补 `SO_RCVBUF`**。诊断报告
（[`../research/2026-08-10-transfer-throughput-diagnosis.md`](../research/2026-08-10-transfer-throughput-diagnosis.md)
§1.3 / §4）把它们列为修复 #3/#4，但明确要求**先看丢包计数再改**：假设不成立的话，
改了不仅白改，还平白加 2.4 MB/连接的内存。

### ⚠️ SCTP 接收窗口**必须**从消息尺寸推导，配大了会静默丢数据（2026-08-11 修）

webrtc-rs 的 driver 把每条收到的 DataChannel 消息 `try_send` 进一条深度 **256** 的通道
（`DATA_CHANNEL_EVENT_CHANNEL_CAPACITY`，`pub(crate)` 未导出），**满了就直接丢**，
只打一行 ERROR（[webrtc#858](https://github.com/webrtc-rs/webrtc/issues/858)）。

那条队列在 **SCTP 之下**，所以 SCTP 的可靠性覆盖不到它：对端的数据确实送达、确实重组好了，
然后被扔掉。上层 libp2p 字节流中间少一段，**永远等不齐**。

**判据是硬的**：`SCTP 接收窗口 > 256 × max_message_size ⇒ 必然丢数据`。
本仓曾配 8 MiB，而 8 KiB 消息下队列只装得下 2 MiB —— 超了 4 倍。

**正确做法**：窗口从 `StreamConfig` 推导，两个模式各按自己的配置算
（direct 用 `ctx.stream_config`，打洞用 `StreamConfig::default()`）。

```rust
fn sctp_receive_buffer(stream_config: StreamConfig) -> u32 {
    DRIVER_EVENT_QUEUE_LEN.saturating_mul(stream_config.max_message_size() as u32)
}
```

**不要做**：
- 不要写死一个常量——它会与 driver 队列脱钩，而**没有任何编译期信号**。
  `sctp_receive_buffer` 那两条护栏测试是唯一的兜底。
- 不要以为「窗口越大吞吐越高」。旧注释写的是「1 MiB 默认会在 LAN 上掉线，spike 在 4 MiB
  观察到失败」，据此调到 8 MiB ——**因果反了**。而且 libp2p 默认 16 KiB 消息下队列恰好
  装 4 MiB，正是那个「观察到失败」的值。

**这条缓解不完备**：队列按**条数**限、窗口按**字节**限，对端发大量小消息仍能撑爆它。
之所以够用，是因为 libp2p 的 framing 会攒满 `max_data_size` 才 flush。**真正的修复在上游**
——webrtc master 与 0.21.0-alpha.1 改成「队列满时停止从 core 拉取」并把这个旋钮整个删了。
**crates.io 的 0.21.0-alpha.1 不能直升**，它缺 rtc#159/#161（driver 忙循环烧核）。
注意措辞：**不是「0.21 回退了修复」，是发布早于修复**——#154 bump 版本在 08-09 23:48，
而 #159 合于 08-10 23:38、#161 合于 08-11 01:51。两个仓的 master 现在都对了。

已在 `chore/webrtc-0.21` 分支验证过一条可行路径（fork 集成分支 + `[patch.crates-io]`
指 rtc master，三个修复一次拿全，门禁全绿、实测零丢），但那是独立的大改动，尚未合入。
退出条件与实测数据见
[`../research/2026-08-11-web-webrtc-throughput.md`](../research/2026-08-11-web-webrtc-throughput.md) §7.5。

**相关文件**：`crates/webrtc-p2p/src/backend/native/mod.rs`、
`crates/webrtc-p2p/src/backend/native/direct/upgrade.rs`、
`crates/net/examples/transport_throughput.rs`（三方 transport 对照基准）

### ⚠️ `set_send_high_water_mark` 是**下界**，不是 buffer 上界（2026-08-11 修）

上一条讲的是**接收**侧丢消息。这条是**发送**侧，症状一模一样（传输卡住、零报错），
根因完全不同，两条一起看才完整。

`asynchronous-codec` 的 `Framed`：

```rust
fn poll_ready(..) { while buffer.len() >= high_water_mark { flush } }  // ← 攒够才 flush
fn start_send(item) { encode(item, &mut buffer) }                      // ← 之后再追加一整条
```

于是 buffer 峰值 = `hwm - 1 + 一条完整帧`。而 `PollDataChannel::poll_write` 把**一次写出
变成一条 SCTP 消息**，长度一旦超过协商的 `max_message_size`，rtc 直接拒收整条
（`SctpHandler.handle_write got error: outbound packet larger than maximum message size`），
那一帧就没了，上层字节流永不重同步。

libp2p 原本写的是 `set_send_high_water_mark(config.max_data_size())`，注释还说这是为了
避免超限——**方向反了**。正确值是 **1**：buffer 只要非空就 flush，于是每条帧单独成一条
消息，既不会超限也符合 spec（每条 DataChannel 消息 = 一个 protobuf 帧）。已修，见
libp2p PR #6560 的第二个 commit。

**触发条件是混合帧尺寸**，这决定了测试怎么写：一串满尺寸帧**复现不出来**——每条都把
buffer 顶过水位线、当场 flush，什么都不剩。必须先来一条**短帧**（留下的 buffer 低于
水位线），再跟一条满尺寸帧，两者才会被一起写出。本仓实测（1 MiB / 8 KiB 上限）：
125 次写出 8190 B，**3 次 8419 B**，SCTP 恰好拒了那三条，接收端少 49,467 B。

**它被另一个 bug 掩盖了很久**：SDP 里 `a=max-message-size` 曾硬编码 16384，而本仓 framing
用 8 KiB，合并后的写出正好卡在 16384 以内。把声明改成跟随配置（同 PR 第一个 commit）之后
才炸出来。所以那两个 commit **必须一起进，只合前一个是净回归**——这也是「修一个 bug 前
先确认它没在掩盖另一个」的实例。

**判据**：`max_message_size` 的发送侧上限由 rtc 的
`SctpTransport::calc_message_size(remote_sdp_advertised, local_can_send)` 取 **min** 决定，
按它 resize `internal_buffer`，发送时判 `payload.len() > internal_buffer.len()`。也就是说
**对端 SDP 声明的值直接决定本端能发多大**。

### ⚠️ `webrtc` 与 `webrtc_p2p` 是**两个 target**，日志 filter 要分别放行

这是上面那条 udp_mux 教训的**第二次**：`webrtc_p2p` 是本仓的传输 crate，`webrtc` 是
webrtc-rs 自己。`EnvFilter` 按**字符串前缀**匹配，而 `"webrtc::…".starts_with("webrtc_p2p")`
为 **false** —— 于是 `webrtc_p2p=info` 够不着 webrtc-rs 的任何日志。

被挡住的正是上一条那个丢弃 ERROR。**回环实测：生产 filter 下丢掉 10 MiB，日志里零条记录。**

**正确做法**：两端的 `DEFAULT_FILTER` 都要带 `webrtc=warn`（取 warn 不取 info：只要告警，
不要每包 trace）。`EnvFilter` 取最长匹配，所以 `webrtc_p2p=info` 仍然更具体、不受影响。

**相关文件**：`src-tauri/src/logging.rs`、
`mobile/packages/swarmdrop-core/rust/mobile-core/src/logging/mod.rs`
——**两份独立常量，改一端必须改另一端**，各自有一条护栏测试看守。

### 坑：mDNS socket 也走 `wrap_udp_socket`

`MuxedRuntime` 若无差别替换，`PeerConnection` 额外绑的 `0.0.0.0:5353` 多播 socket
也会拿到同一条 mux 支路。更糟的是 driver 对 mDNS socket 用
**`AsyncUdpSocket::local_addr()`** 建索引（我们返回的是共享监听端口），而对 ICE socket
用 **bind 时的临时端口**——于是同一批入站包被随机打上两种 `local_addr` 标签，其中一半
永远匹配不上 local candidate。

症状极隐蔽：ICE 只在 warn 级刷 `Discarded message, not a valid local candidate`，
握手静默超时。**修法**：`se.set_multicast_dns_mode(MulticastDnsMode::Disabled)`
——direct 模式的地址是确定性构造的，本来也没有要发现的东西。

### 坑：`PeerConnection` 的 `connected` 早于 DataChannel 的 `open`（native 也一样）

research 文档曾记「native 侧没有这个问题，`webrtc-rs` 的 `send` 是 async 内部等 SCTP」
——**0.20 不是这样**：DTLS 完成（`connected`）与 SCTP 关联建立之间实测差约 2 ms，
期间 `dc.send()` **成功返回但把数据丢掉**。

Noise 的第一条握手消息就这么静默消失，两端各自等对方直到超时。修法是写第一个字节前
先等 `RTCDataChannelState::Open`（`data_channel::await_open`），出站/入站子流同样要等
——否则表现为「刚开的流对端读到 EOF」。

这与 wasm 侧那条「`Connecting` 不是错误」是同一个时序问题的两面。

### 坑：webrtc 0.20 的 `on_data_channel` **也会回灌本端自己开的通道**

`PeerConnectionEventHandler::on_data_channel` 顾名思义应该只报对端开来的通道，但
webrtc 0.20 的 driver 对**每一个** `OnOpen` 事件都调它，不区分通道由谁建
（`peer_connection/driver.rs` 的 `RTCDataChannelEvent::OnOpen` 分支）。

> ⚠️ **别把这归因成「rtc 给 negotiated 通道发了 DCEP」——那是错的**，本文档一度这么写。
> `rtc-datachannel` 明确对 negotiated 通道抑制了 DCEP 发送（`data_channel/mod.rs` 里
> 带注释与专门的测试 `test_data_channel_negotiated_opens_stream_without_dcep_handshake`），
> 对端**根本看不到**那条通道。出现在 `on_data_channel` 里的 id 0 是**自己刚建的**。

后果有两个，第二个更严重：

1. direct 的 Noise 通道（`negotiated` id 0）被当成第一条入站子流交给上层——它握手完就
   关了，症状是「连接建好了，第一条子流一读就 `UnexpectedEof`」，与真正的对端关闭难分。
2. **`poll_outbound` 开的业务子流同样会被回灌**，于是上层在一条对端根本不知情的流上等
   协议协商。这条**打洞路径也有**，只是「一端只开流、另一端只收流」的测试撞不出来。

正确的不变量是**「muxer 永不把本端开的通道当成入站子流」**：`Muxer` 持有一份
`local_channels: HashSet<RTCDataChannelId>`，构造时收下建连期间已开的（direct 的 Noise
通道），`poll_outbound` 每开一条就登记。按 id 而非 label 过滤——label 是本端自己编的，
对端完全可以用同名 label 开流。

（`init` 通道那条按 label 过滤是另一回事，不要一起改掉：它确实由对端 in-band 通告。）

**这个缺口值得提上游**：与 rtc #137 同一性质，几行判断即可，且能同时消掉两个 target 的
workaround。尚未提。

### ⛔ 建 DataChannel 永远不要传 `None`——rtc 的 `ordered` 默认是 `false`

**2026-07-28 实证，本轮最贵的一个坑。** `rtc::data_channel::RTCDataChannelInit` 是
`#[derive(Default)]`，`ordered: bool` 于是默认成 **`false`**——与它自己紧邻的文档
（「The default value of `true` guarantees that data will be delivered in order」）
和 W3C 规范（`ordered` 默认 `true`）**都相反**。

无序通道的后果远不止「乱序」：**无序 chunk 绕过 SCTP 的有序投递队列**，会抢在同一批
发出的 DCEP OPEN 前面到达对端。对端在一条还不认识的 stream 上看到用户数据，
`RTCDataChannelInternal::accept` 要求 PPID 必须是 DCEP，于是报
`InvalidPayloadProtocolIdentifier(53)`（53 = WebRTC Binary，完全正常的值）——而那个错误跑在
pipeline 的 read pass 上，**只 `warn!` 不上抛**，与 [issue 826](https://github.com/webrtc-rs/webrtc/issues/826)
同一条吞错误的路径。

净效果：**每条子流的第一条消息静默丢失**。发送端 `send()` 返回 `Ok`，链路零报错，
表现为 multistream-select 永远协商不完、两端各自 10s 超时：

```
libp2p_swarm::connection: inbound stream upgrade timed out    ← 两端都刷
rtc::peer_connection::handler: DataChannelHandler.handle_read got error:
    Unknown PayloadProtocolIdentifier 53                       ← 开 RUST_LOG=rtc=debug 才看得见
```

排障提示：症状停在 libp2p 层，根因只在 `rtc` 的日志里。**direct 排障第一件事就是
`RUST_LOG=rtc=debug`**——上面那行 warn 是唯一的线索。

修法在 `native/muxer.rs::ordered_reliable()`，crate 内**所有** `create_data_channel`
都过它，`None` 一处不留（Noise 通道本来就显式写了 `ordered: true`，所以握手一直是好的，
坏的只有子流）。

⚠️ **crate 级测试撞不出来**：`direct_loopback.rs` 那种手写 poll 循环会在
`create_data_channel` 之后立刻再转一圈，DCEP OPEN 因而单独成包先发，竞态被盖住。
真 swarm 的 poll 节奏才暴露它——回归守卫是 `crates/net/tests/webrtc.rs` 的
`dial_own_webrtc_direct_listen_addr`（摘掉修复必红，已双向验证）。

wasm 侧不受影响：浏览器 `createDataChannel` 按规范默认 `ordered: true`。

上游已提 [rtc issue 139](https://github.com/webrtc-rs/rtc/issues/139)。

### 用 `rtc::` 转出子 crate，不要直接依赖 `rtc-ice` / `rtc-stun`

rtc `pub use` 了全套子 crate（`rtc::ice` / `rtc::stun` / `rtc::dtls` / …）。直接依赖
`rtc-ice` 看似等价，但只要两边解析到的不是同一份，同名类型就分叉成两个，报
「expected `rtc::rtc_ice::X`, found `rtc_ice::X`」这种极绕的错。经 `rtc::` 转一手
天然同源。

（2026-07 那阵 rtc 被 `[patch]` 换成 git 源时这是必然发生的；patch 虽已删除，但换成
版本号解析后，任何一次版本漂移都能重演同样的分叉，所以这条约束照旧。）

### ⚠️ `rtc` 是 `webrtc` 仓的 **git submodule** —— 两个都 git 依赖必然分叉（2026-08-11）

上一条说的是「别绕过 `rtc::`」。这条更硬：**`webrtc` 与 `rtc` 不能同时用 git 依赖。**

`webrtc` 仓把 rtc 放在 `rtc/` 子目录（submodule，`url = https://github.com/webrtc-rs/rtc`），
`Cargo.toml` 里写的是 `rtc = { version = "…", path = "rtc" }`。于是：

```toml
# ❌ 这么写，rtc 会有两份
webrtc = { git = "https://github.com/webrtc-rs/webrtc", rev = "…" }
rtc    = { git = "https://github.com/webrtc-rs/rtc",    rev = "…" }   # 与 webrtc 的 path 依赖不同源
```

cargo **会**拉 webrtc 的 submodule（能在 `~/.cargo/git/checkouts/webrtc-*/…/rtc/` 看到），
但那是 path 依赖，与我们这条 git 依赖是两个 source。`[patch.crates-io]` 也救不了——
它管不到 path 依赖。实测报错：

```text
expected `RTCDataChannelState`, found `rtc::data_channel::RTCDataChannelState`
expected trait `webrtc::peer_connection::rtc_crypto::RTCCrypto`, found `rtc::rtc_crypto::RTCCrypto`
```

**唯一的解法是所有类型都从 `webrtc::` 取**，不出现 `rtc` 这个直接依赖。上游同意这个方向
——`peer_connection/mod.rs` 明确写着「`rtc` is a private dependency of this crate」并为部分
参数类型做了 re-export。但 2026-08-11 时**规则没走完**，本仓要用的 5 个不在其中
（`MulticastDnsMode` / `NetworkType` / `RTCDtlsRole` / `SctpMaxMessageSize` /
`CertificateParams`）。已提 [webrtc#869](https://github.com/webrtc-rs/webrtc/pull/869) 补齐。

一个**不受此限**的例外：`rtc::stun`（`udp_mux` 解析入站 STUN 学 ufrag）。它的类型
**不跨 API 边界**——我们只拿它解析字节得到一个 ufrag 字符串，不把 `StunMessage` 传给任何
webrtc API，所以即便存在两份 stun crate 也不会撞类型。判据就是这句话：**分叉只在类型
经过 API 边界时才致命**。

（走 crates.io 版本号时没有这个问题：webrtc 的 path 依赖会被同版本号的 crates.io `rtc`
统一解析。所以这条只在「想用 git master 提前拿修复」时才咬人。）

### direct 的 UDP 读循环挂在 `Transport::poll` 上

与官方 `libp2p-webrtc` 同构：`UdpMux::poll` 由 `Transport::poll` 驱动，本 crate 全程
无 `spawn`。代价是**停止 poll transport 就等于停掉这个端口上所有连接的收包**——
包括建连之后的数据面。真实场景里 swarm 的事件循环一直在 poll，所以不成问题；但写
测试时必须自己模拟（`direct_loopback.rs` 的 `drive`），否则会看到「握手永远不完成」。

### 浏览器侧 direct dialer：两处只能绕的平台限制

浏览器只做 dialer（`RTCPeerConnection` 没有服务端形态），且有两件事在 native 上是一行
API、在这里必须绕：

| | native | 浏览器 |
|---|---|---|
| 设 ICE 凭据 | `se.set_ice_credentials(ufrag, ufrag)` | **改写 `create_offer` 产出的 SDP**（`munge_ufrag`）——没有对应接口 |
| 取本端指纹 | 直接从证书算 | 只能 parse `local_description()` 的 `a=fingerprint` 行 |

证书由 `generateCertificate` 现生成、每次都换，这在 direct 里无所谓：**浏览器只拨不被拨**，
没人会把它的 certhash 记进地址。

另外 wasm 侧也补了 `await_connected`（等 `RTCPeerConnectionState` 的终态）。少了它，失败的
连接只表现为「Noise 握手永远不返回」，最后由上层超时收场——**拿不到任何原因**。排 direct
问题时，先看这条日志走到哪一步。

### 浏览器实测怎么做

`cargo run -p webrtc-p2p --example direct_listener` 起一个最小监听端，它打印带 certhash 与
`/p2p` 段的完整地址；把地址粘进 `docs/app/app` 的 connect 框即可。

**wasm 代码编过不代表逻辑对**——本次 wasm dialer 一次编译通过，但首轮实测直接超时（那次
是公网 relay 挂了，不是实现问题）。浏览器那半必须真的用浏览器跑。

⚠️ 改了 `crates/webrtc-p2p` 或 `crates/web` 后**必须 `cd docs && pnpm build:wasm`**，否则
浏览器加载的还是旧产物——曾对着旧产物的日志判断新代码的行为。

### 后续可做（本轮 code review 提出、未做）

- **`DirectTransport` 实现 `libp2p_core::Transport`**，用 `OptionalTransport + or_transport`
  替掉 `swarm/transport.rs` 里手写的地址分派。现在等于在 `Transport` 之上又造了一个小
  `Transport`。纯结构重构、无功能收益，故未在发版前动。
- **`webrtc_p2p::new` 强制要 `Factory`**，而 direct 全程不碰 `Backend`——测试与 example
  都得捏一个「必然返回 Err」的假工厂来表达「我不需要这个平面」。对一个准备独立发布的
  crate，这是 API 层的问题。
> 这节里原有的四条「上游候选」**已全部提出去并落地**（webrtc #825/#828、rtc #138、
> libp2p #6571/#6572），`remote_fingerprint` 也已换成上游 API。现状以上面的
> 「上游缺口台账」为准，不要再照这节去重复提。

**相关文件**：`crates/webrtc-p2p/src/backend/native/direct/{udp_mux,upgrade,sdp,certificate,transport}.rs`、
`crates/webrtc-p2p/src/backend/wasm/direct.rs`、
`crates/webrtc-p2p/src/{config.rs,swarm/{transport,direct}.rs}`、
`crates/webrtc-p2p/examples/direct_listener.rs`、`crates/webrtc-p2p/Cargo.toml`（`rtc` /
`webrtc` 的版本下限说明——**两批修复卡出两个下限**：≥ 0.20.0 与 ≥ 0.20.2，后者见
[`2026-08-11-webrtc-driver-busy-loop.md`](../research/2026-08-11-webrtc-driver-busy-loop.md)）。
根 `Cargo.toml` 现无 `[patch.crates-io]` 段——2026-08-04 与 08-11 各有一批 patch，均已
兑现退出条件删除

## wasm 工程约定

- 双 target 门禁：`scripts/check-wasm.sh`（CI rust.yml 的 wasm job 每 PR 跑）。
  macOS 本机跑 wasm 检查**必须经此脚本**（Apple clang 无 wasm backend，脚本会指向
  Homebrew LLVM）。
- cfg alias 集中定义（各 crate build.rs 的 `wasm_browser`），代码里只写
  `#[cfg(wasm_browser)]`；**业务层（crates/core）零 cfg** 是硬约束（iroh 的
  「shared 核心零 cfg」范式），平台差异全部被内核与 n0-future 吸收。
- Send 约束：当前统一 `Send`（wasm 侧 handler 不碰 JS 类型即可满足；
  storage-abstraction.md 的 SendWrapper 结论支撑）。`MaybeSend` 方案备而未用，
  真被 !Send 卡住时再引入。
- `wasm-bindgen-futures` 必须精确 pin `=0.4.58`（master 的 libp2p-swarm 钉死了它）。
- **`check-wasm.sh --clippy` 用 `-D warnings`，比本机 `cargo clippy` 严**：改 core/host 里
  会进 wasm 门禁的代码时，纯 `cargo clippy`（无 `-D warnings`）只当 warning 放行的 lint
  （如给组合根加参数触发的 `too_many_arguments`）会在 wasm job 变硬错误挂 CI。
  提交前对 wasm 侧改动跑 `bash scripts/check-wasm.sh --clippy`，别只信本机 clippy 绿。

## wire v2 契约点（改动前先看固化测试）

- net-base 的 serde 表示是 IPC/wire 契约：NodeId/Addr 字符串、状态枚举 camelCase
  （`status.rs` / `node_id.rs` / `addr.rs` 的契约测试）。
- `DhtKey::namespaced` 带长度前缀域分离（纯拼接下 `("ab","c")==("a","bc")`，
  旧栈同缺陷已修）——**改派生规则 = 分享码/在线宣告全部失配**。
- transfer 数据面 `BlockData.proof` = bao-tree 逐块验签切片（u8 标志 + 可选 len-prefixed
  bytes）。**已启用（2026-07-18）**，不再恒 None：接入未 bump 协议版本（proof 是 opaque
  bytes，wire 布局不变）。选型 Approach B——proof 携完整 bao 切片、`data` 置空（叶子只出现
  一次、无 2x 冗余）；root == `FileInfo.checksum`（标准 blake3，chunk group 不改 root）；
  proof 缺失/验签失败 = 协议违规 → 断流走 Interrupted 恢复。
  发送端 outboard 与 checksum 同一遍流式构建（**2026-08 起才真是一遍**，此前是两遍读加一条
  `debug_assert_eq!`），落 `transfer_files.outboard` 供 resume 免重算——可用性判据是长度
  （`bao::is_outboard_usable`）而非「是否为空」。
  **chunk group 自 2026-08 起 == `CHUNK_SIZE`（256KiB），每个传输块恰好一个叶子**；曾是
  16KiB，验签粒度比传输块细 16 倍而无消费方，代价是 outboard 大 16 倍、构建时对
  `read_source_chunk` 的调用次数大 16 倍（三端宿主都是每次重开文件，调用次数才是主导成本）。
  改它等于改 wire（proof 树形状变，旧端第一个块就验签失败），**必须同时 bump
  `TRANSFER_DATA_PROTOCOL`**；同一次变更摘除了 `/2` `/3` 的注册，不兼容因此表现为协商失败
  而非「传输老是断」。
  实现见 `crates/transfer/src/bao.rs`（sync encode/decode 纯算法 wasm 可编；outboard 构建走
  bao-tree tokio_fsm + iroh-io 的 AsyncSliceReader 适配 FileAccess，均实测 wasm 可编，无 cfg）。
- RPC 帧：u32 BE 长度前缀 + CBOR，上限 1MiB，恶意长度在**分配前**被拒
  （`rpc.rs` 帧测试）。

## 配对邀请 PairInvite（`crates/invite`，替代 6 位配对码）

6 位配对码 + DHT 分享码已**整体废弃**（低熵可枚举、DHT 记录不证明身份）。替代品是独立
wasm-clean crate `swarmdrop-invite`（依赖 net-base，不依赖 core——core 与 web 共享），
`PairingMethod` 现只剩 `Direct`（LAN mDNS）+ `Invite`。

- **wire 契约（`invite.rs`，改动前看 `wire_v1_keeps_version_capability_and_tail_signature_layout`
  单测）**：链接是 `sd:` 前缀 +
  base64url-nopad；二维码是 `SD` 前缀 + base32-nopad。二者承载相同的 postcard 单变体
  enum `InviteWire::V1`（判别码 `0x00` 即版本，未知变体解码即失败）。wire 只传 128bit
  capability、身份、精简地址、到期时刻、网络策略、设备名与平台名；不再传 invite_id 或签发时刻。
  **签名尾置**——`InviteV1.signature` 是末位定长 64 字节，signable =
  `bytes[..len-64]` 覆盖含版本判别码在内的全部前置字节（防降级），验签公钥从 `inviter_id`
  的 identity multihash 就地恢复。字段序即契约，V1 发布后不可改。
- **一次性/TTL**：`InviteRegistry`（发起端内存态）以 `sha256(capability)` 为键，不存明文；入站 handle
  非消费预检 + respond(Success) 原子 CAS `Pending→Consumed`（两台扫同码仅先确认者成功）。
- **撤销（`PairingManager::revoke_invite`，2026-07-29 补齐接线）**：入参是**邀请串**不是
  capability——三端 UI 手上只有串，capability 经解码取回（`revoke_via_decoded_invite_string_blocks_consume`
  钉死往返一致）。**幂等且不返回 Result**：串解不开、capability 不在表里（已消费 / 非本机发出 /
  节点重启后表已空）语义上都等价于「它已不可用」，正是调用方要的终态；传入他人的邀请串同样
  no-op，撤销不了别人的东西。三端调用点统一为「生成新邀请前撤销被覆盖的旧串」+「clearActiveInvite」，
  一律 fire-and-forget。
  ⚠️ **不要在离开邀请页时撤销**：store 里 `activeInvite` 是刻意跨页面持久化的（用户复制走
  链接后会切走等对方粘贴），撤销而不同步清 store 会得到「二维码照常显示、实际拨不通」——
  比不撤销更糟。撤销与清状态必须成对。
- **QR 三端统一（`qr.rs`，唯一编码源）**：链接 payload 的 Base64URL **不能**大写；先解出并验签
  wire，再编码为 `SD` + Base32，才能落 QR alphanumeric 模式；ECL::M + 4 模块 quiet zone。
  三端渲染 core 出的 SVG/矩阵（桌面/web 用 `invite_qr_svg`、RN 用 `invite_qr_matrix` +
  react-native-svg），**深模块 + 白底不随暗色反色**。
  `decode` 对 `SD` 二维码前缀和 Base32 payload 大小写不敏感；带 `:` 的 Base64URL 链接则必须
  保持原样，补有两类回归断言。
- **地址瘦身**：每类网络分别只留 TCP（无则 QUIC，native）、**WebTransport** 和 WebRTC
  （后两者都是浏览器可拨的直连传输）各一条；三类路径从该网络分类的全部地址中独立挑选，
  避免 TCP-only 网卡排在前面时误删 WebRTC。Auto 最多保留 100.64/10 overlay（Tailscale）、
  LAN、公网与 relay 的三类各一条；198.18/15 仅在没有 overlay 时回退。LocalOnly 只保留 LAN 那桶。
  挑完还要过一道 **QR 密度闸**（下一节），地址多到扫不动时按价值反序回收。
- **三端接线**：桌面命令 `generate_pair_invite`/`decode_pair_invite`/`invite_qr_svg`/
  `consume_pair_invite`；mobile uniffi 同名 + `pair_direct`（补回 Direct）+ `invite_qr_matrix`；
  web `WebNode::connect_invite`（decode 纯函数只需 net-base）。剪贴板感知（`hasStringAsync`
  探测亮 chip）与移动扫码（expo-camera `CameraView`：`barcodeTypes:["qr"]` + 前缀校验 +
  `lockRef` 一次性闸 + 权限三态 + AppState 回前台重拉）均已落地（`mobile/src/app/pairing/scan.tsx`）；
  原生 `CameraView` 需 `expo prebuild` 重编。

### 邀请地址有 QR 密度上限，而它在 2026-08-12 之前就已经卡线（2026-08-12 加）

**WebTransport 地址补进邀请**时才发现的：`select_invite_addrs` 是「每桶挑几条 × 桶数」，
而**桶数没有上界** —— 一台同时有 CGNAT 覆盖网、局域网、公网直连的机器就是 3 个桶。
补 WebTransport 之前，那种满配桌面的码面已经是 **97 模块**，距上限 98 只剩一格。

上限从码面尺寸反推，不是拍的：三端最小码面 **196px**（移动端白卡内沿 `220-2×12`、
Web 端 `QR_SIZE`；桌面 260px 更宽松），px/模块跌破 2 摄像头就读不出来 ⇒ 含 quiet zone
≤ 98 模块。**容量从来不是约束**（ECL::M 下 2079 字节才到顶），先出事的永远是密度。

一条 WebTransport 地址约 **+85B wire**（两个 certhash 占大头，轮换期两张证书都要在场），
折成 base32 是 ~140 字符，比普通地址贵 3 倍。实测模块数（`INJECTED_NAME` 作设备名）：

| 配置 | 不带 WT | 带上但不裁 | 现在（带 + 裁） |
|---|---|---|---|
| 家用 lan + circuit | 85 | 93 | **93**（5 条，一条没裁） |
| 公网 lan + public + circuit | 89 | 105 ❌ | **97**（8 → 7，保住 WT） |
| CGNAT shared + lan + circuit | 89 | 105 ❌ | **97**（8 → 7，保住 WT） |
| 满配 四类齐全 | 97 | 117 ❌ | **97**（11 → 8，WT 全裁） |

所以 `fit_invite_to_scannable`（`crates/core/src/pairing/manager.rs`）**逐次重编码 + 量码面**
往回裁，判据就是最终码面本身、零推导误差。丢弃顺序 = 价值反序：

1. **WebTransport** —— 纯增益。丢了浏览器仍能靠同桶的 webrtc-direct 拨通；配对一旦成功，
   identify 会把完整监听地址交回来，**后续传输照样走 WebTransport**。所以邀请里的它只影响
   「首次拨号那一下」，代价是慢不是连不上。同类里从后往前丢（公网 → 局域网 → 覆盖网）：
   越靠前的桶越可能与扫码方同网，而同网正是 WT 唯一比 webrtc-direct 快一个数量级的场景。
2. 其余直连地址，同样从后往前。
3. **circuit 一条都不丢。** 跨网时它是唯一可达路径，而扫码方在哪个网络，生成邀请的这端
   并不知道。

回归钉是 `invite_stays_scannable_at_every_scale`，四档配置齐上。⚠️ 它必须用**固定
capability**：`generate` 那份是随机的，而 base32 payload 里数字连段的长短会改变最优分段
的取舍，码面因此浮动一档（`qr.rs` 的 `fixed_invite` 记着同一件事）。

两条**必须留住**的下界，少任何一条都会产出比「扫不动」更糟的东西：

- **circuit 一条都不丢**（跨网唯一可达路径）；
- **最后一条地址也不丢，哪怕它不是 circuit**。零地址的邀请**编得出、扫得动、复制得走，
  唯独没有任何东西可拨**，两端都不报错。这条不能只靠上一条兜：`LocalOnly` 邀请**根本
  没有 circuit 地址**（只放私网那一桶），设备名长一点（`DeviceName::MAX_CHARS = 40`，
  CJK 下约 120 字节）就会一路裁到空。

所以这个函数给的是**尽力而为**，不是保证：真裁不下去时返回一条密度超标但语义完好的邀请
—— 用户还能改用粘贴链接。

⚠️ 丢 WebTransport 时 **`lan` 那条留到最后**，不是天真的「从后往前」。桶序是
shared → lan → public，`rposition` 从尾部删等于「公网 → **局域网** → 覆盖网」，正好把最该
留的排在中间：真正同网的是 RFC1918，不是 100.64/10 覆盖网，而那 4.5 倍只在真正同网时兑现。
挂着 Tailscale 的笔记本（shared 桶有东西 ⇒ 密度压力最常见的来源）恰恰会踩中这个次序。

> **下次往邀请里加东西之前先跑这条测试。** 越线不会有任何编译错误、QR 照样生成、链接照样
> 能用 —— 只有真机扫码那一下失败，而那是最难归因的失败形态。

**一条有意接受的代价**：裁剪跑在 `encode_invite` 里，于是**链接分享也被裁**——而链接根本
没有密度上限。这不是疏漏：邀请只有**一种**对外文本形态（openspec: invite-url-canonical），
链接与二维码是同一个字符串。给两者各签一份会让「已复制」「撤销」「消费」全部要分辨手上
是哪一份，换来的只是纯链接用户多几条 WebTransport 提示，而那些提示在配对成功后由 identify
补齐。**要改这条，先改的是「单一形态」那条约束，不是这里。**

### ⚠️ 客户端不要对邀请串做大小写归一（2026-07-29 修）

`sdinvite` 时代的载荷是 Base32，大小写不敏感，于是 `scan.tsx` 顺手写了
`previewInvite(raw.toLowerCase())`——注释还写着"归一回小写规范形态"。**换成
`sd:<base64url>` 后这一行直接毁掉载荷**：Base64URL 里 `A` 与 `a` 是不同的 6 bit，
小写化后 postcard 解不出来，移动端「粘贴邀请」100% 失败（扫码那条侥幸没事——二维码
本来就是 Base32）。

同一次改动里 KIND 前缀从 8 字符缩到 2 字符，`startsWith("sd")` 也一并退化成
近乎无效的判据：任何以 sd 开头的二维码都会被送进 `previewInvite`，白白锁住扫码器
再弹一次「邀请无效」。现已换成带字符集与长度下限的 `INVITE_PATTERN`，两种载体各一支。

> 通用教训：**编码换了字母表，就要回头查所有做大小写变换的调用点**。Rust 侧
> `decode_wire_text` 当时已经写明"带 `:` 的链接形态必须保持大小写"、单测也钉了
> `decode(&s.to_ascii_uppercase()).is_err()`——契约是对的，漏的是三端调用侧的同步。
> 前缀长度变化同理：它既是判别码也是误匹配的唯一屏障。

## WebTransport native transport（`crates/webtransport-p2p`，2026-08-12）

rust-libp2p 只有 `transports/webtransport-websys`（浏览器侧拨号，wasm），**没有 native
listener** —— 浏览器能拨、没人能接。上游 [PR #4348](https://github.com/libp2p/rust-libp2p/pull/4348)
（维护者 mxinden 的 native draft）自 2023-10 停在 draft。本 crate 补的是这个缺口。

底层库是 crates.io 的 **`wtransport` 0.7.1**（quinn 0.11 + rustls 0.23，与 libp2p-quic 同源）。
选它而非 `web-transport-quinn` 的决定性理由是 `Endpoint::reload_config(cfg, rebind=false)`
—— 换服务端证书**不断既有连接**，那是证书轮换的硬前提，对方没有对应 API。

### 回环吞吐：比 webrtc-direct 快 4.5 倍，且方差小一个数量级

同机回环、同一 `Endpoint` 应用层、只换 transport，64 MiB × **6 次取中位数**
（`crates/net/examples/transport_throughput.rs`）：

| transport | 中位数 | 区间 |
|---|---|---|
| TCP + Noise + yamux | 933 MiB/s | 927–1149 |
| **WebTransport** | **322 MiB/s** | 286–326（±7%） |
| QUIC | 266 MiB/s | 248–276 |
| WebRTC-direct | 72 MiB/s | **43.7–288（6.6 倍）** |

两条结论各自独立成立：

1. **吞吐**：WebTransport ≈ 4.5× webrtc-direct。差距来自用户态栈的深度 —— QUIC 一层做完
   可靠传输 + 多路复用 + TLS，WebRTC 要 ICE + DTLS + SCTP 三层各遍历一次数据。
2. **稳定性**：webrtc-direct 的区间横跨 6.6 倍，WebTransport 只有 ±7%。对「传大文件要多久」
   这类用户可感知的指标，方差比中位数更重要。

⚠️ **WebTransport 比裸 QUIC 还快 21%，这一点尚未查清**（理论上它是 QUIC + HTTP/3 一层，
应该更慢）。可能是 quinn 配置差异（buffer、拥塞控制）或 libp2p-quic 的 stream 包装层开销。
**别把它当成已知结论去引用**。

⚠️ 回环瓶颈是 CPU，跨网瓶颈通常是带宽与 RTT。**这组数字不能外推到真机** —— 真机测量仍是
未做的前置（见下方"已知负债"）。

### 架构：重心在证书生命周期，不在传输层

与 `webrtc-p2p` 逐条对比，只有最后一行变复杂：

| 维度 | `webrtc-p2p` | `webtransport-p2p` |
|---|---|---|
| 模式数 | 2（打洞 + direct） | 1 —— WebTransport 没有 NAT 穿透 |
| 建连协商 | SDP、ICE、DTLS 角色、ufrag | 无，QUIC 握手是库的事 |
| socket 复用 | 自写 1121 行 `UdpMux` | 无，独占端口 |
| 子流 | DataChannel + 自做 framing + `init` 通道陷阱 | QUIC 流**本身就是流**，muxer 极薄 |
| 后端抽象 | 必须（native / wasm 两套栈） | 不需要，浏览器侧用上游 websys |
| 证书 | 一张，**永不改变** | **两张，会过期，14 天轮换** |

那一行引入了 `webrtc-p2p` 里完全不存在的维度：**时间**。全 crate 约 1500 行，其中
`certificate/rotation.rs` 是唯一有状态、有时钟、驱动外部可见行为（通告地址）的部分。

分层：`addr` / `certificate`（纯逻辑，零 IO）→ `noise` / `muxer`（libp2p 语义）→
`listener` / `dialer` / `transport`（wtransport 绑定）。

⚠️ **「只有 L2 认识 wtransport」这句话不成立，别照抄。** 9 个源文件里 6 个 import 它：
`certificate` 借 `wtransport::Identity` 当证书容器，`muxer` 直接绑它的流类型（为唯一一个
实现造 trait 是 YAGNI）。真正成立的是——**换库时决定「行为」的部分一行不用动**：轮换状态机
（456 行，全 crate 最复杂）、地址解析、Noise 语义都不认识 wtransport。这条差别很重要：
第一版文档写的是前者，而 design.md 拿它兜底了三处论证，属于「先写断言、实现没跟上」。

### ⚠️ 与 webrtc-direct 的 Noise **机制互斥**，照抄必失败

| | webrtc-direct | WebTransport |
|---|---|---|
| 身份与信道绑定 | `libp2p-webrtc-noise:` + 双方 DTLS 指纹作 **prologue** | **`webtransport_certhashes` Noise 扩展**，**不设 prologue** |

两者是同一目的的两种机制。照抄隔壁 crate 会让握手在第一条消息就失败，而症状看起来像
「Noise 实现有 bug」，极难归因。`libp2p-noise` 的 `Config::with_webtransport_certhashes`
两侧都现成（responder 上报 / initiator 验证），spec 里最难的一块不用自己写。

### 通告地址的实际寿命是 28 天，不是 14

spec 要求通告地址同时携带 `current` 与 `next` 两个 certhash。把时间轴画开：

```
第 0 天    通告 [A, B]  ← 客户端记下
第 14 天   A 过期 → 通告 [B, C]
           客户端持旧地址拨 → 服务端用 B → B ∈ {A,B} → 连得上
第 28 天   B 过期 → 通告 [C, D]
           客户端持旧地址拨 → 服务端用 C → C ∉ {A,B} → 断
```

即一条通告地址能撑过**一整轮**轮换。这条推论直接决定上层「多久刷新一次 bootstrap 清单」，
由 `advertised_addr_survives_one_rotation` 钉死。

**由此还得到一条不显然的必需品**：服务端的 Noise 扩展**必须带上刚退役的 certhash**。
上表第 14 天那行，TLS 层过了（服务端出示 B，B 在客户端集合内），但 Noise 层若只上报
`{B, C}`，`{A,B} ⊄ {B,C}` 就会失败 —— **TLS 过了 Noise 仍会挂**。spec 那句「近期过期的
也建议带」不是可选优化。

### 判据：libp2p-quic 不会抢走 WebTransport 地址

WebTransport 地址形如 `/ip4/…/udp/…/quic-v1/webtransport/certhash/…`，同时含 `/quic-v1`。
它没被 libp2p-quic 认领，唯一依据是上游 `multiaddr_to_socketaddr` 对 `/quic-v1` 之后的任何
非 `/p2p` 段一律 `return None`（`transports/quic/src/transport.rs`）。

**升 libp2p rev 时要重新确认这一条**。破了的话表现是 WebTransport 地址被 quic 认领、然后
永远拨不通，且没有任何错误指向真正的原因。

`Addr::transport()` 里同理：`/webtransport` 的判别**必须排在 `QuicV1` 之前**，否则会判成
普通 QUIC —— 判据错了但没有编译错误。

### 证书生命周期的四条判据（都是 code-review 抓出来的真 bug）

第一版实现全绿、clippy 干净、61 条测试通过，仍然错了四处。四条各自独立，且**都不会在
任何测试里表现为红**，只会在 14 天后、或某次重启后、或某台机器上悄悄发作：

1. **两张证书必须重叠，不能首尾相接。** 轮换由 poll 里一个 60s 的定时器驱动，「该换了」与
   「真的换了」之间必然有滞后。若判据是「`current` **已经过期**才换」，那段滞后里服务端
   出示的是过期证书 —— 浏览器与 `wtransport` 客户端都直接判 `Expired`，**期间所有新连接
   一律 TLS 失败**，每 14 天来一次。现在 `next` 提前 1 小时生效，切换点落在两张都有效的
   重叠区里。
2. **退役 certhash 必须跟着 PEM 一起持久化。** 它只活在内存里的话，**一次重启就把「旧地址
   能撑过一整轮」打掉**：对端拿旧地址来拨，TLS 过得去（服务端出示的 current 在它的接受
   集合内），Noise 却失败（判据是「期望 ⊆ 收到」，而重启后服务端不再上报退役的那个）。
   解法是 `retired` 存整张证书而非只存 hash，`to_pem` 一并写出。
3. **`store.load()` 报 IO 错时绝不能回写。** 读失败不等于数据坏了 —— 文件可能被杀软临时
   锁住、权限被改、一次 EIO。覆盖它等于把一次瞬时故障变成**永久**的身份丢失。
   CLAUDE.md 为设备身份文件写死过同一条判据（「读取失败不降级」），这里是同一个坑的第二次。
4. **私钥的编码变体要挡在门口。** `wtransport::PrivateKey::from_der_pkcs8` 只是**贴标签**
   不解析，而起监听走的是 rustls 的 `.expect("已经验证过")`。一份 SEC1 私钥
   （`-----BEGIN EC PRIVATE KEY-----`，openssl 默认输出）会一路通过构造、在第一次
   `listen_on` 时把整个 Swarm 线程 panic 掉。

> 通用判据：**「有状态 + 有时钟 + 有持久化」的子系统，测试全绿不代表对。** 上面四条对应的
> 是四个不同的时间尺度（一个检查周期 / 一次重启 / 一次 IO 抖动 / 一次手工换文件），任何一条都不在
> 常规测试的观察窗口内。写这类代码时要专门列举「哪些时刻会发生什么」，而不是等测试告诉你。

### 抗 DoS：`mpsc::channel(n)` 的容量不是 n

`futures::channel::mpsc::channel(n)` 的真实容量是 **`n + Sender 个数`** —— 每 clone 一个
Sender 就多一个保证槽位。accept 循环若给每条连接 clone 一个 Sender，那个数字就形同虚设，
「靠通道容量做背压」整句话不成立。

WebTransport 的 listener 因此改用**信号量限住在途握手数**（拿不到许可直接丢弃 incoming，
不排队 —— 排队本身就是要防的那个堆积），并给 QUIC+CONNECT 那一段单独加了超时（`Config`
里那个握手超时只盖交付给 Swarm **之后**的 Noise，管不到这段）。公网 relay 上这是可被远程
触发的内存增长路径。

### 部署形态

- bootstrap 独占 **UDP 4004**，与 webrtc-direct 的 4003 **并存**（两条浏览器入口同时提供，
  可对比吞吐后再决定是否下线前者）。
- **WebTransport 的公网地址不能静态登记。** TCP/QUIC 的公网地址只由「IP + 端口」决定，
  恒定不变；WebTransport 的地址带 certhash，静态算出来的那条会在第一次轮换后失效。
  bootstrap 因此有一个后台任务盯着内核的监听地址视图，把 WebTransport 地址改写 IP 后
  **连同静态那几条整份声明**给内核（`crates/bootstrap/src/lib.rs`）。整份声明这一点不是
  风格选择，见下面「地址集合只增不删」。
- **桌面与移动端都监听 WebTransport（2026-08-12 起）**，端口由系统分配。启用判据是**宿主
  给没给证书端口**，不是「是不是原生端」（后者会让浏览器也被算进去，而它起不了任何监听，
  `bind` 会直接失败）。浏览器传 `None`，只拨号。
  > 移动端一度被判为「不该监听」，两条理由都不成立，值得记住怎么错的：
  > ①「要动 uniffi 跨 FFI 契约」—— 那是把证书端口错挂在 `KeychainProvider` 上才有的代价，
  > 它本来就不该挂在那儿（见下节）；落在 Rust 侧的文件里，跨 FFI 面一个字节没动。
  > ②「手机在 NAT 后，浏览器直连走不通」—— 只对**公网**成立。局域网内浏览器直连手机是走
  > 得通的，而那正是移动端**早就**在监听 webrtc-direct 的理由（`presets::Native` 里那条地址
  > 的注释写的就是「浏览器到原生端的局域网直连入口」）。同一个场景，没有理由只开慢的那条。
- **浏览器不需要写死 WebTransport 地址**：它先用 webrtc-direct 连上 bootstrap，经 identify
  学到带**当前** certhash 的 WebTransport 地址。这天然绕开了「清单里的地址会过期」的问题，
  也是 `docs/app/app/_lib/relay-helpers.ts` 不必改的原因。
- 日志三个 target（`webtransport_p2p` / `wtransport` / `quinn`）互不为前缀，也都不以
  `swarmdrop` 开头，**桌面与移动两份 `DEFAULT_FILTER` 要一起改**，各有一条断言看守。

### 测试纪律：三条「假绿」，都是变异测试抓出来的

变异测试（把实现改回缺陷形态，确认测试真的变红）在本轮抓出三条自以为有效的护栏。
**写完护栏就变异一次**，成本是一分钟，而假绿护栏比没有护栏更糟 —— 它会让人以为那条
判据有人看着。

第三条在 `crates/net`（地址簿淘汰），前两条在 `crates/webtransport-p2p`：

0. **`still_advertised_address_survives_a_flood_of_new_ones` 第一版是假绿的。** 它让那条
   「仍在被上报」的地址每轮都 `touch` 一次，可 `touch` 对**已被淘汰**的地址等于重新插入
   到簿首 —— 于是把淘汰逻辑换成「按物理位置截断」（即缺陷形态），测试照样通过：它验的
   是「被重新插入」而不是「被保护」。修正的关键是先用新地址把它**推到物理最末位**，再
   只刷新一次序号 —— 两种实现在这里才会分道扬镳。
   **同类陷阱**：测「X 不该被删掉」时，要确认 X 没有在别处被悄悄重建。

1. **`rejects_non_sha256_certhash` 原本是假绿的。** 它用 sha1（20 字节摘要）构造非法
   certhash，而实现里挡住它的是**长度检查**不是 code 检查 —— 把 `hash.code() != SHA2_256`
   整段删掉，测试照样通过。改用 blake3-256（摘要同为 32 字节）才真正验到 code 那条判据。
   **同类陷阱**：用「哪儿都不对」的输入测一个多条件判据，只能验到最先失败的那条。
2. **`rotation_keeps_existing_connections_alive` 原本会挂死而不是失败。** 把
   `reload_config` 的 `rebind` 改成 `true`，reload 失败 → 不发任何事件 → `next_event`
   永久 `Pending`。CI 里的表现是「job 超时」，看不出是谁挂的。现在所有等事件/等 IO 的
   helper 都套了 10s 超时。**凡是等 `Poll::Pending` 的测试都要有超时**，否则「本该发生的
   事没发生」这类 bug 的失败形态是不可读的。

### ⚠️ `Watcher::get()` 不推进版本标记（写 watch 相关测试必踩）

`Watcher::get()` 走的是 `borrow()`，**只有 `updated()` 里的 `borrow_and_update()` 会推进
版本**。而 `Endpoint` 持有的那个 receiver 从来没被读过，于是 `watch_addrs()` 每次 clone
出来的 `Watcher` 一开始就带着「有未读变更」—— **首次 `updated()` 必定立刻返回**。

写「重复操作不该触发更新」这类测试时，拿 `get()` 当消费手段会得到一条永远失败的测试
（测的是那个继承来的标记，与被测行为无关）。要消费积压只能用 `updated()`。

另一个同源的坑：`watch_addrs` 覆盖**整个** `AddrsInfo`，监听地址到达同样会唤醒它。
测「外部地址是否变化」时若还开着 listen，测的就成了「`NewListenAddr` 有没有恰好在这
几百毫秒里到」——一条会随机变红的测试。两条都记在
`crates/net/tests/lifecycle.rs` 的 `redeclaring_the_same_addresses_is_idempotent` 上。

### 两个由 `/simplify` 审出来的真 bug（都不是风格问题）

1. **「这对证书是不是刚生成的」在错误的地方二次派生。** `load_or_bootstrap` 明明知道自己
   走了哪条路径，却把这个事实丢掉，让调用方再 `store.load()` 一次去反推。两处对「fresh」
   的定义因此**对不上**：存量 PEM 损坏时，加载走的是「重新生成」，而反推看到
   `Ok(Some(垃圾))` 判成「不是新的」→ **不落盘** → 下次启动再坏一次、再生成一次，
   **certhash 每次重启都变**，而那正是这个持久化端口存在的唯一理由。
   一条 warn，没有任何东西指向「文件坏了但没人修它」。
   > 通用判据：**信息要在产生它的地方回报，不要在消费它的地方猜回来。** 猜得到的前提是
   > 「输入没变过」，而这里恰恰变了。

2. **Rust 侧加了传输种类，JS 侧两份镜像没跟上。** `TransportKind::Webtransport` 加进
   `Addr::transport()` 时特意写了「必须排在 `QuicV1` 之前」的注释（WebTransport 地址
   **同时含**两个段），但 `src/routes/_app/settings/-bootstrap-nodes-section.tsx` 与
   `docs/app/app/_lib/relay-helpers.ts` 各有一份从 multiaddr 字符串猜传输的 if 链，
   两处都把 `/quic` 排在前面 —— 地址被标成 "QUIC"，而同一屏的校验又以
   `unsupportedTransport` 拒掉它并列出「本端支持 TCP · QUIC · WebRTC Direct」。
   用户看到两句互相矛盾的话，无从判断错在哪。
   > 通用判据：**顺序敏感的 if 链是"加一个变体就会静默出错"的结构**，而它们没有编译期
   > 保护。仓里现在有三处（桌面 / Web / 后端），Web 那份已补了护栏测试
   > （`docs/app/app/_lib/relay-helpers.test.ts`）。真正的解法是三处都改成消费后端下发的
   > wire 名，别再猜 —— 后端已经有权威判据了。

## 地址集合只增不删：同一个 bug 的两面（2026-08-12 修）

带 `certhash` 的传输（WebTransport 14 天轮换、webrtc-direct 换证书时）让一个此前无害的
形态变成了真缺陷：**内核里保存地址的集合都只去重、不淘汰**。它有对称的两面，当时只看见
了一面。

### 面一：本机通告出去的外部地址（`Endpoint::set_external_addrs`）

后果分两级，**第二级最初漏评估了**：

- **立即**：identify 发出的地址集 = external ∪ listen。对端学到的 WebTransport 候选里只有
  1 条是活的；libp2p 的并发拨号预算默认 8 条，约 3 个月（6 次轮换）后死地址占满预算，
  连接成功率随进程运行时长线性下降。
- **最终**：identify 的 payload 走 `prost_codec::Codec::new(4096)`，而**编码端不检查长度、
  解码端检查** —— 超限时不是本机报错，是**每一个对端**都静默地解不出这条 identify。
  一条 WebTransport 地址约 89 字节，扣掉约 1.2KB 底噪后约 32 次轮换（≈15 个月）到顶。
  bootstrap 正是那种会连续跑几个月的进程。

**修法不是补一个 `remove_external_addr`，而是把 API 换成声明式的整份替换。** 命令式的
add/remove 会把一份易错的记账纪律推给每一个调用方：自己维护「已登记集合」、部分失败时只
回滚失败的那些、还要保证「先调用后记账」这个顺序不写反（写反的后果是一次瞬时失败让某条
地址**永久**不再重试，而日志只有一行 warn）。声明式把这些全部消掉 —— 调用方每轮把「现在
应该通告什么」整份发过来，重试就是把同一份声明再发一次，差量由内核算且只有一份实现。
它还天生对**漏采样**免疫：`watch` 是 last-value-wins，轮换瞬间的中间态大概率被跳过，而
每轮重新声明全集的话，跳过与否都不影响最终收敛。

两条判据不能破：

1. **声明的与自动确认的分开存。** 内核持 `declared_external`（宿主声明）与
   `confirmed_external`（AutoNAT / identify 观测），视图是二者并集。合成一个集合的话，
   宿主每声明一次就会抹掉 AutoNAT 刚确认的地址，下一轮 AutoNAT 又加回来，视图永久抖动。
2. **差量以视图为基准，不以 Swarm 的 external 集合为基准。** LanHelper 会把私网监听地址
   直接登记进 Swarm 却**刻意不进视图**（视图是给上层看的公网地址诊断）。改成「以 Swarm
   为准全量重算」会把那条当多余的删掉，relay 的 reservation 应答随即失去可拨地址，
   客户端报 `NoAddressesInReservation`。

### 面二：地址簿里对端的地址（`Actor::address_book`）

同一枚硬币的背面，而且更隐蔽：`record_addr` 只去重不淘汰，而 `address_book.remove` **只在
注销基础设施节点时**调用 —— 普通对端的条目在进程生命周期内永不清除。进簿的四条路径
（mDNS / DHT presence record / identify / 显式注入）都会随时间产出新地址：对端换 Wi-Fi 或
DHCP 续租就是一批新 IP，带 certhash 的地址更是每次轮换必变一条。这边撑爆的不是 identify
payload，而是**拨号预算**。

修法是上限 32 + LRU，但**淘汰判据必须是「最久没被提及」而不是「进簿最早」**：

- 一条一直可用的公网地址进簿最早、物理上排在最后，但只要对端还在 identify / DHT 里持续
  上报它，它就不该被淘汰。按物理位置淘汰的话，恰恰是那条唯一还能用的地址被新涌入的私网
  地址挤掉。
- 「最近提及」用**逻辑计数器**而不是时间戳：这里要回答的只是「谁更久没被提及」这个相对
  问题，而 wasm target 下没有可靠的单调时钟（kad 的 `Instant` 就为此分叉过）。
- **物理顺序（= 拨号优先级）与淘汰序号是两件事**：重报只刷新序号、不挪位置，否则拨号
  顺序会随 mDNS 的广播节奏抖动。

护栏是 `still_advertised_address_survives_a_flood_of_new_ones`，它**第一版是假绿的** ——
见下面「测试纪律」。

### 证书端口为什么不挂在 `KeychainProvider` 上

接桌面端时最初判断「要给 `KeychainProvider` 加三个方法，牵动 uniffi 跨 FFI 契约与 4 个
入库的生成文件」，因此把它推迟成独立工作。**那个判断是错的**：那个 trait 的三组方法都是
「读一次就完」的形态（身份与 webrtc 证书永不改变，宿主启动时交出去就不再过问），而
WebTransport 的证书要轮换并**回写** —— 它需要的是长期持有的可写端口，本就不该挂上去。

真正要解决的是另一件事：`webtransport_p2p::CertificateStore` 是 **native-only 依赖的类型**，
wasm target 下根本不在依赖树里，直接用会逼着 `swarmdrop_core` 的组合根给参数和字段加
`cfg(wasm_browser)` 分支 —— 而「业务层不写 cfg」是本仓的硬约束。做法是在 `crates/net` 定义
一个平台中立的同名端口（native 侧 12 行 adapter 转回去），组合根于是零分支。

**监听判据是「宿主给没给证书端口」，不是「是不是 Native」。** 后者把浏览器也算了进去，
而它起不了任何监听 —— 多给一条监听地址会让 `bind` 直接失败。正反两条 core 测试看守它。

**文件实现两端共用一份**（`crates/net` 的 `WebTransportFileCertificateStore`，各宿主只给
路径），与 `JsonFileDeviceConfig` 同一体例。这里刻意不走「端口三端各写一份」的常规体例，
判据是**实现里有没有容易写错、且错了没有反馈回路的不变量**：原子写（半截 PEM → 下次启动
重新生成 → certhash 变，而日志一切正常）、`0600`（里面有私钥）、读失败不降级。三条各写
两遍就是两次写错的机会。设备名那份用裸 `fs::write` 反而是对的 —— 丢了可以重设。

## 浏览器手测怎么做（2026-08-12 实测流程）

`serverCertificateHashes` 的准入只能在真浏览器里验，但**不需要人工点**：

1. 起本地 bootstrap：`--listen-ip 127.0.0.1 --external-ip 127.0.0.1`。
   **拨 `127.0.0.1` 能绕开 Chrome 的 LNA（Local Network Access）拦截** —— 拨局域网 IP 则
   未必，那是另一个变量，别混在一起测。
2. Chrome 走完整链路：`agent-browser` 打开 `/app/settings` → 「添加自定义引导节点」→ 粘贴
   `/ip4/127.0.0.1/udp/4004/quic-v1/webtransport/certhash/<h1>/certhash/<h2>/p2p/<id>`。
   判据看**服务端** debug 日志：入站连接的 `endpoint=Listener{ local_addr: …/webtransport/… }`
   且带 `peer_id`（有 peer_id ⇒ Noise 已完成），随后 `ReservationReqAccepted`。
3. Safari / Firefox 无法用 CDP 驱动。用一个最小页面拨 `wt.ready` 并把结论写进
   `document.title`，再用 `osascript -e 'tell application "Safari" to get name of document 1'`
   读回来 —— 不需要 safaridriver，也不需要 `do JavaScript` 权限。
4. **每个浏览器都要做负面对照**（喂一个全零 certhash，期望 `Opening handshake failed`）。
   没有它，「WT-OK」可能只是说明那个浏览器压根没校验 certhash。

顺带可验证书持久化：重启 bootstrap 后 certhash 应**逐字不变**，浏览器经同一条地址重连成功。

## 已知负债（勿当 bug 重报）

- mdns/autonat/dcutr 的 native 运行时行为未经自动化测试（依赖真机/多机冒烟）。
- 事件订阅溢出（256 队列满丢弃）只有计数无测试。
- presence 慢测与 LAN helper e2e 沿旧例 `#[ignore]`。
- ~~webrtc-direct 浏览器端到端待 M5 实测~~ **已实测通过（2026-07-18）**：浏览器
  ws/webrtc-direct dial、circuit 被动接收、双向 RPC 五格全通，记录见
  `spike/net-web-smoke/README.md`。wasm 产物 598KB gzip（iroh spike 为 849KB）。
  未测：跨机器、Safari/Firefox、https 页面组合。

**WebTransport（2026-08-12 落地）：**

- ~~真机测量未做~~ **局域网已测（2026-08-12，v0.18.0）**：Android ↔ 桌面 Chrome，2 GB
  单文件，手机发 **20 MB/s**、浏览器发 **9 MB/s**。前者落进 native↔native QUIC 的区间
  （12–23 MB/s）——浏览器在**接收**方向上已不是瓶颈。
  ⚠️ **不要拿它除 0.36–0.96 MB/s 得出倍数**：那个分母来自 `opt-level = "z"` 的旧构建
  （`-Oz` 关掉内联后 WebRTC 的纯 Rust AES-GCM 慢一个数量级，正是改回 `3` 的理由），
  改完之后 WebRTC 那条**没有在真机上重测过**。要倍数就得做同构建同链路的 A/B。
  **跨网仍未测**——局域网数说明不了中转/打洞路径，「打洞 vs webrtc-direct」这个变量
  至今没分离过。
- **发送方向慢 2.2 倍（20 vs 9 MB/s）：归因是「接收端流水线化了、发送端没有」，
  发送端已于 2026-08-12 补上（openspec: pipeline-send-path）。**
  2026-08-10 那轮只拆了接收端（收帧 ‖ 消化），发送端的 `write_block` 一直是
  `读 .await → 建 proof → 发 .await` 的串行链。串行本身两端都有，但代价只在浏览器那侧
  显形：Android 的「读+算」是原生文件读 + NEON blake3，相对网络几乎免费；浏览器的是
  promise 往返 + 无 SIMD 的 wasm blake3，且完全不与网络发送重叠。
  现在发送端也是两条并发路径（备块 ‖ 发帧 + 有界队列 + `join`），与接收端同构。
  已排除的：prepare（Web 端单独追踪 `activePrepare`，9 是纯数据面速率）、OPFS 写盘（收才写
  而收更快）、wasm blake3 本身（两向工作量同量级）、`encode_proof` 的 O(n²)（按 range 走
  O(块 + log n)）、停等窗口 RTT（4 MiB 一窗，2 GB 只停 512 次）。
  ⚠️ 「发送侧多一份跨 JS↔wasm 拷贝」那条说法是**错的**——两向都跨两次，不对称的是**重叠**。
  ⚠️ **天花板是 `proof`**：`join` 给的是并发不是并行，而 `encode_proof` 是 wasm 主线程上的
  同步 CPU，谁也压不住它。每块壁钟从 `read + proof + write` 降到约 `proof + max(read, write)`，
  **实际收益取决于 `proof` 的占比，至今未实测**。量它不用改代码：探针已拆成
  `send`（read/proof/enqueue）与 `send-frame`（queue/write/ack/rest）两条，都打在浏览器
  console 上（`swarmdrop_transfer` 在 Web 端是 DEBUG，探针发 `info!`）。判读表见
  [`2026-08-12-webtransport-field-test.md`](../research/2026-08-12-webtransport-field-test.md)。
- ~~桌面端未接入~~ **已接入（2026-08-12）**，且没有动 `KeychainProvider` —— 判据见下方
  「地址集合只增不删」那节后面的「证书端口为什么不挂在 KeychainProvider 上」。
- ~~旧的公网地址不会被撤销~~ **已修（2026-08-12）**，见下节。
- ~~浏览器端到端未手测~~ **Chrome 完整链路 + Safari/Edge 准入层已实测**（2026-08-12）。
  **Firefox（Gecko）仍未测** —— 本机没装。复现只需一条 URL，见下方「浏览器手测怎么做」。
- **`connection closed by peer: 0` 被记成 WARN。** 浏览器每次刷新/关页都会在服务端刷一条
  `connection closed with error … 建立 WebTransport 会话失败：接受入站子流失败`。
  功能无碍，但正常操作产生 WARN 会淹掉真告警。**没有就地改**：libp2p 的 `StreamMuxer`
  没有「正常结束」的表达位（连接终止一律经 Error 传播），把它在 muxer 层吞成 EOF 有
  连带吞掉真实错误的风险，要改得先确认 quic / webrtc-direct 在同一场景下的表现，
  不能只看 WebTransport 这一条。
- **浏览器端到端未手测**（Chrome / Firefox / Safari 各拨通一次）。native↔native 的 9 条
  集成测试已覆盖握手链路，但浏览器侧的 `serverCertificateHashes` 准入只能手测 ——
  `wtransport` 客户端用的是同一套判据（有效期 ≤14 天、ECDSA P-256、窗口内），
  所以 native 拨得通的东西浏览器**应该**也拨得通，但那是推论不是实测。
