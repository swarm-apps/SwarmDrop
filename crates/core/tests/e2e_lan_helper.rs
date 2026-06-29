//! 局域网协助节点端到端测试。
//!
//! 测试启动三个真实 P2P 节点：A/B 为普通节点，C 开启 LAN Helper。A/B 通过
//! 模拟的 mDNS + Identify 事件自动识别 C，并把 C 注册为 Kad/Relay infrastructure
//! peer；随后用 Kad record 验证 A 写、B 读可通过 C 完成。

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use swarm_p2p_core::libp2p::identity::Keypair;
use swarm_p2p_core::libp2p::kad::{Record, RecordKey};
use swarm_p2p_core::libp2p::{Multiaddr, PeerId, StreamProtocol};
use swarm_p2p_core::{LanHelperConfig, NodeConfig, NodeEvent};
use uuid::Uuid;

use swarmdrop_core::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_core::host::{CoreAppPaths, CoreEvent, EventBus, FileAccess, MemoryHost};
use swarmdrop_core::network::config::{NetworkRuntimeConfig, create_candidate_manager};
use swarmdrop_core::network::event_loop::{handle_core_node_event, run_event_loop};
use swarmdrop_core::network::{BootstrapCandidateSource, DiscoveryMode, NetManager};
use swarmdrop_core::protocol::{AppRequest, AppResponse};
use swarmdrop_core::transfer::data_frame::TRANSFER_DATA_PROTOCOL;
use swarmdrop_core::transfer::manager::TransferManager;

const TEST_TIMEOUT: Duration = Duration::from_secs(15);

struct TestNode {
    peer_id: PeerId,
    manager: NetManager<TransferManager>,
    host: MemoryHost,
    _db: Arc<DatabaseConnection>,
}

fn test_paths() -> CoreAppPaths {
    let base = std::env::temp_dir();
    CoreAppPaths {
        data_dir: base.clone(),
        cache_dir: base.clone(),
        temp_dir: base.clone(),
        log_dir: base,
    }
}

async fn make_db() -> Arc<DatabaseConnection> {
    let mut opt = ConnectOptions::new("sqlite::memory:");
    opt.max_connections(1)
        .min_connections(1)
        .sqlx_logging(false);
    let db = Database::connect(opt)
        .await
        .expect("connect sqlite::memory:");
    migration::Migrator::up(&db, None)
        .await
        .expect("run migrations");
    Arc::new(db)
}

fn listen_addr(ip: Ipv4Addr) -> Multiaddr {
    format!("/ip4/{ip}/tcp/0")
        .parse()
        .expect("valid listen addr")
}

fn ordinary_config(ip: Ipv4Addr, agent_version: String) -> NodeConfig {
    NodeConfig::new("/swarmdrop/1.0.0", agent_version)
        .with_listen_addrs(vec![listen_addr(ip)])
        .with_mdns(false)
        .with_relay_client(true)
        .with_dcutr(false)
        .with_autonat(false)
        .with_data_channel_protocols(vec![StreamProtocol::new(TRANSFER_DATA_PROTOCOL)])
        .with_kad_server_mode(false)
        .with_req_resp_timeout(Duration::from_secs(30))
}

fn helper_config(ip: Ipv4Addr, agent_version: String) -> NodeConfig {
    ordinary_config(ip, agent_version)
        .with_kad_server_mode(true)
        .with_lan_helper(LanHelperConfig::default())
}

fn runtime_config(discovery_mode: DiscoveryMode, provide_lan_helper: bool) -> NetworkRuntimeConfig {
    NetworkRuntimeConfig {
        custom_bootstrap_nodes: Vec::new(),
        discovery_mode,
        auto_discover_lan_helpers: true,
        provide_lan_helper,
    }
}

