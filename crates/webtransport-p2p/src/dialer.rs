//! 拨号。
//!
//! # 为什么 native 侧也要实现拨号
//!
//! 生产上大概率零用例：桌面↔桌面走 QUIC / TCP 更快，桌面↔浏览器是浏览器拨过来。
//! **它存在的理由是可测性** —— 没有它，native↔native 的集成测试写不了，这个 crate 在 CI
//! 里的覆盖就是零，只能靠浏览器手测；而手测无法满足「护栏测试必须红过一次」。
//!
//! 删它之前先想清楚 CI 拿什么覆盖握手链路。
//!
//! # 每次拨号一个 endpoint
//!
//! `wtransport` 把 TLS 配置（含"愿意接受哪些 certhash"）挂在 `Endpoint` 上，而每条地址的
//! certhash 集合都不同，所以没法共享一个 client endpoint。代价是每次拨号占一个临时 UDP
//! 端口，由 [`crate::muxer::Muxer`] 持有到连接结束。

use std::net::SocketAddr;
use std::sync::Arc;

use libp2p_identity::PeerId;
use wtransport::endpoint::endpoint_side::Client;
use wtransport::{ClientConfig, Endpoint};

use crate::addr::DialAddr;
use crate::config::Config;
use crate::error::{Error, with_timeout};
use crate::muxer::Muxer;
use crate::noise;

/// 拨一条 WebTransport 地址：建会话 → 开第一条流 → Noise 认证。
///
/// `target.peer` 非空时，握手结果必须与之相符，否则以 [`Error::PeerIdMismatch`] 失败。
pub(crate) async fn dial(target: DialAddr, config: Arc<Config>) -> Result<(PeerId, Muxer), Error> {
    let timeout = config.handshake_timeout();
    with_timeout(dial_inner(target, config), timeout).await
}

async fn dial_inner(target: DialAddr, config: Arc<Config>) -> Result<(PeerId, Muxer), Error> {
    let endpoint = client_endpoint(&target)?;
    let url = connect_url(target.socket);

    tracing::debug!(remote = %target.socket, certhashes = target.certhashes.len(), "webtransport 拨号");

    let session = endpoint
        .connect(&url)
        .await
        .map_err(|e| Error::Connect(format!("拨 {} 失败：{e}", target.socket)))?;

    // spec：客户端在 CONNECT 之后开的**第一条**流用于 Noise 握手。
    let stream = session
        .open_bi()
        .await
        .map_err(|e| Error::Connect(format!("申请握手流失败：{e}")))?
        .await
        .map_err(|e| Error::Connect(format!("打开握手流失败：{e}")))?;

    let expected = target.certhashes.iter().map(|h| h.to_multihash()).collect();
    let peer = noise::authenticate_outbound(
        config.id_keys(),
        crate::muxer::Substream::new(stream),
        expected,
    )
    .await?;

    if let Some(expected) = target.peer
        && expected != peer
    {
        return Err(Error::peer_id_mismatch(expected, peer));
    }

    Ok((peer, Muxer::new(session, Some(endpoint))))
}

/// 按地址里有没有 certhash 选择证书校验方式。
///
/// - **有 certhash** → 只按哈希校验（`ServerHashVerification`）。它顺带强制了 spec 的三条：
///   有效期 ≤ 14 天、ECDSA P-256、当前时刻在窗口内 —— 与浏览器 `serverCertificateHashes`
///   的准入条件逐条一致，因此 native 拨通的东西浏览器也一定拨得通。
/// - **无 certhash** → 走系统信任库。spec 明说 CA 签名证书时不加 `/certhash`，
///   这条路径是为将来 bootstrap 换成域名 + CA 证书留的。
fn client_endpoint(target: &DialAddr) -> Result<Endpoint<Client>, Error> {
    let builder = ClientConfig::builder().with_bind_default();

    let config = if target.certhashes.is_empty() {
        builder.with_native_certs().build()
    } else {
        builder
            .with_server_certificate_hashes(
                target
                    .certhashes
                    .iter()
                    .map(|h| wtransport::tls::Sha256Digest::new(*h.as_bytes())),
            )
            .build()
    };

    Endpoint::client(config).map_err(|source| Error::Bind {
        // 拨号绑的是临时端口，报错时把它写成通配更贴近实情。
        addr: SocketAddr::from(([0, 0, 0, 0], 0)),
        source,
    })
}

/// spec 规定的 CONNECT 目标。
///
/// IPv6 字面量必须加方括号，否则 URL 里的冒号会被当成端口分隔符。
fn connect_url(socket: SocketAddr) -> String {
    let host = match socket {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
    };
    format!(
        "https://{host}:{}/.well-known/libp2p-webtransport?type=noise",
        socket.port()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_spec_connect_url() {
        assert_eq!(
            connect_url("1.2.3.4:4004".parse().unwrap()),
            "https://1.2.3.4:4004/.well-known/libp2p-webtransport?type=noise"
        );
    }

    /// IPv6 不加方括号的话，URL 解析会把地址里的冒号当成端口分隔符 ——
    /// 表现为"拨号立刻失败且错误信息指向 URL 格式"，与网络问题很难区分。
    #[test]
    fn brackets_ipv6_literals() {
        assert_eq!(
            connect_url("[2001:db8::1]:4004".parse().unwrap()),
            "https://[2001:db8::1]:4004/.well-known/libp2p-webtransport?type=noise"
        );
    }
}
