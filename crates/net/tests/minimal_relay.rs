//! 最小 repro：脱离 swarmdrop-net 封装，验证 libp2p master（pin rev）的
//! relay server + client reservation 在我们的 feature 集下是否工作。
//! 用于二分「上游/feature 问题」vs「我们的 Behaviour 组合问题」。

use std::time::Duration;

use futures::StreamExt;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, SwarmBuilder, identify, noise, ping, relay, tcp, yamux};

#[derive(NetworkBehaviour)]
struct ServerBehaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    relay: relay::Behaviour,
}

#[derive(NetworkBehaviour)]
struct ClientBehaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    relay_client: relay::client::Behaviour,
}

#[tokio::test]
async fn minimal_reservation() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    // ── relay server ──
    let mut server = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|key| ServerBehaviour {
            identify: identify::Behaviour::new(identify::Config::new(
                "/test/1".into(),
                key.public(),
            )),
            ping: ping::Behaviour::new(ping::Config::new()),
            relay: {
                // master PR 6154：HOP 广告默认随 external addr 自动开关，
                // 本地测试无公网地址必须显式 Enable
                let mut r =
                    relay::Behaviour::new(key.public().to_peer_id(), relay::Config::default());
                r.set_status(Some(relay::Status::Enable));
                r
            },
        })
        .unwrap()
        .build();
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let server_id = *server.local_peer_id();

    // 等 server 地址；同时登记为 external——reservation 应答必须携带
    // relay 自身的 external 地址（否则 client 报 NoAddressesInReservation）
    let server_addr: Multiaddr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            server.add_external_address(address.clone());
            break address;
        }
    };

    // ── client ──
    let mut client = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .unwrap()
        .with_behaviour(|key, relay_client| ClientBehaviour {
            identify: identify::Behaviour::new(identify::Config::new(
                "/test/1".into(),
                key.public(),
            )),
            ping: ping::Behaviour::new(ping::Config::new()),
            relay_client,
        })
        .unwrap()
        .build();

    // client 连 server 后 listen circuit
    client.dial(server_addr.clone()).unwrap();
    let circuit: Multiaddr = format!("{server_addr}/p2p/{server_id}/p2p-circuit")
        .parse()
        .unwrap();

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut listened = false;
        loop {
            tokio::select! {
                ev = client.select_next_some() => match ev {
                    SwarmEvent::ConnectionEstablished { .. } if !listened => {
                        listened = true;
                        client.listen_on(circuit.clone()).unwrap();
                    }
                    SwarmEvent::Behaviour(ClientBehaviourEvent::RelayClient(
                        relay::client::Event::ReservationReqAccepted { .. },
                    )) => return true,
                    other => tracing::debug!(?other, "client event"),
                },
                ev = server.select_next_some() => {
                    tracing::debug!(?ev, "server event");
                }
            }
        }
    })
    .await;

    assert!(
        matches!(result, Ok(true)),
        "минimal relay reservation 应在 10s 内被接受"
    );
}
