## Context

动机见 [proposal.md](proposal.md) 的 Why；行为契约见两份 spec。本文只讲怎么实现，以及
那些**由 dsh 的架构反推出来、不写下来就会被下一个人推翻**的决定。

事实底座（2026-08-20 从 `deepseek-ai/deepseek-harness` 一手核实，版本 `0.1.0-rc.8`，
本地检出在 `/Volumes/yexiyue/deepseek-harness`）：

- dsh 插件是 Cordis 插件，Node 半边导出 `apply(ctx)`；浏览器半边靠 `package.json` 的
  `dsh.client` 声明 + `exports["./client"]` 被 host 自动扫描并组装进 web bundle
  （`docs/subsystems/client-modules.zh.md`）。**这是官方一等扩展路径，不需要 patch 任何
  内部包。**
- dsh 是**事件溯源**架构：Session 事件流按 `seq` 升序回放必须能确定性重建 UI，
  Conversation Node 的 `start`/`update` 不得依赖「只存在于实时内存中的状态」
  （`docs/cookbook/adding-a-conversation-node.zh.md`）。
- 上游明确声明会有破坏性变更（developer preview）。

本仓侧的既有资产：`crates/cli` 已有三档取数入口（`NodeAccess` / `DaemonAccess` /
`RecordAccess`）、本地通道、单实例仲裁、`--json` / `--no-input` 两个全局开关；npm 包
`swarmdrop` 已随 `dist` 发布，`optionalDependencies` 按平台拉二进制的路已经通了。

## Goals / Non-Goals

**Goals:**

- 浏览器侧**零旁路**：插件 Client 半边的全部数据来自 dsh 官方通道，因此 carrier 无关。
- 本仓只出**协议与二进制**，harness 适配层留在仓外——换 harness 不换底座。
- MCP 工具实现的 trait 边界从第一天起就是平台中立的，将来上移是搬文件而非重写。

**Non-Goals:**

- 不替换 dsh 的 `ctx.connection` carrier（那是 M4「完整台面层」，依赖 developer-preview
  的内部包，且 26 家在抢同一层）。
- 不做「agent → 人」的审批 / 提问转发到手机（要动移动端 UI 与授权模型，独立立项）。
- 不重构桌面端那 20 个 MCP 工具（理由见 D7）。
- 不做跨订阅持久 seq 与 `--since` 续订（理由见 D14），不做事件的落库与保留策略。

## Decisions

### D1. 浏览器侧的数据一律从 Session 事件流 fold，不走任何旁路 RPC

**这是本设计的地基，其余决定都由它派生。**

第三方插件的 Client 半边**没有**调自己 Node 半边的官方通道，三条路都堵死：

| 路径 | 为什么不可用 |
|---|---|
| Typert Remote（`ctx.remote.<ns>`） | 生成流水线跑在 dsh **根构建**里（typert generator 以 Host aggregate 为唯一 `ts.Program` 种子），仓外的包生成不出 `typert.remote-client.*`；而「Client Remote 拒绝挂载缺少严格 codec 的 SRC 描述符」，开发回退也堵死 |
| API Proxy（`POST /api/<method>`） | `RpcMethodMap` 是封闭集合，只注册 `SessionsApi` / `HostApi` / `EventsApi` 三个领域接口 |
| 浏览器直连 loopback | 见下方「为什么否决」 |

**这不是第三方待遇差，是架构不变量。** dsh 自己的包也走同一条路——`ui-deliverables`
的产出文件行是从每个 Turn 的 mutation 事件 fold 出来的，不是查出来的。一条旁路 RPC 会
直接破坏「UI 状态完全由持久事件重建」：刷新页面、翻历史分页、多客户端同看一个会话时，
那部分 UI 就重建不出来。

**否决 loopback 直连**（浏览器 `fetch http://127.0.0.1:<cli-port>`）：它看起来最省事，
但假设了「browser carrier + Host 在本机」。dsh 的 carrier 已经有三种实现（browser /
in-process / Electron），in-process 与 Electron 下这个假设都不成立；将来若真做远程访问，
它第一个坏掉。而事件流是 dsh 的地基，哪种 carrier 下都在——**人在外面用手机连回家里的
dsh，收件箱选择器照样能用，因为数据跟着事件流走而不是跟着 localhost 走。**

代价照实说：数据新鲜度受限于事件推送，做不到「点开选择器的瞬间现查一次」。可接受——
收件箱变化本身就会推事件，选择器的数据在变化发生时就已经更新了。

