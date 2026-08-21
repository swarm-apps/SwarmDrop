//! 发送编排：解析目标 → 枚举文件 → 准备 → 发出 → 等终态。
//!
//! 放在 runtime 而非 cmd，是因为**两条路径都要用它**：本进程自持节点时直接调，
//! 常驻节点在跑时由通道服务端调。两处各写一遍会立刻漂移。
//!
//! 文本投递（[`send_text`]）与文件发送共用「解析目标」这一步，**其余一步不共用**：
//! 它没有枚举、没有准备、没有分块，也不订阅事件——一次 RPC 从发出到拿到确定结果，
//! 结论就在返回值里。硬凑成同一条流程只会让两边都长出一半是空转的分支。

use std::future::Future;
use std::path::{Path, PathBuf};

use swarmdrop_core::protocol::TransferOrigin;
use swarmdrop_core::transfer::HostEnumeratedFile;

use crate::exit::{CliError, CliResult};

use super::boot::RunningNode;

/// 发送进度往哪儿去。
///
/// 两条路径**画的是同一件事，画在不同的地方**：本进程自持节点时终端就在这一侧，直接画；
/// 常驻节点在做时终端在通道对面，只能把事实推过去、由那边画。
///
/// 渲染只有客户端一份（[`crate::render::send`]），这一层只负责把事实送到对的地方——
/// 服务端画进度等于画进它自己的日志流，用户的终端上仍然一个字都没有。
pub enum ProgressOut<'a> {
    /// 就画在这个终端上。`enabled` 为假 = 结构化输出模式，什么都不画。
    Bars { enabled: bool },
    /// 推给通道对面的客户端。
    Ipc(&'a crate::runtime::ipc::FrameSink),
}

/// 进度帧的 `phase` 取值。**客户端按它分发到两种进度条**，改了要两边一起改。
///
/// 抄成常量而不是共享一个枚举：这是通道上的**线格式**，与内部类型解耦——
/// 内部枚举改名不该悄悄改变线上的字符串。
///
/// **没有护栏测试，也不需要**：生产者（本文件）与消费者
/// （[`crate::render::send::RemoteProgress::on_frame`]）引用的是**同一个常量**，
/// 同一版二进制里不可能分叉。真正的漂移只发生在跨版本（新客户端 × 旧节点），
/// 而那由 `on_frame` 的「不认识的 phase 一律忽略」兜住——降级成少画一个进度条，
/// 不是打断传输。
pub const PHASE_PREPARING: &str = "preparing";
pub const PHASE_TRANSFERRING: &str = "transferring";

impl<'a> ProgressOut<'a> {
    /// 开一段准备进度。
    fn preparing(&self) -> Stage<'_, crate::render::send::Preparing> {
        match self {
            Self::Bars { enabled } => Stage::Local(crate::render::send::Preparing::new(*enabled)),
            Self::Ipc(sink) => Stage::Remote(sink),
        }
    }

    /// 开一段传输进度。
    fn transferring(&self) -> Stage<'_, crate::render::send::Progress> {
        match self {
            Self::Bars { enabled } => Stage::Local(crate::render::send::Progress::new(*enabled)),
            Self::Ipc(sink) => Stage::Remote(sink),
        }
    }
}

/// 一段进度，**出口已经绑定**。
///
/// 这个类型替掉了原先的 `draws_locally()`。那个方法存在的唯一目的是喂两个进度条的构造
/// 函数，于是两处调用点都得写「先问该不该画、再建一个可能永远不画的 bar」——推给通道时
/// 那个 bar 建了从不使用。现在「出口是什么」只在开一段的时候回答一次，之后 `update`
/// 无条件调用。
///
/// bar 作为字段被持有，因此它的 `Drop`（收进度条、清行）与这一段的作用域绑在一起，
/// 不再依赖调用点记得让它活到正确的时刻。
enum Stage<'a, B> {
    /// 画在本地终端。
    Local(B),
    /// 推给通道对面的客户端。
    Remote(&'a crate::runtime::ipc::FrameSink),
}

