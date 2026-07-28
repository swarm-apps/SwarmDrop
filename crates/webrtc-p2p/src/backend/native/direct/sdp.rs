//! direct 模式的确定性 SDP。
//!
//! 这是 direct 与打洞最根本的差别：**没有信令，双方各自在本地把对方的 SDP 算出来**。
//! 能这么做是因为一方的全部必要信息都写在 multiaddr 里（IP、端口、certhash），而
//! ufrag 由客户端选定后经 STUN 的 `USERNAME` 属性带给服务端。
//!
//! | | 谁真的生成 | 谁本地构造 |
//! |---|---|---|
//! | 客户端（dialer） | 自己的 **offer**（`create_offer`） | 服务端的 **answer** |
//! | 服务端（listener） | 自己的 **answer**（`create_answer`） | 客户端的 **offer** |
//!
//! answer 模板复用 `libp2p-webrtc-utils`（`sdp::answer`）；offer 模板它没导出，
//! 只能抄一份过来——见 [`CLIENT_SESSION_DESCRIPTION`]。

use std::net::SocketAddr;

use libp2p_webrtc_utils::Fingerprint;
use libp2p_webrtc_utils::sdp::render_description;
use webrtc::peer_connection::RTCSessionDescription;

/// 客户端视角的 offer，由**服务端**本地构造。
///
/// 服务端此刻并不知道客户端的证书指纹（它在 DTLS 握手时才出现），所以这里填一个
/// 占位值 [`Fingerprint::FF`]，并在建连时关掉指纹校验
/// （`disable_certificate_fingerprint_verification`）。真正的身份绑定由随后的
/// Noise 握手完成——这正是 direct 模式必须跑 Noise 的原因。
pub(crate) fn offer(addr: SocketAddr, client_ufrag: &str) -> Result<RTCSessionDescription, Error> {
    let sdp = render_description(
        CLIENT_SESSION_DESCRIPTION,
        addr,
        Fingerprint::FF,
        client_ufrag,
    );
    RTCSessionDescription::offer(sdp).map_err(|e| Error(e.to_string()))
}

/// 服务端视角的 answer，由**客户端**本地构造。
///
/// 与 offer 不同，这里的指纹是**真的**——它来自 multiaddr 的 certhash。DTLS 握手
/// 会校验它，因此拨到冒名顶替的服务端会在握手阶段就失败。
pub(crate) fn answer(
    addr: SocketAddr,
    server_fingerprint: Fingerprint,
    client_ufrag: &str,
) -> Result<RTCSessionDescription, Error> {
    let sdp = libp2p_webrtc_utils::sdp::answer(addr, server_fingerprint, client_ufrag);
    RTCSessionDescription::answer(sdp).map_err(|e| Error(e.to_string()))
}

/// SDP 构造失败。模板是常量、参数已校验，实践中只会因 rtc 侧解析器变更而出现。
#[derive(Debug, thiserror::Error)]
#[error("构造 SDP 失败：{0}")]
pub(crate) struct Error(String);

/// 客户端 offer 的模板。
///
/// 逐字抄自官方 `libp2p-webrtc` 的 `tokio/sdp.rs`（它是 crate 私有的，导不出来）。
/// **不要"顺手优化"里面的任何一行**——两端要对同一份 SDP 达成一致，改动会直接
/// 表现为与官方实现、与 js-libp2p 互不相认。
///
/// 各字段的含义（RFC 8866 / 8841 / 8839 / 9143 / 8122）：
///
/// - `o=` / `s=` 用哑值保持匿名，spec 允许
/// - `c=` 是**远端**地址
/// - `a=ice-ufrag` / `a=ice-pwd` 两者取同一个值，且必须与 answer 一致
/// - `a=setup:actpass` —— offerer 必须用它，并准备好在收到 answer 前就接 `client_hello`
/// - `a=sctp-port` 是 SCTP 端口，与 `m=` 行的传输层端口无关
const CLIENT_SESSION_DESCRIPTION: &str = "v=0
o=- 0 0 IN {ip_version} {target_ip}
s=-
c=IN {ip_version} {target_ip}
t=0 0

m=application {target_port} UDP/DTLS/SCTP webrtc-datachannel
a=mid:0
a=ice-options:ice2
a=ice-ufrag:{ufrag}
a=ice-pwd:{pwd}
a=fingerprint:{fingerprint_algorithm} {fingerprint_value}
a=setup:actpass
a=sctp-port:5000
a=max-message-size:{max_message_size}
";

#[cfg(test)]
mod tests {
    use super::*;

    fn addr() -> SocketAddr {
        "1.2.3.4:4003".parse().unwrap()
    }

    fn fingerprint() -> Fingerprint {
        Fingerprint::raw([0xAB; 32])
    }

    /// offer 必须带上客户端的 ufrag（服务端就是靠它分流的），且 setup 是 actpass。
    #[test]
    fn offer_carries_ufrag_and_actpass() {
        let sdp = offer(addr(), "libp2p+webrtc+v1/test").unwrap().sdp;

        assert!(sdp.contains("a=ice-ufrag:libp2p+webrtc+v1/test"));
        assert!(
            sdp.contains("a=ice-pwd:libp2p+webrtc+v1/test"),
            "pwd 与 ufrag 同值"
        );
        assert!(sdp.contains("a=setup:actpass"));
        assert!(sdp.contains("m=application 4003 UDP/DTLS/SCTP webrtc-datachannel"));
        assert!(sdp.contains("c=IN IP4 1.2.3.4"));
    }

    /// answer 必须是 ICE-lite、setup:passive，并带上真实指纹与 host candidate。
    /// 少了 `a=ice-lite` 客户端会等服务端来做连通性检查，而服务端根本不会做。
    #[test]
    fn answer_is_ice_lite_with_real_fingerprint() {
        let sdp = answer(addr(), fingerprint(), "libp2p+webrtc+v1/test")
            .unwrap()
            .sdp;

        assert!(sdp.contains("a=ice-lite"));
        assert!(sdp.contains("a=setup:passive"));
        assert!(sdp.contains(&fingerprint().to_sdp_format()));
        assert!(
            sdp.contains("typ host"),
            "必须带确定性构造的 host candidate"
        );
        assert!(sdp.contains("a=end-of-candidates"), "ICE-lite 不 trickle");
    }

    /// offer 的指纹是占位符——服务端此刻不可能知道客户端的真实指纹。
    /// 这条同时提醒：因此**必须**关掉指纹校验并改由 Noise 认证。
    #[test]
    fn offer_fingerprint_is_placeholder() {
        let sdp = offer(addr(), "u").unwrap().sdp;
        assert!(sdp.contains(&Fingerprint::FF.to_sdp_format()));
    }

    #[test]
    fn renders_ipv6() {
        let addr: SocketAddr = "[::1]:4003".parse().unwrap();
        assert!(offer(addr, "u").unwrap().sdp.contains("c=IN IP6 ::1"));
        assert!(
            answer(addr, fingerprint(), "u")
                .unwrap()
                .sdp
                .contains("c=IN IP6 ::1")
        );
    }
}