### D2. 每处 UI 落在哪个官方 seam

| 要做的 | seam | 稳定性 |
|---|---|---|
| `@` 引用收件箱里的文件 | `ctx.inputTriggers` 的 source roster（开放注册，pick 产出 `ReferenceInsert`，`appearance: 'file'`） | 文档化约定 |
| 传输进度 / 发送记录的富渲染 | `ConversationNodeDefinition` + `conversation.chat.node` keyed renderer | 有 233 行官方指南 |
| 「手机发来了 X」出现在对话里并可一键引用 | 同上 | 同上 |
| 设备面板、节点状态 | `ctx.slots.register()` | 一等扩展点 |
| 设置卡 | `settings.*` 系列 slot | 有 cookbook |
| `/swarmdrop …` 人触发的动作 | `CommandDefinition`（**handler 在 Node 侧执行**） | 文档化 |
| deliverables 行加「发到手机」 | `conversation.chat.turnTail` hole | 同 `ui-deliverables` |

`CommandResult` 的 `sourceEventSeq` 是把命令与富渲染缝起来的关键：handler 先发一条持久
domain 事件，result 只指向它，展示交给那个事件对应的 conversation node。于是数据流始终
单向——**人的动作触发 Node 执行，Node 发事件，Client 渲染事件**。

### D3. 事件是「真实发生的事」，不是状态镜像

进 dsh session 日志的 `swarmdrop/*` 事件按「这次对话里发生了什么」设计，而不是把
SwarmDrop 的状态往里镜像：

```
swarmdrop/inbox-baseline    会话开始时，你手边有这些可引用的东西
swarmdrop/inbox-received    手机刚发来一个文件            ← 真实事件，有时间点
swarmdrop/sent              agent 把 X 发给了 iPhone      ← 真实事件
swarmdrop/referenced        用户引用了收件箱的 contract.pdf ← 真实事件
```

`inbox-baseline` 不是妥协而是语义正确的一等事件：三个月后回放这段对话，它回答的是
「他当时手边有什么可用」——那正是理解这段对话所需的上下文。它也正是 conversation node
指南推荐的 whole-value checkpoint（「start 位于已加载窗口之外时它仍可直接使用」）。

**只发最近 N 条**（spec: `cli-event-stream` 的「初始基线」）。收件箱会累积到数千条，
而每个会话开头都搬一次全量既昂贵又无用；真要更早的条目，按需检索才是正确取数方式。

**被否决的替代**：只发增量、不发 baseline。那样新开的会话里选择器是空的——而「引用收件箱
里的东西」恰恰最常发生在会话刚开始、你正要说第一句话的时候。

### D4. 插件用原生 `ctx.tools`，`swarmdrop mcp` 服务其他 harness

两者不重叠，各有不可替代的理由：

- **插件走 `ctx.tools.register()`**：少一跳进程与一层协议，工具名没有 `mcp__swarmdrop__`
  前缀，而且——最关键的——插件的工具执行时能**顺手发 Session 事件**，让这次发送在对话里
  长出一个富渲染节点。经 MCP 转发做不到这件事，MCP 的结果只是一段文本。
- **`swarmdrop mcp` 的价值在 dsh 之外**：Claude Code、Codex 的扩展机制各不相同，唯一的
  公约数是「能执行一条命令」。有了它，Claude Code 接入是**零代码**（几行配置）。
  它同时也服务不装插件、只想要工具能力的 dsh 用户。

推论：`swarmdrop mcp` **不依赖 dsh 是否长期存在**，这正是它值得先做的理由。

### D5. `swarmdrop mcp` 归 `NodeAccess`，节点与 server 同生命周期

按知识库那句可判定的问句——「这条命令会不会导致一个数据包离开本机？」——会，所以是
`NodeAccess`（有常驻就复用、没有就自持）。

但它与既有 `NodeAccess` 命令有一处不同：**节点持有到 server 退出**，而不是每次工具调用
现开现关。后者的代价是每次发送都要重新连引导节点并做 NAT 探测，把秒级调用拖成数秒。
知识库的分档表要补一行说明这个长驻形态（`runtime/` 本就不得假设调用方是一次性命令，
见 `standalone-cli-host/design.md` 的 D11）。

单实例锁的交互照旧：自持节点时持锁，用户此时 `swarmdrop start` 会被正确拒绝并提示。

### D6. 事件订阅命令叫 `swarmdrop watch`，与 `transfer watch` 并存

命名按知识库的三条规则：操作对象是**本程序自身的事件流**且为单例 → 规则 1，平铺动词。

