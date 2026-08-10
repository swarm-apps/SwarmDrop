//! 把一个 UDP 端口复用给多个 `PeerConnection`。
//!
//! direct 模式的监听地址里写死了端口号（`/ip4/…/udp/4003/webrtc-direct/…`），所有
//! 客户端都往那一个端口发包，但每条连接要一个独立的 `PeerConnection`。于是需要在
//! 端口之上做一层分流。
//!
//! # 分流依据
//!
//! | 包类型 | 依据 |
//! |---|---|
//! | ICE binding request（首包） | STUN `USERNAME` 属性里的 **local ufrag** |
//! | 其余（DTLS / SCTP / 后续 STUN） | **源地址**（首包建立的映射） |
//!
//! ufrag 由客户端生成并写进它本地构造的 SDP，所以服务端是**从首包里学到**它的，
//! 不是事先约定的——这正是 direct 模式不需要信令的原因。
//!
//! # 为什么不是移植官方那 579 行
//!
//! 官方 `libp2p-webrtc` 的 `udp_mux.rs` 有 579 行，其中大半在适配 webrtc-rs **0.17**
//! 的 `UDPMux` / `UDPMuxWriter` / `UDPMuxConn` trait 体系（req_res_chan、注册回调、
//! 弱引用回环…）。
//!
//! 0.20 把这套东西整个删了（`SettingEngine::set_udp_network` 在 rtc 0.20 里是一段
//! 注释掉的 TODO），换成了更下层的 [`Runtime::wrap_udp_socket`] 注入点：
//! `PeerConnection` 拿到的 socket 由 `Runtime` 提供。于是复用一个端口的办法变成
//! **给每条连接发一个假 socket**（[`MuxedSocket`]）——发包转给共享 socket，收包从
//! 自己的支路取。那套 trait 适配层随之消失，只剩下真正的分流逻辑。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::future::Future;
use std::io;
use std::io::IoSliceMut;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc;
use rtc::stun::attributes::ATTR_USERNAME;
use rtc::stun::message::{Message as StunMessage, is_stun_message};
use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    Transmit,
};

// ── 下面这三个常量与两个函数：为什么本仓自己持有一份 ───────────────────────
//
// 它们对应 webrtc-rs 内部的 `gro_recv_buf_len` / `is_retryable_socket_recv_error`。
// 本仓曾提 [webrtc#853](https://github.com/webrtc-rs/webrtc/pull/853)（原 #850）想把
// 它们提为 `pub` 直接复用，**上游拒绝了**，理由成立：这两件事是 **driver 的策略**而非
// `AsyncUdpSocket` 契约的一部分，公开就等于把内部分配策略冻进 1.0 的兼容承诺。
//
// 而本 mux 把**一个** UDP socket 多路复用给多个 PeerConnection，扮演的正是 driver 的角色
// ——自己的接收循环，就该自己拥有缓冲尺寸与错误分类。上游同时把两条规则写进了公开文档
// （`AsyncUdpSocket::poll_recv` 的 `# Errors`、`max_gro_segments` 的 `# Buffer sizing`，
// 见 webrtc-rs commit `ef8ba660`），本文件按那份文档实现，**不是照源码抄**。
//
// ⚠️ 两者都**没有反馈回路**：缓冲算小了内核静默丢尾部段，判据漏一种就把公网监听端口
// 永久关掉，两者都不报错。本仓最早那版两个都错了（拿单包上限当段长、错误集漏三个变体）。
// 文件末尾那两条测试是唯一的护栏，**改这里必须同时改测试**。

/// 单个数据报的接收上限。WebRTC 的包远小于此（受 MTU 约束），取值对齐官方实现。
const RECEIVE_MTU: usize = 8192;

/// GRO 单段的长度上限，等于一个路径 MTU——**合并出来的段不可能比这更大**。
///
/// 用它而不是 [`RECEIVE_MTU`] 去乘段数：后者是「单个数据报的上限」，拿来当段长会把
/// 缓冲算大 5 倍多（64 段时 512 KiB vs 94 KiB）。
///
/// 上游 `max_gro_segments` 的文档把这条说死了：**「合并出来的段受路径 MTU 约束，
/// 而不是受应用发送的最大数据报约束」**，并注明 MTU > 1500 的路径不支持 GRO。
const GRO_SEGMENT_LEN: usize = 1500;

/// 一次读最多接受多少个 GRO 段。
///
/// 同时是**分配上限**：`max_gro_segments()` 来自 `AsyncUdpSocket` 实现，不该被直接
/// 当成分配乘数（上游文档原话：「the count is an allocation multiplier」，且内部同样 clamp）。
/// 64 是内核 `UDP_SEGMENT` 的合并上限。
const MAX_GRO_SEGMENTS: usize = 64;

/// 一次 [`UdpMux::poll`] 最多连续读多少轮就让出。
///
/// 没有这个上限，公网 4003 端口上一股持续的流量就能把 `Transport::poll` 永久留在
/// 读循环里——**节点其余所有传输、连接、behaviour 一起饿死**，而这是个未认证的
/// 输入源。达到上限时立刻自唤醒并让出，数据不会滞留。取值对齐 webrtc-rs 的
/// `MAX_UDP_RECV_BURST`。
const MAX_RECV_BURST: usize = 64;

