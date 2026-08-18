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
    /// ⚠️ 将来若要放开网络层日志，**每个非 `swarmdrop` 前缀的 crate 都必须单列**
    /// （`EnvFilter` 按字符串前缀匹配，`webrtc`/`rtc*`/`wtransport`/`quinn` 都不以
    /// `swarmdrop` 开头）。桌面与移动端各有一条同样的常量与断言测试看守这件事，
    /// 漏掉哪条那一层的日志在生产里就一条都不出现。
    pub fn default_log_filter(&self) -> &'static str {
        if self.json {
            "warn"
        } else {
            "warn,swarmdrop_cli=info,swarmdrop_core=info,swarmdrop_net=info"
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
    },

    /// 停止正在运行的节点。
    ///
    /// 对前台与后台启动的节点同样生效；无节点在运行时以成功退出。
    Stop,

    /// 显示节点状态、监听地址、NAT 与中继可达性。
    Status,

    /// 生成配对邀请，或以一个邀请完成配对。
    Pair {
        /// 邀请链接。省略则生成一个新的邀请。
        invite: Option<String>,

        /// 不渲染二维码，只输出邀请链接（用于 CI / 日志 / 管道）。
        #[arg(long)]
        no_qr: bool,
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
        Command::Start { detach } => start::run(&data_dir, cli.json, detach).await,
        Command::Stop => stop::run(&data_dir, cli.json).await,
        Command::Status => status::run(&data_dir, cli.json).await,
        Command::Pair { invite, no_qr } => pair::run(&data_dir, cli.json, invite, no_qr).await,
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

    /// 全局选项在子命令之后也能被识别——用户不会记得它必须写在前面。
    #[test]
    fn global_flags_are_accepted_after_subcommand() {
        let cli = Cli::try_parse_from(["swarmdrop", "devices", "--json"]).expect("parse");
        assert!(cli.json);
    }
}
