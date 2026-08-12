//! 改名端到端测试：A 改名 → **保持连接的** B 在秒级内看到新名字。
//!
//! 钉的是这条链路（今天零覆盖，断了是**静默失效**：用户改名、对方永远看不到、
//! 没有任何报错）：
//!
//! ```text
//! rename_device → PairingManager::set_device_name → Endpoint::set_agent_version
//!   → identify 逐连接下发 + push → 对端 NetEvent::PeerIdentified
//!   → refresh_paired_device_from_identify → CoreEvent::PairedDeviceAdded
//! ```
//!
//! 用 `NetManager<()>`（不带传输运行时）：改名只经 identify，不碰传输域，省掉
//! sqlite / TransferManager / Router 那一整套 harness。

use std::sync::Arc;
use std::time::Duration;

use swarmdrop_core::device::{DeviceName, OsInfo, PairedDeviceInfo};
use swarmdrop_core::device_name::rename_device;
use swarmdrop_core::host::{CoreEvent, DeviceConfig, EventBus, MemoryHost};
use swarmdrop_core::network::config::create_candidate_manager;
use swarmdrop_core::network::event_loop::handle_core_node_event;
use swarmdrop_core::network::{NetManager, NetworkRuntimeConfig};
use swarmdrop_core::pairing::PairingPorts;
use swarmdrop_net::{Addr, DhtConfig, Endpoint, NodeAddr, NodeId, SecretKey};

/// A 的主机名。**必须与测试里用的设备名不同**——`to_agent_version()` 在
/// `name == hostname` 时不写 `name=` 槽位，同名会让「改名」在 wire 上无迹可寻。
const HOSTNAME_A: &str = "swarmdrop-rename-a";
const HOSTNAME_B: &str = "swarmdrop-rename-b";
const NEW_NAME: &str = "小李的 MacBook";

/// 平台字段全部写死：`OsInfo::from_agent_version` 要求 host/os/platform/arch 齐备
/// 才返回 `Some`，缺一条对端就整条丢弃、测试会以「没收到事件」的形式误报。
fn os_info(hostname: &str) -> OsInfo {
    OsInfo {
        name: None,
        hostname: hostname.to_string(),
        os: "test-os".to_string(),
        platform: "test-platform".to_string(),
        arch: "test-arch".to_string(),
        capabilities: Vec::new(),
    }
}

/// 关 mDNS（两个本机节点不能靠 mDNS 互相串扰，连接一律显式 `add_addrs` + `connect`）。
async fn test_endpoint(secret: SecretKey, agent_version: String) -> Endpoint {
    Endpoint::builder()
        .secret_key(secret)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version(agent_version)
        .listen(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
        .dht(DhtConfig {
            server_mode: true,
            ..DhtConfig::default()
        })
        .mdns(false)
        .relay_client(false)
        .bind()
        .await
        .expect("bind test endpoint")
}

struct TestNode {
    peer_id: NodeId,
    manager: NetManager<()>,
    /// 既当 `EventBus`（断言 `CoreEvent`）又当 `DeviceConfig`（断言落盘）。
    host: MemoryHost,
    pump: tokio::task::JoinHandle<()>,
}

/// 复刻 `runtime::start_node` 里与本用例相关的那一段：endpoint（agent_version 由
/// 同一份 `OsInfo` 算出）+ NetManager + 网络事件泵。
async fn spawn_node(secret: SecretKey, os_info: OsInfo, paired: Vec<PairedDeviceInfo>) -> TestNode {
    let peer_id = secret.node_id();
    let endpoint = test_endpoint(secret, os_info.to_agent_version()).await;
    let mut events = endpoint.subscribe().await.expect("subscribe");

    let host = MemoryHost::new();
    let bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let network_config = NetworkRuntimeConfig::default();
    let candidates = create_candidate_manager(&network_config);
    let manager = NetManager::new(
        endpoint,
        os_info,
        paired,
        (),
        network_config,
        candidates,
        PairingPorts {
            event_bus: bus.clone(),
            notifier: None,
            invite_store: Arc::new(swarmdrop_invite::NoopInviteStore),
            paired_store: Arc::new(host.clone()),
        },
    );

    let shared = manager.shared_refs();
    let pump = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            handle_core_node_event(&shared, &event, bus.as_ref()).await;
        }
    });

    TestNode {
        peer_id,
        manager,
        host,
        pump,
    }
}