/// 每条支路的收包缓冲深度。
///
/// 满了就**丢包**而不是阻塞读循环——一条慢支路卡住整个端口是不可接受的（其余连接
/// 会一起饿死）。UDP 本就允许丢包，DTLS 与 SCTP 各自有重传。
const BRANCH_CAPACITY: usize = 256;

/// 一个共享 UDP 端口的读写中枢。
pub(crate) struct UdpMux {
    socket: Arc<dyn AsyncUdpSocket>,
    listen_addr: SocketAddr,
    /// ufrag → 支路投递端。
    conns: HashMap<String, mpsc::Sender<Datagram>>,
    /// 源地址 → ufrag。DTLS / SCTP 包里没有 ufrag，只能靠首包建立的这层映射。
    by_addr: HashMap<SocketAddr, String>,
    /// 已上报过的 ufrag。
    ///
    /// ICE 会持续重发 binding request，没有这层去重，同一个客户端会被反复当成
    /// 「新连接」，每次都建一个 `PeerConnection`。
    ///
    /// **按 ufrag 而不是按来源地址去重**：ufrag 是一次连接尝试的标识，地址不是。
    /// 按地址去重会把「同一个 NAT 端口换新 ufrag 再来」（客户端重连、NAT 重绑）
    /// 静默吞掉——那恰恰是必须上报的新连接。
    announced: HashSet<String>,
    /// 复用的接收缓冲。
    ///
    /// 尺寸**必须**按 GRO 上限放大（见 [`AsyncUdpSocket::max_gro_segments`]）：内核会把
    /// 同一对端的多个数据报合并进一条 message，装不下的部分是**被丢掉**而不是留到下次读。
    /// 所以这里不能为了省内存去缩小它——Linux 上 64 段即约 94 KiB，而一个监听端口
    /// 只有一份。无 GRO 的平台（macOS / Windows）退化成一个 [`RECEIVE_MTU`]。
    recv_buf: Vec<u8>,
    /// 已从内核收下、尚未分流完的数据报。
    ///
    /// 一次 `poll_recv` 可能带回多个数据报（见 [`UdpMux::poll`] 里的 GRO 拆分），
    /// 而 `poll` 一次只产出一个事件，故先切分入队、再逐个 dispatch。
    pending: VecDeque<Datagram>,
    /// 支路满导致的累计丢包数（见 [`BRANCH_CAPACITY`] 与 [`UdpMux::deliver`]）。
    ///
    /// **持续丢包会把 SCTP 的拥塞窗口压塌**，表现为「吞吐从第一秒起就恒定在一个远低于
    /// 链路能力的值上、但传输仍能完成」——2026-08-10 观测到的浏览器→桌面恒定 3.3 MB/s
    /// 正是这个形状（等效 cwnd ≈ 3 个 MTU）。
    ///
    /// 这里原先只有一句 `debug!`，而桌面与移动的默认 filter 是
    /// `swarmdrop=debug,swarmdrop_net=debug`（`EnvFilter` 按 target **前缀**匹配），
    /// **`webrtc_p2p` 一条都进不去**——这条路径在生产日志里完全隐形，诊断时只能靠推理。
    /// 现在计数并按 2 的幂次打 `warn!`。
    dropped_full: u64,
}

/// 一个数据报：内容 + 来源。
type Datagram = (Vec<u8>, SocketAddr);

/// [`UdpMux::poll`] 产出的事件。
#[derive(Debug)]
pub(crate) enum UdpMuxEvent {
    /// 来自一个未见过的地址的 ICE binding request——一条新的入站连接。
    NewAddr { addr: SocketAddr, ufrag: String },
    /// 读循环出错，端口已不可用。
    Error(io::Error),
}

impl fmt::Debug for UdpMux {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpMux")
            .field("listen_addr", &self.listen_addr)
            .field("conns", &self.conns.len())
            .finish()
    }
}

impl UdpMux {
    /// 绑定端口。
    ///
    /// socket 经 `webrtc` 自己的 [`Runtime`] 包装，因此**不引入 tokio 直接依赖**，
    /// 也自动跟随宿主选定的运行时（tokio 或 smol）。
    pub(crate) fn bind(addr: SocketAddr, runtime: &Arc<dyn Runtime>) -> io::Result<Self> {
        let std_socket = std::net::UdpSocket::bind(addr)?;
        std_socket.set_nonblocking(true)?;
        let listen_addr = std_socket.local_addr()?;
        let socket = runtime.wrap_udp_socket(std_socket)?;
        let recv_buf = vec![0u8; gro_recv_buf_len(socket.max_gro_segments())];

        Ok(Self {
            socket,
            listen_addr,
            conns: HashMap::new(),
            by_addr: HashMap::new(),
            announced: HashSet::new(),
            recv_buf,
            pending: VecDeque::new(),
            dropped_full: 0,
        })
    }

    /// 实际绑定到的地址。传 0 端口时这里才是系统分配的真实端口。
    pub(crate) fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// 为一个 ufrag 开一条支路，交给对应的 `PeerConnection` 当 socket 用。
    ///
    /// 重复注册同一个 ufrag 会顶掉旧支路——ICE 重启时正是这个行为。
    pub(crate) fn register(&mut self, ufrag: &str) -> Arc<MuxedSocket> {
        let (tx, rx) = mpsc::channel(BRANCH_CAPACITY);
        self.conns.insert(ufrag.to_owned(), tx);
        Arc::new(MuxedSocket {
            shared: self.socket.clone(),
            listen_addr: self.listen_addr,
            inbox: Mutex::new(rx),
        })
    }

