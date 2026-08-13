//! direct 模式的传输平面（浏览器）。
//!
//! 与 native 侧同名同形状，但**只能拨号**——浏览器没有监听 UDP 端口的能力，这是平台
//! 天花板而不是尚未实现。于是这里没有 UDP mux、没有 ICE-lite、没有证书持久化：
//!
//! | | native | 浏览器 |
//! |---|---|---|
//! | 角色 | 监听 + 拨号 | **只拨号** |
//! | 证书 | 自己生成、PEM 持久化 | `generateCertificate` 现生成，**每次都换** |
//! | 本端指纹 | 直接从证书算 | 只能从 `localDescription` 的 SDP 里 parse |
//! | ufrag | `set_ice_credentials` | **改写 SDP**（浏览器不给这个接口） |
//!
//! 证书每次都换没关系：**浏览器只拨不被拨**，没人会把它的 certhash 记进地址。
//!
//! [`crate::swarm::transport`] 只用一处 `cfg` 在这个类型与 native 版之间选择，事件
//! 处理代码两端共用。

use std::net::SocketAddr;
use std::task::{Context as TaskContext, Poll};

use futures::channel::mpsc;
use js_sys::{Object, Reflect};
use libp2p_core::muxing::StreamMuxerBox;
use libp2p_core::transport::ListenerId;
use libp2p_identity::PeerId;
use libp2p_webrtc_utils::{Fingerprint, noise};
use send_wrapper::SendWrapper;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    RtcConfiguration, RtcDataChannel, RtcDataChannelEvent, RtcDataChannelInit, RtcPeerConnection,
    RtcPeerConnectionState, RtcSdpType,
};

use super::callbacks::JsCallbacks;
use super::muxer::{Muxer, NOISE_CHANNEL_ID, Substream};
use crate::config::DirectConfig;
use crate::error::Error;
use crate::swarm::Connection;
use crate::swarm::direct::{DirectEvent, StateEvent, Upgrade, await_connected};

/// 浏览器侧的 direct 拨号器。
#[derive(Debug)]
pub(crate) struct DirectTransport {
    config: DirectConfig,
}

impl DirectTransport {
    pub(crate) fn new(config: &DirectConfig) -> Result<Self, Error> {
        Ok(Self {
            config: config.clone(),
        })
    }

    /// 浏览器不能监听 UDP 端口。
    ///
    /// 这不是「还没做」——`RTCPeerConnection` 根本没有服务端形态。返回错误而不是
    /// 静默忽略，免得上层以为地址通告出去了。
    pub(crate) fn listen_on(&mut self, _id: ListenerId, _socket: SocketAddr) -> Result<(), Error> {
        Err(Error::Connection(
            "浏览器无法监听 webrtc-direct：请在原生端监听，由浏览器发起拨号".into(),
        ))
    }

    pub(crate) fn remove_listener(&mut self, _id: ListenerId) -> bool {
        false
    }

    pub(crate) fn dial(&mut self, remote: SocketAddr, fingerprint: Fingerprint) -> Upgrade {
        let config = self.config.clone();
        // `RtcPeerConnection` 与它的回调闭包都不是 `Send`，而 `Upgrade` 要求 `Send`。
        // wasm 是单线程，非创建线程访问会 panic，故包起来是安全的（官方
        // `webrtc-websys` 同款做法）。
        Box::pin(SendWrapper::new(async move {
            outbound(remote, fingerprint, config).await
        }))
    }

    /// 没有监听器就没有事件。
    pub(crate) fn poll(&mut self, _cx: &mut TaskContext<'_>) -> Poll<DirectEvent> {
        Poll::Pending
    }
}

/// 浏览器作为客户端拨一条 direct 连接。
///
/// 步骤与 native 的 `upgrade::outbound` 一一对应，差别只在两处受平台限制的地方
/// （证书由浏览器生成、ufrag 靠改写 SDP 注入），见模块文档的对照表。
async fn outbound(
    remote: SocketAddr,
    server_fingerprint: Fingerprint,
    config: DirectConfig,
) -> Result<(PeerId, Connection), Error> {
    // 证书的摘要算法必须与对端 certhash 用的一致，否则 DTLS 校验必然失配。
    let mut pending =
        PendingConnection::new(new_peer_connection(&server_fingerprint.algorithm()).await?);
    let connected = upgrade(pending.pc(), remote, server_fingerprint, config).await?;
    // 成功了，连接归 muxer 管（见它的 `Drop`）。
    pending.disarm();
    Ok(connected)
}

