//! 服务端：一条订阅从建立到断开的全过程。
//!
//! 只有常驻节点跑得了这里——事件在它的进程里。客户端那一半在
//! [`crate::cmd::watch`]，它负责跨节点起落把这些片段接成一条连续的流。
//!
//! ## 背压的终点必须在本函数里
//!
//! 三段依次相连，缺一段就会把代价转嫁到不该承担它的地方：
//!
//! ```mermaid
//! flowchart LR
//!   P["领域 publish"] -->|try_send<br/>满了就丢| Q["有界队列"]
//!   Q --> L["serve 循环"]
//!   L -->|阻塞写<br/>慢就慢| S["本地套接字"]
//!   S --> C["客户端 stdout"]
//!   Q -.->|边沿类还要计数| T["truncated 事件"]
//! ```
//!
//! - **入队非阻塞**：`publish` 被 await 在传输的收发块簿记里，阻塞它等于让一个旁观者
//!   拖慢正在进行的传输——那条回路的终点不在本程序手里，永远闭合不了。
//! - **出队可阻塞**：本循环是这条订阅**专属**的任务，挂住不影响任何别的东西；而阻塞
//!   正是把压力顶回上一段的方式。中途偷偷丢帧才是错的（见 [`FrameSink::send`]）。
//! - **丢弃如实上报**：变成一条 [`WatchEvent::Truncated`]，绝不静默
//!   （spec: `cli-event-stream` 的「边沿事件不得静默丢失」）。

use crate::adapter::events::DropCount;
use crate::exit::CliError;
use crate::runtime::boot::RunningNode;
use crate::runtime::ipc::{FrameSink, Response};
use crate::runtime::progress::ProgressCache;

use super::baseline::{self, Source};
use super::event::{self, DeviceEntry, WatchEvent};
use super::fold::{Coalescer, SAMPLE_INTERVAL};

/// 一条订阅的待发队列容量（条）。
///
/// 只在**客户端读得比事件产生得慢**时才用得上，正常情况下它一直是空的。
/// 挑一个够大的数是为了让「客户端卡了一两秒」不产生截断（那种卡顿在终端里很常见），
/// 而不是为了兜住一个永远不读的客户端——那种情况本来就该如实截断。
const QUEUE_CAPACITY: usize = 512;

/// 跑一条订阅，直到客户端走开或节点关停。
///
/// 返回值是这次请求的终态。它**通常写不出去**（连接已经没了，[`FrameSink`] 已封口），
/// 但节点正常关停那条路径上写得出去——客户端于是收到一个干净的收尾而不是一次 EOF。
pub async fn serve(
    node: &RunningNode,
    progress: &ProgressCache,
    sink: &FrameSink,
    inbox_limit: u32,
) -> Response {
    // ⚠️ **先订阅再拼基线，顺序不能反。** 反过来的话，快照拍完到订阅建立之间发生的事件
    // 谁也不认领——它们不在基线里，也不会被推送。反过来的重复（基线里已有、随后又推一遍）
    // 是无害的：消费方按 `itemId` / `sessionId` 覆盖即可。
    let (mut rx, dropped) = node
        .events
        .subscribe_lossy(QUEUE_CAPACITY, event::report_loss);

    let store = node.manager.transfer_arc().store().clone();
    let baseline = baseline::build(
        &*store,
        Source::Node {
            devices: devices_now(node),
            progress,
        },
        inbox_limit,
    )
    .await;

    let baseline = match baseline {
        Ok(baseline) => baseline,
        Err(err) => return Response::err(err),
    };
    // **用基线里那张设备表给收敛器开局**：节点起来后第一条 `DevicesChanged` 几乎必然
    // 与它逐字相同，不 seed 的话消费方会在基线之后立刻收到一条什么都没变的变化事件。
    let mut fold = Coalescer::seeded(baseline.devices.clone());
    if !emit(sink, WatchEvent::Baseline(baseline)).await {
        return client_gone();
    }

    let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let delivered = tokio::select! {
            received = rx.recv() => match received {
                Some(core) => forward(node, sink, &mut fold, &core).await,
                // 事件总线没了 = 节点正在关停。给客户端一个干净的收尾。
                None => return Response::Ok,
            },
            _ = ticker.tick() => flush(sink, &mut fold).await,
        };

        if !delivered || !report_truncation(sink, &dropped).await {
            return client_gone();
        }
    }
}

/// 客户端走了。
///
/// 措辞不会有人看见（连接已经没了，[`FrameSink`] 也已封口），但它必须是个**错误**而不是
/// `Ok`：万一它写得出去，`Ok` 会让客户端把一次异常断开读成一次正常收尾。
fn client_gone() -> Response {
    Response::err(CliError::NodeUnavailable("订阅的对端已断开".into()))
}

/// 把一条领域事件转成订阅面事件并投递出去。返回 `false` = 连接没了。
async fn forward(
    node: &RunningNode,
    sink: &FrameSink,
    fold: &mut Coalescer,
    core: &swarmdrop_core::host::CoreEvent,
) -> bool {
    // 配对成功 / 解除配对只带一个 `peer_id`，全量设备表要向节点现取——
    // 判据与「为什么不能等下一条 `DevicesChanged`」见 [`event::invalidates_devices`]。
    if event::invalidates_devices(core) {
        let refreshed = WatchEvent::DevicesChanged {
            devices: devices_now(node),
        };
        if let Some(out) = fold.accept(refreshed)
            && !emit(sink, out).await
        {
            return false;
        }
    }

    match event::translate(core).and_then(|event| fold.accept(event)) {
        Some(out) => emit(sink, out).await,
        None => true,
    }
}

/// 降频窗口到点：把攒着的采样交出去。返回 `false` = 连接没了。
async fn flush(sink: &FrameSink, fold: &mut Coalescer) -> bool {
    for event in fold.flush() {
        if !emit(sink, event).await {
            return false;
        }
    }
    true
}

/// 有边沿事件被丢掉时如实告诉消费方。返回 `false` = 连接没了。
async fn report_truncation(sink: &FrameSink, dropped: &DropCount) -> bool {
    match dropped.take() {
        0 => true,
        dropped => emit(sink, WatchEvent::Truncated { dropped }).await,
    }
}

/// 此刻的已配对设备表。
///
/// 名册经 [`crate::runtime::devices::paired_on_node`] 取——「该用哪个 `DeviceFilter`」
/// 那个决定只有一份（用 `All` 会把局域网里路过的陌生设备也列进来）。
/// 取到之后仍经 [`event::paired_entries`]，是为了那里的**排序**：下游靠「与上一份逐字
/// 相同」去重，次序一抖就会每次 ping 都推一条内容完全一样的变化事件。
fn devices_now(node: &RunningNode) -> Vec<DeviceEntry> {
    event::paired_entries(&crate::runtime::devices::paired_on_node(node))
}

/// 序列化并投递一帧。返回 `false` = 连接没了。
async fn emit(sink: &FrameSink, event: WatchEvent) -> bool {
    match serde_json::to_value(&event) {
        Ok(payload) => sink.send(payload).await,
        // 这些 DTO 全是纯数据，序列化失败只可能来自本端的 bug。丢掉这一条比中断整条
        // 订阅好，但要留痕——否则它是这条流上唯一一种不上报的丢失。
        Err(err) => {
            tracing::warn!(%err, ?event, "订阅事件序列化失败，本帧丢弃");
            true
        }
    }
}
