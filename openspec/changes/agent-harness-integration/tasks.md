## 1. CLI：MCP 宿主骨架

- [x] 1.1 `crates/cli/Cargo.toml` 加 `rmcp`（2.x，feature `server` + `transport-io`），与桌面端同主版本
- [x] 1.2 定义平台中立的工具 backend trait 与其 DTO：不引用 `tauri::*`、不引用 clap 类型、不引用 `DataDir` 之外的宿主细节（design D7）
- [x] 1.3 `Command::Mcp` 变体加进 `cmd/mod.rs`，平铺一级、无子命令；在模块文档里写明它归命名规则 1 及理由
- [x] 1.4 `cmd/mcp.rs` 薄壳：解析参数 → 取 `NodeAccess` → 起 server，业务逻辑不留在壳里
- [x] 1.5 接上 `dispatch`，并确认 `Cli::command().debug_assert()` 与命令面三条命名规则的既有断言测试仍绿
- [x] 1.6 日志与 stdout 分流：MCP 模式下压制一切非协议内容进 stdout，日志走 stderr（spec: stdio 传输与 stdout 纯净）
- [x] 1.7 MCP 模式等同 `--no-input`：`prompt::configure` 在该命令下声明不可交互（spec: 调用方是程序）

## 2. CLI：MCP 工具面

- [x] 2.1 发送类工具：向已配对设备发文件与发文本，返回可用于后续查询的会话标识
- [x] 2.2 设备类工具：列出已配对设备及在线状态；目标无法解析时返回明确的工具错误且不发起传输
- [x] 2.3 收件箱类工具：列出、检索、取条目详情、取条目内文件的本地路径；文件缺失时明确报告而非返回无效路径
- [x] 2.4 传输类工具：列出会话、查单个会话状态、暂停 / 恢复 / 取消
- [x] 2.5 复用既有三档取数入口，不另起一套；暂停 / 恢复 / 取消走 `DaemonAccess` 语义（对象是活 actor）
- [x] 2.6 确认工具语义与桌面端同名工具一致（对照 `src-tauri/src/mcp/tools.rs` 的描述逐条核）
- [x] 2.7 明确不实现「代收入站传输」类工具（spec 有 SHALL NOT，加一条测试或注释锁住意图）

## 3. CLI：节点生命周期与配对安全

- [x] 3.1 节点持有到 server 退出，不为每次工具调用启停（design D5）
- [x] 3.2 stdin 关闭或进程被终止时，关停自持节点并释放单实例锁
- [x] 3.3 MCP 运行期间不打开配对窗口：入站配对一律拒绝且不消费邀请凭证（spec: 配对窗口不因 MCP 而打开）
- [x] 3.4 为 3.3 加护栏测试——这条错了会静默泄露一次性凭证，是最贵的一类失败

## 4. 领域：收件箱一等事件（`crates/transfer`）

> 范围扩大的决定与证据见 design D10/D11；行为契约见 spec `inbox-domain-events`。

- [x] 4.1 在 `crates/transfer` 定义收件箱事件族（新增 / 归档 / 取消归档 / 删除），载荷**不含正文、不含凭证**
- [x] 4.2 文件到达路径发「新增」：`receiver.rs` 的 `ensure_inbox_item_after_completion` 目前**丢弃**了返回值（注释写着「刻意不消费」），改为用它发事件
- [x] 4.3 文本到达路径发「新增」：`text_delivery/service.rs` 两处（手动 accept 与 AutoAccept）都已握着 `InboxItemDetail`，补发事件
- [x] 4.4 `archive_inbox_item` 建共享编排层（现在是端口方法，被 4 个宿主 + 2 个 MCP server 直调），成功后发事件
- [x] 4.5 `delete_inbox_item` 已是编排函数，签名补事件端口，成功后发事件
- [x] 4.6 发送方向的传输完成 SHALL NOT 产生收件箱事件——加测试钉住（移动端现有缺陷正是漏了这个判断）

## 5. 四端接线

