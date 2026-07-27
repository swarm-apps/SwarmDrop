//! libp2p WebRTC 打洞传输。
//!
//! 让两个都无法 listen 的节点（浏览器、NAT 后的原生端）经 relay 交换信令、打洞建立直连。
//! 实现 libp2p spec [`webrtc/webrtc.md`] 的 `/webrtc-signaling/0.0.1`（spec 里称这个场景为
//! private-to-private，对应 js-libp2p 的 `webRTC()`）。
//!
//! [`webrtc/webrtc.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc.md
//!
//! # 与官方 `libp2p-webrtc` 的关系
//!
//! 是**两个不同的传输**，可共存：
//!
//! | | 官方 `libp2p-webrtc`（webrtc-direct） | 本 crate |
//! |---|---|---|
//! | 场景 | browser → **有公网可达地址**的 server | 双方**都**不可达（浏览器 / NAT 后） |
//! | ICE | ICE-lite，服务端只被动应答 | 完整 ICE，双向收集候选 + 打洞 |
//! | 信令 | 无——SDP 由 multiaddr 确定性构造 | `/webrtc-signaling/0.0.1`，经 relay |
//! | 地址 | `/ip4/…/udp/…/webrtc-direct/certhash/…` | `<relayed-multiaddr>/webrtc/p2p/…` |
//! | 证书指纹信任 | **不信任**（可经非认证信道传播） | **信任**（经已认证的 relay 连接交换） |
//!
//! # 为什么不需要额外的 Noise 握手
//!
//! SDP 里含 DTLS 证书指纹，而 SDP 经**已认证的** relay 连接传输；DTLS 握手会验证该指纹。
//! 身份因此被绑定，无需在 DataChannel 之上再叠一层握手（spec FAQ 第一条）。
//!
//! ⚠️ 这条把「relay 连接必须是认证的」变成**安全前提**而非实现细节。libp2p 的 relay
//! 连接本身经 Noise/TLS 认证，天然满足；但若将来允许经未认证信道传信令，整个模型会塌。
//!
//! # 对称性：spec 的 MUST，不是可选优化
//!
//! spec 步骤 4 原文：*"A MUST as well be able to handle an incoming signaling protocol
//! stream to support the case where B initiates the signaling process."*
//!
//! 即**每一侧都要能 offer 也能 answer**。「A 发起」只是防止双方同时发起而建出两条连接的
//! 约定，不是能力划分。本 crate 的两个 target 特化必须都实现完整的双向能力——
//! 上游 PR #5978 只做了浏览器侧，因此只覆盖 web↔web，拿不到 web↔NAT 后原生端。
//!
//! # 结构
//!
//! 协议层（本模块 [`signaling`]）与两个 target 后端分离：状态机、SDP 处理、multiaddr
//! 格式全部共享，只有底层 PeerConnection 不同（native = `webrtc-rs`，
//! wasm = 浏览器 `RTCPeerConnection`）。

pub mod addr;
pub mod behaviour;
mod channel;
mod config;
mod connection;
pub mod error;
pub mod signaling;
pub mod transport;

pub use behaviour::{Behaviour, Event};
pub use config::Config;
pub use connection::Connection;
pub use error::Error;
pub use signaling::{Message, MessageType, SIGNALING_PROTOCOL};
pub use transport::Transport;

/// 创建配对的 [`Transport`] 与 [`Behaviour`]。
///
/// 两者**必须注册进同一个 Swarm**：transport 的建连过程要借 behaviour 在 relay 连接上
/// 开信令流（原委见 [`channel`] 模块）。只注册其一时，dial 会以
/// [`Error::BehaviourDetached`] 快速失败，而不是静默挂起。
///
/// ```no_run
/// # fn main() {
/// let (transport, behaviour) = webrtc_p2p::new(webrtc_p2p::Config::default());
/// // transport 经 `with_other_transport` 接入，behaviour 放进你的 NetworkBehaviour 派生结构
/// # let _ = (transport, behaviour);
/// # }
/// ```
pub fn new(config: Config) -> (Transport, Behaviour) {
    let (transport_side, behaviour_side) = channel::pair();
    (
        Transport::new(config.clone(), transport_side),
        Behaviour::new(config, behaviour_side),
    )
}
