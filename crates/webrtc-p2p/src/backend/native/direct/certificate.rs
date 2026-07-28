//! direct 模式的 DTLS 证书与 certhash。
//!
//! # 为什么证书必须持久化
//!
//! direct 地址把证书指纹**写在 multiaddr 里**
//! （`/ip4/…/udp/…/webrtc-direct/certhash/uEi…`）。换一次证书，所有已被对端记下的
//! 地址就全部拨不通——它们会拿旧 certhash 去校验新证书的 DTLS 指纹，必然失配。
//! 所以宿主要把 PEM 存下来，重启后原样加载（[`Certificate::from_pem`]）。
//!
//! 这与打洞模式相反：那边的指纹每次经 SDP 现场交换，证书是不是同一份无所谓。

use webrtc::peer_connection::RTCCertificate;

use libp2p_webrtc_utils::Fingerprint;

/// direct 模式使用的自签 DTLS 证书。
///
/// 薄封装，存在的意义是**不把 `webrtc` 类型泄漏到本 crate 的公开 API**——
/// 它要独立发布，用户不该被迫依赖某个特定版本的 webrtc-rs。
#[derive(Debug, Clone, PartialEq)]
pub struct Certificate {
    inner: RTCCertificate,
}

impl Certificate {
    /// 生成一张新的自签证书。
    ///
    /// **ECDSA P-256，不要换成 Ed25519。** 浏览器的 WebRTC DTLS 实现以 P-256 为事实
    /// 标准（`RTCPeerConnection` 自生成的证书就是它），Ed25519 的支持面不确定；而
    /// direct 模式的服务端要面对的正是浏览器。官方 `libp2p-webrtc` 同样用 P-256
    /// （`rcgen::KeyPair::generate()` 的默认）。
    pub fn generate() -> Result<Self, Error> {
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| Error(Kind::KeyGen(e.to_string())))?;
        let inner = RTCCertificate::from_key_pair(key_pair)
            .map_err(|e| Error(Kind::Build(e.to_string())))?;
        Ok(Self { inner })
    }

    /// 从 PEM 加载（含私钥），格式与 [`Certificate::serialize_pem`] 对应。
    pub fn from_pem(pem: &str) -> Result<Self, Error> {
        let inner =
            RTCCertificate::from_pem(pem).map_err(|e| Error(Kind::InvalidPem(e.to_string())))?;
        Ok(Self { inner })
    }

    /// 序列化成 PEM（含私钥）供宿主持久化。
    pub fn serialize_pem(&self) -> String {
        self.inner.serialize_pem()
    }

    /// 本证书的 SHA-256 指纹，也就是 multiaddr 里那个 certhash 的来源。
    ///
    /// 直接对 DER 做 SHA-256，与 `RTCCertificate::get_fingerprints()` 等价但少一次
    /// 「格式化成冒号 hex 再解析回来」的往返。两条路径的一致性由单测钉死——
    /// 万一 rtc 改了摘要算法，那个测试会红，而不是等到线上握手失败才发现。
    ///
    /// # Panics
    ///
    /// 证书链为空时 panic。`RTCCertificate` 的两个构造入口都保证至少一张证书，
    /// 走到这里说明底层坏了，继续跑只会在 DTLS 握手时报更难懂的错。
    pub fn fingerprint(&self) -> Fingerprint {
        let der = self
            .inner
            .dtls_certificate
            .certificate
            .first()
            .expect("RTCCertificate 必然含至少一张证书");
        Fingerprint::from_certificate(der.as_ref())
    }

    /// 取出底层类型交给 `PeerConnection`。
    ///
    /// `pub(crate)` 是刻意的——见类型文档。
    pub(crate) fn to_rtc(&self) -> RTCCertificate {
        self.inner.clone()
    }
}

