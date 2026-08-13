//! M2 验收：webrtc-direct listener 起得来、certhash 地址随持久化证书稳定，
//! 且该监听地址真的能被拨通（Endpoint 层实拨 + 数据面往返）。

mod common;

use std::time::Duration;

use common::init_tracing;
use futures::{AsyncReadExt, AsyncWriteExt};
use swarmdrop_net::{
    AcceptError, Addr, Endpoint, NodeAddr, P2pStream, PathKind, ProtocolHandler, ProtocolId, Router,
};

const ECHO: ProtocolId = ProtocolId::from_static("/test/webrtc-direct-echo/1");

/// 原样回写的裸流 echo——用来证明连上之后数据面确实通。
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

/// 从地址集里挑出 webrtc-direct 地址的 certhash 段。
fn certhash_of(addrs: &[Addr]) -> Option<String> {
    addrs.iter().find_map(|a| {
        let s = a.to_string();
        s.contains("/webrtc-direct/certhash/")
            .then(|| s.split("/certhash/").nth(1).map(str::to_owned))
            .flatten()
    })
}

/// 等到 `endpoint` 的监听地址里出现带 certhash 的 webrtc-direct 地址。
///
/// 端口 0 + certhash 都由 transport 在 listener 就绪后回填，故必须等 watch 而非
/// 读一次快照。
async fn wait_webrtc_direct_addrs(endpoint: &Endpoint) -> Vec<Addr> {
    let mut watcher = endpoint.watch_addrs();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listen = watcher.get().listen;
            if certhash_of(&listen).is_some() {
                return listen;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("webrtc-direct listener timeout")
}

#[tokio::test]
async fn webrtc_direct_listener_and_stable_certhash() {
    init_tracing();

    // 持久化证书：两次 bind 的 certhash 必须一致（模拟身份存储里的存量）
    let cert_pem = swarmdrop_net::generate_webrtc_certificate_pem().expect("generate cert");

    let mut hashes = Vec::new();
    for _ in 0..2 {
        let ep = Endpoint::builder()
            .listen(vec![
                "/ip4/127.0.0.1/udp/0/webrtc-direct".parse().expect("valid"),
            ])
            .webrtc_certificate(cert_pem.clone())
            .bind()
            .await
            .expect("bind");

        let listen = wait_webrtc_direct_addrs(&ep).await;
        hashes.push(certhash_of(&listen).expect("certhash in listen addr"));
        ep.close().await;
    }

    assert_eq!(
        hashes[0], hashes[1],
        "同一持久化证书两次 bind 的 certhash 必须一致（分享地址稳定性）"
    );
}

/// 监听地址不只是「打印出来好看」——它必须真能被拨通。
///
/// 服务端**只**监听 webrtc-direct，拨号方也只拿到这一个地址，于是 TCP/QUIC 都不可能
/// 顶替：连上即证明 direct 这条路径（ICE-lite + DTLS + SCTP + Noise + muxer）在
/// Endpoint 这一层完整接通，而不只是在 `webrtc-p2p` 的 crate 内部自测里成立。
#[tokio::test]
async fn dial_own_webrtc_direct_listen_addr() {
    init_tracing();

    let server = Endpoint::builder()
        .listen(vec![
            "/ip4/127.0.0.1/udp/0/webrtc-direct".parse().expect("valid"),
        ])
        .bind()
        .await
        .expect("bind server");
    let server_addrs = wait_webrtc_direct_addrs(&server).await;

    // 拨号方不监听任何地址——拨号不需要 listener，也排除「其实是对端拨过来的」这种解释。
    let client = Endpoint::builder()
        .listen(Vec::new())
        .bind()
        .await
        .expect("bind client");

    let router = Router::builder(server.clone()).accept(ECHO, Echo).spawn();

    let info = client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("dial webrtc-direct listen addr");
    assert_eq!(
        info.path,
        PathKind::Local,
        "127.0.0.1 上的 webrtc-direct 是本地直连，不该被记成中转"
    );

    let payload = b"webrtc-direct roundtrip".to_vec();
    let mut stream = client
        .open(server.node_id(), ECHO)
        .await
        .expect("open stream");
    stream.write_all(&payload).await.expect("write");
    stream.close().await.expect("half-close write side");

    let mut echoed = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut echoed))
        .await
        .expect("echo timeout")
        .expect("read");
    assert_eq!(echoed, payload, "数据面必须逐字节往返");

    router.shutdown().await;
    server.close().await;
    client.close().await;
}

/// **删除 `webrtc_direct_addr_from_pem` 的前提**：`external_ip` 映射出来的公网地址必须
/// 带着与监听地址逐字相同的 certhash。
///
/// 那个函数曾是「从 PEM 第二次派生 certhash」的独立路径，公网 bootstrap 靠它静态声明
/// webrtc-direct 公网地址。改成从监听地址映射之后，如果监听地址**不带** certhash（或
/// 映射把它吃掉了），bootstrap 就会通告一条浏览器根本拨不通的地址——而这件事没有任何
/// 报错，浏览器只是连不上而已。这条测试是那个假设的唯一证据。
#[tokio::test]
async fn external_ip_mapping_carries_the_listener_certhash() {
    init_tracing();

    let public_ip = "203.0.113.10";
    let endpoint = Endpoint::builder()
        .listen(vec![
            "/ip4/127.0.0.1/udp/0/webrtc-direct".parse().expect("valid"),
        ])
        .external_ip(public_ip.parse().expect("valid ip"))
        .bind()
        .await
        .expect("bind");

    let listen_hash =
        certhash_of(&wait_webrtc_direct_addrs(&endpoint).await).expect("监听地址必须带 certhash");

    let mut watcher = endpoint.watch_addrs();
    let external = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let external = watcher.get().external;
            if certhash_of(&external).is_some() {
                return external;
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("公网地址里始终没出现 webrtc-direct");

    assert_eq!(
        certhash_of(&external).as_deref(),
        Some(listen_hash.as_str()),
        "映射出来的公网地址必须带监听地址那个 certhash：{external:?}"
    );
    assert!(
        external
            .iter()
            .any(|a| a.to_string().starts_with(&format!("/ip4/{public_ip}/udp/"))),
        "公网地址的 IP 段必须是配置的公网 IP：{external:?}"
    );

    endpoint.close().await;
}
