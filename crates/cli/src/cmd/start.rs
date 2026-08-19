//! `start`：启动常驻节点。

use std::sync::Arc;

use tokio::sync::Notify;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::boot::{RunningNode, boot};
use crate::runtime::ipc::{IpcServer, Request, RequestHandler, Response};
use crate::runtime::pairing::ConfirmationDesk;
use crate::runtime::single::{Acquisition, acquire};

/// 后台启动时，等待节点就绪的上限。
///
/// 超时不代表失败——子进程可能只是起得慢。所以超时只影响「要不要打印就绪信息」，
/// 不影响退出码：把一次慢启动报成失败，会诱导用户重复执行，反而撞上单实例拒绝。
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 收到 `stop` 信号后，关停前留给在途应答写回的窗口。
///
/// 应答在独立任务里写，而主循环一旦跳出就会关停节点、进程随即退出——未写完的应答会随
/// 运行时一起消失，客户端只看到连接断开。200ms 足够写一行 JSON 到本地套接字，
/// 用户也感知不到。**不用「等那个任务结束」代替它**：同一批在途请求里可能有一个正在
/// 传几分钟的 `send`，等它等于让 `stop` 失去意义。
const DRAIN_WINDOW: std::time::Duration = std::time::Duration::from_millis(200);

pub async fn run(data_dir: &DataDir, json: bool, detach: bool, auto_accept: bool) -> CliResult<()> {
    if detach {
        return spawn_detached(data_dir, json, auto_accept).await;
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

    // 入站配对**不自动接受**：请求转交给正在等待的 `swarmdrop pair` 客户端由人确认，
    // 没有人在等就拒绝。判据与理由见 `runtime::pairing`。
    let desk = Arc::new(ConfirmationDesk::default());
    crate::runtime::pairing::spawn_desk_service(node.clone(), desk.clone(), auto_accept);

    crate::render::status::render_started(&node.node_id, json);
    if !json {
        println!("接收落点  {}", save_dir.display());
        // 把配对的门开在哪儿说清楚。不说的话，用户在别处扫码被拒时无从判断
        // 是网络不通还是被本机挡了。
        if auto_accept {
            println!("配对策略  自动接受邀请配对（--auto-accept）");
        } else {
            println!("配对策略  需要确认——在本机执行 swarmdrop pair 期间才接受配对");
        }
    }

    let shutdown = Arc::new(Notify::new());
    let handler: Arc<dyn RequestHandler> = Arc::new(NodeHandler {
        node: node.clone(),
        desk,
        shutdown: shutdown.clone(),
    });

    serve_until_stopped(&server, handler, &shutdown).await?;

    node.manager.shutdown().await;
    drop(lock); // 显式：持有权必须在节点关停之后释放
    Ok(())
}

/// 接受连接，直到收到停止信号或 Ctrl-C。
///
/// **停止必须由处理器主动唤醒本循环，不能靠「accept 返回后读一个标志位」**：
/// [`IpcServer::accept_one`] 在接受连接后立即返回，请求是在独立任务里处理的，
/// 所以标志位在那一刻**必然**还是旧值——循环转头又阻塞在下一次 accept 上，而 `stop`
/// 客户端一问一答就断开了，不会再有连接把它唤醒。表现是 `swarmdrop stop` 打印
/// 「节点已停止」、客户端正常退出，**前台进程却一直挂着**，直到有人碰巧再执行一条命令。
/// 由 [`tests::stop_ends_serve_loop_without_further_connections`] 看守。
async fn serve_until_stopped(
    server: &IpcServer,
    handler: Arc<dyn RequestHandler>,
    shutdown: &Notify,
) -> CliResult<()> {
    loop {
        tokio::select! {
            // 前台形态的正常终止路径：服务管理器与托管本进程的外部程序都靠它。
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = shutdown.notified() => {
                tokio::time::sleep(DRAIN_WINDOW).await;
                return Ok(());
            }
            result = server.accept_one(handler.clone()) => result?,
        }
    }
}

/// 常驻节点的请求处理器。
struct NodeHandler {
    node: Arc<RunningNode>,
    desk: Arc<ConfirmationDesk>,
    shutdown: Arc<Notify>,
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
                serde_json::to_value(crate::runtime::pairing::paired_devices(&self.node)),
                "设备列表",
            ),
            Request::PairGenerate => {
                match self
                    .node
                    .manager
                    .pairing()
                    .encode_invite(
                        &self.node.secret_key,
                        swarmdrop_invite::TransportPolicy::Auto,
                    )
                    .await
                {
                    Ok(invite) => Response::Data {
                        payload: serde_json::json!({ "invite": invite }),
                    },
                    Err(err) => Response::Error {
                        message: format!("生成邀请失败: {err}"),
                    },
                }
            }
            Request::PairWaitNext => match self.desk.take().await {
                Some(request) => json_or_error(serde_json::to_value(request), "待确认的配对请求"),
                // 本轮没有请求。客户端据此立即再问一次——**轮询不能停**，
                // 停下来常驻节点就认为没有人在等配对了。
                None => Response::Ok,
            },
            Request::PairRespond { pending_id, accept } => {
                // 接受与拒绝**同一个形状**（`PairOutcome`），与 `PairAccept` 那条路径一致。
                // 给拒绝单开一个无负载的 `Ok`，等于让同一件事在通道上有两种表达，
                // 而客户端那侧只会有一条被测到。
                match crate::runtime::pairing::respond(&self.node, pending_id, accept).await {
                    Some(outcome) => json_or_error(serde_json::to_value(outcome), "配对结果"),
                    None => Response::Error {
                        message: "这条配对请求已经失效（对端已断开或等待超时）".into(),
                    },
                }
            }
            Request::PairAccept { invite } => {
                // **必须把 `PairingResponse` 原样传回**：`Ok(..)` 只说明「这次问答走完了」，
                // 对端完全可能答的是「拒绝」。丢掉它等于把婉拒渲染成配对成功——
                // 用户要到之后 `send` 找不到设备时才发现，那时已经无从归因。
                match self.node.manager.pairing().pair_with_invite(&invite).await {
                    Ok((response, commit)) => json_or_error(
                        serde_json::to_value(crate::runtime::pairing::PairOutcome::new(
                            &response, &commit,
                        )),
                        "配对结果",
                    ),
                    Err(err) => Response::Error {
                        message: format!("配对失败: {err}"),
                    },
                }
            }
            Request::InboxList => {
                match self
                    .node
                    .manager
                    .transfer_arc()
                    .store()
                    .list_inbox_items(false)
                    .await
                {
                    Ok(items) => json_or_error(serde_json::to_value(items), "收件箱"),
                    Err(err) => Response::Error {
                        message: format!("读取收件箱失败: {err}"),
                    },
                }
            }
            Request::InboxGet { id } => {
                let Ok(uuid) = uuid::Uuid::parse_str(&id) else {
                    return Response::Error {
                        message: format!("不是合法的条目标识: {id}"),
                    };
                };
                match self
                    .node
                    .manager
                    .transfer_arc()
                    .store()
                    .get_inbox_item_detail(uuid)
                    .await
                {
                    Ok(Some(detail)) => json_or_error(serde_json::to_value(detail), "条目详情"),
                    Ok(None) => Response::Error {
                        message: format!("收件箱里没有条目 {id}"),
                    },
                    Err(err) => Response::Error {
                        message: format!("读取条目失败: {err}"),
                    },
                }
            }
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
                // 只发信号、不在这里关停：关停要发生在**应答写回之后**，否则客户端读到的
                // 是一条断开的连接而不是一个成功的响应。应答由 `accept_one` 那个独立任务
                // 在本方法返回之后才写，所以服务循环那侧留了一个排水窗口。
                self.shutdown.notify_one();
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
async fn spawn_detached(data_dir: &DataDir, json: bool, auto_accept: bool) -> CliResult<()> {
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
    if auto_accept {
        cmd.arg("--auto-accept");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ipc::request;

    /// **一条 `stop` 之后没有任何后续连接，服务循环也必须自己结束。**
    ///
    /// 这条看守的是一个只有手工操作才会撞见的活性缺陷：此前循环是「accept 返回后读一个
    /// 标志位」，而请求在独立任务里处理，那一刻标志位必然还没被置上——于是 `stop` 的
    /// 客户端正常收到应答并退出，前台进程却一直挂着，直到有人碰巧再执行一条命令
    /// 才把它唤醒。任何自动检查都发现不了它，因为每一次问答本身都是成功的。
    #[tokio::test]
    async fn stop_ends_serve_loop_without_further_connections() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("t.sock");
        let server = IpcServer::bind(&path).expect("监听");
        let shutdown = Arc::new(Notify::new());

        struct StopHandler {
            shutdown: Arc<Notify>,
        }
        #[async_trait::async_trait]
        impl RequestHandler for StopHandler {
            async fn handle(&self, _req: Request) -> Response {
                self.shutdown.notify_one();
                Response::Ok
            }
        }

        let handler: Arc<dyn RequestHandler> = Arc::new(StopHandler {
            shutdown: shutdown.clone(),
        });
        let serving = {
            let shutdown = shutdown.clone();
            tokio::spawn(async move { serve_until_stopped(&server, handler, &shutdown).await })
        };

        let response = request(&path, &Request::Stop).await.expect("发送 stop");
        assert!(matches!(response, Some(Response::Ok)));

        // 关键：**不再建立任何连接**。挂住即失败。
        tokio::time::timeout(std::time::Duration::from_secs(5), serving)
            .await
            .expect("服务循环在 stop 之后没有退出")
            .expect("任务 panic")
            .expect("循环返回错误");
    }
}
