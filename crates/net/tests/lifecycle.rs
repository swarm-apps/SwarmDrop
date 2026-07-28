//! 生命周期契约：close 后的 API 行为、clone 共享关停、入站流配额拒绝。

mod common;

use std::net::TcpListener;
use std::time::{Duration, Instant};

use common::spawn_node;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, ConnectError, Endpoint, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router,
    SecretKey, StreamLimits,
};

const HOLD: ProtocolId = ProtocolId::from_static("/test/hold/1");

/// 读到 EOF 才回一个字节的 handler——用于占住入站配额。
#[derive(Debug, Clone)]
struct HoldEcho;

impl ProtocolHandler for HoldEcho {
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await?;
        stream.write_all(&[1]).await?;
        stream.close().await?;
        Ok(())
    }
}

#[tokio::test]
async fn closed_endpoint_rejects_all_operations() {
    let (a, _) = spawn_node().await;
    let (b, b_addrs) = spawn_node().await;
    let a_clone = a.clone();

    a.close().await;

    // clone 共享同一 actor——一处 close 处处失效
    assert!(
        a_clone
            .connect(NodeAddr::with_addrs(b.node_id(), b_addrs))
            .await
            .is_err(),
        "closed endpoint 的 connect 必须失败"
    );
    assert!(a_clone.open(b.node_id(), HOLD).await.is_err());
    assert!(a_clone.subscribe().await.is_err());
    assert!(a_clone.add_addrs(b.node_id(), vec![]).await.is_err());

    // close 幂等
    a_clone.close().await;

    // closed() 信号已 resolve
    tokio::time::timeout(Duration::from_secs(1), a.closed())
        .await
        .expect("closed() should resolve after close()");

    b.close().await;
}

#[tokio::test]
async fn subscriber_stream_ends_on_close() {
    let (a, _) = spawn_node().await;
    let mut events = a.subscribe().await.expect("subscribe");
    a.close().await;
    // actor 停止 → 事件流结束（不是挂死）
    let end = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event stream must end, not hang");
    assert!(end.is_none());
}

/// connect 超时不只是丢掉 Future：第二次同 peer 拨号必须能新建 TCP 连接。
/// 否则会复用第一次遗留的 pending dial（libp2p 的 DialPeerConditionFalse）。
#[tokio::test]
async fn connect_timeout_aborts_orphaned_dial() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind hanging peer");
    listener
        .set_nonblocking(true)
        .expect("make listener non-blocking");
    let port = listener.local_addr().expect("listener addr").port();
    let accepted = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut sockets = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                // 保持 TCP 打开但不参与 Noise 握手：每次 client 拨号都会留下一
                // 条可计数连接，直到测试结束统一 drop。
                Ok((socket, _)) => sockets.push(socket),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept hanging peer: {error}"),
            }
        }
        sockets.len()
    });

    let client = Endpoint::builder()
        .connect_timeout(Duration::from_millis(100))
        .bind()
        .await
        .expect("bind client");
    let target = NodeAddr::with_addrs(
        SecretKey::generate().node_id(),
        vec![
            format!("/ip4/127.0.0.1/tcp/{port}")
                .parse()
                .expect("valid address"),
        ],
    );
    let result = client.connect(target.clone()).await;
    assert!(
        matches!(result, Err(ConnectError::Timeout)),
        "hanging peer 应在调用方时限内超时，got: {result:?}"
    );
    // 给 actor 一次 poll Swarm 的机会处理 libp2p abort；随后同 peer 再拨一次。
    tokio::time::sleep(Duration::from_millis(50)).await;
    let retry = client
        .connect_with_timeout(target, Duration::from_millis(100))
        .await;
    assert!(
        matches!(retry, Err(ConnectError::Timeout)),
        "第二次拨号也应独立超时，got: {retry:?}"
    );
    assert!(
        accepted.join().expect("hanging peer thread") >= 2,
        "取消旧拨号后，同 peer 的新 connect 必须发起新的 TCP 拨号"
    );

    client.close().await;
}

#[tokio::test]
async fn explicitly_registered_external_addresses_are_published() {
    let configured: swarmdrop_net::Addr = "/ip4/203.0.113.10/tcp/4001".parse().unwrap();
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .external_addrs(vec![configured.clone()])
        .bind()
        .await
        .expect("bind");

    let dynamic: swarmdrop_net::Addr = "/ip4/203.0.113.10/udp/4003/quic-v1".parse().unwrap();
    endpoint
        .add_external_addr(dynamic.clone())
        .await
        .expect("register dynamic address");

    let mut watcher = endpoint.watch_addrs();
    let external = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let external = watcher.get().external;
            if external.contains(&configured) && external.contains(&dynamic) {
                return external;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("external addresses should be published");
    assert!(external.contains(&configured));
    assert!(external.contains(&dynamic));
    endpoint.close().await;
}

#[tokio::test]
async fn inbound_streams_beyond_limit_are_rejected() {
    // server 每 peer 只允许 1 条入站流
    let server = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .stream_limits(StreamLimits {
            max_inbound_per_peer: 1,
            max_outbound_per_peer: 8,
            max_per_protocol: 64,
        })
        .bind()
        .await
        .expect("bind server");
    let server_addrs = common::wait_listen_addrs(&server).await;
    let router = Router::builder(server.clone())
        .accept(HOLD, HoldEcho)
        .spawn();

    let (client, _) = spawn_node().await;
    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    // 第一条流：不关写侧，长期占住配额
    let mut held = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open first");
    held.write_all(b"hold").await.expect("write");
    // （不 close——handler 的 read_to_end 挂着，配额不归还）

    // 给 Router 一点时间领走第一条流
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 第二条流：超限，Router 直接 drop——读侧表现为立即 EOF/reset
    let mut rejected = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open second (open 本身成功，拒绝发生在对端)");
    let _ = rejected.write_all(b"x").await;
    let _ = rejected.close().await;
    let mut sink = Vec::new();
    let outcome = tokio::time::timeout(Duration::from_secs(5), rejected.read_to_end(&mut sink))
        .await
        .expect("rejected stream must resolve quickly");
    assert!(
        outcome.is_err() || sink.is_empty(),
        "超限流不得收到任何业务响应，got: {sink:?}"
    );

    // 释放第一条流后配额归还，新流恢复服务
    held.close().await.expect("close held");
    let mut resp = Vec::new();
    held.read_to_end(&mut resp).await.expect("held echo");
    assert_eq!(resp, [1]);

    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut ok = client
        .open(server.node_id(), HOLD)
        .await
        .expect("open after release");
    ok.write_all(b"again").await.expect("write");
    ok.close().await.expect("close");
    let mut resp = Vec::new();
    ok.read_to_end(&mut resp).await.expect("read");
    assert_eq!(resp, [1], "配额释放后新流必须恢复服务");

    router.shutdown().await;
    client.close().await;
    server.close().await;
}
