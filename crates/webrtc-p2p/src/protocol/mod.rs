//! 协议层：`/webrtc-signaling/0.0.1` 的线上格式与地址约定。
//!
//! **这一层不依赖 `libp2p-swarm`**，只用到 `libp2p-core` 的 `Multiaddr` 与
//! `libp2p-identity` 的 `PeerId`。好处有二：
//!
//! - 与 spec 对照审阅时不必绕开 libp2p 的 poll 模型
//! - 换宿主（换 libp2p 版本、甚至脱离 libp2p）时这一层原样可用
//!
//! 依赖方向是单向的：`swarm` → `backend` → `protocol`，本层不反向引用任何一层。

pub mod addr;
pub mod codec;
pub mod message;

pub use codec::Codec;
pub use message::{MAX_MESSAGE_LEN, Message, MessageType, SIGNALING_PROTOCOL};