与 `transfer watch` 并存是刻意的，两者是不同的东西：后者是给人看的传输面板（固定间隔
重绘全量快照、带热键），前者是给程序用的三类事件增量推送。design 层面的判据：
**面板服务于「此刻我想看一眼」，订阅服务于「有变化就告诉我」**。

分档上它不启动节点（`RecordAccess` 的判据：不会有数据包离开本机），但需要推送而非查询，
因此实现上是「有常驻节点就经本地通道订阅，没有就等待节点出现」。这让消费方不必关心自己
与节点的启动顺序。

传输形态选 **stdout NDJSON**，不另开本地通道：消费方是 spawn 它的宿主进程，管道天然存在、
跨平台一致、无需端口或路径发现。

### D7. 工具实现落在 `crates/cli` 内部，但 trait 边界从第一天就平台中立

工具的 backend 定义成一个**不引用任何宿主类型**的 trait（不碰 `tauri::AppHandle`、
不碰 clap 的类型），CLI 提供它的实现。工具的 schema、描述与分派只依赖那个 trait。

**为什么不现在就抽 `crates/mcp` 让桌面端共用**：桌面端那 20 个工具全绑
`tauri::AppHandle`（`resolve_transfer(app)`、`mcp_default_receive_dir(app)`、
`organization_alias(app, …)` 等），解耦它们是一次实打实的重构，会把这次探针的节奏拖成
基建。而 trait 边界一旦是干净的，将来合并就是搬文件——反过来（先耦合再解耦）才是重写。

这条也兑现了 `dev-notes/architecture/future-openspec-candidates.md` §5 `mcp-cross-host`
的方向，只是分两步走。

### D8. 仓库边界落在协议与二进制上

```
本仓（稳）                          仓外（易碎）
├─ crates/cli                       dsh-swarmdrop（独立仓 · TS · npm）
│   ├─ swarmdrop mcp    ────────┐      ├─ Node 半边：spawn CLI、注册命令与工具、发事件
│   └─ swarmdrop watch  ────────┼──→   └─ Client 半边：inputTriggers / node / slots
└─ 事件与工具的契约（两份 spec）  │
                                 └──→  Claude Code hooks（几行配置，零代码）
```

插件独立仓的理由：它是 TS 项目、走 npm 发布、要进 awesome 列表、README 得用英文写；
塞进本仓这个 Rust workspace 只会污染发版流程。更重要的是**上游会 breaking**——易碎的
那一半全在仓外，dsh 改了 seam 只改那个仓，本仓的配对 / 传输 / 收件箱一行不动。

### D9. 版本字段挂在每条事件上，不靠握手协商

因为消费方会**持久化**这些事件：插件把它们转写进 agent 的会话日志，那些日志跨月留存
并会被回放。握手协商只覆盖「这次连接」，覆盖不了「三个月前写下的那一行」。

递增与不递增的判据写在 spec，此处不重复。

### D10. 收件箱事件由领域发出，四端接线（**2026-08-21 决定，范围因此扩大**）

调研查实的现状：

| 端 | 「收件箱变了」怎么知道的 | 状态 |
|---|---|---|
| 桌面 | **不知道** | `inbox-store.ts` 零事件监听，收件箱页只在挂载与用户动作时刷新 |
| 移动 | `TransferCompleted` → `refreshInbox()` | **漏判 direction**，发送完成也白刷一次 |
| Web | `TransferCompleted` 且 `direction === "receive"` | 正确 |
| 文本到达 | 三端各按 `kind === "received"` 字符串约定推一遍 | 桌面那份只刷待确认队列、不刷收件箱 |

三份推导、两份有缺陷、一份不存在。CLI 再推一遍会是第四份，**且是最差的一份**：它要依赖
一条只以行内注释存在、零护栏测试的顺序不变量（先建条目 → 再发 `TransferCompleted`），
而文本那条还有实缺口——按 `delivery_id` 反查条目不在端口上，只能全表扫。

那条顺序不变量的脆弱性已经在生产里显形：终态 projection 比条目创建**更早**发出，于是桌面
UI 被迫向用户道歉——`session-panel.tsx:530` 那句「收件箱记录还在生成，请稍后再试」就是一次
竞态的用户可见残留。

**决定：在领域内发一等事件，四端全部接线。** 两个条目创建点都已经握着刚建出来的
`InboxItemDetail` 与事件 sink（`receiver.rs:1038` 的返回值目前被注释标着「刻意不消费」），
补事件不需要新造任何数据通路；载荷类型也零新增成本（`InboxItemDetail` 已 derive
`specta::Type`、已在 `bindings.ts`、已有 uniffi 的 `From`、已跨 wasm 传输）。

