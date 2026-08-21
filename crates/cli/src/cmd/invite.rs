//! `invite`：配对邀请的生成、使用、清点与撤销。
//!
//! ## 四条命令分走两条取数路径
//!
//! `create` 与 `use` 走 [`NodeAccess`](crate::runtime::access)——前者签发的
//! 邀请里带的是**签发节点**的可拨地址，后者要真的拨过去。
//!
//! `list` 与 `revoke` 走 [`RecordAccess`](crate::runtime::access)，
//! **不需要节点**。这不是便利性考虑：邀请 TTL 24 小时且跨重启存活，发现泄露时撤销是唯一
//! 有效的处置，而那一刻用户很可能并未启动节点。把清点绑在运行中的节点上，等于要求用户在
//! 止损前先完成一次可能失败的启动。
//!
//! ## 生成的那一侧要一直守到配对完成
//!
//! 而不是打完码就退出。两个理由：邀请的可拨地址就是签发节点的（临时节点一退出码就作废），
//! 以及——更重要的——入站请求要由**这个终端前的人**看过对端信息之后才放行。
//! `invite create` 在跑，就是「此刻有人在等一次配对」的唯一表达；它不在跑时，常驻节点会
//! 拒掉一切配对请求。

use std::path::Path;

use crate::adapter::paths::DataDir;
use crate::cmd::InviteAction;
use crate::exit::{CliError, CliResult};
use crate::prompt::Question;
use crate::prompt::pick::Picker;
use crate::runtime::access::{NodeAccess, RecordAccess, now_secs, to_value};
use crate::runtime::invites::{self, InviteRow, PrefixError, RevokeOutcome};
use crate::runtime::ipc::{self, Request, Response};
use crate::runtime::pairing::{InboundPairings, PairOutcome, PairingRequest};

pub async fn run(data_dir: &DataDir, json: bool, action: InviteAction) -> CliResult<()> {
    match action {
        InviteAction::Create { auto_accept } => create(data_dir, json, auto_accept).await,
        InviteAction::Use { invite } => use_invite(data_dir, json, invite).await,
        InviteAction::List => list(data_dir, json).await,
        InviteAction::Revoke { ids, all, yes } => revoke(data_dir, json, ids, all, yes).await,
    }
}

// ---------------------------------------------------------------- 清点与撤销

/// 列出本机已发出、尚未过期的邀请。
async fn list(data_dir: &DataDir, json: bool) -> CliResult<()> {
    crate::render::invite::render_list(&fetch(&RecordAccess::open(data_dir).await).await?, json);
    Ok(())
}

