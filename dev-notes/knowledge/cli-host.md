# 命令行宿主（`crates/cli`）

> 第四个宿主，与桌面 / 移动 / Web 并列。bin 名 `swarmdrop`，package 名 `swarmdrop-cli`。
> **碰 `crates/cli`、`crates/host-fs`、`dist-workspace.toml` 时必读。**

## 它复用了什么、自己做了什么

复用面比预想的大：`swarmdrop_core::runtime::start_node` 这个组合根本来就是平台中立的，
`HostPorts` 只有 5 个字段，`notifier` 还是 `Option`（注释明写「`None` = 该端没有这个概念」）。

| 端口 | CLI 怎么给 |
|---|---|
| `device_config` / `paired_device_store` / `KeychainProvider` / `FileAccess` | **全部复用 `crates/host-fs`**，只提供路径 |
| `event_bus` | 自己实现（`adapter/events.rs`），带订阅能力 |
| `notifier` | `None` |
| `invite_store` / `TransferStore` | `storage-sql` 现成 |

**没有 DTO 层**：CLI 不跨 FFI，直接用 core 的类型。桌面 1613 行 + 移动 2581 行的那层
「命令」代码是 specta / uniffi 强加的类型翻译，**不是可复用的业务逻辑**——CLI 对应的量接近 0。

## 端口的 native 实现住在 `crates/host-fs`

`crates/host` 是**纯端口**（trait + DTO + error + device 类型，零文件 IO）；
本地文件系统的实现全在 `crates/host-fs`：

- `JsonFileIdentityStore` —— 身份 + 已配对设备（原子写、unix 0600、**读取失败不降级**）
- `JsonFileDeviceConfig` —— 设备名
- `LocalFileAccess` —— 读源、暂存、发布（含符号链接越界防护）

**依赖方向**：core → 端口；**宿主 → 端口 + 实现**。core 刻意不依赖 `host-fs`——它要过 wasm
双 target 门禁，而实现是 native-only。谁用实现谁声明它。

⚠️ 拆分时发现的事实：桌面的 `FileAccess` 实现**早就不用 Tauri 了**，7 处 `tauri` 全是未使用的
`_app` 参数。三层结构（trait 实现 → 单变体 enum 分派 → 路径操作）中间那层是历史遗留，
迁移时跨过了它。目录扫描（`enumerate_dir`）留在桌面——它不属于 `FileAccess` 契约，
返回形状是宿主界面的事。

## 单实例：通道发现 + 文件锁仲裁，**不要 pidfile**

同一份数据目录任一时刻最多一个进程持有节点。这是**正确性要求**：同一身份两个进程上线会让
DHT 可达性记录互相覆盖、relay 预留互相顶替、已配对设备表并发写。

```
通道能连上 ──────────────────────▶ 有活节点，走 IPC
     │ 连不上
     ▼
判为陈旧残留 ──▶ 抢文件锁 ──┬─ 拿到 ──▶ 清理残留，我持有节点
                           └─ 没拿到 ──▶ 有并发者正在启动，回到第一步重连
```

**两段缺一不可**：只有通道判定时，两个进程可能同时判出「陈旧」然后一起启动；只有文件锁时，
无法区分「锁没人拿」与「持锁进程已崩溃」。

**pidfile 是错的**：PID 会被复用，陈旧 pidfile 会把「没有节点」误判成「有节点」，
用户陷入「怎么都起不来、也没有进程可杀」的死局。

**文件锁用标准库**（`std::fs::File::try_lock`，1.89 起稳定），不需要 `fs4` 之类的 crate ——
一度加了 fs4，结果发现 `file.try_lock()` 被解析到了标准库那个方法上。

## 节点生命周期与三端同语义

`start` / `stop` / `status` 直接对应核心的 `NodeStatus::{Running, Stopped}`，是三端 UI
（`NodeStatusSheet` / `NodeControlSheet` / Web 弹窗）之外的第四份实现。

**没有 `recv` 命令**：接收是节点在线时的被动后台行为，与「配对 + 被动接收」模型一致。
常驻节点靠 `runtime/receive.rs` 自动接受入站传输——没有界面可以弹确认框，等一个不会到来的
人工确认只会让对端一直卡着。

**不得有隐式常驻启动**。这条与前端的 `pnpm check:node-lifecycle` 是同一条原则
（「那会长成收敛环，用户点了停止立刻被拉回」）。因此：

| | 常驻节点 | 临时节点 |
|---|---|---|
| 谁起的 | `start` | 无节点时的一次性命令 |
| 何时止 | `stop` 或信号 | 命令结束 |
| 都持锁、都开通道 | 是 | 是 |

`status` 不区分两者（设备此刻确实在线）；`stop` 对两者都生效（显式意图优先）。

## 本地通道的三个坑

1. **必须并发处理连接**。`send` 会阻塞到传输终态（可能几分钟），串行处理时那期间连 `stop`
   都递不进来，用户只能杀进程。
2. **`stop` 要留排水窗口**。应答在独立任务里写，而主循环一跳出就关停节点、进程随即退出，
   未写完的应答会随运行时一起消失。现在关停前等 200ms；**不能改成「等那个任务结束」**——
   同一批在途请求里可能有个正在传几分钟的 `send`。
