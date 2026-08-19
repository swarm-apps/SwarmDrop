//! 已配对设备的清点、定位与解除。
//!
//! **本层不含面向用户的文案**（见 [`super`] 的约束）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use swarmdrop_core::device_manager::DeviceFilter;
use swarmdrop_host::device::{ConnectionType, Device, DeviceStatus, PairedDeviceInfo};

use crate::adapter::events::{CliEventBus, QuietRenderer};
use crate::exit::{CliError, CliResult};

use super::access::Records;
use super::boot::{CliNetManager, RunningNode};

/// 设备清单的一条。
///
/// 两条取数路径（活节点 / 无节点直读记录）**共用这一个形状**，差别只在 [`Self::online`]
/// 能不能填出来。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub peer_id: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    /// 在线与否。
    ///
    /// **`None` 是「未知」，不是「离线」。** 无节点时本机没有做过任何探测，把未知说成
    /// 离线是一个凭空的断言；而用户看到「离线」会去排查网络，看到「未知」才会想到
    /// 「哦，节点没开」。
    ///
    /// 也**不为了填上它去起一个临时节点**：节点刚起来时 presence 探测尚未完成，那个值
    /// 同样不可信——花几秒换一个不准的答案，两头不讨好。
    pub online: Option<bool>,
    pub connection: Option<ConnectionType>,
    /// 配对时刻（Unix 秒）。活节点路径拿不到它时为 `None`。
    pub paired_at: Option<i64>,
}

impl From<Device> for DeviceRow {
    fn from(device: Device) -> Self {
        Self {
            peer_id: device.peer_id.to_string(),
            name: device.os_info.display_name(),
            os: device.os_info.os.clone(),
            arch: device.os_info.arch.clone(),
            online: Some(matches!(device.status, DeviceStatus::Online)),
            connection: device.connection,
            paired_at: None,
        }
    }
}

impl From<PairedDeviceInfo> for DeviceRow {
    fn from(info: PairedDeviceInfo) -> Self {
        Self {
            peer_id: info.peer_id.to_string(),
            name: info.os_info.display_name(),
            os: info.os_info.os.clone(),
            arch: info.os_info.arch.clone(),
            // 无节点 ⇒ 未探测 ⇒ 未知。见 `online` 的文档。
            online: None,
            connection: None,
            paired_at: Some(info.paired_at),
        }
    }
}

/// 活节点上的已配对设备。
///
/// **过滤器必须是 `Paired` 而不是默认的 `All`**：后者返回的是「本次运行发现的 peer」，
/// 与「已配对设备」是两个集合，两个方向都会错——刚配完对方却因为还没被发现而不出现，
/// 同时局域网里路过的陌生设备反而列了出来。用户唯一能确认「配上没有」的手段就是这条命令，
/// 它答错等于配对功能不存在。
pub fn from_node(node: &RunningNode) -> Vec<DeviceRow> {
    node.manager
        .devices()
        .get_devices(DeviceFilter::Paired)
        .into_iter()
        .map(DeviceRow::from)
        .collect()
}

/// 无节点时的已配对设备：直读本机记录。
pub async fn from_records(records: &Records) -> CliResult<Vec<DeviceRow>> {
    Ok(records
        .paired_devices()
        .await?
        .into_iter()
        .map(DeviceRow::from)
        .collect())
}

/// 用户给的目标定位不到设备的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    NotFound,
    /// 同名设备不止一台。附全部候选的标识——同名是常态（两台都叫「MacBook Pro」），
    /// 而解除配对不可逆，不能替用户挑一台。
    Ambiguous(Vec<String>),
}

/// 按名称或节点标识定位一台已配对设备。
///
/// 匹配顺序是**标识优先**：标识唯一且用户复制它时通常正是为了避开重名。
/// 名称匹配大小写不敏感（用户不会记得设备名的大小写）。
pub fn resolve_target<'a>(
    rows: &'a [DeviceRow],
    target: &str,
) -> Result<&'a DeviceRow, TargetError> {
    if let Some(row) = rows.iter().find(|row| row.peer_id == target) {
        return Ok(row);
    }

    let needle = target.to_lowercase();
    let matched: Vec<&DeviceRow> = rows
        .iter()
        .filter(|row| row.name.to_lowercase() == needle)
        .collect();

    match matched.as_slice() {
        [] => Err(TargetError::NotFound),
        [only] => Ok(only),
        many => Err(TargetError::Ambiguous(
            many.iter().map(|row| row.peer_id.clone()).collect(),
        )),
    }
}