/// 取一次邀请清单。
async fn fetch(access: &RecordAccess) -> CliResult<Vec<InviteRow>> {
    let payload = access
        .query(Request::InviteList, |records| async move {
            let registry = records.invites().await?;
            to_value(&invites::list(&registry, now_secs()), "邀请清单")
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析邀请清单: {err}")))
}

/// 撤销：指定的几张、勾选的几张，或全部。
///
/// **逐张与 `--all` 不是同一件事**，不要把后者实现成「取列表再逐条撤」：那是 N 次往返，
/// 且这期间新签发的邀请会漏掉——而 `--all` 服务的正是「不知道哪张泄露了」。
async fn revoke(
    data_dir: &DataDir,
    json: bool,
    ids: Vec<String>,
    all: bool,
    yes: bool,
) -> CliResult<()> {
    let access = RecordAccess::open(data_dir).await;

    if all {
        return revoke_all(&access, json, yes).await;
    }

    // 整屏共用一个时刻：每行各取一次时钟会让相邻两行的剩余时间落在不同的秒上
    // （「还有 59 分钟」紧挨着「还有 58 分钟」）。
    let now = now_secs();
    let chosen = Picker {
        fetch: async || fetch(&access).await,
        label: |row: &InviteRow| crate::render::invite::menu_line(row, now),
        prompt: "撤销哪些邀请？",
        empty: "本机没有尚未过期的邀请，无需撤销",
        unavailable: "请指定要撤销哪些邀请（标识，接受唯一前缀，可给多个）。\n\
                      当前环境无法交互选择；用 swarmdrop invite list 查看有哪些，或用 --all 全部撤销。",
    }
    .many(&ids, locate)
    .await?;

    // **先去重**：两个参数可能指向同一张（`revoke abcd abcd1234` 都是那张的唯一前缀），
    // 而每一条都会被计数一次——用户会看到「已撤销 2 张」下面列着同一个标识两遍。
    // 按标识去重而不是按参数：前缀写法不同、指向同一张时字符串并不相等。
    let chosen = super::dedup_by_id(chosen, |row| row.id.clone());

    // **一次调用撤掉全部选中的**，不要在这里循环：无常驻节点时每一次 `send_revoke` 都要
    // 新开数据库连接、跑迁移、把整张邀请表读回内存（还带一次 prune 写事务），逐张发等于
    // 把这些全做 N 遍；有常驻节点时则是 N 次通道往返。
    let hashes: Vec<String> = chosen.iter().map(|row| row.id.clone()).collect();
    let outcome = send_revoke(
        &access,
        Request::InviteRevoke {
            hashes: hashes.clone(),
        },
        {
            move |registry| async move {
                invites::revoke_each(&registry, hashes.iter().map(String::as_str))
                    .await
                    .map_err(|bad| CliError::Usage(format!("不是合法的邀请标识: {bad}")))
            }
        },
    )
    .await?;

    crate::render::invite::render_revoked(&chosen, &outcome, json);
    Ok(())
}

/// 全撤。
///
/// **在可交互环境下确认一次**：它一次废掉全部在外流通的邀请，属于 clig.dev 分级里的
/// moderate。不可交互且未给 `--yes` 时报用法错误——脚本要做这件事就得把意图写明白。
async fn revoke_all(access: &RecordAccess, json: bool, yes: bool) -> CliResult<()> {
    let rows = fetch(access).await?;
    if rows.is_empty() {
        return Err(CliError::Usage("本机没有尚未过期的邀请，无需撤销".into()));
    }

    if !yes {
        if !crate::prompt::can_ask() {
            return Err(CliError::Usage(format!(
                "将撤销全部 {} 张未过期邀请，这不可撤回。\n\
                 当前环境无法确认；确定要这么做请加 --yes。",
                rows.len()
            )));
        }
        let prompt = format!("确定撤销全部 {} 张未过期的邀请？", rows.len());
        // 读不到回答与明确的「否」都不放行，但只有前者是「人走了」。
        if crate::prompt::confirm(prompt).await != Some(true) {
            return Err(CliError::Aborted);
        }
    }

    let outcome = send_revoke(access, Request::InviteRevokeAll, |registry| async move {
        Ok(invites::revoke_all(&registry, now_secs()).await)
    })
    .await?;

    crate::render::invite::render_revoked_all(&outcome, json);
    Ok(())
}

/// 送一条撤销请求：有常驻走通道，否则在本机记录上做。
async fn send_revoke<F, Fut>(
    access: &RecordAccess,
    request: Request,
    local: F,
) -> CliResult<RevokeOutcome>
where
    F: FnOnce(swarmdrop_invite::InviteRegistry) -> Fut,
    Fut: std::future::Future<Output = CliResult<RevokeOutcome>>,
{
    let payload = access
        .query(request, |records| async move {
            // ⚠️ 注册表**必须是 load 过的**（`Records::invites` 保证了这点）：
            // `revoke_by_hash` 查不到内存记录会直接 no-op 并报告成功。
            let registry = records.invites().await?;
            to_value(&local(registry).await?, "撤销结果")
        })
        .await?;

    serde_json::from_value(payload)
        .map_err(|err| CliError::NodeUnavailable(format!("无法解析撤销结果: {err}")))
}

/// 按前缀定位一张邀请，把失败翻成可行动的措辞。
///
/// 返回**拥有的**行而非引用：[`Picker`] 要把选中的结果带出候选集的作用域
/// （候选集是它内部取的临时值）。`InviteRow` 只有四个小字段，克隆的代价可以忽略。
fn locate(rows: &[InviteRow], prefix: &str) -> CliResult<InviteRow> {
    invites::resolve_prefix(rows, prefix)
        .cloned()
        .map_err(|err| match err {
            PrefixError::TooShort => CliError::Usage(format!(
                "邀请标识至少要给 {} 位，防止手滑撤掉一张没打算撤的。",
                invites::MIN_PREFIX
            )),
            PrefixError::NotFound => CliError::Usage(format!(
                "没有以「{prefix}」开头的未过期邀请。用 swarmdrop invite list 看看有哪些。"
            )),
            // **绝不代为消解歧义**：撤销不可逆。
            PrefixError::Ambiguous(candidates) => CliError::Usage(format!(
                "「{prefix}」匹配到 {} 张邀请，请多给几位区分开：\n  {}",
                candidates.len(),
                candidates.join("\n  ")
            )),
        })
}

// ---------------------------------------------------------------- 生成与使用

/// 生成一张新邀请，然后守着它直到有设备配对成功或用户中断。
///
/// 常驻节点在跑时**必须由它签发**：邀请里带的是签发者的可拨地址，本进程另起一个节点
/// 签出来的邀请指向一个即将消失的临时节点——对方会拿到一条拨不通的邀请。此时确认仍然
/// 发生在**本命令**这一侧：常驻节点把入站请求经本地通道转交过来。
async fn create(data_dir: &DataDir, json: bool, auto_accept: bool) -> CliResult<()> {
    // **`--json` 与 `--no-input` 在这里必须分开处理**，尽管两者都关掉了交互。
    //
    // `--no-input` 是「不要问我」：命令照常运行、守着邀请，期间到达的请求一律拒绝
    // （spec: 禁止交互时收到入站配对请求 → 拒绝）。它有明确的用途——只想把邀请摆出来
    // 看看，不打算真的配对。
    //
    // `--json` 是「调用方是程序」，而本命令在结构化模式下**没有可用的形态**：
    // 它会生成一张注定配不上的邀请（每个入站请求都被拒），然后**永不返回**——
    // 更糟的是 `render_declined` / `render_request_expired` 的 json 分支会持续往
    // stdout 追加对象，直接破坏「结构化模式下 stdout 只能有最终结果」这条契约，
    // 而调用方那边表现为一个读不完的流。所以快速失败，并指出唯一可用的组合。
    if json && !auto_accept {
        return Err(CliError::Usage(
            "结构化输出模式下无法交互确认配对请求。
             请加 --auto-accept（自动接受第一台出示有效邀请的设备——届时无人核对对端身份，
             请只在可控网络里这么做）。"
                .into(),
        ));
    }

    // 剩下的是「环境问不了人，而用户什么也没说」——那时这条命令注定做不成它该做的事。
    // 先判断而不是等请求真的来了才发现问不了：那时对端已经在等，而这一侧只能干拒。
    if !auto_accept && !crate::prompt::can_ask() && !crate::prompt::interaction_declined() {
        return Err(CliError::Usage(
            "无法交互确认配对请求：当前没有可用的终端。\n\
             无人值守场景请加 --auto-accept，那表示自动接受第一台出示有效邀请的设备——\n\
             届时无人核对对端身份，请只在可控网络里这么做；\n\
             若只想生成邀请而不接受任何配对，加 --no-input。"
                .into(),
        ));
    }

    let access = NodeAccess::open(data_dir, json).await?;
    let result = serve_invite(&access, data_dir, json, auto_accept).await;
    access.close().await;
    result
}

async fn serve_invite(
    access: &NodeAccess,
    data_dir: &DataDir,
    json: bool,
    auto_accept: bool,
) -> CliResult<()> {
    let invite = match access.ask(&Request::InviteCreate).await? {
        Some(Response::Data { payload }) => payload
            .get("invite")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| CliError::NodeUnavailable("节点返回的邀请为空".into()))?,
        Some(Response::Error { code, message }) => return Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => {
            let node = access.require_local()?;
            node.manager
                .pairing()
                .encode_invite(&node.secret_key, swarmdrop_invite::TransportPolicy::Auto)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("生成邀请失败: {err}")))?
        }
    };

    // 刚生成的这张的标识。**必须给出**：邀请清单里没有邀请串本身（明文不落盘），
    // 能区分多张的信息只有标识与时刻，一分钟内发两张时仅凭时刻分不出哪张发给了谁。
    // 它是「刚发错人、立刻撤回」这条主场景可用的前提。
    //
    // **从手上这个串直接算，不去查清单**：清单按 `created_at`（Unix 秒）倒序且底层是
    // `HashMap`，同一秒里发的两张谁排第一完全任意——「取第一条」会给出另一张的标识，
    // 而它紧接着被印成「发错了可以撤回：… revoke {id}」。判据见 `PairInvite::id`。
    // 解码失败只让标识缺席，不让整条命令失败：邀请本身已经生成好了。
    let id = swarmdrop_invite::PairInvite::decode(&invite)
        .ok()
        .map(|parsed| parsed.id());

    crate::render::invite::render_created(&invite, id.as_deref(), json);
    crate::render::invite::render_waiting(access.local().is_some(), auto_accept, json);

    let abort = spawn_abort_watch();
    let socket = data_dir.socket();
    let desk = match access.local() {
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
                    Some(Response::Error { code, message }) => {
                        return Err(CliError::from_code(code, message));
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
                crate::render::invite::render_paired(&outcome, json);
                return Ok(());
            }
            // 拒绝之后**继续守着**：用户等的是自己那台设备，而被拒的这次没有消费掉
            // 邀请，那张码仍然有效。
            Some(_) => crate::render::invite::render_declined(&request, json),
            None => crate::render::invite::render_request_expired(json),
        }
    }
}