/// 建连期间持有 `RTCPeerConnection`：只要没走到 [`Self::disarm`]，drop 就 `close()`。
///
/// 浏览器不会回收一条还在跑 ICE 的连接，光 drop 掉 Rust 句柄会攒下僵尸连接。
/// 而**只在错误分支关是不够的**：本函数的返回值是 libp2p `Transport::dial` 的
/// `Upgrade` future，`ConcurrentDial` 在同时拨的另一个地址先成功时会把它**直接丢弃**
/// ——那条路径上错误分支根本不执行。取消、`?` 早退、panic 三条路只有 RAII 一起覆盖得住。
struct PendingConnection {
    pc: RtcPeerConnection,
    armed: bool,
}

impl PendingConnection {
    fn new(pc: RtcPeerConnection) -> Self {
        Self { pc, armed: true }
    }

    fn pc(&self) -> &RtcPeerConnection {
        &self.pc
    }

    /// 交出所有权：此后由数据面负责关闭。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PendingConnection {
    fn drop(&mut self) {
        if self.armed {
            self.pc.close();
        }
    }
}

/// 建连主体。中间八个 `?` 的失败路径与整个 future 被取消的路径，都由调用方那个
/// [`PendingConnection`] 兜底关闭连接，这里只管把流程走完。
async fn upgrade(
    pc: &RtcPeerConnection,
    remote: SocketAddr,
    server_fingerprint: Fingerprint,
    config: DirectConfig,
) -> Result<(PeerId, Connection), Error> {
    let ufrag = libp2p_webrtc_utils::sdp::random_ufrag();
    tracing::debug!(%remote, "webrtc-direct: 浏览器发起出站连接");

    // 两个闭包**不能只绑在局部量上**——函数一返回它们就 drop，JS 侧回调随之静默失效。
    // `ondatachannel` 尤其致命：连接看着是好的，但 `incoming` 再也收不到对端开的子流。
    // 建连完成后连同 muxer 一起移交（见 `Muxer` 的 `_callbacks`）；中途失败则由
    // `JsCallbacks` 在此处 drop 时先解绑再释放。
    let (inbound_dc, on_data_channel) = attach_data_channel_sink(pc);
    let (mut states, on_state_change) = attach_state_sink(pc);
    let callbacks = JsCallbacks::new(
        pc.clone(),
        |pc: &RtcPeerConnection| {
            pc.set_ondatachannel(None);
            pc.set_onconnectionstatechange(None);
        },
        (on_data_channel, on_state_change),
    );

    // **必须在 create_offer 之前建**，否则 offer 里不带 m=application 行。
    let noise_channel = new_negotiated_channel(pc);

    let offer = JsFuture::from(pc.create_offer())
        .await
        .map_err(|e| js_err("创建 offer", e))?;
    let offer_sdp = sdp_of(&offer)?;

    // 浏览器不提供 `set_ice_credentials`，只能改写它生成的 SDP。服务端要靠这个 ufrag
    // 在共享端口上认出我们（它从入站 STUN 的 USERNAME 里读）。
    let munged_sdp = munge_ufrag(&offer_sdp, &ufrag);
    // 两份 SDP 都留 trace：direct 建连出问题时，第一件事就是比对它们
    // （ufrag 有没有换掉、fingerprint 在不在、m-line 端口对不对）。
    tracing::trace!(sdp = %munged_sdp, "webrtc-direct: 本端 offer（已注入 ufrag）");
    let munged = describe(RtcSdpType::Offer, &munged_sdp);
    JsFuture::from(pc.set_local_description(&munged))
        .await
        .map_err(|e| js_err("设置本地 offer", e))?;

    // 服务端的 answer 由我们本地算出来——direct 模式没有信令。
    let answer_sdp = libp2p_webrtc_utils::sdp::answer(
        remote,
        server_fingerprint,
        &ufrag,
        config.stream_config(),
    );
    tracing::trace!(sdp = %answer_sdp, "webrtc-direct: 为对端构造的 answer");
    let answer = describe(RtcSdpType::Answer, &answer_sdp);
    JsFuture::from(pc.set_remote_description(&answer))
        .await
        .map_err(|e| js_err("设置构造的 answer", e))?;

    // 本端指纹只能从 local description 里读——浏览器不把证书内容交出来。
    // 取 munge 之后的那份（`local_description()`），不是 `create_offer` 的原始返回。
    let client_fingerprint = local_fingerprint(pc)?;

    // 等 DTLS 握手完成再动 DataChannel。
    //
    // 少了这一步，失败的连接只会表现为「Noise 握手永远不返回」，最后由上层超时收场
    // ——拿不到任何原因。等在这里，`failed` 能立刻报出来。
    await_connected(&mut states).await?;

    // 用 `Substream` 而非裸 `Stream`：后者内部持有 JS 对象（经 `Rc`）不是 `Send`，
    // 而 `noise::outbound_with_message_size` 要求 `Send`。
    let stream = Substream::without_drop_listener(noise_channel, config.stream_config());

    let (peer, stream_config) = noise::outbound_with_message_size(
        config.id_keys().clone(),
        stream,
        server_fingerprint,
        client_fingerprint,
        config.stream_config(),
    )
    .await
    .map_err(|e| Error::Connection(format!("Noise 握手失败：{e}")))?;

    tracing::debug!(%remote, %peer, "webrtc-direct: 浏览器出站连接就绪");

    let muxer = Muxer::new(pc.clone(), inbound_dc, stream_config, callbacks);
    Ok((peer, Connection::new(peer, StreamMuxerBox::new(muxer))))
}