impl Stage<'_, crate::render::send::Preparing> {
    async fn update(&self, event: &PrepareFrame<'_>) {
        match self {
            Self::Local(bar) => bar.update(
                event.done,
                event.total,
                event.file,
                event.completed_files,
                event.total_files,
            ),
            Self::Remote(sink) => {
                sink.try_send(serde_json::json!({
                    "phase": PHASE_PREPARING,
                    "done": event.done,
                    "total": event.total,
                    "file": event.file,
                    "completedFiles": event.completed_files,
                    "totalFiles": event.total_files,
                }))
                .await
            }
        }
    }
}

impl Stage<'_, crate::render::send::Progress> {
    async fn update(&self, done: u64, total: u64) {
        match self {
            Self::Local(bar) => bar.update(done, total),
            Self::Remote(sink) => {
                sink.try_send(serde_json::json!({
                    "phase": PHASE_TRANSFERRING,
                    "done": done,
                    "total": total,
                }))
                .await
            }
        }
    }
}

/// 一帧准备进度的字段。收成结构体只是为了不让参数列表长到六个。
struct PrepareFrame<'a> {
    done: u64,
    total: u64,
    file: &'a str,
    completed_files: u32,
    total_files: u32,
}

/// 发送结果。
pub struct SendOutcome {
    pub session_id: uuid::Uuid,
    pub file_count: usize,
    pub total_bytes: u64,
}

/// 一次文件发送的结果在**三条对外路径**上的共同形状：`--json` 的 stdout、本地通道的
/// 负载、MCP 工具的返回值。
///
/// ⚠️ **它归 runtime 而不是 render。** 它不是「怎么把结果显示给人看」，而是「这次发送
/// 的结果是什么」——一份契约。放在 render 会让另外两条路径为了拿到同一份 JSON 反向
/// 依赖表现层（通道服务端就曾经这么调），而工具面本来是要能整体搬到共享 crate 的。
pub fn file_payload(outcome: &SendOutcome) -> serde_json::Value {
    serde_json::json!({
        "sessionId": outcome.session_id.to_string(),
        "fileCount": outcome.file_count,
        "totalBytes": outcome.total_bytes,
    })
}

/// 把文件或目录发给一台已配对设备，**阻塞到传输终态**。
pub async fn send_files(
    node: &RunningNode,
    paths: &[PathBuf],
    to: &str,
    progress: ProgressOut<'_>,
) -> CliResult<SendOutcome> {
    let (peer_id, peer_name) = resolve_target(node, to)?;

    let files = collect_files(paths)?;
    if files.is_empty() {
        return Err(CliError::Usage("没有可发送的文件".into()));
    }
    let file_count = files.len();
    let total_bytes = files.iter().map(|f| f.size).sum();

    let transfer = node.manager.transfer_arc();
    let prepared_id = uuid::Uuid::new_v4();

    // **订阅必须早于 `prepare`，不只是早于 `send_offer`。** 两条理由各自独立：
    //
    // - 准备阶段（校验和 + bao 验签树）自己就是一串事件，而 `prepare` 是一次**长阻塞
    //   调用**——几个 GB 的文件在这里要算几十秒。订阅建在它之后的话，那段时间用户面对
    //   的是一个既不动也不报错的终端，而进度事件早已发完、无处可订。`prepared_id` 由
    //   调用方生成正是为了这件事：事件带着它，好让宿主**在准备开始之前**就认领得到
    //   （桌面端同理，见 `src/stores/transfer-store.ts` 的 `activePrepare`）。
    // - 一次极快的传输可能在订阅建立之前就结束，于是等待方永远等不到那条终态事件。
    let mut events = node.events.subscribe();

    let prepared = prepare_with_progress(
        transfer.prepare(prepared_id, files),
        &mut events,
        prepared_id,
        &progress,
    )
    .await
    .map_err(|err| CliError::TransferFailed(format!("准备发送失败: {err}")))?;

    let selected: Vec<u32> = prepared.files.iter().map(|f| f.file_id).collect();

    let started = transfer
        .send_offer(
            &prepared.prepared_id,
            &peer_id,
            &peer_name,
            &selected,
            TransferOrigin::Human,
        )
        .await
        .map_err(|err| CliError::PeerUnreachable(format!("发出传输请求失败: {err}")))?;
    let session_id = started.session_id;

    wait_for_terminal(&mut events, session_id, &progress).await?;

    Ok(SendOutcome {
        session_id,
        file_count,
        total_bytes,
    })
}

