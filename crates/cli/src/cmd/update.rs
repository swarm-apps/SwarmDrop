//! `update`：更新到最新版本。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::update;

pub async fn run(data_dir: &DataDir, json: bool, check: bool) -> CliResult<()> {
    let channel = update::detect_channel();
    let current = update::current_version();

    // 渠道不归本程序管 ⇒ 转交，**并以成功退出**。用户没有做错任何事，只是用 brew 装的；
    // 报成失败会让「你该用另一条命令」在脚本里等同于「更新出错了」。
    if !channel.is_self_updatable() {
        crate::render::update::render_handoff(current, channel, json);
        return Ok(());
    }

    if check {
        return match update::query_new_version().await? {
            Some(latest) => {
                crate::render::update::render_available(current, &latest, json);
                Ok(())
            }
            None => {
                crate::render::update::render_up_to_date(current, json);
                Ok(())
            }
        };
    }

    ensure_node_stopped(data_dir).await?;

    match update::install_latest(json).await? {
        Some(done) => crate::render::update::render_installed(&done, json),
        None => crate::render::update::render_up_to_date(current, json),
    }
    Ok(())
}

/// 更新会覆盖本程序的可执行文件，所以常驻节点必须先停。
///
/// **不是保守起见，两个平台各有一条硬理由**：
///
/// - **Windows 根本不让覆盖**正在运行的可执行文件（文件被映射时拒绝写入）。installer
///   会在把文件搬到位那一步失败，而那时归档已经下载、旧文件可能已被改名——半途失败
///   比不做更糟。
/// - **Unix 让覆盖**（替换的是目录项，运行中的进程仍持有旧 inode），于是失败形态反而
///   更隐蔽：命令报「已更新」，而那个还在跑的节点仍是旧代码，直到某天有人重启它。
///   用户据此以为新版本的修复已经生效。
///
/// 用 [`CliError::UpdateFailed`] 而不是 `NodeUnavailable`：节点没有不可用，恰恰是**太可用**。
async fn ensure_node_stopped(data_dir: &DataDir) -> CliResult<()> {
    if crate::runtime::ipc::is_alive(&data_dir.socket()).await {
        return Err(CliError::UpdateFailed(
            "节点正在运行；请先执行 swarmdrop stop 再更新".into(),
        ));
    }
    Ok(())
}

/// `start` 顺带做的一次版本检查。
///
/// **整条链路都必须是静默失败**：用户执行的是 `start`，检查更新只是搭便车。网络不通、
/// GitHub 限速、receipt 损坏——任何一条都不该影响节点，也不该往屏幕上写错误。
/// 所以这里吞掉所有 `Err`，只在**确实有新版本**时打一行提示。
///
/// 调用它之前 [`update::should_check`] 已经决定了「此刻该不该查」；这里只管查。
pub async fn hint_if_outdated(data_dir: &DataDir) {
    let channel = update::detect_channel();
    // 非 Managed 渠道**查不到版本**——版本来源写在 install receipt 里，而那正是它们没有的
    // 东西。所以这里直接返回，而不是去做一次注定失败的查询。包管理器自己会提示升级。
    //
    // **这条路径刻意不记时间戳。** 记了的话，用户从 brew 改用安装脚本之后要等满一个节流
    // 间隔才会被提示；而这次判定不发任何网络请求（读一次 receipt + 比一次路径），
    // 每次启动重做一遍的代价可以忽略。
    if !channel.is_self_updatable() {
        return;
    }

    // 无论查成没查成都记一次时间戳：失败时若不记，下一次启动会立刻重试——而失败最常见
    // 的原因恰恰是限速与断网，重试只会更快地撞上同一堵墙。
    update::mark_checked(data_dir);

    if let Ok(Some(latest)) = update::query_new_version().await {
        crate::render::update::hint_new_version(update::current_version(), &latest);
    }
}
