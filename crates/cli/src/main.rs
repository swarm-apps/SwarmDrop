//! `swarmdrop` —— SwarmDrop 的命令行宿主。
//!
//! 桌面 / 移动 / Web 之外的第四个宿主。与前三者共用 [`swarmdrop_core`] 的组合根，
//! 自备 headless 的事件与文件访问实现。
//!
//! ## 分层
//!
//! ```text
//! cmd/      解析参数 → 调用 → 交给 render      不含网络与存储细节
//! runtime/  节点装配 / 单实例仲裁 / 本地通道    不含任何面向用户的文案
//! adapter/  端口实现（事件、文件、路径）         不含业务判断
//! render/   人类可读 / 机器可读两套输出          不含业务判断
//! prompt/   向用户提问（只在有终端时）              不含业务判断
//! ```
//!
//! 这几条约束不是审美：`runtime/` 一旦混进用户文案，交互式前端就没法复用它；
//! `render/` 一旦混进业务判断，两套输出就会各自漂移出不同的行为；`prompt/` 一旦自己
//! 决定「要不要问」，那个判断就会散进每一处调用点，而它必须只回答一次
//! ——命令行宿主同样跑在服务单元与脚本里，那些场景没有人可问。
//!
//! ⚠️ **`runtime/` 不得假设调用方是一次性命令**。它同时要服务常驻的 `start`、
//! 一次性的 `send`，以及将来可能出现的长期存活的交互式前端。判据见
//! `openspec/changes/standalone-cli-host/design.md` 的 D11。

mod adapter;
mod cmd;
mod exit;
mod prompt;
mod render;
mod runtime;

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = cmd::Cli::parse();

    // 日志一律走 stderr：stdout 在结构化模式下只能有最终结果，任何诊断信息混入
    // 都会破坏调用方的解析（spec: cli-host「结构化输出模式」）。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(cli.default_log_filter())),
        )
        .init();

    // 顶层自己建 runtime 而不是 `#[tokio::main]`：退出码要由 `exit` 统一裁决，
    // 而 `#[tokio::main]` 会把 `main` 的返回值语义固定成 `Termination` 的默认实现。
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("无法创建异步运行时: {err}");
            return exit::Code::NodeUnavailable.into();
        }
    };

    runtime.block_on(cmd::dispatch(cli)).into()
}
