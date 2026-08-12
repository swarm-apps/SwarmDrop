//! native↔native 端到端：建连 → Noise 认证 → 子流收发。
//!
//! **这是本 crate 在 CI 里唯一的端到端覆盖**，也是 native 侧实现拨号的全部理由
//! （生产上桌面↔桌面走 QUIC 更快，桌面↔浏览器是浏览器拨过来）。删掉拨号之前，
//! 先想清楚拿什么替代这个文件。
//!
//! 浏览器侧的对应验证只能手测，而手测无法满足「护栏测试必须红过一次」。

use std::pin::Pin;
use std::sync::Arc;

use futures::future::poll_fn;
use futures::{AsyncReadExt, AsyncWriteExt};
use libp2p_core::Transport as _;
use libp2p_core::multiaddr::{Multiaddr, Protocol};
use libp2p_core::muxing::StreamMuxer;
use libp2p_core::transport::{DialOpts, ListenerId, PortUse, TransportError, TransportEvent};
use libp2p_core::{Endpoint as ConnectedPoint, multihash::Multihash};
use libp2p_identity::{Keypair, PeerId};
use webtransport_p2p::{Config, MemoryCertificateStore, Transport};

fn transport(keys: &Keypair) -> Transport {
    Transport::new(Config::new(keys.clone())).expect("装配 transport")
}

/// 等一个 transport 事件。
///
/// ⚠️ **超时不能省**：`poll` 无事件时返回 `Pending`，「本该发事件却没发」的表现因此是
/// 永久挂起而非断言失败，在 CI 里只会看到一句 job 超时。
async fn next_event(
    transport: &mut Transport,
) -> TransportEvent<<Transport as libp2p_core::Transport>::ListenerUpgrade, webtransport_p2p::Error>
{
    tokio::time::timeout(DEADLINE, poll_fn(|cx| Pin::new(&mut *transport).poll(cx)))
        .await
        .expect("等待 transport 事件超时：预期的事件根本没有被发出")
}

/// 任何一步 IO 的等待上限。
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// 给任意一步 IO 套上超时，把「挂住」变成「失败」。
async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(DEADLINE, fut)
        .await
        .unwrap_or_else(|_| panic!("{what} 超时"))
}

/// 起一个只绑回环的监听器，返回它通告的第一条地址。
async fn listen(transport: &mut Transport) -> (ListenerId, Multiaddr) {
    let id = ListenerId::next();
    transport
        .listen_on(
            id,
            "/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap(),
        )
        .expect("监听回环");

    match next_event(transport).await {
        TransportEvent::NewAddress { listen_addr, .. } => (id, listen_addr),
        other => panic!("首个事件应是 NewAddress，实得 {other:?}"),
    }
}

fn dial_opts() -> DialOpts {
    DialOpts {
        role: ConnectedPoint::Dialer,
        port_use: PortUse::New,
    }
}

/// 在后台驱动 listener 的 poll，直到拿到一条已认证的入站连接。
fn accept_one(mut server: Transport) -> tokio::task::JoinHandle<(PeerId, webtransport_p2p::Muxer)> {
    tokio::spawn(async move {
        loop {
            if let TransportEvent::Incoming { upgrade, .. } = next_event(&mut server).await {
                return within("入站升级", upgrade).await.expect("入站升级应成功");
            }
        }
    })
}

/// 基本路径：双方都认出了对方正确的 PeerId。
#[tokio::test]
async fn dials_and_authenticates() {
    let (server_keys, client_keys) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
    let (server_id, client_id) = (
        server_keys.public().to_peer_id(),
        client_keys.public().to_peer_id(),
    );

    let mut server = transport(&server_keys);
    let (_, addr) = listen(&mut server).await;
    let accepted = accept_one(server);

    let mut client = transport(&client_keys);
    let (dialed_peer, _muxer) = within(
        "拨号",
        client.dial(addr, dial_opts()).expect("地址应被认领"),
    )
    .await
    .expect("拨号应成功");

    assert_eq!(dialed_peer, server_id, "客户端应认出服务端身份");
    let accepted = within("服务端接受连接", accepted).await.unwrap();
    assert_eq!(accepted.0, client_id, "服务端应认出客户端身份");
}

