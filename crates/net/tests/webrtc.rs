//! M2 验收：webrtc-direct listener 起得来、certhash 地址随持久化证书稳定。
//! （浏览器端到端拨号在 M5 冒烟壳验证。）

mod common;

use std::time::Duration;

use common::init_tracing;
use swarmdrop_net::Endpoint;

/// native↔native 的 webrtc-direct 完整拨通依赖浏览器语义之外的路径，
/// 这里只验收内核侧的两件事：listener 就绪 + certhash 出现在监听地址里。
#[tokio::test]
async fn webrtc_direct_listener_and_stable_certhash() {
    init_tracing();

    // 持久化证书：两次 bind 的 certhash 必须一致
    let cert_pem = {
        // 用 libp2p-webrtc 生成一份 PEM 供注入（模拟 keychain 存量）
        let cert = libp2p_webrtc::tokio::Certificate::generate(&mut rand::thread_rng())
            .expect("generate cert");
        cert.serialize_pem()
    };

    let certhash_of = |addrs: &[swarmdrop_net::Addr]| -> Option<String> {
        addrs.iter().find_map(|a| {
            let s = a.to_string();
            s.contains("/webrtc-direct/certhash/")
                .then(|| s.split("/certhash/").nth(1).map(str::to_owned))
                .flatten()
        })
    };

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

        let mut w = ep.watch_addrs();
        let listen = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let info = w.get();
                if certhash_of(&info.listen).is_some() {
                    return info.listen;
                }
                w.updated().await.expect("watch closed");
            }
        })
        .await
        .expect("webrtc-direct listener timeout");

        hashes.push(certhash_of(&listen).expect("certhash in listen addr"));
        ep.close().await;
    }

    assert_eq!(
        hashes[0], hashes[1],
        "同一持久化证书两次 bind 的 certhash 必须一致（分享地址稳定性）"
    );
}
