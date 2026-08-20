//! 自更新的机制：装这份程序的是哪条渠道、有没有新版本、把新版本装上。
//!
//! **本层不含面向用户的文案**（分层约束见 crate 文档）。它只回答事实，「怎么说」归
//! [`crate::render::update`]，「要不要做」归 [`crate::cmd::update`]。
//!
//! ## 为什么是子命令而不是 dist 的独立 updater
//!
//! `dist` 的 `install-updater = true` 会产出一个**独立可执行文件**，随 shell / powershell
//! installer 一起装。本仓不用它，`dist-workspace.toml` 里那行保持 `false`，理由有两条：
//!
//! 1. **那个文件在本仓叫 `swarmdrop-cli-update`**——它按**包名**命名，而本仓的包名与 bin
//!    名刻意不同（`swarmdrop-cli` vs `swarmdrop`）。于是 dist 文档宣传的「支持外部子命令
//!    的程序可以直接 `yourpackage update`」在这里**不成立**：clap 的外部子命令机制查的是
//!    `swarmdrop-update`，那个文件不叫这个名字。用户得记住一个与主命令不同名的二进制。
//! 2. 它只随两条渠道分发，而 `swarmdrop update` 编进二进制、**每条渠道都在**——包管理器
//!    装的那些正是最需要一句「请用 brew 升级」的，独立 updater 恰恰不会出现在那里。
//!
//! 「内建 updater 会与包管理器争夺事实源」这条顾虑仍然成立，但它的解法是[`Channel`]：
//! **先认出渠道，不属于自己的就转交出去**，而不是不做自更新。

use std::path::Path;

use axoupdater::{AxoUpdater, AxoupdateError};

use crate::exit::{CliError, CliResult};

/// axoupdater 认的应用名。
///
/// ⚠️ **是 cargo 包名 `swarmdrop-cli`，不是 bin 名 `swarmdrop`。** install receipt 由 dist
/// 的 installer 按包名写在 `~/.config/<包名>/<包名>-receipt.json`（Windows 是
/// `%LOCALAPPDATA%` 下），axoupdater 也按同一个名字去找它。写成 bin 名的话
/// [`AxoUpdater::load_receipt`] 会一律报 `NoReceipt`，于是**每一个用户都会被判成
/// 「不是 shell installer 装的」**，自更新整条路径静默失效——而它看起来只是一句
/// 「请用当初安装它的方式升级」，不像故障。
///
/// ⚠️ 它与 [`crate::cmd::Cli::default_log_filter`] 里那条**方向正好相反**（那里必须用 bin
/// 名，因为 tracing 的 target 取 `module_path!()`，而 bin target 的 crate 根是 bin 名）。
/// 两处都对，别为了「统一」把其中一个改成另一个——各自的消费方本来就用不同的名字。
/// 由 [`tests::app_name_is_the_package_name`] 看守。
const APP_NAME: &str = "swarmdrop-cli";

/// 抑制启动时的更新检查。
///
/// 用环境变量而非配置文件，与接收落点的覆盖同一个体例：命令行宿主常跑在脚本与服务单元里，
/// 那些地方没有人会去改一个配置文件，但注入一个环境变量是现成的。
pub const NO_CHECK_ENV: &str = "SWARMDROP_NO_UPDATE_CHECK";

/// 两次启动检查之间的最小间隔。
///
/// 24 小时不是为了省流量，是为了 **GitHub 的匿名 API 限速（60 次/小时/IP）**：本仓三条
/// 版本线发到同一个 releases 列表，而 `releases/latest` 常年指向桌面端（`cli-release-polish.yml`
/// 主动这么做），于是 axoupdater 每次检查都命不中、要退回去**分页枚举全部 release**。
/// 一次检查因此不止一个请求——实测请求打到了 `?page=4`，几轮调试就把配额耗尽（那时的
/// 失败形态是 `403 rate limit exceeded`）。
const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// 装这份程序的是哪条渠道。
///
/// 决定的是「能不能就地自更新」以及「不能的话该把用户转交给谁」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// dist 的 shell / powershell installer 装的——有 install receipt，能就地自更新。
    Managed,
    Homebrew,
    Npm,
    /// 认不出来：`cargo install`、从源码构建、手动解压归档，都落在这里。
    Unknown,
}

impl Channel {
    /// 这条渠道能不能由本程序自己完成更新。
    pub fn is_self_updatable(self) -> bool {
        // 穷尽 match 而非 `matches!`：新增渠道时这里会编译失败，而不是让它默认落进
        // 「不能自更新」——那会静默地把一条本可以自更新的渠道降级成一句提示。
        match self {
            Self::Managed => true,
            Self::Homebrew | Self::Npm | Self::Unknown => false,
        }
    }
}