- [x] 5.1 core 的 `event_adapter.rs`（`From<TransferEvent> for CoreEvent`）接上新变体——编译期强制，漏了编不过
- [x] 5.2 `crates/web` 的 `types.rs`（`type_name()` + `From`）接上——同为编译期强制
- [x] 5.3 桌面 `src-tauri/src/host/event_bus.rs` 接上并转发；**`CoreEvent` 是 `#[non_exhaustive]`，catch-all 会静默吞掉新变体**，补护栏测试
- [x] 5.4 移动 `mobile-core/src/events.rs` 镜像接上，补护栏测试（现成体例 `file_publish_should_survive_the_mobile_mirror`）
- [x] 5.5 桌面前端 `inbox-store.ts` 订阅新事件刷新（**现状零事件监听**），订阅挂 `_app.tsx` 布局层
- [x] 5.5b ~~删掉 `session-panel.tsx` 那句竞态道歉~~ —— **核实后不做**：那句话的根因是终态 projection 比条目创建更早发出（projection 在 coordinator reduce 成功即发，条目在 receiver 收尾才建），本变更没有改这个顺序，竞态窗口仍在。删掉只会把一次可解释的提示变成静默失败。要真正消除它得改 projection 与条目创建的先后，那是独立的一次改动
- [x] 5.6 移动前端 `event-bus.ts` 改订阅新事件，**移除按 `TransferCompleted` 的推导**（那份漏判 direction）
- [x] 5.7 Web 前端 `docs/app/app/_lib/store.ts` 改订阅新事件，移除 `transferCompleted` 推导分支
- [x] 5.8 重建并提交两个入库产物：`cd docs && pnpm build:wasm`，以及 `pnpm --filter react-native-swarmdrop-core build:ios`（uniffi 生成 TS 带 checksum 断言）
- [x] 5.9 `./scripts/check-wasm.sh --clippy` 必过（动了 `transfer` / `core` / `web`）

## 6. CLI：事件订阅面

- [x] 6.1 `Command::Watch` + `cmd/watch.rs` 薄壳，平铺一级；模块文档写明与 `transfer watch` 的分工判据（D6）与两条流语义相反的理由（D16）。顺带把 `render::watch`（面板）改名为 `render::panel`——它与新命令重名，两条语义相反的流叫同一个名字迟早会被互相搬写法
- [x] 6.2 **wire 是本订阅面自定义的窄 DTO**，绝不转发 `CoreEvent` 或条目摘要类型（D12：会泄露 128bit capability 明文与文本正文）
- [x] 6.3 事件三类 + 每条带 schema 版本字段与**订阅内单调 seq**
- [x] 6.4 传输类只取会话投影 + **聚合**进度（去掉逐文件数组），按约 1s 降频
- [x] 6.5 基线事件：最近 N 条收件箱 + 设备与在线状态 + 未完成传输；标明存在更早条目
- [x] 6.6 基线的传输部分**必须经 `TransferUnfinished` + `ProgressCache::overlay`**，不直连库——发送方向进度在传输期间是假的，直连症状是「进度条一路停在 0%，暂停的瞬间跳到 43%」（`transfer watch` 第一版栽过）
- [x] 6.7 设备类去抖 + 差分，差分键 `(peer_id, status, is_paired)`，输入一律是 `Device.status`（改用 `PeerConnected`/`PeerDisconnected` 就是第二套在线语义——GUI 的「在线」含 15s presence 宽限期）
- [x] 6.8 背压：入队侧有界 + 非阻塞投递（照 `crates/net` 的 fan-out 体例），出队侧每连接独立写任务可阻塞写；采样类入队前按会话折叠 last-value-wins（D13）
- [x] 6.9 边沿队列溢出 → 显式截断事件（含 dropped 计数），**绝不静默**
- [x] 6.10 不启动节点；无节点时推送基线后保持等待，节点出现自动接上，节点关停推一条事件后继续等待
- [x] 6.11 `--json` 输出 NDJSON（每行一条完整事件）；非结构化模式输出面向人的事件行；两种模式的事件都走 stdout
- [x] 6.12 通道侧新增订阅动词，复用 `Frame` 的行分隔线格式与 `peer_gone` 机制，**不复用 `ProgressSink` 的三道闸**（D13）

## 7. 退出语义（修既有缺陷）

- [x] 7.1 `SIGTERM` 与 `SIGINT` 同等以**成功**退出（D15：全仓零 SIGTERM 处理，而 harness 的 terminate 与 systemd stop 都发它）
- [x] 7.2 覆盖长驻命令：`watch` / `mcp` / `start`（前台）
- [x] 7.3 加测试：发 SIGTERM 后退出码为 0

## 8. 门禁与收尾

