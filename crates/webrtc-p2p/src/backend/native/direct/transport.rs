//! direct 模式的传输平面（native）。
//!
//! 它是 [`crate::Transport`] 里 `/webrtc-direct` 那一半的实现：管监听端口、把入站
//! 连接变成 `Incoming` 事件、把出站请求变成一个 future。打洞那一半在
//! [`crate::swarm::behaviour`]，两者互不知晓。
//!
//! wasm 侧有一个同名同形状的对应物（[`crate::backend::wasm::direct`]），只支持拨号
//! ——浏览器不能 listen。上层因此只需一处 `cfg` 选实现，其余代码两端共用。

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::task::{Context as TaskContext, Poll};

use futures::FutureExt;
use libp2p_core::Multiaddr;
use libp2p_core::transport::ListenerId;
use libp2p_webrtc_utils::Fingerprint;
use webrtc::runtime::default_runtime;

use super::certificate::Certificate;
use super::udp_mux::{UdpMux, UdpMuxEvent};
use super::upgrade;
use crate::config::DirectConfig;
use crate::error::Error;
use crate::protocol::addr;
use crate::swarm::direct::{DirectEvent, Upgrade};

/// direct 模式的监听与拨号。
pub(crate) struct DirectTransport {
    ctx: upgrade::Context,
    listeners: HashMap<ListenerId, Listener>,
    /// 待交付的事件（`listen_on` 是同步调用，事件只能先攒着）。
    pending: VecDeque<DirectEvent>,
}

struct Listener {
    mux: UdpMux,
    /// 报给 swarm 的「这条连接归哪个监听器」。
    ///
    /// 只存一条：通告地址在 `listen_on` 当场就全部发成 `NewAddress` 了，之后没人
    /// 再需要整份清单。入站连接经哪张网卡进来在这一层也看不出。
    local_addr: Multiaddr,
}

impl DirectTransport {
    /// 由配置构造。
    ///
    /// 证书取自 [`DirectConfig::certificate_pem`]；没给就现生成一张，并留痕提醒
    /// ——重启后 certhash 会变，对端记下的地址会全部失效。
    pub(crate) fn new(config: &DirectConfig) -> Result<Self, Error> {
        let certificate = match config.certificate_pem() {
            Some(pem) => Certificate::from_pem(pem)
                .map_err(|e| Error::Connection(format!("加载 webrtc-direct 证书失败：{e}")))?,
            None => {
                tracing::warn!(
                    "未提供 webrtc-direct 证书，本次运行现生成一张：\
                     重启后 certhash 会变，对端已记下的地址将失效。\
                     生产环境请持久化 `DirectConfig::with_certificate_pem`"
                );
                Certificate::generate()
                    .map_err(|e| Error::Connection(format!("生成 webrtc-direct 证书失败：{e}")))?
            }
        };

        let runtime = default_runtime().ok_or_else(|| {
            Error::Connection(
                "未检测到 async 运行时：webrtc-direct 需在 tokio 或 smol 中运行".into(),
            )
        })?;

        Ok(Self {
            ctx: upgrade::Context {
                certificate,
                id_keys: config.id_keys().clone(),
                stream_config: config.stream_config(),
                runtime,
            },
            listeners: HashMap::new(),
            pending: VecDeque::new(),
        })
    }

    /// 本机证书指纹，也就是通告地址里那个 certhash。
    pub(crate) fn fingerprint(&self) -> Fingerprint {
        self.ctx.certificate.fingerprint()
    }

    /// 开始监听。
    pub(crate) fn listen_on(&mut self, id: ListenerId, socket: SocketAddr) -> Result<(), Error> {
        let mux = UdpMux::bind(socket, &self.ctx.runtime)
            .map_err(|e| Error::Connection(format!("绑定 {socket} 失败：{e}")))?;
        let bound = mux.listen_addr();

        let announced = announce_addrs(bound, self.fingerprint());
        // `announce_addrs` 的三条 return 路径都保证非空。
        let local_addr = announced.first().cloned().expect("通告地址不会为空");
        tracing::debug!(%bound, addrs = announced.len(), "webrtc-direct: 开始监听");
        for listen_addr in announced {
            self.pending.push_back(DirectEvent::NewAddress {
                listener_id: id,
                listen_addr,
            });
        }

        self.listeners.insert(id, Listener { mux, local_addr });
        Ok(())
    }

    pub(crate) fn remove_listener(&mut self, id: ListenerId) -> bool {
        self.listeners.remove(&id).is_some()
    }

    /// 拨一个 `/webrtc-direct` 地址。
    ///
    /// 不需要本机已在监听——dialer 用独立端口（与官方实现的 `NoListeners` 限制不同）。
    pub(crate) fn dial(&mut self, remote: SocketAddr, fingerprint: Fingerprint) -> Upgrade {
        let ctx = self.ctx.clone();
        async move { upgrade::outbound(remote, fingerprint, ctx).await }.boxed()
    }

