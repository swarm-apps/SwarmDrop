//! M1 验收：裸流大数据 / handler panic 隔离 / 优雅关停。

mod common;

use std::time::Duration;

use common::spawn_node;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{AcceptError, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router};

const BULK: ProtocolId = ProtocolId::from_static("/test/bulk/1");
const BOOM: ProtocolId = ProtocolId::from_static("/test/boom/1");

/// 读全部字节再原样写回的裸流 echo。
#[derive(Debug, Clone)]
struct BulkEcho;

impl ProtocolHandler for BulkEcho {
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let mut data = Vec::new();
        stream.read_to_end(&mut data).await?;
        stream.write_all(&data).await?;
        stream.close().await?;
        Ok(())
    }
}

/// 见流就 panic 的 handler——验证 panic 被 per-stream 任务隔离。
#[derive(Debug, Clone)]
struct Panicker;

impl ProtocolHandler for Panicker {
    async fn accept(&self, _stream: P2pStream) -> Result<(), AcceptError> {
        panic!("boom");
    }
}

#[tokio::test]
async fn bulk_stream_8mib_roundtrip() {
    let (client, _) = spawn_node().await;
    let (server, server_addrs) = spawn_node().await;

    let router = Router::builder(server.clone())
        .accept(BULK, BulkEcho)
        .spawn();

    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    let payload = vec![0xAB_u8; 8 * 1024 * 1024];
    let mut stream = client
        .open(server.node_id(), BULK)
        .await
        .expect("open stream");
    stream.write_all(&payload).await.expect("write");
    stream.close().await.expect("half-close write side");

    let mut echoed = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut echoed))
        .await
        .expect("echo timeout")
        .expect("read");
    assert_eq!(echoed.len(), payload.len());
    assert_eq!(echoed, payload, "8MiB 数据必须逐字节一致");

    router.shutdown().await;
    server.close().await;
    client.close().await;
}

#[tokio::test]
async fn handler_panic_does_not_take_down_router() {
    let (client, _) = spawn_node().await;
    let (server, server_addrs) = spawn_node().await;

    let router = Router::builder(server.clone())
        .accept(BOOM, Panicker)
        .accept(BULK, BulkEcho)
        .spawn();

    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    // 第一条流触发 handler panic
    let mut boom = client
        .open(server.node_id(), BOOM)
        .await
        .expect("open boom");
    let _ = boom.write_all(b"x").await;
    let _ = boom.close().await;
    // 对端 handler panic 后本端读到流被 reset/EOF——不 hang 即可
    let mut sink = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), boom.read_to_end(&mut sink)).await;

    // Router 仍然活着：BULK 协议照常服务
    let mut stream = client
        .open(server.node_id(), BULK)
        .await
        .expect("open bulk");
    stream.write_all(b"still alive").await.expect("write");
    stream.close().await.expect("close");
    let mut echoed = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut echoed))
        .await
        .expect("echo timeout")
        .expect("read");
    assert_eq!(echoed, b"still alive");

    router.shutdown().await;
    server.close().await;
    client.close().await;
}

#[tokio::test]
#[should_panic(expected = "registered twice")]
async fn duplicate_protocol_registration_panics() {
    let (endpoint, _) = spawn_node().await;
    let _ = Router::builder(endpoint)
        .accept(BULK, BulkEcho)
        .accept(BULK, BulkEcho);
}
