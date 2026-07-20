//! M2 验收：DHT put/get/remove + StaticLookup 按需解析。

mod common;

use std::time::Duration;

use common::spawn_node;
use swarmdrop_net::{
    DhtConfig, DhtError, DhtKey, Endpoint, InfraRoles, NodeAddr, StaticLookup, presets,
};

/// 起一个开 DHT 的节点（server 模式，测试环境已知可达）。
async fn spawn_dht_node() -> (Endpoint, Vec<swarmdrop_net::Addr>) {
    let endpoint = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .dht(DhtConfig {
            server_mode: true,
            ..DhtConfig::default()
        })
        .bind()
        .await
        .expect("bind");
    let addrs = common::wait_listen_addrs(&endpoint).await;
    (endpoint, addrs)
}

#[tokio::test]
async fn dht_put_get_remove_roundtrip() {
    let (a, _) = spawn_dht_node().await;
    let (b, b_addrs) = spawn_dht_node().await;

    // 互相注册为 kad server（进路由表）
    a.add_infrastructure_peer(
        NodeAddr::with_addrs(b.node_id(), b_addrs),
        InfraRoles {
            relay: false,
            kad_server: true,
        },
    )
    .await
    .expect("add infra");
    a.connect(NodeAddr::new(b.node_id()))
        .await
        .expect("connect");

    let dht = a.dht().expect("dht enabled").clone();
    let key = DhtKey::namespaced("/swarmdrop/share-code/", b"123456");

    // put → 对端可 get 且 publisher 是发布者
    dht.put(
        key,
        b"record-payload".to_vec(),
        Some(Duration::from_secs(300)),
    )
    .await
    .expect("put");

    let b_dht = b.dht().expect("dht enabled").clone();
    let record = b_dht.get(key).await.expect("get");
    assert_eq!(record.value, b"record-payload");
    assert_eq!(record.publisher, Some(a.node_id()), "publisher 应为发布方");

    // 未知 key → NotFound
    let missing = DhtKey::namespaced("/swarmdrop/share-code/", b"999999");
    assert!(matches!(b_dht.get(missing).await, Err(DhtError::NotFound)));

    // 本地移除幂等成功
    dht.remove(key).await.expect("remove");

    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn dht_disabled_returns_none() {
    let (a, _) = spawn_node().await;
    assert!(a.dht().is_none(), "未启用 DHT 时 dht() 应为 None");
    a.close().await;
}

#[tokio::test]
async fn static_lookup_resolves_on_connect() {
    let (server, server_addrs) = spawn_node().await;

    // client 只配 StaticLookup，不注入地址簿
    let client = Endpoint::builder()
        .preset(presets::Native)
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .mdns(false)
        .address_lookup(StaticLookup::new([(server.node_id(), server_addrs)]))
        .bind()
        .await
        .expect("bind client");

    // connect 只给 NodeId——地址由 lookup 解析
    let info = client
        .connect(NodeAddr::new(server.node_id()))
        .await
        .expect("connect via lookup");
    assert!(client.is_connected(server.node_id()));
    drop(info);

    // 未知节点：lookup 无结果 → NoAddresses
    let unknown = swarmdrop_net::SecretKey::generate().node_id();
    let err = client.connect(NodeAddr::new(unknown)).await;
    assert!(err.is_err(), "未知节点应解析失败");

    client.close().await;
    server.close().await;
}