**被否决的替代**：只接 CLI 不接三端。它避免了扩大范围，但留下「事件已有、没人接」的中间态
——本仓对这种中间态有过教训（生物识别插件空挂一段时间，而文档一直在宣传一个不存在的功能）。

**代价照实说**：作废 proposal 原先「不动三端」的承诺、两个入库产物要重建提交、桌面与移动的
`CoreEvent` catch-all 会**静默吞掉**新变体（`CoreEvent` 是 `#[non_exhaustive]`，编译器只护得住
`transfer → core/web` 那一段），两处各需一条护栏测试。

### D11. 归档与软删收进共享编排（**2026-08-21 决定**）

`archive_inbox_item` 目前**连编排层都没有**：它是端口方法，被 4 个宿主 + 2 个 MCP server 直调。
后果是同一台机器上「桌面 MCP 归档了一条」对桌面界面完全不可见——同一份数据，一个进程里的
两个部分看到不同的事实。`delete_inbox_item` 已是编排函数，但签名里没有事件端口。

**决定：两者都经共享编排并发事件。** 这也是订阅面能如实承诺「覆盖归档/删除」的前提——
否则那条承诺是假的：归档可能发生在别的宿主进程里，CLI 的 `watch` 无论如何都观测不到。

**被否决的替代**：① v1 降级、spec 写明不覆盖——诚实但留着缺陷；② 只报本机 CLI 发起的
变更——造出「本机改看得见、桌面改看不见」的**半可靠**语义，对一份会被回放数月的日志，
半可靠比不提供更糟：消费方无从知道自己的视图是不是对的，也就不会去补。

### D12. 事件 wire 是本订阅面自定义的窄 DTO

**绝不 `serde_json::to_value(&CoreEvent)` 转发。** 三条独立理由，任一条都足够：

1. **会泄露配对凭证**。`PairingRequestReceived` 把 `PairingRequest` flatten 进事件，而
   `PairingMethod::Invite` 携带 128bit bearer capability **明文**——那个类型的注释自己写着
   「明文不落盘」，而订阅面的终点正是落盘。
2. **会泄露正文**。文本条目的标题字段**就是正文的前 160 字节**，所以载荷也不能直接复用
   条目摘要类型。本仓已有两条同源不变量白纸黑字写着「事件不含正文，避免系统通知或日志
   泄露敏感内容」。
3. **`CoreEvent` 的 serde 实现在生产里从未被执行过**——四个宿主全是 match 重映射。直接透传
   等于让一个未经检验的 `tag` + `flatten` 组合当场变成对外契约。

**传输类只取会话投影 + 聚合进度**，不把六个窄边沿事件各映一条：投影是唯一同时带对端、
阶段、终态原因与**机器可读失败码**的载荷，且创建、每次阶段变化、终态三种时机全覆盖；
而 `TransferFailed.error` 是**自由文本**（本仓为此栽过一次：移动端跑英文正则误判），
`TransferAccepted` 只有一个 session_id。暴露六条只会让消费方拿到同一件事的两条记录并被迫
去重——那正是「同一条规则的第二份实现」在契约层的形态。

### D13. 背压：入队有界丢弃，出队独立任务

两份调研在这里直接冲突：一份主张复用既有的进度写入器（「别写第二份实现」），另一份主张
绝不复用（「它按设计丢帧、单次超时即永久封口」）。**两边各对了一半，分歧源于一个可查证的
前提**：那个写入器三道闸存在的唯一理由写在它自己的文档里——「调用方是 `select!` 的**分支体**，
分支体挂住时常驻节点上真正的哈希计算停下来」。而订阅的写者不是分支体：本地通道的每个连接
已经各自 spawn 一个独立任务。

于是正确的分解不是「复用与否」，而是**把背压和写分开放**：

- **入队侧**（在传输热路径上）：有界通道 + 非阻塞投递，照 `crates/net` 的 fan-out 体例。
  **绝不阻塞式投递**——本仓明写阻塞式背压只允许用在「回路能自己闭合」的地方，而这条回路的
  终点是**别人的传输**。**也不用广播通道**：全仓零先例，且它的滞后语义只告诉你跳了几条、
  不告诉你跳掉哪几条，恰是持久化消费方最不能接受的形态。
