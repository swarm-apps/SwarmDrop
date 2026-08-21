//! 接收落点的三个**原语**：环境变量、内置默认、确保可用。
//!
//! 命令行宿主没有界面让用户挑目录，所以落点取一个**用户可见的常规位置**并允许覆盖。
//! 不落进数据目录：那是应用私有区，用户在文件管理器里翻不到，收到的文件等于丢了。
//!
//! ⚠️ **三层来源的组合不在这里**（`环境变量 → 持久化配置 → 内置默认`）。那是业务判断，
//! 归 [`crate::runtime::settings::scalar`]；本层按分层约定只提供不含判断的原语。
//! 混进来的话，「此刻生效的落点是哪个」会有两个答案——一个给接收路径，一个给配置读面。

use std::path::PathBuf;

use crate::exit::{CliError, CliResult};

/// 压住持久化配置的环境变量。
///
/// 它保持最高优先级是刻意的：命令行宿主常跑在脚本与服务单元里，那些地方设一个环境变量
/// 比维护一份配置文件自然得多。新增持久化配置不等于要把它降级。
pub const ENV_VAR: &str = "SWARMDROP_RECEIVE_DIR";

/// 落点在下载目录里的子目录名（与桌面端默认落点同名，便于用户认出）。
const RECEIVE_SUBDIR: &str = "SwarmDrop";

/// 环境变量给出的落点。未设置、或全是空白时为 `None`。
///
/// ⚠️ **展开 `~` 但不做别的**：环境变量不经 shell——写 `~/Downloads/x` 会原样传进来，
/// 于是没有这一步就会**创建一个名字就叫 `~` 的目录**并把收到的文件放进去。
///
/// 同样重要的是**不要**用 [`crate::prompt::paths::parse`]（那是给交互输入框用的）：
/// 它按 shell 规则拆行，而环境变量里的空格就是路径的一部分——`/home/me/My Files`
/// 会被截成 `/home/me/My`，同样是静默地把文件放进一个用户找不到的地方。
pub fn from_env() -> Option<PathBuf> {
    let raw = std::env::var(ENV_VAR).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| crate::prompt::paths::expand(trimmed))
}

/// 内置默认落点 `<下载目录>/SwarmDrop`。
///
/// `None` 只出现在拿不到下载目录的环境（没有 `$HOME` 的服务单元、无桌面的容器）。
/// 那时接收会被一句可行动的错误拦下，而不是把文件收进一个用户找不到的地方。
pub fn default_dir() -> Option<PathBuf> {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|d| d.join(RECEIVE_SUBDIR)))
}

/// 确保这个目录存在且写得进去，返回它自己。
///
/// **写探针不是多余的**：`create_dir_all` 对一个已存在但只读的目录会成功返回，于是
/// 「保存成功」之后第一次接收才失败——而那时用户已经在另一台设备上按了发送。
pub fn ensure_writable(dir: PathBuf) -> CliResult<PathBuf> {
    std::fs::create_dir_all(&dir)
        .map_err(|err| CliError::Usage(format!("无法创建接收目录 {}: {err}", dir.display())))?;

    let probe = dir.join(".swarmdrop-write-test");
    std::fs::write(&probe, b"")
        .map_err(|err| CliError::Usage(format!("接收目录 {} 不可写: {err}", dir.display())))?;
    // 删不掉不算失败：目录**能写**这件事已经证明了，而残留只是一个空文件。
    let _ = std::fs::remove_file(&probe);

    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录被创建出来，并通过写探针。
    ///
    /// 用 `--data-dir` 之外的独立开关是刻意的：数据目录是应用私有区，
    /// 收到的文件必须落在用户看得见的地方。
    #[test]
    fn ensuring_creates_the_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("inbox-here");

        assert_eq!(ensure_writable(target.clone()).expect("可写"), target);
        assert!(target.is_dir());
        // 探针文件不留下——它每次接收前都会跑一遍。
        assert!(!target.join(".swarmdrop-write-test").exists());
    }

    /// 只读目录必须当场被拒，而不是留到第一次接收时才失败。
    #[cfg(unix)]
    #[test]
    fn a_read_only_directory_is_refused() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("locked");
        std::fs::create_dir(&target).expect("建目录");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500)).expect("改权限");

        // root 无视权限位，那种环境下这条测不出东西——**先自己试一次**再决定断言什么，
        // 而不是断言一个在 CI 容器里恒假的命题。
        let writable_anyway = std::fs::write(target.join(".root-probe"), b"").is_ok();
        let refused = ensure_writable(target.clone()).is_err();

        // 先恢复权限，否则 TempDir 的析构删不掉它。
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).expect("恢复");

        assert!(
            refused || writable_anyway,
            "只读目录应当被拒——写探针没起作用"
        );
    }
}
