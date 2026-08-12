//! Transport 配置与证书持久化端口。

use std::sync::Arc;
use std::time::Duration;

use libp2p_identity::Keypair;

/// 从会话建立到 Noise 握手完成的整体超时。
///
/// 与 `webrtc-p2p` 的 direct 路径同为 20s。它盖住的是「QUIC 握手 + CONNECT + 开流 +
/// Noise 四条消息」，跨洲际链路上 4 个 RTT 也远在这个数以内；取大是为了容忍
/// 移动网络的首包重传。
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// 检查证书是否该轮换的间隔。
///
/// # 为什么是 10 分钟而不是 1 分钟
///
/// 这个定时器**注册了 waker**，于是它同时是「完全空闲时多久唤醒一次 transport」的下界。
/// 移动端也监听 WebTransport 之后，60 秒会成为待机时最密的唤醒源（identify 是 5 分钟、
/// kad 刷新是分钟级）—— 那是个功耗问题，不是 CPU 问题。
///
/// 上界由 [`ROTATION_OVERLAP`](crate::certificate) 定：`next` 提前 1 小时生效，所以检查
/// 间隔必须**远小于**一小时，否则可能整段重叠窗口都没醒过、错过提前切换。10 分钟留了
/// 6 倍余量。
///
/// 精度上完全够：轮换周期 14 天，晚十分钟没有任何可观测影响。
pub const ROTATION_CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// 证书对的持久化端口。
///
/// # 为什么是端口，而不是 `with_certificate_pem(String)`
///
/// `webrtc-direct` 的证书**永不改变**，所以宿主启动时给一次就够。这里的证书每 14 天换一次，
/// 那个形态表达不了回写 —— 而不回写就等于没持久化：每次重启 certhash 都会变，对端记下的
/// 地址全部失效。
///
/// trait 定义在本 crate、由宿主实现（依赖倒置），因此本 crate 仍然零外部依赖，可独立发布。
///
/// # 实现约定
///
/// - **格式是多段 PEM**，由 [`crate::certificate::Rotation`] 产出与消费。宿主原样读写，
///   不要解析、不要附加元数据 —— 有效期本就编码在 X.509 里。
/// - `load` 返回 `Ok(None)` 表示「还没有」，是首次启动的正常路径，不是错误。
/// - 私钥在里面。宿主要按存放私钥的规格保护它（桌面端是 `0600` 的文件）。
pub trait CertificateStore: Send + Sync + 'static {
    /// 读回上次保存的内容。首次启动返回 `Ok(None)`。
    fn load(&self) -> Result<Option<String>, StoreError>;

    /// 保存。**失败不会中断服务**（见 [`crate::Transport`] 的轮换处理），只会让下次启动
    /// 退化成「重新生成」。
    fn store(&self, pem: &str) -> Result<(), StoreError>;
}

/// 持久化端口的错误。
///
/// 刻意不枚举原因：宿主的存储介质千差万别（文件、keychain、数据库），而本 crate 对任何
/// 一种失败的处理都一样 —— 记 warn 并继续。
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StoreError(Box<dyn std::error::Error + Send + Sync>);

impl StoreError {
    pub fn new(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::new(e)
    }
}

/// 只在内存里存的 [`CertificateStore`]。
///
/// 两个用途：
/// 1. **测试与基准** —— 不必碰文件系统，也不必各处手写一份（本仓一度有 6 份逐字相同的）。
/// 2. **宿主实现的参考** —— `load`/`store` 的语义（`Ok(None)` = 还没有，不是错误）看这里
///    最快。
///
/// ⚠️ **不要用在生产**：进程一退证书就没了，下次启动 certhash 全变，等于没持久化。
#[derive(Debug, Default, Clone)]
pub struct MemoryCertificateStore(std::sync::Arc<std::sync::Mutex<Option<String>>>);

impl CertificateStore for MemoryCertificateStore {
    fn load(&self) -> Result<Option<String>, StoreError> {
        Ok(self.0.lock().expect("证书存储互斥量中毒").clone())
    }

    fn store(&self, pem: &str) -> Result<(), StoreError> {
        *self.0.lock().expect("证书存储互斥量中毒") = Some(pem.to_owned());
        Ok(())
    }
}

/// Transport 配置。
#[derive(Clone)]
pub struct Config {
    id_keys: Keypair,
    store: Option<Arc<dyn CertificateStore>>,
    handshake_timeout: Duration,
}

impl Config {
    /// 本机 libp2p 身份 —— Noise 握手要用它。
    ///
    /// 与 `webrtc-p2p` 的打洞模式不同，这里的身份认证**不能省**：certhash 写在 multiaddr
    /// 里、可经任何不可信信道传播，TLS 只能证明「对面持有这张证书」。
    pub fn new(id_keys: Keypair) -> Self {
        Self {
            id_keys,
            store: None,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
        }
    }

    /// 接上证书持久化端口。
    ///
    /// **强烈建议接上。** 不接的话每次启动都会现生成一对证书，通告地址里的 certhash 随之
    /// 改变，对端此前记下的地址全部拨不通。不接时会留一条 warn。
    pub fn with_certificate_store(mut self, store: Arc<dyn CertificateStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// 覆盖握手超时。
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    pub fn id_keys(&self) -> &Keypair {
        &self.id_keys
    }

    pub fn handshake_timeout(&self) -> Duration {
        self.handshake_timeout
    }

    pub(crate) fn store(&self) -> Option<&Arc<dyn CertificateStore>> {
        self.store.as_ref()
    }
}

impl std::fmt::Debug for Config {
    /// 不打印 `id_keys` —— 它含私钥。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("peer_id", &self.id_keys.public().to_peer_id())
            .field("has_certificate_store", &self.store.is_some())
            .field("handshake_timeout", &self.handshake_timeout)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_applied() {
        let config = Config::new(Keypair::generate_ed25519());

        assert_eq!(config.handshake_timeout(), DEFAULT_HANDSHAKE_TIMEOUT);
        assert!(config.store().is_none());
    }

    /// Debug 不得泄漏私钥。
    #[test]
    fn debug_does_not_leak_private_key() {
        let keys = Keypair::generate_ed25519();
        let rendered = format!("{:?}", Config::new(keys.clone()));

        assert!(rendered.contains(&keys.public().to_peer_id().to_string()));
        assert!(!rendered.contains("Keypair"), "{rendered}");
    }
}
