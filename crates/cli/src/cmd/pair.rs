//! `pair`：生成配对邀请并守着它，或以一个邀请完成配对。
//!
//! **生成邀请的那一侧要一直守到配对完成**，而不是打完码就退出。两个理由：
//! 邀请的可拨地址就是签发节点的（临时节点一退出码就作废），以及——更重要的——
//! 入站请求要由**这个终端前的人**看过对端信息之后才放行。`pair` 在跑，就是
//! 「此刻有人在等一次配对」的唯一表达；它不在跑时，常驻节点会拒掉一切配对请求。

use std::path::Path;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{self, Request, Response};
use crate::runtime::pairing::{InboundPairings, PairOutcome, PairingRequest};
use crate::runtime::session::Session;

pub async fn run(
    data_dir: &DataDir,
    json: bool,
    invite: Option<String>,
    no_qr: bool,
    auto_accept: bool,
) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;

    let result = match invite {
        Some(invite) => accept(&session, json, invite).await,
        None => generate(&session, data_dir, json, no_qr, auto_accept).await,
    };

    session.close().await;
    result
}

/// 生成一张新邀请，然后守着它直到有设备配对成功或用户中断。
///
/// 常驻节点在跑时**必须由它签发**：邀请里带的是签发者的可拨地址，本进程另起一个节点
/// 签出来的码指向一个即将消失的临时节点——扫码方会拿到一张拨不通的码。此时确认仍然
/// 发生在**本命令**这一侧：常驻节点把入站请求经本地通道转交过来。
async fn generate(
    session: &Session,
    data_dir: &DataDir,
    json: bool,
    no_qr: bool,
    auto_accept: bool,
) -> CliResult<()> {
    // 先判断「待会儿问得了人吗」，而不是等请求真的来了才发现问不了——那时对端已经
    // 在等，而这一侧只能干拒。
    if !auto_accept && !crate::prompt::can_ask() {
        return Err(CliError::Usage(
            "无法交互确认配对请求：当前没有可用的终端。\n\
             无人值守场景请加 --auto-accept，那表示自动接受第一台出示有效邀请的设备——\n\
             届时无人核对对端身份，请只在可控网络里这么做。"
                .into(),
        ));
    }
    if !auto_accept && json {
        return Err(CliError::Usage(
            "结构化输出模式下无法交互确认配对请求；请加 --auto-accept 明确表示自动接受。".into(),
        ));
    }

    let invite = match session.ask(&Request::PairGenerate).await? {
        Some(Response::Data { payload }) => payload
            .get("invite")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| CliError::NodeUnavailable("节点返回的邀请为空".into()))?,
        Some(Response::Error { message }) => return Err(CliError::NodeUnavailable(message)),
        Some(Response::Ok) | None => {
            let node = session.require_local()?;
            node.manager
                .pairing()
                .encode_invite(&node.secret_key, swarmdrop_invite::TransportPolicy::Auto)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("生成邀请失败: {err}")))?
        }
    };

    crate::render::pair::render_invite(&invite, json, no_qr);
    crate::render::pair::render_waiting(session.local().is_some(), auto_accept, json);

    let abort = spawn_abort_watch();
    let socket = data_dir.socket();
    let desk = match session.local() {
        Some(node) => Desk::Local {
            node,
            inbound: InboundPairings::subscribe(node),
        },
        None => Desk::Daemon { socket: &socket },
    };

    serve(desk, json, auto_accept, &abort).await
}

/// 待确认请求从哪来、答复往哪送。
///
/// 两条路径的差别**只有这两步**——本进程自持节点时直接订阅事件、直接应答；复用常驻节点
/// 时经本地通道取和送。把差别收在这里，等待循环（中止处理、确认、结果渲染）就只有一份，
/// 不会像此前那样在两处各写一遍、改一处漏一处。
enum Desk<'a> {
    Local {
        node: &'a crate::runtime::boot::RunningNode,
        inbound: InboundPairings,
    },
    Daemon {
        socket: &'a Path,
    },
}

impl Desk<'_> {
    /// 等到下一个待确认的请求。
    async fn next(&mut self) -> CliResult<PairingRequest> {
        match self {
            // 事件通道断开只发生在节点关停时。
            Self::Local { node, inbound } => inbound.next(node).await.ok_or(CliError::Aborted),
            Self::Daemon { socket } => loop {
                match ipc::request(socket, &Request::PairWaitNext).await? {
                    Some(Response::Data { payload }) => {
                        return serde_json::from_value(payload).map_err(|err| {
                            CliError::NodeUnavailable(format!("无法解析配对请求: {err}"))
                        });
                    }
                    // 本轮没有请求，**立即再问一次**：常驻节点靠「有没有客户端在取」
                    // 判断配对窗口开着没有，停下来就等于关窗。
                    Some(Response::Ok) => continue,
                    Some(Response::Error { message }) => {
                        return Err(CliError::NodeUnavailable(message));
                    }
                    None => {
                        return Err(CliError::NodeUnavailable("常驻节点已停止，配对中断".into()));
                    }
                }
            },
        }
    }

    /// 送回答复，拿到这次配对的结局。
    ///
    /// `None` = 这条请求已经失效（对端断开，或核心那侧等待超时）。
    async fn answer(&self, pending_id: u64, accept: bool) -> CliResult<Option<PairOutcome>> {
        match self {
            Self::Local { node, .. } => {
                Ok(crate::runtime::pairing::respond(node, pending_id, accept).await)
            }
            Self::Daemon { socket } => {
                let request = Request::PairRespond { pending_id, accept };
                match ipc::request(socket, &request).await? {
                    Some(Response::Data { payload }) => {
                        serde_json::from_value(payload).map(Some).map_err(|err| {
                            CliError::NodeUnavailable(format!("无法解析配对结果: {err}"))
                        })
                    }
                    // 服务端只在「请求已失效」时报错，那不是致命错误，调用方会继续守着。
                    Some(Response::Error { .. }) | Some(Response::Ok) => Ok(None),
                    None => Err(CliError::NodeUnavailable("常驻节点已停止，配对中断".into())),
                }
            }
        }
    }
}

