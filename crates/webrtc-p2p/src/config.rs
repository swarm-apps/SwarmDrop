//! Transport configuration.

use std::net::SocketAddr;
use std::time::Duration;

/// STUN servers.
///
/// From the STUN section of the spec: a browser node cannot discover its own public
/// address via identify (every WebRTC connection uses a fresh port), so it **can only
/// rely on STUN**. This is therefore not an optional optimization — without STUN there
/// are only host candidates, and crossing a NAT is guaranteed to fail.
///
/// The two sides need not use the same server.
pub const DEFAULT_STUN_SERVERS: &[&str] = &["stun:stun.l.google.com:19302"];

/// Overall timeout for the signaling exchange.
///
/// Covers the whole "open stream → offer → answer → ICE converges" sequence. On timeout
/// the signaling stream is reset and the dial fails, leaving it to the layer above to
/// decide whether to fall back to relaying (spec step 8 explicitly leaves that fallback
/// policy to the application).
pub const DEFAULT_SIGNALING_TIMEOUT: Duration = Duration::from_secs(30);

/// Transport configuration.
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

/// Configuration for direct mode (`/webrtc-direct`).
///
/// Without it this transport only handles hole-punching addresses; with it, it also
/// takes over `/webrtc-direct`. Both modes share a single
/// [`Transport`](crate::Transport), dispatched on the address segments.
#[derive(Debug, Clone)]
pub struct DirectConfig {
    id_keys: libp2p_identity::Keypair,
    certificate_pem: Option<String>,
    max_message_size: Option<std::num::NonZeroUsize>,
}

impl DirectConfig {
    /// Direct mode must know the local identity — it runs a second Noise handshake on
    /// top of the DataChannel.
    ///
    /// Hole-punching mode does not, because there the certificate fingerprint is
    /// exchanged over an **authenticated** relay connection; a direct-mode certhash sits
    /// in the multiaddr and may travel over any untrusted channel (spec FAQ, first
    /// entry).
    pub fn new(id_keys: libp2p_identity::Keypair) -> Self {
        Self {
            id_keys,
            certificate_pem: None,
            max_message_size: None,
        }
    }

    /// Supplies a persisted DTLS certificate (PEM, private key included).
    ///
    /// **Strongly recommended.** The certhash in the advertised address is derived from
    /// this certificate, so leaving it unset means a new address on every start and every
    /// previously recorded address becoming undialable. The host should store it and
    /// reuse it across restarts — generate the first one with
    /// `Certificate::generate().serialize_pem()` (native).
    pub fn with_certificate_pem(mut self, pem: impl Into<String>) -> Self {
        self.certificate_pem = Some(pem.into());
        self
    }

    /// Declares this side's upper bound on a single encoded DataChannel message.
    ///
    /// After the Noise handshake both sides **negotiate down to the smaller value**, so
    /// this is only "how large a message this side is willing to receive". Leave it unset
    /// to use the `libp2p-webrtc-utils` default.
    ///
    /// In a multi-platform deployment every end should declare the same value — the
    /// browser's safety limit is the tightest, so let it set the global one.
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
    /// Default configuration (public STUN + 30s signaling timeout).
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the STUN server list.
    ///
    /// Passing an empty list disables STUN — **only host candidates will be produced**,
    /// which is usable on the same LAN only.
    pub fn with_stun_servers(mut self, servers: impl IntoIterator<Item = String>) -> Self {
        self.stun_servers = servers.into_iter().collect();
        self
    }

    /// Overrides the signaling timeout.
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

    /// Overrides the local addresses ICE binds to.
    ///
    /// Leave it empty to let the backend enumerate the local interfaces. **Do not pass
    /// `0.0.0.0`** — webrtc-rs does not expand it into interfaces; it writes the literal
    /// into the host candidate, which the remote cannot use, invalidating the entire host
    /// path (measured in a spike: throughput dropped from 50 MiB/s to 0.6 MiB/s).
    pub fn with_udp_bind_addrs(mut self, addrs: impl IntoIterator<Item = SocketAddr>) -> Self {
        self.udp_bind_addrs = addrs.into_iter().collect();
        self
    }

    pub fn udp_bind_addrs(&self) -> &[SocketAddr] {
        &self.udp_bind_addrs
    }

    /// Enables direct mode (`/webrtc-direct`).
    ///
    /// Without this call, `/webrtc-direct` addresses are rejected by this transport
    /// (left to the official `libp2p-webrtc` or another implementation).
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
            "the default must include STUN, or crossing a NAT cannot work"
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
