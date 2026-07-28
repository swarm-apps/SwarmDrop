//! 数据面：把 `PeerConnection` 的 DataChannel 暴露为 libp2p 的 [`StreamMuxer`]。
//!
//! 每条 libp2p 子流对应一条 DataChannel，字节流由 [`PollDataChannel`] 适配、framing 由
//! `libp2p_webrtc_utils::Stream` 承担。

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use libp2p_core::muxing::{StreamMuxer, StreamMuxerEvent};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::PeerConnection;

use rtc::data_channel::RTCDataChannelId;

use super::data_channel::PollDataChannel;
use crate::error::Error;

/// 「取回一条入站通道的 label」这件事的进行中状态。
type LabelCheck = BoxFuture<'static, (Arc<dyn DataChannel>, Option<String>)>;

/// spec 步骤 4 那条只为让 SDP 带上 ICE 信息而建的通道。
///
/// **它不是数据流**：步骤 8 要求建连后关闭它。若把它当子流交给上层，对端会收到一条
/// 永远没有数据的流，libp2p 的协议协商会卡在那儿。
pub(crate) const INIT_CHANNEL_LABEL: &str = "init";

/// 一条 WebRTC 连接的数据面。
pub(crate) struct Muxer {
    pc: Arc<dyn PeerConnection>,
    /// `on_data_channel` 投递来的通道。
    ///
    /// ⚠️ 名字里的「入站」不能当保证：webrtc 0.20 的 driver 对**每一个** `OnOpen`
    /// 都调 `handler.on_data_channel`，不分通道是本端建的还是对端建的。修复已提上游
    /// （<https://github.com/webrtc-rs/webrtc/pull/825>），本仓 pin 的集成分支已含。
    ///
    /// 即便如此，[`Muxer::local_channels`] 那道过滤**不要删**——它守的是「本端开的流
    /// 不是入站流」这条不变式，不是某个版本的 bug 补丁。
    incoming: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    /// 本端开出去的通道 id。
    ///
    /// 少了这道过滤，`poll_inbound` 会把自己刚开的**出站**流当成入站流交给上层：
    /// 上层于是在一条对端根本不知情的流上等协议协商，而真正的入站流被这条假流挤在
    /// 后面。direct 的 Noise 通道（`negotiated` id 0）同理——它握手完就关了，
    /// 交出去的症状是「连接建好了，第一条子流一读就 `UnexpectedEof`」。
    ///
    /// 按 id 而不是按 label 过滤：`poll_outbound` 的 label 是本端自己编的，
    /// 对端完全可以用同名 label 开流。
    local_channels: HashSet<RTCDataChannelId>,
    /// 正在创建的出站 DataChannel。
    creating: Option<BoxFuture<'static, Result<Arc<dyn DataChannel>, Error>>>,
    /// 正在取 label 的入站 DataChannel。
    ///
    /// `label()` 是 `async fn`（要锁 PeerConnection 内部状态），**不能用
    /// `now_or_never()` 图省事**——锁一有竞争它就返回 `None`，init 通道会漏过筛子被
    /// 当成数据流交出去，对端从此挂在一条永不产生数据的流上。且这偶发。
    checking: Option<LabelCheck>,
    /// 正在关闭 PeerConnection。
    closing: Option<BoxFuture<'static, ()>>,
    /// 出站通道的自增编号，仅用于生成互不重复的 label。
    next_outbound_id: u64,
    /// 子流的 drop 通知。必须持续驱动，否则子流被 drop 时底层通道不会关闭，
    /// 连接上会积累永不释放的 DataChannel。
    drop_listeners: FuturesUnordered<libp2p_webrtc_utils::DropListener<PollDataChannel>>,
    /// 子流的消息尺寸策略。
    stream_config: libp2p_webrtc_utils::StreamConfig,
}

impl Muxer {
    /// 打洞模式用：消息尺寸取默认值。
    ///
    /// 那条路径没有 Noise 握手，也就没有可协商的上限。
    pub(crate) fn new(
        pc: Arc<dyn PeerConnection>,
        incoming: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
    ) -> Self {
        // 打洞路径的 init 通道靠 label 过滤（它的 id 是自动分配的，事先不知道）。
        Self::with_config(
            pc,
            incoming,
            libp2p_webrtc_utils::StreamConfig::default(),
            HashSet::new(),
        )
    }

