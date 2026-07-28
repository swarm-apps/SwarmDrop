//! 传输配置。

use std::net::SocketAddr;
use std::time::Duration;

/// STUN 服务器。
///
/// spec 的 STUN 一节：浏览器节点无法用 identify 发现自己的公网地址（每条 WebRTC 连接
/// 用新端口），**只能靠 STUN**。因此这不是可选优化——没有 STUN 就只有 host candidate，
/// 跨 NAT 必然打不通。
///
/// 收发双方不必用同一台。
pub const DEFAULT_STUN_SERVERS: &[&str] = &["stun:stun.l.google.com:19302"];

/// 信令交换的整体超时。
///
/// 覆盖「开流 → offer → answer → ICE 收敛」全过程。超时后 reset 信令流并让 dial 失败，
/// 由上层决定是否退回 relay 中转（spec 步骤 8 明确把这个回退策略留给应用）。
pub const DEFAULT_SIGNALING_TIMEOUT: Duration = Duration::from_secs(30);

/// 传输配置。
#[derive(Debug, Clone)]
pub struct Config {
    stun_servers: Vec<String>,
    signaling_timeout: Duration,
    udp_bind_addrs: Vec<SocketAddr>,
    direct: Option<DirectConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stun_servers: DEFAULT_STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
            signaling_timeout: DEFAULT_SIGNALING_TIMEOUT,
            udp_bind_addrs: Vec::new(),
            direct: None,
        }
    }
}

/// direct 模式（`/webrtc-direct`）的配置。
///
/// 不配它，本传输只处理打洞地址；配了才会接管 `/webrtc-direct`。两种模式共用一个
/// [`Transport`](crate::Transport)，按地址段分派。
#[derive(Debug, Clone)]
pub struct DirectConfig {
    id_keys: libp2p_identity::Keypair,
    certificate_pem: Option<String>,
    max_message_size: Option<std::num::NonZeroUsize>,
}

impl DirectConfig {
    /// direct 模式必须知道本机身份——它要在 DataChannel 之上再跑一次 Noise 握手。
    ///
    /// 打洞模式不需要，因为那边的证书指纹经**已认证**的 relay 连接交换；direct 的
    /// certhash 写在 multiaddr 里，可能经任何不可信信道传播（spec FAQ 第一条）。
    pub fn new(id_keys: libp2p_identity::Keypair) -> Self {
        Self {
            id_keys,
            certificate_pem: None,
            max_message_size: None,
        }
    }

    /// 指定持久化的 DTLS 证书（PEM，含私钥）。
    ///
    /// **强烈建议配置。** 通告地址里的 certhash 由这张证书决定，不配就等于每次启动
    /// 换一个地址，对端记下的旧地址全部拨不通。宿主应把它存起来跨重启复用——
    /// 首次可用 `Certificate::generate().serialize_pem()` 生成（native）。
    pub fn with_certificate_pem(mut self, pem: impl Into<String>) -> Self {
        self.certificate_pem = Some(pem.into());
        self
    }

    /// 声明本端单条编码 DataChannel 消息的上限。
    ///
    /// Noise 握手后两端**自动协商取较小值**，所以这只是「本端愿意收多大」。留空则用
    /// `libp2p-webrtc-utils` 的默认值。
    ///
    /// 多端部署时应当各端声明同一个值——浏览器的安全上限最紧，让它决定全局。
    pub fn with_max_message_size(mut self, size: std::num::NonZeroUsize) -> Self {
        self.max_message_size = Some(size);
        self
    }

    pub fn id_keys(&self) -> &libp2p_identity::Keypair {
        &self.id_keys
    }

    pub fn certificate_pem(&self) -> Option<&str> {
        self.certificate_pem.as_deref()
    }

    pub(crate) fn stream_config(&self) -> libp2p_webrtc_utils::StreamConfig {
        match self.max_message_size {
            Some(size) => libp2p_webrtc_utils::StreamConfig::new(size),
            None => libp2p_webrtc_utils::StreamConfig::default(),
        }
    }
}

impl Config {
    /// 默认配置（公共 STUN + 30s 信令超时）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 覆盖 STUN 服务器清单。
    ///
    /// 传空表示不做 STUN——**只会产出 host candidate**，仅适用于同局域网。
    pub fn with_stun_servers(mut self, servers: impl IntoIterator<Item = String>) -> Self {
        self.stun_servers = servers.into_iter().collect();
        self
    }

    /// 覆盖信令超时。
    pub fn with_signaling_timeout(mut self, timeout: Duration) -> Self {
        self.signaling_timeout = timeout;
        self
    }

    pub fn stun_servers(&self) -> &[String] {
        &self.stun_servers
    }

    pub fn signaling_timeout(&self) -> Duration {
        self.signaling_timeout
    }

    /// 覆盖 ICE 绑定的本地地址。
    ///
    /// 留空则由后端枚举本机网卡。**不要传 `0.0.0.0`**——webrtc-rs 不会据此展开网卡，
    /// 而是把字面量写进 host candidate，对端无法使用，host 路径整条作废
    /// （spike 实测吞吐从 50 MiB/s 掉到 0.6 MiB/s）。
    pub fn with_udp_bind_addrs(mut self, addrs: impl IntoIterator<Item = SocketAddr>) -> Self {
        self.udp_bind_addrs = addrs.into_iter().collect();
        self
    }

    pub fn udp_bind_addrs(&self) -> &[SocketAddr] {
        &self.udp_bind_addrs
    }

    /// 启用 direct 模式（`/webrtc-direct`）。
    ///
    /// 不调用它，`/webrtc-direct` 地址会被本传输拒绝（留给官方 `libp2p-webrtc` 或
    /// 其他实现处理）。
    pub fn with_direct(mut self, direct: DirectConfig) -> Self {
        self.direct = Some(direct);
        self
    }

    pub fn direct(&self) -> Option<&DirectConfig> {
        self.direct.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_stun_and_timeout() {
        let c = Config::new();
        assert!(
            !c.stun_servers().is_empty(),
            "默认必须带 STUN，否则跨 NAT 打不通"
        );
        assert_eq!(c.signaling_timeout(), DEFAULT_SIGNALING_TIMEOUT);
    }

    #[test]
    fn builders_override() {
        let c = Config::new()
            .with_stun_servers(["stun:example.org:3478".to_string()])
            .with_signaling_timeout(Duration::from_secs(5));
        assert_eq!(c.stun_servers(), ["stun:example.org:3478"]);
        assert_eq!(c.signaling_timeout(), Duration::from_secs(5));
    }
}