/// 解除配对的结局。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetOutcome {
    pub peer_id: String,
    /// 解除后本机还记着几台。
    pub remaining: usize,
}

/// 解除与某台设备的配对。
///
/// **单方面操作**：它移除的是本机对该设备的记录，对端是否仍记着本机不在本命令的控制
/// 范围内（这也是命令用 `forget` 而非 `unpair` 的理由）。
///
/// `node` 为 `Some` 时核心会额外停掉对该设备的在线状态维持；为 `None` 时只改本机记录——
/// 分派逻辑在核心的 `unpair` 里，本函数只负责凑齐端口。
pub async fn forget(
    records: &Records,
    node: Option<&RunningNode>,
    peer_id: &str,
) -> CliResult<ForgetOutcome> {
    let parsed = peer_id
        .parse::<swarmdrop_net::NodeId>()
        .map_err(|err| CliError::Usage(format!("不是合法的节点标识 {peer_id}: {err}")))?;

    let store = records.device_store();
    // 无节点时没有任何东西在监听事件，静默渲染即可——事件是给常驻进程的运行叙述用的，
    // 而本命令的结果由它自己的返回值表达。
    let events = CliEventBus::new(Arc::new(QuietRenderer));

    let remaining = swarmdrop_core::paired_devices::unpair(
        &parsed,
        &*store,
        &events,
        // ⚠️ 泛型参数在 `None` 分支推不出来，必须显式标注。别名收在 `boot` 里，
        // 免得每个调用点各写一遍那串类型。
        node.map(|node| &node.manager) as Option<&CliNetManager>,
    )
    .await
    .map_err(|err| CliError::NodeUnavailable(format!("解除配对失败: {err}")))?;

    Ok(ForgetOutcome {
        peer_id: parsed.to_string(),
        remaining: remaining.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(peer_id: &str, name: &str) -> DeviceRow {
        DeviceRow {
            peer_id: peer_id.into(),
            name: name.into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            online: None,
            connection: None,
            paired_at: Some(1_800_000_000),
        }
    }

    #[test]
    fn resolves_by_exact_peer_id() {
        let rows = [row("12D3KooWA", "书房"), row("12D3KooWB", "手机")];
        assert_eq!(resolve_target(&rows, "12D3KooWB").unwrap().name, "手机");
    }

    #[test]
    fn resolves_by_name_case_insensitively() {
        let rows = [row("12D3KooWA", "Study Mac")];
        assert_eq!(
            resolve_target(&rows, "study mac").unwrap().peer_id,
            "12D3KooWA"
        );
    }

    /// **重名必须拒绝并给出候选**：同名是常态（两台都叫「MacBook Pro」），
    /// 而解除配对不可逆。
    #[test]
    fn duplicate_names_are_refused_with_candidates() {
        let rows = [
            row("12D3KooWA", "MacBook Pro"),
            row("12D3KooWB", "MacBook Pro"),
        ];
        match resolve_target(&rows, "MacBook Pro") {
            Err(TargetError::Ambiguous(ids)) => assert_eq!(ids.len(), 2),
            other => panic!("重名必须拒绝，实际: {other:?}"),
        }
    }

    /// 标识优先于名称——用户复制标识通常正是为了避开重名。
    #[test]
    fn peer_id_wins_over_name() {
        let rows = [row("12D3KooWA", "手机"), row("手机", "别的")];
        assert_eq!(resolve_target(&rows, "手机").unwrap().peer_id, "手机");
    }

    /// 无节点路径的在线状态必须是**未知**，不得是「离线」。
    ///
    /// 这条看守的是一个凭空断言：本机根本没做过探测，说「离线」会让用户去排查网络，
    /// 而真实原因是节点没开。
    #[test]
    fn records_path_reports_unknown_not_offline() {
        use swarmdrop_host::device::OsInfo;

        let info = PairedDeviceInfo::new(
            swarmdrop_net::SecretKey::generate().node_id(),
            OsInfo::default(),
            1_800_000_000,
        );
        let row = DeviceRow::from(info);
        assert_eq!(row.online, None, "无节点时在线状态必须是未知");
    }

    #[test]
    fn row_round_trips() {
        let value = serde_json::to_value(row("12D3KooWA", "书房")).expect("编码");
        let back: DeviceRow = serde_json::from_value(value).expect("往返");
        assert_eq!(back.peer_id, "12D3KooWA");
        assert_eq!(back.online, None);
    }
}