3. **`stop` 必须由处理器主动唤醒服务循环，不能靠「accept 返回后读一个标志位」**。
   前两条的直接推论，却极容易漏：`accept_one` 接受连接后立即返回，请求在独立任务里处理
   ——所以循环读标志位的那一刻它**必然**还是旧值，于是转头又阻塞在下一次 accept 上，
   而 `stop` 客户端一问一答就断开了，不会再有连接把它唤醒。

   表现非常有迷惑性：`swarmdrop stop` 打印「节点已停止」、客户端退出码 0、节点也确实
   不再接受新请求，**但前台进程一直挂着**，直到有人碰巧再执行一条命令（`status` 之类）
   顺带把循环唤醒。`cargo run` 跑 `start` 时最容易撞见。

   现在用 `tokio::sync::Notify`，handler 处理 `Stop` 时 `notify_one()`，服务循环的 select
   多一条 `notified()` 分支。护栏测试是
   `cmd::start::tests::stop_ends_serve_loop_without_further_connections`——**它的要害是
   「发完 stop 之后不再建立任何连接」**，少了这个约束，有缺陷的实现照样通过。

## 邀请的有效期 = 签发者的在线时长

`pair` 生成的邀请里带的是**签发者当时的可拨地址**。所以：

- 常驻节点在跑 ⇒ **必须由它签发**（经通道），否则另起的临时节点一退出，那张码就指向一个
  不存在的节点
- 没有常驻节点 ⇒ 临时节点签发后**必须保持在线**直到配对完成或用户中断

⚠️ 最初的实现是「签完就关节点」，产出的是一张**扫了也拨不通的码，且没有任何报错**——
用户只会看到对方说「连不上」。这类缺陷编译器和单测都发现不了，只有把「邀请里到底带了
什么」想清楚才会意识到。

## 入站配对必须由人确认，且窗口只在 `invite create` 运行时打开

与「接收不是一条命令」是**不同**的判断，别把那条照搬过来：接收的信任边界是「已配对」，
而配对正是在建立那条边界。

⚠️ 中间错过一版：曾按「邀请是签名凭证 ⇒ 可以直接接受」实现，被用户一句话推翻——
**邀请只回答「这个请求不是凭空捏造的」，回答不了「出示它的是不是我要配的那台」**。
它会泄露（截图、投屏、日志、旁人抢先扫码），而且是一次性的：被抢走的那次**消耗掉凭证**，
真正的设备再来就用不了了。所以默认必须由人看着对端信息点头。

三条判据：

| 情形 | 处置 |
|---|---|
| 不带凭证的局域网直连 | 直接拒，不打扰用户（唯一依据是「在 mDNS 多播域内」，那不构成授权） |
| 带凭证，且有人在等配对 | 展示对端信息，等人确认 |
| 带凭证，但没人在等 | 拒绝 |

**「有没有人在等」= `swarmdrop invite create` 在不在跑。** 常驻节点自己问不了人（多半在服务单元里，
没有 stdin），所以它把请求经本地通道转交给正在轮询的 `pair` 客户端。于是配对窗口有了一个
用户能直接控制的开合判据：**只有你执行 `pair` 时它才开着**。

`--auto-accept` 是显式的风险交换（脚本 / CI / harness），**`start` 与 `pair` 各有一份**：
前者让常驻节点不经确认台直接接受，后者让等待中的命令不停下来问。**没有它且无法交互时，
`pair` 在生成邀请之前就报用法错误**——生成一张注定无人能确认的邀请，只会让对端白等三分钟。

⚠️ 两处的范围都是「**任一**有效邀请」而不是「刚打印的这张」：邀请跨重启存活、TTL 24 小时，
本机此前发出过而尚未过期的都算数。要收窄到某一张，前提是先有邀请清单与撤销入口（见下文
「仍然缺的」）。

### 拒绝不消费凭证

core 只在 `PairingResponse::Success` 时才 CAS 消费邀请。所以拒掉一个抢配对的请求之后，
**同一张邀请对真正的设备仍然有效**——这条性质是「默认要人确认」能成立的前提，
否则拒绝一次就等于自废武功。已由真机验证：被拒的邀请再次使用配对成功。

### 处理期间必须盯着对端有没有走

`PairWaitNext` 是长轮询（`POLL_TIMEOUT` 15s），而 `accept_one` 把每个连接丢进独立任务，
**任务的生命周期与连接的存活完全脱钩**——客户端被 Ctrl-C 掉之后它照跑不误。后果不是「多等
一会儿」这么轻：

1. 它继续占着确认台的名额（`waiting` 计数）与接收锁，**配对窗口仍算开着**；
2. 这期间到达的配对请求会被它取走、写进一条已经关闭的连接、**就此消失**——对端只能等满
   core 的 170s 才被婉拒；
3. 用户重新执行 `pair` 时，新会话卡在 `rx.lock()` 上排它后面，最长 15 秒里既看不到也接不住
   任何请求。

所以 `accept_one` 在 handler 跑的同时 `select!` 一个 `peer_gone`（读到 EOF 即断开）。
护栏测试 `handling_stops_when_the_client_walks_away` 用 `Arc` 探针观察 handler 的 future
有没有被丢弃——去掉 select 那一侧它立刻红。

### Ctrl-C 不能靠在循环里反复 `tokio::signal::ctrl_c()`

