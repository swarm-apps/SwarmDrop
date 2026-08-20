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

`invite create` 生成的邀请里带的是**签发者当时的可拨地址**。所以：

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
没有 stdin），所以它把请求经本地通道转交给正在轮询的 `invite create` 客户端。于是配对窗口
有了一个用户能直接控制的开合判据：**只有你执行 `invite create` 时它才开着**。

`--auto-accept` 是显式的风险交换（脚本 / CI / harness），**`start` 与 `invite create` 各有
一份**：前者让常驻节点不经确认台直接接受，后者让等待中的命令不停下来问。**没有它且无法
交互时，`invite create` 在生成邀请之前就报用法错误**——生成一张注定无人能确认的邀请，只会让对端白等三分钟。

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
3. 用户重新执行 `invite create` 时，新会话卡在 `rx.lock()` 上排它后面，最长 15 秒里既看不到也接不住
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

⚠️ **Windows 没做**，见下一节——那里的通道根本不在数据目录里，这道 0700 管不着它。

## Windows 的本地通道是命名管道，不是数据目录下的文件（2026-08-20）

v0.1.0 在 Windows 上**整个 CLI 不可用**：`swarmdrop start` 报
「本地通道路径不可用: not a named pipe path」当场退出，只读命令则静默认不出常驻节点。

根因：通道地址一律取 `<data_dir>/swarmdrop.sock`，而 Windows 的命名管道**不在文件系统里**。
`interprocess` 的 `GenericFilePath` 文档逐字写着：Windows 上只接受 `\\.\pipe\` 开头的路径，
其余（含 `\\?\` 这种绕规范化的写法）一律报错。

**不能改用 `GenericNamespaced` 一把统一**，尽管它正是为跨平台命名设计的。上游文档同样逐条
列了它的映射：Windows 前缀 `\\.\pipe\`、**Linux 落到 abstract namespace**、其余 Unix 落到
`/tmp/`。后两者都不在数据目录里，于是上一节那道 0700 再也挡不住任何人——而这条通道能启停
节点、列设备、发文件、应答配对请求。**它给的是可移植性，安全语义得自己选。**

所以按平台分叉（`DataDir::socket`）：

| | 形态 | 谁挡住其他用户 |
|---|---|---|
| Unix | 数据目录下的域套接字文件 | 目录 0700 |
| Windows | `\\.\pipe\swarmdrop-cli-<16 位 hex>` | 管道默认 DACL（**未实测**） |

管道名从数据目录派生，因为**管道命名空间是全局的**：`--data-dir` 允许同机跑多个互不相干的
实例，名字撞了就等于两个实例互相接管对方的命令。派生有三条要求：同一目录恒得同一名字、
不同目录不撞名、**跨版本稳定**。最后一条决定了**不能用 `DefaultHasher`**——标准库明写它的
算法与种子不保证在版本间不变，换一次就等于旧节点还在跑、新命令却连不上它，而文件锁又不让
新进程起节点，用户陷入「怎么都连不上、也停不掉」。现在用写死在代码里的 FNV-1a，
由 `fnv1a_matches_the_published_vectors`（官方测试向量）钉住。
路径先 `canonicalize` 再转小写：Windows 文件系统大小写不敏感，`C:\Users` 与 `c:\users` 是
同一个目录，得到两个名字就等于把一个实例分裂成两个。

### 连带的两处

- **陈旧残留只有 Unix 要清**（`single::clear_stale_channel`）。域套接字是文件、不随进程退出
  消失；Windows 的命名管道由内核在最后一个句柄关闭时回收，那里 `socket_path` 也根本不是
  文件系统路径，**不能去 `remove_file` 它**。
- **访问控制是已知缺口，但不是「上游不给口子」**。此前注释写作「`interprocess` 没有暴露」
  ——**是错的**：`os::windows::local_socket::ListenerOptionsExt::security_descriptor()` 配
  `SecurityDescriptor::deserialize()`（吃 SDDL 字符串）就是那个口子。没顺手补上是因为
  **补错的失败形态比缺口更糟**：SDDL 写错时 `deserialize` 返回 `Err`，要么让 `start` 又一次
  起不来，要么被静默忽略而只是看起来做了防护；而 `CREATOR OWNER` 在非继承 DACL 里是否按
  预期生效、默认 DACL 现在放行到什么程度，**本仓一条都没实测过**。要做就得在真 Windows 上
  验证后再合。

⚠️ **`cfg(windows)` 的代码在别的平台上连语法都不检查。** 写这段时就踩了一次：一个 raw
string 以反斜杠结尾（`r"\\.\pipe\"`，编译错误），而 macOS 上 `cargo test` 全绿。
本机也**交叉编译不了**验证（`cargo check --target x86_64-pc-windows-msvc` 卡在 `ring` 的 C
代码上，缺 Windows SDK 头文件）。对策是**把 cfg 区域内的代码压到趋近于零**：管道名的构造
（`named_pipe_path`）与哈希都不带 cfg、在所有平台编译，`cfg(windows)` 那侧只剩一行转调，
测试因此在每个平台上都跑得到它们。

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

现在默认过滤器一律带 `libp2p_mdns=error,rtc=error,webrtc=error`，`invite create` 另外整体压到
`warn`（不开 `swarmdrop=info`）。**是提门槛不是关掉**，`RUST_LOG` 照样能调回去。
⚠️ 三条都必须单列——`EnvFilter` 按字符串前缀匹配，它们都不以 `swarmdrop` 开头。

## 设备列表要用 `DeviceFilter::Paired`，默认的 `All` 是错的

`get_devices(Default::default())` 取的是 `All` = **本次运行发现的对端**，与「已配对设备」
是两个集合，两个方向都会错：

- 刚配对完的设备因为还没被发现而**不出现**——一次性命令每次都新起节点，这是常态
- 局域网里路过的陌生设备**反而列了出来**（`isPaired: false` 混在列表里）

它是用户确认「到底配上没有」的唯一手段，答错等于配对功能不存在。

⚠️ **同一个坑有第二处：`runtime::transfer::resolve_target`**（2026-08-20 才发现）。
它把 `--to` 解析成一台已配对设备，却也用了 `Default::default()`——于是**没有常驻节点时
`swarmdrop send … --to <设备>` 必然报「找不到已配对设备」**，而 `swarmdrop device list`
明明列着它：一次性命令每次新起临时节点，那张发现表在发出请求的这一刻还是空的。
`send` 从落地起就带着这个缺陷，直到加 `--text` 时才被撞见（两者共用这个函数）。

**新加任何「按名字或标识找一台已配对设备」的地方，先看这一条。** 判据是一句话：
要的是「配过对的」还是「这次跑起来看见过的」——`Paired` 与 `All` 是两个集合，
不是同一个集合的宽窄。

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
| `DaemonAccess` | **必须有常驻节点，绝不起临时节点** | `transfer pause/resume/cancel`（含面板热键） |
| `RecordAccess` | 有常驻走通道，否则直连本机记录，**永不起节点** | `device list/forget` · `invite list/revoke` · `inbox list/show` · `transfer list/show/watch` |
| （都不用） | 只碰本地文件系统 | `inbox export` 的文件复制部分 |
| 自成一路 | 节点生命周期本身 | `start` / `stop` / `status` |

**答错不报错**：该起节点的没起，表现是「跑完但一个包都没发」；反过来则让「看一眼本机记录」
变成一次连引导节点的几秒等待。

要节点的那一档还要再问一句：**它作用的对象是不是「某个正在运行的节点内存里的东西」？**
是则 `DaemonAccess`——传输的暂停 / 恢复 / 取消动的是活 actor，临时节点里空空如也。
走错这一档同样**不报错**：`transfer pause` 会先花几秒起一个临时节点、连引导节点、
做 NAT 探测，然后在那个节点里报「会话不存在」，而用户会转头去查那条记录是不是被删了。

`transfer watch` 归 `RecordAccess` 而不是 `DaemonAccess` 是刻意的：没有常驻节点时它照样
有用——列出的是等着续传的那几条，正是用户此刻该知道的事。**面板上的动作**另走
`DaemonAccess`，于是「看得见」与「动得了」是两个各自诚实的判断（无节点时按 `p`
就地报一句「先 start」，面板不退）。

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
撤销不可逆，猜错没有补救。由 `tests/without_a_node.rs::missing_argument_in_a_pipe_exits_
with_usage`（进程外，覆盖每一个可缺省的参数）与 `prompt::pick` 里那两条带超时的单测看守。

### 三态骨架只有一份实现：`prompt::pick::Picker`（2026-08-20）

