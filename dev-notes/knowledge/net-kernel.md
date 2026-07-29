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
- `remove_relay_intent` 的直接注销调用是低延迟快路径，环是兜底，二者幂等叠加。

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

#### fork 到底比上游多什么（2026-07-28 更新）

**全部自有补丁都已提交上游 PR，无未提项。**

| fork commit | 补丁 | 上游 PR | 状态（2026-07-28） |
|---|---|---|---|
| `db1bc23e` | `fix(webrtc-websys): defer data channel callback wakes` | [#6558](https://github.com/libp2p/rust-libp2p/pull/6558) | **OPEN** |
| `c7d37a8d` | `feat(webrtc): negotiate data channel message limits` | [#6560](https://github.com/libp2p/rust-libp2p/pull/6560) | **OPEN** |
| `9e3bcd9b` | `docs: add WebRTC message limit changelogs` | #6560（同 PR） | **OPEN** |
| `c4c2c167` + `989cb610` | separate / configure receive buffer limit | #6560 的 `5984c716`（**squash 成单 commit**） | **OPEN** |
| `262dea51` | `fix(relay): don't panic on circuit request without a matching reservation` | [#6570](https://github.com/libp2p/rust-libp2p/pull/6570)（issue [#6569](https://github.com/libp2p/rust-libp2p/issues/6569)） | **OPEN** |

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

**唯一确实未进 PR 的内容**：`misc/webrtc-utils/src/stream.rs` 里 3 行文档注释（说明
transport-local 聚合接收缓冲为何与协商的单条消息上限相互独立）。纯注释、零功能影响，
上游合并后随 rebase 自然消失，**不构成退出阻塞**。

#### 退出条件（两阶段，各自可判定）

**阶段 1 — 切回官方 git URL**：三个 PR 都进入 upstream master。

```bash
# 主判据：三个 PR 均为 MERGED —— 此时上游 master 已含全部所需修复
gh pr view 6558 --repo libp2p/rust-libp2p --json state --jq .state
gh pr view 6560 --repo libp2p/rust-libp2p --json state --jq .state
gh pr view 6570 --repo libp2p/rust-libp2p --json state --jq .state   # relay panic
```

三个都 MERGED 后，把**五行** git 依赖（libp2p / -stream / -core / -swarm / -webrtc-utils）的
URL 换回 `libp2p/rust-libp2p`、rev 换成上游 master 上含这三个 PR 的 commit，跑全量测试 +
`./scripts/check-wasm.sh`，然后**删掉 fork pin**。

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

**正确做法**：
- 组合根在 `Endpoint::bind()` 前经 `Builder::external_addrs()` 登记已知公网
  TCP / QUIC / WebSocket 地址；它们同时成为 `watch_addrs().external` 初值。
- 运行期得到的地址经 `Endpoint::add_external_addr()` 登记；actor 同步更新同一
  watch 状态并通知 address lookup。
- WebRTC Direct 使用与 transport 完全相同的持久化 PEM，通过
  `webrtc_direct_addr_from_pem()` 预先派生带 `certhash` 的公网地址，**不要**等待
  listener 启动后从字符串猜 hash。

**相关文件**：`crates/net/src/{endpoint/{builder.rs,mod.rs},actor.rs,lib.rs}`、
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

### 其余确认

- `with_wasm_bindgen()` 在 master 仍在（删的是 cargo feature，不是方法）。
- websocket phase 依赖 dns feature 的隐式耦合仍在（同开即可）。
- `NetworkBehaviour` derive 的 **cfg 字段**（mdns/autonat/dcutr）双 target 编译均过；
  但 native 行为只有 relay/kad/identify/ping 被测试实证，**mdns/autonat/dcutr 的
  运行时行为待真机冒烟确认**。
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

### DataChannel 的 `Connecting` 不是错误（wasm 侧）

`PeerConnection` 的 `connected`（DTLS 完成）**早于** DataChannel 的 `open`（SCTP 完成）。
muxer 一交出去上层就开始写，此刻必然还是 `Connecting`——把它当写错误会让刚建立的打洞连接
立刻刷屏报错，实际只差几十毫秒。正确做法是注册 waker 返回 `Pending`，并配 `onopen` 回调
唤醒（**没有 onopen 就永远等不到通知**）。native 侧无此问题：`webrtc-rs` 的 `send` 是
async，内部等 SCTP。

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

修复已提上游 <https://github.com/webrtc-rs/rtc/pull/137>，根 `Cargo.toml` 的
`[patch.crates-io]` 临时 pin 个人 fork，**退出条件写在那段注释里**（两条可判定命令）。

### 上游缺口台账（2026-07-28）

做 direct 期间踩到的上游问题都已提出去。**关键区分：只有下表标「阻塞」的两项影响本仓的
依赖 pin**，其余是反哺与待改进——看到「5 个上游 PR」不要以为退出条件从 3 个变成 5 个。

| 仓 | 编号 | 内容 | 对本仓的意义 |
|---|---|---|---|
| webrtc-rs/rtc | [PR 137](https://github.com/webrtc-rs/rtc/pull/137) | `disable_certificate_fingerprint_verification` 是死代码 | **阻塞** — direct 服务端没它建不起来 |
| libp2p/rust-libp2p | [PR 6472](https://github.com/libp2p/rust-libp2p/pull/6472)（上游自己的） | relay circuit 无 reservation 时 panic | **阻塞** — 与 #6558/#6560 同属 git pin 退出条件 |
| webrtc-rs/**rtc** | [PR 140](https://github.com/webrtc-rs/rtc/pull/140) | `RTCDataChannelInit` 的 `ordered` 默认成 `false`（issue 139） | 反哺 — 本仓已在自己这侧显式传参，不进 pin |
| webrtc-rs/webrtc | [PR 825](https://github.com/webrtc-rs/webrtc/pull/825) | `on_data_channel` 把本端开的通道也报上来 | **已 pin**（见下）；muxer 的 `local_channels` 仍保留，它是不变式不是补丁 |
| webrtc-rs/**rtc** | [PR 138](https://github.com/webrtc-rs/rtc/pull/138) | `send()` 在通道 open 前/关闭后返回 `Ok` 但**静默丢数据**（issue 826） | **已 pin**；`data_channel::await_open` **无论如何都要留** |
| webrtc-rs/webrtc | [PR 828](https://github.com/webrtc-rs/webrtc/pull/828) | 加 `remote_certificate_fingerprint`（issue 827） | **已 pin**，`remote_fingerprint()` 收成一行 |
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

### webrtc 也 pin 了 fork（2026-07-28）

`[patch.crates-io]` 现在有**两条**：`rtc` 与 `webrtc`。后者指向集成分支
`yexiyue/webrtc@swarmdrop-integration` = upstream/master + PR 825 + PR 828 + 一行本地改动。

⚠️ **那一行本地改动不能省，也不能带进上游 PR**：上游 `webrtc` 用
`rtc = { version = "...", path = "rtc" }` 指向 submodule，而 **`[patch.crates-io]` 不作用于
path 依赖**。保留 path 的话，`webrtc` 的 rtc 来自 submodule、`webrtc-p2p` 的 rtc 来自 patch，
两个 source id = 两个互不兼容的同名 crate，`webrtc` 返回的类型对不上 `use rtc::...` 的类型，
直接编译失败。集成分支把 path 去掉，让它也从 crates.io 解析、被同一条 patch 命中。

验证收敛的命令（应只有一行 rtc）：

```bash
cargo tree -p webrtc-p2p -i rtc
```

退出条件写在根 `Cargo.toml` 那段注释里（两个 PR 均 MERGED 即可删）。

### webrtc 0.20 没有 UDPMux —— 改从 `Runtime::wrap_udp_socket` 注入

0.17 的 `UDPMux` / `UDPMuxWriter` / `UDPMuxConn` 体系在 0.20 整个消失
（`SettingEngine::set_udp_network` 在 rtc 里已是**注释掉的 TODO**）。

替代注入点更下层也更干净：`Runtime::wrap_udp_socket` 决定 `PeerConnection` 用哪个
socket。于是复用一个端口的做法变成「给每条连接发一个假 socket」——发包转给共享
socket，收包从自己的支路取。官方那 579 行 `udp_mux.rs` 里的 trait 适配层随之消失，
只剩真正的分流逻辑（约 250 行）。

**分流依据**：首包按 STUN `USERNAME` 里的 local ufrag（`<对端>:<本端>`，取**冒号前**
那一半），其余按源地址。

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
`rtc-ice` 看似等价，但 **rtc 一旦被 `[patch]` 换成 git 源**，直接依赖仍从 crates.io
解析，同名类型就分叉成两个，报「expected `rtc::rtc_ice::X`, found `rtc_ice::X`」
这种极绕的错。经 `rtc::` 转一手天然同源。

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
`crates/webrtc-p2p/examples/direct_listener.rs`、根 `Cargo.toml` 的 `[patch.crates-io]`

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
  （如给 `start_node` 加参数触发的 `too_many_arguments`）会在 wasm job 变硬错误挂 CI。
  提交前对 wasm 侧改动跑 `bash scripts/check-wasm.sh --clippy`，别只信本机 clippy 绿。

## wire v2 契约点（改动前先看固化测试）

- net-base 的 serde 表示是 IPC/wire 契约：NodeId/Addr 字符串、状态枚举 camelCase
  （`status.rs` / `node_id.rs` / `addr.rs` 的契约测试）。
- `DhtKey::namespaced` 带长度前缀域分离（纯拼接下 `("ab","c")==("a","bc")`，
  旧栈同缺陷已修）——**改派生规则 = 分享码/在线宣告全部失配**。
- transfer 数据面 `BlockData.proof` = bao-tree 逐块验签切片（u8 标志 + 可选 len-prefixed
  bytes）。**已启用（2026-07-18）**，不再恒 None：接入未 bump 协议版本（proof 是 opaque
  bytes，wire 布局不变）。选型 Approach B——proof 携完整 bao 切片、`data` 置空（叶子只出现
  一次、无 2x 冗余）；root == `FileInfo.checksum`（标准 blake3，`BlockSize::from_chunk_log(4)`
  下 chunk group 不改 root）；proof 缺失/验签失败 = 协议违规 → 断流走 Interrupted 恢复。
  发送端 outboard 与 checksum 同一遍流式构建、落 `transfer_files.outboard` 供 resume 免重算。
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
- **地址瘦身**：每类网络分别只留 TCP（无则 QUIC，native）和 WebRTC（浏览器）各一条；
  两类路径从该网络分类的全部地址中独立挑选，避免 TCP-only 网卡排在前面时误删 WebRTC。
  Auto 最多保留 100.64/10 overlay（Tailscale）、LAN、公网与 relay 的 native/WebRTC
  各一条；198.18/15 仅在没有 overlay 时回退。LocalOnly 只保留 LAN 的 native/WebRTC。
- **三端接线**：桌面命令 `generate_pair_invite`/`decode_pair_invite`/`invite_qr_svg`/
  `consume_pair_invite`；mobile uniffi 同名 + `pair_direct`（补回 Direct）+ `invite_qr_matrix`；
  web `WebNode::connect_invite`（decode 纯函数只需 net-base）。剪贴板感知（`hasStringAsync`
  探测亮 chip）与移动扫码（expo-camera `CameraView`：`barcodeTypes:["qr"]` + 前缀校验 +
  `lockRef` 一次性闸 + 权限三态 + AppState 回前台重拉）均已落地（`mobile/src/app/pairing/scan.tsx`）；
  原生 `CameraView` 需 `expo prebuild` 重编。

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

## 已知负债（勿当 bug 重报）

- mdns/autonat/dcutr 的 native 运行时行为未经自动化测试（依赖真机/多机冒烟）。
- 事件订阅溢出（256 队列满丢弃）只有计数无测试。
- presence 慢测与 LAN helper e2e 沿旧例 `#[ignore]`。
- ~~webrtc-direct 浏览器端到端待 M5 实测~~ **已实测通过（2026-07-18）**：浏览器
  ws/webrtc-direct dial、circuit 被动接收、双向 RPC 五格全通，记录见
  `spike/net-web-smoke/README.md`。wasm 产物 598KB gzip（iroh spike 为 849KB）。
  未测：跨机器、Safari/Firefox、https 页面组合。
