//! 事件出口：核心事件 → 输出。
//!
//! **订阅逻辑只写一遍，渲染换实现**。核心事件的种类与输出形态无关，所以这里不是
//! 「每种输出一个 `EventBus`」，而是一个 [`CliEventBus`] 持有一个 [`EventRenderer`]。
//!
//! 流向是硬约束（见 [`crate::render`]）：本模块**渲染**出去的事件属于运行叙述，
//! 一律走 stderr——结构化模式下 stdout 只能有命令的最终结果。
//!
//! ⚠️ 这条约束管的是[`EventRenderer`]，**不是「事件绝不进 stdout」**。`swarmdrop watch`
//! 的整条 stdout 就是事件流（NDJSON），但那是它的**命令结果**，经
//! [`Self::subscribe_lossy`](CliEventBus::subscribe_lossy) 拿到、翻译成订阅面自己的窄
//! 结构之后输出的，与这里的渲染器无关。两件事只是名字撞了。

use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_host::AppResult;

/// 核心事件的呈现方式。
pub trait EventRenderer: Send + Sync {
    fn render(&self, event: &CoreEvent);
}

/// 结构化日志渲染：常驻模式（`start`）用。
///
/// 走 `tracing` 而不是直接 `eprintln!`，因此能被 `RUST_LOG` 调节，也能被服务管理器
/// 按级别收集。
#[derive(Debug, Default)]
pub struct LogRenderer;

impl EventRenderer for LogRenderer {
    fn render(&self, event: &CoreEvent) {
        // 事件是个 `#[non_exhaustive]` 的大枚举，且新增变体是常态。这里刻意**不**逐变体
        // 匹配：一个只为打日志而存在的 match 会在每次上游加变体时变成一处编译错误，
        // 而它并不承载任何判断。需要区分变体的地方（审批、提问）在各自的命令里处理。
        tracing::info!(?event, "core event");
    }
}

/// 静默渲染：结构化输出模式用。
///
/// 调用方是程序，运行叙述对它没有意义；真正需要的信息在命令的最终结果里。
#[derive(Debug, Default)]
pub struct QuietRenderer;

impl EventRenderer for QuietRenderer {
    fn render(&self, _event: &CoreEvent) {}
}

/// [`EventBus`] 的命令行实现。
///
/// 除渲染外还支持**订阅**：一次性命令（如发送）需要等待自己关心的终态事件，而核心侧
/// 只有这一个事件出口。订阅者是有界的（每条命令一个），断开的订阅在下次广播时清理。
pub struct CliEventBus {
    renderer: Arc<dyn EventRenderer>,
    watchers: std::sync::Mutex<Vec<Watcher>>,
}