/// 认出装这份程序的渠道。
pub fn detect_channel() -> Channel {
    classify(
        std::env::current_exe().ok().as_deref(),
        receipt_matches_this_exe(),
    )
}

/// 系统上有一份 install receipt，**并且它描述的就是正在运行的这个可执行文件**。
///
/// 后半句不是多余的谨慎，它有一个具体的失败场景：用户先用安装脚本装过，后来改用
/// Homebrew——旧 receipt 还躺在 `~/.config/` 里，而跑起来的是 brew 那份。只看
/// 「receipt 在不在」会把它判成 [`Channel::Managed`]，于是自更新去改写另一个位置的文件，
/// 而用户实际执行的那份纹丝不动。
///
/// **判据借 axoupdater 自己的实现**（它正是为这个场景写的，文档里写着「guard against
/// cases where the running EXE is from a package manager, but a receipt from a
/// shell installed-copy is present」），而不是自己去推导 receipt 的路径：那条推导要认
/// `AXOUPDATER_CONFIG_PATH` / `AXOUPDATER_CONFIG_WORKING_DIR` / `XDG_CONFIG_HOME` /
/// `%LOCALAPPDATA%` 四套规则，重写一遍只会在它变的那天悄悄对不上。
///
/// ⚠️ **不能把这道校验留给 axoupdater 内部那次调用。** 它在
/// [`AxoUpdater::is_update_needed`] 里，不匹配时返回的是 `false`（「不需要更新」）
/// ——于是 brew 用户会看到一句「已是最新版本」，哪怕真有新版本。同一个事实，
/// 在这里能翻成「该用 brew 升级」，在那里只能翻成一句错话。
fn receipt_matches_this_exe() -> bool {
    let mut updater = AxoUpdater::new_for(APP_NAME);
    updater.load_receipt().is_ok()
        && updater
            .check_receipt_is_for_this_executable()
            .unwrap_or(false)
}

/// 渠道判定的纯函数部分。
///
/// ⚠️ **receipt 必须先判，路径判据只在 receipt 对不上时才看。** 本仓的
/// `install-path = "CARGO_HOME"` 让 shell installer **也**把二进制装进 `~/.cargo/bin`
/// ——路径判据先行的话，正常的 shell 安装会被认成 `cargo install` 而永远拒绝自更新。
/// 顺序反了不会报错，只是自更新再也不工作。由 [`tests::receipt_outranks_the_path`] 看守。
///
/// 路径判据本身是启发式，认不出就是 [`Channel::Unknown`]——那一格的文案不做任何断言
/// （只指向 releases 页），所以判错的代价是「说得不够具体」而不是「说错」。
fn classify(exe: Option<&Path>, receipt_matches: bool) -> Channel {
    if receipt_matches {
        return Channel::Managed;
    }
    let Some(exe) = exe else {
        return Channel::Unknown;
    };

    // 大小写归一：Windows 的路径大小写不敏感，而这些判据都是路径里的固定片段。
    // 分隔符也归一，好让同一组判据在两个平台上通用。
    let path = exe.to_string_lossy().to_lowercase().replace('\\', "/");

    // npm 装的二进制躺在包目录里（全局装与 `npx` 都一样）。
    if path.contains("/node_modules/") {
        return Channel::Npm;
    }
    // Homebrew 的实体在 Cellar 下，`bin/` 里那个是符号链接——而 `current_exe` 解析符号链接，
    // 所以这里看到的是 Cellar 那条。`/homebrew/` 兜住 Linuxbrew 与非默认前缀。
    if path.contains("/cellar/") || path.contains("/homebrew/") {
        return Channel::Homebrew;
    }
    Channel::Unknown
}