`select!` 每轮新建一个 `ctrl_c()` future，另一分支胜出时它连同注册一起被丢弃；而 tokio 的
信号驱动**在没有监听者时会无条件清掉 pending 标志**，那一刻到达的信号就此蒸发。

配对确认期间正好踩中：`dialoguer` 在 raw 模式下关掉 `ISIG`、自己读到 `\x03` 后补发一个
SIGINT，而那时没有任何监听者。表现是**用户按了 Ctrl-C 只等到一句「已拒绝」，命令若无其事
地继续等下一台设备**，要再按一次才退得掉。

改成进循环前 `tokio::spawn` 一个只注册一次的监听，用 `Notify::notify_one`（会存 permit，
所以确认提示期间到达的信号不会丢）。

顺带一条措辞判据：`prompt::confirm` 返回 `Option<bool>`，**「读不到回答」与「答否」必须分开**
——两者都不放行，但后续动作不同。混成一个 `false` 的结果是屏幕上先打印「已拒绝，仍在等待」
紧接着「已中止」，自相矛盾。

## `Ok` 不等于配对成功

`pair_with_invite` 返回 `(PairingResponse, Option<PairedDeviceCommit>)`。`Ok(..)` 只说明
RPC 问答走完了，答案完全可能是 `Refused`。最初两条路径（本地与经通道）都写成
`.map(|_| ())` / `Ok(_) => Response::Ok`，于是**对端婉拒会被渲染成「配对成功」并退出 0**
——用户要到之后 `send` 找不到设备时才发现，那时已经无从归因。

「配对被拒绝」有独立退出码（6），**不并进「对端不可达」（4）**：两者的处置相反，
一个重试无用（要换一张邀请），一个正是该重试的。

## 数据目录必须 0700，否则本地通道对其他用户敞着

`identity.json` 自己是 0600，但**套接字不是**：`create_dir_all` 走 umask，通常落成 0755，
于是同机的其他用户能连上那条通道——而它能启停节点、列设备、发文件、应答配对请求。
私钥保住了，节点却被别人使唤。

收紧**目录**而不是逐个文件：套接字由 `interprocess` 创建、锁文件由 `File::create` 创建，
逐个 chmod 都留着「创建完到 chmod 之间」的窗口，目录权限则在文件出现之前就已就位。
存量目录也要在每次 `resolve` 时重新收紧（用户可能是从旧版本升上来的）。

边界与本仓既有形态一致：防的是「其他用户」，**不防「同用户下的其他进程」**——那类进程能
直接读走明文私钥并冒充这台设备，再去拦一条本地通道没有意义。这条也解释了为什么本地通道
**刻意不做认证**：能连上就等于能指挥节点，而拦住其他用户的是目录权限。

⚠️ **Windows 没做**：命名管道不经文件系统权限，收紧要构造 SECURITY_ATTRIBUTES，
而 `interprocess` 当前没有暴露那个口子。已知缺口。

## 默认设备名不能用 `OsInfo::default().hostname`

它读的是 `COMPUTERNAME` / `HOSTNAME` 环境变量，而 **macOS 与多数 Linux 上 `HOSTNAME` 是
shell 变量、根本没有导出**（`env | grep HOSTNAME` 零命中）。于是它必然落到兜底值 `Device`，
CLI 的默认名就成了所有安装都一样的 `Device (cli)`——而那个名字正是对面在决定要不要接受
配对时看的第一行。

改用 `whoami::devicename()`（跨平台取系统里那个「电脑名称」：macOS 的共享名、Windows 的
计算机名、Linux 的 pretty hostname），拿不到再退 `whoami::hostname()`。实测从
`Device (cli)` 变成 `yexiyue的Mac mini (cli)`。

⚠️ **先给后缀留位置再截断**：`DeviceName::parse` 自己也会截到 `MAX_CHARS`，但它砍尾巴——
长名字会把 ` (cli)` 整个砍掉，宿主标识就没了。

> 这条缺陷不止影响 CLI：`OsInfo::default()` 是 core 的，桌面端的 `hostname` 字段同样是
> `Device`。桌面端因为 onboarding 会让用户起名字，所以没显形。

## 进度条走 `indicatif`，不要自己 `\r` 刷新

自写版本（`eprint!("\r…")` + `flush`）在终端里看着没问题，代价藏在三处：

1. **非终端时照样输出控制符**。`swarmdrop send … 2>log` 或 CI 里，日志文件变成一行几百个
   `\r` 拼起来的乱码。indicatif 自己检测 stderr 是不是 tty，不是就整个静默
   （实测：管道重定向下 stderr **0 字节**）。
2. **没有速率与剩余时间**。传大文件时这两个数才是用户在等的答案，而它们要维护时间窗口，
   不是「再加一行 `format!`」能顺手做对的。
3. **被日志插一行后留残影**。stderr 上还有 tracing 的输出，`\r` 只回到行首、不清行。

增量成本只有 indicatif 自己：`console` 早就在依赖树里（dialoguer 带的）。

⚠️ **收尾用 `Drop` 而不是 `finish()` 方法**。等待终态的循环有四条出口（完成 / 失败 /
拒绝 / 事件通道断开），只有第一条会让人自然想起要收尾——自写版本就漏了另外三条，于是
「传输失败: …」直接印在那条没换行的进度行后面。新增一种终态时会再漏一次。

