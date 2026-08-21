//! 命令面：解析参数 → 调用 → 交给 [`crate::render`]。
//!
//! **本层不含网络与存储细节**。任何「怎么连上对端」「文件写到哪」的知识都属于
//! [`crate::runtime`] 与 [`crate::adapter`]；本层只知道用户要做什么。
//!
//! ## 命令怎么组织：三条可判定的规则
//!
//! 1. 操作对象是**程序自身且为单例** → 平铺动词。`start` / `stop` / `status`
//!    （节点没有集合，无从列举）。
//! 2. 操作对象是**本程序管理的一个集合** → 「名词 + 动词」两级。
//!    `invite` / `device` / `inbox` / `transfer`。
//! 3. 操作对象**不归本程序管理** → 平铺动词。`send`（对象是文件系统里的文件）。
//!
//! 规则 3 是 `send` 唯一的豁免依据，写下来是为了防止它被读成「高频所以平放」——
//! 那条理由不可判定，下一个人会用它把 `device list` 也拉平。`git push` 与 `docker run`
//! 同样落在规则 3。
//!
//! **同一集合上的动作不得做成开关**（`invite --list` / `--revoke`）：开关表达修饰而非
//! 动作，且两个动作开关的互斥关系在 `--help` 里看不出来，只能运行时报错。
//! **层级不超过两级**——三层只在「资源的子资源」上才成立，本仓没有那种嵌套。
//!
//! ### 规则 1 下的名词形态：运行形态
//!
//! `mcp` 是个名词，却平铺在一级。它落在规则 1（对象是程序自身且为单例），只是那条规则
//! 写的是「平铺动词」，而这里命名的东西没有对应的常用动词——它是**本程序的一种运行形态**，
//! 与 `start` 平级：`start` 是「以常驻节点形态运行」，`mcp` 是「以 MCP server 形态运行」。
//!
//! **判据（防止它变成拉平一切的先例）**：这个名词命名的必须是**本程序的一种运行形态**，
//! 而不是一个可列举的集合。`device` / `invite` / `inbox` / `transfer` 都是集合——集合上
//! 的动作必须带动词，规则 2 一步不让。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod device;
pub mod inbox;
pub mod invite;
pub mod mcp;
pub mod send;
pub mod start;
pub mod status;
pub mod stop;
pub mod transfer;
pub mod update;
pub mod watch;

use crate::exit::{CliResult, Code};

/// 第三方网络库在**正常运行**中就会持续打的 WARN，一律压到 `error`。
///
/// 它们不是异常，是常态噪声：`libp2p_mdns` 每次广播都为放不进 mDNS TXT 记录的长地址
/// 各打一条（本仓的 relay circuit 地址必然超长，一次广播十几条），`rtc` / `webrtc` 则
/// 对每个不属于自己的 STUN 包打一条。合起来每秒几十行——而 CLI 的日志直接落在用户
/// 终端上，配对确认那一屏就是被它们顶掉的（实测：确认框出现不到一秒即被推出可视区）。
///
/// **是提门槛不是关掉**：真正的 error 仍然出得来，`RUST_LOG` 也照样能调回去。
///
/// ⚠️ 每条都必须单列，且不能指望通配——`EnvFilter` 按字符串前缀匹配，
/// 这三条都不以 `swarmdrop` 开头。漏掉哪条，那一层的噪声就照旧刷屏。
macro_rules! third_party_noise {
    () => {
        "libp2p_mdns=error,rtc=error,webrtc=error"
    };
}

/// SwarmDrop 命令行宿主。
#[derive(Debug, Parser)]
#[command(
    name = "swarmdrop",
    version,
    about,
    long_about = "SwarmDrop 命令行宿主：无账号、无公网 IP 的设备间端到端加密传输。\n\n\
上手：先 `swarmdrop invite create` 生成一张配对邀请给对方扫，\n\
配对完成后用 `swarmdrop send <文件> --to <设备>` 发送，\n\
或 `swarmdrop send --text <内容> --to <设备>` 发一段文本。\n\
接收是节点在线时的被动行为，没有对应的命令。"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// 结构化输出：结果以机器可读格式写入 stdout。
    ///
    /// 进度与诊断始终走 stderr，因此本开关不会污染 stdout（spec: cli-host）。
    /// 它同时**禁用一切交互提示**——该模式声明的是「调用方是程序」。
    #[arg(long, global = true)]
    pub json: bool,

    /// 禁止一切交互提示。
    ///
    /// 缺参数时不再询问而是直接以用法错误退出；等待配对期间收到的入站请求一律**拒绝**。
    ///
    /// ⚠️ **与 `--auto-accept` 方向相反**，两者不可混用理解：本开关是 fail-closed
    /// （不问就是不放行），`--auto-accept` 是 fail-open（不问但放行）。两者同时给出时
    /// `--auto-accept` 生效——它是对配对行为的明确指令，而本开关只声明不弹提示。
    #[arg(long, global = true)]
    pub no_input: bool,

    /// 数据目录（身份、配对表、数据库、本地通道）。默认取平台约定位置。
    #[arg(
        long,
        global = true,
        env = "SWARMDROP_CLI_DATA_DIR",
        value_name = "DIR"
    )]
    pub data_dir: Option<PathBuf>,
}

