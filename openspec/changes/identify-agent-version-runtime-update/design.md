# identify-agent-version-runtime-update 设计

消除 `device-config-port`（C5）留下的最后一条限制：**改名要重启节点**。目标是改完之后已连接的
对端在一个 RTT 内看到新名字，任何一端都不重启、不断连、不刷新页面。

依赖 C5（`DeviceConfig` 端口、`DeviceName` newtype、`PairingManager` 持本机 `OsInfo`）。本文凡
提到这三样，签名以 C5 的 artifacts 为准，本 change 只调用不定义——唯一的例外是 `OsInfo` 的
**可变性**：C5 D10 明确把「可变句柄长什么样」留给了本 change（见 D7）。

> 代码引用按 `develop`（`3a309b99`）与 fork rev `262dea51` 逐条核对过。与立项材料不一致的地方
> 集中记在文末「核对修正」。

---

## D1：为什么必须动 libp2p fork，而不是在 `crates/net` 里绕

立项判断是「agent_version 锁在私有 `Behaviour.config` 里，加个 setter 就行」。核对后发现**只对
了一半**，而漏掉的那一半恰恰是技术核心：`agent_version` 在**每条连接建立时**被 clone 进该连接
的 `Handler`。

```
protocols/identify/src/behaviour.rs:399-403   handle_established_inbound_connection
protocols/identify/src/behaviour.rs:432-436   handle_established_outbound_connection
    Handler::new(…, self.config.agent_version.clone(), …)
                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
protocols/identify/src/handler.rs:93          agent_version: String,   ← 每条连接一份
protocols/identify/src/handler.rs:242         build_info() 读的是 self.agent_version
```

所以只开 config setter 是无效的：已建立的连接照样报旧名字，而「已连接的对端看到新名字」正是
本 change 的验收条件。

四个选项：

| 选项 | 做法 | 判定 |
|---|---|---|
| A | 只在 `crates/net` 改 `EndpointConfig.agent_version` | **无效**。Behaviour 建成后不再读 config（`behaviour/mod.rs:74` 只在构造期读一次），改了没人看 |
| B | 运行时重建 identify Behaviour | **不可能**。`#[derive(NetworkBehaviour)]` 出来的字段不支持热换；即使能换，也会丢掉 `discovered_peers`（PeerCache）与 `connected` 表，等价于一次局部重启 |
| C | 不用 agent_version，另起一个自研协议广播设备名 | 可行但代价大：要设计 wire、要处理「对端不支持」的降级、要新增一条重连后的同步路径。而 `OsInfo::to_agent_version()` / `from_agent_version()`（`crates/host/src/device.rs:240` / `:278`）与三端的消费路径全都建立在 agent_version 上，等于把一条已跑通的链路推倒重来 |
| D | 保持现状（重启节点） | 就是本 change 要消灭的东西 |

**结论：打 fork 补丁**，而且没有第二条路——A/B 物理上不成立，C 是另一个量级的改动。

好在补丁**有现成的同形先例**可抄：`InEvent::AddressesChanged` 就是「Behaviour 侧状态变了 →
逐连接下发给 handler」（`behaviour.rs:545-559`）：

```rust
let change_events = self.connected.iter()
    .flat_map(|(peer, map)| map.keys().map(|id| (*peer, id)))
    .map(|(peer_id, connection_id)| ToSwarm::NotifyHandler {
        peer_id,
        handler: NotifyHandler::One(*connection_id),   // ← 是 One 不是 Any
        event: InEvent::AddressesChanged(self.all_addresses()),
    })
    .collect::<Vec<_>>();
```

`NotifyHandler::One(connection_id)` 而不是 `Any`：一个 peer 可能同时有多条连接（TCP + QUIC、
relay + direct，`crates/net` 的 `Actor.conns` 就是 `HashMap<PeerId, Vec<(ConnectionId, ConnInfo)>>`
——`actor.rs:141`），`Any` 只命中其中一条，剩下的仍带旧值。本补丁照抄这个形状。

---

## D2：fork 补丁的形状与最小面

**新增两处，零改动既有行为：**

