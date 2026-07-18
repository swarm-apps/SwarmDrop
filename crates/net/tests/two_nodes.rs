//! M1 验收：双节点 connect / 事件 / watch / 优雅关停。

mod common;

use std::time::Duration;

use common::{spawn_node, wait_event};
use swarmdrop_net::{NetEvent, NodeAddr, PathKind};

#[tokio::test]
async fn connect_disconnect_with_events_and_watch() {
    let (a, _) = spawn_node().await;
    let (b, b_addrs) = spawn_node().await;
    let mut a_events = a.subscribe().await.expect("subscribe");
    let mut b_events = b.subscribe().await.expect("subscribe");

    // 连接：127.0.0.1 分类为 Local
    let info = a
        .connect(NodeAddr::with_addrs(b.node_id(), b_addrs))
        .await
        .expect("connect");
    assert_eq!(info.path, PathKind::Local);
    assert!(a.is_connected(b.node_id()));

    // 两侧都收到 PeerConnected 边沿
    let b_id = b.node_id();
    wait_event(
        &mut a_events,
        |e| matches!(e, NetEvent::PeerConnected { node, .. } if *node == b_id),
    )
    .await;
    let a_id = a.node_id();
    wait_event(
        &mut b_events,
        |e| matches!(e, NetEvent::PeerConnected { node, .. } if *node == a_id),
    )
    .await;

    // watch_conns 里出现对端
    assert!(a.watch_conns().get().contains_key(&b_id));

    // 重复 connect 幂等：立即返回快照
    let again = a
        .connect(NodeAddr::new(b.node_id()))
        .await
        .expect("reconnect");
    assert_eq!(again.path, PathKind::Local);

    // 断开：两侧都收到 PeerDisconnected，watch 清空
    a.disconnect(b_id).await.expect("disconnect");
    wait_event(
        &mut a_events,
        |e| matches!(e, NetEvent::PeerDisconnected { node } if *node == b_id),
    )
    .await;
    wait_event(
        &mut b_events,
        |e| matches!(e, NetEvent::PeerDisconnected { node } if *node == a_id),
    )
    .await;
    assert!(!a.is_connected(b_id));

    // 优雅关停必须在时限内完成
    tokio::time::timeout(Duration::from_secs(10), async {
        a.close().await;
        b.close().await;
    })
    .await
    .expect("close should not hang");
}

#[tokio::test]
async fn connect_unreachable_fails() {
    let (a, _) = spawn_node().await;
    let (b, _) = spawn_node().await;

    // 给一个没人监听的地址
    let bogus = NodeAddr::with_addrs(
        b.node_id(),
        vec!["/ip4/127.0.0.1/tcp/1".parse().expect("valid")],
    );
    let result = a.connect(bogus).await;
    assert!(result.is_err(), "connect to unreachable addr must fail");

    a.close().await;
    b.close().await;
}