七个参数可以缺省（`send` 的两个、`invite use/revoke`、`device forget`、`inbox show/export`、
`transfer show`），三态要是摊在每条命令里各写一遍，就是七处近乎一样的 `match`——而**漏掉
第三态不报错，只在管道里挂住**。现在命令面只写一份声明：

```rust
Picker {
    fetch: async || devices::list(access).await,  // 候选集从哪来
    label: render::device::menu_line,             // 菜单一行长什么样
    prompt: "解除与哪些设备的配对？",
    empty: "本机还没有已配对设备",                  // 候选集为空
    unavailable: "请指定…（当前环境无法交互选择）",   // 问不了人
}.many(&targets, locate).await?                   // 或 .one(arg, locate) / .menu()
```

三处措辞**必须分开**：`empty` 是「没得选」，`unavailable` 是「没法问」，用户的下一步动作
完全不同（去配一台设备 vs 换个终端 / 加参数）。

⚠️ 但**判断顺序是「问不了人」在前**：不可交互时立刻退出，**不为了分辨是哪一种而先取
一次候选集**——那次查询（打开数据库、跑迁移、把整张表读回来，或一次通道往返）的结果
只会被丢掉，而 spec 要求的正是「立即退出」。代价是管道里空机器上报的是「无法交互」
而非「没有候选」，那条措辞的价值本来也只在有人看屏幕时才兑现。
`a_pipe_never_pays_for_the_candidate_set`（fetch 一被调用就 panic）钉住这个顺序。

### 「参数是精确值」那一态**不在** Picker 里（2026-08-20 修正）

`transfer show <会话标识>` / `inbox show <条目标识>` 的参数是完整 UUID，自己就能定位，
**不该为了查一条记录先把几百条拉回来**。那条路径因此压根不构造 `Picker`：

```rust
let record = match id {
    Some(id) => show(&access, id).await?,   // 直查
    None => picker(&access).menu().await?,  // 菜单
};
```

⚠️ 最初把它做成了 `Locator::Direct` 变体——**错在返回类型**：`one()` 承诺给「一行记录」，
而那条路上只有一个字符串，于是要伪造一个**只带标识、其余字段全空**的假行，再靠一句注释
维持「调用方只读 id」这个不变量。谁哪天多读一个字段（比如导出前判 `missing`），菜单那条路
给出正确值、参数那条路给 `null`，**编译器一声不吭**。现在「精确参数不取数」是结构性事实，
不是靠一条 panic 测试看守的行为。

顺带修掉一处真实浪费：`transfer` 的清单与详情**是同一个类型**（`TransferProjection`），
所以菜单选中的那一行本身就是详情，此前还要再查一次。⚠️ **收件箱不是**：清单是
`InboxItemSummary`、详情是 `InboxItemDetail`，选完必须再查一次——别顺手一起改。

### 批量动作要一次送到底，别在命令面循环

`invite revoke a b c` / `device forget x y` 的通道动词**收复数**（`hashes` / `peer_ids`）。
一度写成「客户端循环逐个发」，代价在无常驻节点时尤其大：每一次撤销都要新开数据库连接、
跑一遍迁移、把整张邀请表读回内存（还带一次 prune 写事务），撤 3 张就是 4 遍；解除配对
则是把 `paired-devices.json` 读两遍写一遍 × N。

一个可判定的信号：`device forget` 那版循环里，`remaining` 只能取「最后一次」的值并配了
一段注释解释——那正是「本该一次调用」被拆成 N 次留下的痕迹。聚合语义（取最后一个
`remaining`、`persisted` 取合取）属于 runtime，不该由命令面在循环里拼。

⚠️ **标识要先全部解析完再动手**：其中一个不合法时一台都不该被解除。批量操作不可逆，
部分执行之后用户既不知道做到了哪台，也无法原样重试。（它只覆盖**解析**失败；某一台
写盘失败仍会留下部分执行，要根治得让存储层支持事务性批量写——`paired-devices.json`
是整体重写，本来做得到，没做。）

⚠️ **目标要去重**（`cmd::dedup_by_id`）。同一条记录有多种写法——邀请标识的不同长度前缀、
设备的名称与节点标识——它们**作为字符串并不相等**，所以只能在解析成记录之后按标识去重。
不去重的后果不是「多做一遍」（两个动作都幂等），而是**虚报**：`revoke abcd abcd1234`
会说「已撤销 2 张」并把同一个标识列两遍，`--json` 的 `revoked` 也跟着翻倍。

⚠️ 两处措辞要如实：`RevokeOutcome.revoked` 严格说是「**送去撤销**的条数」
（`revoke_by_hash` 报的是有没有写穿，不是有没有命中）；而「收复数」省掉的是通道往返，
**不是**那一侧的文件读写——`unpair` 仍是每台一轮「读 → 改 → 原子写回」。

### 「参数缺席 ⇒ 会问人」的穷尽性交给机器

这条规则同时决定日志要不要压到 `warn`（`Command::is_interactive`），两边**必须同步**：
漏了的表现是那条命令的菜单被 info 级日志冲掉，只在真终端上显形。

`every_optional_target_makes_the_command_interactive` 因此**不列命令清单**，而是从
`Cli::command()` 递归找出「带可缺省取值参数」的每一条，构造最小调用去断言。人工列举的
清单在新增命令时只会一直绿着。

⚠️ 判据用 `arg.get_action().takes_values()` 而**不是** `get_num_args()`：后者只在显式设过
`num_args` 时才是 `Some`，而本 crate 一处都没设——用它会让这条测试只扫到三条命令、
其余静默放行（写这条时就先踩了一次，靠末尾那句 `checked >= 7` 才发现）。

> 更根本的解法是把决定权还给 prompt 层（`tracing_subscriber::reload`，提问期间临时压低
> 日志），那样 `is_interactive` 与几张测试表整体消失，也顺带覆盖 `revoke --all` 的确认框
> 与入站配对确认。评估过，**没做**：它要动 main.rs 的订阅者装配与一个全局 handle，
> 而 `invite create` 仍需要「整条命令安静」的特判。要做就单独做。

### 多选：一次能处理几条，参数侧也要收几个

撤销邀请与解除配对**天然是复数**（清理时一次好几条）。只能单选的话，用户得把同一条命令
敲 N 遍，每遍都要重新看列表、重新认标识。所以 `invite revoke` / `device forget` 走
`MultiSelect`，参数侧同步改成 `Vec<String>`——**交互能多选而参数只收一个是不对称的**，
脚本那侧只能循环。

两条判据：

- **键位要写进提示语**（`（空格勾选 · 回车确认）`，由 `select_many` 统一附加）。多选菜单画
  出来与单选几乎一样，不说的话用户直接回车得到空选择，然后以为命令什么都没做。
- **空勾选是中止（130）不是成功**。勾零项回车意味着用户看过之后决定不动手，报「已撤销 0 张」
  是在假装做了事。

⚠️ `invite revoke --all` **不能**实现成「取列表再逐条撤」：那是 N 次往返，且这期间新签发的
邀请会漏掉——而 `--all` 服务的正是「不知道哪张泄露了」。它与逐张走两条不同的通道动词。

### 路径输入：自己拆行，**不用 `shlex` / `shell-words`**

`send` 缺文件参数时逐行问，而交互输入框里 **shell 不介入**：拆行、去转义、`~` 展开、Tab
补全全落到自己头上（`prompt::paths`）。两个现成库都在依赖树里，拿来即用似乎理所当然，
**实测三条都不成立**（2026-08-20，shlex 1.3 / shell-words 1.1）：

| 输入 | 两库的结果 | 后果 |
|---|---|---|
| `C:\Users\me\a.txt` | `C:Usersmea.txt` | Windows 上**静默**毁掉路径 |
| `"/tmp/My Doc`（引号未闭合） | `None` / `Err` | 补全在最需要它的时候失效 |
| 任意 | 只给 `Vec<String>` | 拿不到 token 起始位置，补全无从原位改写 |

