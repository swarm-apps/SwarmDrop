//! M2 验收：3 节点（helper + 2 client）reservation → circuit 拨号 → RPC 穿中继。

mod common;

use std::time::Duration;

use common::{spawn_node, wait_event};
use serde::{Deserialize, Serialize};
use swarmdrop_net::{
    AcceptError, Endpoint, NetEvent, NodeAddr, NodeId, PathKind, ProtocolId, RelayServerConfig,
    RelayState, Router, Rpc, RpcService, SecretKey,
};

/// 等 watch_relays 中某 relay 的状态满足谓词（10s 超时），返回该状态快照。
async fn wait_relay_state(
    endpoint: &Endpoint,
    relay: NodeId,
    pred: impl Fn(Option<&RelayState>) -> bool,
) -> Option<RelayState> {
    let mut watcher = endpoint.watch_relays();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let map = watcher.get();
            if pred(map.get(&relay)) {
                return map.get(&relay).cloned();
            }
            watcher.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("relay state timeout")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ping2(String);
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pong2(String);

const RELAY_ECHO: Rpc<Ping2, Pong2> = Rpc::new(ProtocolId::from_static("/test/relay-echo/1"));

#[derive(Debug)]
struct Echo2;

impl RpcService<Ping2, Pong2> for Echo2 {
    async fn handle(&self, _from: NodeId, req: Ping2) -> Result<Pong2, AcceptError> {
        Ok(Pong2(req.0))
    }
}

/// helper 兼任 relay server；receiver 经 helper 建 reservation；
/// sender 拨 `<helper>/p2p-circuit/p2p/<receiver>` 打通 RPC。
#[tokio::test]
async fn rpc_through_circuit_relay() {
    common::init_tracing();
    // helper：本机 LanHelper 形态（relay server 开）
    let helper = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .relay_server(RelayServerConfig::default())
        .bind()
        .await
        .expect("bind helper");
    let helper_addrs = common::wait_listen_addrs(&helper).await;

    let (receiver, _) = spawn_node().await;
    let (sender, _) = spawn_node().await;
    let mut receiver_events = receiver.subscribe().await.expect("subscribe");

    // receiver 经 helper 请求 reservation
    receiver
        .ensure_relay_reservation(NodeAddr::with_addrs(helper.node_id(), helper_addrs.clone()))
        .await
        .expect("ensure reservation");

    // 等 reservation 被接受（事件 + watch 双确认）
    let helper_id = helper.node_id();
    wait_event(
        &mut receiver_events,
        |e| matches!(e, NetEvent::RelayReservationAccepted { relay, .. } if *relay == helper_id),
    )
    .await;
    // Active 携带内核拼装的 circuit 地址（单一事实源，调用方不自行拼接）
    let expected_circuit: swarmdrop_net::Addr = format!(
        "{}/p2p/{}/p2p-circuit/p2p/{}",
        helper_addrs[0],
        helper.node_id(),
        receiver.node_id()
    )
    .parse()
    .expect("valid circuit addr");
    match receiver.watch_relays().get().get(&helper_id) {
        Some(RelayState::Active { circuit_addr }) => {
            assert_eq!(circuit_addr, &expected_circuit, "circuit 地址应由内核下发");
        }
        other => panic!("应为 Active，实际 {other:?}"),
    }

    // receiver 挂 RPC 服务
    let router = Router::builder(receiver.clone())
        .accept(RELAY_ECHO.protocol(), RELAY_ECHO.handler(Echo2))
        .spawn();

    // sender 拨 circuit 地址
    let circuit_addr: swarmdrop_net::Addr = format!(
        "{}/p2p/{}/p2p-circuit/p2p/{}",
        helper_addrs[0],
        helper.node_id(),
        receiver.node_id()
    )
    .parse()
    .expect("valid circuit addr");

    let info = sender
        .connect(NodeAddr::with_addrs(receiver.node_id(), vec![circuit_addr]))
        .await
        .expect("connect via circuit");
    assert_eq!(info.path, PathKind::Relayed, "circuit 连接路径应为 Relayed");

    // RPC 穿中继
    let resp = RELAY_ECHO
        .call(&sender, receiver.node_id(), &Ping2("via-relay".into()))
        .await
        .expect("rpc through relay");
    assert_eq!(resp.0, "via-relay");

    router.shutdown().await;
    sender.close().await;
    receiver.close().await;
    helper.close().await;
}

/// 不可达 helper：拨号失败（全地址耗尽）应翻 `Failed` 并携带错误信息，
/// 而非永远停留在 `Connecting`（issue #84 的挂起根因）。
#[tokio::test]
async fn unreachable_helper_enters_failed() {
    common::init_tracing();
    let (node, _) = spawn_node().await;

    // 无人监听的地址（TCP 端口 1 拒连）+ 随机身份
    let ghost = SecretKey::generate().node_id();
    node.ensure_relay_reservation(NodeAddr::with_addrs(
        ghost,
        vec!["/ip4/127.0.0.1/tcp/1".parse().expect("valid")],
    ))
    .await
    .expect("register intent");

    let state = wait_relay_state(&node, ghost, |s| {
        matches!(s, Some(RelayState::Failed { .. }))
    })
    .await;
    match state {
        Some(RelayState::Failed { last_error }) => {
            assert!(!last_error.is_empty(), "失败必须携带错误信息");
        }
        other => panic!("应为 Failed，实际 {other:?}"),
    }

    node.close().await;
}

/// 注销后：watch 条目消失，且不再有任何针对该节点的重试路径。
#[tokio::test]
async fn removed_relay_is_forgotten() {
    common::init_tracing();
    let (node, _) = spawn_node().await;

    let ghost = SecretKey::generate().node_id();
    node.ensure_relay_reservation(NodeAddr::with_addrs(
        ghost,
        vec!["/ip4/127.0.0.1/tcp/1".parse().expect("valid")],
    ))
    .await
    .expect("register intent");
    wait_relay_state(&node, ghost, |s| {
        matches!(s, Some(RelayState::Failed { .. }))
    })
    .await;

    node.remove_infrastructure_peer(ghost)
        .await
        .expect("remove");
    wait_relay_state(&node, ghost, |s| s.is_none()).await;

    // 静置观察：无任何复活（Connecting/Failed 重现即回归）
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !node.watch_relays().get().contains_key(&ghost),
        "注销后不得复活"
    );

    node.close().await;
}