impl Cli {
    /// 未设置 `RUST_LOG` 时的默认日志过滤。
    ///
    /// 结构化模式下压到 `warn`：那种场景的调用方是程序，info 级的运行叙述对它没有意义。
    ///
    /// **交互命令一并压到 `warn`**：它们要让用户看清屏幕上的内容再作决定，而节点起来时的
    /// info 级叙述（每次网络状态变化都是一整个结构体）会在几百毫秒里把那一屏顶出可视区。
    /// 配对确认与撤销选择恰恰是要求用户看清屏幕的地方，日志把它冲掉等于让那道确认失去意义。
    ///
    /// ⚠️ **`swarmdrop_cli` 在这里是错的**，尽管它是 package 名：tracing 的 target 默认取
    /// `module_path!()`，而 bin target 的 crate 根是 **bin 名 `swarmdrop`**。`EnvFilter`
    /// 按字符串前缀匹配，`swarmdrop::runtime::pairing` 不以 `swarmdrop_cli` 开头，于是本
    /// 程序自己的日志**一条都不会出现**——修复前常驻节点接受了配对、拒绝了直连请求，
    /// 日志里却什么都没有，而那正是无人值守场景下唯一的排查凭据。
    /// 由 `tests::default_filter_covers_this_crate` 看守。
    ///
    /// 反过来 `swarmdrop` 这一条同时覆盖了 `swarmdrop_core` / `swarmdrop_net` / 其余
    /// `swarmdrop_*`——同样是那条前缀规则，单列它们纯属冗余。
    ///
    /// ⚠️ 将来若要放开网络层日志，**每个非 `swarmdrop` 前缀的 crate 都必须单列**
    /// （`webrtc`/`rtc*`/`wtransport`/`quinn` 都不以 `swarmdrop` 开头）。桌面与移动端
    /// 各有一条同样的常量与断言测试看守这件事，漏掉哪条那一层的日志在生产里就一条都不出现。
    pub fn default_log_filter(&self) -> &'static str {
        if self.json || self.command.is_interactive() || self.command.speaks_a_protocol() {
            concat!("warn,", third_party_noise!())
        } else {
            concat!("warn,swarmdrop=info,", third_party_noise!())
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 启动常驻节点（默认前台运行）。
    Start {
        /// 转入后台运行，命令在节点就绪后立即返回。
        ///
        /// 默认前台是刻意的：服务管理器（systemd / launchd）与托管本程序的外部程序
        /// （agent harness）都要求前台，而那两个才是主场景。
        #[arg(short = 'd', long)]
        detach: bool,

        /// 自动接受出示有效邀请的入站配对，不等人确认。
        ///
        /// **默认关闭**：邀请会泄露、会被抢先用掉，而它是一次性的——被抢走那次会消耗
        /// 掉凭证，真正的设备再来就配不上了。默认形态是把请求转交给正在运行的
        /// `swarmdrop invite create` 由人核对后放行，没有人在等时一律拒绝。
        #[arg(long)]
        auto_accept: bool,
    },

    /// 停止正在运行的节点。
    ///
    /// 对前台与后台启动的节点同样生效；无节点在运行时以成功退出。
    Stop,

    /// 显示节点状态、监听地址、NAT 与中继可达性。
    Status,

    /// 更新到最新版本。
    ///
    /// 落在「操作对象是程序自身且为单例」那条规则下，与 `start` / `stop` / `status` 并列。
    ///
    /// **只有安装脚本（shell / powershell）装的那份能就地更新**。用 Homebrew 或 npm 装的
    /// 会被认出来并转交给对应的包管理器——不去和它们争「哪个版本是当前版本」。
    Update {
        /// 只检查有没有新版本，不安装。
        #[arg(long)]
        check: bool,
    },

    /// 以 MCP server 形态运行，让 agent 经标准输入输出调用 SwarmDrop。
    ///
    /// 面向 agent harness（DeepSeek Harness、Claude Code、Codex …）——它们各自的扩展机制
    /// 互不相同，唯一的公约数是「能执行一条命令」。
    ///
    /// **stdout 只承载 MCP 协议帧**，日志与诊断一律走 stderr。混入一个字节的非协议内容，
    /// 宿主的解析器就会失败，而失败形态是「server 起来了但一个工具都不可见」——与
    /// 「没装 SwarmDrop」无法区分。
    ///
    /// 节点按「有常驻就复用、没有就自持」接入，且**持有到 server 退出**：每次工具调用
    /// 现起现关的话，每一次发送都要重连引导节点并做 NAT 探测，秒级调用会被拖成数秒。
    Mcp,

    /// 订阅本机发生的事件（收件箱 / 传输 / 设备），推给调用它的程序。
    ///
    /// 落在「操作对象是程序自身且为单例」那条规则下——对象是**本程序自己的事件流**，
    /// 与 `start` / `status` 并列。
    ///
    /// ⚠️ 与 `swarmdrop transfer watch` 不是一件事，两者并存是刻意的：那条是给人看的
    /// 传输面板（每秒重绘全量快照、带热键、画在 stderr），这条是给程序用的三类事件
    /// **增量推送**（NDJSON、写在 stdout、跨节点起落存活）。
    ///
    /// **不启动节点**：它只观察本机发生的事，不会有任何数据包因它离开本机。没有常驻
    /// 节点时也不报错——先给一条直读本机记录的基线，然后等节点出现，节点关停后继续等。
    Watch {
        /// 基线里最多带几条收件箱记录。
        ///
        /// 基线刻意不发全量：条目会累积到数千条，而消费方每次订阅都搬一次既昂贵又无用。
        /// 更早的条目按需检索（`swarmdrop inbox list` / MCP 工具）。
        #[arg(long, value_name = "N", default_value_t = crate::runtime::watch::baseline::DEFAULT_INBOX_LIMIT)]
        inbox_limit: u32,
    },

    /// 向一台已配对设备发送文件、目录或一段文本。
    Send {
        /// 要发送的文件或目录。不给则逐行询问（需要可交互的终端）。
        #[arg(value_name = "PATH")]
        files: Vec<PathBuf>,

        /// 目标设备（名称或节点标识）。不给则列出已配对设备让你选。
        #[arg(long, value_name = "DEVICE")]
        to: Option<String>,

        /// 发一段文本而不是文件。与路径互斥——一次只送一样东西。
        ///
        /// 只给 `--text` 不给内容时正文从别处取，取决于标准输入是不是终端：是管道就
        /// 读到 EOF（`pbpaste | swarmdrop send --text --to 台式机`），是终端就拉起
        /// `$EDITOR` 让你写。
        ///
        /// 对端在收件箱里收到它（`swarmdrop inbox show` 看正文），不是文件。
        //
        // ⚠️ **这是内容类型而不是一个动作开关**，所以它不违反本模块开头那条「同一集合上
        // 的动作不得做成开关」：`send` 的动作只有一个，`--text` 换的是被发送的东西。
        //
        // `num_args = 0..=1` 那层是「给没给内容」与「用没用这个开关」的区别，塌成
        // `Option<String>` 会让 `swarmdrop send --text` 变成解析错误，管道那条路径就此
        // 消失（由 `text_flag_keeps_its_three_states` 看守）。
        #[arg(long, value_name = "TEXT", num_args = 0..=1, conflicts_with = "files")]
        text: Option<Option<String>>,
    },

    /// 配对邀请：生成、使用、清点与撤销。
    Invite {
        #[command(subcommand)]
        action: InviteAction,
    },

    /// 已配对设备。
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },

    /// 收件箱。
    Inbox {
        #[command(subcommand)]
        action: InboxAction,
    },

    /// 传输记录。
    Transfer {
        #[command(subcommand)]
        action: TransferAction,
    },
}

/// 按标识去重，保留首次出现的顺序。
///
/// 批量目标要过这一道：同一条记录可以有多种写法（邀请标识的不同长度前缀、设备的名称与
/// 节点标识），它们**作为字符串并不相等**，所以只能在解析成记录之后按标识去重。
///
/// 不去重的后果不是「多做一遍」（撤销与解除都幂等），而是**虚报**——命令会说
/// 「已撤销 2 张」并把同一个标识列两遍，`--json` 里的 `revoked` 也跟着翻倍。
fn dedup_by_id<T>(rows: Vec<T>, id: impl Fn(&T) -> String) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|row| seen.insert(id(row)))
        .collect()
}

/// `send` 的**内容**这一侧要不要占着屏幕。
///
/// 三种来源里只有一种不占：`--text` 只给了开关、而标准输入是一条管道——那时正文从管道
/// 读到 EOF，全程没有人在看。另外两种都占：拉起 `$EDITOR` 是整屏接管，逐行问路径要用户
/// 看清每一条回显。
///
/// ⚠️ **判据是「标准输入是不是终端」，不能换成 [`crate::prompt::can_ask`]**：那个函数在
/// 本函数的唯一调用点（[`Cli::default_log_filter`]，跑在 `main` 里）尚未被
/// [`crate::prompt::configure`] 初始化，读到的是两个还是默认值的静态位。
fn send_body_is_interactive(files: &[PathBuf], text: &Option<Option<String>>) -> bool {
    use std::io::IsTerminal;

    match text {
        Some(Some(_)) => false,
        Some(None) => std::io::stdin().is_terminal(),
        None => files.is_empty(),
    }
}

impl Command {
    /// 这条命令会不会停下来等用户看屏幕、作决定。
    ///
    /// 穷尽 match 而非 `matches!`：新增交互命令时这里会编译失败，
    /// 而不是让它带着一屏滚动的日志上线。
    ///
    /// **判据基本等同于「有参数缺席」**（唯一的例外是 `invite create`，它没有参数可缺、
    /// 却整条命令都要有人看着屏幕）。缺席的那个会由 [`crate::prompt::pick::Picker`] 或
    /// [`crate::prompt::Question`] 补出来，而补的过程要用户看清屏幕。
    fn is_interactive(&self) -> bool {
        match self {
            Self::Invite { action } => action.is_interactive(),
            Self::Device { action } => action.is_interactive(),
            Self::Inbox { action } => action.is_interactive(),
            Self::Transfer { action } => action.is_interactive(),
            Self::Send { files, to, text } => to.is_none() || send_body_is_interactive(files, text),
            // `Mcp` 的调用方**一定**是程序（宿主经 stdio 说话），比其余几条更强：
            // 那几条只是「不需要有人看屏幕」，而它连屏幕都没有——stdout 归 MCP 协议，
            // stdin 是宿主的请求流。它在 `run` 里另有一道 `prompt::configure` 硬关交互。
            // `Watch` 与 `Mcp` 同类：调用方是程序。它虽然长驻，却从不停下来等人——
            // 没有任何参数需要靠提问补出来，也没有需要用户看清的那一屏。
            Self::Start { .. }
            | Self::Stop
            | Self::Status
            | Self::Update { .. }
            | Self::Mcp
            | Self::Watch { .. } => false,
        }
    }