第一条是因为它们实现的是 POSIX 规则（`\` 是转义符），而 Windows 终端拖入**不含空格**的
文件时给的正是无引号形式；第二条是因为补全发生在用户敲到一半时，那一刻引号本来就开着；
第三条是 API 形状——绕开它可以「解析后重建整行」，但那要求解析成功，于是撞回第二条。

所以自己写一个**够用的子集**。转义与拆分**必须互为逆运算**，否则补出一个带空格的目录名
之后，用户下一次回车会拿到两个半截路径——由 `escaping_round_trips_through_splitting` 看守
（用例里**必须有以分隔符结尾的路径**，那正是目录补全每次的产出形状）。

#### `\` 是不是转义符：**按平台分，不是「按情况分」**

最初写成「`\` 只在其后是空白或引号时才转义」，想同时照顾 Windows 路径与转义空格。
**那个折中是错的**，而且错在一个只有 Windows 上才显形的地方：目录补全总在末尾补一个 `\`
（`C:\dir\images\`），用户接着敲空格再敲下一条路径时，那个 `\` 把空格吃成了字面量，
两条路径合成一条——报错里出现一个用户从没敲过的文件名。

现在按平台切换（`BACKSLASH_ESCAPES`）：

| | `\` | 含空格的路径怎么写 |
|---|---|---|
| Unix | 转义符 | `/tmp/My\ Docs`（拖拽产出的就是它） |
| Windows | **字面量**（路径分隔符） | `"C:\My Docs\a.txt"`（拖拽含空格的文件时终端自己加引号） |

用 `cfg!()` 而不是 `#[cfg]`：**两个分支在每个平台上都编译**，两套规则因此都能在任一平台
上被测到——`cfg` 掉的代码连语法都不检查（本仓在 Windows 通道那处已经踩过一次）。

#### 三个入口，别混用

| 来源 | 用哪个 | 为什么 |
|---|---|---|
| 交互输入框（一行可多条） | `paths::parse` | 补全写回的是**转义过的**形式，必须解回来 |
| 交互输入框（只要一条） | `paths::parse_one` | 同上 |
| 环境变量、配置值 | `paths::expand`（只展开 `~`） | 那里的空格**是路径的一部分**，没有任何转义 |

两条各自踩过：

- `inbox export` 的目录输入开着补全、却只调了 tilde 展开没解转义，于是拿到一条**还带着
  反斜杠**的路径（只在目录名含空格时显形）。现在 `Question::complete_paths()` 由
  `ask_path()` / `ask_paths()` 终结（直接返回 `PathBuf`），拆不开。
- `SWARMDROP_RECEIVE_DIR` 反过来错用了 `parse`：`/home/me/My Files` 被按未转义空白拆成
  两条、取第一条，于是程序**创建** `/home/me/My` 并把收到的文件放进去，用户在 `My Files`
  里怎么找都找不到。环境变量不经 shell，systemd 的 `Environment=` 连引号都会剥掉。
  由 `a_bare_path_keeps_its_spaces` 看守。

`Question::ask` 还**一律 trim，且 trim 之后要再回落一次默认值**：dialoguer 的 `default`
只在回答**完全为空**时生效，而用户敲一个空格再回车时它看到的是 `" "`——非空，于是既不用
默认值、也过得了 `allow_empty(false)`，trim 完成了空串，一路走到「导出到当前目录」
且提示里的目标是一片空白。

### dialoguer 的三个细节

- **主题只构造一处**（`prompt::theme()`，`ColorfulTheme`）。同一条命令里可能连着出现菜单与
  输入框（`send` 就是），各写各的会让选中标记与提示前缀长得不一样。
- **菜单要设 `max_length`**。传输记录与收件箱只增不减，几百条时菜单把整个终端顶掉，
  用户连提示行都看不见。
- **回答会被后续输出复述时要 `.report(false)`**（`Question::no_echo`）。发送的路径紧接着
  逐条回显成 `+ …`，不关的话同一条长路径连着出现两次，而路径长到折行时那两次看起来像是
  「加了两遍」。
- `Input` 要用 **`interact_text_on`** 而不是 `interact_on`：前者是逐键读的行编辑器
  （方向键、Tab 补全都靠它），后者只做一次 `read_line`，Tab 会原样进到字符串里。

### 测试里改交互状态，复位要放 `Drop`

`prompt::configure` 写的是进程级状态，测试之间要互斥（`no_interaction()` 拿的那把锁），
但**光互斥不够**：写在测试体末尾的复位语句在 `assert!` / `expect()` 失败时跑不到，
于是一个失败的测试会把 `NO_INPUT=true` 泄漏给同进程的下一个——那个测试随后以一种
与自己完全无关的方式失败。所以复位在 `InteractionGuard` 的 `Drop` 里。

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
这条命令的产出就是那条邀请链接，得有人把它搬到另一台设备上（或在浏览器里打开它扫码），
而且要守着等、以分钟计。

放开日志（`swarmdrop=info`）的净效果是临时节点起来后 `NetworkStatusChanged`（二十来个
字段的结构体）与 `DevicesChanged`（core 自己的文档写着「每秒可能刷新多次」）在几秒内把
链接顶出可视区。

### 终端不画二维码，出码搬到落地页（2026-08-20）

`invite create` 曾在终端画半块字符二维码并附 `--no-qr` 开关。**两者都已删除**，改为只打印
canonical 链接 + 一句「在浏览器打开它，页面会显示二维码」。

**判据是尺寸，而且没有可调空间。** 邀请体积的大头是签名 64B + 公钥 32B + capability 16B
与 42 字符的链接前缀，地址只占零头 —— 实测（`crates/invite` 里跑一遍 `build_qr`）：

| 邀请里的地址数 | 链接长度 | 模块数 | 半块渲染下的终端占用 |
|---|---|---|---|
| 1（`fit` 的下界） | 295 | 69 | **69 列 × 35 行** |
| 3 | 393 | 73 | 73 列 × 37 行 |
| 5（满配） | 425 | 77 | 77 列 × 39 行 |

也就是说**裁地址救不了**：裁到只剩一条仍是 35 行，标准 80×24 终端一屏半，而
`render_waiting` 之后还要继续往下写。半块（1 列 = 1 模块、1 行 = 2 模块）又已经是长宽比
正确的最密形态：2×2 象限块能把宽度减半，但终端字符本身高约两倍于宽，码会被压成 2:1 的
竖长条，扫码器直接读不出。

**扫码路径没有断，只是换了承载**：canonical 链接指向 `docs/public/p/`，那一页按需渲染
二维码（浏览器没有宽高比与行数约束）。于是那一页的访客变成两种 —— 被邀请方，和**邀请方
自己**（点开只为看那张码）。落地页侧的判据见
[`web-app-frontend.md`](web-app-frontend.md) 的「落地页的二维码」。

⚠️ 顺带一条别再走回头路的：**终端里那版渲染是手写的**（40 行半块字符拼接），而
`qrcode` crate 自带 `render::unicode::Dense1x2`，无 feature gate、行为一致。真要再做终端
码，用库的那个，别再手写。

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

**三条版本线共存把它从「可能」变成「必然」**：桌面已走到 0.23.x，CLI 从 0.1.0 重新起步，
CLI 的 0.1.x / 0.2.x 一路都会撞上桌面的历史条目。

`DistMetadata` 的 73 个配置项里**没有** changelog 路径字段（`changelog` 只存在于 dist 自有
manifest 的 `[package]` 表，那是给非 cargo 的 generic 包用的），所以这件事无法用配置消除
——tag 形式就是唯一的开关。因此发版走 **`./scripts/release-cli.sh`**：它从
`crates/cli/Cargo.toml` 读版本、构造包级 tag，并在打 tag 前把 `dist plan` 解析出的正文
前三行回查 `crates/cli/CHANGELOG.md`（判据是内容而非配置，dist 取错了就一行都对不上）。
`--check-only` 只校验。护栏本身验证过：把脚本里的 tag 改回 `cli/v$VERSION`，它会红。

### release 标题与 latest 归属：`cli-release-polish.yml`

三条版本线发到**同一个** releases 列表，另外两条自带前缀（`SwarmDrop v0.23.0` /
`SwarmDrop Mobile mobile-v0.23.0`），CLI 却是裸的 `0.1.0 - 2026-08-19`。

**`announcement_title` 改不了。** 它是 dist 从 `crates/cli/CHANGELOG.md` 的 section 标题
原样取的，没有对应配置项（`display-name` 只改 `display_name` 字段，管的是 installer 文案，
实测不动 title）。而给 changelog 标题加前缀会**连正文一起丢**——三种写法实测都让
parse_changelog 认不出版本号，title 退化成 tag 本身、notes 变空：

| changelog 标题 | dist 给出的 title |
|---|---|
| `## [0.1.0] - 2026-08-19` | `0.1.0 - 2026-08-19` ✅ 正文正常 |
| `## SwarmDrop CLI 0.1.0 - 2026-08-19` | `cli/swarmdrop-cli-v0.1.0`（解析失败） |
| `## [SwarmDrop CLI 0.1.0] - 2026-08-19` | 同上 |
| `## SwarmDrop CLI v0.1.0 - 2026-08-19` | 同上 |

