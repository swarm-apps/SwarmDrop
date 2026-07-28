//! direct 模式（`/webrtc-direct`）的 native 侧实现。
//!
//! 对应 libp2p spec [`webrtc/webrtc-direct.md`]：一端有可达地址（公网或同网段），
//! 另一端（通常是浏览器）直接拨过去，**没有信令**——SDP 由 multiaddr 确定性构造。
//!
//! [`webrtc/webrtc-direct.md`]: https://github.com/libp2p/specs/blob/master/webrtc/webrtc-direct.md
//!
//! # 与打洞模式的分工
//!
//! 同一个 crate 里两条**互不复用**的建连路径，别为了「少写点代码」把它们合并：
//!
//! | | 打洞（[`super`] 的其余部分） | direct（本模块） |
//! |---|---|---|
//! | ICE | 完整 agent，双向收候选 | **ICE-lite**：服务端只被动应答 |
//! | 信令 | `/webrtc-signaling/0.0.1` 经 relay | 无 |
//! | 指纹信道 | 已认证的 relay 连接 | multiaddr，**不可信** |
//! | 身份认证 | DTLS 指纹绑定即可 | **必须再跑一次 Noise** |
//!
//! 最后一行是 spec FAQ 的第一条，也是最容易图省事踩错的地方：direct 的 certhash
//! 可能经任何不可信信道传播（贴在网页上、二维码里），只有 Noise 握手能证明
//! 「持有这张证书的人确实是那个 PeerId」。打洞模式不需要，因为 SDP 走的是已认证连接。

mod certificate;
mod sdp;
pub(crate) mod transport;
mod udp_mux;
mod upgrade;

pub use certificate::{Certificate, Error as CertificateError};