```rust
// protocols/identify/src/handler.rs（InEvent 当前在 :107-111）
pub enum InEvent {
    AddressesChanged(HashSet<Multiaddr>),
    AgentVersionChanged(String),          // ← 新增
    Push,
}

// on_behaviour_event（:321-336）
InEvent::AgentVersionChanged(v) => self.agent_version = v,
```

```rust
// protocols/identify/src/behaviour.rs，紧邻 push（:280）
impl Behaviour {
    /// Updates the agent version advertised to peers.
    ///
    /// Existing connections are updated in place: the new value is used for the next
    /// identify exchange on every connection. Combine with [`Behaviour::push`] to
    /// propagate the change immediately.
    pub fn set_agent_version(&mut self, agent_version: String) { … }
}
```

四个刻意的取舍：

**① 不自动 push。** `Behaviour::push`（`behaviour.rs:280`）早就是公开 API。让 set 与 push 正交，
调用方自己决定「立刻广播」还是「等下次周期交换」。这也与 identify 既有的
`push_listen_addr_updates`（opt-in 配置，`behaviour.rs:561`）风格一致——库不替用户定推送时机。
`crates/net` 侧把两者组合起来（D3 / D4）。

**② 不顺带做 `set_protocol_version`。** protocol_version 是**兼容性判别码**（本仓用
`/swarmdrop/2.0.0`，`crates/core` 拿它筛 SwarmDrop 节点，见 `event_loop.rs:115` 的
`protocol != IDENTIFY_PROTOCOL`），运行时改会让对端的兼容判断在连接中途翻转，语义上是另一回事，
且没有用例。补丁面越小，上游接受概率越高。

**③ 值未变时短路 return。** 否则一次无意义的 set 会往 `self.events` 塞 N 条 `NotifyHandler`
（N = 连接数）并触发 N 次 push。与 `refresh_os_info`（`crates/host/src/device.rs:358-363`）的
相等短路同一思路，也是 spec 里「幂等调用不产生网络行为」那条的实现依据。

**④ 通用化，不带任何 SwarmDrop 语义。** 英文 doc comment、参数是裸 `String`、补丁内不出现
「device name」这类上层概念——为 D11 的上游 PR 留口子。

---

## D3：主动 push vs 等下次周期交换（任务书点名的问题）

**周期交换的延迟是 5 分钟。** fork 的 `Config` 默认 `interval: Duration::from_secs(5 * 60)`
（`behaviour.rs:183`），而 `crates/net/src/behaviour/mod.rs:72-77` **没有覆盖它**（只设了
agent_version / push_listen_addr_updates / cache_size）。纯靠周期交换，用户改完名最坏要等
5 分钟才在对方设备列表里生效——「不用重启了，但要等五分钟」是个尴尬的半成品。

**主动 push 的开销：** 每条连接开一次 `/ipfs/id/push/1.0.0` 出站流，负载是完整 `Info`
（公钥 + protocol_version + agent_version + 地址集 + 协议列表，量级几百字节到 ~1–2 KB）。
`Behaviour::push` 自身按 `self.connected` 过滤（`behaviour.rs:285-288`），对未连接的 peer 不做
无谓工作。

| | 延迟 | 开销 | 结论 |
|---|---|---|---|
| 只等周期交换 | ≤ 5 min | 0 | 作为兜底保留（D8） |
| 主动 push | 一个 RTT | O(已连接 peer 数) × 一条短流 | **选它** |

**改名是低频人工动作**（一个用户一辈子改几次），拿一次 O(peer 数) 的短流换掉 5 分钟延迟，
性价比毫无疑问。真正需要担心开销的是「频繁自动改名」，那个场景不存在。

**不对 bootstrap / relay 节点做特殊处理。** 它们本来就在 `connected` 里，会跟着收到 push。
agent_version 对它们无语义，但一条流的浪费不值得引入「哪些 peer 该 push」的分类逻辑——那种
逻辑一旦引入就会长出第二个真值。

**明确否掉的方案：改名后主动断开重连。** 那是「重启节点」的缩小版，仍然丢连接、丢 reservation、
丢在途传输，正是本 change 要消灭的东西。

---

## D4：set 与 push 的**顺序**不可交换（最容易埋的暗坑）