所以改成 post-process：`.github/workflows/cli-release-polish.yml` 把标题改成
`SwarmDrop CLI v<版本>`。**不手改 dist 生成的 `cli-release.yml`**（那个文件
`dist generate` 会覆盖），而是经 `dist-workspace.toml` 的 `post-announce-jobs`
把这个 workflow 接进去——怎么接、以及为什么**不能**写成 `on: release: published`，
见下面「触发方式」。标题完全由 tag 推出，所以重跑一次结果相同，不需要幂等守卫。

**顺带修 latest 归属。** GitHub 把最新创建的非 prerelease release 标为 latest，于是 CLI
一发版就把仓库首页的 Releases 侧栏和 `releases/latest`（`docs/lib/shared.ts` 的下载入口
指向它）从桌面端手里拿走。

⚠️ **`make_latest=false` 解决不了**：它的语义是「别把**这个** release 设为 latest」，
不是「取消现有的 latest」——latest 总要指向某个 release。实测对 CLI 发
`make_latest=false`（`gh release edit --latest=false` 与 REST PATCH 两条路都试过）之后，
它**仍然是** latest，因为它依然是最新创建的非 prerelease。正解是显式把 latest
`make_latest=true` 交给最新的桌面 release。

⚠️ 查那个 release 时**不要加 `--paginate`**：jq 表达式会对每一页各求一次值，`first`
于是每页出一个，`$desktop` 变成多行 id，PATCH 的 URL 直接废掉。最新的桌面版必在第一页
（API 按创建时间倒序），`?per_page=100` + `head -1` 就够。

**改 tag 形式不影响 workflow 触发**：模式仍是 `dist generate` 产出的
`'cli**[0-9]+.[0-9]+.[0-9]+*'`（改配置注释后重跑 generate 零 diff），`**` 段吃掉
`/swarmdrop-cli-v`，版本号照常匹配。三条版本线的隔离性也没变。

参照实现：`../SwarmHive/dist-workspace.toml`（同家族项目，配置与踩坑注释可直接对照）。

#### 触发方式：为什么不是 `on: release: published`

**这条查了两轮才查全，第一轮的结论是对的但不够，于是又静默失败了一次。**
`cli/swarmdrop-cli-v0.2.0` 与 `v0.3.0` 两次发版，标题和 latest 都是事后手工补的。

失败形态彻底静默：release 正常发出、六平台产物齐全，只是标题仍是裸的
`0.3.0 - 2026-08-20` 且 latest 被它拿走——这个 workflow 存在的全部理由都没兑现，
而 Actions 页面上**没有任何一条运行记录**（`gh run list --workflow=cli-release-polish.yml`
返回空数组，不报错也不提示，看起来跟「还没跑完」一模一样）。

两条**各自独立**的根因：

1. **GITHUB_TOKEN 造出来的事件不触发新的 workflow run。** GitHub 防递归的硬规则，
   例外只有 `workflow_dispatch` / `repository_dispatch`。而 release 正是
   `cli-release.yml` 的 `host` job 用 `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` 跑
   `gh release create` 造出来的（它直接建成已发布状态，不经草稿）。
   **这条换分支、换事件类型都绕不开**，除非改用 PAT。
2. **`release` 事件只从默认分支读 workflow 定义。** 这类事件（同 `issues` / `schedule`
   等除 `push` / `pull_request` 之外的仓库事件）不附带 commit，GitHub 无从知道该拿
   哪一版。`cli-release.yml` 之所以在 develop 上就能跑，是因为它由 `push: tags` 触发，
   那种事件带着 commit。

第一轮只查出第 2 条，把文件合进 `main` 就以为修好了——`v0.3.0` 发版前还专门确认过
「`origin/main` 命中」。它确实在 main 上，然后照样一条记录都没有。**验证「文件在
默认分支上」根本不构成「它会跑」的证据**；能构成证据的只有一次真实的运行记录。

**现在的形态：dist 的 post-announce job。**

```toml
# dist-workspace.toml
post-announce-jobs = ["./cli-release-polish"]
```

`dist generate` 据此在 `cli-release.yml` 末尾生成：

```yaml
custom-cli-release-polish:
  needs: [plan, announce]
  uses: ./.github/workflows/cli-release-polish.yml
  with:
    plan: ${{ needs.plan.outputs.val }}
  secrets: inherit
```

于是它跑在**同一次运行**里：没有事件，就没有 token 限制；也不再依赖「文件躺在 main
上」——`workflow_call` 跟着 tag 所在的 commit 走，与 `cli-release.yml` 同一条规则。
被调用方要写成 `on: workflow_call` + 一个 `plan` 输入（dist 固定传这个），并自己声明
`permissions: contents: write`（可复用 workflow 的权限与调用方取交集，`cli-release.yml`
顶层正好是 `contents: write`）。

tag 与 prerelease 判据从 `inputs.plan` 里取，**不要**用 `github.event.release.*`
（workflow_call 里那个对象不存在）也不必用 `github.ref_name`：

| 字段 | 值（实测 `dist plan --tag=cli/swarmdrop-cli-v0.3.0 --output-format=json`） |
|---|---|
| `announcement_tag` | `cli/swarmdrop-cli-v0.3.0` |
| `announcement_is_prerelease` | `false` |
| `announcement_title` | `0.3.0 - 2026-08-20`（就是要被改掉的那个） |

跟着 dist 自己建 release 用的同一批字段走，就不会有第二套解析。

代价：手动在 UI 上 publish 的 release 不再会被摆正。可以接受——CLI 的 release 一律由
dist 创建。

**可推广的那条**：新增任何按 `release` / `workflow_run` / `issues` 触发的 workflow，
先问「触发它的那个事件是谁造的」。只要是 CI 里用 `GITHUB_TOKEN` 造的，它就不会触发，
且**不留任何痕迹**。能跑在同一次运行里就别拆成两次。

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

## 发送进度：两段，且两段都可能画不出来

### 订阅必须早于 `prepare`，不只是早于 `send_offer`

`prepare`（校验和 + bao 验签树）是一次**长阻塞调用**，几个 GB 的文件要算几十秒，而它的
进度只在事件流里。订阅建在它之后的话，那段时间用户面对的是一个既不动也不报错的终端
——而事件早已发完、无处可订。表现就是用户报的「输入完文件后卡住」。

`prepared_id` 由**调用方**生成正是为了这件事：事件带着它，好让宿主在准备开始之前就认领
得到（桌面端同理，见 `src/stores/transfer-store.ts` 的 `activePrepare`）。

⚠️ **准备期间 drain 掉的事件不会再传给 `wait_for_terminal`。** 可以吞是因为本会话此刻
**还不存在**（`session_id` 要到 `send_offer` 之后才有）。这条前提一旦变了，就必须改成
转发而非丢弃，否则一次极快的传输会把终态事件丢在这儿，命令永远等不到。

### 准备与传输**必须视觉可区分**

跨端契约，不是措辞偏好：准备是本机在算、一个字节都还没上网，传输是字节真的在动。
两者共用同一个视觉原语，正是那次「用户把 1.99 GB 的准备读成传输、进而把新会话读成
『续传从 0% 重来』」的直接原因。桌面与移动端用颜色（灰 / teal），终端这一份用**动词**
（「准备中」/「传输中」），并且准备段**不显示速率与剩余时间**——那两个数在这里量的是
磁盘而非网络。

判据的事实源是 `DESIGN.md` 的 **`### Transfer Progress Contract (cross-platform)`**
（`crates/transfer/src/flow/prepare.rs` 的注释指的就是它），那一节已在 2026-08-20
补入命令行宿主这个第四端。

> 这段一度写着「那一节并不存在」——当时是真的：`e7d9caee` 在重新生成 `DESIGN.md` 时把
> 整个 `## Cross-platform Contracts` 冲掉了（1274 → 186 行、10 个 Contract → 0 个），
> 六天后才被发现并恢复。**同一个 `/impeccable` 重新生成随时可能再来一次**，所以校验方式
> 记在这里：`grep -c '^### .*Contract' DESIGN.md` 应当 ≥ 10。

### 交给常驻节点做时，进度要经通道推回客户端

`send` 有常驻节点时走本地通道，于是准备与传输的事件全在**服务端进程**里，客户端从敲下
回车到传完一个字都没有。这一侧曾经什么都不画，依据是「文件那支自己有真进度条」——
**那句话只对本地临时节点成立**。