/// 注销活跃 reservation：circuit listener 关闭、条目消失；
/// 重新登记后可再次建立（注销不是永久黑名单）。
#[tokio::test]
async fn remove_active_reservation_closes_listener() {
    common::init_tracing();
    let helper = Endpoint::builder()
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().expect("valid")])
        .relay_server(RelayServerConfig::default())
        .bind()
        .await
        .expect("bind helper");
    let helper_addrs = common::wait_listen_addrs(&helper).await;
    let helper_id = helper.node_id();

    let (node, _) = spawn_node().await;
    node.ensure_relay_reservation(NodeAddr::with_addrs(helper_id, helper_addrs.clone()))
        .await
        .expect("ensure reservation");
    wait_relay_state(&node, helper_id, |s| {
        matches!(s, Some(RelayState::Active { .. }))
    })
    .await;
    assert!(
        node.watch_addrs()
            .get()
            .listen
            .iter()
            .any(|a| a.is_circuit()),
        "Active 后 listen 视图应含 circuit 地址"
    );

    node.remove_infrastructure_peer(helper_id)
        .await
        .expect("remove");
    wait_relay_state(&node, helper_id, |s| s.is_none()).await;
    // circuit listener 关闭后其地址从 listen 视图移除
    let mut addrs_watch = node.watch_addrs();
    tokio::time::timeout(Duration::from_secs(10), async {
        while addrs_watch.get().listen.iter().any(|a| a.is_circuit()) {
            addrs_watch.updated().await.expect("watch closed");
        }
    })
    .await
    .expect("circuit listen addr should be removed");

    // 可逆性：重新登记 → 再次 Active
    node.ensure_relay_reservation(NodeAddr::with_addrs(helper_id, helper_addrs))
        .await
        .expect("re-ensure");
    wait_relay_state(&node, helper_id, |s| {
        matches!(s, Some(RelayState::Active { .. }))
    })
    .await;

    node.close().await;
    helper.close().await;
}