/// 起一个只注册一次的中止监听。
///
/// ⚠️ **不能在循环里反复新建信号监听器**。`select!` 每轮新建一个，另一分支胜出时它连同
/// 注册一起被丢弃；而 tokio 的信号接收端是个 `watch` 订阅，新订阅从当前版本起算，
/// 看不到自己出生前的那一次通知——那一刻到达的信号就此蒸发。这条判据现在由
/// [`crate::runtime::signal::Shutdown`] 承载（建一次、反复 `recv`），这里是它最早的
/// 那个用例。
///
/// 确认提示期间尤其致命：`dialoguer` 在 raw 模式下关掉了 `ISIG`、自己读到 `\x03` 之后
/// 补发一个 SIGINT，而那正好是没有监听者的时候。表现是用户按了 Ctrl-C 只等到一句
/// 「已拒绝」，命令若无其事地继续等下一台设备，要再按一次才真的退得掉。
fn spawn_abort_watch() -> std::sync::Arc<tokio::sync::Notify> {
    let abort = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = abort.clone();

    tokio::spawn(async move {
        // 一直挂着、不重建：注册那一刻起就不再有空档。
        {
            crate::runtime::signal::Shutdown::listen().recv().await;
            // `notify_one` 会存下 permit，所以即使此刻没人在等（正卡在确认提示里），
            // 下一次 `notified()` 也会立刻完成。
            signal.notify_one();
        }
    });

    abort
}