/// 建一个带自签证书的 `RTCPeerConnection`。
///
/// direct 模式**不配 STUN**：服务端已有可达地址，客户端只需一个 host candidate。
/// 配了反而要等 srflx 收集，白白拖慢建连。
async fn new_peer_connection(hash_algorithm: &str) -> Result<RtcPeerConnection, Error> {
    let algo = Object::new();
    for (key, value) in [
        ("name", "ECDSA"),
        ("namedCurve", "P-256"),
        ("hash", hash_algorithm),
    ] {
        Reflect::set(&algo, &key.into(), &value.into())
            .map_err(|e| js_err(&format!("设置证书参数 {key}"), e))?;
    }

    let certificate = JsFuture::from(
        RtcPeerConnection::generate_certificate_with_object(&algo)
            .map_err(|e| js_err("请求生成证书", e))?,
    )
    .await
    .map_err(|e| js_err("生成证书", e))?;

    let certificates = js_sys::Array::new();
    certificates.push(&certificate);
    let rtc_config = RtcConfiguration::new();
    rtc_config.set_certificates(&certificates);

    RtcPeerConnection::new_with_configuration(&rtc_config)
        .map_err(|e| js_err("构造 RTCPeerConnection", e))
}

/// 建 Noise 握手用的通道。
///
/// `negotiated` + id 0 —— **两端都必须建**，且都用 id 0。它不经 SDP 通告，所以对端
/// 不会在 `ondatachannel` 里看到它（`muxer` 那边仍按 id 显式跳过，见 `NOISE_CHANNEL_ID`）。
fn new_negotiated_channel(pc: &RtcPeerConnection) -> RtcDataChannel {
    let options = RtcDataChannelInit::new();
    options.set_negotiated(true);
    options.set_id(NOISE_CHANNEL_ID);
    pc.create_data_channel_with_data_channel_dict("", &options)
}

/// 把连接状态的**终态**引到一个 channel 里。
///
/// 中间态（`connecting` / `new`）滤掉——调用方只关心成没成。
fn attach_state_sink(
    pc: &RtcPeerConnection,
) -> (mpsc::UnboundedReceiver<StateEvent>, Closure<dyn FnMut()>) {
    let (tx, rx) = mpsc::unbounded();
    let pc_ref = pc.clone();
    let onstatechange = Closure::wrap(Box::new(move || {
        let event = match pc_ref.connection_state() {
            RtcPeerConnectionState::Connected => StateEvent::Connected,
            RtcPeerConnectionState::Failed => StateEvent::Failed,
            // `Disconnected` 可能自行恢复，不当作终态。
            other => {
                tracing::trace!(?other, "webrtc-direct: 连接状态变化");
                return;
            }
        };
        let _ = tx.unbounded_send(event);
    }) as Box<dyn FnMut()>);
    pc.set_onconnectionstatechange(Some(onstatechange.as_ref().unchecked_ref()));
    (rx, onstatechange)
}

/// 把对端开来的 DataChannel 引到一个 channel 里，供 muxer 消费。
///
/// 返回的闭包**必须一直持有**——drop 掉它 JS 侧回调就静默失效。
fn attach_data_channel_sink(
    pc: &RtcPeerConnection,
) -> (
    mpsc::UnboundedReceiver<RtcDataChannel>,
    Closure<dyn FnMut(RtcDataChannelEvent)>,
) {
    let (tx, rx) = mpsc::unbounded();
    let ondatachannel = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
        let _ = tx.unbounded_send(ev.channel());
    }) as Box<dyn FnMut(RtcDataChannelEvent)>);
    pc.set_ondatachannel(Some(ondatachannel.as_ref().unchecked_ref()));
    (rx, ondatachannel)
}

