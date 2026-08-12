//! WebTransport 在 `Endpoint` 层的接入验收：listener 起得来、地址带 certhash、
//! 能被真的拨通，且**数据面走的确实是 WebTransport 而不是别的 transport**。
//!
//! # 最后半句是这个文件存在的主要理由
//!
//! WebTransport 地址形如 `/ip4/…/udp/…/quic-v1/webtransport/certhash/…` —— 它**同时含**
//! `/quic-v1`。它没被 libp2p-quic 抢走，唯一依据是上游 `multiaddr_to_socketaddr` 对
//! `/quic-v1` 之后的任何非 `/p2p` 段一律 `return None`（`transports/quic/src/transport.rs`）。
//!
//! 那是一条**上游实现细节**，不是契约。升 libp2p rev 时它可能变，而变了之后的表现是
//! WebTransport 地址被 quic 认领、然后永远拨不通 —— 没有任何错误指向真正的原因。
//! 本文件是那条判据在 CI 里的唯一看守。

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::init_tracing;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, Addr, Endpoint, NodeAddr, P2pStream, ProtocolHandler, ProtocolId, Router,
    WebTransportConfig, WebTransportMemoryCertificateStore,
};

const ECHO: ProtocolId = ProtocolId::from_static("/test/webtransport-echo/1");

/// 原样回写的裸流 echo —— 证明连上之后数据面确实通。
#[derive(Debug, Clone)]
struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, mut stream: P2pStream) -> Result<(), AcceptError> {
        let mut data = Vec::new();
        stream.read_to_end(&mut data).await?;
        stream.write_all(&data).await?;
        stream.close().await?;
        Ok(())
    }
}

/// 挑出 WebTransport 监听地址。
fn webtransport_addr(addrs: &[Addr]) -> Option<Addr> {
    addrs
        .iter()
        .find(|a| a.to_string().contains("/webtransport/certhash/"))
        .cloned()
}

