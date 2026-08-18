//! `start`：启动常驻节点。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::boot::{RunningNode, boot};
use crate::runtime::ipc::{IpcServer, Request, RequestHandler, Response};
use crate::runtime::single::{Acquisition, acquire};

/// 后台启动时，等待节点就绪的上限。
///
/// 超时不代表失败——子进程可能只是起得慢。所以超时只影响「要不要打印就绪信息」，
/// 不影响退出码：把一次慢启动报成失败，会诱导用户重复执行，反而撞上单实例拒绝。
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 收到 `stop` 后，关停前留给在途应答写回的窗口。
///
/// 应答在独立任务里写，而主循环一旦跳出就会关停节点、进程随即退出——未写完的应答会随
/// 运行时一起消失，客户端只看到连接断开。200ms 足够写一行 JSON 到本地套接字，
/// 用户也感知不到。**不用「等那个任务结束」代替它**：同一批在途请求里可能有一个正在
/// 传几分钟的 `send`，等它等于让 `stop` 失去意义。
const DRAIN_WINDOW: std::time::Duration = std::time::Duration::from_millis(200);

pub async fn run(data_dir: &DataDir, json: bool, detach: bool) -> CliResult<()> {
    if detach {
        return spawn_detached(data_dir, json).await;
    }

    let Acquisition::Owner(lock) = acquire(data_dir).await? else {
        return Err(CliError::NodeUnavailable(
            "节点已在运行；如需停止请执行 swarmdrop stop".into(),
        ));
    };

    let node = Arc::new(boot(data_dir, json).await?);
    let server = IpcServer::bind(lock.socket_path())?;

    // 被动接收：节点在线即自动接受入站传输（spec: cli-host「接收不是一条命令」）。
    let save_dir = crate::adapter::receive::resolve()?;
    crate::runtime::receive::spawn_auto_accept(node.clone(), save_dir.clone());

    crate::render::status::render_started(&node.node_id, json);
    if !json {
        println!("接收落点  {}", save_dir.display());
    }

    let stopping = Arc::new(AtomicBool::new(false));
    let handler: Arc<dyn RequestHandler> = Arc::new(NodeHandler {
        node: node.clone(),
        stopping: stopping.clone(),
    });

    loop {
        tokio::select! {
            // 前台形态的正常终止路径：服务管理器与托管本进程的外部程序都靠它。
            _ = tokio::signal::ctrl_c() => break,
            result = server.accept_one(handler.clone()) => {
                result?;
                if stopping.load(Ordering::SeqCst) {
                    tokio::time::sleep(DRAIN_WINDOW).await;
                    break;
                }
            }
        }
    }

    node.manager.shutdown().await;
    drop(lock); // 显式：持有权必须在节点关停之后释放
    Ok(())
}

/// 常驻节点的请求处理器。
struct NodeHandler {
    node: Arc<RunningNode>,
    stopping: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl RequestHandler for NodeHandler {
    async fn handle(&self, req: Request) -> Response {
        match req {
            Request::Status => json_or_error(
                serde_json::to_value(self.node.manager.get_network_status()),
                "状态",
            ),
            Request::Devices => json_or_error(
                serde_json::to_value(self.node.manager.devices().get_devices(Default::default())),
                "设备列表",
            ),
            Request::Send { paths, to } => {
                let paths: Vec<std::path::PathBuf> =
                    paths.into_iter().map(std::path::PathBuf::from).collect();
                match crate::runtime::transfer::send_files(&self.node, &paths, &to, false).await {
                    Ok(outcome) => Response::Data {
                        payload: serde_json::json!({
                            "sessionId": outcome.session_id.to_string(),
                            "fileCount": outcome.file_count,
                            "totalBytes": outcome.total_bytes,
                        }),
                    },
                    Err(err) => Response::Error {
                        message: err.to_string(),
                    },
                }
            }
            Request::Stop => {
                // 只置标志，不在这里关停：关停要发生在**应答写回之后**，
                // 否则客户端读到的是一条断开的连接而不是一个成功的响应。
                self.stopping.store(true, Ordering::SeqCst);
                Response::Ok
            }
        }
    }
}

fn json_or_error(value: serde_json::Result<serde_json::Value>, what: &str) -> Response {
    match value {
        Ok(payload) => Response::Data { payload },
        Err(err) => Response::Error {
            message: format!("序列化{what}失败: {err}"),
        },
    }
}

/// 以后台方式启动：重新拉起自己（不带后台标志），父进程在节点就绪后返回。
///
/// **不做 daemon 化的那套仪式**（fork/setsid/重定向）：那些在 Windows 上完全不同，
/// 而真正需要脱离终端的场景（开机自启）本就该交给服务管理器。这里只解决交互式用户的
/// 便利：拉起一个子进程，自己退出。
async fn spawn_detached(data_dir: &DataDir, json: bool) -> CliResult<()> {
    if crate::runtime::ipc::is_alive(&data_dir.socket()).await {
        return Err(CliError::NodeUnavailable(
            "节点已在运行；如需停止请执行 swarmdrop stop".into(),
        ));
    }

    let exe = std::env::current_exe()
        .map_err(|err| CliError::NodeUnavailable(format!("无法定位自身可执行文件: {err}")))?;

    let mut cmd = std::process::Command::new(exe);
    cmd.arg("start")
        .arg("--data-dir")
        .arg(data_dir.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if json {
        cmd.arg("--json");
    }

    cmd.spawn()
        .map_err(|err| CliError::NodeUnavailable(format!("后台启动失败: {err}")))?;

    // 轮询通道，等它把服务端建起来。
    let deadline = std::time::Instant::now() + READY_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if crate::runtime::ipc::is_alive(&data_dir.socket()).await {
            crate::render::status::render_detached(true, json);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    crate::render::status::render_detached(false, json);
    Ok(())
}