`Behaviour::push` 用的是 `NotifyHandler::Any`（`behaviour.rs:292`），也就是「这个 peer 的任意
一条连接」。而 `set_agent_version` 用 `One` 逐条更新。两者组合起来只有一个正确顺序：

```
① set_agent_version(v)  → 往 events 队列压 N 条 NotifyHandler::One(AgentVersionChanged)
② push(peers)           → 往 events 队列压 M 条 NotifyHandler::Any(Push)
```

`self.events` 是 `VecDeque`，`poll` 按 FIFO 逐条弹给 Swarm，Swarm 同步派发到对应 handler 的
`on_behaviour_event`。所以「先 set 后 push」保证 **Push 到达任意一条连接时，那条连接的
`self.agent_version` 已经是新值**——`build_info()`（`handler.rs:239-247`）读的正是它。

反过来（先 push 后 set）会推出旧值，且失败是静默的：对端收到一次「内容没变」的 push，日志里
一切正常，名字就是不更新，5 分钟后才被周期交换纠正。**`crates/net` 的 actor 分支里必须写死这个
顺序并加注释**，`crates/net/tests/identify_agent_version.rs` 里那条「秒级收到新值」的断言就是它
的回归防线。

另一个附带结论：`push` 用 `Any` 是**够用的**。对端的 `Info` 是 per-peer 语义，一条连接上收到
push 就会抛一次 `Event::Received`，`crates/net` 据此发一条 `PeerIdentified`。不需要为了 push 也
改成 One——那只会让对端对同一次改名收到 N 条重复事件。

---

## D5：对端接收路径**零改动**（含证据链）

任务书问「接收路径是否要跟着改」。核对结论是**不需要**，因为 push 与周期交换在接收侧走同一个出口：

```
handler.rs:385   Success::ReceivedIdentifyPush(PushInfo)
      ↓ info.merge(push_info)                 protocol.rs:61-80，agent_version 是 Option，有就覆盖
      ↓ handle_incoming_info                  handler.rs:251-261
      ↓ ConnectionHandlerEvent::NotifyBehaviour(Event::Identified(info))
                                              ← 与周期交换的 ReceivedIdentify 完全同一个变体
behaviour.rs     Event::Received { peer_id, info, .. }
      ↓
crates/net/src/actor.rs:1042  BehaviourEvent::Identify(identify::Event::Received {..})
      ↓ emit NetEvent::PeerIdentified { agent: info.agent_version, .. }   （:1054-1064）
      ↓
crates/core/src/network/event_loop.rs:84-94  refresh_paired_device_from_identify
      ↓ :92  OsInfo::from_agent_version(agent)
      ↓ crates/core/src/pairing/manager.rs:468  refresh_paired_device_os_info
      ↓ crates/host/src/device.rs:358-363       refresh_os_info —— 相等则返回 false（短路）
      ↓
crates/core/src/network/event_loop.rs:76-81   CoreEvent::PairedDeviceAdded { device }
      ↓ host 各自持久化（桌面 host/event_bus.rs:111 / 移动 events.rs:196 / Web event_bus.rs:60）
```

三条结论：

1. **push 与周期交换对上层不可区分**——都收敛到 `Event::Received`，`crates/net` 与 `crates/core`
   都不需要知道这次是推来的还是轮到的。
2. **`refresh_os_info` 的相等短路天然去重**：push 之后紧跟着的周期交换带同样的 agent_version，
   不会重复发 `PairedDeviceAdded`，也就不打扰持久化层。
3. **`PairedDeviceAdded` 本来就是「设备信息刷新」通道**，不是「新配对了一台设备」——
   `event_loop.rs:77` 的注释与 `mobile-core/src/events.rs:192-195` 的注释都写明了这一点。
   所以改名在对端不需要新事件类型。

**但要补测试钉死。** 这条链路目前**零端到端覆盖**，而它一旦断（比如有人在 event_loop 里加个
「只在首次 identify 时刷新」的优化）就是静默失效：用户改名，对方永远看不到，没有任何报错。

---

## D6：编排放哪——core 的自由函数，不是 `NetManager` 的方法

`rename_device` 需要四个协作者：`DeviceConfig` 端口（落盘）、本机 `OsInfo`（内存态）、`Endpoint`
（推 identify）、`EventBus`（发事件）。放哪有两个候选：

