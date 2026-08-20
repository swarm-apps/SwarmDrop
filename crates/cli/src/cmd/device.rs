//! `device`：已配对设备的清点与解除。
//!
//! 两条都是 `Persisted` 档——已配对设备表就在本机的文件里，**节点停着它照样在**。
//! 需要节点的只有「在线状态」，而那一位在无节点时如实报「未知」（见
//! [`crate::runtime::devices::DeviceRow::online`]）。

use crate::adapter::paths::DataDir;
use crate::cmd::DeviceAction;
use crate::exit::{CliError, CliResult};
use crate::prompt::pick::Picker;
use crate::runtime::access::{RecordAccess, to_value};
use crate::runtime::devices::{self, DeviceRow, ForgetOutcome, TargetError};
use crate::runtime::ipc::Request;

pub async fn run(data_dir: &DataDir, json: bool, action: DeviceAction) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    match action {
        DeviceAction::List => {
            crate::render::device::render_list(&devices::list(&access).await?, json);
        }
        DeviceAction::Forget { devices } => forget(&access, json, devices).await?,
    }

    Ok(())
}

/// 解除配对：先定位目标，再逐台执行。
///
/// 三态由 [`Picker`] 承担（给了直接做、没给且能问就问、没给且不能问就报用法错误退出）。
async fn forget(access: &RecordAccess, json: bool, targets: Vec<String>) -> CliResult<()> {
    let chosen = Picker {
        fetch: async || devices::list(access).await,
        label: crate::render::device::menu_line,
        prompt: "解除与哪些设备的配对？",
        empty: "本机还没有已配对设备",
        unavailable: "请指定要解除配对的设备（名称或节点标识，可给多个）。\n\
                      当前环境无法交互选择；用 swarmdrop device list 查看有哪些设备。",
    }
    .many(&targets, locate)
    .await?;

    // **先去重**：`forget phone phone`、或一个名称与一个节点标识指向同一台时，
    // 那台会被列两遍（「已解除与 2 台设备的配对」下面是同一个名字）。
    let chosen = super::dedup_by_id(chosen, |row| row.peer_id.clone());

    // **一次调用解除全部选中的**，不要在这里循环：每一次解除在那一侧都是一轮
    // 「读已配对设备表 → 改 → 原子写回」，逐台发等于把同一个文件读写 N 遍；
    // 「最后还剩几台」也该由那一侧给出，而不是命令面自己从 N 个返回值里拼。
    let peer_ids: Vec<String> = chosen.iter().map(|row| row.peer_id.clone()).collect();
    let payload = access
        .query(
            Request::DeviceForget {
                peer_ids: peer_ids.clone(),
            },
            |records| async move {
                to_value(
                    &devices::forget(&records, None, &peer_ids).await?,
                    "解除结果",
                )
            },
        )
        .await?;

    let outcome: ForgetOutcome = serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析解除结果: {err}")))?;

    crate::render::device::render_forgotten(&chosen, outcome.remaining, json);
    Ok(())
}

/// 按用户给的名称或标识定位，把失败翻成可行动的措辞。
///
/// 返回**拥有的**行：[`Picker`] 要把结果带出候选集的作用域（那是它内部取的临时值）。
pub(super) fn locate(rows: &[DeviceRow], target: &str) -> CliResult<DeviceRow> {
    devices::resolve_target(rows, target)
        .cloned()
        .map_err(|err| match err {
            TargetError::NotFound => CliError::Usage(format!(
                "没有叫「{target}」的已配对设备。用 swarmdrop device list 看看有哪些。"
            )),
            TargetError::Ambiguous(ids) => CliError::Usage(format!(
                "有多台设备叫「{target}」，请改用节点标识指定其中一台：\n  {}",
                ids.join("\n  ")
            )),
        })
}
