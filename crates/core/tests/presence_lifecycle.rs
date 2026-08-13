//! presence 端到端慢测：验证「闲置不掉线 → 杀端宽限判离线」全链路。
//!
//! 使用真实的默认 idle_connection_timeout（60s），全程约 2 分钟，
//! 默认 `#[ignore]`。本地验证：
//! `cargo test -p swarmdrop-core --test presence_lifecycle -- --ignored`
//!
//! 另含一条**不** `#[ignore]` 的快测 [`unpair_clears_shared_paired_table`]：
//! 它钉的是 presence 撤销的前置条件——解除配对必须把 peer 从 `NetManager` 那份共享
//! paired 表里删掉，否则 `reconcile_whitelist` 的差集永远算不出它。放在本文件是因为
//! 它属于 presence 撤销这条链路的上游，而不是 pairing 的内部行为。

use std::sync::Arc;
use std::time::Duration;

use swarmdrop_core::AppResult;
use swarmdrop_core::device::{DeviceStatus, OsInfo, PairedDeviceInfo};
use swarmdrop_core::device_manager::DeviceFilter;
use swarmdrop_core::host::{CoreEvent, EventBus, MemoryHost, PairedDeviceStore};
use swarmdrop_core::network::config::create_candidate_manager;
use swarmdrop_core::network::event_loop::handle_core_node_event;
use swarmdrop_core::network::{NetManager, NetworkRuntimeConfig};
use swarmdrop_core::pairing::PairingPorts;
use swarmdrop_net::{Addr, DhtConfig, Endpoint, NodeAddr, NodeId, SecretKey};

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

/// 关 mDNS（显式注册地址），其余（含 60s idle / 15s ping）保持生产默认。
async fn test_endpoint(secret: SecretKey) -> Endpoint {
    Endpoint::builder()
        .secret_key(secret)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("swarmdrop/presence-test")
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
    manager: NetManager<()>,
    pump: tokio::task::JoinHandle<()>,
    /// 与 `NetManager` 共享同一份已配对设备持久化端口，供 unpair 用例断言落盘结果。
    host: MemoryHost,
}

async fn spawn_node(secret: SecretKey, paired: Vec<PairedDeviceInfo>) -> TestNode {
    let endpoint = test_endpoint(secret).await;
    let mut events = endpoint.subscribe().await.expect("subscribe");

    let network_config = NetworkRuntimeConfig::default();
    let candidates = create_candidate_manager(&network_config);
    let bus: Arc<dyn EventBus> = Arc::new(NoopBus);
    let host = memory_host();
    let manager = NetManager::new(
        endpoint,
        OsInfo::default(),
        paired,
        (),
        network_config,
        candidates,
        PairingPorts {
            event_bus: bus.clone(),
            notifier: None,
            invite_store: std::sync::Arc::new(swarmdrop_invite::NoopInviteStore),
            paired_store: Arc::new(host.clone()),
        },
    );

    let shared = manager.shared_refs();

    // 与 run_event_loop 等价的网络事件部分：presence 后台任务 + 事件泵
    tokio::spawn(shared.presence.clone().run(shared.clone(), bus.clone()));
    let pump = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            handle_core_node_event(&shared, &event, bus.as_ref()).await;
        }
    });

    TestNode {
        manager,
        pump,
        host,
    }
}