| | 形态 | 问题 |
|---|---|---|
| A | `NetManager::rename_device(&self, …)` | `NetManager::new` 已经是 8 参数并挂着 `#[expect(clippy::too_many_arguments)]`（`network/manager.rs:49-63`），还要再收一个 `device_config` 并把本来只是穿过去的 `event_bus` 留成字段——为一个功能往已经标注「参数太多」的构造器上加，是往烂处走。**更要命的是它答不了「节点没起时怎么改名」**——那时根本没有 `NetManager`，于是三端各写一遍 `if let Some(m) = manager { … } else { device_config.save(…) }`，与本 change 的立意直接冲突 |
| **B** | **`crates/core/src/device_name.rs` 的自由函数，收 `Option<&NetManager<T>>`** | 两条分支都在 core 内；`NetManager` 一个字段不加；宿主只有一次调用 |

选 B：

```rust
// crates/core/src/device_name.rs
pub async fn rename_device<T: TransferRuntime>(
    name: Option<DeviceName>,               // C5 的 newtype，归一化已在类型里完成
    device_config: &dyn DeviceConfig,       // C5 的端口
    events: &dyn EventBus,
    net: Option<&NetManager<T>>,            // None = 节点未启动（onboarding / 设置页早于 start）
) -> AppResult<()>
```

三端持有的东西天然对得上这个签名：桌面 `NetManagerState = Mutex<Option<NetManager>>`
（`src-tauri/src/network.rs:15`）、移动 `MobileCore.net_manager: Mutex<Option<NetManager<…>>>`
（`app.rs:32`）都已经是 `Option`，Web 的 `WebNode` 直接传 `Some(&self.net_manager)`。
函数在 `crates/core` 内部，可以直接用 `NetManager` 的私有字段，**不需要为它开任何新 accessor**。

**四步顺序是有理由的**：

```
1. device_config.save_device_name(name).await?      ← 失败即整体失败，后续一步不做
2. let os_info = net.pairing.set_device_name(name)  ← 内存态（配对请求 / 新邀请的名字来源）
3. net.endpoint.set_agent_version(os_info.to_agent_version()).await?
4. events.publish(CoreEvent::DeviceRenamed { … }).await
```

**为什么持久化在最前**：反过来（先广播再落盘）一旦落盘失败，用户会看到「改成功了」，下次启动
却变回旧名字——**名字自己回滚**是最难向用户解释的状态。当前顺序的失败模式是可自愈的：第 1 步
成功、第 3 步失败（现实中只可能是 actor 已关停），持久化已生效，节点下次启动自然带上新名字。
**把不可自愈的那一步放在最前**，这是原子性做不到时的次优解。

**为什么发事件而不是让调用方自己刷 UI**：设备名在三端都有多处显示（设置页、设备卡片、onboarding
回显），还有非 UI 消费者（桌面 MCP server 的设备信息资源）。`CoreEvent::DeviceRenamed {
name: Option<String>, display_name: String }` 里的 `display_name` 直接给前端用，省得每端再写一遍
`name?.trim() || hostname`（那个回退现在在 `src/lib/device-name.ts:11-16` 与
`mobile/src/lib/device-name.ts:11-16` 各有一份，而 core 侧 `OsInfo::display_name()`
（`crates/host/src/device.rs:194`）本来就是它）。

---

## D7：本机 `OsInfo` 的可变句柄形状（承接 C5 D10）

C5 D10 写得很明确：「本机 OsInfo 在节点生命周期内不变……C6 要引入的是『net actor 收命令改
agent_version』那条通路，届时可变句柄的形状由那条通路的需要决定」。现在来定。

**存储形态：`std::sync::RwLock<OsInfo>`。** 不用 tokio 的 `RwLock`，也不引入 `arc-swap`：

- 与仓内既有风格一致——`crates/core/src/network/manager.rs:1` 就是 `use std::sync::{Arc, RwLock}`。
- 读者是 `request_pairing`（`pairing/manager.rs:274`）与 `encode_invite`，都是「clone 一份就走」，
  临界区里没有 `.await`。用 std 锁反而**多一层编译期保护**：`RwLockReadGuard` 不是 `Send`，
  谁要是把它跨 `.await` 持有，future 立刻不满足 `Send`，编译期就红。