async fn poll_until<F: FnMut() -> bool>(mut pred: F, timeout: Duration, label: &str) {
    let steps = (timeout.as_millis() / 50).max(1);
    for _ in 0..steps {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("超时等待: {label}");
}

async fn wait_listen_addrs(node: &TestNode) -> Vec<Addr> {
    let mut addrs = Vec::new();
    poll_until(
        || {
            addrs = node.manager.endpoint().watch_addrs().get().listen;
            !addrs.is_empty()
        },
        Duration::from_secs(10),
        "节点开始监听",
    )
    .await;
    addrs
}

async fn connect(a: &TestNode, b: &TestNode) {
    let addrs_b = wait_listen_addrs(b).await;
    a.manager
        .endpoint()
        .add_addrs(b.peer_id, addrs_b)
        .await
        .expect("A 注册 B 地址");

    for _ in 0..100 {
        if a.manager.devices().is_connected(&b.peer_id)
            && b.manager.devices().is_connected(&a.peer_id)
        {
            return;
        }
        let _ = a.manager.endpoint().connect(NodeAddr::new(b.peer_id)).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("两节点未能在超时内建连");
}

/// `observer` 最近一次从 identify 刷出来的 `peer` 设备信息。
///
/// 取**最后一条**而非「存在一条」：改名会连发多条 `PairedDeviceAdded`，
/// 只有末条代表当前认知。
fn latest_refreshed(observer: &TestNode, peer: &NodeId) -> Option<OsInfo> {
    observer
        .host
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            CoreEvent::PairedDeviceAdded { device } if &device.peer_id == peer => {
                Some(device.os_info)
            }
            _ => None,
        })
}

/// 核心验收：A、B 已配对且**保持连接**，A 改名后 B 秒级内看到新名字，连接不断。
///
/// 第二段（改回等于 hostname）单独钉一次：`to_agent_version()` 在 `name == hostname`
/// 时不写 `name=` 槽位，wire 上的表现是「名字槽位消失」而不是「换了个值」，最容易被
/// 当成「没变化」而漏掉。
#[tokio::test(flavor = "multi_thread")]
async fn rename_propagates_to_connected_peer_without_restart() {
    let secret_a = SecretKey::generate();
    let secret_b = SecretKey::generate();
    let peer_a = secret_a.node_id();
    let peer_b = secret_b.node_id();

    // 双方预置为已配对（生产里由配对流程写入；本用例只测改名传播）。
    // A 侧的占位 OsInfo 用 `unknown_from_peer_id`，与配对成功后的真实形态一致。
    let node_a = spawn_node(
        secret_a,
        os_info(HOSTNAME_A),
        vec![PairedDeviceInfo::new(
            peer_b,
            OsInfo::unknown_from_peer_id(&peer_b),
            0,
        )],
    )
    .await;
    let node_b = spawn_node(
        secret_b,
        os_info(HOSTNAME_B),
        vec![PairedDeviceInfo::new(
            peer_a,
            OsInfo::unknown_from_peer_id(&peer_a),
            0,
        )],
    )
    .await;

    connect(&node_a, &node_b).await;

    // 首次 identify：B 认得 A 的 hostname，此时还没有用户设的名字。
    poll_until(
        || {
            latest_refreshed(&node_b, &peer_a)
                .is_some_and(|info| info.hostname == HOSTNAME_A && info.name.is_none())
        },
        Duration::from_secs(15),
        "B 完成对 A 的首次 identify",
    )
    .await;

    // === 第一段：设一个与 hostname 不同的名字 ===
    let new_name = DeviceName::parse(NEW_NAME).expect("非空");
    rename_device(
        Some(new_name),
        &node_a.host,
        &node_a.host,
        Some(&node_a.manager),
    )
    .await
    .expect("改名必须成功");

    poll_until(
        || {
            latest_refreshed(&node_b, &peer_a)
                .is_some_and(|info| info.name.as_deref() == Some(NEW_NAME))
        },
        Duration::from_secs(15),
        "B 看到 A 的新设备名",
    )
    .await;

    // 本 change 的价值证明：名字变了，连接没断（重启方案做不到这一点）。
    assert!(
        node_a.manager.endpoint().is_connected(peer_b),
        "改名不得断连——重启节点正是本 change 要消灭的东西"
    );

    // 本机侧：落盘 + 事件。
    assert_eq!(
        node_a
            .host
            .load_device_name()
            .await
            .as_ref()
            .map(DeviceName::as_str),
        Some(NEW_NAME),
        "改名必须先落盘，否则下次启动名字会自己回滚"
    );
    assert!(
        node_a.host.events().iter().any(|event| matches!(
            event,
            CoreEvent::DeviceRenamed { name, display_name }
                if name.as_deref() == Some(NEW_NAME) && display_name == NEW_NAME
        )),
        "本机各处 UI 靠 DeviceRenamed 同步"
    );

    // === 第二段：改成与 hostname 相同的值 —— 同样是一次真实变更 ===
    let same_as_hostname = DeviceName::parse(HOSTNAME_A).expect("非空");
    rename_device(
        Some(same_as_hostname),
        &node_a.host,
        &node_a.host,
        Some(&node_a.manager),
    )
    .await
    .expect("改回 hostname 同名必须成功");

    poll_until(
        || {
            latest_refreshed(&node_b, &peer_a).is_some_and(|info| {
                // agent_version 里的 `name=` 槽位消失，对端解析出 name = None，
                // 显示名回退到 hostname —— 正是用户要的结果。
                info.name.is_none() && info.display_name() == HOSTNAME_A
            })
        },
        Duration::from_secs(15),
        "B 看到 A 的名字回落到 hostname",
    )
    .await;

    node_a.pump.abort();
    node_b.pump.abort();
}