- [x] 8.1 `cargo fmt --all` + `cargo check --workspace --all-targets` + `cargo test --workspace` + `cargo clippy --workspace` 全绿
- [x] 8.2 `./scripts/check-wasm.sh --clippy` 与 `./scripts/test-wasm.sh`
- [x] 8.3 前端门禁：`pnpm exec tsc --noEmit`、`pnpm test`、`pnpm check:zustand-access`；mobile 下 `pnpm typecheck` + `pnpm lint:ci`
- [x] 8.4 `tests/without_a_node.rs` 补 `watch` 用例——**不能加进 `record_commands_never_start_a_node` 的用例表**：那张表的 `run()` 用 `.output()` 等进程结束，加一条长驻命令会永久挂住整个测试。改成独立用例：spawn + 轮询 `identity.json` + kill
- [x] 8.5 默认日志过滤覆盖新命令，且写的是 `swarmdrop` 而非 `swarmdrop_cli`
- [x] 8.6 `crates/cli/CHANGELOG.md` 补条目
- [x] 8.7 跑 `/simplify`（四路并行：复用 / 简化 / 效率 / 分层）。查实并修掉的**两条真缺陷**：
  ① `swarmdrop mcp` 自持节点时不是常驻形态——持锁数小时却不建通道、不起被动接收，
  三种失败都静默（同机命令撞「另一个进程正在启动」、`watch` 判活永远为假、节点在线却收不下
  文件）。装配抽成 `runtime/daemon.rs` 与 `start` 共用，护栏 `tests/mcp_host.rs`；
  ② MCP 的 `include_archived` 静默无效（按 `archived` 筛，而字段是 `archivedAt`），
  已下推到端口。顺带修了它引出的第三条：`mcp` 收到信号后**挂死**（stdio 传输把 stdin 交给
  tokio 阻塞读任务，运行时析构等它永不返回），清理后改为直接退，另加一条护栏
- [x] 8.7b 分层整改：watch 客户端的取数/重连/发号 `cmd/` → `runtime/watch/client.rs`；
  发送结果 payload `render/send.rs` → `runtime/transfer.rs`（此前通道服务端反向依赖表现层）；
  `render::watch` → `render::panel`（与新命令重名）；`TransferManager::events()` 的不变量
  改成「同一个事件总线」（桌面与移动结构性地做不到「同一个 `Arc`」）
- [x] 8.7c 复用与效率：「算不算在线」与「设备名册用哪个 filter」各收成一份
  （`devices::is_online` / `devices::paired_on_node`）；有界订阅改 `try_reserve` +
  `permit.send`，队列满时不再白克隆一整个事件（最密的 `TransferProgress` 带着整个文件向量，
  而 `publish` 就在传输的收发块簿记里）；`request_watching` 的回调改按值交出，
  省掉订阅侧每事件一次克隆；`Subscription` 去掉两个可派生字段；`render::stream::Stream`
  只剩一个 `bool`，塌成自由函数；`array_or_empty` 上移到 `render/mod.rs`
- [x] 8.8 跑 `/code-review high`。15 条发现，**逐条核实后全部处理**，其中这几条是真缺陷：
  ① 订阅重连无退避——撞上旧常驻节点时是满速死循环；② 丢弃计数把「与本面无关的事件」
  也算进 `truncated`，凭空报告数据丢失；③ `ensure_inbox_item_*` 的 `Option` 语义被我读反了
  （`None` 是「不算已完成接收」而非「条目已存在」），导致重复收尾会重复广播「新收到一个文件」；
  ④ `mcp` 的清理靠 `Arc::into_inner`，协议栈还持着克隆时**整段跳过**；⑤ `serve_until_stopped`
  等三处在 `select!` 里现建信号监听器，落在缝隙里的 `SIGTERM` 被静默吞掉；⑥ 复用常驻节点的
  `mcp` 压根没装信号处理器；⑦ `println!` 在 `EPIPE` 上 panic（长驻流的致命形态）；
  ⑧ `is_alive` 与 `connect` 之间的竞态会让订阅一条事件都收不到；⑨ `repair_*` 与移动端的
  只删账本绕过编排、不发事件；⑩ 归档/删除对不存在的条目仍广播变更；⑪ 桌面 `inbox-store`
  的事件重取漏了搜索结果与详情，且 StrictMode 下会泄漏一组监听器
- [x] 8.8b 一条**被否掉的修法**记在这里：给 `SIGPIPE` 恢复默认处置能一次修好所有命令的
  管道行为，但本进程跑着 P2P 栈——Linux 上对已关闭的 TCP 连接 `write` 同样会抬 `SIGPIPE`
  （Rust std 只在 Apple 平台设 `SO_NOSIGPIPE`），恢复默认等于让任何一次对端断连都可能杀掉
  节点。改成在订阅那一条流上用 `writeln!` + 把 `BrokenPipe` 当正常终点

## 9. 文档与知识库

