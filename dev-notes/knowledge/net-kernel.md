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

#### fork 到底比上游多什么（2026-07-27 实测：ahead 7 / behind 0）

**全部自有补丁都已提交上游 PR，无未提项。** 7 个 commit 对账（2 个是 merge commit）：

| fork commit | 补丁 | 上游 PR | 状态（2026-07-27） |
|---|---|---|---|
| `db1bc23e` | `fix(webrtc-websys): defer data channel callback wakes` | [#6558](https://github.com/libp2p/rust-libp2p/pull/6558) | **OPEN** |
| `c7d37a8d` | `feat(webrtc): negotiate data channel message limits` | [#6560](https://github.com/libp2p/rust-libp2p/pull/6560) | **OPEN** |
| `9e3bcd9b` | `docs: add WebRTC message limit changelogs` | #6560（同 PR） | **OPEN** |
| `c4c2c167` + `989cb610` | separate / configure receive buffer limit | #6560 的 `5984c716`（**squash 成单 commit**） | **OPEN** |

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

**阶段 1 — 切回官方 git URL**：两个 PR 都进入 upstream master。

```bash
# 主判据：两个 PR 均为 MERGED —— 此时上游 master 已含全部所需修复
gh pr view 6558 --repo libp2p/rust-libp2p --json state --jq .state
gh pr view 6560 --repo libp2p/rust-libp2p --json state --jq .state
```

两个都 MERGED 后，把三行 git 依赖的 URL 换回 `libp2p/rust-libp2p`、rev 换成上游 master 上
含这两个 PR 的 commit，跑全量测试 + `./scripts/check-wasm.sh`，然后**删掉 fork pin**。

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

自研的打洞传输已接进内核。**默认关**：`EndpointConfig.webrtc_p2p: Option<WebRtcP2pConfig>`，
经 `Builder::webrtc_p2p(..)` 开启；当前只有 Browser profile 开（`crates/core/src/runtime.rs`），
桌面/移动不开——它们有 autonat + dcutr，浏览器才是没有 DCUtR 的那一端。

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

- **wire 契约（`invite.rs`，改动前看 `wire_v1_hex_snapshot` 单测）**：`sdinvite` 前缀 +
  base32-nopad 小写 + postcard 单变体 enum `InviteWire::V1`（判别码 `0x00` 即版本，未知变体
  解码即失败）。**签名尾置**——`InviteV1.signature` 是末位定长 64 字节，signable =
  `bytes[..len-64]` 覆盖含版本判别码在内的全部前置字节（防降级），验签公钥从 `inviter_id`
  的 identity multihash 就地恢复。字段序即契约，V1 发布后不可改。
- **一次性/TTL**：`InviteRegistry`（发起端内存态）只存 `sha256(capability)`；入站 handle
  非消费预检 + respond(Success) 原子 CAS `Pending→Consumed`（两台扫同码仅先确认者成功）。
- **QR 三端统一（`qr.rs`，唯一编码源）**：喂 fast_qr 前把**整串（含 `sdinvite` 前缀）**
  `.to_ascii_uppercase()` → 落 QR alphanumeric 模式（byte 模式 v13-15 降 v11-12，模块 -15%）；
  ECL::M + 4 模块 quiet zone。三端渲染 core 出的 SVG/矩阵（桌面/web 用 `invite_qr_svg`、
  RN 用 `invite_qr_matrix` + react-native-svg），**深模块 + 白底不随暗色反色**。
  ⚠️ **整串大写含前缀**，故 `decode` 对前缀**必须大小写不敏感**——`strip_prefix("sdinvite")`
  曾大小写敏感，扫码得到的 `SDINVITE…` 100% 解不出（粘贴走小写规范串侥幸没暴露，移动扫码落地
  才发现）；已修（`invite.rs` 前缀 `eq_ignore_ascii_case` 回退）+ 补「整串大写 / 混排前缀」回归
  断言（`roundtrip_and_case_insensitive`）。payload 段本就大小写不敏感。
- **三端接线**：桌面命令 `generate_pair_invite`/`decode_pair_invite`/`invite_qr_svg`/
  `consume_pair_invite`；mobile uniffi 同名 + `pair_direct`（补回 Direct）+ `invite_qr_matrix`；
  web `WebNode::connect_invite`（decode 纯函数只需 net-base）。剪贴板感知（`hasStringAsync`
  探测亮 chip）与移动扫码（expo-camera `CameraView`：`barcodeTypes:["qr"]` + 前缀校验 +
  `lockRef` 一次性闸 + 权限三态 + AppState 回前台重拉）均已落地（`mobile/src/app/pairing/scan.tsx`）；
  原生 `CameraView` 需 `expo prebuild` 重编。

## 已知负债（勿当 bug 重报）

- mdns/autonat/dcutr 的 native 运行时行为未经自动化测试（依赖真机/多机冒烟）。
- 事件订阅溢出（256 队列满丢弃）只有计数无测试。
- presence 慢测与 LAN helper e2e 沿旧例 `#[ignore]`。
- ~~webrtc-direct 浏览器端到端待 M5 实测~~ **已实测通过（2026-07-18）**：浏览器
  ws/webrtc-direct dial、circuit 被动接收、双向 RPC 五格全通，记录见
  `spike/net-web-smoke/README.md`。wasm 产物 598KB gzip（iroh spike 为 849KB）。
  未测：跨机器、Safari/Firefox、https 页面组合。