async fn spawn_node(
    keypair: Keypair,
    node_config: NodeConfig,
    network_config: NetworkRuntimeConfig,
) -> TestNode {
    let peer_id = PeerId::from_public_key(&keypair.public());
    let host = MemoryHost::new(test_paths());
    let db = make_db().await;

    let (client, receiver, dc_receiver) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, node_config).expect("start node");
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());
    let transfer = TransferManager::new(
        client.clone(),
        event_bus.clone(),
        db.clone(),
        file_access,
        dc_receiver,
    );
    let candidate_manager = create_candidate_manager(&network_config);
    let manager = NetManager::new(
        client,
        peer_id,
        Vec::<PairedDeviceInfo>::new(),
        transfer,
        network_config,
        candidate_manager,
    );

    tokio::spawn(run_event_loop(
        receiver,
        manager.shared_refs(),
        event_bus,
        None,
    ));

    TestNode {
        peer_id,
        manager,
        host,
        _db: db,
    }
}

async fn poll_until<F: FnMut() -> bool>(mut pred: F, label: &str) {
    let steps = (TEST_TIMEOUT.as_millis() / 50).max(1);
    for _ in 0..steps {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("超时等待: {label}");
}

async fn wait_listen_addr(node: &TestNode) -> Multiaddr {
    let mut addr = None;
    poll_until(
        || {
            addr = node
                .manager
                .get_network_status()
                .listen_addrs
                .into_iter()
                .next();
            addr.is_some()
        },
        "节点开始监听",
    )
    .await;
    addr.expect("listen addr ready")
}

async fn wait_helper_running(helper: &TestNode) {
    poll_until(
        || {
            let status = helper.manager.get_network_status();
            status.local_lan_helper_running
                && status.relay_server_enabled
                && !status.lan_helper_advertised_addrs.is_empty()
        },
        "LAN Helper 启动并公告私有地址",
    )
    .await;
}

async fn inject_lan_helper_discovery(
    node: &TestNode,
    helper: &TestNode,
    helper_addr: Multiaddr,
    helper_agent: String,
) {
    let discovered = NodeEvent::PeersDiscovered {
        peers: vec![(helper.peer_id, helper_addr.clone())],
    };
    handle_core_node_event(&node.manager.shared_refs(), &discovered, &node.host)
        .await
        .expect("handle discovered event");

    let identified = NodeEvent::IdentifyReceived {
        peer_id: helper.peer_id,
        agent_version: helper_agent,
        protocol_version: "/swarmdrop/1.0.0".to_string(),
        listen_addrs: vec![helper_addr],
        protocols: Vec::new(),
    };
    handle_core_node_event(&node.manager.shared_refs(), &identified, &node.host)
        .await
        .expect("handle identify event");
}

async fn wait_registered_with_helper(node: &TestNode, helper: &TestNode) {
    poll_until(
        || {
            let status = node.manager.get_network_status();
            node.manager.devices().is_connected(&helper.peer_id)
                && status.lan_helper_count == 1
                && status
                    .candidate_sources
                    .iter()
                    .any(|s| s.source == BootstrapCandidateSource::MdnsLanHelper)
        },
        "普通节点自动注册 LAN Helper",
    )
    .await;
}

async fn assert_kad_record_roundtrip(node_a: &TestNode, node_b: &TestNode) {
    let key_bytes = format!("/swarmdrop/test/lan-helper/{}", Uuid::new_v4());
    let key = RecordKey::new(&key_bytes);
    let value = b"hello-lan-helper-kad".to_vec();
    let record = Record::new(key.clone(), value.clone());

    tokio::time::timeout(TEST_TIMEOUT, node_a.manager.client().put_record(record))
        .await
        .expect("put_record timed out")
        .expect("put_record failed");

    for _ in 0..80 {
        if let Ok(result) = node_b.manager.client().get_record(key.clone()).await
            && result.record.value == value
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("B 未能通过 LAN Helper 读取 A 写入的 Kad record");
}

fn lan_helper_agent() -> String {
    OsInfo::default()
        .with_capability(OsInfo::LAN_HELPER_CAPABILITY)
        .to_agent_version()
}

fn ordinary_agent() -> String {
    OsInfo::default().to_agent_version()
}

fn routable_private_ipv4() -> Option<Ipv4Addr> {
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            let IpAddr::V4(ip) = iface.ip() else {
                continue;
            };
            if iface.is_oper_up()
                && !iface.is_loopback()
                && !iface.is_link_local()
                && !iface.is_p2p()
                && ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
            {
                return Some(ip);
            }
        }
    }

    // UDP connect 不发送数据包，只让 OS 选择出站本地地址；在常见 NAT/LAN 环境
    // 会得到真实私有网卡 IP。没有私有地址时返回 None，让真实 LAN 集成测试跳过。
    for target in ["192.168.0.1:9", "10.255.255.255:9", "8.8.8.8:80"] {
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).ok()?;
        socket.connect(target).ok()?;
        let IpAddr::V4(ip) = socket.local_addr().ok()?.ip() else {
            continue;
        };
        if ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified() {
            return Some(ip);
        }
    }
    None
}