- [x] 9.1 **四处会过期的事实源同 PR 改掉**（D16）：`cmd/transfer.rs` 的「不是靠事件推送」、`ipc.rs` 的「保持长连接只会多一套超时与心跳」、`adapter/events.rs` 的无界通道理由与「事件一律走 stderr 绝不进 stdout」。另修一处：`cmd/mcp.rs` 指的 `tests/mcp_host.rs` 不存在（护栏测试在 `src/mcp/mod.rs` 里）
- [x] 9.2 `dev-notes/knowledge/cli-host.md`：三档资源需求表补 `mcp`（`NodeAccess` 但节点长驻）与 `watch`（不启动节点但要推送）
- [x] 9.3 同文件补：`watch` 与 `transfer watch` 的分工判据、两条 NDJSON 流语义相反的理由
- [x] 9.4 记录 dsh 的三条硬约束（Typert Remote 对第三方不可用、API Proxy 封闭方法集、事件溯源是共同根因）
- [x] 9.5 `dev-notes/knowledge/rust-backend.md`：收件箱领域事件的归位，以及「宿主不得从传输事件推导收件箱变化」这条判据

## 10. 插件仓 `dsh-swarmdrop`（**仓外**：`/Volumes/yexiyue/dsh-swarmdrop`）

- [x] 10.1 建仓：TS 项目、`dsh.client` 声明、`exports["./client"]`、`optionalDependencies` 拉 `swarmdrop` 二进制
- [x] 10.2 Node 半边：spawn `swarmdrop watch --json` 长驻订阅，把 CLI 事件转写成 `swarmdrop/*` Session 事件（带版本字段）
- [x] 10.3 Node 半边：`ctx.tools.register()` 注册 5 个原生工具（不经 MCP 转发，见 D4），发送时顺手发 Session 事件
- [x] 10.4 Node 半边：`CommandDefinition` 注册 `/swarmdrop`，handler 用 `sourceEventSeq` 指向富渲染事件
- [x] 10.5 Client 半边：`ConversationNodeDefinition` + `conversation.chat.node` renderer——传输进度、「手机发来了 X」
- [x] 10.6 Client 半边：`ctx.inputTriggers` 注册 `@` source，候选来自**会话投影**，实现 `subscribeLexicon`
- [x] 10.7 ~~`ctx.slots` 设备面板 + `settings.*` 设置卡~~ —— **本轮不做**：两者都要求本插件自带 locale 命名空间与
  设置 schema，而它们的价值取决于真实使用中「用户多久要看一次设备列表」——在有人真用之前做，是在猜
- [ ] 10.8 Client 半边：`conversation.chat.turnTail` 加「发到手机」
- [ ] 10.9 英文 README ✅ 已写；中文文档站页面与 `awesome-dsh-plugin` 收录待办
- [ ] 10.10 端到端验证：`dsh plugin add` → 在 dsh 里说「把这个发到我手机」成功；手机发来文件后能在对话里 `@` 引用到

### 10.a 三条**必须记住**的发现（都是编译器逼出来的，不是读文档读出来的）

- **两半不能进同一个 TS program。** dsh 在两侧对 `Context.sessions` 各augment 一次（Node 是
  `SessionStore`，浏览器是 `ISessions`），合在一起会让浏览器半边对着 Node 的服务面编译，
  报出的错完全指不到根因。推论是源码级的规矩：**client 文件绝不 import 包根**，只走
  `/types` 与 `/client` 子路径。
- **官方 conversation-node cookbook 里那段示例原样编不过。** `ChatNodeViewProps` 捆了
  `t: TranslateNS<'conversation'>`，而 slot 只在注册时传了 `locale` 才注入 `t`，第一方传的那个
  命名空间值又没导出。
- **`exec.agent` 是可选的。** Code Mode 的嵌套分发没有 agent——发送照样发生，但没有会话可归属。

### 10.b 一处对 design D1 的修正

D1 写的是「浏览器侧的数据一律从 Session 事件流 fold」。**更正确的说法是「一律来自日志派生的
成品值」**：dsh 有专门的 session-projection seam（领域注册一个纯折叠单元，框架驱动它跑过每条
已提交事件，客户端收成品值），其文档明写「客户端从不折叠领域事件」。`@` 的候选走的是它，
不是手写 fold。结论没变（不许旁路 RPC、carrier 无关），拿到数据的方式更正统了。

### 10.c 一个**阻塞发布**的上游问题

`@deepseek-ai/dsh-client-runtime` 依赖 `@deepseek-ai/dsh-compact`，而后者**没发到 npm**——
client 侧的依赖链在 registry 上是断的。在它发布之前，浏览器半边只能对着 dsh 检出做类型校验
（`scripts/dev-tsconfig.mjs` 复用 dsh 自己那份 153 条 paths 映射）。