/// 本程序当前的版本。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 查有没有新版本；`Ok(None)` 表示已是最新。
///
/// **只查不装。** 调用方拿到版本号之后自己决定是提示还是安装。
///
/// ⚠️ **比较由 [`AxoUpdater::is_update_needed`] 做，不能只用
/// [`AxoUpdater::query_new_version`]。** 后者的名字有误导性——它只是把最新 release 的
/// 版本号取回来，**一次比较都不做**，fetch 成功时永远是 `Some`。只用它的话，
/// 已经是最新的用户也会被告知「有新版本可用：0.2.0 → 0.2.0」。
pub async fn query_new_version() -> CliResult<Option<String>> {
    let mut updater = managed_updater()?;
    // 只查版本也要把 installer 输出关掉：这个方法的调用方之一是启动时的后台检查，
    // 它绝不能往用户的终端上写任何东西。
    updater.disable_installer_output();

    // ⚠️ **顺序不能反。** `query_new_version` 是**无条件** `fetch_release()`
    // （axoupdater 0.10.2 `lib.rs:630`），而 `is_update_needed` 只在
    // `requested_release` 为空时才取（`lib.rs:347-351`）。
    // 先 `is_update_needed` 再 `query_new_version` 等于**取两轮**——而一轮不止一个请求：
    // `releases/latest` 常年指向桌面端（`cli-release-polish.yml` 主动这么做），
    // 于是每次都命不中、退回分页枚举全部 release（实测打到 `?page=4`）。
    // 两轮就是十来个匿名请求，而 GitHub 的匿名配额是 60 次/小时/IP——整段 24 小时节流
    // 的理由会被这一处抵消一半，且**恰好只坑到有新版本的那批用户**。
    let latest = updater
        .query_new_version()
        .await
        .map_err(describe)?
        .map(ToString::to_string);
    let Some(latest) = latest else {
        return Ok(None);
    };
    // 这一步复用上面取回来的那份，不再发请求。
    if !updater.is_update_needed().await.map_err(describe)? {
        return Ok(None);
    }
    Ok(Some(latest))
}

/// 装上最新版本；`Ok(None)` 表示本来就是最新，什么都没动。
///
/// `quiet` 为真时连底层 installer 的 stderr 也一并关掉——`--json` 模式声明的是
/// 「调用方是程序」，那里不该有任何面向人的进度输出。
pub async fn install_latest(quiet: bool) -> CliResult<Option<Installed>> {
    let mut updater = managed_updater()?;
    // **stdout 一律关掉**：底层 installer 默认往 stdout 写进度，而本程序的 stdout 只能
    // 有命令的最终结果（spec: cli-host「结构化输出模式」）。进度属于诊断，归 stderr。
    updater.disable_installer_stdout();
    if quiet {
        updater.disable_installer_stderr();
    }

    let result = updater.run().await.map_err(describe)?;
    Ok(result.map(|done| Installed {
        old_version: done.old_version.map(|v| v.to_string()),
        new_version: done.new_version.to_string(),
        install_prefix: done.install_prefix.to_string(),
    }))
}

/// 一次成功的更新做了什么。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub old_version: Option<String>,
    pub new_version: String,
    pub install_prefix: String,
}

/// 建一个已加载 receipt 的更新器。
///
/// 调用它之前必须已经确认渠道是 [`Channel::Managed`]——否则这里会以 `NoReceipt` 失败，
/// 而那条错误对用户毫无意义（他并没有做错什么，只是用 brew 装的）。
///
/// ⚠️ **比较基准必须换成编译进本二进制的版本，且只能在 `load_receipt` 之后换。**
/// axoupdater 默认拿 **receipt 里记的版本**去比，而那是「上次 installer 装了什么」，
/// 不是「此刻正在跑的是什么」。两者在一种常见情形下会分叉：用户把二进制换掉了
/// （`cargo install`、手动覆盖、从源码构建后拷过去），receipt 却还停在旧版本号上。
/// 那时用户会看到自相矛盾的一屏——命令打印的当前版本取自 [`current_version`]（真实值），
/// 而「有没有新版本」的判断却基于 receipt 里的旧值，于是「当前 0.2.0 / 最新 0.2.0」
/// 也能报出有更新。顺序反了同样不行：`load_receipt` 会**覆写**这个字段。
fn managed_updater() -> CliResult<AxoUpdater> {
    let mut updater = AxoUpdater::new_for(APP_NAME);
    updater.load_receipt().map_err(describe)?;

    // 解析失败就保留 receipt 里那份。这是编译期常量、由 cargo 保证是 semver，
    // 走不到这条分支；写成降级而非 `expect` 只是为了不让一个更新检查 panic 掉整个进程。
    if let Ok(running) = axoupdater::Version::parse(current_version()) {
        updater.set_current_version(running).map_err(describe)?;
    }
    Ok(updater)
}

/// 把 axoupdater 的错误翻成本程序的失败分类。
///
/// 全部归 [`CliError::UpdateFailed`]：调用方对它们的处置是同一个（稍后再试或换渠道），
/// 再细分只会多出几个调用方分不出用途的退出码。**原文照带**——那些消息里有版本号、
/// 路径与 HTTP 状态，是排查时唯一的凭据。
fn describe(err: AxoupdateError) -> CliError {
    CliError::UpdateFailed(format!("{err}"))
}