    /// 注销一条支路，连同它占用的地址映射。
    pub(crate) fn remove(&mut self, ufrag: &str) {
        if self.conns.remove(ufrag).is_none() {
            return;
        }
        self.by_addr.retain(|_, owner| owner != ufrag);
        self.announced.remove(ufrag);
    }

    /// 回收对端已消失的支路。
    ///
    /// `Sender::is_closed()` 为真意味着接收端（那条连接的 `MuxedSocket`）已被 drop。
    fn reap_closed(&mut self) {
        let dead: Vec<String> = self
            .conns
            .iter()
            .filter(|(_, tx)| tx.is_closed())
            .map(|(ufrag, _)| ufrag.clone())
            .collect();
        if dead.is_empty() {
            return;
        }
        tracing::debug!(count = dead.len(), "webrtc-direct: 回收已关闭的支路");
        for ufrag in dead {
            self.remove(&ufrag);
        }
    }

    /// 驱动读循环。
    ///
    /// 由 `Transport::poll` 调用——与官方实现一致，本 crate 全程无 `spawn`。
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<UdpMuxEvent> {
        for _ in 0..MAX_RECV_BURST {
            while let Some((packet, from)) = self.pending.pop_front() {
                if let Some(event) = self.dispatch(packet, from) {
                    return Poll::Ready(event);
                }
            }

            let mut meta = [RecvMeta::default(); 1];
            let count = {
                let mut bufs = [IoSliceMut::new(&mut self.recv_buf)];
                match futures::ready!(self.socket.poll_recv(cx, &mut bufs, &mut meta)) {
                    Ok(count) => count,
                    Err(e) if is_retryable_recv_error(&e) => {
                        tracing::debug!("忽略瞬时读错误：{e}");
                        continue;
                    }
                    Err(e) => return Poll::Ready(UdpMuxEvent::Error(e)),
                }
            };

            // 契约上 `Ok(0)` 是「什么都没准备好」，**不是**「收到 0 条消息」。当成后者
            // 会转出一轮没有 waker 的空循环——socket 不会再唤醒我们，而我们也不会停。
            // 让出并自唤醒：既不空转，也不挂死。
            if count == 0 {
                break;
            }

            // `count` 与 `len` / `stride` 一样来自 socket 实现，一律不盲信：
            // 越界切片会 panic，而这条路径上跑的是公网端口的收包循环。
            for m in &meta[..count.min(meta.len())] {
                split_datagrams(&self.recv_buf, m, &mut self.pending);
            }
        }

        // 读满一轮 burst（或撞上 `Ok(0)`）就让出，别把 swarm 的 poll 线程占住。
        // `pending` 里可能还有货，所以必须自唤醒——否则这些包要等下一个数据报才被处理。
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    /// 把一个数据报投递到对应支路；无处可去时判断是不是一条新连接。
    fn dispatch(&mut self, packet: Vec<u8>, from: SocketAddr) -> Option<UdpMuxEvent> {
        // 1. 已知来源：直接按地址投递（DTLS / SCTP 走的都是这条）。
        if let Some(ufrag) = self.by_addr.get(&from).cloned() {
            self.deliver(&ufrag, packet, from);
            return None;
        }

        // 2. 未知来源，且不是 STUN：无从判断归属，丢弃。
        //    公网端口上这类包（扫描、残留）是常态，不值得留 warn。
        if !is_stun_message(&packet) {
            tracing::trace!(%from, "丢弃来源未知的非 STUN 包");
            return None;
        }

        let Some(ufrag) = local_ufrag(&packet) else {
            tracing::debug!(%from, "STUN 包缺少可用的 USERNAME 属性，丢弃");
            return None;
        };

        // 3. 已注册的 ufrag 换了来源地址：ICE 在做地址迁移，接纳并重新绑定。
        if self.conns.contains_key(&ufrag) {
            self.by_addr.insert(from, ufrag.clone());
            self.deliver(&ufrag, packet, from);
            return None;
        }

        // 4. 全新的 ufrag：一条新的入站连接。
        //    此刻**先不投递**——`PeerConnection` 还没建好，支路不存在。ICE 会重发
        //    binding request，等支路就绪后自然被收下。
        if !self.announced.insert(ufrag.clone()) {
            return None;
        }

        // 顺手回收已死的支路。
        //
        // `deliver` 那条清理路径要求「再收到一个发往该 ufrag 的包」才会触发，而对端
        // 悄悄消失时（正常关闭后不再发包、进程被杀、NAT 映射过期）永远等不到。
        // 少了这一步，三张表会随**历史**连接数无上限增长——这是个暴露在公网 UDP 端口、
        // 由未认证输入驱动的增长面，扫描器也能刷。挂在「新连接」这种低频事件上，
        // `is_closed()` 只是一次原子读。
        self.reap_closed();

        tracing::debug!(%from, %ufrag, "webrtc-direct: 新的入站连接");
        Some(UdpMuxEvent::NewAddr { addr: from, ufrag })
    }

    /// 投递到支路。支路满或已关闭时丢包（见 [`BRANCH_CAPACITY`]）。
    fn deliver(&mut self, ufrag: &str, packet: Vec<u8>, from: SocketAddr) {
        let Some(tx) = self.conns.get_mut(ufrag) else {
            return;
        };
        match tx.try_send((packet, from)) {
            Ok(()) => {}
            Err(e) if e.is_full() => {
                self.dropped_full += 1;
                // 按 2 的幂次报告（第 1、2、4、8… 次）：真丢包时日志行数只有 log₂ 级，
                // 既不刷屏，又保证**第一次丢包一定被记下来**——定频报告（每 N 次一条）
                // 会把「只丢了几个」这种更值得警惕的情形整个吞掉。
                if self.dropped_full.is_power_of_two() {
                    tracing::warn!(
                        %ufrag,
                        %from,
                        dropped = self.dropped_full,
                        capacity = BRANCH_CAPACITY,
                        "udp mux 支路缓冲已满，丢弃数据报（累计数按 2 的幂次报告）"
                    );
                }
            }
            Err(_) => {
                // 对端已 drop：连接没了，清掉映射免得一直占着 ufrag。
                tracing::debug!(%ufrag, "支路已关闭，注销");
                self.remove(ufrag);
            }
        }
    }
}

/// 接收缓冲该多大——**必须装得下内核一次能合并出来的全部段**。
///
/// 装不下的尾部段是被内核**丢掉**的，不会留到下次读，所以这个值宁大勿小。
/// 但也不能拿 [`RECEIVE_MTU`] 当段长去乘：GRO 的段不会超过一个路径 MTU
/// （见 [`GRO_SEGMENT_LEN`]）。
///
/// 依据是 `AsyncUdpSocket::max_gro_segments` 的 `# Buffer sizing` 段——上游把「按自己的
/// 规则分配接收缓冲的实现（比如跨连接多路复用的共享 socket）要在自己的循环里负责同样的
/// MTU 上界」写成了公开契约，本函数就是本 mux 履行它的地方。
///
/// 与上游 `gro_recv_buf_len` 的唯一差异：无 GRO 时的下限取 [`RECEIVE_MTU`]（8192）而非
/// 它的 2000。方向是**更保守**（宁大勿小，绝不会截断），且与本文件既有的单包上限一致。
fn gro_recv_buf_len(max_gro: usize) -> usize {
    // 无 GRO 的平台（macOS / Windows）按单个数据报的上限来，这也是本函数的下限：
    // 即便有 GRO，一条 message 也至少要装得下一个完整数据报。
    (max_gro.min(MAX_GRO_SEGMENTS) * GRO_SEGMENT_LEN).max(RECEIVE_MTU)
}

/// 这个读错误是不是「某个对端的事」，而不是「端口废了」。
///
/// ⚠️ **这五个变体来自 `AsyncUdpSocket::poll_recv` 的 `# Errors` 段，是上游写死的公开
/// 契约**（driver 对瞬时错误的分类），少一个都不行。漏一种就
/// 意味着：一次本该忽略的瞬时错误会顺着 `UdpMuxEvent::Error` 冒到 `Transport::poll`，
/// 那里会 `listeners.remove()` 并发 `ListenerClosed`——**公网 4003 端口就此消失，
/// 进程活着但再没人能连进来**，也没有重试路径。
///
/// 尤其是 `ConnectionRefused`：ICMP port unreachable 在 **Linux 上是这个**，
/// Windows 上才是 `ConnectionReset`。只认后者等于在 Linux 上留了个远程可触发的开关
/// ——随便哪个对端关掉进程，回来的 ICMP 就能掀掉整个监听端口。
fn is_retryable_recv_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::Interrupted            // 信号打断 recvmsg（EINTR）
            | io::ErrorKind::WouldBlock       // 没数据；poll 式 socket 下不该出现，兜底
            | io::ErrorKind::ConnectionRefused // ICMP port unreachable（Linux）
            | io::ErrorKind::ConnectionReset  // 同上（Windows）
            | io::ErrorKind::TimedOut
    )
}

