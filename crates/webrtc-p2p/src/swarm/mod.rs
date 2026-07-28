//! libp2p 集成层：把信令会话接到 `Transport` / `NetworkBehaviour` 两个平面上。
//!
//! # 为什么要分成四块
//!
//! libp2p 把「拨号」与「协议流」放在互不相通的两个平面，而本传输的建连需要**先有一条
//! 已建立的连接**才能换 SDP。于是职责被迫拆开，各块的边界是：
//!
//! | 模块 | 职责 | 依赖 libp2p 吗 |
//! |---|---|---|
//! | [`session`] | 信令会话状态机——收到什么就该做什么 | **否**（纯逻辑，可独立测试） |
//! | [`handler`] | 把 session 接到 `ConnectionHandler` 的 poll 模型上 | 是 |
//! | [`behaviour`] | 连接管理、拨号记账、与 transport 的往返 | 是 |
//! | [`transport`] | `Transport` trait 实现，地址分派 | 是 |
//!
//! **[`session`] 与 libp2p 解耦是刻意的**：状态机是最容易出错的部分，把它从 poll 适配里
//! 摘出来后，可以用同步的方式逐步驱动、逐条断言，不必构造真实的 `Stream`。

pub mod behaviour;
pub(crate) mod channel;
pub mod connection;
pub mod handler;
pub mod session;
pub mod transport;

pub use behaviour::{Behaviour, Event};
pub use connection::Connection;
pub use transport::Transport;
