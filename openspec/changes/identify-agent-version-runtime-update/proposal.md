## Why

`device-config-port`（C5）把设备名接进了 identify 的 `agent_version`、配对请求与邀请串，
但它只回答了「名字进不进得去」，没回答「**改完之后怎么让它上线**」。今天的答案是**重启整个
P2P 节点**——而且这个答案是当作契约写在生产代码注释里的：

```
src-tauri/src/commands/identity.rs:52-54
/// 仅写入 `device_config.json`。要让新名字通过 identify 协议的 `agent_version`
/// 重新广播，前端在本命令返回后自己调 `shutdown` + `start`（前端持有
/// paired_devices + network_options 上下文）。
```

把编排推给前端的代价已经兑现：同一段逻辑三端各写一份，且**已经分叉**。

| 端 | 位置 | 失败时用户看到什么 |
|---|---|---|
| 桌面 | `src/lib/device-name.ts:29-37` | `startNetwork()` 失败只 `console.warn`（:35），调用方 `-device-info-section.tsx:96` 照样 `toast.success("设备名称已更新")` —— 名字改了，节点悄悄停了 |
| 移动 | `mobile/src/lib/device-name.ts:38-49` | `throw new Error(shutdown.error)` —— 报错，但名字**已经写进 preferences-store**（:36，先写缓存后重启），提示却是纯失败 |
| Web | 无此路径 | `docs/app/app/_lib/node-runtime.ts` 的 `spawnNode()`(:28) 是页面级单例，`closeNode()`(:51) 之后没有再 spawn 的入口（`web-node-bootstrap.tsx` 刻意不在 cleanup 里关节点）。C5 落地后 Web 的提示是「刷新页面后生效」 |

两种错误语义、三种用户可见行为。这不是「前端没写好」，是**编排本来就不该放在前端**——
它需要知道 identify 的广播时机、需要在失败时决定名字回不回滚，这两件事前端都不该知道。

重启的代价也不是「慢一点」：它断开全部连接、丢掉 relay reservation、中断正在进行的传输。
为一次改名付这个代价不成比例。

### 根因在内核往下两层

**第一层（`crates/net`）**：`agent_version` 只在建 Behaviour 时读一次
（`crates/net/src/behaviour/mod.rs:74` 的 `.with_agent_version(config.agent_version.clone())`），
对外只有 builder setter（`crates/net/src/endpoint/builder.rs:70-73`），运行期没有任何入口。

**第二层（libp2p identify）——这才是真正卡住的地方**。立项时的判断是「agent_version 锁在私有
`Behaviour.config` 里，加个 setter 就行」。核对 fork 树（rev `262dea51`）后发现**只对了一半**：
`agent_version` 在**每条连接建立时**就被 clone 进了那条连接的 `Handler`：

```
protocols/identify/src/behaviour.rs:403   Handler::new(…, self.config.agent_version.clone(), …)  // inbound
protocols/identify/src/behaviour.rs:436   Handler::new(…, self.config.agent_version.clone(), …)  // outbound
protocols/identify/src/handler.rs:93        agent_version: String,   ← 每条连接一份独立副本
protocols/identify/src/handler.rs:242       agent_version: self.agent_version.clone(),  ← build_info 读的是副本
```

所以「把 `Behaviour.config` 开个 setter」**不够**——已建立的连接照样报旧名字，而
「已连接的对端能看到新名字」正是本 change 的验收条件。真正需要的是把变更下发到每个 handler，
libp2p 里已有完全同形的先例（`InEvent::AddressesChanged` 经 `NotifyHandler::One(connection_id)`
逐连接广播，`behaviour.rs:545-559`），本 change 沿用它。

**为什么现在能做**：五行 libp2p 依赖本来就 pin 在个人 fork（`Cargo.toml:75 / :76 / :79 / :80 / :87`，
同 rev `262dea51`）。这个补丁是我们自己能落的，不必等上游 #6558 / #6560 合并——它与那两个 PR
在不同 crate，无耦合。

## What Changes

