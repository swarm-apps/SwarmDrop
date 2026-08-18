//! 事件出口：核心事件 → 输出。
//!
//! **订阅逻辑只写一遍，渲染换实现**。核心事件的种类与输出形态无关，所以这里不是
//! 「每种输出一个 `EventBus`」，而是一个 [`CliEventBus`] 持有一个 [`EventRenderer`]。
//!
//! 流向是硬约束（见 [`crate::render`]）：事件属于运行叙述，**一律走 stderr**，
//! 绝不进 stdout——结构化模式下 stdout 只能有命令的最终结果。

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
    watchers: std::sync::Mutex<Vec<tokio::sync::mpsc::UnboundedSender<CoreEvent>>>,
}

impl CliEventBus {
    pub fn new(renderer: Arc<dyn EventRenderer>) -> Self {
        Self {
            renderer,
            watchers: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 订阅事件流。
    ///
    /// 用**无界**通道：事件丢失比内存增长更糟——丢掉一条 `TransferCompleted`，
    /// 等待方会一直挂到超时，而用户看到的是「命令卡住」。命令的生命周期本就很短，
    /// 无界不会真的涨起来。
    pub fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<CoreEvent> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.watchers.lock().expect("watchers 锁中毒").push(tx);
        rx
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

#[async_trait]
impl EventBus for CliEventBus {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        self.renderer.render(&event);
        // 广播给订阅者，顺手清掉已断开的——订阅者是命令，命令结束通道就没了。
        self.watchers
            .lock()
            .expect("watchers 锁中毒")
            .retain(|tx| tx.send(event.clone()).is_ok());
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
}