/// 守着邀请，逐个确认入站请求，直到有一台设备配上或用户中止。
async fn serve(
    mut desk: Desk<'_>,
    json: bool,
    auto_accept: bool,
    abort: &tokio::sync::Notify,
) -> CliResult<()> {
    loop {
        let request = tokio::select! {
            _ = abort.notified() => return Err(CliError::Aborted),
            request = desk.next() => request?,
        };

        let decision = tokio::select! {
            // **确认提示期间也要能中止**：用户按 Ctrl-C 是想退出，不是想回答「否」。
            _ = abort.notified() => None,
            decision = decide(&request, auto_accept, json) => decision,
        };

        // 读不到回答 ⇒ 问的那个人已经走了。手上这条顺手拒掉（别让对端干等到核心那侧
        // 的超时），然后收工——**不走下面的「已拒绝，仍在等待」**，那句话与紧随其后的
        // 中止自相矛盾。
        let Some(accept) = decision else {
            let _ = desk.answer(request.pending_id, false).await;
            return Err(CliError::Aborted);
        };

        match desk.answer(request.pending_id, accept).await? {
            Some(outcome) if outcome.accepted => {
                crate::render::pair::render_paired(&outcome, json);
                return Ok(());
            }
            // 拒绝之后**继续守着**：用户等的是自己那台设备，而被拒的这次没有消费掉
            // 邀请，那张码仍然有效。
            Some(_) => crate::render::pair::render_declined(&request, json),
            None => crate::render::pair::render_request_expired(json),
        }
    }
}

/// 起一个只注册一次的中止监听。
///
/// ⚠️ **不能在循环里反复 `tokio::signal::ctrl_c()`**。`select!` 每轮新建一个，另一分支
/// 胜出时它连同注册一起被丢弃；而 tokio 的信号驱动在没有监听者时会无条件清掉 pending
/// 标志——那一刻到达的 Ctrl-C 就此蒸发。
///
/// 确认提示期间尤其致命：`dialoguer` 在 raw 模式下关掉了 `ISIG`、自己读到 `\x03` 之后
/// 补发一个 SIGINT，而那正好是没有监听者的时候。表现是用户按了 Ctrl-C 只等到一句
/// 「已拒绝」，命令若无其事地继续等下一台设备，要再按一次才真的退得掉。
fn spawn_abort_watch() -> std::sync::Arc<tokio::sync::Notify> {
    let abort = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = abort.clone();

    tokio::spawn(async move {
        // 一直挂着、不重建：注册那一刻起就不再有空档。
        if tokio::signal::ctrl_c().await.is_ok() {
            // `notify_one` 会存下 permit，所以即使此刻没人在等（正卡在确认提示里），
            // 下一次 `notified()` 也会立刻完成。
            signal.notify_one();
        }
    });

    abort
}

/// 展示对端信息并决定接不接受。
///
/// **`--auto-accept` 也照样把信息打出来**：无人值守不等于事后无法追查，
/// 日志里留着「当时配上来的是谁」是这条开关唯一的安全兜底。
async fn decide(request: &PairingRequest, auto_accept: bool, json: bool) -> Option<bool> {
    crate::render::pair::render_pairing_request(request, json);

    if auto_accept {
        return Some(true);
    }
    crate::prompt::confirm(format!("接受来自「{}」的配对请求？", request.device)).await
}

/// 以一个邀请完成配对。
///
/// **先在本地解码一次**再交给节点：解码失败是用户把串抄错了（用法错误），与「对端连不上」
/// 是两回事，而退出码要区分它们——脚本对这两种的处置不同（一个是改参数重来，一个是等对方
/// 上线再试）。不先解码的话，两种失败会一起落进「对端不可达」。
async fn accept(session: &Session, json: bool, invite: String) -> CliResult<()> {
    let decoded = swarmdrop_invite::PairInvite::decode(&invite)
        .map_err(|err| CliError::Usage(format!("邀请串无效: {err}；请确认完整复制了整条链接")))?;

    crate::render::pair::render_awaiting_confirmation(&decoded.inviter.id.to_string(), json);

    let outcome = match session
        .ask(&Request::PairAccept {
            invite: invite.clone(),
        })
        .await?
    {
        Some(Response::Data { payload }) => serde_json::from_value::<PairOutcome>(payload)
            .map_err(|err| CliError::NodeUnavailable(format!("无法解析配对结果: {err}")))?,
        Some(Response::Error { message }) => return Err(CliError::PeerUnreachable(message)),
        Some(Response::Ok) | None => {
            let node = session.require_local()?;
            let (response, commit) = node
                .manager
                .pairing()
                .pair_with_invite(&invite)
                .await
                .map_err(|err| CliError::PeerUnreachable(format!("配对失败: {err}")))?;
            PairOutcome::new(&response, &commit)
        }
    };

    // **`Ok` 不等于配对成功**：RPC 走完了，答案完全可能是「拒绝」。修复前这里丢掉了
    // 对端的答复直接报成功，用户要到之后 `send` 找不到设备时才发现，那时已无从归因。
    if !outcome.accepted {
        return Err(CliError::PairingRefused(
            "对方拒绝了这次配对。若不是本人拒绝，请确认邀请没有被抄错或用过；\
             必要时让对方重新生成一张"
                .into(),
        ));
    }

    crate::render::pair::render_paired(&outcome, json);
    Ok(())
}