/// 展示对端信息并决定接不接受。
///
/// 三条判据，顺序不能换：
/// 1. `--auto-accept` → 接受（用户已显式表达「别问我」）。
/// 2. 问得了人 → 问。
/// 3. 否则 → **拒绝**。显式关掉交互是 fail-closed 的：不问就是不放行。
///
/// **`--auto-accept` 也照样把信息打出来**：无人值守不等于事后无法追查，
/// 日志里留着「当时配上来的是谁」是这条开关唯一的安全兜底。
async fn decide(request: &PairingRequest, auto_accept: bool, json: bool) -> Option<bool> {
    crate::render::invite::render_pairing_request(request, json);

    if auto_accept {
        return Some(true);
    }
    if !crate::prompt::can_ask() {
        tracing::warn!(
            who = request.device,
            "已拒绝入站配对请求：交互已被禁用（--no-input / --json）"
        );
        return Some(false);
    }
    crate::prompt::confirm(format!("接受来自「{}」的配对请求？", request.device)).await
}

/// 用一张别人给的邀请完成配对。
///
/// **先在本地解码一次**再交给节点：解码失败是用户把串抄错了（用法错误），与「对端连不上」
/// 是两回事，而退出码要区分它们——脚本对这两种的处置不同（一个是改参数重来，一个是等对方
/// 上线再试）。不先解码的话，两种失败会一起落进「对端不可达」。
async fn use_invite(data_dir: &DataDir, json: bool, invite: Option<String>) -> CliResult<()> {
    // 没给就问。**这里没有候选集可列**——邀请是对方给的，本机对它一无所知，
    // 所以这一条走行输入而不是菜单。
    let invite = match invite {
        Some(invite) => invite,
        None => {
            Question::new("粘贴对方给你的邀请链接")
                .ask(
                    "请给出邀请链接。当前环境无法交互输入；\n\
                     链接由对方执行 swarmdrop invite create 生成。",
                )
                .await?
        }
    };
    // **粘贴几乎必然带上首尾空白**（换行、终端软换行留下的空格），而 base32 载荷里
    // 没有空白字符，trim 掉不会误伤任何合法链接。不 trim 的话用户看到的是一句
    // 「邀请串无效」，而屏幕上那条链接看起来完全正常。
    let invite = invite.trim().to_owned();

    let decoded = swarmdrop_invite::PairInvite::decode(&invite)
        .map_err(|err| CliError::Usage(format!("邀请串无效: {err}；请确认完整复制了整条链接")))?;

    let access = NodeAccess::open(data_dir, json).await?;
    let result = pair_with(&access, json, &invite, &decoded.inviter.id.to_string()).await;
    access.close().await;
    result
}

