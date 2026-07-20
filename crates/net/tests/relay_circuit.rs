//! M2 验收：3 节点（helper + 2 client）reservation → circuit 拨号 → RPC 穿中继。

mod common;

use common::{spawn_node, wait_event};
use serde::{Deserialize, Serialize};
use swarmdrop_net::{
    AcceptError, Endpoint, NetEvent, NodeAddr, NodeId, PathKind, ProtocolId, RelayServerConfig,
    RelayState, Router, Rpc, RpcService,
};

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
    assert_eq!(
        receiver.watch_relays().get().get(&helper_id),
        Some(&RelayState::Active)
    );

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
