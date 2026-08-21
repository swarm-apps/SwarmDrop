//! 单实例仲裁：同一份数据目录在任一时刻最多一个进程持有运行中的节点。
//!
//! 这是**正确性要求而非优化**：同一设备身份被两个进程同时用于上线，会让对端的可达性
//! 记录互相覆盖、中继预留互相顶替，且两个进程会并发写同一份已配对设备表。
//!
//! ## 两段：通道用于发现，文件锁用于仲裁
//!
//! ```text
//!   通道能连上 ────────────────────────────▶ 有活节点，走 IPC
//!        │ 连不上
//!        ▼
//!   判为陈旧残留 ──▶ 抢文件锁 ──┬─ 拿到 ──▶ 清理残留，我来持有节点
//!                              └─ 没拿到 ──▶ 有并发者正在启动，回到第一步重连
//! ```
//!
//! **两段缺一不可**。只有通道判定时，两个进程可能同时判出「陈旧」然后一起启动；
//! 只有文件锁时，无法区分「锁没人拿」与「持锁进程已崩溃」——而崩溃留下的锁文件本身
//! 是无害的，真正要判的是那个节点还在不在。
//!
//! 文件锁用**标准库**的 [`std::fs::File::try_lock`]（1.89 起稳定），不引第三方 crate：
//! 这里要的就是它提供的那点语义——非阻塞地拿一把随进程退出自动释放的独占锁。

use std::time::Duration;

use std::fs::TryLockError;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};

use super::ipc;

/// 仲裁结果。
pub enum Acquisition {
    /// 本进程持有节点。锁随 [`NodeLock`] 的生命周期释放。
    Owner(NodeLock),
    /// 已有活节点在运行，本进程应经通道复用它。
    Existing,
}

/// 节点持有权。
///
/// `Drop` 时释放锁并清理通道文件——**必须在节点关停之后**再 drop 它，否则下一个进程
/// 会在旧节点还活着时拿到锁。
pub struct NodeLock {
    file: std::fs::File,
    /// 持有整个数据目录而不只是通道路径：清理残留是**它**的方法
    /// （见 [`DataDir::clear_stale_channel`]），那条平台差异不该在这一层再写一遍。
    data_dir: DataDir,
}

impl Drop for NodeLock {
    fn drop(&mut self) {
        self.data_dir.clear_stale_channel();
        let _ = self.file.unlock();
    }
}

/// 争用时的重试间隔与次数。
///
/// 只覆盖「另一个进程正在启动」这个窄窗口：它拿着锁、通道还没建好，而我们已经判过
/// 一次陈旧。等的是它把通道建起来，通常是毫秒级。
const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const RETRY_LIMIT: usize = 40; // 合计约 2 秒

/// 判定本进程应当持有节点，还是复用已有的那个。
pub async fn acquire(data_dir: &DataDir) -> CliResult<Acquisition> {
    let socket_path = data_dir.socket();

    for _ in 0..RETRY_LIMIT {
        // 第一段：发现。
        if ipc::is_alive(&socket_path).await {
            return Ok(Acquisition::Existing);
        }

        // 第二段：仲裁。
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(data_dir.lock())
            .map_err(|err| CliError::NodeUnavailable(format!("打开锁文件失败: {err}")))?;

        match file.try_lock() {
            Ok(()) => {
                // 拿到锁：此刻通道要么不存在，要么是崩溃留下的残留——两种都该清掉，
                // 否则监听器会因「地址已占用」起不来。
                data_dir.clear_stale_channel();
                return Ok(Acquisition::Owner(NodeLock {
                    file,
                    data_dir: data_dir.clone(),
                }));
            }
            Err(TryLockError::WouldBlock) => {
                // 有并发者正持锁启动。等它把通道建起来，然后走复用路径。
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
            Err(TryLockError::Error(err)) => {
                return Err(CliError::NodeUnavailable(format!(
                    "获取单实例锁失败: {err}"
                )));
            }
        }
    }

    Err(CliError::NodeUnavailable(
        "另一个 swarmdrop 进程正在启动，等待超时；若确认没有其他实例，请重试".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data_dir() -> (tempfile::TempDir, DataDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");
        (tmp, dir)
    }

    /// 空目录下第一个进程直接拿到持有权。
    #[tokio::test]
    async fn first_acquirer_becomes_owner() {
        let (_tmp, dir) = temp_data_dir();
        assert!(matches!(
            acquire(&dir).await.unwrap(),
            Acquisition::Owner(_)
        ));
    }

    /// **护栏**：陈旧的通道残留不得阻塞启动。
    ///
    /// 进程被强杀后类 Unix 上会留下一个连不上的套接字文件。若据此判定「有节点在跑」，
    /// 用户会陷入「怎么都起不来、也没有进程可杀」的死局，只能手工删文件。
    /// 只在 Unix：Windows 的通道不是文件，写不出「陈旧残留」这个前提。
    #[cfg(unix)]
    #[tokio::test]
    async fn stale_socket_does_not_block_startup() {
        let (_tmp, dir) = temp_data_dir();
        std::fs::write(dir.socket(), b"stale").unwrap();

        assert!(matches!(
            acquire(&dir).await.unwrap(),
            Acquisition::Owner(_)
        ));
        assert!(!dir.socket().exists(), "拿到持有权时应清理陈旧残留");
    }

    /// **护栏**：持有权释放后，下一个进程才能拿到。
    #[tokio::test]
    async fn ownership_is_released_on_drop() {
        let (_tmp, dir) = temp_data_dir();

        let owner = acquire(&dir).await.unwrap();
        drop(owner);

        assert!(matches!(
            acquire(&dir).await.unwrap(),
            Acquisition::Owner(_)
        ));
    }

    /// **护栏**：持锁期间的并发争用不会产生第二个持有者。
    ///
    /// 这是「两个进程同时判定陈旧」那个竞态的直接回归——只有文件锁能仲裁它。
    /// 此处持锁者不建通道（模拟「正在启动」的窗口），故争用者最终以超时失败；
    /// 关键断言是**它没有也拿到持有权**。
    #[tokio::test]
    async fn concurrent_acquire_never_yields_two_owners() {
        let (_tmp, dir) = temp_data_dir();
        let _owner = acquire(&dir).await.unwrap();

        match acquire(&dir).await {
            Err(_) => {}
            Ok(Acquisition::Existing) => panic!("持锁者尚未建立通道，不该判为可复用"),
            Ok(Acquisition::Owner(_)) => panic!("同一数据目录出现两个持有者"),
        }
    }
}
