//! 退出码与失败分类。
//!
//! 脚本与 agent harness 靠退出码判断结果，**不应该被迫解析文本**（spec: cli-host
//! 「退出码区分失败原因」）。因此失败分类与退出码在同一处定义、一一对应——分开定义时，
//! 新增一个失败分类而忘记给它退出码是**静默**的，调用方只会看到一个笼统的失败。

use std::process::ExitCode;

/// 退出码。
///
/// `0` 与 `2` 沿用既有惯例（POSIX 成功、clap 的用法错误），`130` 是 shell 的
/// `128 + SIGINT`。中间三个是本程序自有的分类，取值只要求稳定且互不相同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    #[error("已中止")]
    Aborted,
}

impl CliError {
    pub fn code(&self) -> Code {
        // 无 `_ =>` 兜底分支：新增一个变体时编译器会在这里报错，
        // 强制为它选一个退出码，而不是让它静默落进某个笼统的类别。
        match self {
            Self::Usage(_) => Code::Usage,
            Self::NodeUnavailable(_) => Code::NodeUnavailable,
            Self::PeerUnreachable(_) => Code::PeerUnreachable,
            Self::TransferFailed(_) => Code::TransferFailed,
            Self::PairingRefused(_) => Code::PairingRefused,
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
            CliError::Aborted.code(),
        ];
        let mut seen = std::collections::HashSet::new();
        for code in codes {
            assert_ne!(code, Code::Success, "失败分类不得映射到成功码");
            assert!(seen.insert(code as u8), "退出码重复: {code:?}");
        }
    }
}