async fn pair_with(access: &NodeAccess, json: bool, invite: &str, inviter: &str) -> CliResult<()> {
    crate::render::invite::render_awaiting_confirmation(inviter, json);

    let outcome = match access
        .ask(&Request::InviteUse {
            invite: invite.to_owned(),
        })
        .await?
    {
        Some(Response::Data { payload }) => serde_json::from_value::<PairOutcome>(payload)
            .map_err(|err| CliError::NodeUnavailable(format!("无法解析配对结果: {err}")))?,
        Some(Response::Error { code, message }) => return Err(CliError::from_code(code, message)),
        Some(Response::Ok) | None => {
            let node = access.require_local()?;
            let (response, commit) = node
                .manager
                .pairing()
                .pair_with_invite(invite)
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

    crate::render::invite::render_paired(&outcome, json);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> InviteRow {
        InviteRow {
            id: id.into(),
            created_at: now_secs(),
            expires_at: now_secs() + 3600,
            consumed: false,
        }
    }

    /// 往一个空数据目录里塞一张真邀请。
    ///
    /// `revoke_all` 自己取清单，所以测它必须有**真的落盘的**记录——空库上它会先撞到
    /// 「没有尚未过期的邀请」，那条路径同样退 2，会让下面那条断言变成假绿。
    async fn with_one_invite(dir: &DataDir) {
        let invite = swarmdrop_invite::PairInvite::generate(
            &swarmdrop_net::SecretKey::generate(),
            vec!["/ip4/192.168.1.10/tcp/4001".parse().expect("addr")],
            swarmdrop_invite::TransportPolicy::Auto,
            "测试机".into(),
            "macos".into(),
            now_secs(),
        )
        .expect("生成邀请");

        crate::runtime::access::Records::new(dir.clone())
            .invites()
            .await
            .expect("注册表")
            .register(&invite, now_secs())
            .await;
    }

    /// 全撤在不可交互且未给 `--yes` 时立刻退出，不得静默执行。
    ///
    /// **静默执行比挂起更坏**：它会在没有任何确认的情况下废掉全部在外流通的邀请。
    #[tokio::test]
    async fn revoke_all_without_a_terminal_requires_yes() {
        let _guard = crate::prompt::no_interaction().await;

        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        with_one_invite(&dir).await;
        let access = RecordAccess::open(&dir).await;

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            revoke_all(&access, false, false),
        )
        .await;

        let outcome = result.expect("确认环节挂起了");
        let err = outcome.expect_err("应当要求 --yes");
        assert_eq!(err.code(), crate::exit::Code::Usage);
        assert!(err.to_string().contains("--yes"), "措辞要指出出路: {err}");
    }

    /// 前缀撞车时把候选原样交给用户，**不代为挑一张**——撤销不可逆。
    #[test]
    fn ambiguous_prefix_surfaces_candidates() {
        let rows = [row("abcd1111"), row("abcd2222")];
        let err = locate(&rows, "abcd").expect_err("撞车必须拒绝");
        assert_eq!(err.code(), crate::exit::Code::Usage);
        let text = err.to_string();
        assert!(
            text.contains("abcd1111") && text.contains("abcd2222"),
            "{text}"
        );
    }
}
