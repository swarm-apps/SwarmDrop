//! infra 收敛端到端：relay reservation 断线后自动重建。
//!
//! 场景对应用户拓扑的核心痛点——LAN Helper（手机）挂起/重启后，
//! 依赖它的设备必须在不重启节点的前提下自动恢复跨网可达。

use std::sync::Arc;
use std::time::Duration;

use swarmdrop_core::AppResult;
use swarmdrop_core::device::OsInfo;
use swarmdrop_core::host::{CoreEvent, EventBus, MemoryHost};
use swarmdrop_core::infra::RelayLinkState;
use swarmdrop_core::network::candidates::{BootstrapCandidateSource, CandidateRoles};
use swarmdrop_core::network::config::create_candidate_manager;
use swarmdrop_core::network::event_loop::handle_core_node_event;
use swarmdrop_core::network::{DiscoveryMode, NetManager, NetworkRuntimeConfig};
use swarmdrop_net::{Addr, DhtConfig, Endpoint, RelayServerConfig, SecretKey};

/// 已配对设备列表的持久化端口替身（本用例只关心 presence/infra，列表恒为空）。
fn memory_host() -> MemoryHost {
    MemoryHost::new()
}

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
        OsInfo::default(),
        Vec::new(),
        (),
        network_config,
        candidates,
        bus.clone(),
        None,
        std::sync::Arc::new(swarmdrop_invite::NoopInviteStore),
        Arc::new(memory_host()),
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
    );

    poll_until(
        || manager.get_network_status().relay_ready,
        Duration::from_secs(15),
        "infra 收敛应自动建立 reservation",
    )
    .await;

    // 逐条读模型与聚合位必须同源：`relay_ready` 现在就是从 `infra_links` 派生的，
    // 两者不一致说明 `build_infra_links` 与 `active_relay_peers` 读到了不同的世界。
    {
        let status = manager.get_network_status();
        let link = status
            .infra_links
            .iter()
            .find(|l| l.peer_id == helper_id)
            .expect("helper 应在 infra_links 中");
        assert!(
            matches!(link.relay, Some(RelayLinkState::Active { .. })),
            "relay_ready 为真时该 link 必须是 Active，实际 {:?}",
            link.relay
        );
        assert!(link.ever_active, "建立过 reservation 后 ever_active 应置位");
        assert!(
            link.excluded.is_none(),
            "具备 relay 角色且未被闸门拦下时不应有 excluded"
        );
    }

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

/// 启动路径的角色降级不能连带把 DHT 种子也关掉。
///
/// `runtime.rs` 的启动注册从 `InfraRoles::bootstrap()`（kad + relay）降为
/// `{ kad_server: true, relay: false }`，堵的是「关掉公网可达性的用户照样在启动时建了
/// 公网 reservation」这个绕过闸门的漏洞。但两个开关是**正交**的——用户以为关掉的是
/// 「别让我被动可达」，不该连带关掉「跨网还能不能找到人」。
///
/// 所以这条钉死降级后**仍然会拨号并连上**：kad 路由表拿得到种子，`dht.bootstrap()`
/// 与在线记录发布不受影响。反向的那一半（Public 范围候选在闸门关闭时不建 reservation）
/// 由 `infra::supervisor` 的 `public_reachability_off_skips_public_candidates` 覆盖——
/// 那半边在这里表达不了：集成测试只有 loopback 地址可用，而 `CandidateScope::infer`
/// 见到 loopback 一律判 `Lan`，构造不出 `Public` 范围的真实连接。
#[tokio::test(flavor = "multi_thread")]
async fn kad_only_registration_still_dials_the_bootstrap_node() {
    use swarmdrop_net::{InfraRoles, NodeAddr};

    let seed_secret = SecretKey::generate();
    let seed_id = seed_secret.node_id();
    let seed = helper_endpoint(seed_secret).await;
    let seed_addr = listen_addr(&seed).await;

    let client = client_endpoint(SecretKey::generate()).await;

    client
        .add_infrastructure_peer(
            NodeAddr {
                id: seed_id,
                addrs: vec![seed_addr],
            },
            InfraRoles {
                kad_server: true,
                relay: false,
            },
        )
        .await
        .expect("kad-only 注册不应报错");

    poll_until(
        || client.is_connected(seed_id),
        Duration::from_secs(15),
        "relay=false 的基础设施注册仍应拨号并连上（DHT 种子）",
    )
    .await;

    // 而 relay 轨道确实没有被建立——降级是真降级，不是换个写法的 bootstrap()
    assert!(
        !client.watch_relays().get().contains_key(&seed_id),
        "relay=false 不得写 relays map"
    );

    client.close().await;
    seed.close().await;
}

