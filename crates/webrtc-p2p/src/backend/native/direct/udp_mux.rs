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

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use rtc::stun::attributes::ATTR_USERNAME;
use rtc::stun::message::{Message as StunMessage, is_stun_message};
use webrtc::runtime::{AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, Runtime};

/// 单个数据报的接收上限。WebRTC 的包远小于此（受 MTU 约束），取值对齐官方实现。
const RECEIVE_MTU: usize = 8192;

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
    /// 正在进行的一次 `recv_from`。
    recv: Option<BoxFuture<'static, io::Result<Datagram>>>,
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

        Ok(Self {
            socket,
            listen_addr,
            conns: HashMap::new(),
            by_addr: HashMap::new(),
            announced: HashSet::new(),
            recv: None,
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
            inbox: futures::lock::Mutex::new(rx),
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
        loop {
            let fut = self.recv.get_or_insert_with(|| {
                let socket = self.socket.clone();
                async move {
                    let mut buf = vec![0u8; RECEIVE_MTU];
                    let (n, from) = socket.recv_from(&mut buf).await?;
                    buf.truncate(n);
                    Ok((buf, from))
                }
                .boxed()
            });

            let (packet, from) = match futures::ready!(fut.poll_unpin(cx)) {
                Ok(datagram) => {
                    self.recv = None;
                    datagram
                }
                Err(e) => {
                    self.recv = None;
                    // 单个对端不可达不该拆掉整个监听端口——ICMP port unreachable 在
                    // Windows 上会以 ConnectionReset 冒到这里，它只说明那个对端没了。
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::TimedOut
                    ) {
                        tracing::debug!("忽略瞬时读错误：{e}");
                        continue;
                    }
                    return Poll::Ready(UdpMuxEvent::Error(e));
                }
            };

            if let Some(event) = self.dispatch(packet, from) {
                return Poll::Ready(event);
            }
        }
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
                tracing::debug!(%ufrag, "支路缓冲已满，丢弃数据报");
            }
            Err(_) => {
                // 对端已 drop：连接没了，清掉映射免得一直占着 ufrag。
                tracing::debug!(%ufrag, "支路已关闭，注销");
                self.remove(ufrag);
            }
        }
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
    /// `AsyncUdpSocket::recv_from` 收的是 `&self`，而取包要改 receiver 的状态，
    /// 故加锁。驱动同一条连接的只有它自己的 driver 任务，实际零竞争。
    inbox: futures::lock::Mutex<mpsc::Receiver<Datagram>>,
}

impl fmt::Debug for MuxedSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MuxedSocket")
            .field("listen_addr", &self.listen_addr)
            .finish()
    }
}

impl AsyncUdpSocket for MuxedSocket {
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        self.shared.send_to(buf, target)
    }

    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move {
            let mut inbox = self.inbox.lock().await;
            let (packet, from) = inbox
                .next()
                .await
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "udp mux 支路已关闭"))?;
            let n = packet.len().min(buf.len());
            buf[..n].copy_from_slice(&packet[..n]);
            Ok((n, from))
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.listen_addr)
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
        Arc::new(Self { inner, socket })
    }
}

impl Runtime for MuxedRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle {
        self.inner.spawn(future)
    }

    fn spawn_reactor(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle {
        self.inner.spawn_reactor(future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        // 丢弃它。端口在 `PeerConnection` 构造时已经绑上，这里只是不用而已。
        drop(socket);
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
}
