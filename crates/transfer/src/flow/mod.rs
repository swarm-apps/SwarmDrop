//! 生命周期编排层：`TransferManager` 按阶段拆分的方法（公共结构体在 [`super::manager`]）。
//!
//! - [`prepare`] —— 发送方哈希准备
//! - [`send`]    —— 发送方 Offer / 暂停 / 取消
//! - [`receive`] —— 接收方 accept / reject / 暂停 / 取消 + IncomingTransferRuntime 接收 helper
//! - [`resume`]  —— 双侧断点续传 + IncomingTransferRuntime 续传 helper

pub(crate) mod prepare;
pub(crate) mod receive;
pub(crate) mod resume;
pub(crate) mod send;

use uuid::Uuid;

use crate::manager::TransferManager;
use crate::{AppError, AppResult};

impl TransferManager {
    /// 暂停一条会话，方向由会话自己的记录派生。
    ///
    /// 与 [`pause_send`](TransferManager::pause_send) /
    /// [`pause_receive`](TransferManager::pause_receive) **并存而不是取代**：持有投影的
    /// 调用方（三端 UI）手上已经有 `direction`，为它多查一次库没有意义；而不持有投影的
    /// 调用方——命令行宿主的通道服务端只拿到一串会话标识——需要这个入口，否则那一侧
    /// 就得把「按方向分派」这条规则再实现一遍。
    ///
    /// **不是「先试发送失败再试接收」的试错**：那种写法会把一条真实错误藏进两串拼接的
    /// 文案里，用户看到的是「发送会话不存在；接收会话不存在」而真正的原因是别的。
    /// 这里按 `session.direction` 查表派生，与
    /// [`initiate_resume`](TransferManager::initiate_resume) 同一形态。
    pub async fn pause(&self, session_id: &Uuid) -> AppResult<()> {
        match self.session_direction(session_id).await? {
            entity::TransferDirection::Send => self.pause_send(session_id).await,
            entity::TransferDirection::Receive => self.pause_receive(session_id).await,
        }
    }

    /// 取消一条会话，方向由会话自己的记录派生。理由同 [`Self::pause`]。
    pub async fn cancel(&self, session_id: &Uuid) -> AppResult<()> {
        match self.session_direction(session_id).await? {
            entity::TransferDirection::Send => self.cancel_send(session_id).await,
            entity::TransferDirection::Receive => self.cancel_receive(session_id).await,
        }
    }

    /// 会话的方向。查不到记录时报 `SessionNotFound`——**与两条具体路径同一个分类**，
    /// 于是「这条会话根本不存在」和「它存在但没有活 actor」在调用方那里仍是同一类失败，
    /// 不会因为走了派生入口而变成另一种错误。
    async fn session_direction(&self, session_id: &Uuid) -> AppResult<entity::TransferDirection> {
        self.store
            .find_session(*session_id)
            .await?
            .map(|session| session.direction)
            .ok_or_else(|| AppError::SessionNotFound(format!("传输会话不存在: {session_id}")))
    }
}
