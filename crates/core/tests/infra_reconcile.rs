//! infra 收敛端到端：relay reservation 断线后自动重建。
//!
//! 场景对应用户拓扑的核心痛点——LAN Helper（手机）挂起/重启后，
//! 依赖它的设备必须在不重启节点的前提下自动恢复跨网可达。

use std::sync::Arc;
use std::time::Duration;

use swarm_p2p_core::libp2p::PeerId;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarm_p2p_core::{LanHelperConfig, NodeConfig};
use swarmdrop_core::AppResult;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::network::candidates::{
    BootstrapCandidateSource, CandidateRoles, CandidateScope,
};
use swarmdrop_core::network::config::create_candidate_manager;
use swarmdrop_core::network::event_loop::handle_core_node_event;
use swarmdrop_core::network::{NetManager, NetworkRuntimeConfig};
use swarmdrop_core::protocol::{AppRequest, AppResponse};

struct NoopBus;

#[async_trait::async_trait]
impl EventBus for NoopBus {
    async fn publish(&self, _event: CoreEvent) -> AppResult<()> {
        Ok(())
    }
}

const PROTOCOL: &str = "/swarmdrop-infra-e2e/1.0.0";

fn client_node_config() -> NodeConfig {
    NodeConfig::new(PROTOCOL, "test/1.0.0")
        .with_listen_addrs(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .with_mdns(false)
        .with_relay_client(true)
        .with_dcutr(false)
        .with_autonat(false)
}

fn helper_node_config() -> NodeConfig {
    client_node_config().with_lan_helper(LanHelperConfig {
        announce_loopback_addrs: true,
        ..LanHelperConfig::default()
    })
}

struct Helper {
    peer_id: PeerId,
    client: swarm_p2p_core::NetClient<AppRequest, AppResponse>,
    events: swarm_p2p_core::EventReceiver<AppRequest>,
}

fn start_helper(keypair: Keypair) -> Helper {
    let peer_id = PeerId::from_public_key(&keypair.public());
    let (client, events, _dc) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, helper_node_config())
            .expect("start helper");
    Helper {
        peer_id,
        client,
        events,
    }
}

async fn helper_listen_addr(helper: &Helper) -> swarm_p2p_core::libp2p::Multiaddr {
    for _ in 0..50 {
        if let Ok(addrs) = helper.client.get_addrs().await
            && let Some(addr) = addrs.first()
        {
            return addr.clone();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("helper 监听地址未就绪");
}

async fn poll_until<F: FnMut() -> bool>(mut pred: F, timeout: Duration, label: &str) {
    let steps = (timeout.as_millis() / 200).max(1);
    for _ in 0..steps {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("超时等待: {label}");
}

#[tokio::test(flavor = "multi_thread")]
async fn reservation_rebuilds_after_helper_restart() {
    // === 启动 helper（LAN Helper 角色）与业务节点 B ===
    let helper_key = Keypair::generate_ed25519();
    let helper = start_helper(helper_key.clone());
    let helper_addr = helper_listen_addr(&helper).await;

    let b_key = Keypair::generate_ed25519();
    let b_id = PeerId::from_public_key(&b_key.public());
    let (b_client, mut b_receiver, _dc) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(b_key, client_node_config())
            .expect("start B");

    let network_config = NetworkRuntimeConfig::default();
    let candidates = create_candidate_manager(&network_config);
    let manager = NetManager::new(b_client, b_id, Vec::new(), (), network_config, candidates);

    let bus: Arc<dyn EventBus> = Arc::new(NoopBus);
    let shared = manager.shared_refs();

    // 与 run_event_loop 等价：infra 收敛任务 + 事件泵
    tokio::spawn(shared.infra.clone().run(shared.clone()));
    {
        let shared = shared.clone();
        let bus = bus.clone();
        tokio::spawn(async move {
            while let Some(event) = b_receiver.recv().await {
                let _ = handle_core_node_event(&shared, &event, bus.as_ref()).await;
            }
        });
    }

    // 把 helper 注册为候选（生产环境由 mDNS 发现完成）；
    // 连接/reservation 的建立全部交给 infra 收敛
    shared.candidates.write().unwrap().upsert(
        helper.peer_id,
        vec![helper_addr],
        BootstrapCandidateSource::MdnsLanHelper,
        CandidateRoles::kad_and_relay(),
        CandidateScope::Lan,
    );

    poll_until(
        || manager.get_network_status().relay_ready,
        Duration::from_secs(15),
        "infra 收敛应自动建立 reservation",
    )
    .await;

    // === 杀掉 helper：reservation 丢失必须反映到状态 ===
    drop(helper.client);
    drop(helper.events);

    poll_until(
        || !manager.get_network_status().relay_ready,
        Duration::from_secs(20),
        "helper 死亡后 reservation 丢失应可见",
    )
    .await;

    // === helper 重启（同 PeerId、新端口）：地址由 mDNS 重发现更新（此处模拟）===
    let helper2 = start_helper(helper_key);
    assert_eq!(helper2.peer_id, helper.peer_id, "同 keypair 必须同 PeerId");
    let helper2_addr = helper_listen_addr(&helper2).await;
    shared.candidates.write().unwrap().upsert(
        helper2.peer_id,
        vec![helper2_addr],
        BootstrapCandidateSource::MdnsLanHelper,
        CandidateRoles::kad_and_relay(),
        CandidateScope::Lan,
    );

    // 无需任何重启/手工干预：收敛层自动重建 reservation
    poll_until(
        || manager.get_network_status().relay_ready,
        Duration::from_secs(30),
        "helper 重启后 reservation 应自动重建",
    )
    .await;

    manager.cancel_background_tasks();
}
