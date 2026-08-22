//! 以**常驻形态**服务一个自持节点。
//!
//! ## 它为什么不属于 `cmd/start.rs`
//!
//! 「起一个节点」与「把它摆成常驻形态」是两件事：前者是 [`super::boot`]，后者是绑本地
//! 通道、起被动接收、开配对确认台，再服务到有人叫停。此前后者整段写在 `cmd/start.rs` 的
//! `run()` 里，于是它**隐含了「只有 `start` 会长驻」这个假设**——而 `crates/cli` 的分层
//! 约定明写着 `runtime/` 不得假设调用方是一次性命令（`main.rs` 的模块文档、
//! `standalone-cli-host/design.md` 的 D11）。
//!
//! 第二个长驻消费者出现时那个假设当场显形：`swarmdrop mcp` 自持节点时会持锁数小时，
//! 而它经 [`super::access::NodeAccess`] 拿到的只是「一个起来了的节点」——
//! **不建通道**（于是同机的每一条命令都撞「另一个 swarmdrop 进程正在启动，等待超时」，
//! 连 `swarmdrop watch` 都以为本机没有节点）、**不起被动接收**（于是那个节点在线、可达、
//! 却一个文件都收不下）。三种失败没有一种会报错。
//!
//! 所以装配收在这里，两条命令各自在它之上加自己那一层：`start` 加渲染与后台拉起，
//! `mcp` 加 stdio 上的协议 server。

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Notify;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::boot::RunningNode;
use crate::runtime::ipc::{FrameSink, IpcServer, Request, RequestHandler, Response};
use crate::runtime::pairing::ConfirmationDesk;
use crate::runtime::receive::ReceiveDir;
use crate::runtime::settings::SettingsStore;

/// 一个已经摆成常驻形态的自持节点。
///
/// **只有自持节点才需要它**：复用别人的常驻节点时，通道与被动接收都在那个进程里。
pub struct Daemon {
    server: IpcServer,
    handler: Arc<dyn RequestHandler>,
    shutdown: Arc<Notify>,
    /// 接收落点。`start` 要把它打印出来——用户没有别的地方看得到它。
    ///
    /// **是句柄不是值**：`swarmdrop config set receive-dir` 要在不重启节点的前提下换掉它。
    receive: Arc<ReceiveDir>,
}

impl Daemon {
    /// 把一个刚起来的节点摆成常驻形态。
    ///
    /// 三件事一件都不能少，理由见模块文档。`auto_accept` 只影响**配对**：为真时第一台
    /// 出示有效邀请的设备直接放行，为假时请求交给正在等待的 `invite create` 客户端由人
    /// 确认，没有人在等就拒绝（判据见 [`crate::runtime::pairing`]）。
    ///
    /// **被动接收不受它影响**：已配对设备发来的文件一律收下（spec: cli-host
    /// 「接收不是一条命令」）。
    pub fn start(data_dir: &DataDir, node: Arc<RunningNode>, auto_accept: bool) -> CliResult<Self> {
        let server = IpcServer::bind(&data_dir.socket())?;

        let settings = SettingsStore::new(data_dir.settings()).read()?;
        let receive = Arc::new(ReceiveDir::resolve(&settings)?);
        crate::runtime::receive::spawn_auto_accept(node.clone(), receive.clone());

        let desk = Arc::new(ConfirmationDesk::default());
        crate::runtime::pairing::spawn_desk_service(node.clone(), desk.clone(), auto_accept);

        let shutdown = Arc::new(Notify::new());
        Ok(Self {
            server,
            shutdown: shutdown.clone(),
            handler: Arc::new(NodeHandler {
                shutdown,
                // 必须在这里起：`transfer watch` 与订阅面的基线问的都是常驻节点，
                // 而只有常驻节点看得见传输事件。见 `runtime::progress`——库里的发送
                // 进度在传输期间是陈旧的。
                progress: crate::runtime::progress::ProgressCache::spawn(&node.events),
                node,
                desk,
                data_dir: data_dir.clone(),
                receive: receive.clone(),
            }),
            receive,
        })
    }

    /// 此刻的接收落点。`start` 打印它——用户没有别的地方看得到。
    pub fn save_dir(&self) -> PathBuf {
        self.receive.current()
    }