- **出队侧**：每条订阅连接一个独立写任务，可以阻塞地写——它挂住只影响这一条连接。因此
  不需要那三道闸中的任何一道。
- **采样类入队前按会话折叠成 last-value-wins**。做完这步，队列满就从常态退化成真正的异常。
- **边沿队列真满了 → 显式截断事件**，不静默。既有的进度写入器选的是「静默封口」，那在它的
  场景里成立（丢的是采样帧）；这里丢的是消费方跨月日志里的洞，且它读到裸 EOF 只会当成
  节点没了并重连，无从知道断点之前已经丢过。

**顺带**：`CliEventBus::subscribe()` 的无界通道注释（「命令的生命周期本就很短」）在本变更
之前**就已经不成立**——已有两个长驻订阅者，只是消费得极快。那条注释要一起改。

### D14. seq 限于单次订阅

每条事件带订阅内单调序号，消费方仅凭跳变即可判定漏读，不必信任任何自述计数。

**不做跨订阅持久序号与 `--since` 续订**：那会把一次探针变成有状态的子系统，还会放大一个
真空——没有节点时收件箱仍可能被改（只读命令直接开库），「那段期间的变更算不算漏」就成了
必须回答的问题。v1 的消费方与订阅是父子进程，生命周期本就绑在一起。

写死在 spec 里是为了让将来的升级是一次**明确的版本递增**，而不是一次悄悄的语义漂移。

### D15. SIGTERM 与 SIGINT 同等成功退出（**修既有缺陷**）

全仓零 `SIGTERM` 处理——只有 4 处 `ctrl_c()`，Unix 上只接 `SIGINT`。而 agent harness 结束
子进程与服务管理器停止服务用的都是 `SIGTERM`。照现状，**最常见的正常收摊路径永远退非零**，
正好触发 spec 想避免的那件事（消费方把正常结束读成崩溃并重启）。

这是既有缺陷而非本变更引入，但它与长驻命令直接相关，一并修。

### D16. 四处会在本变更落地后过期的事实源，必须同 PR 改

本仓最贵的一类缺陷是「同一条规则的第二份实现」，**两份互相矛盾的理由同理**：

1. `cmd/transfer.rs` 的「不是靠事件推送……本地通道是一问一答的」
2. `ipc.rs` 的「保持长连接只会多一套超时与心跳逻辑」
3. `adapter/events.rs` 的无界通道理由「命令的生命周期本就很短」
4. `adapter/events.rs` 的「事件属于运行叙述，一律走 stderr，绝不进 stdout」——`watch` 的事件
   是这条命令的**结果**而非过程叙述，走 stdout 不违反那条规则的精神，但那句话的字面必须补
   例外，否则它会误导下一个人

另有一处**语义反转**要在 design 里写明而不是留给后人猜：`transfer watch --json` 取数失败即
退出（其注释专门论证了这个不对称是刻意的），而本变更的 `watch` 要求节点关停时继续等待。
同一个二进制上会有两条 NDJSON 流、语义相反——这不是错误（两条命令服务不同消费方），
但不写明就会有人以为其中一条写错了。

### D17. 每次接上节点都推一条基线，因此没有「节点可用」事件（**2026-08-21 实现期决定**）

订阅跨节点起落存活，于是「接上节点」这件事在一次订阅里会发生多次。两种做法：

| | 只在订阅建立时推一次基线 + 一条 `nodeAvailable` | **每次接上都推基线** |
|---|---|---|
| 消费方接上后的视图 | 不完整：无节点期间那份基线里在线状态全是「未知」，只能靠增量补 | 完整：整值覆盖 |
| 事件种类 | 多一个空事件 | 少一个 |
| 消费方的处理 | 要区分「第一条基线」与「后续增量」两种模式 | 一种：基线到了就整值覆盖 |

选后者。基线本来就是 conversation node 指南推荐的 **whole-value checkpoint**
（「start 位于已加载窗口之外时它仍可直接使用」），而 checkpoint 的价值恰恰在于它可以
重复给出。一条 `nodeRunning: true` 的基线既宣告了节点在跑、又交出了此刻的真实状态，
再加一个空事件只是同一件事的第二种说法。

`nodeUnavailable` **没有对称的伙伴，这是刻意的**——它承载的信息（「此后不会再有传输与
设备事件，直到另行通知」）没有别的事件能替代。

### D18. 「哪些丢了要上报」的判据归订阅面，不归事件总线

`subscribe_lossy` 收一个 `fn(&CoreEvent) -> bool` 而不是自己判断：**采样与边沿的划分是
订阅面的语义**，事件总线只认「有人要、队列满了」。