⚠️ **模板写错是静默降级**：`ProgressStyle::with_template` 失败时代码回退到默认样式，
进度条照常出现、只是速率和剩余时间不见了，没有任何报错。`render/send.rs` 有一条测试钉住
模板可解析。

**相关文件**：`crates/cli/src/render/send.rs`（`Progress`）、`crates/cli/src/runtime/transfer.rs`

## 渲染层的三个共享辅助，别再各写一份

`render/` 各模块吃的都是 JSON（数据可能来自本进程，也可能来自通道对面的常驻节点），
于是「取一个字段并降级」「设备名为空给占位符」「字节数转人类可读」这三件事每个模块都要做。
它们**都在 `render/mod.rs`**：`text_or` / `blank_as_placeholder` / `human_bytes`
（后者在 `render/send.rs`，因为发送结果是它的主场）。

这条是踩出来的：曾经 `text_of` 有三份、`blank_as_placeholder` 有两份、字节换算有两份，
而**三份 `text_of` 的行为并不一致**——两份把非字符串 `to_string()`、第三份用 `as_str()`
当作缺失处理。后者会把 `natStatus` 这种「序列化后是带标签对象」的字段显示成占位符，
一个有值的字段看起来像没值。

占位符确实该由调用点决定（状态字段缺失是「未知」，清单字段缺失是「—」），所以
`text_or` 收一个 `fallback` 参数——**差异化的是参数，不是第二份实现**。

## 交互命令的默认日志要压住第三方噪声

`libp2p_mdns` 每次广播都为放不进 mDNS TXT 记录的长地址各打一条 **WARN**（本仓的 relay
circuit 地址必然超长，一次十几条），`rtc` / `webrtc` 对每个不属于自己的 STUN 包各打一条。
合起来每秒几十行，而 CLI 的日志**直接落在用户终端上**。

实测后果：配对确认框出现不到一秒就被顶出可视区——那是全流程中唯一要求用户看清内容再决定
的一屏，被刷走等于那道确认不存在。

现在默认过滤器一律带 `libp2p_mdns=error,rtc=error,webrtc=error`，`pair` 另外整体压到
`warn`（不开 `swarmdrop=info`）。**是提门槛不是关掉**，`RUST_LOG` 照样能调回去。
⚠️ 三条都必须单列——`EnvFilter` 按字符串前缀匹配，它们都不以 `swarmdrop` 开头。

## 设备列表要用 `DeviceFilter::Paired`，默认的 `All` 是错的

`get_devices(Default::default())` 取的是 `All` = **本次运行发现的对端**，与「已配对设备」
是两个集合，两个方向都会错：

- 刚配对完的设备因为还没被发现而**不出现**——一次性命令每次都新起节点，这是常态
- 局域网里路过的陌生设备**反而列了出来**（`isPaired: false` 混在列表里）

它是用户确认「到底配上没有」的唯一手段，答错等于配对功能不存在。

## 默认日志过滤要写 `swarmdrop` 而不是 `swarmdrop_cli`

tracing 的 target 取 `module_path!()`，而 **bin target 的 crate 根是 bin 名**
（`swarmdrop`），不是 package 名（`swarmdrop-cli`）。`EnvFilter` 按字符串前缀匹配，
`swarmdrop::runtime::pairing` 不以 `swarmdrop_cli` 开头 —— 于是本程序自己的日志
**一条都不会出现**，而那正是无人值守场景下唯一的排查凭据（常驻节点接受了配对、
拒绝了直连请求，日志里什么都没有）。

反过来 `swarmdrop` 这一条同时覆盖 `swarmdrop_core` / `swarmdrop_net` / 其余
`swarmdrop_*`，单列它们纯属冗余。`cmd::tests::default_filter_covers_this_crate` 看守这件事
——写错前缀不报错、不 panic，只是日志全部消失。

## 命令面：三条命名规则 + 三档资源需求

**命令怎么命名**（`cmd/mod.rs` 的模块文档是权威）：

1. 操作对象是**程序自身且为单例** → 平铺动词：`start` / `stop` / `status`。
2. 操作对象是**本程序管理的一个集合** → 名词 + 动词两级：
   `invite` / `device` / `inbox` / `transfer`。
3. 操作对象**不归本程序管理** → 平铺动词：`send`（对象是文件系统里的文件）。

规则 3 是 `send` 唯一的豁免依据。**别把它读成「高频所以平放」**——那条理由不可判定，
下一个人会用它把 `device list` 也拉平。同一集合上的动作不得做成开关（`invite --list`），
层级不得超过两级。三条都有断言测试看守。

⚠️ **`pair` / `devices` / `inbox get` 已彻底删除，连别名都没留**（2026-08-19）。
CLI 从未发布过，那是唯一一次能干净改名的窗口；留别名的结局看 Docker——`docker ps` 与
`docker container ls` 至今并存，短的赢了，新的那套没人用。

**每条命令走哪个取数入口**，由一句可判定的问句决定：
**这条命令会不会导致一个数据包离开本机？**

