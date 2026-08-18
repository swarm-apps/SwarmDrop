//! `status`：节点状态快照。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{Request, Response};
use crate::runtime::session::Session;

/// 取一次状态并交给渲染层。
///
/// 常驻节点在跑时经本地通道取它的状态；否则起一个临时节点——**临时节点期间状态就是
/// `Running`**，因为这台设备此刻确实在线（spec: cli-host「临时节点期间的状态查询」）。
pub async fn run(data_dir: &DataDir, json: bool) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;

    let payload = match session.ask(&Request::Status).await? {
        Some(Response::Data { payload }) => payload,
        Some(Response::Error { message }) => {
            session.close().await;
            return Err(CliError::NodeUnavailable(message));
        }
        Some(Response::Ok) | None => {
            // 本进程自持节点：直接取。
            let node = session
                .local()
                .ok_or_else(|| CliError::NodeUnavailable("节点不可用".into()))?;
            serde_json::to_value(node.manager.get_network_status())
                .map_err(|err| CliError::NodeUnavailable(format!("序列化状态失败: {err}")))?
        }
    };

    crate::render::status::render(&payload, json);
    session.close().await;
    Ok(())
}