/// 启动时的更新检查该不该在此刻发起。
///
/// 三道闸，任何一道拦下都不检查：
///
/// 1. `--json`：那种模式的调用方是程序，一行提示对它是噪声。
/// 2. [`NO_CHECK_ENV`]：用户或上游进程明确说了不要。
/// 3. 距上次检查不足 [`CHECK_INTERVAL`]。
///
/// 渠道不在这里判——那是调用方的事，因为它还要决定拿检查结果做什么。
pub fn should_check(data_dir: &crate::adapter::paths::DataDir, json: bool) -> bool {
    if json || std::env::var_os(NO_CHECK_ENV).is_some() {
        return false;
    }
    throttle_elapsed(data_dir)
}

/// 距上次检查是否已超过 [`CHECK_INTERVAL`]。
///
/// 从 [`should_check`] 里单拆出来是为了**让节流可以被测到**：那两道闸里有一道读环境变量，
/// 而环境变量是进程全局的——测试要么依赖开发机恰好没设 [`NO_CHECK_ENV`]，要么就得去改它
/// （并行测试下不安全）。纯函数没有这个问题。
fn throttle_elapsed(data_dir: &crate::adapter::paths::DataDir) -> bool {
    match last_checked(data_dir) {
        // ⚠️ **`Err` 要当成「该查」，不能 `unwrap_or_default()`。**
        // `elapsed()` 在时间戳位于**未来**时返回 `Err`，降级成 `Duration::ZERO` 就是
        // 「刚查过」——而 `mark_checked` 只在决定要查之后才写，那个未来时间戳因此
        // **永远不会被改写**。主板电池没电、首次开机 NTP 未同步、虚拟机快照回滚、
        // 双启系统的 RTC 本地时间/UTC 混用，都能让这台机器在时钟校回来之后
        // **数年内不再提示任何更新**，且无法自愈。
        // 方向与下面 `None => true` 一致：降级代价最多是多查一次网络。
        Some(at) => match at.elapsed() {
            Ok(elapsed) => elapsed >= CHECK_INTERVAL,
            // 时间戳在未来 ⇒ 时钟回拨过。当成「该查」。
            Err(_) => true,
        },
        // 读不到就检查。**与身份存储那条「读取失败不降级」方向相反，这是刻意的**：
        // 那里读失败会导致生成新身份并覆盖原文件（一次坏块换掉整个身份），而这里最坏
        // 只是多查一次网络。降级的代价决定了该往哪边倒。
        None => true,
    }
}

/// 记下「刚查过」。
///
/// **写失败静默忽略**：最坏结果是下次启动多查一次网络，而为它中断启动是本末倒置
/// ——用户执行的是 `start`，不是「更新检查」。
pub fn mark_checked(data_dir: &crate::adapter::paths::DataDir) {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let state = CheckState {
        last_checked_unix: now.as_secs(),
    };
    if let Ok(text) = serde_json::to_string(&state) {
        let _ = std::fs::write(data_dir.update_check(), text);
    }
}

