//! 客户端那一半：把服务端给的片段接成一条**连续的、带单调序号的**流。
//!
//! ## 它为什么不在 `cmd/` 里
//!
//! 这里要回答的三个问题，`cmd/` 一个都不该知道：
//!
//! - **数据从哪儿来**——有常驻节点就经通道订阅，没有就直读本机记录。那正是
//!   [`super::super::access`] 收口的那件事，而它的模块文档写着「每加一条命令都要重做一次
//!   这个判断，而判断错了不报错」。
//! - **断了怎么接**——节点起落是常态，重连与「这一瞬连不上」的竞态处理是运行时的事。
//! - **序号谁来发**——它是线格式契约的一部分，不是输出格式的一部分。
//!
//! `cmd/watch.rs` 因此只剩「解析参数 → 拿一条流 → 交给渲染」。
//!
//! ## 序号的作用域是**这条订阅**，所以只能由这一侧发
//!
//! 服务端随节点起落换人，发不出一个跨起落连续的号。消费方仅凭序号跳变判定漏读
//! （spec: `cli-event-stream` 的「事件带订阅内单调的序号」），所以发号的必须是那个
//! 从头活到尾的进程——本进程。

use serde_json::Value;

use crate::adapter::paths::DataDir;
use crate::exit::CliResult;
use crate::runtime::access::Records;
use crate::runtime::ipc::{self, Request, Response};

use super::baseline::{self, Source};
use super::event::{self, WatchEvent};

/// 没有常驻节点时，隔多久探一次它起来了没有。
///
/// 探测是一次本地套接字 connect，便宜；但节点出现是个罕见事件、而且没有人在盯着这条流
/// 等它——秒级足够，更密只是白烧 CPU。
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// 一条订阅。
///
/// 只存两样**存不出来的**东西：数据目录（通道路径与本机记录都从它派生）与序号。
/// 通道路径与请求都是每次用时现算的一次拼接——把它们也存成字段就有了两份会分叉的事实，
/// 而省下的是一秒一次的一次 `PathBuf` 拼接。
pub struct Subscription {
    data_dir: DataDir,
    inbox_limit: u32,
    /// 本次订阅内单调递增，从 0 起。**不跨订阅保持连续**——那是另一个语义，
    /// 升级它要递增 [`event::SCHEMA_VERSION`]。
    seq: u64,
}

impl Subscription {
    pub fn new(data_dir: &DataDir, inbox_limit: u32) -> Self {
        Self {
            data_dir: data_dir.clone(),
            inbox_limit,
            seq: 0,
        }
    }

    /// 跑到收到中断信号、或消费方把这条流关掉为止。
    ///
    /// 每条**已盖好版本与序号**的事件交给 `sink`；它返回 `false` 表示**没有人在听了**
    /// （宿主关掉了读端），那时本订阅正常收摊——继续往一条没人读的流上写没有意义，
    /// 而它还会让服务端那侧一直挂着一条订阅。
    ///
    /// `sink` 是同步的，而且**慢就该慢**：那次阻塞正是背压，它会一路顶回服务端的有界
    /// 队列并在那里变成一次如实上报的截断。偷偷跑掉一帧才是错的。
    pub async fn run(&mut self, mut sink: impl FnMut(Value) -> bool) -> CliResult<()> {
        // 「没人听了」这件事发生在闭包深处（`request_watching` 的读循环里），而要收摊的是
        // 外层循环。用一个 `Notify` 把它捎出来：`notify_one` 会存下 permit，所以哪怕此刻
        // 外层正卡在 `follow` 里，下一次轮询也立刻就绪。
        let gone = std::sync::Arc::new(tokio::sync::Notify::new());
        let notify = gone.clone();
        let mut emit = move |line: Value| {
            if !sink(line) {
                notify.notify_one();
            }
        };

        // **信号监听只注册一次，且必须在拼基线之前**：注册发生在这个 future 第一次被
        // 轮询的时候，而下面那次 `select!` 就是第一次。放到基线之后的话，「拼基线」这段
        // 读库期间收到的 `SIGTERM` 会按默认处置直接杀掉进程——空机器上它很快，
        // 慢机器上不是。循环里每轮新建一个 future 同样不行：那会在每次注册/注销之间
        // 留一个收不到信号的窗口。
        let mut signals = crate::runtime::signal::Shutdown::listen();

        // 订阅建立时必须先有一条基线（spec: `cli-event-stream` 的「初始基线」）。
        tokio::select! {
            _ = signals.recv() => return Ok(()),
            baseline = self.initial_baseline() => {
                if let Some(baseline) = baseline? {
                    self.deliver(&mut emit, WatchEvent::Baseline(baseline));
                }
            }
        }

        loop {
            tokio::select! {
                // 中断是**正常收摊**，退成功——调用方会把非零读作失败并触发重启或告警
                // （spec: `cli-event-stream` 的「退出语义」）。
                _ = signals.recv() => return Ok(()),
                // 消费方关掉了读端。同样是正常收摊：是它先走的。
                _ = gone.notified() => return Ok(()),
                () = self.follow(&mut emit) => {}
            }
        }
    }

