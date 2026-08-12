//! Noise 认证握手。
//!
//! spec 步骤：客户端在 CONNECT 之后开的**第一条 bidi 流**上发起 Noise 握手，且可以不等
//! 服务端的 CONNECT 响应就开始。
//!
//! # 只认证，不加密
//!
//! 握手拿到 `PeerId` 后那条流就被关掉，后续子流是 WebTransport 上的**明文** ——
//! 保密由 QUIC-TLS 承担。自签名证书能证明「对面持有这张证书」，证不了「这张证书属于那个
//! PeerId」，Noise 补的正是后半句。
//!
//! # ⚠️ 不设 prologue，与 webrtc-direct **机制互斥**
//!
//! webrtc-direct 用 `libp2p-webrtc-noise:` + 双方 DTLS 指纹作 **prologue** 把身份绑到信道；
//! WebTransport 用的是 **`webtransport_certhashes` Noise 扩展**。两者是同一目的的两种机制，
//! 照抄隔壁 crate 会让握手在第一条消息就失败 —— 而症状看起来像「Noise 实现有 bug」，
//! 极难归因。
//!
//! # 角色映射与「退役 hash 必须上报」
//!
//! | 本端 | Noise 角色 | `with_webtransport_certhashes` 的语义 |
//! |---|---|---|
//! | 服务端（listener） | responder（`upgrade_inbound`） | **上报**自己的证书哈希集合 |
//! | 客户端（dialer） | initiator（`upgrade_outbound`） | **验证**：期望集合必须是收到集合的**子集** |
//!
//! 「子集」这条判据有个不显然的后果，它正是 [`crate::certificate::Rotation`] 要保留退役
//! certhash 的原因：
//!
//! ```text
//! 客户端持第 0 天的地址，期望集合 {A, B}
//! 服务端已轮换一轮，current = B
//!   TLS 层：出示 B，B ∈ {A,B}          → 通过
//!   Noise 层：上报 {B, C}，{A,B} ⊄ {B,C} → 失败！
//! ```
//!
//! 即 **TLS 过了 Noise 仍会挂**。服务端必须把刚退役的 A 一并上报，`{A,B} ⊆ {B,C,A}` 才成立。
//! spec 那句「近期过期的也建议带上」不是可选优化，是让上一轮的地址真正可用的前提。

use std::collections::HashSet;

use futures::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p_core::UpgradeInfo;
use libp2p_core::multihash::Multihash;
use libp2p_core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p_identity::{Keypair, PeerId};

use crate::error::Error;

/// 服务端侧：接受一次认证，并上报本机的证书哈希集合。
pub(crate) async fn authenticate_inbound<T>(
    id_keys: &Keypair,
    stream: T,
    certhashes: HashSet<Multihash<64>>,
) -> Result<PeerId, Error>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let config = config(id_keys, certhashes)?;
    let info = protocol_info(&config);
    let (peer, mut channel) = config.upgrade_inbound(stream, info).await?;

    // 只认证不加密：握手完这条流就没用了。关不掉也无所谓 —— 连接本身是好的，
    // 而对端最坏只是多留一条空流，故忽略错误。
    let _ = channel.close().await;
    Ok(peer)
}

/// 客户端侧：发起认证，并校验服务端上报的哈希覆盖了本端从地址里读到的那些。
///
/// `expected` 为空时 `libp2p-noise` **跳过校验**（`with_webtransport_certhashes` 对空集合
/// 等价于没设）。这正是 CA 签名证书那条路径要的行为 —— 那种地址本就不带 `/certhash`。
pub(crate) async fn authenticate_outbound<T>(
    id_keys: &Keypair,
    stream: T,
    expected: HashSet<Multihash<64>>,
) -> Result<PeerId, Error>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let config = config(id_keys, expected)?;
    let info = protocol_info(&config);
    let (peer, mut channel) = config.upgrade_outbound(stream, info).await?;

    let _ = channel.close().await;
    Ok(peer)
}

fn config(
    id_keys: &Keypair,
    certhashes: HashSet<Multihash<64>>,
) -> Result<libp2p_noise::Config, Error> {
    Ok(libp2p_noise::Config::new(id_keys)?.with_webtransport_certhashes(certhashes))
}

