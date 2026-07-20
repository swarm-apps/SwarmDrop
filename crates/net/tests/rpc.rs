//! M1 验收：typed RPC（echo / 延迟应答模拟用户决策 / 未连接直接 call）。

mod common;

use std::time::Duration;

use common::spawn_node;
use serde::{Deserialize, Serialize};
use swarmdrop_net::{AcceptError, NodeAddr, NodeId, ProtocolId, Router, Rpc, RpcService};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EchoReq {
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EchoResp {
    text: String,
    from: String,
}

const ECHO: Rpc<EchoReq, EchoResp> = Rpc::new(ProtocolId::from_static("/test/echo/1"));

#[derive(Debug)]
struct EchoService {
    /// 应答前的人为延迟——模拟「等用户决策」的长交互。
    delay: Duration,
}

impl RpcService<EchoReq, EchoResp> for EchoService {
    async fn handle(&self, from: NodeId, req: EchoReq) -> Result<EchoResp, AcceptError> {
        tokio::time::sleep(self.delay).await;
        Ok(EchoResp {
            text: req.text,
            from: from.to_string(),
        })
    }
}

#[tokio::test]
async fn rpc_echo_roundtrip() {
    let (client, _) = spawn_node().await;
    let (server, server_addrs) = spawn_node().await;

    let router = Router::builder(server.clone())
        .accept(
            ECHO.protocol(),
            ECHO.handler(EchoService {
                delay: Duration::ZERO,
            }),
        )
        .spawn();

    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    let resp = ECHO
        .call(
            &client,
            server.node_id(),
            &EchoReq {
                text: "hello".into(),
            },
        )
        .await
        .expect("rpc call");
    assert_eq!(resp.text, "hello");
    assert_eq!(
        resp.from,
        client.node_id().to_string(),
        "服务端看到的 remote 是调用方身份"
    );

    router.shutdown().await;
    server.close().await;
    client.close().await;
}

#[tokio::test]
async fn rpc_handler_can_await_before_responding() {
    let (client, _) = spawn_node().await;
    let (server, server_addrs) = spawn_node().await;

    let router = Router::builder(server.clone())
        .accept(
            ECHO.protocol(),
            ECHO.handler(EchoService {
                delay: Duration::from_secs(2),
            }),
        )
        .spawn();

    client
        .connect(NodeAddr::with_addrs(server.node_id(), server_addrs))
        .await
        .expect("connect");

    // handler 延迟 2s 应答（模拟等待用户点「接受」），默认 120s 超时内成功
    let started = std::time::Instant::now();
    let resp = ECHO
        .call(
            &client,
            server.node_id(),
            &EchoReq {
                text: "wait".into(),
            },
        )
        .await
        .expect("delayed rpc call");
    assert_eq!(resp.text, "wait");
    assert!(started.elapsed() >= Duration::from_secs(2));

    router.shutdown().await;
    server.close().await;
    client.close().await;
}

#[tokio::test]
async fn rpc_call_without_prior_connect_dials_on_demand() {
    let (client, _) = spawn_node().await;
    let (server, server_addrs) = spawn_node().await;

    let router = Router::builder(server.clone())
        .accept(
            ECHO.protocol(),
            ECHO.handler(EchoService {
                delay: Duration::ZERO,
            }),
        )
        .spawn();

    // 不显式 connect：只注入地址簿，call 内部按需拨号（复刻旧栈 request-response 语义）
    client
        .add_addrs(server.node_id(), server_addrs)
        .await
        .expect("add addrs");
    assert!(!client.is_connected(server.node_id()));

    let resp = ECHO
        .call(
            &client,
            server.node_id(),
            &EchoReq {
                text: "dial".into(),
            },
        )
        .await
        .expect("call with on-demand dial");
    assert_eq!(resp.text, "dial");

    router.shutdown().await;
    server.close().await;
    client.close().await;
}