    /// 跟一轮常驻节点：接得上就一直转发，接不上就等一会儿。
    async fn follow(&mut self, emit: &mut impl FnMut(Value)) {
        let socket = self.data_dir.socket();
        if !ipc::is_alive(&socket).await {
            tokio::time::sleep(POLL_INTERVAL).await;
            return;
        }
        let request = Request::Subscribe {
            inbox_limit: Some(self.inbox_limit),
        };

        // 接上之后**只在连接断开时返回**：服务端会一直推非终态帧。第一帧是它给的基线，
        // 其后是增量。
        //
        // ⚠️ `seq` 在闭包里被改，所以不能用 `self.deliver`——借用冲突。这也正好说明
        // 发号这件事只跟这条流有关，与它从哪儿来无关。
        let seq = &mut self.seq;
        let outcome = ipc::request_watching(&socket, &request, |payload| {
            emit(event::stamp(*seq, payload));
            *seq += 1;
        })
        .await;

        match outcome {
            // 连不上：节点在 `is_alive` 与这一句之间关停了，压根没接上——
            // **不宣告一件没发生过的事**，否则节点反复起落时会刷出一串无中生有的记录。
            //
            // ⚠️ 但如果这是订阅建立的那一瞬（`seq == 0`），基线就被这次竞态吞掉了：
            // `initial_baseline` 当时看到节点还在，把基线让给了服务端。补一条本地的，
            // 否则消费方一条事件都收不到，而 spec 要求「订阅建立时必须先有一条基线」。
            Ok(None) => {
                if self.seq == 0 {
                    match local_baseline(&self.data_dir, self.inbox_limit).await {
                        Ok(baseline) => self.deliver(emit, WatchEvent::Baseline(baseline)),
                        Err(err) => eprintln!("拼本机基线失败: {err}"),
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
                return;
            }
            // 常驻节点回了个错（拼基线失败之类）。它不是流的一部分，走 stderr——
            // 混进那条流会破坏调用方的解析。
            Ok(Some(Response::Error { message, .. })) => {
                eprintln!("常驻节点拒绝了这次订阅: {message}");
            }
            // 干净的终态：节点正常关停，收尾帧写出来了。
            Ok(Some(_)) => {}
            // 连接中途断了。原因对调用方没有意义——它要知道的只是「节点没了」。
            Err(err) => tracing::debug!(%err, "订阅连接断开"),
        }
        self.deliver(emit, WatchEvent::NodeUnavailable);

        // ⚠️ **一定要歇一下再回去重连。** 走到这里有一种情形是「接上了，但对面立刻
        // 拒绝」——比如新客户端撞上没重启的旧常驻节点（它不认识 `subscribe` 这个动词），
        // 或者服务端拼基线持续失败。那时 `is_alive` 仍为真，不退避就是一个满速重连的
        // 死循环：烧满一个核、往宿主的 stdout 灌 `nodeUnavailable`、并且每转一圈都往
        // 常驻节点的订阅者表里塞一个新 `Watcher`（那张表只在下一次广播时才清理）。
        //
        // 重连从来不急：节点关停之后要过一会儿才会回来。
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    /// 发一条本进程自己造的事件（基线、节点不可用）。
    fn deliver(&mut self, emit: &mut impl FnMut(Value), event: WatchEvent) {
        match serde_json::to_value(&event) {
            Ok(value) => {
                emit(event::stamp(self.seq, value));
                self.seq += 1;
            }
            // 纯数据 DTO，序列化失败只可能来自本端 bug。诊断走 stderr，不污染这条流。
            Err(err) => eprintln!("序列化订阅事件失败: {err}"),
        }
    }

    /// 订阅建立那一刻的基线，`None` = 交给常驻节点给。
    ///
    /// 有常驻节点时由**它**给——只有它拿得到在线状态与正在传的实时进度，而那是订阅接上
    /// 之后的第一帧。这一句 `is_alive` 就是两条来源的全部分界，其余共用一份装配。
    async fn initial_baseline(&self) -> CliResult<Option<event::Baseline>> {
        if ipc::is_alive(&self.data_dir.socket()).await {
            return Ok(None);
        }
        local_baseline(&self.data_dir, self.inbox_limit)
            .await
            .map(Some)
    }
}

/// 无常驻节点时的基线：直读本机记录。
///
/// 在线状态一律未知（本机没做过任何探测），进度就用库里那份——**没有节点就没有正在跑的
/// actor**，也就没有「库里那份是陈旧的」这回事。
async fn local_baseline(data_dir: &DataDir, inbox_limit: u32) -> CliResult<event::Baseline> {
    // `Records` 在本函数结束时释放，连带关掉数据库连接：节点随时可能起来，
    // 而它与常驻节点抢的是同一把 SQLite 写锁（判据见 `runtime::access`）。
    let records = Records::new(data_dir.clone());
    let devices = event::record_entries(&records.paired_devices().await?);
    let store = records.transfers().await?;
    baseline::build(&*store, Source::Records { devices }, inbox_limit).await
}