/// 数据面：一条 WebTransport bidi 流直接就是一条 libp2p 子流，首字节即上层数据。
#[tokio::test]
async fn substream_carries_bytes_verbatim() {
    let (server_keys, client_keys) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());

    let mut server = transport(&server_keys);
    let (_, addr) = listen(&mut server).await;

    let echo = tokio::spawn(async move {
        let (_, muxer) = accept_one(server).await.unwrap();
        futures::pin_mut!(muxer);

        let mut stream = within(
            "等待入站子流",
            poll_fn(|cx| muxer.as_mut().poll_inbound(cx)),
        )
        .await
        .expect("应收到入站子流");

        let mut buf = [0u8; 5];
        within("读回显请求", stream.read_exact(&mut buf))
            .await
            .unwrap();
        stream.write_all(&buf).await.unwrap();
        stream.flush().await.unwrap();
        // muxer 必须活到写完 —— 它一 drop 会话就关（见 muxer 的 Drop）。
        std::future::pending::<()>().await;
    });

    let mut client = transport(&client_keys);
    let (_, muxer) = within(
        "拨号",
        client.dial(addr, dial_opts()).expect("地址应被认领"),
    )
    .await
    .expect("拨号应成功");
    futures::pin_mut!(muxer);

    let mut stream = within("开出站子流", poll_fn(|cx| muxer.as_mut().poll_outbound(cx)))
        .await
        .expect("应能开出站子流");
    stream.write_all(b"hello").await.unwrap();
    stream.flush().await.unwrap();

    let mut buf = [0u8; 5];
    within("读回显响应", stream.read_exact(&mut buf))
        .await
        .unwrap();
    assert_eq!(&buf, b"hello", "子流不得对字节做任何加工");

    echo.abort();
}

/// ★ **负向**：certhash 不匹配必须拨不通。
///
/// 只测成功路径等于没测证书校验 —— 把 `with_server_certificate_hashes` 换成
/// `with_no_cert_validation`，上面两条测试仍然全绿。
#[tokio::test]
async fn rejects_wrong_certhash() {
    let (server_keys, client_keys) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());

    let mut server = transport(&server_keys);
    let (_, addr) = listen(&mut server).await;
    // 让服务端在后台继续 accept，确保失败来自证书校验而不是「没人接」。
    let pump = accept_one(server);

    // 把地址里的 certhash 全换成一个不存在的。
    let tampered = replace_certhashes(&addr, &[[0xAB; 32]]);

    let mut client = transport(&client_keys);
    let result = within(
        "拨号",
        client.dial(tampered, dial_opts()).expect("地址仍被认领"),
    )
    .await;

    assert!(result.is_err(), "certhash 不匹配必须失败，实得 {result:?}");
    pump.abort();
}

/// 地址声明的 PeerId 与握手结果不符时必须失败 —— 否则拨错节点也会被当成成功，
/// 而上层会把这条连接当作「已连上目标」。
#[tokio::test]
async fn rejects_peer_id_mismatch() {
    let (server_keys, client_keys) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());

    let mut server = transport(&server_keys);
    let (_, addr) = listen(&mut server).await;
    let pump = accept_one(server);

    let wrong = Keypair::generate_ed25519().public().to_peer_id();
    let addr = addr.with(Protocol::P2p(wrong));

    let mut client = transport(&client_keys);
    let result = within(
        "拨号",
        client.dial(addr, dial_opts()).expect("地址仍被认领"),
    )
    .await;

    assert!(
        matches!(result, Err(webtransport_p2p::Error::PeerIdMismatch(_))),
        "实得 {result:?}"
    );
    pump.abort();
}

/// 不认领的地址必须报 `MultiaddrNotSupported` 而不是别的错误 ——
/// 地址分派链靠这个判别码把地址让给下一个 transport。
#[tokio::test]
async fn does_not_claim_foreign_addresses() {
    let mut transport = transport(&Keypair::generate_ed25519());

    for raw in [
        "/ip4/127.0.0.1/udp/4001/quic-v1",
        "/ip4/127.0.0.1/tcp/4001",
        "/ip4/127.0.0.1/udp/4003/webrtc-direct",
    ] {
        let addr: Multiaddr = raw.parse().unwrap();
        assert!(
            matches!(
                transport.dial(addr.clone(), dial_opts()),
                Err(TransportError::MultiaddrNotSupported(_))
            ),
            "{raw} 不该被本 transport 认领"
        );
        assert!(
            matches!(
                transport.listen_on(ListenerId::next(), addr),
                Err(TransportError::MultiaddrNotSupported(_))
            ),
            "{raw} 不该被本 transport 监听"
        );
    }
}