- **libp2p fork 补丁（`protocols/identify`）**：新增 `Behaviour::set_agent_version(String)` 与
  `InEvent::AgentVersionChanged(String)`。写 `self.config` + 逐连接 `NotifyHandler::One` 下发，
  handler 侧更新自身副本。**刻意不自动 push**——`Behaviour::push`（`behaviour.rs:280`）早就是
  公开 API，让 set 与 push 保持正交（design D2）。补丁写成通用能力、不带任何 SwarmDrop 语义，
  为将来提上游留口子。

- **rev 升级**：五行 git 依赖换新 rev + `Cargo.lock` 同步 + 两处「fork 比上游多什么」的记录更新
  （`Cargo.toml:55` 的「全部已提上游，无未提项」与 `dev-notes/knowledge/net-kernel.md:87` 的同款
  断言，本 change 之后都不再成立）。按 CLAUDE.md 硬约束，**rev 升级走独立 PR + 全量测试 + wasm
  check + 同步 Cargo.lock**。

- **`crates/net`**：`ActorMessage::SetAgentVersion` + `Endpoint::set_agent_version(String)`。
  事件循环封在后台 actor 内、上层拿不到 `EventReceiver`（net-kernel.md 的核心心智），所以必须
  走 actor 命令而不是直接改状态。actor 收到后 `identify.set_agent_version(v)` → 对
  `self.conns`（`actor.rs:141`）里的已连接 peer `identify.push(...)`，两步的**入队顺序不可交换**
  （design D4）；同步 `self.config.agent_version` 保持诊断镜像不脏。

- **`crates/core` 新增 `rename_device` 编排**（新模块 `crates/core/src/device_name.rs`）：
  写 `DeviceConfig` 端口（C5）→ 经 `PairingManager` 的**窄写口**更新本机 `OsInfo` 的 name →
  `endpoint.set_agent_version(os_info.to_agent_version())` → 发 `CoreEvent::DeviceRenamed`。
  **「节点未启动」的分支也收在这里**（收 `Option<&NetManager<T>>`），否则三端又要各写一遍
  `if 节点在跑 { … } else { … }`——那正是本 change 要消灭的形状（design D6）。

- **`PairingManager` 的 `OsInfo` 从不可变快照变为可变**（C5 D10 把这个形状的决定显式留给了本
  change）。写口只接受 name，**不提供整包替换的 `set_os_info`**：`to_agent_version()` 里还带着
  `caps=lan-helper`（`crates/host/src/device.rs:247-250`），而对端靠它决定要不要把这台机器登记成
  LAN Helper（`crates/core/src/network/event_loop.rs:123`）。整包替换意味着某天有人传进一个丢了
  capabilities 的 `OsInfo`，改一次名就把自己从别人的 LAN Helper 名单里静默摘掉（design D7）。

- **三端接线收敛**：桌面 `set_device_name` 命令转调 core；mobile-core 补 `rename_device` uniffi
  导出；`crates/web` 补 `WebNode::rename_device`。三份前端 `applyDeviceName` 各自删掉
  shutdown + start 编排，Web 端删掉 C5 留下的「刷新页面后生效」提示。

- **接收侧零改动，但要写进验收**：对端路径
  `crates/net/src/actor.rs:1042`（`identify::Event::Received`）→ `NetEvent::PeerIdentified`(:1054-1064)
  → `crates/core/src/network/event_loop.rs:92 OsInfo::from_agent_version` →
  `refresh_paired_device_os_info`（`pairing/manager.rs:468`）→ `refresh_os_info` 的相等短路
  （`crates/host/src/device.rs:358-363`）→ `CoreEvent::PairedDeviceAdded`(`event_loop.rs:76-81`)
  → host 持久化。这条链本来就是「identify 一到就刷新」，主动 push 与周期交换在它眼里不可区分
  （design D5）。它目前**零端到端覆盖**，本 change 补测试钉死。

**非目标**：`DeviceConfig` 端口与 `DeviceName` newtype 本身（C5 已做，本 change 只调用）；把 fork
补丁提上游（follow-up，design D11 记了 PR 形态与拆分方式）；presence `OnlineRecord` 里的 `os_info`
（已是 `OsInfo::redacted()`，压根不带名字，`crates/core/src/presence/supervisor.rs:578`）；改名前
已发出的邀请串（一次性 + TTL 300s，过期即消失，追改要么撤销用户已发出去的链接、要么让签名可变）；
identify 周期间隔调优（fork 默认 5 分钟，`behaviour.rs:183`；本 change 靠主动 push 绕开它，不动配置）；
`protocol_version` 的运行时修改（它是兼容性判别码，改它是另一回事，design D2 ②）。