| 入口 | 行为 | 命令 |
|---|---|---|
| `NodeAccess` | 有常驻走通道，否则起临时节点 | `send` · `invite create` · `invite use` |
| `RecordAccess` | 有常驻走通道，否则直连本机记录，**永不起节点** | `device list/forget` · `invite list/revoke` · `inbox list/show` · `transfer list/show` |
| （都不用） | 只碰本地文件系统 | `inbox export` 的文件复制部分 |
| 自成一路 | 节点生命周期本身 | `start` / `stop` / `status` |

**答错不报错**：该起节点的没起，表现是「跑完但一个包都没发」；反过来则让「看一眼本机记录」
变成一次连引导节点的几秒等待。

⚠️ **护栏只有一条，且必须在进程外**：`tests/without_a_node.rs` 的
`record_commands_never_start_a_node`，用**身份文件在不在**判断进程有没有真的起节点
（`identity.json` 只有节点装配路径才会创建——比计时可靠，耗时会随机器与网络波动）。
**新增一条只读命令就要往它的用例表里加一行**，否则那条命令不在任何看守之下。

> 这里曾经还有一条 `Command::need()` 枚举 + `command_needs_are_deliberate` 断言表，
> 2026-08-19 删除。它**看起来**是第二条护栏，实际是自我验证：断言的是「`need()` 返回
> `Persisted`」，而 `need()` 就是那个声明本身——把 `invite list` 改成去起节点，它照样绿。
> 那套枚举唯一的非测试消费点是一行 `tracing::debug!`，而那行记的是一个可能与实现不符的
> 声明，比没有更误导排查。**分类表作为文档留在上面这张表里，代码里不再有它的副本。**

### `Persisted` 为什么必须先看通道，不能无脑直连库

`migration` 的连接**不设 `journal_mode`**（sqlx 默认，走 SQLite 的 `delete` 模式），
那模式下**写事务会阻塞所有读**。常驻节点接收文件时一直在写，此时直连库的 `inbox list`
会以 `database is locked` 失败。

所以判据是「有没有常驻节点」而不是「要不要节点」：

- 有常驻节点 → 走通道（它自己读，没有并发问题）
- 没有 → 直连本机记录（此时没有并发写者，且不必为看一眼记录去起一个 P2P 节点、连引导节点）

通道刚断开的竞态也要兜住：回落直连而不是报错——那一刻恰恰是直连最安全的时候。
**这段竞态处理只存在 `RecordAccess::query` 一处**；此前它摊在每条命令里各写一遍，
漏掉的那份会在节点关停的瞬间报一个与真实原因无关的错。

### 无节点的邀请清点/撤销：建一个孤立的 `InviteRegistry`，别查表

`Records::invites()` 的做法是 `InviteRegistry::new(SqlInviteStore)` + `load(now)`
（正是节点启动时做的事），然后照常调 `list_active` / `revoke_by_hash`。两条理由：

1. **领域规则零重复**。「未过期、非已撤销、按创建时刻倒序」住在 `list_active` 里，
   直接写 SQL 过滤就会长出第二份实现。
2. **`revoke_by_hash` 本来就依赖内存表**。它 `invites.get_mut(&hash)`，查不到直接
   `return true`（no-op）。**不 `load` 就撤销 = 报告成功但什么都没发生**——最坏的一种
   失败形态，而且用户要到那张邀请被人用掉才发现。
   由 `revoking_without_a_node_survives_restart` 看守。

`crates/invite` 因此一行未改（后来为 `PairInvite::id()` 加过方法，与本条无关）。

### `status` 不得起临时节点

它曾经走通用的「无常驻就起临时节点」路径，于是在没有节点的机器上执行 `swarmdrop status`
会**启动一个节点、报告 `Running`、再把它关掉**——用户问「节点在跑吗」，得到的答案是这次
提问自己造成的。现在无节点时直接返回 `NetworkStatus::default()`（即 `Stopped`）。

spec 的「临时节点期间的状态查询 → `Running`」不受影响，反而更自洽：那条说的是**别的**
命令（如 `send`）持有临时节点的期间，此时通道活着，`status` 经它取到 `Running`。

## 交互补全：三态，且「问不了」必须立刻退出

`clap` 负责解析，`dialoguer` 负责补全缺的那个参数（撤销哪张邀请、解除哪台设备）：

- 给了参数 → 直接执行。**人和程序走同一条命令**，不为 agent 另开一套。
- 没给 + 问得了人 → 弹选择菜单。
- 没给 + 问不了人 → **立刻报用法错误退出（码 2）**。

第三条是硬要求。`dialoguer` 在非 TTY 下可能去读一个**永不到来的 stdin**，在管道或 CI 中
表现为**永久挂起且日志无异常**——最难诊断的一种失败。同样**不许猜默认值**（「撤最新那张」）：
撤销不可逆，猜错没有补救。由 `tests/without_a_node.rs` 与
`choosing_without_a_terminal_fails_fast`（带超时）看守。

`prompt::can_ask()` 是唯一判据，四条缺一不可：stdin 是 TTY、stderr 是 TTY、未开 `--json`、
未给 `--no-input`。它做成**进程级状态**（`prompt::configure` 在分派前调一次）而不是逐层传参
——这两位是环境事实不是命令参数，让它们跟着调用链走等于要求每个可能提问的函数都多带两个
布尔，而漏掉任何一处的表现就是「在不该问的地方问了」。
⚠️ 测试里改它要拿 `prompt::interaction_test_guard()`，`cargo test` 默认并行。

