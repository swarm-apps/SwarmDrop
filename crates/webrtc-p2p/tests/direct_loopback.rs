//! 两个真实节点跑通 `/webrtc-direct` 全链路（同机回环）。
//!
//! 这是 direct 模式成立的最强本地证据，不 mock 任何东西：真的绑 UDP 端口、真的走
//! ICE-lite + DTLS + SCTP、真的跑 Noise 握手、真的开子流传字节。
//!
//! # 它顺带钉死的一件事
//!
//! webrtc 0.20 删掉了 0.17 的 `UDPMux` 体系（`set_udp_network` 在 rtc 里已是注释掉的
//! TODO），我们改用 `Runtime::wrap_udp_socket` 注入假 socket 来复用端口。副作用是
//! `PeerConnection` 内部会把自己 bind 的**临时端口**当成本地地址（four-tuple 的
//! `local_addr`、SDP 里的 host candidate），而实际收发走的是共享端口。
//!
//! 「这样 rtc 内部仍然自洽」是个**假设**，静态类型检查不出来。本测试就是它的证据：
//! 跑通说明假设成立；哪天 rtc 改了 socket 的用法，这里会先红。

#![cfg(not(target_family = "wasm"))]

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use futures::future::poll_fn;
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p_core::Transport as _;
use libp2p_core::muxing::StreamMuxer;
use libp2p_core::transport::{DialOpts, ListenerId, PortUse, TransportEvent};
use libp2p_core::{Endpoint, Multiaddr};
use libp2p_identity::{Keypair, PeerId};
use webrtc_p2p::{Config, Connection, DirectConfig, Error, Transport};

/// 打开日志。
///
/// 默认只放行本 crate 的 debug——webrtc-rs 各层的日志量大到会把真正的线索冲掉
/// （Web 端排障时实测吃过这个亏）。要看底层时用
/// `RUST_LOG=webrtc_p2p=trace,rtc=debug,webrtc=debug cargo test …`。
fn init_logs() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webrtc_p2p=debug".into()),
        )
        .with_test_writer()
        .try_init();
}

/// 建一个启用 direct 的 transport，连同它的 behaviour（本测试用不上，但不能 drop
/// ——drop 掉打洞路径的对端会让 `Transport` 报 `BehaviourDetached`）。
fn node(seed: u8) -> (Transport, webrtc_p2p::Behaviour, PeerId) {
    init_logs();
    let keypair = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let peer = keypair.public().to_peer_id();

    let config = Config::default().with_direct(DirectConfig::new(keypair));
    let (transport, behaviour) = webrtc_p2p::new(config, mock_factory());
    (transport, behaviour, peer)
}

/// direct 模式不经 [`webrtc_p2p::Backend`]，工厂只是构造 `new()` 的必填参数。
fn mock_factory() -> webrtc_p2p::Factory {
    std::sync::Arc::new(|_: &Config| {
        Err(webrtc_p2p::BackendError::new(
            "本测试只走 direct 路径，不该建打洞后端",
        ))
    })
}

const DIAL_OPTS: DialOpts = DialOpts {
    role: Endpoint::Dialer,
    port_use: PortUse::Reuse,
};

type Upgraded = Result<(PeerId, Connection), Error>;

/// 驱动 transport 直到拿到第一个 `NewAddress`。
async fn listen_and_get_addr(t: &mut Transport, id: ListenerId) -> Multiaddr {
    t.listen_on(id, "/ip4/127.0.0.1/udp/0/webrtc-direct".parse().unwrap())
        .expect("应能监听");

    match poll_fn(|cx| Pin::new(&mut *t).poll(cx)).await {
        TransportEvent::NewAddress { listen_addr, .. } => listen_addr,
        e => panic!("应产出 NewAddress，实得 {e:?}"),
    }
}

/// 一边喂 transport 的读循环，一边推进 `fut`。
///
/// **不能只 await `fut`**：direct 的 UDP 读循环挂在 `Transport::poll` 上（与官方
/// `libp2p-webrtc` 同构），停止 poll 就等于停掉这个端口上所有连接的收包——**包括
/// 建连之后的数据面**。真实场景里 swarm 的事件循环一直在 poll transport，所以不成
/// 问题；测试必须自己模拟这件事，否则会看到「握手永远不完成」（第一版正是这么挂的）。
async fn drive<T>(transport: &mut Transport, fut: impl Future<Output = T>) -> T {
    let mut fut = std::pin::pin!(fut);
    poll_fn(|cx| {
        // 先喂读循环再看 future 的进展——反过来会白等一轮。
        while let Poll::Ready(event) = Pin::new(&mut *transport).poll(cx) {
            if let TransportEvent::ListenerClosed { reason, .. } = event {
                panic!("监听器意外关闭：{reason:?}");
            }
        }
        fut.as_mut().poll(cx)
    })
    .await
}

