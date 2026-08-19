//! `send`：向一台已配对设备发送文件或目录。

use std::path::PathBuf;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::access::NodeAccess;
use crate::runtime::ipc::{Request, Response};
use crate::runtime::transfer::send_files;

pub async fn run(data_dir: &DataDir, json: bool, files: Vec<PathBuf>, to: String) -> CliResult<()> {
    // 路径要在这里转成绝对：常驻节点可能在别的工作目录下启动，相对路径到了那边就指向别处。
    let absolute: Vec<PathBuf> = files
        .into_iter()
        .map(|p| {
            std::fs::canonicalize(&p)
                .map_err(|err| CliError::Usage(format!("无法解析路径 {}: {err}", p.display())))
        })
        .collect::<CliResult<_>>()?;

    let access = NodeAccess::open(data_dir, json).await?;

    let request = Request::Send {
        paths: absolute
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        to: to.clone(),
    };

    let result = match access.ask(&request).await? {
        Some(Response::Data { payload }) => {
            crate::render::send::render_from_json(&payload, json);
            Ok(())
        }
        // 尊重服务端给的分类：它区分得出「对端不可达」与「传输中断」，
        // 而在这里一律按后者会让脚本的重试策略选错。
        Some(Response::Error { code, message }) => Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => {
            let node = access.require_local()?;
            match send_files(node, &absolute, &to, !json).await {
                Ok(outcome) => {
                    crate::render::send::render(&outcome, json);
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
    };

    access.close().await;
    result
}