/// 等到 WebTransport 监听地址就绪。
///
/// 端口 0 与 certhash 都由 transport 在 listener 就绪后回填，故必须等 watch 而非读快照。
async fn wait_webtransport_addr(endpoint: &Endpoint) -> Addr {
    let mut watcher = endpoint.watch_addrs();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(addr) = webtransport_addr(&watcher.get().listen) {
                return addr;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("webtransport listener 超时")
}

async fn spawn(store: WebTransportMemoryCertificateStore, listen: Vec<Addr>) -> Endpoint {
    Endpoint::builder()
        .listen(listen)
        .webtransport(WebTransportConfig::with_store(Arc::new(store)))
        .bind()
        .await
        .expect("bind")
}

/// ★ 端到端：经 `Endpoint` 拨通 WebTransport 地址并跑一次 echo。
///
/// 这同时证明了三件事：地址分派把它交给了 WebTransport（而不是 libp2p-quic）、
/// Noise 认证过了、子流数据面通。
#[tokio::test]
async fn dials_a_webtransport_address_end_to_end() {
    init_tracing();

    let server = spawn(
        WebTransportMemoryCertificateStore::default(),
        vec!["/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap()],
    )
    .await;
    let listen = wait_webtransport_addr(&server).await;
    let router = Router::builder(server.clone()).accept(ECHO, Echo).spawn();

    // 拨号方不监听任何东西 —— 但仍要注入证书端口，否则 transport 根本不装配。
    let client = spawn(WebTransportMemoryCertificateStore::default(), Vec::new()).await;

    let target = NodeAddr::with_addrs(server.node_id(), vec![listen.clone()]);
    tokio::time::timeout(Duration::from_secs(20), client.connect(target))
        .await
        .expect("连接超时")
        .expect("应能拨通 WebTransport 地址");

    let mut stream = client.open(server.node_id(), ECHO).await.expect("开流");
    stream.write_all(b"webtransport").await.expect("写");
    stream.close().await.expect("关写端");
    let mut echoed = Vec::new();
    stream.read_to_end(&mut echoed).await.expect("读");
    assert_eq!(&echoed, b"webtransport", "数据面必须原样往返");

    router.shutdown().await;
    client.close().await;
    server.close().await;
}

/// ★ 纯拨号方用 `client_only()` 就能拨通 —— **不需要伪造一个证书存储**。
///
/// 这是 `WebTransportConfig` 把「启用」与「服务端证书持久化」拆开的全部理由。没有这条测试
/// 的话，那个构造函数在全仓零真实调用点，下一个人只会得出「它不管用」的结论。
#[tokio::test]
async fn client_only_can_dial_without_a_certificate_store() {
    init_tracing();

    let server = spawn(
        WebTransportMemoryCertificateStore::default(),
        vec!["/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap()],
    )
    .await;
    let listen = wait_webtransport_addr(&server).await;

    let client = Endpoint::builder()
        .listen(Vec::new())
        .webtransport(WebTransportConfig::client_only())
        .bind()
        .await
        .expect("bind");

    let target = NodeAddr::with_addrs(server.node_id(), vec![listen]);
    let result = tokio::time::timeout(Duration::from_secs(20), client.connect(target)).await;

    assert!(
        matches!(result, Ok(Ok(_))),
        "client_only 必须能拨通，实得 {result:?}"
    );

    client.close().await;
    server.close().await;
}

/// 通告地址必须带**两个** certhash（spec 要求同时通告 current 与 next），
/// 且持久化后重启保持不变。
///
/// 后半句是持久化端口存在的全部理由：变了就意味着对端记下的地址集体失效。
#[tokio::test]
async fn certhashes_are_advertised_and_stable_across_restarts() {
    init_tracing();

    let store = WebTransportMemoryCertificateStore::default();
    let mut seen = Vec::new();

    for _ in 0..2 {
        let ep = spawn(
            store.clone(),
            vec!["/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap()],
        )
        .await;
        let addr = wait_webtransport_addr(&ep).await.to_string();

        assert_eq!(
            addr.matches("/certhash/").count(),
            2,
            "spec 要求同时通告 current 与 next，实得 {addr}"
        );
        // 端口是 0 分配的，两次不同；只比 certhash 段。
        seen.push(
            addr.split("/certhash/")
                .skip(1)
                .collect::<Vec<_>>()
                .join("|"),
        );
        ep.close().await;
    }

    assert_eq!(seen[0], seen[1], "重启后 certhash 必须不变");
}

/// **未启用** WebTransport 时，那类地址不该被认领 —— 快速失败好过挂着等超时。
///
/// ⚠️ 名字里是「未启用」不是「未注入证书端口」：两者是不同的判据，而
/// `WebTransportConfig::client_only()` 的存在就是为了把它们分开。上一版把这条测试命名成
/// 前者、注释写成后者，等于用错误的判据看守一条正确的规则。
#[tokio::test]
async fn without_enabling_webtransport_the_address_is_not_claimed() {
    init_tracing();

    // 起一个真的 WebTransport listener，确保失败不是因为「没人接」。
    let server = spawn(
        WebTransportMemoryCertificateStore::default(),
        vec!["/ip4/127.0.0.1/udp/0/quic-v1/webtransport".parse().unwrap()],
    )
    .await;
    let listen = wait_webtransport_addr(&server).await;

    // 拨号方**完全不调 `.webtransport(..)`** → transport 不装配。
    let client = Endpoint::builder()
        .listen(Vec::new())
        .bind()
        .await
        .expect("bind");

    let target = NodeAddr::with_addrs(server.node_id(), vec![listen]);
    let result = tokio::time::timeout(Duration::from_secs(15), client.connect(target)).await;

    match result {
        Ok(Err(_)) => {}
        // 超时也算失败，但那说明它在挂着等而不是快速拒 —— 值得单独报出来。
        Err(_) => panic!("未装配 WebTransport 时应快速失败，而不是挂到超时"),
        Ok(Ok(_)) => panic!("未装配 WebTransport 却拨通了 —— 说明地址被别的 transport 认领了"),
    }

    client.close().await;
    server.close().await;
}