impl CliEventBus {
    pub fn new(renderer: Arc<dyn EventRenderer>) -> Self {
        Self {
            renderer,
            watchers: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 订阅事件流，**一条都不丢**。
    ///
    /// 用无界通道：这些订阅者丢一条就是功能缺失——`spawn_auto_accept` 丢一条入站 offer
    /// 等于漏收一个文件，`ProgressCache` 丢一条终态等于进度永远停在中途。它们的消费都是
    /// 常数时间（一次 HashMap 写 / 一次 accept 调度），积压不起来。
    ///
    /// ⚠️ **给长驻且可能读得慢的消费者用 [`Self::subscribe_lossy`]。** 这里此前的注释写着
    /// 「命令的生命周期本就很短，无界不会真的涨起来」——那句话在 `ProgressCache::spawn` 与
    /// `spawn_auto_accept` 这两个随节点存活的订阅者出现之后就不完全成立了，它们只是消费得
    /// 足够快而已。真正读得慢的消费者（把事件转发给外部进程的订阅面）不能走这条。
    pub fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<CoreEvent> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.watchers
            .lock()
            .expect("watchers 锁中毒")
            .push(Watcher::Unbounded(tx));
        rx
    }

    /// 订阅事件流，**队列满时丢弃并计数**。
    ///
    /// 给「消费方在进程外、读得可能很慢」的订阅面用（`swarmdrop watch`）。
    ///
    /// ## 为什么必须是有界且非阻塞
    ///
    /// `publish` 被 await 在传输的收发块簿记里。走阻塞式背压（`send().await`）意味着一个
    /// 读得慢的**旁观者**能把正在进行的传输拖慢——那条回路的终点不在本程序手里，它永远
    /// 闭合不了。所以这里 `try_send`：满了就丢，代价局限在那条订阅上。
    ///
    /// ## 为什么返回丢弃计数而不是静默丢
    ///
    /// 订阅面要把丢弃如实告诉消费方（spec: `cli-event-stream` 的「边沿事件不得静默丢失」）。
    /// 消费方会把事件持久化进跨月留存的记录，一个无声的洞比一次诚实的截断难查得多。
    ///
    /// **也不用 `tokio::sync::broadcast`**：它的滞后语义只告诉你跳了几条、不告诉你跳掉
    /// 哪几条，而且全仓零先例。
    ///
    /// ## `report_loss` 为什么必须由订阅方给
    ///
    /// 不是每一条丢弃都值得上报：进度是**采样**，下一帧会纠正它，把它算进截断计数只会
    /// 让一次正常的降压长得像一次数据损失。而哪些算采样是**订阅面的语义**，不是事件
    /// 总线的——本模块只认「有人要、队列满了」。判据的归属地在
    /// [`crate::runtime::watch::event::report_loss`]。
    pub fn subscribe_lossy(
        &self,
        capacity: usize,
        report_loss: fn(&CoreEvent) -> bool,
    ) -> (tokio::sync::mpsc::Receiver<CoreEvent>, DropCount) {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        let dropped = DropCount::default();
        self.watchers
            .lock()
            .expect("watchers 锁中毒")
            .push(Watcher::Bounded {
                tx,
                dropped: dropped.clone(),
                report_loss,
            });
        (rx, dropped)
    }

    /// 按输出模式选渲染器。
    pub fn for_mode(json: bool) -> Self {
        let renderer: Arc<dyn EventRenderer> = if json {
            Arc::new(QuietRenderer)
        } else {
            Arc::new(LogRenderer)
        };
        Self::new(renderer)
    }
}

/// 一个订阅者的投递端。
enum Watcher {
    /// 一条都不丢（[`CliEventBus::subscribe`]）。
    Unbounded(tokio::sync::mpsc::UnboundedSender<CoreEvent>),
    /// 满了就丢；值得上报的那些还要计数（[`CliEventBus::subscribe_lossy`]）。
    Bounded {
        tx: tokio::sync::mpsc::Sender<CoreEvent>,
        dropped: DropCount,
        /// 丢掉这一条要不要计进 `dropped`。
        report_loss: fn(&CoreEvent) -> bool,
    },
}

impl Watcher {
    /// 投递一条；返回 `false` 表示这个订阅者已经走了，应当从表里清掉。
    ///
    /// **队列满不算走了**：订阅者还在，只是读得慢——丢这一条、记一笔，下一条继续送。
    fn deliver(&self, event: &CoreEvent) -> bool {
        match self {
            Self::Unbounded(tx) => tx.send(event.clone()).is_ok(),
            // **先占位再克隆**，不是 `try_send(event.clone())`：后者在队列满时会白白克隆
            // 一整个事件再扔掉，而这条路径上最密的正是 `TransferProgress`——它带着整个
            // 文件向量，几万文件的目录传输时那一次克隆就是几 MB 的纯 memcpy，且发生在
            // 传输的收发块簿记里（`publish` 被 await 在那儿）。队列满恰恰是最忙的时候。
            Self::Bounded {
                tx,
                dropped,
                report_loss,
            } => match tx.try_reserve() {
                Ok(permit) => {
                    permit.send(event.clone());
                    true
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(())) => {
                    if report_loss(event) {
                        dropped.record();
                    }
                    true
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => false,
            },
        }
    }
}

/// 一条有界订阅上累计丢弃了多少条。
///
/// 由订阅面读走并清零，转成一条**显式的截断事件**告诉消费方——不是内部指标。
#[derive(Clone, Default)]
pub struct DropCount(Arc<std::sync::atomic::AtomicUsize>);

impl DropCount {
    fn record(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 取走累计值并清零。
    pub fn take(&self) -> usize {
        self.0.swap(0, std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl EventBus for CliEventBus {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        self.renderer.render(&event);
        // 广播给订阅者，顺手清掉已断开的——订阅者是命令，命令结束通道就没了。
        self.watchers
            .lock()
            .expect("watchers 锁中毒")
            .retain(|w| w.deliver(&event));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct CountingRenderer(AtomicUsize);

    impl EventRenderer for CountingRenderer {
        fn render(&self, _event: &CoreEvent) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 每个事件恰好渲染一次——重复渲染在常驻模式下会把日志翻倍。
    #[tokio::test]
    async fn each_event_renders_once() {
        let renderer = Arc::new(CountingRenderer::default());
        let bus = CliEventBus::new(renderer.clone());

        bus.publish(CoreEvent::NetworkStatusChanged {
            status: Default::default(),
        })
        .await
        .unwrap();

        assert_eq!(renderer.0.load(Ordering::Relaxed), 1);
    }

    /// 结构化模式必须选到静默渲染器：事件写进 stderr 不影响解析，但会淹没调用方的日志。
    #[tokio::test]
    async fn json_mode_is_silent() {
        let bus = CliEventBus::for_mode(true);
        // 静默渲染器不产生任何输出，只断言它不 panic 且返回 Ok。
        bus.publish(CoreEvent::NetworkStatusChanged {
            status: Default::default(),
        })
        .await
        .unwrap();
    }

    fn an_event() -> CoreEvent {
        CoreEvent::Error {
            message: "x".into(),
        }
    }

    /// **一个读得慢的订阅者不得拖住 `publish`。**
    ///
    /// 这条看守的正是有界订阅存在的理由：`publish` 被 await 在传输的收发块簿记里，
    /// 走阻塞式背压意味着一个旁观者能把正在进行的传输拖慢。队列满了照样立刻返回。
    #[tokio::test]
    async fn a_stalled_subscriber_never_blocks_publish() {
        let bus = CliEventBus::new(Arc::new(QuietRenderer));
        let (_rx, dropped) = bus.subscribe_lossy(2, |_| true);

        // 队列容量 2，发 10 条——多出来的 8 条必须被丢掉而不是让这里挂住。
        for _ in 0..10 {
            tokio::time::timeout(std::time::Duration::from_secs(1), bus.publish(an_event()))
                .await
                .expect("publish 被慢订阅者拖住了")
                .expect("publish 失败");
        }
        assert_eq!(dropped.take(), 8);
        assert_eq!(dropped.take(), 0, "取走之后必须清零");
    }

    /// **只有值得上报的丢弃才计数。**
    ///
    /// 采样类事件（进度）丢了不留痕迹——把它算进截断计数会让一次正常的降压长得像一次
    /// 数据损失，而消费方据此判断自己的记录是不是完整的。
    #[tokio::test]
    async fn losses_the_subscriber_does_not_care_about_are_not_counted() {
        let bus = CliEventBus::new(Arc::new(QuietRenderer));
        let (_rx, dropped) = bus.subscribe_lossy(1, |_| false);

        for _ in 0..5 {
            bus.publish(an_event()).await.expect("publish 失败");
        }
        assert_eq!(dropped.take(), 0);
    }

    /// 订阅者走了要从表里清掉，否则每次广播都为一条死通道克隆一遍事件。
    #[tokio::test]
    async fn a_departed_subscriber_is_dropped_from_the_table() {
        let bus = CliEventBus::new(Arc::new(QuietRenderer));
        let (rx, _dropped) = bus.subscribe_lossy(4, |_| true);
        drop(rx);

        bus.publish(an_event()).await.expect("publish 失败");
        assert!(bus.watchers.lock().unwrap().is_empty());
    }
}
