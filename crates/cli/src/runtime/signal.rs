//! 「宿主要我停下」的信号。
//!
//! ## 为什么不能只接 `SIGINT`
//!
//! `tokio::signal::ctrl_c()` 在 Unix 上**只接 `SIGINT`**。而本程序的两类主要调用方都发
//! `SIGTERM`：
//!
//! - **agent harness** 结束子进程（dsh 的 `terminate()`、Node 的 `child.kill()` 默认都是它）
//! - **服务管理器** 停止服务（`systemctl stop` / launchd）
//!
//! 也就是说，只接 `SIGINT` 会让**最常见的正常收摊路径**走不到清理代码，进程被直接杀掉、
//! 退出码非零。而消费方会把非零读作失败并触发重启或告警——一次正常的停止于是长得像一次
//! 崩溃（spec: `cli-event-stream` 的「退出语义」）。
//!
//! ## 为什么是一个「建一次、反复等」的类型，而不是一个 `async fn`
//!
//! ⚠️ **监听器一旦丢弃再重建，那期间到达的信号会被静默吞掉。** tokio 的信号接收端是一个
//! `watch` 订阅：新订阅从**当前版本**起算，看不到自己出生前的那一次通知；而 OS 层的处理
//! 器一经安装就不再卸载，于是默认处置（终止进程）也不会兜底。两头都不接 = 信号消失。
//!
//! 这正是 `select!` 循环最容易写错的形状——把 `signal::shutdown()` 直接写在 `select!` 的
//! 一支里，每转一圈就丢弃并重建一次监听器，落在那个缝隙里的 `SIGTERM` 就没了。表现是
//! 「`systemctl stop` 没反应，等满超时才被 SIGKILL」，而日志里什么都没有。
//!
//! 所以这里给出的是一个**要先建、再反复 `recv`** 的类型：建在循环外是它唯一自然的用法。
//!
//! Windows 上没有 `SIGTERM`；那里 `CtrlC` 覆盖控制台关闭事件，够用。
//!
//! ## ⚠️ 这份实现在本仓有第二处
//!
//! `crates/bootstrap/src/util/signal.rs` 的 `shutdown_signal()` 与本模块同构（只是那边仍是
//! 一次性的 `async fn`，它只在启动路径上等一次，没有循环）。两者没有合并，是因为没有合适
//! 的共享落点：`crates/host` 要过 wasm 门禁（`tokio::signal` 进不去），`crates/host-fs` 的
//! 定位是文件系统端口实现，而让 CLI 依赖整个引导节点 crate 只为二十行平台代码是本末倒置。
//!
//! **改这里就要看一眼那里。** 两处都指着对方，是为了让下一个人发现得了。

/// 停止信号的监听器。
///
/// **建一次，反复 `recv`**——理由见模块文档。
pub struct Shutdown {
    #[cfg(unix)]
    listeners: Vec<tokio::signal::unix::Signal>,
    #[cfg(windows)]
    ctrl_c: Option<tokio::signal::windows::CtrlC>,
}

impl Shutdown {
    /// 装上监听器。
    ///
    /// 装不上**不 panic**：那种失败意味着进程的信号处理已经被别的东西占了，而此时能做的
    /// 最好的事是继续跑，不是拒绝启动。两个信号各装各的，其中一个失败不影响另一个。
    pub fn listen() -> Self {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Self {
                listeners: [SignalKind::interrupt(), SignalKind::terminate()]
                    .into_iter()
                    .filter_map(|kind| signal(kind).ok())
                    .collect(),
            }
        }

        #[cfg(windows)]
        {
            Self {
                ctrl_c: tokio::signal::windows::ctrl_c().ok(),
            }
        }
    }

    /// 等下一个停止信号。
    ///
    /// 一个监听器都没装上时**永不完成**（而不是立刻返回）：立刻返回会让调用方的
    /// `select!` 转成一个满速空转的循环，那比收不到信号糟得多。
    pub async fn recv(&mut self) {
        #[cfg(unix)]
        {
            if self.listeners.is_empty() {
                std::future::pending::<()>().await;
            }
            // 手写 `poll_fn` 而不是 `select!`：数量是运行时决定的（装上几个算几个）。
            std::future::poll_fn(|cx| {
                for listener in &mut self.listeners {
                    if listener.poll_recv(cx).is_ready() {
                        return std::task::Poll::Ready(());
                    }
                }
                std::task::Poll::Pending
            })
            .await;
        }

        #[cfg(windows)]
        match &mut self.ctrl_c {
            Some(ctrl_c) => {
                ctrl_c.recv().await;
            }
            None => std::future::pending::<()>().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **同一个监听器要经得起反复等。**
    ///
    /// 这条钉的是本模块存在的形状：调用方会把它建在循环外、每轮 `select!` 里 `&mut` 一次。
    /// 若 `recv` 只能用一次（比如内部把监听器 `take` 走），第二轮起就再也收不到信号，
    /// 而那**不报错**——只是 `systemctl stop` 没反应。
    #[tokio::test]
    async fn a_listener_survives_being_polled_across_iterations() {
        let mut shutdown = Shutdown::listen();
        for _ in 0..3 {
            // 没有信号到达，所以每一轮都该超时——关键是它不 panic、也不立刻就绪
            // （立刻就绪会让调用方的 select 循环满速空转）。
            let waited =
                tokio::time::timeout(std::time::Duration::from_millis(20), shutdown.recv()).await;
            assert!(waited.is_err(), "没有信号时不该立刻就绪");
        }
    }
}