判据本身写成「**只有进度是采样**」而不是逐条列举边沿——这样下一个人新增的领域事件默认
落进边沿一侧，也就是默认「丢了要上报」。反过来写（默认可丢）会让它静默消失，而那正是
这条要求存在的理由。

不分类的代价是真实的：一个卡住几秒的消费方会让队列填满进度样本，恢复后收到一条
`truncated { dropped: 500 }`——而那 500 条全是下一帧就会纠正的采样。消费方据此判断
自己的记录完不完整，一次正常的降压不该长得像一次数据损失。

### D19. `FrameSink` 的两个方法是两种投递策略，不是「安全 / 不安全」

同一件事在两种调用位置上的正确答案不同：

- `try_send`（进度）：调用方是 `prepare_with_progress` 的 `select!` **分支体**。挂住会让
  同一个 `select!` 里真正干活的 future 得不到轮询——**常驻节点上别人的传输就停在那儿**。
  所以宁可丢一帧，下一帧会纠正它。
- `send`（订阅）：调用方是这条订阅**专属**的任务。挂住什么都不影响，而阻塞正是把压力顶回
  上一段有界队列的方式，在那里变成一次如实上报的截断。在这里用 `try_send` 会让边沿事件
  无声消失。

写进 design 是因为这两个方法长得几乎一样，而挑错那个不报错：挑 `try_send` 只是偶尔少
几条事件，挑 `send` 只在客户端卡住时才显形。

### D20. `render::watch` 改名 `render::panel`

新命令叫 `watch`，而渲染层里那个 `watch` 是 `transfer watch` 的**面板**——两条语义正好
相反的流（重绘全量快照 vs 增量推送、stderr vs stdout、给人看 vs 给程序看）叫同一个名字，
迟早会被互相搬写法。改名是这次唯一的既有代码重命名，没有行为变化。

## Risks / Trade-offs

- **dsh 是 5 天大的 developer preview，明确会 breaking** → 易碎的一半全部留在独立仓；
  本仓只依赖「能执行一条命令」这个最稳的接口。插件只用文档化的 seam，不碰内部包。
- **`ConversationNodeDefinition` 与 `inputTriggers` 的类型面很大**（declaration merging、
  `SessionEventMap` / `ChatNodeDataMap` / `ConversationStepDataMap` 三处合并）→ 严格照
  官方指南与现有包的写法，先做一个最小的事件族跑通再扩。
- **CLI 事件流一旦发布就是对外契约，难改** → 版本字段 + 「新增可选字段不递增版本」的
  明确规则；订阅面从一开始就只暴露必要字段，不图省事把内部结构整个 `serialize` 出去。
- **baseline 的 N 与日志体积** → 只发最近 N 条并标明存在更早条目；N 可由调用方压低。
- **节点单实例锁与 `mcp` 长驻的交互** → 沿用既有仲裁语义（拿不到锁就走瘦客户端），
  不为这个场景新造一套规则；`tests/without_a_node.rs` 的看守用例要覆盖新命令。
- **「完整适配 dsh」是个会持续膨胀的目标** → 本变更的边界写死在 proposal 的「不做」清单
  与本文的 Non-Goals；超出的部分（审批转发、carrier 替换）各自独立立项。

## Migration Plan

无数据迁移。两条命令都是纯新增，不改变任何既有命令的行为与输出。

回滚是删掉两条命令与其模块——它们不被其余命令依赖，`crates/cli` 的既有取数入口与渲染层
不因它们发生变化。插件仓与本仓解耦，插件下架不影响 CLI。

发布沿用既有 CLI 版本线 `cli/swarmdrop-cli-v*`（`./scripts/release-cli.sh`），无新版本线。

## Open Questions

- ~~**N 的默认值**~~ → 定为 **50**，与 `INBOX_SEARCH_LIMIT` 同一个数、同一条理由
  （一屏之内、一次转写之内够用，超出的部分本来就该按需检索）。`--inbox-limit` 可调。
- ~~**人类可读模式下 `swarmdrop watch` 的行式排版**~~ → 每条一行「类别 + 一句话」，
  复用 `render::transfer` 的 `direction_glyph` / `phase_label`（同一个状态三端一个说法）。
- **进度降频窗口定为 1 秒是拍的，没有实测依据。** 真实数据来自消费方开始持久化之后——
  太密会让日志膨胀，太疏会让进度条一顿一顿。改它**不需要递增 schema 版本**（频率不是
  契约的一部分）。
