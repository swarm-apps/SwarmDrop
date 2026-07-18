//! [`Preset`]：可组合的配置包（`fn apply(self, Builder) -> Builder`，学 iroh）。
//!
//! preset 没有魔法——等价一串手写 setter，先应用、后续 setter 可覆盖。

use super::builder::Builder;

/// 配置包。
pub trait Preset {
    /// 把这组配置施加到 Builder 上。
    fn apply(self, builder: Builder) -> Builder;
}

/// 原生端默认：TCP+QUIC 全网卡监听、mDNS、AutoNAT、DCUtR、relay 客户端。
///
/// DHT 不在其中——按需 `Builder::dht(DhtConfig::default())` 启用。
#[derive(Debug, Clone, Copy, Default)]
pub struct Native;

impl Preset for Native {
    fn apply(self, builder: Builder) -> Builder {
        builder
            .listen(vec![
                "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
                "/ip4/0.0.0.0/udp/0/quic-v1"
                    .parse()
                    .expect("valid multiaddr"),
            ])
            .mdns(true)
            .autonat(true)
            .dcutr(true)
            .relay_client(true)
    }
}

/// 浏览器端默认：不 listen 本地 socket（做不到），relay 客户端开——
/// 被动接收连接靠 circuit relay listen（`ensure_relay_reservation` 触发）。
#[derive(Debug, Clone, Copy, Default)]
pub struct Browser;

impl Preset for Browser {
    fn apply(self, builder: Builder) -> Builder {
        builder.listen(Vec::new()).relay_client(true)
    }
}