### `--json` 不等于 `--no-input`，`invite create` 上尤其不能混

两者都关掉交互，但**净效果完全不同**，`prompt.rs` 把它们存成两个独立的原子量正是为此。

`invite create --no-input`：命令照常运行、守着邀请，期间到达的请求一律拒绝。有明确用途
——只想把码摆出来看看，不打算真的配对。

`invite create --json`：**没有可用的形态**。它会生成一张注定配不上的邀请（每个入站请求都
被拒），然后**永不返回**；更糟的是 `render_declined` / `render_request_expired` 的 json 分支
会持续往 stdout 追加对象，破坏「结构化模式下 stdout 只能有最终结果」，调用方那边表现为
一个读不完的流。所以它必须**快速失败**并指出唯一可用的组合（`--auto-accept`）。

⚠️ 用 `interaction_declined()`（= `NO_INPUT || STRUCTURED`）一把抓就会丢掉这个区分——
2026-08-19 这么写过一次，两个开关合流后 `--json` 落进了「照常运行」那条路。

### `invite create --auto-accept` 仍然是交互命令

`--auto-accept` 免去的是「每条入站请求要人点一次确认」，**不是「没人在看屏幕」**：
这条命令的产出就是那张二维码，得有人拿另一台设备去扫，而且要守着等、以分钟计。

放开日志（`swarmdrop=info`）的净效果是临时节点起来后 `NetworkStatusChanged`（二十来个
字段的结构体）与 `DevicesChanged`（core 自己的文档写着「每秒可能刷新多次」）在几秒内把码
顶出可视区。

真正的无人值守是 `start --auto-accept`（常驻节点），那条本来就不算交互命令。
2026-08-19 按「无人值守最需要日志」的直觉改过一次，是错的，`interactive_commands_are_quiet_by_default`
里现在有一条用例钉住它。

### 通道服务端不得产出 `Aborted`

中止是**本地的用户动作**（Ctrl-C），通道对面没有立场替用户宣布中止。两条理由：

1. `CliError::Aborted` 的 Display 是固定的「已中止」，`from_code` 还原时**丢掉 message**
   ——服务端配的任何解释都会静默消失。
2. 它的退出码是 130（`128 + SIGINT`），脚本按惯例读作「人按了 Ctrl-C，别重试」。
   传输因「常驻节点被停」而中断时若报 `Aborted`，一次本该恢复的中断就被当成用户主动放弃。
   那条路径现在报 `TransferFailed`。

由 `runtime/ipc.rs` 的 `aborted_is_never_produced_by_the_server` 看守（它读 `cmd/start.rs`
的源码找 `CliError::Aborted`）。

### `--no-input` 与 `--auto-accept` 方向相反

| flag | 语义 | 遇到入站配对请求 |
|---|---|---|
| `--no-input` | 不要问我 | **拒绝**（fail-closed） |
| `--auto-accept` | 不用问，一律放行 | **接受**（fail-open） |

两者同时给出时 `--auto-accept` 生效——它是对配对行为的明确指令，而 `--no-input` 只声明
不弹提示。**别把前者读成「自动通过」。**

还有一条区分：`invite create` 在「显式关掉交互」时**照常运行**（生成邀请、守着、拒绝所有
请求），只在「环境问不了人**而且**用户什么也没说」时才报用法错误——那时这条命令注定做不成
它该做的事。判据是 `prompt::interaction_declined()`。

### 邀请标识：生成时就要打出来，撤销接受唯一前缀

邀请清单里**没有邀请串本身**（capability 明文不落盘），能区分多张的信息只有标识与时刻
——一分钟内发两张就分不出哪张发给了谁。所以 `invite create` 必须在输出里给出这张的标识，
它是「刚发错人、立刻撤回」这条主场景可用的前提。桌面端靠视觉时序绕过（生成后列表自动刷新，
最上面那条就是），CLI 没有那个连续上下文。

撤销接受标识前缀（≥4 位），**撞车时列出全部候选并拒绝，绝不代为挑一张**——撤销没有 undo。

## 通道请求的两份实现：已收敛 5/8，剩 3 条是已知负债

每条「只读本机记录」的命令有两条取数路径——无节点直连库、有常驻节点经通道问它。
实现分别住在 `runtime/{devices,invites,transfers,inbox}.rs`（前者）与 `cmd/start.rs`
的 `impl RequestHandler`（后者）。**没有任何东西强制两者一致**，而不一致的表现是
「同一条命令的行为取决于此刻恰好有没有常驻节点在跑」。

已经咬过一次：`transfer show <格式合法但不存在的 id>` 无节点时退 2（Usage）、
经通道时退 3（`unpack` 把所有 `Response::Error` 压成 `NodeUnavailable`）。
根因是 wire 上没有错误分类，已修（见下一节）。

**收敛做法：函数收端口而不是收「取端口的方式」。**

```rust
// 收 &dyn TransferStore，两条路径各自把自己的 store 传进来
pub async fn list(store: &dyn TransferStore) -> CliResult<Vec<TransferProjection>>
```