中间有过一版是「画个转轮 + 指路 `swarmdrop transfer watch`」。**那是妥协不是答案**：
进度就该在用户此刻看着的这个终端里，让他另开一个终端敲命令是把实现的形状暴露给了用户。

现在通道从严格的一问一答放宽成「一问、**若干条进度**、一答」（`Frame::Progress`）：

- **服务端**经 `ProgressSink` 往同一条连接推进度帧（`ProgressOut::Ipc`）
- **客户端**用 `request_watching` 边读边画（`render::send::RemoteProgress`）
- **渲染只有客户端一份**——服务端画进度等于画进它自己的日志流。两条路径因此长得一模一样

四条不变量：

1. **`Frame` 与 `Response` 必须是两个类型。** 合并（给 `Response` 加个 `Progress` 变体）会让
   每一个调用方的 `match` 都被迫处理一种它那里不可能出现的情况——`request` 早把进度跳掉了。
   那种 `unreachable!()` 分支是纯噪声，而且下一个人会认真去想「这里该怎么办」。
   分成两层之后「拿得到的一定是终态」由类型保证，**既有 match 一处都不用改**。
2. **`Frame` 的三个终态变体逐字抄 `Response`，不许「去重」。** 上面那条只否掉了
   「给 `Response` 加 `Progress`」，没有否掉「收进一个 `Frame::Terminal { #[serde(flatten)] .. }`」
   ——那个更短且编译期保证一样强。不这么写的理由是**线格式**：现在 `Frame` 序列化出来的字节
   与**旧版** `Response` 逐字相同，于是「新客户端 × 旧常驻节点」这个真实窗口里
   （升级 CLI 不会重启常驻节点，`swarmdrop update` 之后尤其如此）旧节点的应答仍然解析得动。
   套一层 `Terminal` 会多一层 JSON 嵌套，兼容性当场消失，**且不报错**。
   由 `a_terminal_frame_is_byte_identical_to_a_bare_response` 钉住。
3. **进度帧与终态帧共用同一个写端锁。** 两边各持一个写端会让两种帧交错成半行，
   客户端的按行解析当场失败。
4. **切换进度条必须先收掉上一个**（`Option` 置空触发 `Drop`）。两条 indicatif 同时活着会
   互相擦掉对方的行——只在真终端里显形，管道与 CI 全绿。

### 推进度**绝不能阻塞调用方**（2026-08-20 修）

`ProgressSink::send` 最初是 `writer.lock().await.write_all(..).await` ——全阻塞。
而它的调用方是 `prepare_with_progress` 那个 `select!` 的**分支体**：分支体挂住时
`prepare` 那条 future 得不到轮询，于是**常驻节点上真正的哈希计算停下来**。
一个客户端的终端流控（Ctrl-S / SIGSTOP / stderr 管道对面停了）因此能卡住服务端上
**别人的**传输，而事件通道是无界的，期间还持续涨内存。

现在三道闸，任何一道都只丢这一帧：`try_lock`（拿不到说明上一帧还在写，进度可以丢、
排队不行）→ 500ms 写超时 → **一次失败即封口**。封口之后连终态帧都不再写：超时的
`write_all` 可能已经写进去半行，再追加一条合法 JSON 只会让客户端读到
`{"kind":"progr{"kind":"data",…}`，解析失败退 3 —— **而那次传输其实是成功的**。
什么都不写让它读到 EOF，得到一个诚实的「节点没有应答」。

⚠️ 「进度是可以丢的、连接不是」这句话此前只覆盖了写**失败**，没覆盖写**慢**。

### 通道的看守测试要往返 `Frame`，不是 `Response`

上线的是 `Frame`（服务端写 `Frame::from(response)`，客户端解析 `Frame` 再 `into_terminal`），
`Response` 一个字节都不过通道。只往返 `Response` 的测试挡不住任何真实漂移——给
`Frame::Error` 的 `code` 加个 `serde(skip)`，测试照样绿，而所有经通道的失败会在客户端解析
失败、一律压成 `NodeUnavailable`(3)，于是同一件事的退出码取决于「此刻有没有常驻节点」。

真正的看守是 `progress_frames_precede_the_terminal_and_do_not_break_plain_requests`：
handler 先推两条进度再返回终态，断言 ① `request` 拿到的是终态而不是第一帧进度；
② `request_watching` 按序收到进度且终态在最后。**把 `request_watching` 的读循环退回单次
`read_line`，它会红**（已实测），而此前那批测试全绿。

### `select!` 里「关掉一条分支」不能靠在分支体里挂起

事件通道关闭后 `recv()` 每次都**立刻**返回 `None`，照着继续循环就是烧满一个核的忙循环。
但在分支体里 `std::future::pending().await` **更糟**：`select!` 的分支体是在**选中之后**
执行的，那时另一条分支（`prepare`）已经被丢弃，于是它再也得不到轮询，**整条命令挂死**。

正解是 `event = events.recv(), if events_open` 这样的**分支前置条件**，`None` 时把标志置
false，让后续迭代只轮询另一条。另一条分支必须永远开着，否则 `select!` 会因「所有分支
都被禁用」而 panic。

由 `a_closed_event_channel_does_not_spin` 看守——它第一次跑就抓到了上面那个挂死版本，
而那个缺陷在正常路径（节点不关停）下**永远不显形**。

## 「同一条规则的第二份实现」是本仓最贵的一类缺陷

三次都长同一个样子：**一份被修好、另一份继续错着，而两份都看起来正常**。

| 规则 | 曾经几份 | 漂移的代价 |
|---|---|---|
| 按名称/标识定位已配对设备 | 2（`devices::resolve_target` + `transfer::resolve_target`） | `DeviceFilter` 用错的修复只落在一份上；另一份还各自漂移出了不同的歧义措辞（一份列出候选标识，一份只报「匹配到 N 台」——用户知道有歧义却无从消歧） |
| 设备显示名 | 3（两处手抄 + 核心的 `OsInfo::display_name`） | 尚未显形，但空名字的占位规则一旦变就会分叉 |
| 收件箱记录的「本地位置」 | 3（CLI 渲染层 · 桌面后端 · 前端 TS） | 同一条记录，桌面复制出来的路径与命令行打印的不是同一个 |

前两条已合并（`transfer::resolve_target` 现在只负责取数与取字段，匹配与措辞都来自
`devices`）。第三条**只能合并两份**——CLI 吃 JSON、桌面吃 typed DTO、前端在 TS 里，
三者取字段的方式天然不同。

### 合并不了时，收敛「判据」而不是「实现」

`crates/transfer/src/inbox.rs` 的 `local_location(entry_count) -> LocalLocation` 只回答
**该取哪个字段**（`Entry(0)` 还是 `Root`），取字段留给各自那侧。两个 Rust 消费者因此共用
唯一的判据，而各自的 JSON / DTO 取法一行没动——**行为零变化，可漂移面归零**。

TS 那份跨语言共享不了，只能在它的注释里指回 Rust 那个函数。这不是护栏，是路标；
但比「两处各写一遍且互不知情」强。

### 判据：这条规则会不会被第二个人在别处重新实现一遍

会，就把它放进领域 crate。**不要等到第二份出现再合并**——那时它已经漂移过一次，
而合并的动作会把「哪一份是对的」变成一个需要考古的问题。
（`resolve_target` 就是这样：合并时得先判断两份的歧义措辞哪个才是想要的。）

## 自更新（`swarmdrop update`）

axoupdater 作**库**用，不用 dist 的独立 updater 二进制（`install-updater` 保持 `false`，
理由写在 `dist-workspace.toml`：那个文件按**包名**叫 `swarmdrop-cli-update`，而 clap 的
外部子命令机制查的是 `swarmdrop-update`，于是「`swarmdrop update` 直接可用」在本仓不成立）。

### 应用名是**包名**，与日志过滤那条方向相反

| 用途 | 取哪个名字 | 为什么 |
|---|---|---|
| install receipt / axoupdater | **包名** `swarmdrop-cli` | receipt 由 dist 按包名写在 `~/.config/<包名>/<包名>-receipt.json` |
| `EnvFilter` 默认过滤 | **bin 名** `swarmdrop` | tracing 的 target 取 `module_path!()`，bin target 的 crate 根是 bin 名 |

两处都对，**别为了「统一」把其中一个改成另一个**。写错 receipt 名不报错——每个用户都会被
判成「不是安装脚本装的」，自更新静默失效，而它看起来只是一句「请用当初安装的方式升级」。
两边各有一条断言测试看守。

### 渠道判据：receipt **且**它指向本可执行文件