fn paired_status(node: &TestNode, peer: &NodeId) -> Option<DeviceStatus> {
    node.manager
        .devices()
        .get_devices(DeviceFilter::Paired)
        .into_iter()
        .find(|d| &d.peer_id == peer)
        .map(|d| d.status)
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

async fn wait_listen_addrs(node: &TestNode) -> Vec<Addr> {
    for _ in 0..50 {
        let addrs = node.manager.endpoint().watch_addrs().get().listen;
        if !addrs.is_empty() {
            return addrs;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("超时等待监听地址就绪");
}

/// 配对双节点：闲置 75s（> 60s idle）不掉线；杀掉一端后，另一端在
/// 宽限期（15s）+ 传播余量内转为离线且不再震荡。
#[tokio::test(flavor = "multi_thread")]
#[ignore = "慢测(~2min)，本地手动运行验证 presence 全链路"]
async fn paired_devices_survive_idle_then_offline_after_peer_death() {
    let secret_a = SecretKey::generate();
    let secret_b = SecretKey::generate();
    let peer_a = secret_a.node_id();
    let peer_b = secret_b.node_id();

    let a = spawn_node(
        secret_a,
        vec![PairedDeviceInfo::new(peer_b, OsInfo::default(), 0)],
    )
    .await;
    let b = spawn_node(
        secret_b,
        vec![PairedDeviceInfo::new(peer_a, OsInfo::default(), 0)],
    )
    .await;

    // 互相注册地址（生产环境由 mDNS / DHT 记录完成）
    let addrs_a = wait_listen_addrs(&a).await;
    let addrs_b = wait_listen_addrs(&b).await;
    a.manager
        .endpoint()
        .add_addrs(peer_b, addrs_b)
        .await
        .expect("A 注册 B 地址");
    b.manager
        .endpoint()
        .add_addrs(peer_a, addrs_a)
        .await
        .expect("B 注册 A 地址");

    // 建连（生产环境由 mDNS Discovered 或 supervisor 重探触发）
    a.manager
        .endpoint()
        .connect(NodeAddr::new(peer_b))
        .await
        .expect("A connect B");

    poll_until(
        || paired_status(&a, &peer_b) == Some(DeviceStatus::Online),
        Duration::from_secs(15),
        "A 视角 B 在线",
    )
    .await;
    poll_until(
        || paired_status(&b, &peer_a) == Some(DeviceStatus::Online),
        Duration::from_secs(15),
        "B 视角 A 在线",
    )
    .await;

    // === 核心断言 1：闲置 75 秒（远超 60s idle timeout）双方保持在线 ===
    eprintln!("[presence-test] 闲置 75s（默认 idle_connection_timeout=60s）……");
    tokio::time::sleep(Duration::from_secs(75)).await;
    assert_eq!(
        paired_status(&a, &peer_b),
        Some(DeviceStatus::Online),
        "闲置后 A 视角 B 必须仍在线（keep-alive 白名单生效）"
    );
    assert_eq!(
        paired_status(&b, &peer_a),
        Some(DeviceStatus::Online),
        "闲置后 B 视角 A 必须仍在线"
    );
    assert!(
        a.manager.endpoint().is_connected(peer_b),
        "底层连接必须真实存活，而非仅 UI 状态"
    );

    // === 核心断言 2：杀掉 B 后，A 先宽限（在线）再判离线 ===
    eprintln!("[presence-test] 杀掉 B 节点……");
    b.manager.shutdown().await;
    b.pump.abort();
    drop(b);

    // 断连传播后短暂处于 Probing：仍显示在线（防抖语义）
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        paired_status(&a, &peer_b),
        Some(DeviceStatus::Online),
        "宽限期内不得闪离线"
    );

    // 宽限期（15s）耗尽后判离线
    poll_until(
        || paired_status(&a, &peer_b) == Some(DeviceStatus::Offline),
        Duration::from_secs(30),
        "宽限超时后 A 视角 B 离线",
    )
    .await;

    // 稳定性：再等 10s 确认不出现无端的在线/离线震荡
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(
        paired_status(&a, &peer_b),
        Some(DeviceStatus::Offline),
        "对端死亡后必须稳定离线"
    );

    a.manager.cancel_background_tasks();
    a.pump.abort();
}

/// presence 撤销的前置条件：`unpair` 必须把 peer 从 `NetManager` 建的那份共享
/// paired 表里删掉。
///
/// `PresenceSupervisor::reconcile_whitelist` 算的是 `presence − paired` 差集，
/// `paired` 就是这份表（`DeviceManager` / `PeerDirectory` 也读它）。因此撤销能不能发生，
/// 全取决于 unpair 这一侧有没有动内存——单测在 supervisor 里已把「表变了 → presence 收敛」
/// 钉死，这里钉的是它的上游「unpair → 表真的变了」，两条合起来才是完整链路。
///
/// 顺带断言持久化也空了：两者必须同时成立，只成立一个就是 #100 那两种失败态之一
/// （重启复活 / presence 永不撤销）。
#[tokio::test(flavor = "multi_thread")]
async fn unpair_clears_shared_paired_table() {
    let secret_a = SecretKey::generate();
    let peer_b = SecretKey::generate().node_id();
    let device = PairedDeviceInfo::new(peer_b, OsInfo::default(), 0);

    let a = spawn_node(secret_a, vec![device.clone()]).await;
    a.host
        .save_paired_devices(&[device])
        .await
        .expect("seed 持久化列表");
    assert!(a.manager.pairing().is_paired(&peer_b));

    let remaining = a.manager.pairing().unpair(&peer_b).await.expect("unpair");

    assert!(remaining.is_empty(), "返回的是移除后的完整列表");
    assert!(
        !a.manager.pairing().is_paired(&peer_b),
        "共享 paired 表必须不含该 peer——presence 撤销的唯一开关"
    );
    assert!(
        paired_status(&a, &peer_b).is_none(),
        "设备列表读模型与 presence 读同一份表，必须一起收敛"
    );
    assert!(
        a.host.load_paired_devices().await.unwrap().is_empty(),
        "持久化列表必须同时清空，否则重启后设备复活"
    );

    a.manager.cancel_background_tasks();
    a.pump.abort();
}