/// `networkStatusChanged` 必须在 relay 进入 `Connecting` / `Failed` 时被推出去。
///
/// 这是本轮那三行补丁的验收：`run_event_loop` 此前只订了 `addrs` 与 `nat` 两条 watch，
/// 而这两个态**没有任何 NetEvent 对应物**（边沿轨只有 Accepted / Lost 两个）。少了
/// `relays_watcher`，原生端就永远看不到「正在连接」，也永远拿不到 `last_error`——
/// 那正是「桌面和移动端不知道哪条引导节点报了什么错」的根因。
///
/// 用一个拨不通的地址触发：内核先翻 `Connecting`，拨号候选耗尽后翻 `Failed`。
#[tokio::test(flavor = "multi_thread")]
async fn relay_state_transitions_are_pushed_to_the_host() {
    use std::sync::Mutex;
    use swarmdrop_core::infra::InfraLink;
    use swarmdrop_core::network::event_loop::run_event_loop;

    /// 只留 `NetworkStatus` 的 infra_links 快照——本用例只关心「推没推」和「推了什么」。
    #[derive(Default)]
    struct RecordingBus(Mutex<Vec<Vec<InfraLink>>>);

    #[async_trait::async_trait]
    impl EventBus for RecordingBus {
        async fn publish(&self, event: CoreEvent) -> AppResult<()> {
            if let CoreEvent::NetworkStatusChanged { status } = event {
                self.0.lock().unwrap().push(status.infra_links);
            }
            Ok(())
        }
    }

    let client = client_endpoint(SecretKey::generate()).await;
    let events = client.subscribe().await.expect("subscribe");

    let network_config = NetworkRuntimeConfig {
        discovery_mode: DiscoveryMode::LanOnly,
        ..NetworkRuntimeConfig::default()
    };
    let candidates = create_candidate_manager(&network_config);
    let bus = Arc::new(RecordingBus::default());
    let manager = NetManager::new(
        client,
        OsInfo::default(),
        Vec::new(),
        // `()` = 没有传输层：本用例只驱动 relay 轨道
        (),
        network_config,
        candidates,
        bus.clone() as Arc<dyn EventBus>,
        None,
        std::sync::Arc::new(swarmdrop_invite::NoopInviteStore),
        Arc::new(memory_host()),
    );
    let shared = manager.shared_refs();
    tokio::spawn(run_event_loop(
        events,
        shared.clone(),
        bus.clone() as Arc<dyn EventBus>,
    ));

    // TEST-NET-1（RFC 5737）+ 一个丢弃端口：保证拨不通，且不会打到任何真实主机。
    let dead: Addr =
        "/ip4/192.0.2.1/tcp/9/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
            .parse()
            .unwrap();
    let dead_id = dead.p2p_node_id().expect("fixture 带 /p2p/");
    shared.candidates.write().unwrap().upsert(
        dead_id,
        vec![dead],
        BootstrapCandidateSource::HostConfigured,
        CandidateRoles::kad_and_relay(),
    );

    // 只看「推送里出现过非 None 的 relay 轨道」——具体停在 Connecting 还是 Failed
    // 取决于拨号超时，钉死其中之一会让这条测试变成计时器竞赛。
    poll_until(
        || {
            bus.0.lock().unwrap().iter().any(|links| {
                links
                    .iter()
                    .any(|l| l.peer_id == dead_id && l.relay.is_some())
            })
        },
        Duration::from_secs(20),
        "relay 状态变化应触发 NetworkStatusChanged 推送",
    )
    .await;

    manager.cancel_background_tasks();
}
