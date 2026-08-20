//! `status`：节点状态快照。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{self, Request, Response};

/// 取一次状态并交给渲染层。
///
/// **无节点时如实报「停止」，不去起一个临时节点来问自己。** 这条曾经是反的：`status`
/// 走通用的「无常驻就起临时节点」路径，于是在没有节点的机器上执行它会启动一个节点、
/// 报告 `Running`、再把它关掉——用户问「节点在跑吗」，得到的答案是这个提问本身造成的。
///
/// spec 的「临时节点期间的状态查询 → `Running`」不受影响，反而更自洽：那条说的是**别的**
/// 命令（如 `send`）持有临时节点的期间，此时通道活着，本命令经它取到 `Running`。
pub async fn run(data_dir: &DataDir, json: bool) -> CliResult<()> {
    let socket = data_dir.socket();

    let payload = match ipc::request(&socket, &Request::Status).await? {
        Some(Response::Data { payload }) => payload,
        Some(Response::Error { code, message }) => return Err(CliError::from_code(code, message)),
        // 通道连不上 = 没有节点在跑。这**不是错误**，是问题的答案。
        Some(Response::Ok) | None => {
            serde_json::to_value(swarmdrop_core::network::NetworkStatus::default())
                .map_err(|err| CliError::NodeUnavailable(format!("序列化状态失败: {err}")))?
        }
    };

    crate::render::status::render(&payload, json);
    Ok(())
}
