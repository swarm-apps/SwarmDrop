//! 生命周期契约：close 后的 API 行为、clone 共享关停、入站流配额拒绝。

mod common;

use std::time::Duration;

use common::spawn_node;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, Endpoint, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router, StreamLimits,
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
