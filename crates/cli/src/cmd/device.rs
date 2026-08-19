//! `device`：已配对设备的清点与解除。
//!
//! 两条都是 `Persisted` 档——已配对设备表就在本机的文件里，**节点停着它照样在**。
//! 需要节点的只有「在线状态」，而那一位在无节点时如实报「未知」（见
//! [`crate::runtime::devices::DeviceRow::online`]）。

use crate::adapter::paths::DataDir;
use crate::cmd::DeviceAction;
use crate::exit::{CliError, CliResult};
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::devices::{self, DeviceRow, TargetError};
use crate::runtime::ipc::Request;

pub async fn run(data_dir: &DataDir, json: bool, action: DeviceAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        DeviceAction::List => {
            crate::render::device::render_list(&fetch(&access).await?, json);
        }
        DeviceAction::Forget { device } => forget(&access, json, device).await?,
    }

    Ok(())
}

/// 取一次设备清单。
async fn fetch(access: &RecordAccess) -> CliResult<Vec<DeviceRow>> {
    let payload = access
        .query(Request::DeviceList, |records| async move {
            to_value(&devices::from_records(&records).await?, "设备列表")
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析设备列表: {err}")))
}

/// 解除配对：先定位目标，再执行。
///
/// 不给目标时列出设备让用户选——**三态判据**（见 `crate::prompt::can_ask`）：给了直接做、
/// 没给且能问就问、没给且不能问就报用法错误退出。
async fn forget(access: &RecordAccess, json: bool, target: Option<String>) -> CliResult<()> {
    let rows = fetch(access).await?;
    if rows.is_empty() {
        return Err(CliError::Usage("本机还没有已配对设备".into()));
    }

    let chosen = match target {
        Some(target) => pick(&rows, &target)?,
        None => choose(&rows).await?,
    };

    let payload = access
        .query(
            Request::DeviceForget {
                peer_id: chosen.peer_id.clone(),
            },
            {
                let peer_id = chosen.peer_id.clone();
                |records| async move {
                    to_value(
                        &devices::forget(&records, None, &peer_id).await?,
                        "解除结果",
                    )
                }
            },
        )
        .await?;

    let outcome = serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析解除结果: {err}")))?;
    crate::render::device::render_forgotten(chosen, &outcome, json);
    Ok(())
}

/// 按用户给的名称或标识定位，把失败翻成可行动的措辞。
fn pick<'a>(rows: &'a [DeviceRow], target: &str) -> CliResult<&'a DeviceRow> {
    devices::resolve_target(rows, target).map_err(|err| match err {
        TargetError::NotFound => CliError::Usage(format!(
            "没有叫「{target}」的已配对设备。用 swarmdrop device list 看看有哪些。"
        )),
        TargetError::Ambiguous(ids) => CliError::Usage(format!(
            "有多台设备叫「{target}」，请改用节点标识指定其中一台：\n  {}",
            ids.join("\n  ")
        )),
    })
}

/// 没给目标时，列出设备让用户挑一台。
async fn choose(rows: &[DeviceRow]) -> CliResult<&DeviceRow> {
    crate::prompt::choose_one(
        rows,
        "解除与哪台设备的配对？",
        "请指定要解除配对的设备（名称或节点标识）。\n\
         当前环境无法交互选择；用 swarmdrop device list 查看有哪些设备。",
        crate::render::device::menu_line,
    )
    .await
}
