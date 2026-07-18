//! 状态观察：[`Watcher`]（last-value-wins 采样，非事件流）。
//!
//! 包一层 `tokio::sync::watch::Receiver`（wasm 安全：watch 是纯用户态原语），
//! 不直接暴露 tokio 类型。语义要点：
//! - **采样，不是队列**——两次读取之间的中间值会被覆盖跳过；
//! - 「每个事件必达」的场景（presence 的断连边沿等）用
//!   [`Events`](crate::Events) 事件流，不要用 Watcher 统计状态翻转次数。

use futures::Stream;

/// 某项内核状态的观察句柄。
///
/// 由 `Endpoint::watch_*` 系列方法返回。`get()` 同步读当前值；
/// `updated()` 等待下一次变更；`stream()` 转成「先当前值、后变更」的流。
#[derive(Debug, Clone)]
pub struct Watcher<T> {
    rx: tokio::sync::watch::Receiver<T>,
}

impl<T: Clone> Watcher<T> {
    pub(crate) fn new(rx: tokio::sync::watch::Receiver<T>) -> Self {
        Self { rx }
    }

    /// 当前值快照。
    pub fn get(&self) -> T {
        self.rx.borrow().clone()
    }

    /// 在闭包内借用当前值（`get()` 的零拷贝版：只需读局部字段时免整值深拷贝）。
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.rx.borrow())
    }

    /// 等待下一次变更并返回新值。
    ///
    /// Endpoint 关闭（写端 drop）后返回 `None`。
    pub async fn updated(&mut self) -> Option<T> {
        self.rx.changed().await.ok()?;
        Some(self.rx.borrow_and_update().clone())
    }

    /// 转成流：首项是当前值，之后每次变更产出一项，Endpoint 关闭后结束。
    pub fn stream(mut self) -> impl Stream<Item = T> + Send
    where
        T: Send + Sync + 'static,
    {
        // 只标记一次：让首次 updated() 立即返回当前值
        self.rx.mark_changed();
        futures::stream::unfold(Some(self), |state| async move {
            let mut watcher = state?;
            let value = watcher.updated().await?;
            Some((value, Some(watcher)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_and_updated() {
        let (tx, rx) = tokio::sync::watch::channel(0u32);
        let mut w = Watcher::new(rx);
        assert_eq!(w.get(), 0);

        tx.send(1).unwrap();
        assert_eq!(w.updated().await, Some(1));

        drop(tx);
        assert_eq!(w.updated().await, None, "写端 drop 后 updated 返回 None");
    }

    #[tokio::test]
    async fn watcher_is_sampling_not_queue() {
        let (tx, rx) = tokio::sync::watch::channel(0u32);
        let mut w = Watcher::new(rx);
        // 连发三次，只看得到最后一个（last-value-wins）
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        assert_eq!(w.updated().await, Some(3));
    }

    // 曾修过「每轮 mark_changed 导致无限产出当前值」的 bug——语义固化：
    // 首项 = 当前值（且只产一次），之后每次变更一项，写端 drop 后流结束。
    #[tokio::test]
    async fn stream_emits_current_then_changes_then_ends() {
        use futures::StreamExt;

        let (tx, rx) = tokio::sync::watch::channel(10u32);
        let mut s = Box::pin(Watcher::new(rx).stream());

        // 首项立即返回当前值，不等任何变更
        assert_eq!(s.next().await, Some(10));

        // 无变更时必须挂起（不会重复产出当前值）
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), s.next())
                .await
                .is_err(),
            "无变更时 stream 不得重复产出当前值"
        );

        tx.send(11).unwrap();
        assert_eq!(s.next().await, Some(11));

        drop(tx);
        assert_eq!(s.next().await, None, "写端 drop 后流结束");
    }
}
