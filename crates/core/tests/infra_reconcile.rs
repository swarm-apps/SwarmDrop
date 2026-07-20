//! infra 收敛端到端：relay reservation 断线后自动重建。
//!
//! 场景对应用户拓扑的核心痛点——LAN Helper（手机）挂起/重启后，
//! 依赖它的设备必须在不重启节点的前提下自动恢复跨网可达。

use std::sync::Arc;
use std::time::Duration;

use swarmdrop_core::AppResult;
use swarmdrop_core::host::{CoreEvent, EventBus};
use swarmdrop_core::network::candidates::{
    BootstrapCandidateSource, CandidateRoles, CandidateScope,
};
use swarmdrop_core::network::config::create_candidate_manager;
use swarmdrop_core::network::event_loop::handle_core_node_event;
use swarmdrop_core::network::{DiscoveryMode, NetManager, NetworkRuntimeConfig};
use swarmdrop_net::{Addr, DhtConfig, Endpoint, RelayServerConfig, SecretKey};

struct NoopBus;

#[async_trait::async_trait]
impl EventBus for NoopBus {
    async fn publish(&self, _event: CoreEvent) -> AppResult<()> {
        Ok(())
    }
}

/// 业务节点（relay client + DHT）。
async fn client_endpoint(secret: SecretKey) -> Endpoint {
    Endpoint::builder()
        .secret_key(secret)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("swarmdrop/infra-test")
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .dht(DhtConfig::default())
        .mdns(false)
        .relay_client(true)
        .bind()
        .await
        .expect("bind client endpoint")
}

/// LAN Helper 节点（relay server + DHT server_mode）。
async fn helper_endpoint(secret: SecretKey) -> Endpoint {
    Endpoint::builder()
        .secret_key(secret)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("swarmdrop/infra-helper")
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .dht(DhtConfig {
            server_mode: true,
            ..DhtConfig::default()
        })
        .mdns(false)
        .relay_server(RelayServerConfig::default())
        .bind()
        .await
        .expect("bind helper endpoint")
}

async fn listen_addr(endpoint: &Endpoint) -> Addr {
    for _ in 0..50 {
        if let Some(addr) = endpoint.watch_addrs().get().listen.into_iter().next() {
            return addr;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("监听地址未就绪");
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
    let helper_secret = SecretKey::generate();
    let helper_id = helper_secret.node_id();
    let helper = helper_endpoint(helper_secret.clone()).await;
    let helper_addr = listen_addr(&helper).await;

    let b_secret = SecretKey::generate();
    let b_endpoint = client_endpoint(b_secret).await;
    let mut b_events = b_endpoint.subscribe().await.expect("subscribe B");

    // LanOnly：不加载内置公网引导——否则 B 会同时在真实 bootstrap 上建 reservation，
    // 杀掉 helper 后 relay_ready 仍为真，掩盖本用例要验证的 helper 重建收敛。
    let network_config = NetworkRuntimeConfig {
        discovery_mode: DiscoveryMode::LanOnly,
        ..NetworkRuntimeConfig::default()
    };
    let candidates = create_candidate_manager(&network_config);
    let bus: Arc<dyn EventBus> = Arc::new(NoopBus);
    let manager = NetManager::new(
        b_endpoint,
        Vec::new(),
        (),
        network_config,
        candidates,
        bus.clone(),
        None,
    );

    let shared = manager.shared_refs();

    // 与 run_event_loop 等价：infra 收敛任务 + 事件泵
    tokio::spawn(shared.infra.clone().run(shared.clone()));
    {
        let shared = shared.clone();
        let bus = bus.clone();
        tokio::spawn(async move {
            while let Some(event) = b_events.recv().await {
                handle_core_node_event(&shared, &event, bus.as_ref()).await;
            }
        });
    }

    // 把 helper 注册为候选（生产环境由 mDNS 发现完成）；
    // 连接/reservation 的建立全部交给 infra 收敛
    shared.candidates.write().unwrap().upsert(
        helper_id,
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
    helper.close().await;

    poll_until(
        || !manager.get_network_status().relay_ready,
        Duration::from_secs(20),
        "helper 死亡后 reservation 丢失应可见",
    )
    .await;

    // === helper 重启（同 NodeId、新端口）：地址由 mDNS 重发现更新（此处模拟）===
    let helper2 = helper_endpoint(helper_secret).await;
    assert_eq!(helper2.node_id(), helper_id, "同 secret 必须同 NodeId");
    let helper2_addr = listen_addr(&helper2).await;
    shared.candidates.write().unwrap().upsert(
        helper_id,
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