只看「receipt 在不在」是错的。失败场景很具体：用户先用安装脚本装过，后来改用 Homebrew
——旧 receipt 还躺在 `~/.config/` 里，跑起来的却是 brew 那份。自更新于是去改写另一个位置
的文件，而用户实际执行的那份纹丝不动。

判据借 axoupdater 自己的 `check_receipt_is_for_this_executable()`（它正是为这个场景写的），
**不要自己推导 receipt 路径**——那要认 `AXOUPDATER_CONFIG_PATH` /
`AXOUPDATER_CONFIG_WORKING_DIR` / `XDG_CONFIG_HOME` / `%LOCALAPPDATA%` 四套规则。

⚠️ **这道校验不能留给 axoupdater 内部那次调用**：它在 `is_update_needed()` 里，不匹配时
返回的是 `false`（「不需要更新」）——于是 brew 用户看到的是「已是最新版本」，哪怕真有新版本。
同一个事实，在渠道判定处能翻成「该用 brew 升级」，在那里只能翻成一句错话。

⚠️ **receipt 必须先判，路径启发式（`/cellar/`、`/node_modules/`）只在 receipt 对不上时才看。**
本仓 `install-path = "CARGO_HOME"`，安装脚本装出来的二进制就躺在 `~/.cargo/bin` 里；
先按路径判会把正常安装认成 `cargo install` 而**永远拒绝自更新**。顺序反了不报错，只是不工作。

### `query_new_version()` 不做比较

名字有误导性：它只是把最新 release 的版本号取回来，**一次比较都不做**，fetch 成功时永远是
`Some`。做比较的是 `is_update_needed()`。只用前者的话，已经是最新的用户会被告知
「有新版本可用：0.2.0 → 0.2.0」。

### 比较基准要换成**编译进二进制**的版本

axoupdater 默认拿 receipt 里记的版本去比，那是「上次 installer 装了什么」而非「此刻跑的是
什么」。二进制被换掉（`cargo install`、手动覆盖）而 receipt 没跟上时，用户会看到自相矛盾
的一屏。`set_current_version()` 必须在 `load_receipt()` **之后**调——前者会被后者覆写。

### 三条版本线混住不影响查找，但会多花请求

axoupdater 按资产名前缀 `swarmdrop-cli-installer*` 过滤，桌面 Tauri 产物不带这类资产，
不会被误认。**但 `cli-release-polish.yml` 主动把 latest 交还给桌面 release**，于是
`/releases/latest` 永远命不中 CLI，每次检查都退回去**分页枚举全部 release**（实测请求打到
`?page=4`）。匿名 GitHub API 限速 60 次/小时，几次调试就能耗尽——这正是启动检查节流到
一天一次的理由。限速时的失败形态是 `403 rate limit exceeded`，原文照带给用户。

### 更新前必须停节点

Windows 根本不让覆盖运行中的可执行文件，installer 会在搬文件那步失败——而那时归档已下载、
旧文件可能已被改名，半途失败比不做更糟。Unix 让覆盖（换的是目录项），失败形态反而更隐蔽：
命令报「已更新」，还在跑的节点仍是旧代码，直到某天有人重启它。

拦截放在**任何网络请求之前**（实测 0.014s 返回），用 `UpdateFailed` 而非 `NodeUnavailable`
——节点没有不可用，恰恰是太可用。

### `start -d` 的子进程要抑制检查

父进程拉起子进程后自己等就绪、打印、退出，所以检查由**父进程**做（子进程的 stderr 是
`Stdio::null()`，提示没人看得到）。子进程用 `SWARMDROP_NO_UPDATE_CHECK=1` 抑制——**不加的话
它会抢先记下时间戳**，把父进程那次挤进节流窗口，于是提示既没显示、又要等满一天才有下次机会。
复用现成的环境变量开关，不新增隐藏参数。

前台与后台两条路径的写法**不同且不能统一**：前台 `tokio::spawn`（紧接着是服务循环，
等一次网络往返等于让节点晚几百毫秒开始接受连接），后台 `await` 带 3 秒超时（父进程马上退出，
spawn 出去的任务会随它消失）。

### 本地怎么验证

造一份假 receipt + `XDG_CONFIG_HOME` 指向临时目录即可，不必真发版：

```bash
mkdir -p "$TMP/xdg/swarmdrop-cli"
cat > "$TMP/xdg/swarmdrop-cli/swarmdrop-cli-receipt.json" <<EOF
{"binaries":["swarmdrop"],"binary_aliases":{},"cdylibs":[],"cstaticlibs":[],
"install_layout":"flat","install_prefix":"<本可执行文件所在目录>","modify_path":false,
"provider":{"source":"cargo-dist","version":"0.32.0"},
"source":{"app_name":"swarmdrop-cli","name":"SwarmDrop","owner":"swarm-apps","release_type":"github"},
"version":"0.1.0"}
EOF
XDG_CONFIG_HOME="$TMP/xdg" swarmdrop update --check
```

⚠️ **`source.name` 是仓库名（`SwarmDrop`）而不是应用名**——写成 `swarmdrop-cli` 会得到
`404 Not Found for url .../repos/swarm-apps/swarmdrop-cli/releases`。
`install_prefix` 要指向**本可执行文件的所在目录**，否则 `check_receipt_is_for_this_executable`
判否，渠道被认成 `Unknown`。

⚠️ 测「真有新版本」要把 `crates/cli/Cargo.toml` 的版本临时降到低于线上版本——比较基准取的是
编译期版本，改 receipt 里的版本号没用。

⚠️ 跑过一次真更新之后，`target/debug/swarmdrop` **已被换成下载来的 release 二进制**，
而 **cargo 的 fingerprint 察觉不到**（`cargo build` 会说 `Finished` 却不重建）。
下一次构建前先 `rm` 掉它，否则你以为在测自己的代码，实际跑的是线上那份。

⚠️ 数据目录放在深路径下（比如各种沙箱 scratchpad）会让 `start` **静默超时**：Unix 域套接字
路径上限 108 字节，超了就绑不上。表现是「已在后台拉起节点，但等待就绪超时」而 `status`
显示 stopped。测这块用 `/tmp/xxx` 这样的短路径。

## 接收落点

默认 `<下载目录>/SwarmDrop`，`SWARMDROP_RECEIVE_DIR` 覆盖。
**不落进数据目录**——那是应用私有区，用户在文件管理器里翻不到，收到的文件等于丢了。
用环境变量而非配置文件做覆盖：命令行宿主常跑在脚本与服务单元里。

## 实时进度面板 `transfer watch`（2026-08-20）

`swarmdrop transfer watch` 是一屏随事实刷新的进度条，带三个热键（`p` 暂停 / `r` 恢复 /
`c` 取消 / `q` 退出）。热键只是省掉一次敲命令——按下去弹的正是 `transfer pause` 那套
多选菜单，两条路共用 `control_picker`。

### 库里的**发送**进度在传输期间是假的，必须由常驻节点在内存里补

这是做这个功能时最容易踩空、也最难自己发现的一条：**发送方向的进度不是增量落库的**。
`save_sender_file_progress` 只在四处被调——`SenderActor::on_completed` / `on_interrupted`、
`pause_send`、续传定基线，全是终结时刻。于是 `list_unfinished_projections` 交出来的
`transferred_bytes` 在整条发送传输期间一直是**上一次终结时的值**（首传就是 0），
直到它结束才跳到全量。

桌面 / 移动 / Web 感觉不到：它们与传输在同一个进程里，界面直接吃 `TransferProgress`
事件，数据库只是「重启后的基线」。而 `transfer watch` 跑在**另一个进程**里。

第一版实测的症状就是这个：进度条一路停在 0%，暂停的瞬间跳到 43%。

修法是 `runtime::progress::ProgressCache`——常驻节点自己订阅事件总线、把进度记在内存里，
`TransferUnfinished` 的应答里盖上去。三条判据不能破：

1. **只盖 `transferred_bytes`，不动 `total_size`**。续传时事件里的总量表达的是「本轮要传
   多少」，盖上去会让百分比按一个变小的分母算，进度条在恢复的瞬间往前跳。
2. **五种终结事件都要清掉自己那条**（Paused / Resumed / Completed / Failed / Rejected）。
   缺一条就留下一个永不失效的旧值：那条会话此后每次出现在面板上都带着它最后一刻的进度，
   而库里真正的值（可能因续传基线而**变小**）被它盖住。`TransferResumed` 尤其容易漏——
   续传是新一轮，旧值比新基线大。
3. **不要改成「发送侧周期落库」**。那会给三端的每一次传输都加上周期写事务，只为服务一个
   此刻恰好有人在看的面板。缓存把代价留在需要它的那一侧。