/// 一条文本投递在通道上的状态名（`entity::TextDeliveryStatus` 的 serde 形态）。
///
/// 抄一份而不是依赖 `entity`，理由同 [`super::transfers`] 里的 phase 常量：本 crate 的
/// 生产代码只认端口与 JSON。
///
/// ⚠️ 抄来的字符串会**静默**漂移，而这里漂移的后果比 phase 那边更重：分类落空时
/// [`classify`] 会把一次**成功送达**报成失败并给出非零退出码——脚本据此重发，
/// 对端于是收到两份。`text_status_names_match_the_wire` 是唯一的看守。
const TEXT_DELIVERED: &str = "delivered";
const TEXT_REJECTED: &str = "rejected";
const TEXT_RETRYABLE: &str = "retryable";
const TEXT_EXPIRED: &str = "expired";

/// 失败分类里「根本没把话送到对端」的那两种（`entity::TextDeliveryFailure` 的 serde 形态）。
///
/// 单独列它们是为了选对退出码：这两种是 [`CliError::PeerUnreachable`]（重试或等对方上线
/// 可能就好），其余是 [`CliError::TransferFailed`]（对端看见了但没收下）。
/// 混成一个码，脚本就只能一律退避重试——而「对端拒绝」重试多少次都还是拒绝。
const TEXT_FAILURE_PEER_UNAVAILABLE: &str = "peer_unavailable";
const TEXT_FAILURE_TIMED_OUT: &str = "timed_out";

/// 文本投递结果。
pub struct TextOutcome {
    pub delivery_id: uuid::Uuid,
    /// 目标设备的显示名。命令行里用户敲的可能是节点标识，回执要说人话。
    pub peer_name: String,
    /// 正文的 UTF-8 字节数（**不是字符数**）——64 KiB 上限量的就是它。
    pub bytes: usize,
}

/// 一次文本发送的结果在三条对外路径上的共同形状——**必须只有这一份**，
/// 否则 `--json` 的字段名会因「此刻有没有常驻节点」而不同。判据同 [`file_payload`]。
pub fn text_payload(outcome: &TextOutcome) -> serde_json::Value {
    serde_json::json!({
        "deliveryId": outcome.delivery_id.to_string(),
        "peerName": outcome.peer_name,
        "bytes": outcome.bytes,
    })
}

/// 把一段文本发给一台已配对设备，**阻塞到拿得出确定结论**。
///
/// 「确定结论」不是「已发包」：底层 RPC 的成功应答代表**接收端已经把正文落库**。
/// 对端策略要求人工确认时，这条调用会一直等到对方点了确认或确认窗口（5 分钟）耗尽。
///
/// ⚠️ **这几分钟里的「还在等」由调用方画**（[`crate::cmd::send`]），不像
/// [`send_files`] 那样收个 [`ProgressOut`]。两者的区别是**屏幕在谁那一侧**：文件的
/// 进度是一串只有节点收得到的事件，所以只能由持节点的进程画；而这里从头到尾只是一次
/// 阻塞调用，客户端自己就知道「开始等了」——把它画在这边，常驻节点在跑时那个转轮就会
/// 出现在**服务端**的日志流里，而用户的终端一片空白。
pub async fn send_text(node: &RunningNode, body: String, to: &str) -> CliResult<TextOutcome> {
    let (peer_id, peer_name) = resolve_target(node, to)?;
    let bytes = body.len();

    let record = node
        .manager
        .transfer_arc()
        .send_text_delivery(peer_id, peer_name.clone(), body)
        .await
        .map_err(|err| CliError::TransferFailed(format!("发送文本失败: {err}")))?;

    classify(&record).map(|()| TextOutcome {
        delivery_id: record.delivery_id,
        peer_name,
        bytes,
    })
}

