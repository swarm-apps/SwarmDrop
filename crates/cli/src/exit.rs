//! 退出码与失败分类。
//!
//! 脚本与 agent harness 靠退出码判断结果，**不应该被迫解析文本**（spec: cli-host
//! 「退出码区分失败原因」）。因此失败分类与退出码在同一处定义、一一对应——分开定义时，
//! 新增一个失败分类而忘记给它退出码是**静默**的，调用方只会看到一个笼统的失败。

use std::process::ExitCode;

/// 立刻结束进程，跳过运行时析构。
///
/// ⚠️ **只给「本进程把 stdin 交给了 tokio 的阻塞读」的那几条路径。** tokio 的
/// `io::stdin()` 背后是一个**阻塞**读任务，而 `main` 返回时运行时析构会等所有阻塞任务
/// 收尾——对面还握着 stdin 不放时，那次读**永远不会返回**，进程就此挂死。
///
/// 挂死比退非零更糟：服务管理器会一直等到自己的超时，agent harness 会留下一个僵尸子进程，
/// 而单实例锁虽已释放、进程却还在，`ps` 上看是「停不掉」。
///
/// **两个用例**：`swarmdrop mcp`（stdin 是宿主的请求流）与
/// `swarmdrop invite create --decide-from-stdin`（stdin 是决策通道）。两者都是被信号或
/// `swarmdrop stop` 从外面叫停的——正常收摊那条路上 stdin 已经 EOF，运行时析构等得到，
/// 不必走这里。
///
/// **调用前清理必须已经做完**（节点关停、锁释放）：这之后什么都不会再跑。
pub fn exit_now(code: Code) -> ! {
    use std::io::Write;

    // `process::exit` 不跑 Rust 的 stdout 刷新。流式路径上只有完整的行，
    // 但显式刷一次的代价是零，而少刷一次的代价是最后一行悄悄消失。
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code as i32)
}

/// 退出码。
///
/// `0` 与 `2` 沿用既有惯例（POSIX 成功、clap 的用法错误），`130` 是 shell 的
/// `128 + SIGINT`。中间几个是本程序自有的分类，取值只要求稳定且互不相同。
/// `Serialize` 是为了让分类能过本地通道：服务端把失败的类别一并回给客户端，
/// 否则通道那侧的失败只能一律按「节点不可用」处理（见 [`crate::runtime::ipc::Response`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Code {
    /// 命令按预期完成。
    Success = 0,
    /// 参数或用法错误。与 clap 自身的退出码一致。
    Usage = 2,
    /// 节点未就绪或不可用（未运行、已被占用、装配失败）。
    NodeUnavailable = 3,
    /// 目标设备当前无法建立连接。
    ///
    /// **与 [`Self::TransferFailed`] 必须分开**：前者是「没连上」，重试或等对方上线可能
    /// 就好；后者是「连上了但传输中断」，可能需要检查磁盘或续传。调用方对两者的处置不同。
    PeerUnreachable = 4,
    /// 已建立连接，但传输过程中失败。
    TransferFailed = 5,
    /// 对端明确拒绝了配对请求。
    ///
    /// **与 [`Self::PeerUnreachable`] 必须分开**：拒绝意味着链路是通的、对方看见了并且
    /// 说了不——重试同一条邀请只会再被拒一次。调用方该做的是换一张邀请或让对方主动发起，
    /// 而不是像对待「没连上」那样退避重试。
    PairingRefused = 6,
    /// 更新失败。
    ///
    /// **与 [`Self::NodeUnavailable`] 分开**：更新不碰节点，两者共用一个码会让脚本把
    /// 「装不上新版本」误当成「节点起不来」而去重启节点——那既没用，还会让真正的原因
    /// （网络、权限、渠道不对）被掩盖。
    UpdateFailed = 7,
    /// 被用户中止。
    Aborted = 130,
}

impl From<Code> for ExitCode {
    fn from(code: Code) -> Self {
        ExitCode::from(code as u8)
    }
}

/// 命令失败的原因，与 [`Code`] 一一对应。
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NodeUnavailable(String),
    #[error("{0}")]
    PeerUnreachable(String),
    #[error("{0}")]
    TransferFailed(String),
    #[error("{0}")]
    PairingRefused(String),
    #[error("{0}")]
    UpdateFailed(String),
    #[error("已中止")]
    Aborted,
}

impl CliError {
    /// 按分类重建一个错误。
    ///
    /// 通道对面送回来的失败要还原成同一个类别，否则它的退出码会与本地路径产出的
    /// 那条不同——同一件事因此有两个退出码，取决于此刻有没有常驻节点在跑。
    ///
    /// `Success` 不是失败：真收到它说明服务端把成功编码成了错误，那是它的 bug，
    /// 按「节点不可用」处理（客户端无法为服务端的编码错误给出更准确的类别）。
    pub fn from_code(code: Code, message: String) -> Self {
        match code {
            Code::Usage => Self::Usage(message),
            Code::NodeUnavailable | Code::Success => Self::NodeUnavailable(message),
            Code::PeerUnreachable => Self::PeerUnreachable(message),
            Code::TransferFailed => Self::TransferFailed(message),
            Code::PairingRefused => Self::PairingRefused(message),
            Code::UpdateFailed => Self::UpdateFailed(message),
            // `Aborted` 的 Display 是固定的「已中止」，**消息会被丢掉**。
            // 这可以接受的前提是「服务端不产出 Aborted」——中止是本地的用户动作
            // （Ctrl-C），通道对面没有立场替用户宣布中止。由
            // `aborted_is_never_produced_by_the_server` 看守。
            Code::Aborted => Self::Aborted,
        }
    }

    pub fn code(&self) -> Code {
        // 无 `_ =>` 兜底分支：新增一个变体时编译器会在这里报错，
        // 强制为它选一个退出码，而不是让它静默落进某个笼统的类别。
        match self {
            Self::Usage(_) => Code::Usage,
            Self::NodeUnavailable(_) => Code::NodeUnavailable,
            Self::PeerUnreachable(_) => Code::PeerUnreachable,
            Self::TransferFailed(_) => Code::TransferFailed,
            Self::PairingRefused(_) => Code::PairingRefused,
            Self::UpdateFailed(_) => Code::UpdateFailed,
            Self::Aborted => Code::Aborted,
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个失败分类都有独立的退出码——两个分类共用一个码等于调用方分不出它们。
    #[test]
    fn every_failure_has_a_distinct_code() {
        let codes = [
            CliError::Usage(String::new()).code(),
            CliError::NodeUnavailable(String::new()).code(),
            CliError::PeerUnreachable(String::new()).code(),
            CliError::TransferFailed(String::new()).code(),
            CliError::PairingRefused(String::new()).code(),
            CliError::UpdateFailed(String::new()).code(),
            CliError::Aborted.code(),
        ];
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert_ne!(code, Code::Success, "失败分类不得映射到成功码");
            assert!(seen.insert(code as u8), "退出码重复: {code:?}");
        }
    }
}
