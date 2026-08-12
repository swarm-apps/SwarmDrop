//! 单张自签名证书。
//!
//! # 三条 spec 硬约束，缺一条浏览器就拒绝连接
//!
//! | 约束 | 出处 | 违反时的表现 |
//! |---|---|---|
//! | 有效期 ≤ 14 天 | libp2p spec + W3C `serverCertificateHashes` | 浏览器在 TLS 阶段拒绝 |
//! | ECDSA P-256（**禁 RSA**） | 同上 | 同上 |
//! | 当前时刻在有效期窗口内 | 同上 | 同上 |
//!
//! 三条的错误信息都不指向真正的原因（浏览器只说证书不受信），所以**本模块自己校验**：
//! 生成时保证、加载时拒绝。`wtransport` 客户端侧的 `ServerHashVerification` 用的是同一
//! 套判据，因此 native↔native 与 browser↔native 的准入条件逐条一致。
//!
//! # 为什么有效期要从 DER 解析回来，而不是记在旁边
//!
//! 轮换判据（「current 过期了吗」）读的就是它。若把有效期作为元数据另存一份，持久化格式
//! 就多了一个可以与证书本身矛盾的事实源 —— 而那份元数据一旦与 DER 不一致，节点会用一张
//! 客户端已经拒绝的证书继续对外服务，且自己毫不知情。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wtransport::Identity;
use wtransport::tls::{CertificateChain, PrivateKey};
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::addr::Certhash;

/// spec 规定的有效期上限。
pub const MAX_VALIDITY: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// 自签名证书的 SAN。
///
/// 用 `serverCertificateHashes` 连接时**证书名不参与校验**（`ServerHashVerification`
/// 直接忽略 `server_name`，浏览器同理），所以填什么都不影响连通性。取 `localhost`
/// 与 go-libp2p 保持一致，便于用通用 TLS 工具排障时看到熟悉的值。
const SUBJECT_ALT_NAME: &str = "localhost";

/// 一张自签名的 WebTransport 服务端证书。
///
/// 薄封装，目的是**把 `wtransport` 的类型挡在公共 API 之外** —— 本 crate 要独立发布，
/// 用户不该被迫依赖某个特定版本的 wtransport。
pub struct Certificate {
    identity: Identity,
    /// 从 DER 解析出的有效期。缓存而非每次重解析：轮换判据每次 poll 都要读它。
    not_before: SystemTime,
    not_after: SystemTime,
    /// 证书 DER 的 SHA-256。**同样是缓存**，理由与有效期一模一样：它是不可变派生值，
    /// 而 `Debug`、`PartialEq`、`advertised()`、`noise_certhashes()` 都在反复读它
    /// —— `PartialEq` 一次比较就要算两遍。上游 `wtransport::Certificate::hash()`
    /// 内部没有任何缓存，每次都对 ~450 字节 DER 重跑一遍 SHA-256。
    certhash: Certhash,
}

impl Certificate {
    /// 生成一张新证书。
    ///
    /// `not_before` 由调用方给定而不是取当前时间 —— 轮换需要生成「从上一张过期日开始
    /// 生效」的证书，而测试需要在任意时刻推进（见 [`super::rotation`]）。
    ///
    /// `validity` 超过 [`MAX_VALIDITY`] 时返回错误而不是静默截断：截断会让调用方以为
    /// 拿到了它要的有效期，而实际通告出去的地址寿命完全不同。
    pub fn generate(not_before: SystemTime, validity: Duration) -> Result<Self, Error> {
        if validity > MAX_VALIDITY {
            return Err(Error::Unusable(
                "有效期超过 spec 上限 14 天，浏览器会拒绝该证书",
            ));
        }
        let not_after = not_before + validity;

        let identity = Identity::self_signed_builder()
            .subject_alt_names([SUBJECT_ALT_NAME])
            .not_before(to_offset(not_before)?)
            .not_after(to_offset(not_after)?)
            .build()
            .map_err(|e| Error::Generate(e.to_string()))?;

        Self::from_identity(identity)
    }