## Capabilities

### New Capabilities

- `live-device-rename`: 设备改名即时对**已连接**的对端生效——无需任何一端重启节点、断开连接或
  刷新页面；编排收在 core，三端只调一个入口，「节点未启动」也走同一个入口。

## Impact

- **libp2p fork（`yexiyue/rust-libp2p`）**：`protocols/identify/src/behaviour.rs` + `handler.rs`，
  一个 setter + 一个 `InEvent` 变体 + CHANGELOG。这是本 change 唯一的外部仓改动，也是唯一的高风险项。
- **`Cargo.toml` / `Cargo.lock`**：五行 rev 同步升级 + `:48-74` 注释块补一行未提上游的补丁。
- **`crates/net`**：`actor.rs`（新 `ActorMessage` 变体 + `handle_message` 分支）、`endpoint.rs`
  （新公开方法）、`config.rs:113-114`（`agent_version` 从「构造期配置」变成「运行时可变、权威在
  identify Behaviour」，注释要跟着改）、`endpoint/builder.rs:70-73`（doc 指向运行期入口）、
  新增集成测试 `crates/net/tests/identify_agent_version.rs`。
- **`crates/core`**：新增 `device_name.rs`（编排）；`host.rs` 的 `CoreEvent`(:38-97) 加
  `DeviceRenamed`；`pairing/manager.rs` 的 `os_info`（C5 引入）改为 `RwLock` 并加窄写口；
  `crates/core/tests/` 新增双节点端到端用例。
- **`src-tauri`**：`commands/identity.rs` 的 `set_device_name`(:59) 转调 core 并**整段重写
  :50-56 的 doc comment**（那三行正是本 change 要消灭的契约，留着比没有更糟）；`events.rs` +
  `setup.rs` 登记新事件；`host/event_bus.rs` 加转发分支（对齐 :111）；bindings 再生。
- **`src/`**：`lib/device-name.ts` 删 :29-37 并改写 :18-23 的函数注释；`-device-info-section.tsx:91-103`
  的错误路径核对。
- **`mobile/`**：mobile-core 新增 `rename_device` uniffi 导出 + `events.rs` 的事件转换；
  `mobile/src/lib/device-name.ts` 删 :38-49 并把缓存写入挪到 core 成功之后。
- **`crates/web` + `docs/`**：`WebNode::rename_device` + `event_bus.rs` 分支（对齐 :60）；
  `node-runtime.ts` 加 `renameDevice` 包装；`node-panel.tsx`（C5 的改名入口）删掉刷新提示；
  `pnpm build:wasm` 重生 `docs/packages/swarmdrop-web`。
- **知识库**：`dev-notes/knowledge/net-kernel.md:85-96` 的 fork 补丁表与退出条件段。

**风险**：

1. **fork rev 升级是本仓最大的单点依赖风险**（CLAUDE.md 明列）。新 rev 必须以当前 `262dea51`
   为基线追加提交，**不能顺手 rebase 到上游 master**——那会把三个待合并 PR 的状态一起搅进来，
   出问题时分不清是谁的锅。升级走独立 PR，且 Rust CI 只跑 ubuntu，Windows / macOS 的编译问题
   要到打 tag 才暴露，发版前需在 Windows 上验一次。
2. **push 到达不代表对端会更新**。`handler.rs:385-402` 的 `ReceivedIdentifyPush` 分支要求
   `self.remote_info` 已是 `Some`（该连接至少完成过一次完整 identify）才会向上抛
   `Event::Identified`；否则整条静默丢弃。刚建连就改名的窄竞态下 push 会丢，兜底是周期交换
   （≤ 5 分钟）。design D8 记了为什么接受这个兜底而不加重试。
3. **`agent_version` 出现两处副本**（identify `Behaviour.config` 内 + actor 的 `EndpointConfig`）。
   新连接的 handler 从 Behaviour 取，所以 Behaviour 是权威；actor 那份只用于诊断与 `Debug`。
   不同步就会出现「日志里的名字和线上广播的不一样」——最难查的一类偏差，因为两边各自都自洽。
