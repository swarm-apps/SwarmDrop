//! `devices`：列出已配对设备。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{Request, Response};
use crate::runtime::session::Session;

pub async fn run(data_dir: &DataDir, json: bool) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;

    let payload = match session.ask(&Request::Devices).await? {
        Some(Response::Data { payload }) => payload,
        Some(Response::Error { message }) => {
            session.close().await;
            return Err(CliError::NodeUnavailable(message));
        }
        Some(Response::Ok) | None => {
            let node = session.require_local()?;
            serde_json::to_value(crate::runtime::pairing::paired_devices(node))
                .map_err(|err| CliError::NodeUnavailable(format!("序列化设备列表失败: {err}")))?
        }
    };

    crate::render::devices::render(&payload, json);
    session.close().await;
    Ok(())
}