- `crates/core` 当前依赖里没有 `arc-swap`，为一个每天最多写几次的字段引入新依赖不划算。

**写口只接受 name，不提供整包替换。**

```rust
impl PairingManager {
    /// 更新本机设备名，返回更新后的完整 OsInfo（供调用方重算 agent_version）。
    pub fn set_device_name(&self, name: Option<DeviceName>) -> OsInfo { … }
}
```

**为什么不是 `set_os_info(OsInfo)`**：`to_agent_version()` 拼出来的串里还带着
`caps=lan-helper`（`crates/host/src/device.rs:247-250`，由 `runtime.rs:100-103` 按
`provide_lan_helper` 叠加），而对端靠它决定要不要把这台机器登记成 LAN Helper：

```
crates/core/src/network/event_loop.rs:123
if !os_info.has_capability(OsInfo::LAN_HELPER_CAPABILITY) { return; }
   ↓ 否则进 candidates 并 add_infrastructure_peer
```

一个整包 `set_os_info` 意味着某天有人从别处 new 一个 `OsInfo` 传进来（比如手边正好有个
`OsInfo::native()`），改一次名就把 `caps=lan-helper` 抹了——本机从别人的 LAN Helper 名单里静默
消失，表现是「同网发现忽然变慢了」，几乎不可能定位到改名这一步。窄写口让这件事**结构上无法发生**：
调用方给不了 capabilities，也给不了 hostname / os / arch。

同一条理由也解释了返回值：`rename_device` 需要的是**改完之后的完整 OsInfo**，而不是自己拼一个
——`agent_version` 必须由同一个真值重算。

---

## D8：push 可能被静默丢弃——竞态与兜底

`handler.rs:385-402` 的接收分支有一个前置条件：

```rust
Ok(Ok(Success::ReceivedIdentifyPush(remote_push_info))) => {
    if let Some(mut info) = self.remote_info.clone() {   // ← 没有 remote_info 就整条丢弃
        info.merge(remote_push_info);
        …
    }
}
```

`remote_info` 只有在该连接完成过至少一次**完整** identify 之后才是 `Some`
（`handle_incoming_info`，`handler.rs:251-261`）。所以存在一个窄竞态：**连接刚建立、对端还没
完成首次 identify 时收到我们的 push → 静默丢弃**。

**兜底是周期交换**（≤ 5 分钟）。**不加重试**：一旦为 push 加确认 / 重试，identify 就从「尽力
而为的信息交换」变成「需要 ack 的状态同步协议」，与它的设计语义相悖，也会把一台重试状态机塞进
一个本该无状态的地方。

代价的实际形状：用户在**刚连上对方的那一两秒内**改名，对方最多晚 5 分钟看到。窗口小到可以接受，
且失败模式是「慢」不是「错」——名字最终一定一致。spec 里把它写成显式场景，避免将来有人把它
当 bug 修成重试。

---

## D9：`crates/net` —— actor 命令与真值归属

**必须走 actor 命令。** `crates/net` 的核心心智是「所有可变状态在后台 actor 里，`Endpoint` 只持
命令通道 + watch 读端」，`Swarm` 被 actor 独占，外部拿不到 `behaviour_mut()`。所以：

```rust
// actor.rs：ActorMessage（:43-105）新增
SetAgentVersion { agent_version: String, reply: oneshot::Sender<Result<(), Error>> }

// handle_message（:219 起）新增分支
ActorMessage::SetAgentVersion { agent_version, reply } => {
    // 顺序不可交换，见 design D4
    self.config.agent_version = agent_version.clone();
    self.swarm.behaviour_mut().identify.set_agent_version(agent_version);
    let peers: Vec<PeerId> = self.conns.keys().copied().collect();
    self.swarm.behaviour_mut().identify.push(peers);
    let _ = reply.send(Ok(()));
}

// endpoint.rs
pub async fn set_agent_version(&self, agent_version: String) -> Result<(), Error> {
    self.request(|reply| ActorMessage::SetAgentVersion { agent_version, reply }).await
}
```

