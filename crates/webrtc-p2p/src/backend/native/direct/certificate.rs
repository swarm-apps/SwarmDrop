//! DTLS certificate and certhash for direct mode.
//!
//! # Why the certificate must be persisted
//!
//! A direct address carries the certificate fingerprint **inside the multiaddr**
//! (`/ip4/…/udp/…/webrtc-direct/certhash/uEi…`). Replace the certificate once and every
//! address a remote has recorded becomes undialable — they will check the old certhash
//! against the new certificate's DTLS fingerprint and necessarily mismatch. The host must
//! therefore store the PEM and load it back verbatim after a restart
//! ([`Certificate::from_pem`]).
//!
//! This is the opposite of hole-punching mode, where the fingerprint is exchanged over SDP
//! on every attempt and it makes no difference whether the certificate is the same one.

use webrtc::peer_connection::RTCCertificate;

use libp2p_webrtc_utils::Fingerprint;

/// The self-signed DTLS certificate used by direct mode.
///
/// A thin wrapper whose purpose is to **keep `webrtc` types out of this crate's public
/// API** — the crate is meant to be published on its own, and users should not be forced
/// to depend on one particular version of webrtc-rs.
#[derive(Debug, Clone, PartialEq)]
pub struct Certificate {
    inner: RTCCertificate,
}

impl Certificate {
    /// Generates a new self-signed certificate.
    ///
    /// **ECDSA P-256 — do not switch to Ed25519.** P-256 is the de facto standard in
    /// browser WebRTC DTLS implementations (it is what `RTCPeerConnection` generates for
    /// itself), while Ed25519 support is uncertain — and browsers are exactly who a
    /// direct-mode server has to face. The official `libp2p-webrtc` uses P-256 as well
    /// (the default of `rcgen::KeyPair::generate()`).
    pub fn generate() -> Result<Self, Error> {
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| Error(Kind::KeyGen(e.to_string())))?;
        let inner = RTCCertificate::from_key_pair(key_pair)
            .map_err(|e| Error(Kind::Build(e.to_string())))?;
        Ok(Self { inner })
    }

    /// Loads from PEM (private key included), in the format produced by
    /// [`Certificate::serialize_pem`].
    pub fn from_pem(pem: &str) -> Result<Self, Error> {
        let inner =
            RTCCertificate::from_pem(pem).map_err(|e| Error(Kind::InvalidPem(e.to_string())))?;
        Ok(Self { inner })
    }

    /// Serializes to PEM (private key included) for the host to persist.
    pub fn serialize_pem(&self) -> String {
        self.inner.serialize_pem()
    }

    /// This certificate's SHA-256 fingerprint — the source of the certhash in the
    /// multiaddr.
    ///
    /// Hashes the DER directly with SHA-256. Equivalent to
    /// `RTCCertificate::get_fingerprints()` but without the "format as colon-separated hex,
    /// then parse it back" round trip. A unit test pins the two paths to agree — should rtc
    /// ever change its digest algorithm, that test goes red instead of the failure surfacing
    /// as a broken handshake in production.
    ///
    /// # Panics
    ///
    /// Panics when the certificate chain is empty. Both `RTCCertificate` constructors
    /// guarantee at least one certificate, so reaching this point means the layer beneath is
    /// broken, and carrying on would only surface a more cryptic error during the DTLS
    /// handshake.
    pub fn fingerprint(&self) -> Fingerprint {
        let der = self
            .inner
            .dtls_certificate
            .certificate
            .first()
            .expect("RTCCertificate 必然含至少一张证书");
        Fingerprint::from_certificate(der.as_ref())
    }

    /// Hands the underlying type to `PeerConnection`.
    ///
    /// `pub(crate)` is deliberate — see the type-level docs.
    pub(crate) fn to_rtc(&self) -> RTCCertificate {
        self.inner.clone()
    }
}

/// Certificate error.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(#[from] Kind);

#[derive(Debug, thiserror::Error)]
enum Kind {
    #[error("failed to generate key pair: {0}")]
    KeyGen(String),
    #[error("failed to build certificate from key pair: {0}")]
    Build(String),
    #[error("failed to parse PEM: {0}")]
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