fn last_checked(data_dir: &crate::adapter::paths::DataDir) -> Option<std::time::SystemTime> {
    let text = std::fs::read_to_string(data_dir.update_check()).ok()?;
    let state: CheckState = serde_json::from_str(&text).ok()?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(state.last_checked_unix))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckState {
    last_checked_unix: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// receipt 的应用名必须是**包名**。
    ///
    /// 写成 bin 名 `swarmdrop` 时没有任何编译期信号，运行时也不报错——只是每个用户都被
    /// 判成「不是 shell installer 装的」，自更新静默失效。所以只能由一条断言钉住它。
    #[test]
    fn app_name_is_the_package_name() {
        assert_eq!(APP_NAME, env!("CARGO_PKG_NAME"));
        assert_ne!(APP_NAME, "swarmdrop", "写成了 bin 名");
    }

    /// **receipt 对得上就是 Managed，路径长什么样都不看。**
    ///
    /// 看守的是判定顺序：本仓 `install-path = "CARGO_HOME"`，shell installer 装出来的
    /// 二进制就躺在 `~/.cargo/bin` 里；要是先按路径判，正常安装会被认成别的渠道而
    /// **永远拒绝自更新**。顺序反了不会报错，只是这个功能不再工作。
    #[test]
    fn receipt_outranks_the_path() {
        for path in [
            "/Users/me/.cargo/bin/swarmdrop",
            "/opt/homebrew/Cellar/swarmdrop/0.2.0/bin/swarmdrop",
            "/usr/lib/node_modules/swarmdrop/swarmdrop",
        ] {
            assert_eq!(
                classify(Some(Path::new(path)), true),
                Channel::Managed,
                "{path} 的 receipt 对得上却没判成 Managed"
            );
        }
    }

    #[test]
    fn package_managers_are_recognized_without_a_receipt() {
        let cases = [
            (
                "/opt/homebrew/Cellar/swarmdrop/0.2.0/bin/swarmdrop",
                Channel::Homebrew,
            ),
            (
                "/home/me/.linuxbrew/Cellar/swarmdrop/0.2.0/bin/swarmdrop",
                Channel::Homebrew,
            ),
            (
                "/usr/local/lib/node_modules/swarmdrop/bin/swarmdrop",
                Channel::Npm,
            ),
            ("/Users/me/.cargo/bin/swarmdrop", Channel::Unknown),
            (
                "/Volumes/dev/SwarmDrop/target/debug/swarmdrop",
                Channel::Unknown,
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(classify(Some(Path::new(path)), false), expected, "{path}");
        }
    }

    /// Windows 的路径写法（反斜杠 + 盘符大小写）必须走到同一个判定。
    ///
    /// 判据是路径里的固定片段，而那条路径在 Windows 上用另一种分隔符、且大小写不敏感
    /// ——不归一的话这几条判据在 Windows 上一条都不命中，全部落进 `Unknown`。
    #[test]
    fn windows_paths_normalize_to_the_same_verdict() {
        let path =
            Path::new(r"C:\Users\Me\AppData\Roaming\npm\node_modules\swarmdrop\swarmdrop.exe");
        assert_eq!(classify(Some(path), false), Channel::Npm);
    }

    /// 定位不到自身可执行文件时不得 panic，按「认不出」处理。
    #[test]
    fn a_missing_executable_path_is_not_fatal() {
        assert_eq!(classify(None, false), Channel::Unknown);
    }

    /// 只有 `Managed` 能自更新——新增渠道时如果忘了想清楚这件事，
    /// [`Channel::is_self_updatable`] 的穷尽 match 会先编译失败。
    #[test]
    fn only_the_managed_channel_updates_itself() {
        assert!(Channel::Managed.is_self_updatable());
        for channel in [Channel::Homebrew, Channel::Npm, Channel::Unknown] {
            assert!(!channel.is_self_updatable(), "{channel:?}");
        }
    }

    /// 节流：刚查过就不再查，超过间隔才放行。
    /// **时钟回拨过的机器仍然要检查。**
    ///
    /// 未来的时间戳让 `elapsed()` 返回 `Err`；此前那版 `unwrap_or_default()` 把它降级成
    /// 「刚查过」，而 `mark_checked` 只在决定要查之后才写——于是那个时间戳**永远不会被
    /// 改写**，这台机器在时钟校回来之后数年内不再提示任何更新，且无法自愈。
    /// 触发它不需要什么异常操作：主板电池没电、首次开机 NTP 未同步、虚拟机快照回滚都行。
    #[test]
    fn a_future_timestamp_does_not_permanently_disable_the_check() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data_dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");

        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(4 * 365 * 86400);
        let unix = future
            .duration_since(std::time::UNIX_EPOCH)
            .expect("未来时刻晚于纪元");
        std::fs::write(
            data_dir.update_check(),
            serde_json::to_string(&CheckState {
                last_checked_unix: unix.as_secs(),
            })
            .expect("编码"),
        )
        .expect("写状态文件");

        assert!(
            throttle_elapsed(&data_dir),
            "时钟回拨过的机器被永久关掉了更新检查"
        );
    }

    #[test]
    fn the_check_is_throttled_after_a_recent_run() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");

        // 没有状态文件 ⇒ 该查。
        assert!(throttle_elapsed(&dir), "首次启动应当检查");

        mark_checked(&dir);
        assert!(!throttle_elapsed(&dir), "刚查过不该再查");

        // 把时间戳推回到间隔之外。
        let long_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("现在晚于纪元")
            .as_secs()
            - CHECK_INTERVAL.as_secs()
            - 1;
        std::fs::write(
            dir.update_check(),
            serde_json::to_string(&CheckState {
                last_checked_unix: long_ago,
            })
            .expect("序列化"),
        )
        .expect("写入");
        assert!(throttle_elapsed(&dir), "超过间隔应当重新检查");
    }

    /// 结构化模式一律不检查——那种模式的调用方是程序。
    #[test]
    fn structured_output_never_triggers_a_check() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");
        assert!(!should_check(&dir, true));
    }

    /// 损坏的状态文件按「没查过」处理，不得让启动失败。
    #[test]
    fn a_corrupt_state_file_falls_back_to_checking() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");
        std::fs::write(dir.update_check(), "{ 这不是 JSON").expect("写入");
        assert!(throttle_elapsed(&dir));
    }
}