/// 把一次 `poll_recv` 填好的缓冲切成一个个数据报，追加到 `out`。
///
/// 一条 message 里可能粘着同一对端的**多个**数据报——内核 GRO 把它们合并了，
/// [`RecvMeta::stride`] 是切回来的唯一依据（末段可以更短）。
///
/// ⚠️ **这一步不能省。** 不切分等于把 N 个 UDP 包当成一个巨包投给支路，DTLS 记录层
/// 与 SCTP 都会当场解析失败。
///
/// ⚠️ **这不是 webrtc 0.20.0 才有的约束，是这里一直漏做的一件事。** 旧版的
/// `wrap_udp_socket` 就已经用 `quinn_udp::UdpSocketState::new` 在 socket 上打开了
/// UDP_GRO，只是当时读包走 `recv_from`（底层是裸的 `tokio::UdpSocket::recv_from`）
/// ——**sockopt 开着，内核照样合并，只是承载 stride 的 cmsg 被丢弃了**。所以
/// 0.20.0 之前的 Linux 构建会把合并包整个投给 DTLS/SCTP，且缓冲只有 8 KiB，
/// 超出的尾部段被内核直接丢掉。换成 poll 式 API 只是让这个约束**显式**了。
/// 别把它读成「升级带来的新问题」——按那个因果去查旧分支的丢包会查不到。
fn split_datagrams(buf: &[u8], meta: &RecvMeta, out: &mut VecDeque<Datagram>) {
    // `stride` 至少取 1：零长数据报会让它是 0，那是 `chunks` 的 panic 条件。
    // `len` 同样不能盲信——它来自 socket 实现，越界切片会 panic。
    let filled = &buf[..meta.len.min(buf.len())];
    for datagram in filled.chunks(meta.stride.max(1)) {
        out.push_back((datagram.to_vec(), meta.addr));
    }
}