/// 监听地址必须通告出具体的 certhash，且与 transport 自己持有的一致 ——
/// 对不上就意味着对端拿到的地址拨不通本机。
#[tokio::test]
async fn announced_address_carries_current_certhashes() {
    let mut transport = transport(&Keypair::generate_ed25519());
    let (_, addr) = listen(&mut transport).await;

    let announced: Vec<_> = addr
        .iter()
        .filter_map(|p| match p {
            Protocol::Certhash(hash) => Some(hash),
            _ => None,
        })
        .collect();
    let expected: Vec<Multihash<64>> = transport
        .certhashes()
        .into_iter()
        .map(|h| h.to_multihash())
        .collect();

    assert_eq!(
        announced, expected,
        "通告地址里的 certhash 必须是本机当前那两张"
    );
    assert_eq!(announced.len(), 2, "spec 要求同时通告 current 与 next");
}

/// 重启后 certhash 不变 —— 这正是持久化端口存在的理由。
/// 不成立的话每次重启对端记下的地址就全部作废。
#[tokio::test]
async fn certhashes_survive_restart() {
    let store = MemoryCertificateStore::default();
    let keys = Keypair::generate_ed25519();

    let first =
        Transport::new(Config::new(keys.clone()).with_certificate_store(Arc::new(store.clone())))
            .unwrap();
    let before = first.certhashes();
    drop(first);

    let second = Transport::new(Config::new(keys).with_certificate_store(Arc::new(store))).unwrap();

    assert_eq!(second.certhashes(), before);
}

/// 撤掉监听器要把通告过的地址逐条撤回，并以 `ListenerClosed` 收尾。
/// 少了 `AddressExpired`，Swarm 会继续把已经死掉的地址广告给对端。
#[tokio::test]
async fn removing_listener_expires_its_addresses() {
    let mut transport = transport(&Keypair::generate_ed25519());
    let (id, addr) = listen(&mut transport).await;

    assert!(transport.remove_listener(id));

    match next_event(&mut transport).await {
        TransportEvent::AddressExpired { listen_addr, .. } => assert_eq!(listen_addr, addr),
        other => panic!("应先撤回地址，实得 {other:?}"),
    }
    match next_event(&mut transport).await {
        TransportEvent::ListenerClosed { reason, .. } => assert!(reason.is_ok()),
        other => panic!("应以 ListenerClosed 收尾，实得 {other:?}"),
    }
    assert!(!transport.remove_listener(id), "重复移除应返回 false");
}

/// ★ 端到端验证 `:path` 校验**真的接在链路上**。
///
/// 纯函数单测只能证明判据本身写对了；这条证明它被调用了。少了它，把
/// `is_libp2p_noise_endpoint` 的调用整个删掉，其余测试一条都不会红 —— 而后果是这个端口
/// 变成一个通用的 WebTransport 服务端，任何网页都能对它开会话。
#[tokio::test]
async fn rejects_requests_to_other_paths() {
    use wtransport::{ClientConfig, Endpoint};

    let mut server = transport(&Keypair::generate_ed25519());
    let (_, addr) = listen(&mut server).await;
    let socket = webtransport_p2p::addr::parse_dial(&addr).unwrap().socket;
    let digests: Vec<_> = server
        .certhashes()
        .into_iter()
        .map(|h| wtransport::tls::Sha256Digest::new(*h.as_bytes()))
        .collect();
    let pump = accept_one(server);

    let client = Endpoint::client(
        ClientConfig::builder()
            .with_bind_default()
            .with_server_certificate_hashes(digests)
            .build(),
    )
    .unwrap();

    for path in [
        // 路径不对
        "/",
        "/libp2p-webtransport?type=noise",
        // 路径对但缺 spec 要求的握手类型判别位
        "/.well-known/libp2p-webtransport",
        "/.well-known/libp2p-webtransport?type=tls",
    ] {
        let url = format!("https://{socket}{path}");
        assert!(
            client.connect(&url).await.is_err(),
            "{path} 必须被拒（TLS 能过，CONNECT 应回 404）"
        );
    }

    // 对照组：同一个客户端拨正确的 endpoint 必须成功 —— 否则上面四条可能只是
    // 因为「根本连不上」而通过。
    let ok = client
        .connect(&format!(
            "https://{socket}/.well-known/libp2p-webtransport?type=noise"
        ))
        .await;
    assert!(ok.is_ok(), "spec 规定的 endpoint 必须被接受，实得 {ok:?}");

    pump.abort();
}

/// 把地址里的全部 certhash 换掉，其余段原样保留。
fn replace_certhashes(addr: &Multiaddr, digests: &[[u8; 32]]) -> Multiaddr {
    let mut out = Multiaddr::empty();
    for proto in addr.iter() {
        match proto {
            Protocol::Certhash(_) => {}
            other => out.push(other),
        }
    }
    for digest in digests {
        out.push(Protocol::Certhash(Multihash::wrap(0x12, digest).unwrap()));
    }
    out
}