    /// 这条命令的标准输入输出是不是被某个协议独占了。
    ///
    /// **两条流一起独占，不只是 stdout。** 各自的后果不同，但都是静默的：
    ///
    /// - 写 stdout：宿主的解析器读到一个非协议字节就失败，形态是「server 起来了但一个
    ///   工具都不可见」——与「没装 SwarmDrop」无法区分。
    /// - 读 stdin：交互提示会**吃掉一帧协议消息**。在终端里手动跑 `swarmdrop mcp` 调试
    ///   时尤其显形——那时 stdin 是终端，`prompt::can_ask` 的 TTY 判据拦不住它。
    ///
    /// 因此它同时决定两件事：日志压到 `warn`（见 [`Cli::default_log_filter`]），
    /// 以及**强制禁止交互**（见 [`run`]）。后者不能指望 TTY 检测偶然成立——
    /// spec `cli-mcp-host` 的「调用方是程序」是一条契约，不是一个概率。
    ///
    /// 与 `--json` 的关系是「同一件事的两个来源」：那边由用户显式声明「调用方是程序」，
    /// 这边由命令自身的性质决定。
    ///
    /// 穷尽 match 而非 `matches!`：将来再有协议形态的命令时这里会编译失败，
    /// 而不是让它带着一串 info 日志、外加一个会吃掉协议帧的提示上线。
    fn speaks_a_protocol(&self) -> bool {
        match self {
            Self::Mcp => true,
            // ⚠️ `Watch` **不在其列**，尽管它的 stdout 也是一条机器读的流。判据是
            // 「两条流一起被独占」：它不读 stdin（没有请求要收），也就没有「提示吃掉
            // 一帧协议消息」这回事。它的 stdout 由 `--json` 决定形态，与本判据无关。
            //
            // ⚠️ `Invite` 的 `create --decide-from-stdin` **两条流都占**，本函数却仍答
            // `false`——它拿到的是 `&Command`，看不见子命令上的开关。那条路径由另一条
            // 机制覆盖：该开关带 `requires = "json"`，于是 `STRUCTURED` 为真、
            // `can_ask()` 为假，交互本来就关着。**放宽那个 `requires` 就要回来改这里**，
            // 否则一次提示会吃掉调用方送来的一行决策。
            Self::Start { .. }
            | Self::Stop
            | Self::Status
            | Self::Update { .. }
            | Self::Watch { .. }
            | Self::Send { .. }
            | Self::Invite { .. }
            | Self::Device { .. }
            | Self::Inbox { .. }
            | Self::Transfer { .. } => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum InviteAction {
    /// 生成一张邀请并守着它，直到有设备配对成功或你中断。
    ///
    /// 邀请的可拨地址就是签发节点的，所以**命令退出后临时节点签发的那张邀请即失效**。
    Create {
        /// 自动接受出示**任一有效邀请**的设备，不停下来问。
        ///
        /// ⚠️ 范围是「任一有效邀请」而不是「刚打印的这张」：邀请跨重启存活、TTL 24 小时，
        /// 本机此前发出过而尚未过期的邀请在等待期间同样能配上。要先看清楚有哪些，
        /// 用 `swarmdrop invite list`。
        ///
        /// 无人值守场景（脚本、CI、agent harness）用。**届时没有人核对对端身份**，
        /// 只在可控网络里这么做。
        #[arg(long)]
        auto_accept: bool,

        /// 把每个入站配对请求交给**调用方**决定，而不是问终端前的人。
        ///
        /// 供托管本命令的程序（图形前端、agent harness 的插件）使用：请求以 NDJSON
        /// 写到 stdout（`{"event":"pairingRequest","pendingId":N,…}`），调用方往 stdin
        /// 写回一行 `{"pendingId":N,"accept":true}`。
        ///
        /// **与 `--auto-accept` 的区别是谁在核对身份**：那条谁都放行，这条把对端信息
        /// 交给一个会展示给人看的程序。因此它**不是** fail-open：stdin 关掉就是「问的
        /// 那个人走了」，手上那条顺手拒掉并退出，与「没有终端可问」同一条判据。
        ///
        /// 答不上号的输入（答的是别的 `pendingId`、或者压根不是一个决策对象）**被跳过
        /// 而不是当作拒绝**：把一行垃圾读成「拒绝」，用户会在界面上看到「已拒绝」并去
        /// 排查自己那台设备，而实际什么也没发生。等不到答复时由对端的超时兜底。
        ///
        /// 隐含结构化输出，故要求 `--json`；与 `--auto-accept` 互斥。
        #[arg(long, requires = "json", conflicts_with = "auto_accept")]
        decide_from_stdin: bool,
    },

    /// 用一张别人给的邀请完成配对。
    Use {
        /// 邀请链接。不给则询问（需要可交互的终端）。
        invite: Option<String>,
    },

    /// 列出本机已发出、尚未过期的邀请。
    List,

    /// 撤销邀请。
    ///
    /// 不给标识时会列出邀请让你**勾选若干张**（需要可交互的终端）。
    Revoke {
        /// 邀请标识，接受唯一前缀（至少 4 位）。可给多个。
        #[arg(value_name = "ID")]
        ids: Vec<String>,

        /// 撤销全部未过期的邀请。
        ///
        /// 邀请泄露时往往无法判断是哪一张——而停掉节点**不管用**，邀请已落盘，
        /// 重启回来它们会全部恢复可用。
        ///
        /// **与逐张勾选不是一回事**：它连本命令列不出来的、这一瞬新签发的也一并作废，
        /// 所以它是「不知道是哪张泄露了」时的处置，而勾选是「知道是哪几张」。
        #[arg(long, conflicts_with = "ids")]
        all: bool,

        /// 跳过 `--all` 的确认。
        #[arg(long)]
        yes: bool,
    },
}

impl InviteAction {
    fn is_interactive(&self) -> bool {
        match self {
            // 要展示邀请链接，并在入站请求到来时展示对端信息等人确认。
            //
            // ⚠️ **`--auto-accept` 不是例外**（2026-08-19 试过一次，错了）：它免去的是
            // 「每条入站请求要人点一次确认」，不是「没人在看屏幕」——这条命令的产出**就是
            // 那条链接**，得有人把它搬到另一台设备上（或在浏览器里打开它扫码）。放开日志的
            // 净效果是：临时节点起来后 `NetworkStatusChanged`（二十来个字段的结构体）与
            // `DevicesChanged`（每秒可能多次）几秒内把它顶出可视区，而命令要守着等人
            // 扫码 / 粘贴、以分钟计。
            //
            // 真正的无人值守是 `start --auto-accept`（常驻节点），那条本来就是 false。
            Self::Create { .. } => true,
            // 不给标识时要弹多选菜单；`--all` 要确认。
            Self::Revoke { ids, all, yes } => (ids.is_empty() && !all) || (*all && !yes),
            Self::Use { invite } => invite.is_none(),
            Self::List => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum DeviceAction {
    /// 列出已配对设备。
    List,

    /// 解除与设备的配对。
    ///
    /// **单方面操作**：它移除的是本机对该设备的记录，对端是否仍记着你不在本命令的控制
    /// 范围内。不给目标时会列出设备让你**勾选若干台**（需要可交互的终端）。
    Forget {
        /// 设备名称或节点标识。可给多个。
        #[arg(value_name = "DEVICE")]
        devices: Vec<String>,
    },
}

impl DeviceAction {
    fn is_interactive(&self) -> bool {
        match self {
            Self::Forget { devices } => devices.is_empty(),
            Self::List => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum InboxAction {
    /// 列出收件箱条目。
    List,
    /// 查看一个条目的详情。
    Show {
        /// 条目标识。不给则列出收件箱让你选。
        id: Option<String>,
    },
    /// 把一个条目导出到指定目录。
    Export {
        /// 条目标识。不给则列出收件箱让你选。
        id: Option<String>,
        /// 导出目标目录。不给则询问（默认当前目录）。
        ///
        /// 位置参数而非 `--to`：`send --to` 指的是**设备**，这里是**目录**，
        /// 同名不同类型会诱导误用。形态对齐 `cp 源 目标`。
        ///
        /// ⚠️ 两个位置参数都可缺省，但**只能从后往前省**（`export <id>` 合法，
        /// 「只给目录不给标识」无从表达——第一个位置参数永远解析成标识）。
        dir: Option<PathBuf>,
    },
}

impl InboxAction {
    fn is_interactive(&self) -> bool {
        match self {
            Self::Show { id } => id.is_none(),
            Self::Export { id, dir } => id.is_none() || dir.is_none(),
            Self::List => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum TransferAction {
    /// 列出传输记录（最近开始的在前）。
    List,
    /// 查看一条传输记录的详情。
    Show {
        /// 会话标识。不给则列出传输记录让你选。
        id: Option<String>,
    },

    /// 实时盯着正在进行的传输，并就地暂停 / 恢复 / 取消。
    ///
    /// 每条未结束的传输一行进度条，随事实刷新；按 `q` 退出。
    /// 屏幕上的热键与下面三条命令是同一件事——热键只是省掉一次敲命令。
    Watch,

    /// 暂停正在进行的传输。
    ///
    /// 不给标识时会列出**此刻正在传**的会话让你勾选（需要可交互的终端）。
    /// 暂停会通知对端，双方都停在同一个断点上，之后用 `swarmdrop transfer resume` 续。
    Pause {
        /// 会话标识（完整）。可给多个。
        #[arg(value_name = "ID")]
        ids: Vec<String>,
    },

    /// 恢复已暂停或已中断的传输。
    ///
    /// 不给标识时会列出**可续传**的会话让你勾选。不可恢复的中断不在其列——
    /// 那种只能重新发一次。
    Resume {
        /// 会话标识（完整）。可给多个。
        #[arg(value_name = "ID")]
        ids: Vec<String>,
    },

    /// 取消尚未结束的传输。
    ///
    /// 不给标识时会列出**尚未结束**的会话让你勾选。已暂停的不在其列——
    /// 它此刻没有在跑的东西可取消，要清掉它请删记录。
    Cancel {
        /// 会话标识（完整）。可给多个。
        #[arg(value_name = "ID")]
        ids: Vec<String>,
    },
}

impl TransferAction {
    fn is_interactive(&self) -> bool {
        match self {
            Self::Show { id } => id.is_none(),
            // **整条命令都要有人看着屏幕**，与 `invite create` 同一类（它也没有参数
            // 可缺）：面板每秒重画一次，而 info 级的网络状态叙述会把它冲成一片乱码。
            Self::Watch => true,
            Self::Pause { ids } | Self::Resume { ids } | Self::Cancel { ids } => ids.is_empty(),
            Self::List => false,
        }
    }
}

/// 分派并把失败翻译成退出码。
///
/// 错误在这里统一渲染到 stderr —— 各子命令只负责返回 [`crate::exit::CliError`]，
/// 不各自打印，否则同一类失败会长出多种措辞。
pub async fn dispatch(cli: Cli) -> Code {
    finish(run(cli).await)
}

/// 把一次执行的结果翻成退出码，失败时渲染到 stderr。
///
/// 单独成函数是因为它有第二个调用点：`mcp` 在经通道/信号收摊那条路径上**不能靠 `return`**
/// 走到这里（理由见 [`mcp::run`]），只能自己算出退出码再硬退。共用这一份，是为了让
/// 「同一类失败长出两种措辞」这件事没有发生的余地。
pub(crate) fn finish(result: CliResult<()>) -> Code {
    match result {
        Ok(()) => Code::Success,
        Err(err) => {
            eprintln!("错误: {err}");
            err.code()
        }
    }
}

async fn run(cli: Cli) -> CliResult<()> {
    // 交互能力是**环境事实**，在分派之前记一次；此后 `prompt::can_ask` 是唯一的判据。
    //
    // 协议形态的命令**强制**禁止交互：它的 stdin 归协议，一个提示就会吃掉一帧消息。
    // 不靠 `can_ask` 的 TTY 判据兜——那条在「终端里手动跑 `swarmdrop mcp` 调试」时
    // 恰好不成立，而那正是最容易撞上的场景。
    crate::prompt::configure(cli.no_input || cli.command.speaks_a_protocol(), cli.json);

    // 数据目录对每条命令都是前置条件（身份、数据库、通道、锁都在它下面），
    // 因此在分派之前解析一次，而不是让每个命令各自解析。
    let data_dir = crate::adapter::paths::DataDir::resolve(cli.data_dir)?;
    let json = cli.json;

    match cli.command {
        Command::Start {
            detach,
            auto_accept,
        } => start::run(&data_dir, json, detach, auto_accept).await,
        Command::Stop => stop::run(&data_dir, json).await,
        Command::Status => status::run(&data_dir, json).await,
        Command::Update { check } => update::run(&data_dir, json, check).await,
        Command::Mcp => mcp::run(&data_dir).await,
        Command::Watch { inbox_limit } => watch::run(&data_dir, json, inbox_limit).await,
        Command::Send { files, to, text } => send::run(&data_dir, json, files, to, text).await,
        Command::Invite { action } => invite::run(&data_dir, json, action).await,
        Command::Device { action } => device::run(&data_dir, json, action).await,
        Command::Inbox { action } => inbox::run(&data_dir, json, action).await,
        Command::Transfer { action } => transfer::run(&data_dir, json, action).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("parse")
    }

    /// clap 的定义自洽性（重复的短选项、非法的默认值等）由它自己的 debug_assert 检查。
    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// 两个决策源不能同时给出，且程序化那个必须声明 `--json`。
    ///
    /// **这两条在命令行这一层拦住，才使 `Decider` 那个枚举成立**——它把「既自动放行、
    /// 又要问调用方」这半个组合从类型上消掉，前提是解析期不会构造出那种输入。
    /// 顺带也挡住一个更安静的错误：`--decide-from-stdin` 不带 `--json` 时，请求会以
    /// 人类可读的框线打到 stderr，而调用方在 stdout 上什么也等不到。
    #[test]
    fn the_two_decision_sources_are_exclusive() {
        assert!(
            Cli::try_parse_from([
                "swarmdrop",
                "invite",
                "create",
                "--json",
                "--decide-from-stdin",
            ])
            .is_ok(),
            "程序化决策源应当能解析"
        );
        assert!(
            Cli::try_parse_from(["swarmdrop", "invite", "create", "--decide-from-stdin"]).is_err(),
            "--decide-from-stdin 必须要求 --json"
        );
        assert!(
            Cli::try_parse_from([
                "swarmdrop",
                "invite",
                "create",
                "--json",
                "--decide-from-stdin",
                "--auto-accept",
            ])
            .is_err(),
            "两个决策源不得同时给出"
        );
    }

    /// `send` 的两个参数各自都可以缺席——缺的那个由交互补出来。
    ///
    /// ⚠️ **缺席在解析期合法，不等于在管道里合法**：那里补不出来，必须以用法错误退出。
    /// 那一半由 `tests/without_a_node.rs` 从进程外看守（解析期拦不住它，因为
    /// 「有没有终端」不是 clap 知道的事）。
    #[test]
    fn send_accepts_partial_arguments() {
        for args in [
            vec!["swarmdrop", "send", "a.txt", "--to", "phone"],
            vec!["swarmdrop", "send", "--to", "phone"],
            vec!["swarmdrop", "send", "a.txt"],
            vec!["swarmdrop", "send"],
        ] {
            assert!(Cli::try_parse_from(&args).is_ok(), "{args:?} 应当能解析");
        }
    }

    /// `--text` 的三种形态各自解析成一个可区分的值。
    ///
    /// **三态不能塌成两态**：`Option<Option<String>>` 里那层嵌套正是「给没给内容」与
    /// 「用没用这个开关」的区别，而正文来自哪里（命令行 / 管道 / 编辑器）全靠它。
    /// 塌成 `Option<String>` 之后，`swarmdrop send --text` 会变成一个解析错误，
    /// 管道那条路径就此消失。
    #[test]
    fn text_flag_keeps_its_three_states() {
        for (args, expected) in [
            (
                vec!["swarmdrop", "send", "--text", "你好", "--to", "phone"],
                Some(Some("你好".to_owned())),
            ),
            (
                vec!["swarmdrop", "send", "--text", "--to", "phone"],
                Some(None),
            ),
            (vec!["swarmdrop", "send", "--to", "phone"], None),
        ] {
            let Command::Send { text, .. } = parse(&args).command else {
                panic!("{args:?} 解析成了别的命令");
            };
            assert_eq!(text, expected, "{args:?} 的 --text 解析结果不对");
        }
    }

    /// 一次 `send` 只送一样东西：`--text` 与位置参数互斥。
    ///
    /// 由 clap 在解析期拒绝，而不是留到运行时挑一个——「同时给了两样」没有正确答案，
    /// 而静默地只发其中一样是最坏的那种：用户以为两样都发出去了。
    #[test]
    fn text_and_paths_cannot_both_be_given() {
        assert!(
            Cli::try_parse_from(["swarmdrop", "send", "a.txt", "--text", "你好"]).is_err(),
            "文本与路径同时给出必须被拒绝"
        );
    }

    /// 正文来自命令行时不需要有人看屏幕；目标缺席时需要。
    ///
    /// 这个判据决定的是**日志要不要压到 `warn`**——答错不报错，只是把一屏交互内容
    /// 冲掉（或反过来，在无人值守的管道里白白丢掉运行叙述）。
    ///
    /// 只钉与环境无关的两条：`--text <内容>` 恒不交互，缺 `--to` 恒交互。
    /// 「只给 `--text`」那条取决于标准输入是不是终端，在测试进程里不可靠。
    #[test]
    fn inline_text_needs_nobody_watching() {
        assert!(
            !parse(&["swarmdrop", "send", "--text", "你好", "--to", "phone"])
                .command
                .is_interactive()
        );
        assert!(
            parse(&["swarmdrop", "send", "--text", "你好"])
                .command
                .is_interactive(),
            "缺目标设备就得列出候选让人选"
        );
    }

    /// 集合类命令的目标参数**可以给多个**——交互能勾选多条，参数侧就不该只收一个，
    /// 否则脚本只能把同一条命令循环敲 N 遍。
    #[test]
    fn collection_targets_accept_several() {
        let cli = parse(&["swarmdrop", "invite", "revoke", "abcd1234", "ef567890"]);
        let Command::Invite {
            action: InviteAction::Revoke { ids, .. },
        } = cli.command
        else {
            panic!("解析成了别的命令");
        };
        assert_eq!(ids.len(), 2);

        let cli = parse(&["swarmdrop", "device", "forget", "手机", "书房"]);
        let Command::Device {
            action: DeviceAction::Forget { devices },
        } = cli.command
        else {
            panic!("解析成了别的命令");
        };
        assert_eq!(devices.len(), 2);
    }

    /// 默认过滤器必须覆盖本 crate 自己的 target 根。
    ///
    /// 这条断言看守的是一个**静默**失效：写错前缀不会报错、不会 panic，只是本程序的
    /// 每一行日志都消失。改 bin 名或改过滤器时它会红。
    #[test]
    fn default_filter_covers_this_crate() {
        let root = module_path!().split("::").next().expect("crate root");
        assert_eq!(root, "swarmdrop", "bin 名变了就要同步改默认过滤器");

        let cli = parse(&["swarmdrop", "status"]);
        assert!(
            cli.default_log_filter().contains(&format!("{root}=")),
            "默认过滤器没覆盖本 crate: {}",
            cli.default_log_filter()
        );
    }

    /// 会停下来等用户看屏幕的命令，默认日志必须安静。
    ///
    /// 这条看守的是一个只在真终端上才显形的缺陷：info 级的网络状态叙述会在提示出现后的
    /// 几百毫秒里把它顶出可视区，用户来不及核对就只剩一个光标。
    #[test]
    fn interactive_commands_are_quiet_by_default() {
        for args in [
            vec!["swarmdrop", "invite", "create"],
            // **`--auto-accept` 也要安静**：它免去的是逐条确认，不是「没人在看屏幕」——
            // 产出的二维码仍然要有人扫。改错过一次，这条钉住它。
            vec!["swarmdrop", "invite", "create", "--auto-accept"],
            vec!["swarmdrop", "invite", "revoke"],
            vec!["swarmdrop", "invite", "revoke", "--all"],
            vec!["swarmdrop", "invite", "use"],
            vec!["swarmdrop", "device", "forget"],
            vec!["swarmdrop", "inbox", "show"],
            vec!["swarmdrop", "inbox", "export"],
            // 只缺目录也算——那一问同样要用户看清屏幕。
            vec![
                "swarmdrop",
                "inbox",
                "export",
                "00000000-0000-4000-8000-000000000000",
            ],
            vec!["swarmdrop", "transfer", "show"],
            // **面板每秒重画一次**，info 级的网络状态叙述会把它冲成一片乱码。
            // 它没有可缺省的参数，所以下面那条反射测试覆盖不到它——同 `invite create`。
            vec!["swarmdrop", "transfer", "watch"],
            vec!["swarmdrop", "transfer", "pause"],
            vec!["swarmdrop", "send"],
            vec!["swarmdrop", "send", "a.txt"],
            vec!["swarmdrop", "send", "--to", "phone"],
        ] {
            let cli = parse(&args);
            assert!(
                !cli.default_log_filter().contains("swarmdrop=info"),
                "{args:?} 会提问，日志却不安静"
            );
        }
    }

    /// 给全了参数就不再是交互命令——那时日志该照常出，它是无人值守场景的唯一排查凭据。
    #[test]
    fn fully_specified_commands_keep_their_logs() {
        for args in [
            vec!["swarmdrop", "invite", "revoke", "abcd1234"],
            vec!["swarmdrop", "invite", "revoke", "--all", "--yes"],
            vec!["swarmdrop", "device", "forget", "phone"],
            vec![
                "swarmdrop",
                "transfer",
                "pause",
                "00000000-0000-4000-8000-000000000000",
            ],
            vec!["swarmdrop", "invite", "list"],
            vec!["swarmdrop", "invite", "use", "https://swarmdrop.dev/p/xxx"],
            vec![
                "swarmdrop",
                "transfer",
                "show",
                "00000000-0000-4000-8000-000000000000",
            ],
            vec![
                "swarmdrop",
                "inbox",
                "show",
                "00000000-0000-4000-8000-000000000000",
            ],
            vec![
                "swarmdrop",
                "inbox",
                "export",
                "00000000-0000-4000-8000-000000000000",
                "/tmp",
            ],
            vec!["swarmdrop", "send", "a.txt", "--to", "phone"],
        ] {
            let cli = parse(&args);
            assert!(
                cli.default_log_filter().contains("swarmdrop=info"),
                "{args:?} 不提问，日志不该被压掉"
            );
        }
    }

    /// 两种模式都必须压住第三方库的常态噪声。
    ///
    /// 只压交互模式是不够的：`start` 前台跑在终端里时同样被刷屏，而那正是用户
    /// 用来观察节点在干什么的那个窗口。
    #[test]
    fn third_party_noise_is_suppressed_in_every_mode() {
        for args in [
            vec!["swarmdrop", "invite", "create"],
            vec!["swarmdrop", "start"],
            vec!["swarmdrop", "status", "--json"],
        ] {
            let cli = parse(&args);
            let filter = cli.default_log_filter();
            for directive in third_party_noise!().split(',') {
                assert!(
                    filter.contains(directive),
                    "{args:?} 漏了 {directive}: {filter}"
                );
            }
        }
    }

    /// 全局选项在子命令之后也能被识别——用户不会记得它必须写在前面。
    #[test]
    fn global_flags_are_accepted_after_subcommand() {
        let cli = parse(&["swarmdrop", "device", "list", "--json"]);
        assert!(cli.json);
        let cli = parse(&["swarmdrop", "invite", "revoke", "abcd", "--no-input"]);
        assert!(cli.no_input);
    }

    /// 命令层级不得超过两级。
    ///
    /// 三层只在「资源的子资源」上才成立（`gh repo deploy-key list`），本仓没有那种嵌套。
    /// 多一层就是多一次用户要记住的跳转。
    #[test]
    fn command_tree_is_at_most_two_levels() {
        for top in Cli::command().get_subcommands() {
            for second in top.get_subcommands() {
                assert!(
                    second.get_subcommands().next().is_none(),
                    "{} {} 下面还有第三层",
                    top.get_name(),
                    second.get_name()
                );
            }
        }
    }

    /// **集合上的动作不得做成开关。**
    ///
    /// `invite --list` / `--revoke` 这种形态在 `--help` 里看不出互斥关系，只能运行时报错。
    /// 这条扫的是名词类子命令下有没有混进动作开关。
    #[test]
    fn collection_actions_are_subcommands_not_flags() {
        let forbidden = [
            "list", "create", "revoke", "forget", "show", "export", "use",
        ];
        for top in Cli::command().get_subcommands() {
            for arg in top.get_arguments() {
                let name = arg.get_id().as_str();
                assert!(
                    !forbidden.contains(&name),
                    "{} 把动作 `{name}` 做成了开关",
                    top.get_name()
                );
            }
        }
    }

    /// `pair` 必须彻底消失，**连别名都不留**。
    ///
    /// 留了它就赢——它更短，而 `invite create` 会变成没人用的正式写法（Docker 的
    /// `ps` 与 `container ls` 至今并存就是这么来的）。CLI 从未发布，这是唯一一次
    /// 能干净改名的窗口。
    #[test]
    fn pair_is_gone_entirely() {
        assert!(Cli::try_parse_from(["swarmdrop", "pair"]).is_err());
        assert!(Cli::try_parse_from(["swarmdrop", "devices"]).is_err());
        assert!(Cli::try_parse_from(["swarmdrop", "inbox", "get", "x"]).is_err());
    }

    /// `--all` 与具体标识互斥——同时给出是自相矛盾的意图，该在解析期就拦住。
    #[test]
    fn revoke_all_conflicts_with_an_id() {
        assert!(Cli::try_parse_from(["swarmdrop", "invite", "revoke", "abcd", "--all"]).is_err());
    }

    /// 「参数缺席 ⇒ 会问人」这条规则**必须覆盖每一个可缺省的参数**。
    ///
    /// 上面两条测试列的是具体命令，新增命令时容易漏；这条从另一头看守，而且
    /// **穷尽性交给机器**：从 clap 的定义里递归找出「带可缺省取值参数」的每一条命令，
    /// 构造它的最小调用，断言那时算交互。加一个可选参数却忘了同步 `is_interactive`，
    /// 它会自己红——而人工列举的清单只会一直绿着。
    ///
    /// 漏了的表现是那条命令的菜单被 info 级日志冲掉，**只在真终端上显形**。
    #[test]
    fn every_optional_target_makes_the_command_interactive() {
        /// 这条命令有没有「不给就得问」的参数。
        ///
        /// 只算**取值**的参数：`--all` / `--yes` 这类布尔开关不给就是不给，
        /// 没有可补的东西。全局参数（`--data-dir` 等）同样排除，它们不属于任何一条命令。
        fn has_optional_value(cmd: &clap::Command) -> bool {
            cmd.get_arguments().any(|arg| {
                // ⚠️ 判据用 `get_action()` 而**不是** `get_num_args()`：后者只在显式
                // 设过 `num_args` 时才是 `Some`，而本 crate 一处都没设——用它会让这条
                // 测试只扫到三条命令、其余静默放行。
                arg.get_action().takes_values()
                    && !arg.is_required_set()
                    && !arg.is_global_set()
                    // 有默认值的不算「不给就得问」：缺了它用默认值，没有可补的东西。
                    // `watch --inbox-limit` 是第一个这样的参数。
                    && arg.get_default_values().is_empty()
            })
        }

        let mut checked = 0usize;
        for top in Cli::command().get_subcommands() {
            let subs: Vec<_> = top.get_subcommands().collect();
            // 两级命令看第二级，一级命令（`send`）看它自己。
            let targets: Vec<(Vec<&str>, &clap::Command)> = if subs.is_empty() {
                vec![(vec![top.get_name()], top)]
            } else {
                subs.iter()
                    .map(|sub| (vec![top.get_name(), sub.get_name()], *sub))
                    .collect()
            };

            for (path, cmd) in targets {
                if !has_optional_value(cmd) {
                    continue;
                }
                let mut args = vec!["swarmdrop"];
                args.extend(path.iter().copied());
                assert!(
                    parse(&args).command.is_interactive(),
                    "{args:?} 有可缺省的参数，缺着它却不算交互命令"
                );
                checked += 1;
            }
        }

        // 反射失灵（clap 的 API 变了、判据写错）时它会静默通过——那正是这条测试要防的。
        assert!(checked >= 7, "只查到 {checked} 条命令，判据可能失效了");
    }
}