/// 把账本记录的终态翻成「成功」或一个带分类的失败。
///
/// **状态与失败原因经 serde 取字符串**，不 match typed 枚举：生产代码不依赖 `entity`
/// （见上面那组常量）。
fn classify(record: &swarmdrop_core::transfer::text_delivery::TextDeliveryRecord) -> CliResult<()> {
    let status = serde_json::to_value(&record.status).unwrap_or_default();
    let failure = record
        .failure
        .as_ref()
        .and_then(|failure| serde_json::to_value(failure).ok())
        .unwrap_or_default();
    let failure = failure.as_str().unwrap_or_default();

    match status.as_str().unwrap_or_default() {
        TEXT_DELIVERED => Ok(()),
        // 对端看见了并且说了不。重试同一条只会再被拒一次。
        TEXT_REJECTED => Err(CliError::TransferFailed(
            "对端拒绝了这条文本（设备策略或用户拒绝）".into(),
        )),
        // 确认窗口耗尽。**不能建议重发同一条**：接收端已把该投递标为过期，
        // 同一个投递标识再来一次仍然直接判过期（幂等键的另一面）。
        TEXT_EXPIRED => Err(CliError::TransferFailed(
            "对端未在确认窗口内接收，这条文本已过期；请重新发送".into(),
        )),
        TEXT_RETRYABLE => Err(match failure {
            TEXT_FAILURE_PEER_UNAVAILABLE | TEXT_FAILURE_TIMED_OUT => CliError::PeerUnreachable(
                "未能确认送达：对端此刻不可达或没有应答，可稍后重发".into(),
            ),
            _ => CliError::TransferFailed("未能确认送达：对端未能保存这条文本，可稍后重发".into()),
        }),
        // 账本只会以上面四种状态收尾。真出现别的，如实说不知道——
        // 报成功会让一条没送到的文本被当成送到了。
        other => Err(CliError::TransferFailed(format!(
            "文本投递以未知状态收尾: {other}"
        ))),
    }
}

/// 跑一次 `prepare`，同时把它的进度画出来。
///
/// 两件事必须并发：`prepare` 是一次长阻塞调用，而它的进度只在事件流里。
///
/// 收一个 future 而不是 `&TransferManager`，是为了不把核心那个泛型参数
/// （`transfer_arc()` 返回 `Arc<TTransfer>`）拖进签名——这个函数关心的只有
/// 「一个会完成的 future」和「期间画进度」，与被准备的是什么无关。
///
/// ⚠️ **这里 drain 掉的事件不会再传给 [`wait_for_terminal`]。** 可以吞是因为本会话此刻
/// **还不存在**——`session_id` 要到 `send_offer` 之后才有，所以准备阶段流过的一定不是
/// 本会话的传输事件。**这条前提一旦变了**（比如将来 prepare 也挂在 session 上），
/// 这里就必须改成转发而不是丢弃，否则一次极快的传输会把终态事件丢在这儿，
/// 命令永远等不到而挂死。
async fn prepare_with_progress<T, E>(
    prepare: impl Future<Output = Result<T, E>>,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<swarmdrop_core::host::CoreEvent>,
    prepared_id: uuid::Uuid,
    progress: &ProgressOut<'_>,
) -> Result<T, E> {
    use swarmdrop_core::host::CoreEvent;

    // 出口在这里绑定一次，之后无条件 `update`——「该不该画」不再散在每个调用点。
    let stage = progress.preparing();
    tokio::pin!(prepare);

    // 事件通道还开着吗。**这个标志不能省，也不能用「在分支体里挂起」代替**：
    // `recv()` 在通道关闭后每次都**立刻**返回 `None`，照着继续循环就是一个烧满一个核的
    // 忙循环。而在分支体里 `pending().await` 更糟——`select!` 的分支体是在**选中之后**
    // 执行的，那时 `prepare` 那条分支已经被丢弃，于是它再也得不到轮询，整条命令挂死。
    // （这不是推测：`a_closed_event_channel_does_not_spin` 第一次跑就是这么红的。）
    // 正解是把这条分支**关掉**，让后续迭代只轮询 `prepare`。
    let mut events_open = true;

    loop {
        tokio::select! {
            // `biased`：prepare 完成的那一刻就走，不再多轮询一轮事件。
            // 少了它，收尾那条 100% 与结果行的先后是随机的。
            biased;
            result = &mut prepare => {
                // **走之前把残帧排干。** `prepare` 内部最后那次「完成」进度
                // （`completed_files == total_files`，且不受 200ms 节流）是在这个 future
                // resolve **之前**推进通道的，而 `biased` 让我们一到就走——那一帧于是
                // 流进 `wait_for_terminal` 被 `_ => {}` 丢掉。
                //
                // 后果不是「少画一帧」：`send_offer` 之后要等对端接受（最长 180 秒），
                // 这期间屏幕上就停在**最后一次节流帧**的那个数上——可能是 99%，
                // 小文件上可能是 4%。用户看到的与「卡住」一模一样。
                drain_prepare_progress(events, prepared_id, &stage).await;
                return result;
            }
            event = events.recv(), if events_open => match event {
                // 按 `prepared_id` 认领：同一个节点上可能有另一次准备在跑
                // （常驻节点同时服务着别的命令），不认领会让两批进度互相覆盖。
                Some(CoreEvent::PrepareProgress { event }) if event.prepared_id == prepared_id => {
                    emit_prepare_frame(&stage, &event).await;
                }
                // 通道断开（节点关停）：关掉本分支，由 `prepare` 给出真正的失败原因
                // ——那比在这里编一个准确。`prepare` 分支永远开着，所以不会出现
                // 「所有分支都被禁用」那种 `select!` panic。
                None => events_open = false,
                _ => {}
            },
        }
    }
}