/// 证书错误。
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(#[from] Kind);

#[derive(Debug, thiserror::Error)]
enum Kind {
    #[error("生成密钥对失败：{0}")]
    KeyGen(String),
    #[error("由密钥对构造证书失败：{0}")]
    Build(String),
    #[error("解析 PEM 失败：{0}")]
    InvalidPem(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方 `libp2p-webrtc`（webrtc-rs **0.17**）序列化出来的一份真实 PEM。
    ///
    /// 存在的意义是钉死**跨实现的持久化兼容**：本 crate 用 rtc 0.20 读它，必须得到
    /// 与官方逐位相同的 certhash。不然把官方实现换掉的那一刻，所有存量部署（bootstrap
    /// 节点、已被对端记住的地址）会在无人察觉的情况下集体失效——地址还在、还能拨，
    /// 只是 DTLS 指纹对不上。
    ///
    /// 由官方实现现场生成后固化，**不要重新生成**：重新生成就等于把断言改成同义反复。
    const OFFICIAL_PEM: &str = "\
-----BEGIN EXPIRES-----\n\
APfhng8AAAA=\n\
-----END EXPIRES-----\n\
\n\
-----BEGIN PRIVATE_KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgoJLiV+skauLa0c/O\n\
YML+1fAIlTMewGtHdSy1ALahXMKhRANCAARicyvzIYVVL2qpjzxt3r8GiKc3XNV6\n\
i/Yiq593NdpVIH25ELId9fNLJU0EBY0+pQIrMICeWNVlnuM7swNmuBfD\n\
-----END PRIVATE_KEY-----\n\
\n\
-----BEGIN CERTIFICATE-----\n\
MIIBZTCCAQugAwIBAgIUBijW2FlPIDgXWo1tJ30UBPBUNyUwCgYIKoZIzj0EAwIw\n\
ITEfMB0GA1UEAwwWcmNnZW4gc2VsZiBzaWduZWQgY2VydDAgFw03NTAxMDEwMDAw\n\
MDBaGA80MDk2MDEwMTAwMDAwMFowITEfMB0GA1UEAwwWcmNnZW4gc2VsZiBzaWdu\n\
ZWQgY2VydDBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABGJzK/MhhVUvaqmPPG3e\n\
vwaIpzdc1XqL9iKrn3c12lUgfbkQsh3180slTQQFjT6lAiswgJ5Y1WWe4zuzA2a4\n\
F8OjHzAdMBsGA1UdEQQUMBKCEFluaENhZ0ZNcm9KS1duamIwCgYIKoZIzj0EAwID\n\
SAAwRQIhAIv4949AE68MgyEcJjw/9Fej37rPs3Te2ug3p9+QJkjtAiA1mHZD4BIV\n\
0LOBmweoeEKpw4q8J7ey6DtFrPOBJmjm7w==\n\
-----END CERTIFICATE-----\n";

    /// 官方对 [`OFFICIAL_PEM`] 算出的 certhash（multiaddr 里那一段的原文）。
    const OFFICIAL_CERTHASH: &str = "uEiCST46oAnoWKS4f9kK1IDOYVXtIivt2I8MlQeVmSL53ig";

    /// 读官方写的 PEM，必须得到官方那个 certhash。
    ///
    /// 这条一红就说明**不能替换官方实现**——存量地址会全部拨不通。
    #[test]
    fn reads_official_pem_with_identical_certhash() {
        let cert = Certificate::from_pem(OFFICIAL_PEM).expect("应能读官方实现写的 PEM");

        let certhash =
            libp2p_core::multiaddr::Protocol::Certhash(cert.fingerprint().to_multihash());
        assert_eq!(
            certhash.to_string(),
            format!("/certhash/{OFFICIAL_CERTHASH}"),
            "certhash 必须与官方逐位一致，否则存量地址全部失效"
        );
    }

    /// 反过来也要成立：我们写出的 PEM 自己能读回，且 certhash 不变。
    /// 与上一条合起来，两个实现的持久化格式互通。
    #[test]
    fn official_pem_survives_our_roundtrip() {
        let original = Certificate::from_pem(OFFICIAL_PEM).unwrap();
        let reloaded = Certificate::from_pem(&original.serialize_pem()).unwrap();

        assert_eq!(reloaded.fingerprint(), original.fingerprint());
    }

    /// PEM 往返后必须是同一张证书——否则重启就换了 certhash，
    /// 对端记下的地址全部失效（这正是持久化要防的事）。
    #[test]
    fn pem_roundtrip_preserves_fingerprint() {
        let cert = Certificate::generate().unwrap();
        let loaded = Certificate::from_pem(&cert.serialize_pem()).unwrap();

        assert_eq!(loaded, cert);
        assert_eq!(loaded.fingerprint(), cert.fingerprint());
    }

    /// 我们抄近路直接对 DER 取 SHA-256，官方走的是 `get_fingerprints()` 的
    /// 冒号 hex 字符串。两条路径必须给出同一个值——不一致就说明 rtc 换了摘要算法，
    /// 那时线上表现会是「DTLS 指纹校验失败」，极难归因。
    #[test]
    fn fingerprint_matches_rtc_sdp_format() {
        let cert = Certificate::generate().unwrap();

        let rtc_fp = cert.to_rtc().get_fingerprints();
        let sha256 = rtc_fp
            .iter()
            .find(|f| f.algorithm == "sha-256")
            .expect("必须有 SHA-256 指纹");

        assert_eq!(
            cert.fingerprint().to_sdp_format().to_lowercase(),
            sha256.value.to_lowercase(),
            "对 DER 直接取摘要的结果必须与 rtc 的 get_fingerprints 一致"
        );
    }

    /// 两张独立生成的证书不能撞指纹（撞了说明密钥没随机）。
    #[test]
    fn distinct_certificates_have_distinct_fingerprints() {
        let a = Certificate::generate().unwrap();
        let b = Certificate::generate().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn rejects_garbage_pem() {
        assert!(Certificate::from_pem("not a pem").is_err());
        assert!(Certificate::from_pem("").is_err());
    }
}
