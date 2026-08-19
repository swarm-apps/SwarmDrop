//! 命令面：解析参数 → 调用 → 交给 [`crate::render`]。
//!
//! **本层不含网络与存储细节**。任何「怎么连上对端」「文件写到哪」的知识都属于
//! [`crate::runtime`] 与 [`crate::adapter`]；本层只知道用户要做什么。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod devices;
pub mod inbox;
pub mod pair;
pub mod send;
pub mod start;
pub mod status;
pub mod stop;

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
#[command(name = "swarmdrop", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// 结构化输出：结果以机器可读格式写入 stdout。
    ///
    /// 进度与诊断始终走 stderr，因此本开关不会污染 stdout（spec: cli-host）。
    #[arg(long, global = true)]
    pub json: bool,

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
    /// **交互命令一并压到 `warn`**：`pair` 要让用户看清二维码和对端信息，而节点起来时的
    /// info 级叙述（每次网络状态变化都是一整个结构体）会在几百毫秒里把那一屏顶出可视区。
    /// 配对确认恰恰是**唯一一处**要求用户看清屏幕内容再作决定的地方，日志把它冲掉
    /// 等于让那道确认失去意义。需要排查时用 `RUST_LOG` 覆盖。
    ///
    /// ⚠️ **`swarmdrop_cli` 在这里是错的**，尽管它是 package 名：tracing 的 target 默认取
    /// `module_path!()`，而 bin target 的 crate 根是 **bin 名 `swarmdrop`**。`EnvFilter`
    /// 按字符串前缀匹配，`swarmdrop::runtime::pairing` 不以 `swarmdrop_cli` 开头，于是本
    /// 程序自己的日志**一条都不会出现**——修复前常驻节点接受了配对、拒绝了直连请求，
    /// 日志里却什么都没有，而那正是无人值守场景下唯一的排查凭据。
    /// 由 [`tests::default_filter_covers_this_crate`] 看守。
    ///
    /// 反过来 `swarmdrop` 这一条同时覆盖了 `swarmdrop_core` / `swarmdrop_net` / 其余
    /// `swarmdrop_*`——同样是那条前缀规则，单列它们纯属冗余。
    ///
    /// ⚠️ 将来若要放开网络层日志，**每个非 `swarmdrop` 前缀的 crate 都必须单列**
    /// （`webrtc`/`rtc*`/`wtransport`/`quinn` 都不以 `swarmdrop` 开头）。桌面与移动端
    /// 各有一条同样的常量与断言测试看守这件事，漏掉哪条那一层的日志在生产里就一条都不出现。
    pub fn default_log_filter(&self) -> &'static str {
        if self.json || self.command.is_interactive() {
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
        /// `swarmdrop pair` 由人核对后放行，没有人在等时一律拒绝。
        #[arg(long)]
        auto_accept: bool,
    },

    /// 停止正在运行的节点。
    ///
    /// 对前台与后台启动的节点同样生效；无节点在运行时以成功退出。
    Stop,

    /// 显示节点状态、监听地址、NAT 与中继可达性。
    Status,

    /// 生成配对邀请，或以一个邀请完成配对。
    Pair {
        /// 邀请链接。省略则生成一个新的邀请，并守着它直到配对完成。
        invite: Option<String>,

        /// 不渲染二维码，只输出邀请链接（用于 CI / 日志 / 管道）。
        #[arg(long)]
        no_qr: bool,

        /// 自动接受出示**任一有效邀请**的设备，不停下来问。
        ///
        /// ⚠️ 范围是「任一有效邀请」而不是「刚打印的这张」：邀请跨重启存活、TTL 24 小时，
        /// 本机此前发出过而尚未过期的邀请在等待期间同样能配上。要收窄到某一张，前提是
        /// 先有邀请清单与撤销入口（尚未实现）。
        ///
        /// 无人值守场景（脚本、CI、agent harness）用。**届时没有人核对对端身份**，
        /// 只在可控网络里这么做。
        #[arg(long)]
        auto_accept: bool,
    },

    /// 列出已配对设备。
    Devices,

    /// 向一台已配对设备发送文件或目录。
    Send {
        /// 要发送的文件或目录。
        #[arg(required = true, value_name = "PATH")]
        files: Vec<PathBuf>,

        /// 目标设备（名称或节点标识）。
        #[arg(long, value_name = "DEVICE")]
        to: String,
    },

    /// 收件箱。
    Inbox {
        #[command(subcommand)]
        action: InboxAction,
    },
}