/// 把还留在通道里、属于本次准备的进度帧推完。
///
/// **只 `try_recv`，绝不 await 新事件**：准备已经结束，这里要的是「把已经到了的收干净」，
/// 而不是再等。不属于本次准备的事件原样丢弃——`wait_for_terminal` 此刻还没开始读，
/// 而本会话的传输事件要到 `send_offer` 之后才可能出现（判据同上方那段注释）。
async fn drain_prepare_progress(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<swarmdrop_core::host::CoreEvent>,
    prepared_id: uuid::Uuid,
    stage: &Stage<'_, crate::render::send::Preparing>,
) {
    use swarmdrop_core::host::CoreEvent;

    while let Ok(event) = events.try_recv() {
        if let CoreEvent::PrepareProgress { event } = event
            && event.prepared_id == prepared_id
        {
            emit_prepare_frame(stage, &event).await;
        }
    }
}

async fn emit_prepare_frame(
    stage: &Stage<'_, crate::render::send::Preparing>,
    event: &swarmdrop_core::transfer::progress::PrepareProgressEvent,
) {
    stage
        .update(&PrepareFrame {
            done: event.bytes_hashed,
            total: event.total_bytes,
            file: &event.current_file,
            completed_files: event.completed_files,
            total_files: event.total_files,
        })
        .await;
}

/// 等待这条会话进入终态。
///
/// 只认**本会话**的事件：同一个节点上可能有其他传输在跑，按事件类型而不看会话号会
/// 让一条无关传输的失败把本命令带下水。
async fn wait_for_terminal(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<swarmdrop_core::host::CoreEvent>,
    session_id: uuid::Uuid,
    out: &ProgressOut<'_>,
) -> CliResult<()> {
    use swarmdrop_core::host::CoreEvent;

    // 出口在这里绑定一次；之后的调用点无条件 `update`。
    let stage = out.transferring();

    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::TransferProgress { event } if event.session_id == session_id => {
                stage
                    .update(event.transferred_bytes, event.total_bytes)
                    .await;
            }
            CoreEvent::TransferCompleted { event } if event.session_id == session_id => {
                // 进度条由 `Progress` 的 `Drop` 收尾——这里以及下面三条出口都不必各收一次。
                return Ok(());
            }
            CoreEvent::TransferFailed { event } if event.session_id == session_id => {
                return Err(CliError::TransferFailed(format!(
                    "传输失败: {}",
                    event.error
                )));
            }
            CoreEvent::TransferRejected { event } if event.session_id == session_id => {
                return Err(CliError::TransferFailed("对端拒绝了这次传输".into()));
            }
            _ => {}
        }
    }

    // 事件通道断开 = `CliEventBus` 被销毁 = 持有它的节点没了。
    //
    // ⚠️ **这条出口实际上到不了，别照着它写测试**（2026-08-20 核实）。两条路径各有原因：
    // 常驻节点那侧，在途的处理任务自己持有 `handler → node → events` 这条 Arc 链，
    // 它先于事件通道被销毁——`stop` 时客户端读到的是 EOF，由通道层给出
    // `NodeUnavailable`(3)；本进程自持临时节点那侧，`access.close()` 发生在
    // `deliver` 返回**之后**，此时早已走完。
    //
    // 留着它是因为「事件通道可能断」是 `recv()` 的类型事实，而 `unreachable!()` 会把一个
    // 可恢复的意外变成 panic。分类仍按「传输被外力打断」给——不是 `Aborted`（退出码 130，
    // 脚本按惯例读作「人按了 Ctrl-C，别重试」），万一哪天真到得了，重试是合理的。
    Err(CliError::TransferFailed("常驻节点已停止，传输中断".into()))
}

