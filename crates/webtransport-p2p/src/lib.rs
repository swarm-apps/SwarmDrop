//! libp2p WebTransport 传输（native listener + dialer）。
//!
//! spec: [libp2p/specs 的 `webtransport/README.md`][spec]。
//!
//! `rust-libp2p` 只有 `transports/webtransport-websys`（浏览器侧拨号，wasm），**没有 native
//! listener** —— 浏览器能拨、没人能接。上游 [PR #4348] 是维护者的 native draft，自 2023-10
//! 停在 draft。本 crate 补的正是这个缺口。
//!
//! [spec]: https://github.com/libp2p/specs/blob/master/webtransport/README.md
//! [PR #4348]: https://github.com/libp2p/rust-libp2p/pull/4348
//!
//! # 用法
//!
//! ```no_run
//! use std::sync::Arc;
//! use libp2p_identity::Keypair;
//! use webtransport_p2p::{Config, Transport};
//! # use webtransport_p2p::{CertificateStore, StoreError};
//! # struct FileStore;
//! # impl CertificateStore for FileStore {
//! #     fn load(&self) -> Result<Option<String>, StoreError> { Ok(None) }
//! #     fn store(&self, _pem: &str) -> Result<(), StoreError> { Ok(()) }
//! # }
//! let transport = Transport::new(
//!     Config::new(Keypair::generate_ed25519())
//!         .with_certificate_store(Arc::new(FileStore)),
//! )?;
//! // 经 `with_other_transport` 接进 SwarmBuilder；监听
//! // `/ip4/0.0.0.0/udp/4004/quic-v1/webtransport`。
//! # Ok::<_, webtransport_p2p::Error>(())
//! ```
//!
//! # 这个 crate 的重心不在传输层
//!
//! 与 `webrtc-p2p` 逐条对比，只有最后一行变复杂：
//!
//! | 维度 | `webrtc-p2p` | 本 crate |
//! |---|---|---|
//! | 模式数 | 2（打洞 + direct），打洞要状态机 + `NetworkBehaviour` | 1 —— WebTransport 没有 NAT 穿透 |
//! | 建连协商 | SDP 构造、ICE、DTLS 角色、ufrag 学习 | 无，QUIC 握手是库的事 |
//! | socket 复用 | 自写 1100 行 `UdpMux` | 无，独占端口 |
//! | 子流 | DataChannel + 自做 framing + `init` 通道陷阱 | QUIC 流**本身就是流**，muxer 极薄 |
//! | 后端抽象 | 必须（native / wasm 两套 WebRTC 栈） | 不需要，浏览器侧用上游现成 crate |
//! | 证书 | 一张，**永不改变** | **两张，会过期，14 天轮换，通告地址随之变化** |
//!
//! 那一行引入了 `webrtc-p2p` 里完全不存在的维度：**时间**。因此本 crate 的架构成败在
//! [`certificate`] 这个子系统，不在传输层 —— 传输层基本是把 `wtransport` 的 async API
//! 翻译成 libp2p 的 poll API。
//!
//! # 分层
//!
//! 依赖严格单向，下层永不引用上层。
//!
//! | 层 | 模块 | 依赖 `libp2p-core` | 依赖 `wtransport` | 做 IO |
//! |---|---|---|---|---|
//! | L0 纯逻辑 | [`addr`] | 仅 `Multiaddr` / `Multihash` 类型 | **否** | 否 |
//! | L0 纯逻辑 | [`certificate`] | 仅 `Multihash` | **是** —— `Identity` 当证书容器 | 否 |
//! | L1 libp2p 语义 | `noise` | 是 | **否** —— 泛型于流类型 | 是 |
//! | L1 libp2p 语义 | `muxer` | 是 | **是** —— 绑定它的 `Connection` 与流类型 | 是 |
//! | L2 wtransport 绑定 | `listener` / `dialer` / [`transport`] | 是 | 是 | 是 |
//!
//! **「依赖 wtransport」那一列不是清一色的否，这一点要如实看。** `certificate` 借
//! `wtransport::Identity` 当证书容器（自己重做一份 DER + 私钥的容器没有收益），`muxer` 直接
//! 绑它的流类型（为唯一一个实现造 trait 是 YAGNI）。
//!
//! 真正成立的是这条：**换掉 wtransport 时，决定「行为」的那些部分一行都不用动** ——
//! 轮换状态机（`certificate::Rotation`，全 crate 最复杂的 456 行）、地址解析、Noise 语义
//! 都不认识它。要改的是证书容器层与 `muxer` + L2 三个文件，且每一处都有测试兜着。
//!
//! L1 的 `noise` 泛型于流类型也不只是洁癖：它让 Noise 握手能用内存双工流测，包括那条
//! **必须红过一次**的 certhash 负向用例。
//!
//! # 与 `webrtc-p2p` 的一处**机制互斥**
//!
//! webrtc-direct 用 `libp2p-webrtc-noise:` + 双方 DTLS 指纹作 **Noise prologue**；
//! WebTransport 用 **`webtransport_certhashes` Noise 扩展**，**不设 prologue**。
//! 两者是同一目的的两种机制，照抄隔壁 crate 会让握手在第一条消息就失败，而症状看起来
//! 像「Noise 实现有 bug」。细节见 `noise` 模块文档。
//!
//! # 已知边界
//!
//! - **不与 libp2p-quic 共用 UDP 端口。** wtransport 不接受已绑定的 socket，且 QUIC 用
//!   libp2p TLS 扩展证书、WebTransport 用普通自签名证书，rustls 配置本就不同。这正是上游
//!   PR #4348 卡住的地方。
//! - **不解决通告地址的稳定性。** 地址随证书轮换而变（实际寿命 28 天，推导见
//!   [`certificate::Rotation`]），因此不适合独自承担「第一个联系点」的角色。
//! - **native-only。** 浏览器侧用上游的 `libp2p-webtransport-websys`。

pub mod addr;
pub mod certificate;
mod config;
mod dialer;
mod error;
mod listener;
mod muxer;
mod noise;
mod transport;

pub use addr::{Certhash, DialAddr};
pub use config::{
    CertificateStore, Config, DEFAULT_HANDSHAKE_TIMEOUT, MemoryCertificateStore,
    ROTATION_CHECK_INTERVAL, StoreError,
};
pub use error::{Error, PeerIdMismatch};
pub use muxer::{Muxer, Substream};
pub use transport::{Transport, Upgrade};