/// 从 STUN 消息里取出 **local** ufrag。
///
/// `USERNAME` 的格式是 `<对端ufrag>:<本端ufrag>`（RFC 8445 §7.2.2）。对入站的
/// binding request 而言，冒号前那一半才是**本机**这一侧的 ufrag，也就是我们用来
/// 分流的键。取错半边会导致永远匹配不上。
fn local_ufrag(packet: &[u8]) -> Option<String> {
    let mut msg = StunMessage::new();
    msg.unmarshal_binary(packet).ok()?;

    let (attr, found) = msg.attributes.get(ATTR_USERNAME);
    if !found {
        return None;
    }
    let username = String::from_utf8(attr.value).ok()?;
    username.split(':').next().map(str::to_owned)
}

/// 交给单个 `PeerConnection` 的「假 socket」。
///
/// 发包直接走共享 socket；收包从自己的支路取。对 `PeerConnection` 而言它与一个
/// 独占端口的 socket 没有区别。
pub(crate) struct MuxedSocket {
    shared: Arc<dyn AsyncUdpSocket>,
    listen_addr: SocketAddr,
    /// 支路收件箱。
    ///
    /// `AsyncUdpSocket::poll_recv` 收的是 `&self`，而取包要改 receiver 的状态，
    /// 故加锁。驱动同一条连接的只有它自己的 driver 任务，实际零竞争。
    ///
    /// 用**同步**锁：poll 路径上没有 await 点，异步锁在这里既用不上也拿不住。
    inbox: Mutex<mpsc::Receiver<Datagram>>,
}

impl fmt::Debug for MuxedSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MuxedSocket")
            .field("listen_addr", &self.listen_addr)
            .finish()
    }
}

impl AsyncUdpSocket for MuxedSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.listen_addr)
    }

    /// 发包不经支路，直接走共享 socket——出方向本来就不需要分流。
    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        self.shared.poll_send(cx, transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        // 调用方没给地方放，等于「这次收不了」。返回 `Ok(0)` 会被读成「收到 0 条」，
        // 那是另一回事（见 `UdpMux::poll` 里对 `count == 0` 的处理）。
        if bufs.is_empty() || meta.is_empty() {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let (packet, from) = match inbox.poll_next_unpin(cx) {
                Poll::Ready(Some(datagram)) => datagram,
                Poll::Ready(None) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "udp mux 支路已关闭",
                    )));
                }
                Poll::Pending => return Poll::Pending,
            };

            // 装不下就**整个丢掉**，不要截断后投出去。
            //
            // 截断出来的是个「长度合法、内容残缺」的数据报：DTLS 记录层校验 MAC 失败、
            // SCTP 解析 chunk 失败，两者都只是静默丢弃，于是现象是「莫名其妙的丢包」，
            // 全链路没有一行日志指向这里。宁可在这儿说清楚。
            if packet.len() > bufs[0].len() {
                tracing::warn!(
                    datagram = packet.len(),
                    buffer = bufs[0].len(),
                    %from,
                    "数据报超出接收缓冲，丢弃"
                );
                continue;
            }

            let n = packet.len();
            bufs[0][..n].copy_from_slice(&packet);
            // `RecvMeta` 是 `#[non_exhaustive]`，crate 外构造不出结构体字面量，只能逐字段写。
            meta[0] = RecvMeta::default();
            meta[0].addr = from;
            meta[0].len = n;
            // `stride` 必须 ≥ 1：调用方拿它做去分段的除数，0 会当场除零。
            meta[0].stride = n.max(1);
            return Poll::Ready(Ok(1));
        }
    }

    /// GSO 能力取决于**真正发包**的那个 socket。
    fn max_gso_segments(&self) -> usize {
        self.shared.max_gso_segments()
    }

    /// 支路里投的永远是 [`UdpMux::poll`] 切分好的单个数据报，不存在 GRO 合并。
    fn max_gro_segments(&self) -> usize {
        1
    }
}

