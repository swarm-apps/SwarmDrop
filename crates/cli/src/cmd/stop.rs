//! `stop`：停止正在运行的节点。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{Request, Response, request};

/// 请求关停。
///
/// **无节点在运行时以成功退出**：`stop` 表达的是「我要它停下」，而它本来就停着，
/// 这个意图已经满足。报错会让「先 stop 再 start」这类脚本无谓地失败。
pub async fn run(data_dir: &DataDir, json: bool) -> CliResult<()> {
    match request(&data_dir.socket(), &Request::Stop).await? {
        None => {
            crate::render::status::render_stopped(false, json);
            Ok(())
        }
        Some(Response::Ok) | Some(Response::Data { .. }) => {
            crate::render::status::render_stopped(true, json);
            Ok(())
        }
        Some(Response::Error { code, message }) => Err(CliError::from_code(code, message)),
    }
}