沿用 `add_external_addr`（`endpoint.rs:227-230`）的既有形状：`request` helper（:399）+ `oneshot`
回执 + `Error::Closed` 语义，无新概念。`self.conns` 是 `HashMap<PeerId, Vec<(ConnectionId, ConnInfo)>>`
（`actor.rs:141`），`keys()` 正是「当前已连接的 peer 集合」。

**真值归属必须说清楚。** 补丁之后 `agent_version` 有两个副本：

| 位置 | 角色 |
|---|---|
| `identify::Behaviour.config.agent_version` | **权威**。新连接的 handler 从这里 clone（`behaviour.rs:403` / `:436`） |
| `Actor.config.agent_version`（`EndpointConfig`，`actor.rs:165`） | 内核自己的镜像，用于诊断日志与 `Debug` |

两者**必须在同一条命令里一起更新**，否则会出现「日志里的名字和线上广播的不一样」——最难查的
一类偏差，因为两边各自都自洽。`config.rs:113-114` 的字段注释要从「构造期配置」改成「运行时可变，
权威在 identify Behaviour」，`endpoint/builder.rs:70-73` 的 setter doc 补一句指向运行期入口。

**方法名用 `set_agent_version`，不用 `set_device_name`。** `crates/net` 是平台中立的网络内核，
它不知道 agent_version 里装的是设备名——那是 `OsInfo::to_agent_version()` 的约定，属于
`crates/host` 与 `crates/core`。内核层用协议术语，业务层用业务术语，两层名字不同是刻意的。

---

## D10：三端入口——删掉前端编排，不保留「重启」退路

前端只留一个调用：

| 端 | 入口 | 改动 |
|---|---|---|
| 桌面 | `commands::set_device_name`（命令名保留） | 语义从「只写 json」变成「写盘 + 即时生效」；`identity.rs:50-56` 那段「前端自己调 shutdown + start」的 doc **整段重写** |
| 移动 | mobile-core 新增 `rename_device` uniffi 导出 | `mobile/src/lib/device-name.ts:38-49` 整段删除；缓存写入（:36）挪到 core 成功之后 |
| Web | `WebNode::rename_device` | `node-runtime.ts` 加 `renameDevice` 包装；C5 在 `node-panel.tsx` 留下的「刷新页面后生效」提示删除 |

**命令名保留 `set_device_name`。** 项目已明确不考虑向后兼容，但 `set_device_name(None) = 清空、
回退 hostname` 这个语义本来就准确；core 那层叫 `rename_device` 是因为它是一次**编排**（写盘 +
推网络 + 发事件）。两层名字不同是刻意的，与 D9 同一条理由。

**Web 端的分支在 JS 侧，这是形态决定的。** C5 把 `get_device_name` / `set_device_name` 做成
**模块级** wasm 导出（不挂 `WebNode`，为的是节点起不来时设置页仍能改名）。C6 加上
`WebNode::rename_device` 之后，`node-runtime.ts` 里就是一句
`const n = getNode(); return n ? n.rename_device(name) : setDeviceName(name)` ——节点句柄本来就
只活在 JS 里，这个分支挪不进 Rust。三端里只有 Web 有这一行，桌面与移动的分支在 core（D6）。

**不保留「改名后重启节点」的退路。** 有人会想留个 fallback：push 失败就退回重启。不做——它会
把三端刚删掉的编排原样长回来，而 push 失败的唯一现实原因是 actor 已关停（那时重启也做不成）。
D8 的兜底已经覆盖了真实的失败模式。

**onboarding 路径顺带受益**：`src/routes/_onboarding/device-name.lazy.tsx` 与
`mobile/src/app/onboarding/device-name.tsx` 也走 `applyDeviceName`，那时节点还没起，走的正是 D6
里 `net = None` 的分支（只落盘、不推网络、不报错）。这条路径的形状不变，但要在验收里走一遍确认
没被破坏。

---

## D11：fork 补丁的上游化（follow-up，非本期）

补丁按上游可接受的形态写，但**本期不提 PR**（任务书列为非目标）。留下的口子：