async fn run_three_node_lan_helper_flow(discovery_mode: DiscoveryMode) {
    // 这两个用例已用 `#[ignore]` 标注，默认不计入测试结果，需手动 `--ignored` 运行。
    // 在 `--ignored` 上下文下若仍无可绑定私有 IPv4，直接 panic 显式失败，而不是静默
    // return 伪装通过——后者会在缺私网 IP 的环境里给出虚假的“覆盖通过”信号。
    let Some(ip) = routable_private_ipv4() else {
        panic!(
            "LAN Helper 真实三节点测试需要可绑定的私有 IPv4 地址，但当前环境未检测到；\
             请在具备局域网网卡的机器上运行（cargo test -- --ignored）"
        );
    };

    let helper_key = Keypair::generate_ed25519();
    let node_a_key = Keypair::generate_ed25519();
    let node_b_key = Keypair::generate_ed25519();

    let helper = spawn_node(
        helper_key,
        helper_config(ip, lan_helper_agent()),
        runtime_config(discovery_mode, true),
    )
    .await;
    let node_a = spawn_node(
        node_a_key,
        ordinary_config(ip, ordinary_agent()),
        runtime_config(discovery_mode, false),
    )
    .await;
    let node_b = spawn_node(
        node_b_key,
        ordinary_config(ip, ordinary_agent()),
        runtime_config(discovery_mode, false),
    )
    .await;

    let helper_addr = wait_listen_addr(&helper).await;
    wait_helper_running(&helper).await;

    inject_lan_helper_discovery(&node_a, &helper, helper_addr.clone(), lan_helper_agent()).await;
    inject_lan_helper_discovery(&node_b, &helper, helper_addr, lan_helper_agent()).await;

    tokio::join!(
        wait_registered_with_helper(&node_a, &helper),
        wait_registered_with_helper(&node_b, &helper),
    );

    assert_kad_record_roundtrip(&node_a, &node_b).await;

    if matches!(discovery_mode, DiscoveryMode::LanOnly) {
        for (label, node) in [("A", &node_a), ("B", &node_b)] {
            let status = node.manager.get_network_status();
            assert!(
                !status
                    .candidate_sources
                    .iter()
                    .any(|s| s.source == BootstrapCandidateSource::BuiltInPublic),
                "LAN Only 模式下节点 {label} 不应加载内置公网候选"
            );
        }
    }

    assert!(
        helper.host.events().iter().any(|event| matches!(
            event,
            CoreEvent::NetworkStatusChanged { status }
                if status.local_lan_helper_running && status.relay_server_enabled
        )),
        "helper 应发布正在提供局域网协助能力的网络状态"
    );
}

// 整个测试体依赖可绑定的私有 IPv4（绑定真实网卡的三节点真实 P2P 流程），CI 默认
// 环境通常拿不到，故 `#[ignore]`：默认不跑、不计入“通过”，需在有局域网的机器上用
// `cargo test -p swarmdrop-core --test e2e_lan_helper -- --ignored` 手动运行。
#[ignore = "需要可绑定的私有 IPv4 / 真实局域网，手动用 --ignored 运行"]
#[tokio::test(flavor = "multi_thread")]
async fn three_node_auto_discovered_lan_helper_supports_kad_roundtrip() {
    run_three_node_lan_helper_flow(DiscoveryMode::Auto).await;
}

#[ignore = "需要可绑定的私有 IPv4 / 真实局域网，手动用 --ignored 运行"]
#[tokio::test(flavor = "multi_thread")]
async fn lan_only_uses_mdns_lan_helper_without_builtin_public_candidates() {
    run_three_node_lan_helper_flow(DiscoveryMode::LanOnly).await;
}
