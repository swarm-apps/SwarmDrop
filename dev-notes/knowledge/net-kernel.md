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
- 浏览器在 `docs/app/try/relay-helpers.ts` 使用 WebRTC Direct 或 WSS helper；每项必须附带 `/p2p/<peer-id>`，WebRTC Direct 还必须带稳定的 `certhash`。
- 新公网 relay 同时承担 circuit relay 时，仍需按上一节登记其外部地址；客户端清单只解决“如何拨到它”，不替代服务器侧公告。

**不要做**：
- 不要把某一端可用的 `/ws` 或 `/webrtc-direct` 地址无差别下发给所有端；Android 当前无法拨 WebSocket，而浏览器不能拨 TCP/QUIC。

**相关文件**：`crates/core/src/network/config.rs`、`src/lib/bootstrap-nodes.ts`、`mobile/src/core/bootstrap-nodes.ts`、`docs/app/try/relay-helpers.ts`

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
2. `with_websocket()`——**宏展开硬编码 `libp2p_dns::tokio::Transport::system(tcp)`**
   （`libp2p/src/builder/phase/websocket.rs`），不吃 with_dns_config。修法：Android
   直接跳过 ws（WebsocketPhase 有 `with_relay_client` shortcut，内部 without_websocket）；
   WS listener 本来就是「LanHelper 给浏览器」的桌面场景，移动端无消费方。
   **契约后果**：Android endpoint 对 `/ws`、`/wss` 地址**完全不可拨**（不只是不
   listen）——今天无影响（移动拨桌面走 TCP/QUIC），但属于平台能力不对称，规划
   ws-only 节点时要记得。根因是 libp2p 上游缺口（websocket phase 应复用已配置的
   dns config），已提上游 <https://github.com/libp2p/rust-libp2p/issues/6529>，
   修复后本地可收敛回双分支。

只修 1 不修 2 表现完全一样（同一错误字符串），容易误判「没修上」——先怀疑第二处，
再怀疑 .so 没重编。`NameServerConfig` 需要直接依赖 hickory-resolver（libp2p::dns 只
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