    /// 从一段 PEM 还原（证书与私钥各一段，顺序不限）。
    ///
    /// 不合规的证书在这里就被拒绝，由调用方决定是重新生成还是上报 —— 见模块文档。
    pub fn from_pem(pem: &str) -> Result<Self, Error> {
        use rustls_pki_types::pem::PemObject;
        use rustls_pki_types::{CertificateDer, PrivateKeyDer};

        let der = CertificateDer::from_pem_slice(pem.as_bytes())
            .map_err(|e| Error::Pem(e.to_string()))?;
        let key =
            PrivateKeyDer::from_pem_slice(pem.as_bytes()).map_err(|e| Error::Pem(e.to_string()))?;

        Self::from_der(der.to_vec(), key)
    }

    /// 由证书与私钥的 DER 组装。
    ///
    /// `pub(crate)`：多段 PEM 的拆分归 [`super::rotation`]，它按顺序配对后逐对走这里，
    /// 于是「合规性校验」仍只有 [`Self::from_identity`] 一个入口。
    ///
    /// # 为什么要挑私钥的编码变体
    ///
    /// `wtransport` 的 `PrivateKey::from_der_pkcs8` 只是**贴个标签**，不解析内容；而起监听
    /// 时它走的是 rustls 的 `.with_single_cert(..).expect("已经验证过")` —— **`expect`，
    /// 不是 `Result`**。于是一份 SEC1 私钥（`-----BEGIN EC PRIVATE KEY-----`，openssl 的
    /// 默认输出）会一路顺畅地通过构造、在第一次 `listen_on` 时把整个 Swarm 线程 panic 掉。
    ///
    /// 契约要求的是「持久化数据坏了 → warn + 重新生成」，所以判据必须在这里。
    pub(crate) fn from_der(
        cert_der: Vec<u8>,
        key: rustls_pki_types::PrivateKeyDer<'static>,
    ) -> Result<Self, Error> {
        let rustls_pki_types::PrivateKeyDer::Pkcs8(_) = &key else {
            return Err(Error::Unusable(
                "私钥不是 PKCS#8 编码（常见于 openssl 默认输出的 SEC1）——                 沿用会在起监听时 panic",
            ));
        };

        let cert = wtransport::tls::Certificate::from_der(cert_der)
            .map_err(|e| Error::Pem(e.to_string()))?;

        Self::from_identity(Identity::new(
            CertificateChain::single(cert),
            PrivateKey::from_der_pkcs8(key.secret_der().to_vec()),
        ))
    }

    /// 校验并封装一个 `Identity`。所有构造路径都收敛到这里，合规性只判一次。
    fn from_identity(identity: Identity) -> Result<Self, Error> {
        let der = identity
            .certificate_chain()
            .as_slice()
            .first()
            .ok_or(Error::EmptyChain)?
            .der()
            .to_vec();

        let (_, x509) = X509Certificate::from_der(&der)
            .map_err(|e| Error::Pem(format!("X.509 解析失败：{e}")))?;

        ensure_spec_compliant(&x509)?;

        let digest = identity
            .certificate_chain()
            .as_slice()
            .first()
            .ok_or(Error::EmptyChain)?
            .hash();

        let validity = x509.validity();
        Ok(Self {
            not_before: from_timestamp(validity.not_before.timestamp())?,
            not_after: from_timestamp(validity.not_after.timestamp())?,
            certhash: Certhash::new(*digest.as_ref()),
            identity,
        })
    }

    /// 这张证书的 certhash —— 通告地址与 Noise 扩展里的那个值。
    ///
    /// 构造期算一次并缓存，见类型文档。
    pub fn certhash(&self) -> Certhash {
        self.certhash
    }

    /// 生效时刻。
    pub fn not_before(&self) -> SystemTime {
        self.not_before
    }

    /// 过期时刻。轮换判据读的就是它。
    pub fn not_after(&self) -> SystemTime {
        self.not_after
    }

    /// `now` 是否落在有效期窗口内。
    pub fn is_valid_at(&self, now: SystemTime) -> bool {
        self.not_before <= now && now < self.not_after
    }

    /// 序列化成 PEM（证书 + 私钥两段），供宿主持久化。
    ///
    /// 格式即 rustls / wtransport 的既有惯例，**没有自定义元数据** —— 有效期本就编码在
    /// X.509 里，另存一份只会制造第二事实源。
    pub fn to_pem(&self) -> String {
        let chain = self.identity.certificate_chain();
        let cert = chain
            .as_slice()
            .first()
            .expect("构造期已保证证书链非空")
            .to_pem();
        format!("{cert}{}", self.identity.private_key().to_secret_pem())
    }

