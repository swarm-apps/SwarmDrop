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
        let listen = vec![
            "/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"),
            "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("valid multiaddr"),
            // 浏览器到原生端的局域网直连入口。certhash 由持久化 PEM 在 transport
            // 层补入最终监听地址，邀请会随 dialable 地址集自然携带它。
            "/ip4/0.0.0.0/udp/0/webrtc-direct"
                .parse()
                .expect("valid multiaddr"),
            // Android 不编译 WebSocket transport，因此不能在此添加 /ws listener。
            #[cfg(not(target_os = "android"))]
            "/ip4/0.0.0.0/tcp/0/ws".parse().expect("valid multiaddr"),
        ];
        // WebSocket listener：同网浏览器的 LAN 直连入口（ws:// 私有 IP 豁免 mixed content，
        // spike 实证）。浏览器拨不了裸 TCP/QUIC，ws 是它够到本机的唯一免证书路径。
        // Android 的 websocket transport 未接（见 transport.rs `with_websocket` 跳过），不 listen。
        builder
            .listen(listen)
            .mdns(true)
            .autonat(true)
            .dcutr(true)
            .relay_client(true)
    }
}

/// 浏览器端默认：不 listen 本地 socket（做不到），relay 客户端开——
/// 被动接收连接靠 circuit relay listen（`ensure_relay_reservation` 触发）。
///
/// connect 超时下调到 15s（默认 30s 对浏览器交互太长）——这是「任何 JS
/// Promise 在有限时间内 settle」不变量的内核兜底，不依赖调用方传 AbortSignal。
#[derive(Debug, Clone, Copy, Default)]
pub struct Browser;

impl Preset for Browser {
    fn apply(self, builder: Builder) -> Builder {
        builder
            .listen(Vec::new())
            .relay_client(true)
            .connect_timeout(std::time::Duration::from_secs(15))
    }
}