    /// direct 模式用：沿用 Noise 握手**协商出来**的消息尺寸上限。
    ///
    /// 两端必须一致，否则合法的大消息会被一侧判成协议违规而重置子流。
    /// `local_channels` 是建连期间由本端开出、不应被当成子流的通道 id
    /// （direct 模式的 Noise 通道）。
    pub(crate) fn with_config(
        pc: Arc<dyn PeerConnection>,
        incoming: mpsc::UnboundedReceiver<Arc<dyn DataChannel>>,
        stream_config: libp2p_webrtc_utils::StreamConfig,
        local_channels: HashSet<RTCDataChannelId>,
    ) -> Self {
        Self {
            pc,
            incoming,
            creating: None,
            checking: None,
            closing: None,
            next_outbound_id: 0,
            drop_listeners: FuturesUnordered::new(),
            stream_config,
            local_channels,
        }
    }

    /// 把一条 DataChannel 包成 libp2p 子流，并登记它的 drop 通知。
    fn wrap(&mut self, dc: Arc<dyn DataChannel>) -> libp2p_webrtc_utils::Stream<PollDataChannel> {
        let (stream, drop_listener) =
            libp2p_webrtc_utils::Stream::with_config(PollDataChannel::new(dc), self.stream_config);
        self.drop_listeners.push(drop_listener);
        stream
    }
}

impl StreamMuxer for Muxer {
    type Substream = libp2p_webrtc_utils::Stream<PollDataChannel>;
    type Error = Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        loop {
            // 先把正在查 label 的那条处理完，再取下一条。
            if let Some(fut) = this.checking.as_mut() {
                let (dc, label) = ready!(fut.poll_unpin(cx));
                this.checking = None;
                // init 通道不是数据流，跳过（见 INIT_CHANNEL_LABEL 的说明）。
                if label.as_deref() == Some(INIT_CHANNEL_LABEL) {
                    continue;
                }
                // 本端开的通道不是入站子流，跳过（见 `local_channels`）。
                if this.local_channels.contains(&dc.id()) {
                    tracing::trace!(id = ?dc.id(), "跳过本端开的通道，它不是入站子流");
                    continue;
                }
                return Poll::Ready(Ok(this.wrap(dc)));
            }

            let Some(dc) = ready!(this.incoming.poll_next_unpin(cx)) else {
                return Poll::Ready(Err(Error::Connection("连接已关闭".into())));
            };
            this.checking = Some(
                async move {
                    let label = dc.label().await.ok();
                    (dc, label)
                }
                .boxed(),
            );
        }
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();
        if this.creating.is_none() {
            let pc = this.pc.clone();
            let label = format!("libp2p-{}", this.next_outbound_id);
            this.next_outbound_id += 1;
            this.creating = Some(
                async move {
                    let dc = pc
                        .create_data_channel(&label, None)
                        .await
                        .map_err(|e| Error::Connection(format!("创建 DataChannel 失败：{e}")))?;
                    // 必须等真 open 再交出去——未 open 时写入会**静默丢弃**，
                    // 上层看到的是「刚开的流对端读到 EOF」。详见 `data_channel::await_open`。
                    super::data_channel::await_open(&dc).await.map_err(|e| {
                        Error::Connection(format!("等待 DataChannel 打开失败：{e}"))
                    })?;
                    Ok(dc)
                }
                .boxed(),
            );
        }
        let fut = this.creating.as_mut().expect("刚置入");
        let dc = ready!(fut.poll_unpin(cx));
        this.creating = None;
        Poll::Ready(dc.map(|dc| {
            // 登记后 `poll_inbound` 才认得出这条是自己开的——driver 马上会把它
            // 从 `on_data_channel` 回灌回来。
            this.local_channels.insert(dc.id());
            this.wrap(dc)
        }))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // 必须主动关：`PeerConnection` 持有 UDP socket 与 ICE agent，只靠 Drop 不保证
        // 及时释放。关闭失败无从补救也无需上报——连接本来就要没了。
        let this = self.get_mut();
        if this.closing.is_none() {
            let pc = this.pc.clone();
            this.closing = Some(
                async move {
                    if let Err(e) = pc.close().await {
                        tracing::debug!("关闭 PeerConnection 失败：{e}");
                    }
                }
                .boxed(),
            );
        }
        ready!(this.closing.as_mut().expect("刚置入").poll_unpin(cx));
        this.closing = None;
        Poll::Ready(Ok(()))
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        let this = self.get_mut();
        // 驱动 drop 通知：子流被 drop 时靠它关闭底层 DataChannel。
        // 返回值无需处理——完成即达成目的，出错也只说明通道已经没了。
        while let Poll::Ready(Some(_)) = this.drop_listeners.poll_next_unpin(cx) {}
        Poll::Pending
    }
}