/// 把设备名或节点标识解析成一台已配对设备，返回 `(标识, 显示名)`。
///
/// **匹配与措辞都复用 [`super::devices`] 那一份**，这里只负责取数与取字段。此前它是独立的
/// 第二份实现，代价已经付过一次：`DeviceFilter` 用错（`All` 而非 `Paired`）的修复只落在
/// `device list` 那一份上，这一份继续错着——表现是 `swarmdrop send … --to <设备>` 在没有
/// 常驻节点时报「找不到已配对设备」，而 `swarmdrop device list` 明明列着它。
///
/// 两份还各自漂移出了不同的行为：那一份**标识优先**、歧义时列出全部候选标识；这一份把
/// 标识与名称混在一个 filter 里、只报「匹配到 N 台」——用户知道有歧义却无从消歧。
///
/// ⚠️ **过滤器必须是 `Paired`**（现由 [`super::devices::from_node`] 独家决定）。`All` 取的
/// 是本次运行发现的对端，而一次性命令每次都新起临时节点，那张表在发出请求的这一刻通常还是
/// 空的；反方向也错——`All` 会把局域网里路过的**未配对**设备算进候选，于是能按名字选中一台
/// 根本没配过对的机器，然后在数据面被对端拒掉。
fn resolve_target(node: &RunningNode, to: &str) -> CliResult<(String, String)> {
    let rows = super::devices::from_node(node);
    let row = super::devices::resolve_target(&rows, to)
        .map_err(|err| super::devices::target_error(to, err))?;
    Ok((row.peer_id.clone(), row.name.clone()))
}

