# 网络内核（swarmdrop-net）开发知识

> 覆盖 `crates/net-base` + `crates/net`（2026-07 重构产物，替代 libs/ 的 swarm-p2p-core）。
> 架构设计依据见 `dev-notes/why-libp2p-not-iroh.md`；重构决策过程见当次 plan。

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

### 与旧栈（swarm-p2p-core）的关键差异

| 旧 | 新 | 原因 |
|---|---|---|
| `start::<Req,Resp>()` CborMessage 泛型贯穿 | 无泛型；协议注册在 Router | 业务类型不入网络层 |
| request_response behaviour + PendingMap | stream 上的 `Rpc` + handler 长 await | 三件套机制整体消失 |
| 巨型 NodeEvent 枚举直接进前端 | watch + 小 NetEvent；前端事件由 core 层 CoreEvent 组装 | 事件/状态分轨 |
| 命令责任链（trait 对象穿链） | 扁平 ActorMessage 枚举 + oneshot | 协议数固定，责任链的开闭收益换不回间接成本 |
| kad 路由表兼职地址簿 | actor 自维护 AddressBook | `Swarm::add_peer_address` 只是广播（见坑 3） |

## libp2p git master（pin 93c5059）校准实录

**为什么 git 不是 crates.io**：libp2p-webrtc 0.9.0-alpha.1（crates.io 最新）的
webrtc-direct 实证跑不通，修复只在 master（PR 6429）。**升级 rev 必须走独立 PR +
全量测试 + wasm check**；0.57 正式发布后切回 crates.io。identity/multiaddr 不用跟
git——master 树自己解析到 crates.io（0.2.14 / 0.18.2），net-base 用 crates.io 版本天然 unify。

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

### 坑 6：kad `Record.expires` 的类型按 target 分叉

native = `std::time::Instant`，wasm = web_time（与 `n0_future::time::Instant` 同源）——
写跨平台代码需 cfg 分支（`actor.rs` 的 DhtCommand::Put 有样例）。

### 其余确认

- `with_wasm_bindgen()` 在 master 仍在（删的是 cargo feature，不是方法）。
- websocket phase 依赖 dns feature 的隐式耦合仍在（同开即可）。
- `NetworkBehaviour` derive 的 **cfg 字段**（mdns/autonat/dcutr）双 target 编译均过；
  但 native 行为只有 relay/kad/identify/ping 被测试实证，**mdns/autonat/dcutr 的
  运行时行为待真机冒烟确认**。
- ConnectionHandler 的关联类型（InboundOpenInfo 等）与 0.56 一致，keep_alive
  behaviour 近零改动移植。

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

## 已知负债（勿当 bug 重报）

- mdns/autonat/dcutr 的 native 运行时行为未经自动化测试（依赖真机/多机冒烟）。
- 事件订阅溢出（256 队列满丢弃）只有计数无测试。
- presence 慢测与 LAN helper e2e 沿旧例 `#[ignore]`。
- ~~webrtc-direct 浏览器端到端待 M5 实测~~ **已实测通过（2026-07-18）**：浏览器
  ws/webrtc-direct dial、circuit 被动接收、双向 RPC 五格全通，记录见
  `spike/net-web-smoke/README.md`。wasm 产物 598KB gzip（iroh spike 为 849KB）。
  未测：跨机器、Safari/Firefox、https 页面组合。