    /// 交给 `wtransport` 起监听用。`pub(crate)`：见类型文档。
    pub(crate) fn identity(&self) -> Identity {
        self.identity.clone_identity()
    }
}

impl Clone for Certificate {
    /// `Identity` 刻意不实现 `Clone`（避免无意间复制私钥），这里显式复制。
    /// 轮换时 `next` 要整体提升为 `current`，没有 Clone 就只能到处 `Option::take`。
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone_identity(),
            not_before: self.not_before,
            not_after: self.not_after,
            certhash: self.certhash,
        }
    }
}

impl std::fmt::Debug for Certificate {
    /// **不打印私钥**，也不打印完整 DER。certhash + 有效期是排障需要的全部。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Certificate")
            .field("certhash", &self.certhash())
            .field("not_before", &self.not_before)
            .field("not_after", &self.not_after)
            .finish()
    }
}

impl PartialEq for Certificate {
    /// 按 certhash 比 —— 两张证书相同当且仅当 DER 逐位相同。
    fn eq(&self, other: &Self) -> bool {
        self.certhash() == other.certhash()
    }
}

impl Eq for Certificate {}

/// 逐条核对 spec 对证书的要求。
///
/// 判据与 `wtransport` 客户端侧 `ServerHashVerification` 的完全一致（有效期长度、
/// 公钥算法），差别只在这里不检查「当前时刻是否在窗口内」—— 那是轮换状态机的职责，
/// 一张尚未生效的 `next` 证书在这里必须被判为合规。
fn ensure_spec_compliant(x509: &X509Certificate<'_>) -> Result<(), Error> {
    use x509_parser::oid_registry::{OID_EC_P256, OID_KEY_TYPE_EC_PUBLIC_KEY};

    let validity = x509.validity();
    let period =
        (validity.not_after - validity.not_before).ok_or(Error::Unusable("证书有效期为负"))?;
    if period > time::Duration::days(14) {
        return Err(Error::Unusable(
            "证书有效期超过 14 天，浏览器与 native 客户端都会拒绝",
        ));
    }

    let key = x509.public_key();
    if key.algorithm.algorithm != OID_KEY_TYPE_EC_PUBLIC_KEY {
        return Err(Error::Unusable("证书公钥不是 ECDSA（spec 禁 RSA）"));
    }
    if !matches!(
        key.algorithm.parameters.as_ref().map(|any| any.as_oid()),
        Some(Ok(oid)) if oid == OID_EC_P256
    ) {
        return Err(Error::Unusable("证书曲线不是 P-256"));
    }
    Ok(())
}

fn to_offset(time: SystemTime) -> Result<time::OffsetDateTime, Error> {
    time::OffsetDateTime::from_unix_timestamp(
        time.duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Unusable("时刻早于 UNIX 纪元"))?
            .as_secs() as i64,
    )
    .map_err(|e| Error::Generate(format!("时刻超出可表示范围：{e}")))
}