### 反复取数的命令不能用「每次现开一个数据库连接」

`Records::db()` 原本每次调用都 `connect_and_migrate`，理由是「一条命令只取一两次数，
而 SQLite 建连接很便宜」。那条理由对**长期运行**的 `watch` 不成立：`connect_and_migrate`
每次都跑一遍 `Migrator::up`，即使没有待应用的迁移，那也是一次建表 DDL 加一次查询，
也就是一次**写事务**。每秒一次等于持续在库上开写锁——而那正是常驻节点写 checkpoint
要的同一把锁（连接不设 `journal_mode`，走 `delete` 模式，写事务阻塞所有读）。

现在 `Records` 内部是 `Arc<tokio::sync::OnceCell<DatabaseConnection>>`：惰性（走通道的
路径压根不碰它）、且同一个 `Records` 连同它的克隆只开一次。调用点一行未改。

### 面板与选择菜单抢同一片屏幕，解法是「整个丢掉再重建」

`indicatif` 的进度条与 `dialoguer` 的选择框都画在 stderr 的同一片区域，叠在一起两边都
看不清。`MultiProgress::suspend()` 收的是**同步**闭包，而菜单是 async 的（`Picker` 经
`spawn_blocking` 跑 dialoguer），塞不进去。

所以按下热键时直接 `drop(panel)`，菜单结束后 `Panel::new()` 重建——清屏与重画的正确顺序
只写在 `Drop` 里一处。加一对 `hide()`/`show()` 则是两条各自可能写错的路径，而漏掉其中
一条的表现是半屏残留的进度条压在后续输出上面。

### 读键：`std::thread` + 一次一个，不是 `spawn_blocking`

两条都是必须的：

- **`std::thread` 而非 `spawn_blocking`**：阻塞会一直持续到用户**下一次按键**，而那可能
  永远不来。`spawn_blocking` 的任务在运行时销毁时会被等待（不可取消），于是面板因别的
  原因退出后，进程会挂在 `main` 的运行时 drop 上，直到有人碰巧碰一下键盘。detached 的
  OS 线程随进程退出消失。
- **一次只开一个读者，读到键之后才开下一个**：否则弹菜单期间面板的读者还在，
  菜单的方向键会被它截走，用户看到一个动不了的选择框。

`console::Term::read_key_raw()`（`ctrlc_key = true`）把 Ctrl-C 作为**一个键**交回来，
而不是替我们向自己发 `SIGINT`——面板持有一屏进度条与一个 raw 模式的终端，需要自己收尾。
raw 模式关掉了 `ISIG`，所以 ^C 多半是作为字符 `\u{3}` 读到的，**两条路都要接住**。
`select!` 里另有一条 `tokio::signal::ctrl_c()`，接的是没有读键线程的情形（`--no-input`
下的面板，以及两次读键之间那个极短的窗口）。

⚠️ **`Term::read_key` 在 stdin 不是终端时立即返回**，所以循环调用它会变成满速空转的
忙循环。`prompt::hotkey()` 因此只能在 `can_ask()` 为真时调用（有 `debug_assert` 盯着），
而 `watch` 把这个判断做在循环之外、只回答一次。

### 三个动作共用一个通道动词、一份候选判据

`Control { Pause, Resume, Cancel }` 在通道上是一个动词 `TransferControl { action, ids }`
而不是三个：服务端骨架完全一致（解析标识 → 按方向派生 → 汇总），拆开只会让同一段代码
出现三遍。

「哪些会话能做哪个动作」也只有一份实现（`Control::applies`），三处依据它：菜单列哪些候选、
参数指定的那条认不认、面板热键做什么。分开写的话，用户会在菜单里选到一条随即被服务端
拒绝的会话。规则与桌面端 UI 的按钮可用性一致（`src/lib/transfer-projection.ts` +
`-session-row.tsx`）：

| 动作 | 候选 |
|---|---|
| `pause` | `phase == active`（其余阶段没有活 actor 可暂停） |
| `resume` | `phase == suspended && recoverable`（不可恢复的只能重发） |
| `cancel` | `phase ∈ {offered, waiting_accept, active}`（已暂停的没有在跑的东西可取消，要清掉它是删记录） |

⚠️ **phase 名是抄来的字符串**（本 crate 的生产代码不依赖 `entity`），会静默漂移：
核心改了 `rename_all` 或变体名，判据会全部落空——`transfer pause` 报「没有正在传输的
会话」，而屏幕上明明有一条在传。常量收在 `runtime::transfers`（判据、文案、面板样式
三处共用同一份），由 `phase_names_match_the_wire` 与 `status_names_match_the_wire`
两条护栏看守。

### 方向派生的 `pause` / `cancel` 加在了域上，不是在 CLI 里 match

`TransferManager::pause` / `cancel` 按 `session.direction` 查表派生（与 `initiate_resume`
同一形态），与桌面用的 `pause_send` / `pause_receive` **并存而不是取代**：持有投影的调用方
（三端 UI）手上已经有 direction，多查一次库没意义；而通道服务端只拿到一串会话标识。

**不是「先试发送失败再试接收」的试错**——那会把一条真实错误藏进两串拼接文案里
（「发送会话不存在；接收会话不存在」而真正的原因是别的）。

### 未完成投影是端口的一等方法，不是「取全部再过滤」

`SessionStore::list_unfinished_projections`（`phase != Terminal`）。传输历史只增不减，
而面板每秒重取一次——在应用层过滤意味着每一次刷新都要把整张表连同全部文件行读回内存，
读的行数随这台机器用了多久线性增长，而真正要的那几条通常是个位数。

SQL 侧的判据写 `ne(Terminal)` 而不是列举其余四个 phase：新增一个非终态 phase 时，
列举法会把它静默排除在「未完成」之外。

## 发文本 `send --text`（2026-08-20）

文本投递（`crates/transfer/src/text_delivery`）在命令行宿主上此前只有一半：接收侧
（收件箱的 `InboxItemContent::Text`、`inbox show` 打印正文、`inbox export` 写 `.txt`）
早就接好了，**发送侧一条没有**，而接收侧在默认配置下其实也走不通（见下）。

### 它是 `send` 的一个内容开关，不是一个新名词

命令面那三条规则（本文档「命令面」小节）里，「发一段文本」的操作对象是用户敲的一串字，
**不归本程序管理** → 规则 3 → 平铺动词，而那个动词已经存在：`send`。
所以是 `send --text`，不是 `text send`。

`--text` **不违反「同一集合上的动作不得做成开关」**：那条禁的是把*动作*写成选项
（`invite --list` / `--revoke`），而 `send` 的动作只有一个，`--text` 换的是被发送的**东西**。
与位置参数 `conflicts_with`——一次 `send` 只送一样。

反过来说，**不要**为「已发出的文本」建一个 `text` 名词：收到的文本进的是**收件箱**
（与文件同一张表），只有发件账本在别处，做出来会是一个只有一半的集合。

### 正文三态：`Option<Option<String>>`

| 写法 | clap 解析成 | 正文来自 |
|---|---|---|
| `--text 内容` | `Some(Some(..))` | 命令行 |
| `--text` | `Some(None)` | 标准输入是管道 → 读到 EOF；是终端 → `$EDITOR` |
| 不给 | `None` | 文件模式 |

⚠️ **塌成 `Option<String>` 会让 `swarmdrop send --text` 变成解析错误**，管道那条路径
就此消失，而这是命令行宿主相对图形三端唯一多出来的能力
（`tail -50 error.log | swarmdrop send --text --to laptop`）。
由 `text_flag_keeps_its_three_states` 看守。

**没有 `-` 这个写法**（`kubectl -f -` 那个惯例）：管道那条已经由「只给 `--text`」表达，
再给一个等价写法只会让「发一条只有一个减号的消息」无法表达。

### 交互撰写用 `$EDITOR`，不是行输入也不是「空行结束」

`prompt::compose`（dialoguer 的 `editor` feature，为此打开）。两条都不能用：

- **`Question`（dialoguer 的 `Input`）是单行的**，连回车都收不下。而正文上限 64 KiB
  且天然多行——用它等于把这条路径砍成「只能发一行」，另外三端给的是 textarea。
- **「逐行读到空行为止」**（`ask_for_files` 那个形态）**会静默截断**：空行是正文的
  合法内容。

`$EDITOR` 顺带白送两件真的需要的事：改得动（发出去不可撤销），空缓冲区即放弃
（不必再问一次「确定要发吗」）。`$EDITOR` 没设时 dialoguer 退到 `vi` / `notepad`，
精简容器里可能都没有——那时返回 `None`，调用方指路到 `--text <内容>`。