/// `Config` 的协议名恒有且只有一个（`/noise`）。
///
/// # Panics
///
/// 不会。`libp2p-noise` 的 `protocol_info` 返回的是一个单元素常量数组。
fn protocol_info(config: &libp2p_noise::Config) -> <libp2p_noise::Config as UpgradeInfo>::Info {
    config
        .protocol_info()
        .next()
        .expect("libp2p-noise 恒有一个协议名")
}

#[cfg(test)]
mod tests {
    use futures::future;
    use libp2p_core::multihash::Multihash;

    use super::*;
    use crate::addr::Certhash;

    fn keypair() -> Keypair {
        Keypair::generate_ed25519()
    }

    fn hash(seed: u8) -> Multihash<64> {
        Certhash::new([seed; 32]).to_multihash()
    }

    /// 内存双工流：L1 层不依赖 wtransport，因此握手可以完全脱离真实网络测试。
    fn duplex() -> (
        impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
        impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    ) {
        futures_ringbuf::Endpoint::pair(4096, 4096)
    }

    /// 基本路径：双方各自拿到对方正确的 PeerId。
    #[tokio::test]
    async fn authenticates_both_directions() {
        let (server_keys, client_keys) = (keypair(), keypair());
        let (server_id, client_id) = (
            server_keys.public().to_peer_id(),
            client_keys.public().to_peer_id(),
        );
        let (server_io, client_io) = duplex();
        let advertised = HashSet::from([hash(1), hash(2)]);

        let (server, client) = future::join(
            authenticate_inbound(&server_keys, server_io, advertised.clone()),
            authenticate_outbound(&client_keys, client_io, advertised),
        )
        .await;

        assert_eq!(server.unwrap(), client_id);
        assert_eq!(client.unwrap(), server_id);
    }

    /// ★ **负向路径**：客户端期望的 hash 不在服务端上报的集合里，握手必须失败。
    ///
    /// 只测成功路径等于没测 —— 把 `with_webtransport_certhashes` 整个删掉，
    /// 上面那条仍然会绿。
    #[tokio::test]
    async fn rejects_when_expected_certhash_is_not_reported() {
        let (server_keys, client_keys) = (keypair(), keypair());
        let (server_io, client_io) = duplex();

        let (_, client) = future::join(
            authenticate_inbound(&server_keys, server_io, HashSet::from([hash(1)])),
            // 客户端期望一个服务端根本没有的 hash
            authenticate_outbound(&client_keys, client_io, HashSet::from([hash(9)])),
        )
        .await;

        assert!(
            matches!(client, Err(Error::Noise(_))),
            "certhash 不匹配必须让握手失败，实得 {client:?}"
        );
    }

    /// 客户端期望的是服务端集合的**真子集**时必须通过 —— 服务端总会多上报
    /// （next + 退役的），要求完全相等会把每一次轮换后的旧地址全部打死。
    #[tokio::test]
    async fn accepts_when_expected_is_a_strict_subset() {
        let (server_keys, client_keys) = (keypair(), keypair());
        let (server_io, client_io) = duplex();

        let (_, client) = future::join(
            // 服务端：current + next + 一个退役的
            authenticate_inbound(
                &server_keys,
                server_io,
                HashSet::from([hash(2), hash(3), hash(1)]),
            ),
            // 客户端持上一轮的地址：{退役的, current}
            authenticate_outbound(&client_keys, client_io, HashSet::from([hash(1), hash(2)])),
        )
        .await;

        assert!(client.is_ok(), "上一轮的地址必须仍能通过，实得 {client:?}");
    }

    /// 空集合等价于「不校验」—— CA 签名证书那条路径的地址本就不带 certhash。
    #[tokio::test]
    async fn empty_expectation_skips_verification() {
        let (server_keys, client_keys) = (keypair(), keypair());
        let (server_io, client_io) = duplex();

        let (_, client) = future::join(
            authenticate_inbound(&server_keys, server_io, HashSet::new()),
            authenticate_outbound(&client_keys, client_io, HashSet::new()),
        )
        .await;

        assert!(client.is_ok(), "实得 {client:?}");
    }
}