常驻节点那侧的 store 已经握在 `TransferManager` 手里；收 `Records` 会逼它另开一个
数据库连接读同一份数据，于是只能把逻辑再抄一遍（连错误措辞一起）。
`devices::forget(records, Option<&RunningNode>, peer_id)` 是另一种形状——取数源做成参数。

| 请求 | 状态 |
|---|---|
| `TransferList` / `TransferShow` | ✅ 收端口，一份实现 |
| `InboxList` / `InboxShow` | ✅ 收端口，一份实现 |
| `DeviceForget` | ✅ 取数源作参数 |
| `DeviceList` | 刻意两份——活节点那份带在线状态，记录那份带不出来（`online: None` 是「未知」不是「离线」） |
| `InviteList` / `InviteRevoke` / `InviteRevokeAll` | ⚠️ **仍是两份** |

**邀请那三条为什么没收**：无节点路径操作的是现建的 `InviteRegistry`，常驻路径操作的是
节点内存里那张表，而后者只经 `PairingManager::list_invites()` /
`revoke_invite_by_hash()` 两个门面暴露——**那两个门面被桌面、移动、Web 三端共用**
（`src-tauri/src/commands/pairing.rs`、`mobile-core/src/pairing.rs`、`crates/web/src/node.rs`）。
给 core 加一个 `pub fn invites(&self) -> &InviteRegistry` 会变成三套 API 并存，
而那正是本仓一直在避免的 Docker 双轨形态。40 行重复换 API 面扩张，不划算。

⚠️ 两份就要**逐字同构**。客户端那份曾经多一步 hex 往返（`InviteRow.id` → bytes），
带来一条「解析失败就跳过且不计数」的分支，而服务端没有——同一条 `--all` 因此可能
在两条路径上报出不同的撤销条数。现已改为直接取 `InviteSummary::capability_hash`。

**更彻底的做法（未做）**：把 `impl RequestHandler` 整个挪进 `runtime/`，取数源落成
`enum Source { Records(..), Node(..) }`，Request 与实现一对一。那样「要不要起节点」
由取数源本身决定，不必另行声明。改动面是 15 个 arm，超出了当时那次改动的范围。

## 通道上的失败必须带分类，否则退出码取决于「此刻有没有常驻节点」

`Response::Error` 曾经只有 `message: String`，客户端的 `unpack` 于是一律
`CliError::NodeUnavailable`。后果：

```
swarmdrop transfer show <格式合法但不存在的 id>
  无常驻节点 → 本地路径 CliError::Usage           → 退出码 2
  有常驻节点 → Response::Error → NodeUnavailable  → 退出码 3
```

`swarmdrop transfer show $id || retry_if_node_down` 的行为因此取决于此刻恰好有没有
节点在跑——而 spec「退出码区分失败原因」的整个前提是**脚本不必解析文本**。

现在 `Response::Error { code: Code, message }`，`Code` 派生 `Serialize`，
`CliError::from_code` 在客户端按分类重建。三条纪律：

1. **服务端一律走 `Response::err(err)` / `Response::usage(msg)`**，不手写
   `Response::Error { .. }`——分类从 `CliError::code()` 取，手填等于给一次填错的机会，
   而填错**不报错**，客户端只是拿到一个错误的退出码。
2. **客户端一律 `CliError::from_code(code, message)`**，不要自己硬编码一个分类
   （`cmd/send.rs` 曾经一律 `TransferFailed`，把「对端不可达」也吞了）。
3. **兜底响应也要带 `code`**——`ipc.rs` 里「响应序列化失败」那条手拼的 JSON 少一个字段，
   客户端就连它都解析不出来，表现是一直等到超时。

护栏：`runtime/ipc.rs` 的 `error_classification_survives_the_wire`（五种分类往返）与
`server_side_usage_errors_keep_their_code`。

> 副产品：`cmd/transfer.rs` / `cmd/inbox.rs` 里「先在本地校验格式」那两步不再是
> 唯一防线。它们仍留着——无节点时能立刻给出用法错误，省一次通道往返——但即使漏了，
> 分类也会从服务端正确地带回来。

## 邀请标识必须从串本身算，不能「取清单第一条」

生成邀请后要把标识打出来（「发错了可以撤回：swarmdrop invite revoke {id}」）。曾经是发一条
`InviteList` 取回清单、拿 `rows.first()`——**那是错的**：`list_active` 按 `created_at`
（Unix **秒**）倒序，而底层是 `HashMap`，**同一秒里生成的两张谁排第一完全任意**。

后果不是显示错乱而是撤错张：用户照着提示撤，撤掉的是无辜的那张，想撤的那张仍然有效——
一个已经泄露的一次性凭证于是撤不掉，而这正是撤销功能存在的全部理由。

标识就是 `sha256(capability)` 的 hex，而 capability 就在手上那个串里：

```rust
let id = PairInvite::decode(&invite).ok().map(|parsed| parsed.id());
```

`PairInvite::id()` 在 `crates/invite`，两条护栏钉住它（`id_matches_the_registry_listing`
与注册表算出的一致、`ids_are_distinct_for_invites_minted_at_the_same_instant`）。
副产品是省掉一次 IPC 往返。

## 分发（`dist`）的四个坑

