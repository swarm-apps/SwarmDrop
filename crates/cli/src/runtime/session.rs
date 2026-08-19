//! 命令与节点之间的会话：要么本进程持有一个节点，要么复用已经在运行的那个。
//!
//! 一次性命令都经 [`Session::open`] 取得节点，因此「有常驻就复用、没有就起临时的」
//! 这条规则只写一遍。命令层不需要知道自己拿到的是哪一种。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};

use super::boot::{RunningNode, boot};
use super::ipc;
use super::single::{Acquisition, NodeLock};

/// 一次命令期间可用的节点。
pub enum Session {
    /// 本进程起的**临时节点**：命令结束即销毁，不改变「用户是否希望节点常驻」的意图。
    Temporary {
        /// 装箱：两个变体的体量差得远（一个是完整节点，一个只是路径），
        /// 不装箱会让每个 `Session` 值都按大的那个占位。
        node: Box<RunningNode>,
        /// 持有权。**必须活到节点关停之后**才 drop，否则下一个进程会在旧节点还在时拿到锁。
        lock: NodeLock,
    },
    /// 复用正在运行的常驻节点。
    Existing { socket: std::path::PathBuf },
}

impl Session {
    /// 为一次性命令取得节点。
    pub async fn open(data_dir: &DataDir, json: bool) -> CliResult<Self> {
        match super::single::acquire(data_dir).await? {
            Acquisition::Existing => Ok(Self::Existing {
                socket: data_dir.socket(),
            }),
            Acquisition::Owner(lock) => {
                let node = boot(data_dir, json).await?;
                Ok(Self::Temporary {
                    node: Box::new(node),
                    lock,
                })
            }
        }
    }

    /// 向常驻节点发一条请求；本进程自持节点时返回 `None`，由调用方走本地路径。
    pub async fn ask(&self, req: &ipc::Request) -> CliResult<Option<ipc::Response>> {
        match self {
            Self::Temporary { .. } => Ok(None),
            Self::Existing { socket } => ipc::request(socket, req).await,
        }
    }

    /// 本进程自持的节点（复用他人节点时为 `None`）。
    pub fn local(&self) -> Option<&RunningNode> {
        match self {
            Self::Temporary { node, .. } => Some(node),
            Self::Existing { .. } => None,
        }
    }

    /// 取本进程自持的节点，没有则报「节点不可用」。
    ///
    /// 每条命令的本地回落分支都要这一句。摊在各命令里各写一遍时它们迟早会各说各的措辞，
    /// 而这句话正是用户在通道意外断开时看到的唯一解释。
    pub fn require_local(&self) -> CliResult<&RunningNode> {
        self.local()
            .ok_or_else(|| CliError::NodeUnavailable("节点不可用".into()))
    }

    /// 命令收尾。
    ///
    /// 临时节点在此关停；复用他人节点时什么都不做——**绝不能顺手把别人的节点关了**。
    pub async fn close(self) {
        if let Self::Temporary { node, lock } = self {
            node.manager.shutdown().await;
            drop(lock); // 显式：锁必须在关停之后释放
        }
    }
}