impl Command {
    /// 这条命令会不会停下来等用户看屏幕、作决定。
    ///
    /// 目前只有 `pair`：它要展示二维码，并在入站请求到来时展示对端信息等人确认。
    /// 穷尽 match 而非 `matches!`：新增交互命令时这里会编译失败，
    /// 而不是让它带着一屏滚动的日志上线。
    fn is_interactive(&self) -> bool {
        match self {
            Self::Pair { .. } => true,
            Self::Start { .. }
            | Self::Stop
            | Self::Status
            | Self::Devices
            | Self::Send { .. }
            | Self::Inbox { .. } => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum InboxAction {
    /// 列出收件箱条目。
    List,
    /// 查看一个条目的详情。
    Get {
        /// 条目标识。
        id: String,
    },
    /// 把一个条目导出到指定目录。
    Export {
        /// 条目标识。
        id: String,
        /// 导出目标目录。
        #[arg(long, value_name = "DIR")]
        to: PathBuf,
    },
}

/// 分派并把失败翻译成退出码。
///
/// 错误在这里统一渲染到 stderr —— 各子命令只负责返回 [`crate::exit::CliError`]，
/// 不各自打印，否则同一类失败会长出多种措辞。
pub async fn dispatch(cli: Cli) -> Code {
    match run(cli).await {
        Ok(()) => Code::Success,
        Err(err) => {
            eprintln!("错误: {err}");
            err.code()
        }
    }
}

async fn run(cli: Cli) -> CliResult<()> {
    // 数据目录对每条命令都是前置条件（身份、数据库、通道、锁都在它下面），
    // 因此在分派之前解析一次，而不是让每个命令各自解析。
    let data_dir = crate::adapter::paths::DataDir::resolve(cli.data_dir)?;

    match cli.command {
        Command::Start {
            detach,
            auto_accept,
        } => start::run(&data_dir, cli.json, detach, auto_accept).await,
        Command::Stop => stop::run(&data_dir, cli.json).await,
        Command::Status => status::run(&data_dir, cli.json).await,
        Command::Pair {
            invite,
            no_qr,
            auto_accept,
        } => pair::run(&data_dir, cli.json, invite, no_qr, auto_accept).await,
        Command::Devices => devices::run(&data_dir, cli.json).await,
        Command::Send { files, to } => send::run(&data_dir, cli.json, files, to).await,
        Command::Inbox { action } => inbox::run(&data_dir, cli.json, action).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// clap 的定义自洽性（重复的短选项、非法的默认值等）由它自己的 debug_assert 检查。
    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// `send` 至少要一个路径与一个目标——两者缺一都应是用法错误而非运行到一半才失败。
    #[test]
    fn send_requires_files_and_target() {
        assert!(Cli::try_parse_from(["swarmdrop", "send", "--to", "phone"]).is_err());
        assert!(Cli::try_parse_from(["swarmdrop", "send", "a.txt"]).is_err());
        assert!(Cli::try_parse_from(["swarmdrop", "send", "a.txt", "--to", "phone"]).is_ok());
    }

    /// 默认过滤器必须覆盖本 crate 自己的 target 根。
    ///
    /// 这条断言看守的是一个**静默**失效：写错前缀不会报错、不会 panic，只是本程序的
    /// 每一行日志都消失。改 bin 名或改过滤器时它会红。
    #[test]
    fn default_filter_covers_this_crate() {
        let root = module_path!().split("::").next().expect("crate root");
        assert_eq!(root, "swarmdrop", "bin 名变了就要同步改默认过滤器");

        let cli = Cli::try_parse_from(["swarmdrop", "status"]).expect("parse");
        assert!(
            cli.default_log_filter().contains(&format!("{root}=")),
            "默认过滤器没覆盖本 crate: {}",
            cli.default_log_filter()
        );
    }

    /// 配对是唯一要求用户看清屏幕再作决定的命令，它的默认日志必须安静。
    ///
    /// 这条看守的是一个只在真终端上才显形的缺陷：info 级的网络状态叙述会在确认提示
    /// 出现后的几百毫秒里把它顶出可视区，用户来不及核对对端身份就只剩一个光标。
    #[test]
    fn pairing_is_quiet_by_default() {
        let cli = Cli::try_parse_from(["swarmdrop", "pair"]).expect("parse");
        assert!(!cli.default_log_filter().contains("swarmdrop=info"));
    }

    /// 两种模式都必须压住第三方库的常态噪声。
    ///
    /// 只压交互模式是不够的：`start` 前台跑在终端里时同样被刷屏，而那正是用户
    /// 用来观察节点在干什么的那个窗口。
    #[test]
    fn third_party_noise_is_suppressed_in_every_mode() {
        for args in [
            vec!["swarmdrop", "pair"],
            vec!["swarmdrop", "start"],
            vec!["swarmdrop", "status", "--json"],
        ] {
            let cli = Cli::try_parse_from(args.clone()).expect("parse");
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
        let cli = Cli::try_parse_from(["swarmdrop", "devices", "--json"]).expect("parse");
        assert!(cli.json);
    }
}
