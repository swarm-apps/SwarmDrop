//! 接收落点。
//!
//! 命令行宿主没有界面让用户挑目录，所以落点取一个**用户可见的常规位置**并允许覆盖。
//! 不落进数据目录：那是应用私有区，用户在文件管理器里翻不到，收到的文件等于丢了。

use std::path::PathBuf;

use crate::exit::{CliError, CliResult};

/// 落点在下载目录里的子目录名（与桌面端默认落点同名，便于用户认出）。
const RECEIVE_SUBDIR: &str = "SwarmDrop";

/// 解析接收落点：环境变量覆盖优先，否则 `<下载目录>/SwarmDrop`。
///
/// 用环境变量而非配置文件做覆盖：命令行宿主常跑在脚本与服务单元里，那些地方设一个
/// 环境变量比维护一份配置文件自然得多。
pub fn resolve() -> CliResult<PathBuf> {
    if let Ok(explicit) = std::env::var("SWARMDROP_RECEIVE_DIR")
        && !explicit.trim().is_empty()
    {
        return ensure(PathBuf::from(explicit));
    }

    let dir = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|d| d.join(RECEIVE_SUBDIR)))
        .ok_or_else(|| {
            CliError::NodeUnavailable(
                "无法确定下载目录；请用 SWARMDROP_RECEIVE_DIR 指定接收落点".into(),
            )
        })?;
    ensure(dir)
}

fn ensure(dir: PathBuf) -> CliResult<PathBuf> {
    std::fs::create_dir_all(&dir)
        .map_err(|err| CliError::NodeUnavailable(format!("创建接收目录失败: {err}")))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环境变量覆盖生效，且目录被创建出来。
    ///
    /// 用 `--data-dir` 之外的独立开关是刻意的：数据目录是应用私有区，
    /// 收到的文件必须落在用户看得见的地方。
    #[test]
    fn explicit_override_is_created() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("inbox-here");

        // 直接测内部函数，避免依赖进程级环境变量（并行测试下互相干扰）。
        let resolved = ensure(target.clone()).unwrap();

        assert_eq!(resolved, target);
        assert!(target.is_dir());
    }
}