    /// 服务到收到 `stop` 或中断信号。
    pub async fn serve(&self) -> CliResult<()> {
        serve_until_stopped(&self.server, self.handler.clone(), &self.shutdown).await
    }
}

/// 收到 `stop` 信号后，关停前留给在途应答写回的窗口。
///
/// 应答在独立任务里写，而主循环一旦跳出就会关停节点、进程随即退出——未写完的应答会随
/// 运行时一起消失，客户端只看到连接断开。200ms 足够写一行 JSON 到本地套接字，
/// 用户也感知不到。**不用「等那个任务结束」代替它**：同一批在途请求里可能有一个正在
/// 传几分钟的 `send`，等它等于让 `stop` 失去意义。
const DRAIN_WINDOW: std::time::Duration = std::time::Duration::from_millis(200);

/// 接受连接，直到收到停止信号或 Ctrl-C。
///
/// **停止必须由处理器主动唤醒本循环，不能靠「accept 返回后读一个标志位」**：
/// [`IpcServer::accept_one`] 在接受连接后立即返回，请求是在独立任务里处理的，
/// 所以标志位在那一刻**必然**还是旧值——循环转头又阻塞在下一次 accept 上，而 `stop`
/// 客户端一问一答就断开了，不会再有连接把它唤醒。表现是 `swarmdrop stop` 打印
/// 「节点已停止」、客户端正常退出，**前台进程却一直挂着**，直到有人碰巧再执行一条命令。
/// 由 `tests::stop_ends_serve_loop_without_further_connections` 看守。
async fn serve_until_stopped(
    server: &IpcServer,
    handler: Arc<dyn RequestHandler>,
    shutdown: &Notify,
) -> CliResult<()> {
    // **监听器建在循环外**：写在 `select!` 里就是每转一圈丢弃并重建一次，
    // 落在那个缝隙里的 `SIGTERM` 会被静默吞掉（判据见 `runtime::signal`）。
    // 这个循环每接一条连接就转一圈，缝隙不罕见。
    let mut signals = crate::runtime::signal::Shutdown::listen();
    loop {
        tokio::select! {
            // 前台形态的正常终止路径：服务管理器与托管本进程的外部程序都靠它。
            // **必须同时接 SIGTERM**——那两者用的都是它，只接 SIGINT 会让最常见的
            // 正常停止走不到清理、退出码非零（见 `runtime::signal`）。
            _ = signals.recv() => return Ok(()),
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
    /// 解除配对要经它拿到已配对设备表的端口。
    data_dir: DataDir,
    /// 正在传的那几条的实时进度（库里那份在传输期间是陈旧的）。
    progress: Arc<crate::runtime::progress::ProgressCache>,
    /// 接收落点。`config set receive-dir` 要连它一起换，否则盘上与内存分叉。
    receive: Arc<ReceiveDir>,
}

#[async_trait::async_trait]
impl RequestHandler for NodeHandler {
    async fn handle(&self, req: Request, progress: &FrameSink) -> Response {
        match req {
            Request::Status => json_or_error(
                serde_json::to_value(self.node.manager.get_network_status()),
                "状态",
            ),
            Request::DeviceList => json_or_error(
                serde_json::to_value(crate::runtime::devices::from_node(&self.node)),
                "设备列表",
            ),
            Request::DeviceForget { peer_ids } => {
                // 节点在跑 ⇒ 传 `Some`，核心会额外停掉对这些设备的在线状态维持。
                // `Records` 只用来拿已配对设备表这个端口，不另开数据库连接。
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                match crate::runtime::devices::forget(&records, Some(&self.node), &peer_ids).await {
                    Ok(outcome) => json_or_error(serde_json::to_value(outcome), "解除结果"),
                    Err(err) => Response::err(err),
                }
            }
            Request::InviteList => json_or_error(
                serde_json::to_value(
                    self.node
                        .manager
                        .pairing()
                        .list_invites()
                        .into_iter()
                        .map(crate::runtime::invites::InviteRow::from)
                        .collect::<Vec<_>>(),
                ),
                "邀请清单",
            ),
            Request::InviteRevoke { hashes } => {
                // **先把标识全部解析完再动手**：其中一个不合法时一张都不该被撤——
                // 批量撤销不可逆，部分执行之后用户既不知道做到了哪张，也无法原样重试。
                let mut parsed = Vec::with_capacity(hashes.len());
                for hash in &hashes {
                    let Some(bytes) = swarmdrop_invite::capability_hash_from_hex(hash) else {
                        return Response::usage(format!("不是合法的邀请标识: {hash}"));
                    };
                    parsed.push(bytes);
                }

                let mut outcome = crate::runtime::invites::RevokeOutcome {
                    revoked: 0,
                    persisted: true,
                };
                for bytes in parsed {
                    // **不短路**：某一张写穿失败不该让后面的都不撤。
                    outcome.persisted &= self
                        .node
                        .manager
                        .pairing()
                        .revoke_invite_by_hash(bytes)
                        .await;
                    outcome.revoked += 1;
                }
                json_or_error(serde_json::to_value(outcome), "撤销结果")
            }
            Request::InviteRevokeAll => {
                let mut revoked = 0usize;
                let mut persisted = true;
                for summary in self.node.manager.pairing().list_invites() {
                    // **不短路**：某一张写穿失败不该让后面的都不撤——那正是「全撤」要防的情形。
                    persisted &= self
                        .node
                        .manager
                        .pairing()
                        .revoke_invite_by_hash(summary.capability_hash)
                        .await;
                    revoked += 1;
                }
                json_or_error(
                    serde_json::to_value(crate::runtime::invites::RevokeOutcome {
                        revoked,
                        persisted,
                    }),
                    "撤销结果",
                )
            }
            Request::TransferList => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::transfers::list(&*store).await {
                    Ok(items) => json_or_error(serde_json::to_value(items), "传输记录"),
                    Err(err) => Response::err(err),
                }
            }
            Request::TransferShow { id } => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::transfers::show(&*store, &id).await {
                    Ok(mut item) => {
                        // 与清单同源：库里的发送进度整条传输期间是陈旧的，速率则根本
                        // 不落库。少了这两步，`transfer show` 会给出一条停在上次终结
                        // 值的记录，且永远没有速率。
                        self.progress.overlay(std::slice::from_mut(&mut item));
                        let mut payload = serde_json::to_value(item);
                        if let Ok(record) = &mut payload {
                            self.progress.annotate(record);
                        }
                        json_or_error(payload, "传输记录")
                    }
                    Err(err) => Response::err(err),
                }
            }
            Request::TransferUnfinished => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::transfers::unfinished(&*store).await {
                    Ok(mut items) => {
                        // **必须盖在这里，不能留给客户端**：进度事件只在本进程里，
                        // 而客户端是另一个进程。见 `runtime::progress`。
                        self.progress.overlay(&mut items);
                        let mut payload = serde_json::to_value(items);
                        // 速率与剩余时间同理，只是它们在投影里没有字段，得等 JSON 化
                        // 之后再标上去。少了这一步，面板只能自己估——那会高一个数量级
                        // （判据见 `render::send::rate_and_eta`）。
                        if let Ok(records) = &mut payload {
                            self.progress.annotate(records);
                        }
                        json_or_error(payload, "传输记录")
                    }
                    Err(err) => Response::err(err),
                }
            }
            Request::TransferControl { action, ids } => {
                let transfer = self.node.manager.transfer_arc();
                match crate::runtime::transfers::control(&transfer, action, &ids).await {
                    Ok(outcome) => json_or_error(serde_json::to_value(outcome), "控制结果"),
                    Err(err) => Response::err(err),
                }
            }
            Request::InviteCreate => {
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
                    Err(err) => {
                        Response::err(CliError::NodeUnavailable(format!("生成邀请失败: {err}")))
                    }
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
                    // 请求失效不是「节点不可用」——客户端应当继续等下一条，而不是判定节点坏了。
                    None => Response::usage("这条配对请求已经失效（对端已断开或等待超时）"),
                }
            }
            Request::InviteUse { invite } => {
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
                    Err(err) => {
                        Response::err(CliError::PeerUnreachable(format!("配对失败: {err}")))
                    }
                }
            }
            Request::InboxList { include_archived } => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::inbox::list(&*store, include_archived).await {
                    Ok(items) => json_or_error(serde_json::to_value(items), "收件箱"),
                    Err(err) => Response::err(err),
                }
            }
            Request::InboxShow { id } => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::inbox::detail(&*store, &id).await {
                    Ok(detail) => json_or_error(serde_json::to_value(detail), "条目详情"),
                    Err(err) => Response::err(err),
                }
            }
            Request::InboxSearch {
                query,
                limit,
                include_archived,
            } => {
                let store = self.node.manager.transfer_arc().store().clone();
                match crate::runtime::inbox::search(&*store, &query, limit, include_archived).await
                {
                    Ok(hits) => json_or_error(serde_json::to_value(hits), "检索结果"),
                    Err(err) => Response::err(err),
                }
            }
            Request::Send { paths, to } => {
                let paths: Vec<std::path::PathBuf> =
                    paths.into_iter().map(std::path::PathBuf::from).collect();
                // **进度推给通道对面的客户端**：终端在那一侧，画在这边等于画进
                // 常驻节点自己的日志流。
                match crate::runtime::transfer::send_files(
                    &self.node,
                    &paths,
                    &to,
                    crate::runtime::transfer::ProgressOut::Ipc(progress),
                )
                .await
                {
                    Ok(outcome) => Response::Data {
                        payload: crate::runtime::transfer::file_payload(&outcome),
                    },
                    Err(err) => Response::err(err),
                }
            }
            Request::SendText { body, to } => {
                match crate::runtime::transfer::send_text(&self.node, body, &to).await {
                    Ok(outcome) => Response::Data {
                        payload: crate::runtime::transfer::text_payload(&outcome),
                    },
                    Err(err) => Response::err(err),
                }
            }
            Request::Subscribe { inbox_limit } => {
                // **长驻**：本分支一直不返回，直到客户端走开或节点关停。`accept_one`
                // 那侧的 `peer_gone` 分支负责在客户端走人时连同整个任务一起取消。
                crate::runtime::watch::serve::serve(
                    &self.node,
                    &self.progress,
                    progress,
                    inbox_limit.unwrap_or(crate::runtime::watch::baseline::DEFAULT_INBOX_LIMIT),
                )
                .await
            }
            Request::ConfigList => {
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                match crate::runtime::settings::scalar::views(&records).await {
                    Ok(views) => json_or_error(serde_json::to_value(views), "配置"),
                    Err(err) => Response::err(err),
                }
            }
            Request::ConfigSet { key, value } => {
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                let live = crate::runtime::settings::scalar::Live {
                    node: &self.node,
                    receive: &self.receive,
                };
                match crate::runtime::settings::scalar::apply(&records, Some(live), key, value)
                    .await
                {
                    Ok(written) => json_or_error(serde_json::to_value(written), "写入结果"),
                    Err(err) => Response::err(err),
                }
            }
            Request::BootstrapList => {
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                match crate::runtime::bootstrap_nodes::list(&records, Some(&self.node)).await {
                    Ok(rows) => json_or_error(serde_json::to_value(rows), "引导节点清单"),
                    Err(err) => Response::err(err),
                }
            }
            Request::BootstrapAdd { addr } => {
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                match crate::runtime::bootstrap_nodes::add(&records, Some(&self.node), &addr).await
                {
                    Ok(changed) => json_or_error(serde_json::to_value(changed), "添加结果"),
                    Err(err) => Response::err(err),
                }
            }
            Request::BootstrapRemove { addr } => {
                let records = crate::runtime::access::Records::new(self.data_dir.clone());
                match crate::runtime::bootstrap_nodes::remove(&records, Some(&self.node), &addr)
                    .await
                {
                    Ok(changed) => json_or_error(serde_json::to_value(changed), "撤销结果"),
                    Err(err) => Response::err(err),
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
        // 序列化失败是服务端自己的问题，不是用户参数错——按「节点不可用」报。
        Err(err) => Response::err(CliError::NodeUnavailable(format!(
            "序列化{what}失败: {err}"
        ))),
    }
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
            async fn handle(&self, _req: Request, _progress: &FrameSink) -> Response {
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