- **形态**：英文 doc comment、通用命名、`protocols/identify/CHANGELOG.md` 加条目（当前顶部是
  `## 0.48.0`）、`Cargo.toml` 版本号按上游惯例处理、附测试。
- **拆分**：这是一个**与 #6558 / #6560 完全无关**的独立补丁（那两个在 webrtc / webrtc-websys），
  可以单开 PR 并行推进，互不阻塞。
- **可能的上游反馈**：维护者也许更倾向 `Behaviour::set_config(Config)` 这种整体替换的形状。
  真走到那一步再改——整体替换要处理 `cache_size` 变更引发的 `PeerCache` 重建，比单字段 setter
  复杂得多，不是本期该背的复杂度。
- **必须同步的两处记录**：`Cargo.toml:48-74` 的注释块（第 55 行写着「fork 实测 ahead 7 / behind 0，
  **全部已提上游，无未提项**」）与 `dev-notes/knowledge/net-kernel.md:87` 的同款断言。本 change
  之后两处都是错的——不改，下一个对账 fork 与上游的人会以为 identify 那条是别人偷塞进来的。
  新行状态填「未提交（follow-up）」。

---

## D12：测试策略——三层，各钉一段

| 层 | 位置 | 钉什么 |
|---|---|---|
| fork | `protocols/identify/tests/smoke.rs`（已有 `libp2p-swarm-test` harness，`periodic_identify` 可照抄） | 双 swarm 连上 → 首次 identify → A `set_agent_version` + `push` → B 收到带新 `agent_version` 的 `Event::Received`。**这是补丁本身的正确性** |
| `crates/net` | 新增 `tests/identify_agent_version.rs`（用 `tests/common` 的 `spawn_node` / `wait_event`） | 内核封装后仍成立：A 调 `Endpoint::set_agent_version` → B 在秒级内收到第二条 `NetEvent::PeerIdentified` 且 `agent` 是新值；再加一条**幂等**用例（设同值不触发对端事件），对应 D2 ③ |
| `crates/core` | `tests/` 新增双节点用例 | D5 那条接收链路：A、B 配对 → A `rename_device` → B 侧收到 `CoreEvent::PairedDeviceAdded` 且 `device.os_info.name` 是新值。**它今天零覆盖，断了是静默失效** |

`crates/net` 那条测试里 `spawn_node()` 用的是默认 agent_version，需要一个能指定初值的变体
（`Endpoint::builder().agent_version(...)`），加进 `tests/common/mod.rs` 而不是各测试自己拼。

三层都要覆盖的一条边界：**把名字改回等于 hostname 也是一次真实变更**——`to_agent_version()` 在
`name == hostname` 时不写 `name=` 槽位（`crates/host/src/device.rs:239-246`），字符串确实变了。
这条最容易被当成「没变化」而漏测。

---

## D13：明确接受的边界

| 边界 | 行为 | 为什么接受 |
|---|---|---|
| 改名前已发出的邀请串 | 仍带旧 `display_hint` | 邀请是一次性 + TTL 300s（`crates/invite`），5 分钟后自然消失；追改要么撤销用户已发出去的链接、要么让邀请可变（破坏签名） |
| 改名瞬间在途的 `PairingRequest` | 仍带旧 `os_info` | 请求已在网络上，追不回；且对端配对成功后立刻会经 identify 拿到新名字并 `refresh_os_info` |
| presence 的 `OnlineRecord` | 无需处理 | 它的 `os_info` 已是 `OsInfo::redacted()`（`presence/supervisor.rs:578`），**压根不带名字**——这是隐私设计，不是遗漏（同文件 :617 有一条注释专门防止有人改回 `default()`） |
| 传输 offer 上显示的发送方名字 | 自动跟随 | 接收侧用的是自己 paired 记录里的名字（`crates/transfer/src/incoming.rs:213` 的 `display_device_name`），identify 刷新后自动生效，无需额外接线 |
| 未连接的对端 | 下次连接自然拿到 | 新连接的 handler 从 `Behaviour.config` clone（`behaviour.rs:403`/`:436`），而那里已经是新值。**不需要为离线对端排队补推** |
| 名字改回等于 hostname | **是一次真实变更** | 见 D12 末尾。验收单列一条 |

---

## wasm 三条硬约束的触碰情况