fn from_timestamp(seconds: i64) -> Result<SystemTime, Error> {
    let seconds = u64::try_from(seconds).map_err(|_| Error::Unusable("证书时刻早于 UNIX 纪元"))?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

/// 证书错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("生成自签名证书失败：{0}")]
    Generate(String),

    #[error("PEM / DER 解析失败：{0}")]
    Pem(String),

    #[error("证书链为空")]
    EmptyChain,

    /// 证书本身能解析，但不满足 spec 对 WebTransport 自签名证书的要求。
    /// 加载到这种证书时**必须重新生成**，沿用等于对外提供一张客户端注定拒绝的证书。
    #[error("证书不可用：{0}")]
    Unusable(&'static str),

    /// 持久化数据里证书与私钥数量对不上。
    #[error("持久化数据不完整：{certs} 张证书、{keys} 个私钥")]
    Unpaired { certs: usize, keys: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }

    /// 生成的证书必须逐条满足 spec：ECDSA P-256、有效期不超 14 天。
    /// 这三条任一不满足，浏览器都只会说「证书不受信」，排障成本极高。
    #[test]
    fn generated_certificate_is_spec_compliant() {
        let cert = Certificate::generate(now(), MAX_VALIDITY).unwrap();

        assert_eq!(cert.not_before(), now());
        assert_eq!(cert.not_after(), now() + MAX_VALIDITY);
        // 构造走的是 from_identity，合规性已在其中校验过；这里再断言一次窗口语义。
        assert!(cert.is_valid_at(now()));
        assert!(cert.is_valid_at(now() + MAX_VALIDITY - Duration::from_secs(1)));
        assert!(!cert.is_valid_at(now() + MAX_VALIDITY));
        assert!(!cert.is_valid_at(now() - Duration::from_secs(1)));
    }

    /// 超过 14 天必须**报错**而不是截断：截断会让调用方以为拿到了它要的有效期，
    /// 而通告地址的实际寿命完全不同。
    #[test]
    fn rejects_validity_beyond_spec_limit() {
        let err = Certificate::generate(now(), MAX_VALIDITY + Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, Error::Unusable(_)), "实得 {err:?}");
    }

    /// PEM 往返必须保持 certhash 不变 —— 变了就等于重启换了身份，
    /// 对端记下的地址全部失效，而这正是持久化要防的事。
    #[test]
    fn pem_roundtrip_preserves_certhash_and_validity() {
        let cert = Certificate::generate(now(), MAX_VALIDITY).unwrap();
        let loaded = Certificate::from_pem(&cert.to_pem()).unwrap();

        assert_eq!(loaded.certhash(), cert.certhash());
        assert_eq!(loaded.not_before(), cert.not_before());
        assert_eq!(loaded.not_after(), cert.not_after());
    }

    /// 两张独立生成的证书不能撞 certhash（撞了说明密钥没随机）。
    #[test]
    fn distinct_certificates_have_distinct_certhashes() {
        let a = Certificate::generate(now(), MAX_VALIDITY).unwrap();
        let b = Certificate::generate(now(), MAX_VALIDITY).unwrap();
        assert_ne!(a.certhash(), b.certhash());
    }

    /// 未来才生效的证书（轮换里的 `next`）必须被判为**合规**。
    /// 把「当前是否在窗口内」混进合规性检查，就再也生成不出 next。
    #[test]
    fn future_certificate_is_compliant_though_not_yet_valid() {
        let future = now() + MAX_VALIDITY;
        let cert = Certificate::generate(future, MAX_VALIDITY).unwrap();

        assert!(!cert.is_valid_at(now()));
        assert!(cert.is_valid_at(future));
    }

    /// ★ SEC1 私钥必须在这里被拒，而不是一路通过、在 `listen_on` 里 panic。
    ///
    /// `-----BEGIN EC PRIVATE KEY-----` 是 openssl 生成 EC 密钥的默认输出格式，
    /// 宿主手工替换证书文件时极易撞上。
    #[test]
    fn rejects_sec1_private_key_instead_of_panicking_later() {
        let cert = Certificate::generate(now(), MAX_VALIDITY).unwrap();
        let cert_pem = cert
            .to_pem()
            .lines()
            .take_while(|l| !l.contains("PRIVATE"))
            .collect::<Vec<_>>()
            .join("\n");

        // 内容不必是真的 EC 私钥 —— `PrivateKeyDer::from_pem_slice` 只看 PEM 标签来定变体。
        let sec1 = "-----BEGIN EC PRIVATE KEY-----\nMHcCAQEEIA==\n-----END EC PRIVATE KEY-----\n";
        let err = Certificate::from_pem(&format!("{cert_pem}\n{sec1}")).unwrap_err();

        assert!(
            matches!(err, Error::Unusable(_)),
            "SEC1 私钥必须以 Unusable 拒绝（触发重新生成），实得 {err:?}"
        );
    }

    #[test]
    fn rejects_garbage_pem() {
        assert!(Certificate::from_pem("not a pem").is_err());
        assert!(Certificate::from_pem("").is_err());
    }

    /// Debug 不得泄漏私钥。
    #[test]
    fn debug_does_not_leak_private_key() {
        let cert = Certificate::generate(now(), MAX_VALIDITY).unwrap();
        let rendered = format!("{cert:?}");

        assert!(!rendered.contains("PRIVATE"), "{rendered}");
        assert!(rendered.contains("certhash"), "{rendered}");
    }
}