/// 展开命令行给的路径：文件直接收，目录递归展开。
///
/// 自己实现而不复用桌面那份目录扫描：那份的产物带着跨 IPC 的类型（要给前端渲染选择列表），
/// 而这里要的是核心的 [`HostEnumeratedFile`]。形状不同，共享反而要多一层转换。
fn collect_files(paths: &[PathBuf]) -> CliResult<Vec<HostEnumeratedFile>> {
    let mut out = Vec::new();
    for path in paths {
        let meta = std::fs::metadata(path)
            .map_err(|err| CliError::Usage(format!("读取 {} 失败: {err}", path.display())))?;

        if meta.is_dir() {
            let root_name = file_name_of(path);
            walk(path, &root_name, &mut out)?;
        } else {
            out.push(entry_of(path, file_name_of(path), meta.len()));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<HostEnumeratedFile>) -> CliResult<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|err| CliError::Usage(format!("读取目录 {} 失败: {err}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = file_name_of(&path);
        // 相对路径一律用 `/` 分隔：它要跨平台传给对端，Windows 的 `\` 在那边会变成文件名的一部分。
        let relative = format!("{prefix}/{name}");

        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk(&path, &relative, out)?;
        } else if meta.is_file() {
            out.push(entry_of(&path, relative, meta.len()));
        }
        // 其他类型（符号链接指向的特殊文件、设备文件等）跳过：它们没有可传输的字节。
    }
    Ok(())
}

fn entry_of(path: &Path, relative_path: String, size: u64) -> HostEnumeratedFile {
    HostEnumeratedFile {
        // 标识就是路径本身——本地文件访问实现按「先试 JSON、否则当路径」解回来。
        source_id: swarmdrop_core::host::FileSourceId(path.to_string_lossy().into_owned()),
        name: file_name_of(path),
        relative_path,
        size,
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一条准备进度事件。
    fn prepare_event(prepared_id: uuid::Uuid) -> swarmdrop_core::host::CoreEvent {
        swarmdrop_core::host::CoreEvent::PrepareProgress {
            event: swarmdrop_core::transfer::progress::PrepareProgressEvent {
                prepared_id,
                current_file: "a.bin".into(),
                completed_files: 0,
                total_files: 1,
                bytes_hashed: 1,
                total_bytes: 2,
            },
        }
    }

    /// **准备期间流过的事件必须被消费掉，而 `prepare` 一完就返回。**
    ///
    /// 看守的是「输入完文件后卡住」那个缺陷的修复：订阅此前建在 `prepare` **之后**，
    /// 于是准备阶段（大文件要算几十秒的 bao 验签树）一条进度都画不出来——用户面对的是
    /// 一个既不动也不报错的终端。这条测试钉住「边跑边收」这个形状本身：
    /// 事件源源不断时 `prepare` 仍然要能完成并返回它自己的结果。
    #[tokio::test]
    async fn preparing_consumes_progress_and_still_returns() {
        let prepared_id = uuid::Uuid::new_v4();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        // 在 prepare 完成之前先灌几条进度进去。
        for _ in 0..3 {
            tx.send(prepare_event(prepared_id)).expect("发送事件");
        }

        let prepare = async {
            // 让出几次，确保事件分支有机会跑到。
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            Ok::<_, ()>(42)
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            prepare_with_progress(
                prepare,
                &mut rx,
                prepared_id,
                &ProgressOut::Bars { enabled: false },
            ),
        )
        .await
        .expect("prepare_with_progress 没有返回");

        assert_eq!(result, Ok(42));
    }

    /// **事件通道断开后不得忙循环，也不得提前返回。**
    ///
    /// `recv()` 在通道关闭后每次都**立刻**返回 `None`——照着继续 loop 就是一个烧满
    /// 一个核的死循环，而 `prepare` 那条分支再也得不到调度，命令彻底挂死。
    /// 这类缺陷在开发机上表现为「风扇转起来了但命令不动」，没有任何报错。
    ///
    /// 正确行为是挂起，让 `prepare` 自己跑完（或给出它自己的失败原因）。
    #[tokio::test]
    async fn a_closed_event_channel_does_not_spin() {
        let prepared_id = uuid::Uuid::new_v4();
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<swarmdrop_core::host::CoreEvent>();
        drop(tx); // 通道立刻断开：此后每次 recv() 都返回 None

        let prepare = async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok::<_, ()>("done")
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            prepare_with_progress(
                prepare,
                &mut rx,
                prepared_id,
                &ProgressOut::Bars { enabled: false },
            ),
        )
        .await
        .expect("通道断开后挂死了（多半是忙循环饿死了 prepare）");

        assert_eq!(result, Ok("done"));
    }

    /// 别人的准备进度不得被认领。
    ///
    /// 常驻节点同时服务着多条命令时，两批准备的进度会走同一条事件流；
    /// 不按 `prepared_id` 认领就会互相覆盖，进度条来回跳。
    #[tokio::test]
    async fn progress_from_another_prepare_is_ignored() {
        let mine = uuid::Uuid::new_v4();
        let theirs = uuid::Uuid::new_v4();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(prepare_event(theirs)).expect("发送事件");

        let prepare = async {
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            Ok::<_, ()>(())
        };

        // 只要不 panic、不挂死即可：别人的事件被安静地丢掉。
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            prepare_with_progress(
                prepare,
                &mut rx,
                mine,
                &ProgressOut::Bars { enabled: false },
            ),
        )
        .await
        .expect("没有返回")
        .expect("prepare 应当成功");
    }

    /// 目录要递归展开，且相对路径带上根目录名——对端据此重建目录结构。
    #[test]
    fn directory_expands_recursively_with_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("bundle");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), "aa").unwrap();
        std::fs::write(root.join("sub/b.txt"), "bbb").unwrap();

        let files = collect_files(&[root]).unwrap();

        assert_eq!(files.len(), 2);
        let mut paths: Vec<_> = files.iter().map(|f| f.relative_path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["bundle/a.txt", "bundle/sub/b.txt"]);
    }

    /// 单个文件的相对路径就是文件名，不带任何目录前缀。
    #[test]
    fn single_file_has_bare_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("solo.bin");
        std::fs::write(&file, [0u8; 8]).unwrap();

        let files = collect_files(&[file]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "solo.bin");
        assert_eq!(files[0].size, 8);
    }

    /// 路径不存在必须是用法错误，而不是等到传输开始才失败。
    #[test]
    fn missing_path_is_a_usage_error() {
        let err = collect_files(&[PathBuf::from("/definitely/not/here")]).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    /// **本模块认识的投递状态名必须与核心实际序列化出来的一致。**
    ///
    /// 这几个常量是抄来的（生产代码不依赖 `entity`），而抄来的字符串会**静默**漂移：
    /// 核心改了 `rename_all` 或变体名之后，[`classify`] 的分支会全部落空、掉进末尾那条
    /// 兜底——于是一次**成功送达**被报成「未知状态」并给出非零退出码，脚本据此重发，
    /// 对端收到两份。不报错、不 panic、别的测试也不红。除非有这一条。
    #[test]
    fn text_status_names_match_the_wire() {
        for (variant, expected) in [
            (entity::TextDeliveryStatus::Delivered, TEXT_DELIVERED),
            (entity::TextDeliveryStatus::Rejected, TEXT_REJECTED),
            (entity::TextDeliveryStatus::Retryable, TEXT_RETRYABLE),
            (entity::TextDeliveryStatus::Expired, TEXT_EXPIRED),
        ] {
            let wire = serde_json::to_value(&variant).expect("序列化");
            assert_eq!(wire.as_str(), Some(expected), "{variant:?} 的 wire 名变了");
        }

        for (variant, expected) in [
            (
                entity::TextDeliveryFailure::PeerUnavailable,
                TEXT_FAILURE_PEER_UNAVAILABLE,
            ),
            (
                entity::TextDeliveryFailure::TimedOut,
                TEXT_FAILURE_TIMED_OUT,
            ),
        ] {
            let wire = serde_json::to_value(&variant).expect("序列化");
            assert_eq!(wire.as_str(), Some(expected), "{variant:?} 的 wire 名变了");
        }
    }

    /// 只有「已送达」是成功；其余每一种终态都必须以非零退出码收尾。
    ///
    /// **这条钉的是退出码而不只是「有没有报错」**：不可达与传输失败对脚本是两种处置
    /// （退避重试 vs 别再试了），混成一个码等于让调用方回去解析文本。
    #[test]
    fn only_delivered_succeeds_and_unreachable_keeps_its_own_code() {
        use crate::exit::Code;

        let record =
            |status, failure| swarmdrop_core::transfer::text_delivery::TextDeliveryRecord {
                delivery_id: uuid::Uuid::nil(),
                direction: entity::TextDeliveryDirection::Send,
                peer_id: "peer".into(),
                peer_name: "设备".into(),
                body: "正文".into(),
                status,
                failure,
                attempt_count: 1,
                created_at: 0,
                updated_at: 0,
            };

        assert!(classify(&record(entity::TextDeliveryStatus::Delivered, None)).is_ok());

        for (status, failure, expected) in [
            (
                entity::TextDeliveryStatus::Rejected,
                Some(entity::TextDeliveryFailure::Rejected),
                Code::TransferFailed,
            ),
            (
                entity::TextDeliveryStatus::Expired,
                Some(entity::TextDeliveryFailure::Expired),
                Code::TransferFailed,
            ),
            (
                entity::TextDeliveryStatus::Retryable,
                Some(entity::TextDeliveryFailure::TimedOut),
                Code::PeerUnreachable,
            ),
            (
                entity::TextDeliveryStatus::Retryable,
                Some(entity::TextDeliveryFailure::PeerUnavailable),
                Code::PeerUnreachable,
            ),
            (
                entity::TextDeliveryStatus::Retryable,
                Some(entity::TextDeliveryFailure::StorageFailed),
                Code::TransferFailed,
            ),
            // 账本不该以它收尾，但真收到时必须是失败而不是「当作送达」。
            (
                entity::TextDeliveryStatus::WaitingConfirmation,
                None,
                Code::TransferFailed,
            ),
        ] {
            let err = classify(&record(status.clone(), failure)).expect_err("非送达必须失败");
            assert_eq!(err.code(), expected, "{status:?} 的退出码不对");
        }
    }
}
