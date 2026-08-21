//! `config`：本机设置的读与写。
//!
//! 四条动作全是 `Persisted` 档——设置就在本机的文件里，**节点停着它照样能改**，而且
//! 改动**不为此拉起一个节点**（spec: `cli-config-surface` 的「读写配置不启动节点」）。
//!
//! 有常驻节点在跑时读与写都经它，而不是本进程直接动文件。两条理由各自独立：
//!
//! - **写**：它内存里持有设备名（identify 的 `agent_version`）与接收落点。绕过它直写，
//!   磁盘与内存会分叉——用户改完名字，对端看到的还是旧的，重启之后才「莫名其妙地」生效。
//! - **读**：环境变量属于**那个进程**。`SWARMDROP_RECEIVE_DIR=… swarmdrop start` 之后，
//!   另一个 shell 里的 `config list` 读不到那个变量，而生效的是节点那份。

use crate::adapter::paths::DataDir;
use crate::cmd::ConfigAction;
use crate::exit::{CliError, CliResult};
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::ipc::Request;
use crate::runtime::settings::scalar::{self, ScalarKey, ScalarView, ScalarWritten};

pub async fn run(data_dir: &DataDir, json: bool, action: ConfigAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        ConfigAction::List => {
            crate::render::config::render_list(&views(&access).await?, json);
        }
        ConfigAction::Get { key } => {
            // **挑一条走与写入回报同一个入口**，不在这里另写一遍「找不到」的措辞。
            crate::render::config::render_one(&scalar::pick(views(&access).await?, key)?, json);
        }
        ConfigAction::Set { key, value } => write(&access, json, key, Some(value)).await?,
        ConfigAction::Unset { key } => write(&access, json, key, None).await?,
    }

    Ok(())
}

async fn views(access: &RecordAccess) -> CliResult<Vec<ScalarView>> {
    let payload = access
        .query(Request::ConfigList, |records| async move {
            to_value(
                &crate::runtime::settings::scalar::views(&records).await?,
                "配置",
            )
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析配置: {err}")))
}

/// 写一项。`value = None` 是清除。
///
/// **两条路径写的是同一件事**：有常驻节点就交给它（它顺带把内存里那份也换掉），
/// 没有就本进程直接落盘。判断收在 [`RecordAccess::query`] 里，本函数不重做一次。
async fn write(
    access: &RecordAccess,
    json: bool,
    key: ScalarKey,
    value: Option<String>,
) -> CliResult<()> {
    let for_local = value.clone();
    let payload = access
        .query(Request::ConfigSet { key, value }, |records| async move {
            to_value(
                // `live: None` —— 没有常驻节点。**不为写入拉起一个**。
                &crate::runtime::settings::scalar::apply(&records, None, key, for_local).await?,
                "写入结果",
            )
        })
        .await?;

    let written: ScalarWritten = serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析写入结果: {err}")))?;

    crate::render::config::render_written(&written, json);
    Ok(())
}