| 硬约束 | 本 change 是否触碰 | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **否** | 新增的 `device_name.rs` 只依赖 `DeviceConfig` 端口（C5 定义的 trait）、`DeviceName`、`NetManager`、`EventBus`——全是既有的平台中立抽象，不引入任何存储实现。`CoreEvent::DeviceRenamed` 的负载是 `Option<String>` + `String` |
| **`crates/transfer` 零 network 依赖** | **否** | 本 change 一个字都不改 `crates/transfer`。设备名在传输侧是**读取**关系（`incoming.rs:213` 从已注入的 `PairedDeviceInfo` 取），无反向依赖 |
| **`crates/invite` 零 core 依赖** | **否** | 本 change 不改 `crates/invite`。邀请的 `display_hint` 由调用方（`PairingManager`）传纯串进去，方向没变——C5 已经把 `encode_invite` 的 `display` 参数收进 `PairingManager` 自己的字段，本 change 只是让那个字段可写 |

另外四条 wasm 相关的具体检查：

- **identify 补丁是双 target 的。** `protocols/identify` 无 `cfg(target_arch)` 分叉，
  `set_agent_version` 参数是裸 `String`、返回 `()`，不碰任何 native-only 类型。
- **新 `ActorMessage` 变体不加 cfg 门控。** 与 `mdns` / `autonat` / `dcutr` 那些
  `#[cfg(not(wasm_browser))]` 的 behaviour 不同，`identify` 字段无 cfg
  （`crates/net/src/behaviour/mod.rs:27`），命令在两个 target 下形状一致。
- **`std::sync::RwLock` 在 wasm 可用**（单线程下不会阻塞；本 change 的临界区里只有 clone，
  且不跨 `.await`——见 D7）。
- **改了 `crates/net`、`crates/core`、`crates/web` → 必过 `./scripts/check-wasm.sh`（含
  `--clippy`，`-D warnings`）**。这三个 crate 都在 `scripts/check-wasm.sh:25` 的 `CRATES` 列表里，
  且 `crates/web` 在 native target 下近乎空 crate，`cargo check --workspace` **抓不到**它的漏改。

**版本号三处同步**：本 change 不发布，不涉及。**libp2p rev 升级**：涉及，按 CLAUDE.md 走独立 PR。

---

## 核对修正（与立项描述不一致处）

立项材料里的每个 file:line 都核对过，四处需要修正：

1. **「agent_version 锁在私有 `Behaviour.config` 里」只说对了一半。** 更关键的是它在
   `Handler::new` 时被 clone 进每条连接（`behaviour.rs:403` / `:436`），因此**仅开放 config
   setter 不足以让已连接的对端看到新名字**。补丁面因此比预期大一点：多一个 `InEvent` 变体与
   handler 侧的接收分支。见 D1 / D2。

2. **对端接收路径的函数名与行号**：立项写的 `event_loop.rs:92 from_agent_version` 位置准确，
   但它所在的函数是 `refresh_paired_device_from_identify`（`event_loop.rs:84-94`），发事件在
   同文件 `:76-81`。核对结论：**该路径零改动**（D5），只需补测试。

3. **Web 端「请刷新页面」提示当前并不存在** —— `docs/app/app` 现在根本没有设备名设置入口
   （`grep -rn "设备名\|deviceName" docs/app/app` 只命中对**对端**名字的展示）。这条提示是 C5
   引入的过渡形态（C5 proposal 的非目标段写明「Web 提示刷新页面」，C5 design D7 末段与
   `specs/device-naming/spec.md:150` 同）。所以本 change 的措辞是确定的：**C5 引入它，C6 删除它**，
   不是「若存在则删」。

4. **移动端设备名今天不在 Rust 侧**：`mobile/src/lib/device-name.ts` 只写 AsyncStorage，Rust 侧
   在 `mobile-core/src/network.rs:194` 从 `start_node` 入参收，`:216` 拿去 `OsInfo::native(...)`,
   构造完即丢。这正是「改名必须重启」在移动端的直接成因。C5 把它挪进 Rust 侧落盘（C5 D4），
   本 change 假定那一步已完成——**C5 未合并前，本 change 的阶段 4 及之后不可开工**。