/// 驱动服务端，收下 `count` 条入站连接并完成升级。
async fn accept(server: &mut Transport, count: usize) -> Vec<Upgraded> {
    let mut upgrades: Vec<Pin<Box<dyn Future<Output = Upgraded> + Send>>> = Vec::new();
    let mut done = Vec::new();

    poll_fn(|cx| {
        while let Poll::Ready(event) = Pin::new(&mut *server).poll(cx) {
            match event {
                TransportEvent::Incoming { upgrade, .. } => upgrades.push(Box::pin(upgrade)),
                TransportEvent::ListenerClosed { reason, .. } => {
                    panic!("监听器意外关闭：{reason:?}")
                }
                _ => {}
            }
        }

        let mut i = 0;
        while i < upgrades.len() {
            match upgrades[i].as_mut().poll(cx) {
                Poll::Ready(result) => {
                    drop(upgrades.remove(i));
                    done.push(result);
                }
                Poll::Pending => i += 1,
            }
        }

        if done.len() >= count {
            Poll::Ready(std::mem::take(&mut done))
        } else {
            Poll::Pending
        }
    })
    .await
}

#[tokio::test]
async fn direct_dial_completes_noise_and_carries_data() {
    let (mut server, _server_behaviour, server_peer) = node(1);
    let (mut client, _client_behaviour, client_peer) = node(2);

    let listen_addr = listen_and_get_addr(&mut server, ListenerId::next()).await;
    // 通告地址必须自带 certhash——客户端要用它构造服务端的 answer 并校验 DTLS 指纹。
    assert!(
        listen_addr.to_string().contains("/certhash/"),
        "监听地址应带 certhash：{listen_addr}"
    );

    // 补上 /p2p 段：真实场景里对端是从 identify 或邀请串拿到完整地址的。
    let dial_addr: Multiaddr = format!("{listen_addr}/p2p/{server_peer}").parse().unwrap();
    let dialing = client.dial(dial_addr, DIAL_OPTS).expect("应能拨号");

    let (dial_result, mut accepted) = tokio::time::timeout(
        Duration::from_secs(30),
        futures::future::join(dialing, accept(&mut server, 1)),
    )
    .await
    .expect("30s 内应完成 direct 建连");

    let (dialed_peer, mut client_conn) = dial_result.expect("拨号失败");
    let (accepted_peer, mut server_conn) = accepted.remove(0).expect("接受入站连接失败");

    // Noise 握手的产物：两端各自认出了对方的真实身份。
    // 这不是 DTLS 能给的——certhash 只证明「证书没被换」，证明不了「对面是谁」。
    assert_eq!(dialed_peer, server_peer, "客户端应认出服务端身份");
    assert_eq!(accepted_peer, client_peer, "服务端应认出客户端身份");

    // 数据面：开一条子流传字节。服务端的读循环仍要持续被喂。
    const MSG: &[u8] = b"hello over webrtc-direct";
    let received = tokio::time::timeout(
        Duration::from_secs(20),
        drive(&mut server, async {
            let (outbound, inbound) = futures::future::join(
                poll_fn(|cx| Pin::new(&mut client_conn).poll_outbound(cx)),
                poll_fn(|cx| Pin::new(&mut server_conn).poll_inbound(cx)),
            )
            .await;

            let mut sender = outbound.expect("开出站流失败");
            let mut receiver = inbound.expect("收入站流失败");

            sender.write_all(MSG).await.expect("写入失败");
            sender.flush().await.expect("flush 失败");

            let mut buf = vec![0u8; MSG.len()];
            receiver.read_exact(&mut buf).await.expect("读取失败");
            buf
        }),
    )
    .await
    .expect("20s 内应完成一次子流收发");

    assert_eq!(received, MSG, "收到的字节应与发出的一致");
}

/// 一个监听端口要能同时服务多条连接——这正是 UDP mux 存在的理由。
///
/// 少了按 ufrag 分流，第二条连接的包会被投给第一条的 `PeerConnection`，
/// 症状是「第一个客户端正常，之后的全部握手失败」。
#[tokio::test]
async fn one_port_serves_multiple_clients() {
    let (mut server, _sb, server_peer) = node(1);
    let (mut client_a, _ca, peer_a) = node(2);
    let (mut client_b, _cb, peer_b) = node(3);

    let listen_addr = listen_and_get_addr(&mut server, ListenerId::next()).await;
    let dial_addr: Multiaddr = format!("{listen_addr}/p2p/{server_peer}").parse().unwrap();

    let dial_a = client_a.dial(dial_addr.clone(), DIAL_OPTS).unwrap();
    let dial_b = client_b.dial(dial_addr, DIAL_OPTS).unwrap();

    let (a, b, accepted) = tokio::time::timeout(
        Duration::from_secs(45),
        futures::future::join3(dial_a, dial_b, accept(&mut server, 2)),
    )
    .await
    .expect("45s 内两条连接都应建成");

    assert_eq!(a.expect("客户端 A 拨号失败").0, server_peer);
    assert_eq!(b.expect("客户端 B 拨号失败").0, server_peer);

    let peers: Vec<PeerId> = accepted
        .into_iter()
        .map(|r| r.expect("接受入站连接失败").0)
        .collect();
    assert!(
        peers.contains(&peer_a) && peers.contains(&peer_b),
        "同一端口上两条连接应各自认出正确的对端：实得 {peers:?}"
    );
}
