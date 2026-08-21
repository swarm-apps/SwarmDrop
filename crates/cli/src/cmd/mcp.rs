//! `mcp`：以 MCP server 形态运行，让 agent 经标准输入输出调用 SwarmDrop。
//!
//! ## 它为什么持有节点到退出
//!
//! 按「这条命令会不会导致一个数据包离开本机」那句问句，它是 `NodeAccess` 档（会）。
//! 但与其余 `NodeAccess` 命令有一处不同：**节点持有到 server 退出**，而不是一次调用一次。
//!
//! 每次工具调用现起现关的话，每一次发送都要重连引导节点、重做 NAT 探测——秒级的调用会
//! 被拖成数秒，而 agent 的一轮对话里可能连着调好几次。`runtime/` 本就不得假设调用方是
//! 一次性命令（`standalone-cli-host/design.md` 的 D11），这里是那条约定的第一个长驻消费者。
//!
//! ## 自持节点时它必须摆成**常驻形态**
//!
//! 「持有节点数小时」这件事本身就要求它服务本地通道并接收入站传输——否则三种失败一起来，
//! 而且没有一种会报错：
//!
//! 1. 它握着单实例锁，别的命令连不上通道，只能一路重试到「另一个 swarmdrop 进程正在
//!    启动，等待超时」；
//! 2. `swarmdrop watch` 的判活永远为假，订阅停在直读记录的基线上，收不到任何实时事件；
//! 3. 那个节点在线、可达、已配对，却**一个文件都收不下**——而「接收不是一条命令」
//!    （spec: cli-host）正是本产品的基本承诺。
//!
//! 装配收在 [`crate::runtime::daemon`]，与 `swarmdrop start` 同一条。复用别人的常驻节点
//! 时本进程什么都不摆——那三件事都在那个进程里。
//!
//! ## 配对窗口不因它而打开
//!
//! 常驻节点判断「有没有人在等配对」的依据，是有没有客户端在轮询
//! [`crate::runtime::ipc::Request::PairWaitNext`]——本命令**从不发这条请求**，
//! 于是窗口始终是关的，入站配对一律被拒且不消费邀请凭证。
//!
//! 这不是疏漏而是要求（spec: `cli-mcp-host` 的「配对窗口不因 MCP 而打开」）：邀请会泄露
//! 且是一次性的，被抢走那次就消耗掉了凭证；窗口只应在人正在等待时打开，而一个长驻的
//! MCP server 后面没有人在看。由 [`crate::mcp`] 的
//! `the_mcp_host_never_polls_for_inbound_pairing` 看守。
//!
//! ⚠️ 被动**接收**不在此列，两者是不同的门：配对是「让一台陌生设备成为可信设备」，
//! 接收是「已经可信的设备发东西过来」。把后者也关掉不会让任何东西更安全，只会让这个
//! 节点变成一台收不到东西的设备。

use std::sync::Arc;

use crate::adapter::paths::DataDir;
use crate::exit::CliResult;
use crate::mcp;
use crate::runtime::access::NodeAccess;
use crate::runtime::daemon::Daemon;

pub async fn run(data_dir: &DataDir) -> CliResult<()> {
    // `json` 传 false：它控制的是节点装配时把事件渲染成哪种人类可读形式，而本命令的
    // stdout 归协议、事件只落 stderr 日志。传 true 会把事件也压成 JSON 打到日志里，
    // 那既不是给人看的也不是给宿主解析的。
    let access = NodeAccess::open(data_dir, false).await?;

    // 自持节点 ⇒ 摆成常驻形态（理由见模块文档）。复用常驻节点时为 `None`。
    //
    // `auto_accept: false`：配对窗口不因本命令而打开。
    let daemon = match access.local_arc() {
        Some(node) => Some(Daemon::start(data_dir, node.clone(), false)?),
        None => None,
    };

    let host = Arc::new(mcp::host::CliToolHost::new(access));

    let (result, via_channel) = tokio::select! {
        // 常规收摊：宿主关掉了 stdin。
        result = mcp::serve(host.clone()) => (result, false),
        // 自持节点期间收到停止信号，或另一个进程执行了 `swarmdrop stop`——后者停的就是
        // 本进程持有的那个节点，于是本 server 也跟着收摊：留下一个握着死节点的 server
        // 只会让此后每一次工具调用都失败，而宿主那侧看到的是「装了但用不了」。
        result = until_stopped(daemon.as_ref()) => (result, true),
    };

    // 自持节点时关停它；复用常驻节点时是空操作。
    //
    // ⚠️ **收 `&self` 是必须的**：`host` 被包进 `Arc` 交给了协议栈，被信号打断时那边的
    // 后台任务可能仍持着一份克隆。此前这里是 `Arc::into_inner(host)`，拿不到所有权就
    // 静默跳过——于是最需要清理的那条路径反而一步都不做。
    host.shutdown().await;

    if via_channel {
        crate::exit::exit_now(crate::cmd::finish(result));
    }
    result
}

/// 等到「该收摊了」为止。
///
/// 自持节点时那就是常驻服务循环（它自己接 `stop` 与信号）；复用别人的常驻节点时通道
/// 不归本进程管，但**信号仍然归本进程管**。
///
/// ⚠️ 这一支此前写成 `std::future::pending()`——于是复用常驻节点的 `swarmdrop mcp`
/// **一个信号处理器都没装**，`SIGTERM` 按默认处置直接杀掉它（退出码 143），而 spec
/// `cli-event-stream` 的「退出语义」要求的是成功退出。护栏测试用的是全新的临时目录，
/// 只走得到自持那一支，所以它一直绿着。
async fn until_stopped(daemon: Option<&Daemon>) -> CliResult<()> {
    match daemon {
        Some(daemon) => daemon.serve().await,
        None => {
            crate::runtime::signal::Shutdown::listen().recv().await;
            Ok(())
        }
    }
}