/// 把 [`MuxedSocket`] 塞进 `PeerConnection` 的 [`Runtime`] 垫片。
///
/// [`Runtime::wrap_udp_socket`] **忽略**传入的 socket，一律返回预置的那条支路。
/// `PeerConnection` 仍会自己 bind 一个临时端口（我们控制不了那一步），但那个
/// socket 拿到手就被丢弃了。
///
/// ⚠️ 副作用：`PeerConnection` 内部会把**临时端口**当作自己的本地地址（four-tuple
/// 的 `local_addr`、SDP 里的 host candidate）。这在 direct 模式下无害——服务端是
/// ICE-lite，它的 SDP 由客户端本地构造、从不外发；发包时 driver 用那个 key 查到的
/// 正是这条支路，最终从共享端口发出，对端看到的源端口是对的。
/// 但**这条不变量一旦被破坏（比如将来给 dialer 也套上 mux 并依赖它的 candidate），
/// 症状会是「连接建立后对端回包到错误端口」**，很难查。
pub(crate) struct MuxedRuntime {
    inner: Arc<dyn Runtime>,
    socket: Arc<MuxedSocket>,
    /// 支路被交出去几次——见 [`MuxedRuntime::wrap_udp_socket`]，必须恰好一次。
    handed_out: AtomicUsize,
}

impl fmt::Debug for MuxedRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MuxedRuntime").finish()
    }
}

impl MuxedRuntime {
    /// 包一层垫片。返回 `Arc<dyn Runtime>` 而非 `Self`——调用方只会拿它喂
    /// `PeerConnectionBuilder::with_runtime`，暴露具体类型没有意义。
    pub(crate) fn wrap(inner: Arc<dyn Runtime>, socket: Arc<MuxedSocket>) -> Arc<dyn Runtime> {
        Arc::new(Self {
            inner,
            socket,
            handed_out: AtomicUsize::new(0),
        })
    }
}

