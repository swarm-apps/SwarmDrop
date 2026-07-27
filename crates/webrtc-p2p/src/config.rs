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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stun_servers: DEFAULT_STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
            signaling_timeout: DEFAULT_SIGNALING_TIMEOUT,
            udp_bind_addrs: Vec::new(),
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