1. **tag 必须带包名**：`cli/swarmdrop-cli-v0.1.0`。**这条是本节最贵的一条**——选错不报错，
   发出去的 release notes 会是另一条版本线的内容。见下方「tag 形式决定 release notes 取哪份
   CHANGELOG」。也不要用连字符裸形式 `cli-v0.1.0`：`tag-namespace = "cli"` ≠ 包名
   `swarmdrop-cli` 时，dist 会把整串当版本号解析并报
   `Couldn't parse the version ... unexpected character 'c'`。
2. **workspace 里其余 bin crate 必须显式排除**。`dist-workspace.toml` 的 `members` 只列 CLI，
   但 dist 仍会扫描每个有 bin 的包；给它们补了 `repository` 之后就会被纳入发布计划
   （`dist plan` 多 announce 一个 v0.23.0，那是 Tauri 桌面端）。
   `src-tauri` 与 `crates/bootstrap` 因此各带一段 `[package.metadata.dist] dist = false`。
3. **`tag-namespace` 顺带解决了 workflow 命名冲突**：产出的是 `cli-release.yml`，
   不会覆盖既有的 `release.yml`（桌面 Tauri 发版）。改配置后重跑 `dist generate` 前，
   建议先记一份 `release.yml` 的 sha256 用于核对。

### tag 形式决定 release notes 取哪份 CHANGELOG

dist 的 **announcement 粒度由 tag 决定**，粒度又决定 release notes 从哪份 CHANGELOG 取正文。
两份 changelog dist **都会扫到**（`-v info` 能看到两条 `Found CHANGELOG at`），选哪份只看粒度：

| tag | 粒度 | notes 来源 |
|---|---|---|
| `cli/v0.1.0` | 剥掉 namespace 只剩版本号 → 判定「整个 workspace 统一发布」 | **仓库根** `CHANGELOG.md`（桌面版本线） |
| `cli/swarmdrop-cli-v0.1.0` | 带包名 → 包级 | `crates/cli/CHANGELOG.md` ✅ |

它**不报错**——dist 在根 CHANGELOG.md 里按版本号找到桌面端的同号条目，日志照样打
`successfully parsed changelog!`。首个 CLI 版本 `cli/v0.1.0` 就是这么把桌面 2026-02-14 的
「限制 Android 构建目标为 aarch64」「配对请求超时设为 180 秒」发成了 CLI 首版说明，
标题还带着桌面的日期 `0.1.0 - 2026-02-14`。事后用 `gh release edit --title --notes-file` 修的
（assets 挂在原 tag 上，所以 notes 里的下载 URL 段要保留原样，只换 changelog 那一段）。

**三条版本线共存把它从「可能」变成「必然」**：桌面已走到 0.16.x，CLI 从 0.1.0 重新起步，
CLI 的 0.1.x / 0.2.x 一路都会撞上桌面的历史条目。

`DistMetadata` 的 73 个配置项里**没有** changelog 路径字段（`changelog` 只存在于 dist 自有
manifest 的 `[package]` 表，那是给非 cargo 的 generic 包用的），所以这件事无法用配置消除
——tag 形式就是唯一的开关。因此发版走 **`./scripts/release-cli.sh`**：它从
`crates/cli/Cargo.toml` 读版本、构造包级 tag，并在打 tag 前把 `dist plan` 解析出的正文
前三行回查 `crates/cli/CHANGELOG.md`（判据是内容而非配置，dist 取错了就一行都对不上）。
`--check-only` 只校验。护栏本身验证过：把脚本里的 tag 改回 `cli/v$VERSION`，它会红。

**改 tag 形式不影响 workflow 触发**：模式仍是 `dist generate` 产出的
`'cli**[0-9]+.[0-9]+.[0-9]+*'`（改配置注释后重跑 generate 零 diff），`**` 段吃掉
`/swarmdrop-cli-v`，版本号照常匹配。三条版本线的隔离性也没变。

参照实现：`../SwarmHive/dist-workspace.toml`（同家族项目，配置与踩坑注释可直接对照）。

### 本地能验证到哪一步

装上 zig + cargo-zigbuild + cargo-xwin 之后，`dist build --artifacts=all` 能在 macOS 上
产出**全部六个平台**的归档、installer、npm 包与 homebrew formula——不需要发布。

⚠️ 过程中会打印 `× unable to run linkage report for <linux-target> on macos`，
**那是非致命警告**：退出码仍是 0、产物齐全。CI 用 `matrix.runner` 按 target 分配原生
runner，不会走到那条路径。第一次见到它很容易误判成「本地验证不了」而直接去发版。

带 tag 构建才能验证下载地址：`dist build --tag cli/swarmdrop-cli-v0.1.1`。不带 tag 时
npm 包里的 `artifactDownloadUrls` 是 `releases/download/v0.1.1`，**少了 namespace 段**，
带上才是正确的 `releases/download/cli/swarmdrop-cli-v0.1.1`。
（0.1.0 的 assets 挂在旧形式的 `cli/v0.1.0` 下，那个 tag 保持原样——已发出的 installer
与 npm 包里的 URL 都指向它。）

## 接收落点

默认 `<下载目录>/SwarmDrop`，`SWARMDROP_RECEIVE_DIR` 覆盖。
**不落进数据目录**——那是应用私有区，用户在文件管理器里翻不到，收到的文件等于丢了。
用环境变量而非配置文件做覆盖：命令行宿主常跑在脚本与服务单元里。