impl Runtime for MuxedRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        // 丢弃它。端口在 `PeerConnection` 构造时已经绑上，这里只是不用而已。
        drop(socket);

        // 同一条支路只能交出去一次。第二次意味着 `PeerConnection` 绑了多个 UDP 地址，
        // 于是会有两个 recv future 抢同一个 mpsc `Receiver`——它的 `AtomicWaker` 只留
        // 最后注册的那个，另一个永远醒不来，表现为静默的握手超时。
        // 调用点的前提（`upgrade.rs` 里恰好一个 bind 地址）在这里兜底。
        let handed_out = self.handed_out.fetch_add(1, Ordering::Relaxed);
        debug_assert_eq!(
            handed_out, 0,
            "MuxedSocket 被交给了多个 PeerConnection socket——调用点必须只绑一个 UDP 地址"
        );
        if handed_out > 0 {
            tracing::error!("同一条 mux 支路被重复交出，收包会静默饥饿");
        }

        Ok(self.socket.clone())
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        self.inner.wrap_tcp_listener(listener)
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        self.inner.connect_tcp(remote_addr)
    }

    // 以下全是原样转交：这层垫片只替换 UDP socket，其余能力没有理由改写。

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        self.inner.resolve_host(host)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        self.inner.block_on(future);
    }

    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.yield_now()
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一条最小的 STUN binding request，USERNAME = `<local>:<remote>`。
    fn stun_with_username(username: &str) -> Vec<u8> {
        let value = username.as_bytes();
        // 属性头 4 字节 + 值，按 4 字节对齐补零
        let padded = value.len().div_ceil(4) * 4;
        let attr_len = 4 + padded;

        let mut packet = Vec::with_capacity(20 + attr_len);
        packet.extend_from_slice(&0x0001u16.to_be_bytes()); // Binding Request
        packet.extend_from_slice(&(attr_len as u16).to_be_bytes());
        packet.extend_from_slice(&0x2112A442u32.to_be_bytes()); // magic cookie
        packet.extend_from_slice(&[0x42; 12]); // transaction id

        packet.extend_from_slice(&ATTR_USERNAME.0.to_be_bytes());
        packet.extend_from_slice(&(value.len() as u16).to_be_bytes());
        packet.extend_from_slice(value);
        packet.resize(20 + attr_len, 0);

        packet
    }

    /// `USERNAME` 是 `<对端>:<本端>`，分流要的是**冒号前**那一半。
    /// 取错半边的症状是「客户端一直重发 binding request，服务端一直当成新连接」。
    #[test]
    fn extracts_local_half_of_username() {
        let packet = stun_with_username("local-ufrag:remote-ufrag");
        assert_eq!(local_ufrag(&packet).as_deref(), Some("local-ufrag"));
    }

    /// libp2p 的 ufrag 带 `libp2p+webrtc+v1/` 前缀，里面没有冒号，但值得钉一下真实形态。
    #[test]
    fn extracts_libp2p_style_ufrag() {
        let ufrag = "libp2p+webrtc+v1/abcdef0123456789";
        let packet = stun_with_username(&format!("{ufrag}:{ufrag}"));
        assert_eq!(local_ufrag(&packet).as_deref(), Some(ufrag));
    }

    /// 畸形输入不能 panic——这是暴露在公网端口上的解析路径。
    #[test]
    fn malformed_input_is_rejected_not_panicking() {
        for bad in [
            vec![],
            vec![0u8; 4],
            vec![0xFF; 20],
            vec![0u8; 21],
            b"definitely not stun at all".to_vec(),
        ] {
            assert!(local_ufrag(&bad).is_none(), "{bad:?}");
        }
        // 合法 STUN 但没有 USERNAME
        let mut no_username = stun_with_username("x:y");
        no_username.truncate(20);
        no_username[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert!(local_ufrag(&no_username).is_none());
    }

    /// `is_stun_message` 只看 magic cookie，是分流前的快速筛子。
    #[test]
    fn recognises_stun_by_magic_cookie() {
        assert!(is_stun_message(&stun_with_username("a:b")));
        assert!(!is_stun_message(&[0u8; 20]));
        assert!(!is_stun_message(&[]));
    }

    /// `RecvMeta` 是 `#[non_exhaustive]`，crate 外只能 default 完再逐字段赋值。
    fn meta(len: usize, stride: usize) -> RecvMeta {
        let mut m = RecvMeta::default();
        m.addr = "203.0.113.7:4003".parse().unwrap();
        m.len = len;
        m.stride = stride;
        m
    }

    fn split(buf: &[u8], meta: &RecvMeta) -> Vec<Vec<u8>> {
        let mut out = VecDeque::new();
        split_datagrams(buf, meta, &mut out);
        out.into_iter().map(|(packet, _)| packet).collect()
    }

    /// 内核 GRO 把同一对端的连续包合并进一条 message，`stride` 是切回来的唯一依据。
    ///
    /// **本机 loopback 通常不触发 GRO**，所以 `direct_loopback` 那几条真链路测试
    /// 走不到这条分支——它只有这里能覆盖。切错的症状是 DTLS/SCTP 收到粘包直接
    /// 解析失败，而且全链路零报错。
    #[test]
    fn splits_gro_coalesced_message_by_stride() {
        let buf = [b"aaaa".as_slice(), b"bbbb", b"cc"].concat();
        assert_eq!(
            split(&buf, &meta(10, 4)),
            vec![b"aaaa".to_vec(), b"bbbb".to_vec(), b"cc".to_vec()],
            "末段允许短于 stride"
        );
    }

    /// 常态（没发生合并）：`stride == len`，原样切出一个包。
    #[test]
    fn single_datagram_passes_through_unsplit() {
        let buf = b"one-datagram".to_vec();
        assert_eq!(split(&buf, &meta(buf.len(), buf.len())), vec![buf]);
    }

    /// 缓冲比 `len` 大是常态（缓冲按 GRO 上限预分配），尾部的零不能被当成数据。
    #[test]
    fn trailing_buffer_space_is_not_emitted() {
        let mut buf = b"hi".to_vec();
        buf.resize(RECEIVE_MTU, 0);
        assert_eq!(split(&buf, &meta(2, 2)), vec![b"hi".to_vec()]);
    }

    /// socket 实现给出的 `len` / `stride` 是外部输入，畸形值不能 panic。
    ///
    /// `stride == 0`（零长数据报，quinn-udp 会把 `len` 原样镜像进 `stride`）是
    /// `chunks` 的 panic 条件；`len` 超过缓冲则是越界切片。
    #[test]
    fn malformed_meta_does_not_panic() {
        assert!(split(b"abc", &meta(0, 0)).is_empty());
        assert_eq!(split(b"abc", &meta(3, 0)), vec![b"a", b"b", b"c"]);
        assert_eq!(split(b"abc", &meta(usize::MAX, 8)), vec![b"abc".to_vec()]);
    }

    /// 缓冲尺寸要装得下内核一次能合并出来的全部段，且不拿单包上限当段长去乘。
    #[test]
    fn gro_buf_sized_by_segment_not_by_mtu() {
        // 无 GRO：退化成单个数据报的上限。
        assert_eq!(gro_recv_buf_len(1), RECEIVE_MTU);
        // 有 GRO：按段长乘段数，而不是 RECEIVE_MTU * 段数（那会大 5 倍多）。
        assert_eq!(gro_recv_buf_len(64), 64 * GRO_SEGMENT_LEN);
        // `max_gro_segments()` 来自 socket 实现，不能直接当分配乘数。
        assert_eq!(
            gro_recv_buf_len(usize::MAX),
            MAX_GRO_SEGMENTS * GRO_SEGMENT_LEN
        );
    }

    /// 判据漏一种，公网监听端口就会被一次瞬时错误永久掀掉。
    ///
    /// `ConnectionRefused` 尤其关键：ICMP port unreachable 在 **Linux 上是它**，
    /// 只认 Windows 的 `ConnectionReset` 等于留了个远程可触发的开关。
    #[test]
    fn transient_recv_errors_do_not_kill_the_listener() {
        for kind in [
            io::ErrorKind::Interrupted,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::TimedOut,
        ] {
            assert!(
                is_retryable_recv_error(&io::Error::from(kind)),
                "{kind:?} 该被忽略，而不是关掉端口"
            );
        }
        // 真正说明端口废了的错误必须上报。
        for kind in [io::ErrorKind::AddrNotAvailable, io::ErrorKind::NotConnected] {
            assert!(!is_retryable_recv_error(&io::Error::from(kind)), "{kind:?}");
        }
    }

    // ── MuxedSocket::poll_recv ────────────────────────────────────────────
    //
    // 每个 DTLS / SCTP 字节都从这里过。loopback 测试只跑 ≤1200 字节的正常包，
    // 走不到截断、零长、支路关闭这几条分支，只有这里能覆盖。

    /// 只实现三个必需方法的桩，用来当 `MuxedSocket` 的共享 socket。
    #[derive(Debug)]
    struct DummyShared;

    impl AsyncUdpSocket for DummyShared {
        fn local_addr(&self) -> io::Result<SocketAddr> {
            Ok("0.0.0.0:4003".parse().unwrap())
        }
        fn poll_send(&self, _: &mut Context<'_>, t: &Transmit<'_>) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(t.contents.len()))
        }
        fn poll_recv(
            &self,
            _: &mut Context<'_>,
            _: &mut [IoSliceMut<'_>],
            _: &mut [RecvMeta],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }
    }

    /// 造一条支路，返回投递端与 socket 本身。
    fn branch() -> (mpsc::Sender<Datagram>, MuxedSocket) {
        let (tx, rx) = mpsc::channel(BRANCH_CAPACITY);
        (
            tx,
            MuxedSocket {
                shared: Arc::new(DummyShared),
                listen_addr: "0.0.0.0:4003".parse().unwrap(),
                inbox: Mutex::new(rx),
            },
        )
    }

    /// poll 一次 `poll_recv`，返回 `(结果, 收到的 meta)`。
    fn poll_once_recv(
        socket: &MuxedSocket,
        buf_len: usize,
    ) -> (Poll<io::Result<usize>>, RecvMeta, Vec<u8>) {
        let mut buf = vec![0u8; buf_len];
        let mut meta = [RecvMeta::default(); 1];
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let out = {
            let mut bufs = [IoSliceMut::new(&mut buf)];
            socket.poll_recv(&mut cx, &mut bufs, &mut meta)
        };
        (out, meta[0], buf)
    }

    /// 常态：投什么收什么，`stride == len`（支路里不存在 GRO 合并）。
    #[test]
    fn branch_delivers_datagram_with_stride_equal_len() {
        let (mut tx, socket) = branch();
        let from: SocketAddr = "198.51.100.9:1234".parse().unwrap();
        tx.try_send((b"hello".to_vec(), from)).unwrap();

        let (out, meta, buf) = poll_once_recv(&socket, 2000);
        assert!(matches!(out, Poll::Ready(Ok(1))));
        assert_eq!(meta.len, 5);
        assert_eq!(meta.stride, 5, "调用方拿 stride 去分段，必须等于 len");
        assert_eq!(meta.addr, from);
        assert_eq!(&buf[..5], b"hello");
    }

    /// 零长数据报的 `stride` 必须仍 ≥ 1——调用方拿它当除数。
    #[test]
    fn zero_length_datagram_still_reports_stride_at_least_one() {
        let (mut tx, socket) = branch();
        tx.try_send((Vec::new(), "198.51.100.9:1".parse().unwrap()))
            .unwrap();

        let (out, meta, _) = poll_once_recv(&socket, 2000);
        assert!(matches!(out, Poll::Ready(Ok(1))));
        assert_eq!(meta.len, 0);
        assert_eq!(meta.stride, 1, "stride 为 0 会让调用方除零");
    }

    /// 装不下的数据报要**整个丢掉**，不能截断后当成合法包投出去。
    ///
    /// 截断出来的残包会被 DTLS/SCTP 静默拒绝，现象是「莫名丢包」且无日志。
    #[test]
    fn oversized_datagram_is_dropped_not_truncated() {
        let (mut tx, socket) = branch();
        let from: SocketAddr = "198.51.100.9:1".parse().unwrap();
        tx.try_send((vec![0xAB; 3000], from)).unwrap();
        tx.try_send((b"small".to_vec(), from)).unwrap();

        // 超大的那个被跳过，直接拿到后面那个完整的包。
        let (out, meta, buf) = poll_once_recv(&socket, 2000);
        assert!(matches!(out, Poll::Ready(Ok(1))));
        assert_eq!(meta.len, 5, "不该出现 len == 2000 的截断残包");
        assert_eq!(&buf[..5], b"small");
    }

    /// 支路对端已 drop → `BrokenPipe`，让 driver 知道这条连接没了。
    #[test]
    fn closed_branch_reports_broken_pipe() {
        let (tx, socket) = branch();
        drop(tx);

        let (out, ..) = poll_once_recv(&socket, 2000);
        match out {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), io::ErrorKind::BrokenPipe),
            other => panic!("期望 BrokenPipe，实际 {other:?}"),
        }
    }

    /// 支路空着时是 `Pending`，**不是** `Ok(0)`——后者会被读成「收到一个空包」。
    #[test]
    fn empty_branch_is_pending_not_zero_count() {
        let (_tx, socket) = branch();
        let (out, ..) = poll_once_recv(&socket, 2000);
        assert!(matches!(out, Poll::Pending));
    }

    /// 调用方没给缓冲同样是「收不了」，而不是「收到 0 条」。
    #[test]
    fn empty_buffers_yield_pending_not_zero_count() {
        let (mut tx, socket) = branch();
        tx.try_send((b"x".to_vec(), "198.51.100.9:1".parse().unwrap()))
            .unwrap();

        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let out = socket.poll_recv(&mut cx, &mut [], &mut []);
        assert!(matches!(out, Poll::Pending));
    }

    /// 支路只投单个数据报，所以如实申报「不会 GRO 合并」；GSO 则取决于真正发包的
    /// 那个 socket，必须跟着它走。
    #[test]
    fn branch_reports_own_gro_and_delegates_gso() {
        let (_tx, socket) = branch();
        assert_eq!(socket.max_gro_segments(), 1);
        assert_eq!(socket.max_gso_segments(), DummyShared.max_gso_segments());
    }
}