    /// 驱动所有监听端口。
    pub(crate) fn poll(&mut self, cx: &mut TaskContext<'_>) -> Poll<DirectEvent> {
        if let Some(event) = self.pending.pop_front() {
            return Poll::Ready(event);
        }

        let mut closed = Vec::new();
        let mut event = None;

        for (id, listener) in &mut self.listeners {
            match listener.mux.poll(cx) {
                Poll::Ready(UdpMuxEvent::NewAddr { addr, ufrag }) => {
                    let socket = listener.mux.register(&ufrag);
                    let ctx = self.ctx.clone();
                    event = Some(DirectEvent::Incoming {
                        listener_id: *id,
                        local_addr: listener.local_addr.clone(),
                        upgrade: async move { upgrade::inbound(addr, ufrag, socket, ctx).await }
                            .boxed(),
                        // 对端的 certhash 我们不知道（它没写在任何地方），留空。
                        send_back_addr: addr::direct_addr(addr, None),
                    });
                    break;
                }
                Poll::Ready(UdpMuxEvent::Error(e)) => {
                    closed.push((*id, Error::Connection(format!("监听端口读失败：{e}"))));
                }
                Poll::Pending => {}
            }
        }

        for (id, reason) in closed {
            self.listeners.remove(&id);
            self.pending.push_back(DirectEvent::ListenerClosed {
                listener_id: id,
                reason: Err(reason),
            });
        }

        match event {
            Some(e) => Poll::Ready(e),
            None => self.pending.pop_front().map_or(Poll::Pending, Poll::Ready),
        }
    }
}

impl std::fmt::Debug for DirectTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectTransport")
            .field("listeners", &self.listeners.len())
            .field("certhash", &self.fingerprint().to_multihash())
            .finish()
    }
}

/// 由实际绑定的地址推出要通告的 multiaddr 清单。
///
/// 绑在通配地址（`0.0.0.0` / `::`）上时**必须展开成具体网卡地址**——通配地址通告出去
/// 对端没法拨。展开失败（拿不到网卡列表）时原样通告，至少不是静默什么都不报。
fn announce_addrs(bound: SocketAddr, fingerprint: Fingerprint) -> Vec<Multiaddr> {
    if !bound.ip().is_unspecified() {
        return vec![addr::direct_addr(bound, Some(fingerprint))];
    }

    let want_v4 = bound.is_ipv4();
    let addrs: Vec<Multiaddr> = if_addrs::get_if_addrs()
        .map(|ifaces| {
            ifaces
                .into_iter()
                .map(|i| i.addr.ip())
                .filter(|ip| ip.is_ipv4() == want_v4)
                // 回环通告出去只对同机有意义，但那正是集成测试与「本机浏览器连本机
                // 桌面」的场景，留着。
                .map(|ip: IpAddr| {
                    addr::direct_addr(SocketAddr::new(ip, bound.port()), Some(fingerprint))
                })
                .collect()
        })
        .unwrap_or_default();

    if addrs.is_empty() {
        tracing::warn!(%bound, "未能枚举网卡，webrtc-direct 直接通告通配地址（对端多半拨不通）");
        return vec![addr::direct_addr(bound, Some(fingerprint))];
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p_core::multiaddr::Protocol;

    fn fingerprint() -> Fingerprint {
        Fingerprint::raw([0x11; 32])
    }

    /// 具体地址原样通告，且必须带 certhash——不带的话对端无法校验 DTLS 指纹。
    #[test]
    fn concrete_addr_is_announced_verbatim() {
        let bound: SocketAddr = "192.168.1.5:4003".parse().unwrap();
        let addrs = announce_addrs(bound, fingerprint());

        assert_eq!(addrs.len(), 1);
        assert_eq!(
            addrs[0].to_string(),
            addr::direct_addr(bound, Some(fingerprint())).to_string()
        );
        assert!(addrs[0].iter().any(|p| matches!(p, Protocol::Certhash(_))));
    }

    /// 通配地址必须展开：`/ip4/0.0.0.0/...` 通告出去对端根本没法拨。
    #[test]
    fn wildcard_addr_is_expanded_to_interfaces() {
        let addrs = announce_addrs("0.0.0.0:4003".parse().unwrap(), fingerprint());

        assert!(!addrs.is_empty());
        for a in &addrs {
            match a.iter().next() {
                Some(Protocol::Ip4(ip)) => {
                    assert!(!ip.is_unspecified(), "通告地址不能是通配符：{a}");
                }
                other => panic!("应是 IPv4 地址，实得 {other:?}"),
            }
            // 端口必须保留成绑定的那个
            assert!(a.iter().any(|p| p == Protocol::Udp(4003)));
            assert!(a.iter().any(|p| matches!(p, Protocol::Certhash(_))));
        }
    }
}