/// 从 `localDescription` 里读本端 DTLS 指纹。
fn local_fingerprint(pc: &RtcPeerConnection) -> Result<Fingerprint, Error> {
    let sdp = pc
        .local_description()
        .ok_or_else(|| Error::Connection("尚无 local description，取不到本端指纹".into()))?
        .sdp();

    parse_fingerprint(&sdp)
        .ok_or_else(|| Error::Connection("local description 里没有 a=fingerprint 行".into()))
}

/// 从 SDP 里切出 `a=fingerprint:<algo> <hex>` 的 hex 段并解析。
///
/// 解码那半是平台中立的，见 [`crate::protocol::addr::parse_sdp_fingerprint`]。
fn parse_fingerprint(sdp: &str) -> Option<Fingerprint> {
    let value = sdp
        .lines()
        .find_map(|line| line.trim().strip_prefix("a=fingerprint:"))?
        .split_whitespace()
        .nth(1)?;
    crate::protocol::addr::parse_sdp_fingerprint(value)
}

/// 把 SDP 里的 ICE 凭据换成我们选定的 ufrag。
///
/// 浏览器不提供设置 ICE 凭据的接口，只能改写它生成的 offer。`ice-pwd` 也用同一个值
/// ——spec 如此规定，两端本地构造 SDP 时都按这条填。
fn munge_ufrag(sdp: &str, ufrag: &str) -> String {
    let mut out = String::with_capacity(sdp.len());
    for line in sdp.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        if line.starts_with("a=ice-ufrag:") {
            out.push_str(&format!("a=ice-ufrag:{ufrag}\r\n"));
        } else if line.starts_with("a=ice-pwd:") {
            out.push_str(&format!("a=ice-pwd:{ufrag}\r\n"));
        } else {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out
}

fn sdp_of(desc: &wasm_bindgen::JsValue) -> Result<String, Error> {
    Reflect::get(desc, &"sdp".into())
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| Error::Connection("SDP 描述里没有 sdp 字段".into()))
}

fn describe(ty: RtcSdpType, sdp: &str) -> web_sys::RtcSessionDescriptionInit {
    let desc = web_sys::RtcSessionDescriptionInit::new(ty);
    desc.set_sdp(sdp);
    desc
}

fn js_err(what: &str, e: wasm_bindgen::JsValue) -> Error {
    Error::Connection(format!("{what}失败：{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 浏览器给的 SDP 用 `\r\n`、指纹是大写冒号分隔。
    #[test]
    fn parses_browser_style_fingerprint() {
        let sdp = "v=0\r\n\
a=fingerprint:sha-256 A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:\
21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89\r\n\
a=setup:actpass\r\n";

        let fp = parse_fingerprint(sdp).expect("应能解析");
        assert_eq!(
            fp.to_sdp_format(),
            "A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:\
21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89"
        );
        assert_eq!(fp.algorithm(), "sha-256");
    }

    /// 畸形输入返回 None 而不是 panic——这条数据来自浏览器，不该假定它规整。
    #[test]
    fn rejects_malformed_fingerprint_lines() {
        for sdp in [
            "v=0\r\n",
            "a=fingerprint:sha-256\r\n",
            "a=fingerprint:sha-256 zz:zz\r\n",
            "a=fingerprint:sha-256 AA:BB\r\n",
        ] {
            assert!(parse_fingerprint(sdp).is_none(), "{sdp:?}");
        }
    }

    /// ufrag 与 pwd 都要被换掉，其余行原样保留。
    #[test]
    fn munges_both_ice_credentials() {
        let sdp = "v=0\r\n\
a=ice-ufrag:browser-generated\r\n\
a=ice-pwd:browser-secret\r\n\
a=setup:actpass\r\n";

        let out = munge_ufrag(sdp, "libp2p+webrtc+v1/mine");

        assert!(out.contains("a=ice-ufrag:libp2p+webrtc+v1/mine\r\n"));
        assert!(out.contains("a=ice-pwd:libp2p+webrtc+v1/mine\r\n"));
        assert!(!out.contains("browser-generated"));
        assert!(!out.contains("browser-secret"));
        assert!(out.contains("a=setup:actpass\r\n"), "无关行必须原样保留");
        assert!(out.starts_with("v=0\r\n"));
    }

    /// 不能留下空行——SDP 解析器对多余的 `\r\n\r\n` 敏感。
    #[test]
    fn munging_drops_blank_lines() {
        let out = munge_ufrag("v=0\r\n\r\na=ice-ufrag:x\r\n\r\n", "u");
        assert!(!out.contains("\r\n\r\n"), "不应残留空行：{out:?}");
    }
}