### 结尾换行削掉，其余空白一律保留

`trim_trailing_newlines`：只削 `\n` / `\r`。缩进是内容（发一段代码或配置时尤其），
整体 trim 会把它吃掉；而结尾的换行几乎必然是噪声（`echo` 加的那个、编辑器保存时补的
那个）。这也让管道与 `$EDITOR` 两条路径结果一致——dialoguer 的编辑器自带 `trim_newlines`。

### 标准输入要读成 `Vec<u8>` 再转 UTF-8，不能直接 `read_to_string`

读到 `MAX_TEXT_DELIVERY_BYTES + 1` 就停（`Read::take`，多读一个字节才分得清
「正好压线」与「超了」）。**直接 `read_to_string` 会把两种失败混成一种**：超长输入在
64 KiB 处截断时多半正好切在一个多字节字符中间，于是「文本太长」被报成「不是合法 UTF-8」
——而用户只是 `cat` 错了一个文件。

### 常驻节点必须**自己确认**入站文本，否则默认配置下一条都收不到

这条是本次改动里最容易漏、且漏了最难查的一条。

新配对设备的默认信任档位是 `Collaborator`，它的接收策略带 `require_confirmation`
（`crates/host/src/device.rs`）。于是 `evaluate_text_receive_policy` 对**每一条**发给
命令行宿主的文本都返回 `RequireConfirmation`，投递进入待确认队列等人应答。
命令行宿主没有界面可以弹确认框——

**失败形态**：发送端那条命令阻塞满整个确认窗口（`TEXT_DELIVERY_CONFIRMATION_TIMEOUT`
= 5 分钟），然后拿到一句「对端未在确认窗口内接收」。接收端什么都不会说。
看起来完全像网络问题，而两台机器就在同一个局域网里。

解法与文件那支是同一条规则、同一个理由：`runtime::receive::spawn_auto_accept` 里多接一支
`CoreEvent::TextDeliveryAttention { kind: ConfirmationRequired }` → `service.accept(id)`。
判据仍是**已配对**——能发起投递的对端必然过了配对握手。

⚠️ `Received` 那一种 attention **不能**也拿去 accept：它是「已经收下了」的事后通知，
没有待办。

⚠️ 正文**不进日志**：`TextDeliveryAttention` 刻意不携带它，而 CLI 的日志直接落在用户
终端上、也常被服务管理器收走。

### 状态名同样是抄来的，而抄错的后果比 phase 那边重

`runtime::transfer` 的 `TEXT_DELIVERED` / `TEXT_REJECTED` / `TEXT_RETRYABLE` /
`TEXT_EXPIRED` 与两个失败名（`peer_unavailable` / `timed_out`），是
`entity::TextDeliveryStatus` / `TextDeliveryFailure` 的 serde 形态。生产代码不依赖
`entity`（两条取数路径里只有本地那条拿得到 typed 值）。

phase 抄错的后果是「报没有正在传输的会话」；**这里抄错的后果是把一次成功送达报成失败**
并给出非零退出码——脚本据此重发，对端于是收到两份（重发会新建 delivery_id，幂等键不同）。
由 `text_status_names_match_the_wire` 看守。

退出码分两类，不能合并：`peer_unavailable` / `timed_out` → `PeerUnreachable`（4，
重试或等对方上线可能就好），其余 → `TransferFailed`（5，对端看见了但没收下）。
合成一个码，脚本就只能一律退避重试——而「对端拒绝」重试多少次都还是拒绝。

⚠️ **`Expired` 不要建议「重试同一条」**：接收端已把该投递标为过期，同一个 delivery_id
再来一次会被直接判 `Expired`（`text_delivery/service.rs` 的 `existing.status` 分支）。
文案说的是「请重新发送」。

### 「已送达」不是「已发送」

RPC 的成功应答代表**接收端已经把正文落库**（`TextDeliveryResponse::Delivered` 带
`inbox_item_id`）。说「已发送」是把一个确定的事实降级成不确定的，而失败那几支恰恰都
长着「发出去了但没到」的样子。spec: `text-send-experience`。

### 等待转轮画在**命令层**，不是 runtime 层

与 `send_files` 的进度条相反，且理由是**屏幕在谁那一侧**：文件的进度是一串只有节点收得到
的事件，只能由持节点的进程画；而文本从头到尾只是一次阻塞调用，客户端自己就知道「开始等
了」。画在 runtime 里的话，常驻节点在跑时那个转轮会出现在**服务端**的日志流里，
用户的终端一片空白——而那正是要等五分钟的场合。

⚠️ **转轮的作用域必须在打印结果之前结束**。让它活到函数末尾，indicatif 的定时重绘会盖在
结果行上，收尾时的清行还会把结果一起擦掉。这件事**只在真终端里显形**，管道与 CI 全绿。
`cmd::send` 用「做」与「说」分成两段（`run_delivery` → `Delivered::render`）来结构性地
保证它，不靠在每个返回点记得手动收——`render::send::Progress` 的 `Drop` 注释里是同一条
理由的另一半。

### 收件箱标题会带换行，三处渲染都得压平

文本条目的 `title` 是 `text_preview(body)`——正文前 160 字节，**换行原样保留**。
而 `render::inbox` 的三个消费方都假定标题占一行：列表把标识与来源打在下一行、菜单一项
就是一行、详情是对齐的两列。原样打出来的结果是列表看起来散架，菜单更糟——dialoguer
**按项计数、按行绘制**，选中高亮会错位。

`title_line` 压平并截断（48 字符）。**只在真有换行时才压空白**：否则文件名里的连续空格
会被悄悄改写，而那是记录的真实内容。

## 数据目录路径太长会让本地通道 bind 失败（2026-08-20）

`--data-dir` 指向一条很深的路径时，`swarmdrop start` 以
`监听本地通道失败: local socket name length exceeds capacity of sun_path of sockaddr_un`
退出——类 Unix 的 `sockaddr_un.sun_path` 是定长数组（macOS 104 字节、Linux 108），
而套接字文件就建在数据目录下。

**表现容易误判**：`start -d` 那条只会说「等待就绪超时」，节点其实是起了又立刻退出的；
要看到真正的原因得跑前台的 `start`。临时目录（尤其是带会话 UUID 的那种）很容易超。

目前没有兜底，记在这里是为了下次不必再查一遍。真要修的话，可选项是把套接字放到
`/tmp` 下一个由数据目录路径哈希出来的短名字——但那会把「同一个数据目录 = 同一个节点」
这条判据从路径相等改成哈希相等，得连同单实例仲裁一起想。

## 收件箱与传输详情要给出**本地位置**（2026-08-20）

`inbox show` 与 `transfer show` 此前都不打印东西落在哪儿——前者只列相对路径，后者连文件
都不列。图形三端有「在文件管理器中显示」「复制路径」「保存位置」，**命令行端一个都没有，
路径本身就是那个答案**；而接收落点可被 `SWARMDROP_RECEIVE_DIR` 改掉、只在
`swarmdrop start` 启动那一刻打印过一次，事后无从查起。

### 优先级必须与桌面端的 `item_target_path` 一致

单文件条目 → 那个文件自身的 `localPath`（用户接着要 `cat` / `cp` 它）；
多文件条目 → `rootPath`。两端各写各的会长成「同一条记录，桌面复制出来的路径与命令行打印
的不是同一个」，而两者都看起来完全正常——只有用户拿命令行给的路径去桌面那边找不到时才显形。
`render::inbox` 的 `single_file_shows_itself_and_a_bundle_shows_its_root` 看守它。

### **绝不拼接**「根目录 + 相对路径」

`rootPath` 由 core 的 `content_root_of` 解析：各文件 `local_dir` 全部一致时是那个目录，
否则**回退存储根**。所以拼接出来的路径可能根本不存在，而它看起来完全正常。

实测就撞得到：发一个 `bundle/`（内含 `a.txt` 与 `sub/b.txt`）过去，两个文件的 `local_dir`
分别是 `<落点>/bundle` 与 `<落点>/bundle/sub`——不一致，于是 `rootPath` 是落点根。
相对路径 `bundle/sub/b.txt` 恰好能拼对是巧合，不是契约。

### 传输详情的位置行**按有没有值出现**，不印占位符

发送会话没有 `save_path`，`contentRoot` 因此为空。恒印一行「位置 —」只会让人以为记录坏了。
接收方向即使还没传完也有值（`local_dir` 未写时 `content_root_of` 回退到 `save_path`，
那正是「会落到这里」）。
