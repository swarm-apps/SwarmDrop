//! libp2p WebRTC 传输，两种模式。
//!
//! | 模式 | 地址 | 场景 |
//! |---|---|---|
//! | **打洞** | `<relayed-multiaddr>/webrtc/p2p/…` | 双方**都**不可 listen（浏览器 / NAT 后） |
//! | **direct** | `/ip4/…/udp/…/webrtc-direct/certhash/…` | 一端有可达地址（公网 / 同网段） |
//!
//! 打洞实现 libp2p spec [`webrtc/webrtc.md`] 的 `/webrtc-signaling/0.0.1`（spec 里称
//! private-to-private，对应 js-libp2p 的 `webRTC()`）；direct 实现
//! [`webrtc/webrtc-direct.md`]，是官方 `libp2p-webrtc` / `libp2p-webrtc-websys` 的
//! **完整替代品**——两者都基于 webrtc-rs 但版本不同（0.20 vs 0.17），并存等于把整套
//! ICE/DTLS/SCTP 编译两遍。
//!
//! [`webrtc/webrtc.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc.md
//! [`webrtc/webrtc-direct.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc-direct.md
//!
//! # 两种模式的分水岭：证书指纹经什么信道来
//!
//! | | 打洞 | direct |
//! |---|---|---|
//! | 指纹信道 | **已认证的** relay 连接 | multiaddr——**不可信** |
//! | 身份认证 | DTLS 指纹绑定即可 | **必须再跑一次 Noise** |
//! | ICE | 完整，双向收候选 | 服务端 ICE-lite，只被动应答 |
//! | 信令 | `/webrtc-signaling/0.0.1` | 无——SDP 由 multiaddr 确定性构造 |
//!
//! 打洞不需要额外握手：SDP 里含 DTLS 证书指纹，而 SDP 经已认证的 relay 连接传输，
//! DTLS 握手会验证它，身份因此被绑定（spec FAQ 第一条）。
//!
//! ⚠️ 这把「relay 连接必须是认证的」变成**安全前提**而非实现细节。libp2p 的 relay
//! 连接本身经 Noise/TLS 认证，天然满足；但若将来允许经未认证信道传信令，整个模型会塌。
//!
//! direct 的 certhash 则可能经任何信道传播（贴在网页上、印在二维码里），所以那条路径
//! 上的 Noise 握手是**不能省的**——DTLS 只证明「对面持有这张证书」，证明不了「这张
//! 证书属于那个 PeerId」。
//!
//! # 对称性：spec 的 MUST，不是可选优化（打洞模式）
//!
//! spec 步骤 4 原文：*"A MUST as well be able to handle an incoming signaling protocol
//! stream to support the case where B initiates the signaling process."*
//!
//! 即**每一侧都要能 offer 也能 answer**。「A 发起」只是防止双方同时发起而建出两条连接的
//! 约定，不是能力划分。本 crate 的两个 target 特化必须都实现完整的双向能力——
//! 上游 PR #5978 只做了浏览器侧，因此只覆盖 web↔web，拿不到 web↔NAT 后原生端。
//!
//! # 分层
//!
//! 依赖方向单向：`swarm` → `backend` → `protocol`，下层不反向引用上层。
//!
//! | 层 | 职责 | 依赖 libp2p-swarm |
//! |---|---|---|
//! | [`protocol`] | 线上格式：消息编解码、framed codec、两种模式的地址约定 | 否 |
//! | [`backend`] | WebRTC 栈抽象；native / wasm 各自特化 | 否 |
//! | [`swarm`] | 接到 `Transport` / `NetworkBehaviour` 两个平面 | 是 |
//!
//! 三层里唯一与具体 WebRTC 实现绑定的只有 [`backend`]；状态机
//! （[`swarm::session`]）与协议层都是纯逻辑，可脱离真实 WebRTC 与真实 `Stream` 测试。
//!
//! direct 模式没有信令，因而不涉及 `session` / `Behaviour`——它在 [`backend`] 里闭环，
//! 由 [`Transport`] 按地址段分派过去。

pub mod backend;
mod config;
pub mod error;
pub mod protocol;
pub mod swarm;

/// direct 模式的 DTLS 证书（native）。
///
/// 宿主用它生成并持久化证书：`Certificate::generate()?.serialize_pem()` 存盘，
/// 下次启动 `DirectConfig::with_certificate_pem(pem)` 加载回来。**必须持久化**，
/// 否则通告地址里的 certhash 每次重启都变。
#[cfg(not(target_family = "wasm"))]
pub use backend::native::direct::Certificate;
pub use backend::{Backend, BackendError, BackendEvent, Factory};
pub use config::{Config, DirectConfig};
pub use error::Error;
pub use protocol::{Message, MessageType, SIGNALING_PROTOCOL};
pub use swarm::{Behaviour, Connection, Event, Transport};

/// 创建配对的 [`Transport`] 与 [`Behaviour`]。
///
/// 两者**必须注册进同一个 Swarm**：transport 的建连过程要借 behaviour 在 relay 连接上
/// 开信令流（原委见 [`swarm`] 模块）。只注册其一时，dial 会以
/// [`Error::BehaviourDetached`] 快速失败，而不是静默挂起。
///
/// `factory` 为每条信令流创建一个 [`Backend`]。之所以由调用方注入而非内置，是因为两个
/// target 的 WebRTC 栈毫无共同点（native = `webrtc-rs`，wasm = 浏览器
/// `RTCPeerConnection`），且这样状态机可以脱离真实 WebRTC 被测试。
///
/// ```no_run
/// use std::sync::Arc;
/// # use webrtc_p2p::{Backend, BackendError, Config};
/// # fn make_backend() -> Result<Box<dyn Backend>, BackendError> { unimplemented!() }
/// let (transport, behaviour) = webrtc_p2p::new(
///     Config::default(),
///     Arc::new(|_cfg: &Config| make_backend()),
/// );
/// // transport 经 `with_other_transport` 接入，behaviour 放进你的 NetworkBehaviour 派生结构
/// # let _ = (transport, behaviour);
/// ```
pub fn new(config: Config, factory: Factory) -> (Transport, Behaviour) {
    let (transport_side, behaviour_side) = swarm::channel::pair();
    (
        Transport::new(config.clone(), transport_side),
        Behaviour::new(config, factory, behaviour_side),
    )
}
