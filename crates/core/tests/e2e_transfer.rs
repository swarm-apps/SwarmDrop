//! crates/core 端到端集成测试。
//!
//! 不需要 Tauri / 真机：在纯 `cargo test` 里 spawn 两个**真实**的 `swarm_p2p_core`
//! 节点（关 mDNS + 显式 dial），用现成的 [`MemoryHost`] 当 host adapter、
//! `sqlite::memory:` 当数据库，跑通完整的 offer → transfer → complete 链路。
//!
//! 方案见 `dev-notes/knowledge/rust-backend.md`「crates/core 端到端集成测试」。
//! 这是 redesign-transfer-lifecycle 轮 8 的安全网：状态机 / projection / 接线的
//! 正确性都能在这里自动化验证（中断 = drop event_loop task，重启 = 同 db 重 spawn）。

use std::sync::Arc;
use std::time::Duration;

use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use uuid::Uuid;

use swarm_p2p_core::libp2p::identity::Keypair;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use swarm_p2p_core::NodeConfig;

use entity::{SuspendedReason, TerminalReason, TransferDirection, TransferPhase};

use swarmdrop_core::database::ops;
use swarmdrop_core::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_core::host::{
    CoreAppPaths, CoreEvent, CoreSaveLocation, EventBus, FileAccess, FileSinkId, FileSourceId,
    HostFileMetadata, MemoryHost,
};
use swarmdrop_core::network::event_loop::run_event_loop;
use swarmdrop_core::network::NetManager;
use swarmdrop_core::protocol::{AppRequest, AppResponse, FileInfo};
use swarmdrop_core::transfer::coordinator::{NetworkSignal, TransferCoordinator};
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::HostEnumeratedFile;

// ===== harness =====

/// 一个已启动的测试节点：真实 P2P 节点 + 独立 sqlite::memory + MemoryHost。
struct TestNode {
    peer_id: PeerId,
    /// 保活：持有 client / cancel_token / transfer Arc，drop 后 event_loop 退出。
    manager: NetManager<TransferManager>,
    /// 与 event_loop 共享同一实例，发送 / 接收侧都走它。
    transfer: Arc<TransferManager>,
    /// 断言入口：events() 取已发布 CoreEvent、sink_bytes() 取落盘字节。
    host: MemoryHost,
    db: Arc<DatabaseConnection>,
}

/// 测试用 app paths —— MemoryHost 不碰真实文件系统，随便给个目录即可。
fn test_paths() -> CoreAppPaths {
    let base = std::env::temp_dir();
    CoreAppPaths {
        data_dir: base.clone(),
        cache_dir: base.clone(),
        temp_dir: base.clone(),
        log_dir: base,
    }
}

/// 关 mDNS + 关 relay/dcutr/autonat + 只监听 127.0.0.1 随机端口。
///
/// 关 mDNS 是路径 B 的核心：两个本机节点不能靠 mDNS 自动发现，否则会互相串扰
/// 状态；连接一律走显式 `add_peer_addrs` + `dial`。
fn test_config() -> NodeConfig {
    NodeConfig::new("/swarmdrop/1.0.0", "swarmdrop-e2e")
        .with_mdns(false)
        .with_relay_client(false)
        .with_dcutr(false)
        .with_autonat(false)
        .with_listen_addrs(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
}

/// 建一个钉死单连接的 sqlite::memory 库并跑全部 migration。
///
/// `:memory:` 每条物理连接是各自独立的空库，默认池开/关连接会让 migration 与查询
/// 落到不同连接 → "no such table"。`max/min_connections(1)` 钉成单条常驻连接规避。
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

/// 预置一条已配对设备（is_paired 的唯一运行时依据是 PairingManager 的内存 DashMap）。
fn paired_info(peer_id: PeerId) -> PairedDeviceInfo {
    PairedDeviceInfo {
        peer_id,
        os_info: OsInfo::default(),
        paired_at: 0,
    }
}

/// 复刻 `runtime::start_node` 的 body，但换成 [`test_config`]（关 mDNS）。
///
/// keypair 由 caller 先生成，这样两节点能在 spawn 前互相拿到 peer_id 预置配对。
async fn spawn_node(
    keypair: Keypair,
    host: MemoryHost,
    db: Arc<DatabaseConnection>,
    paired: Vec<PairedDeviceInfo>,
) -> TestNode {
    let peer_id = PeerId::from_public_key(&keypair.public());

    let (client, receiver, _dc) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, test_config())
            .expect("start node");

    // 同一个 MemoryHost 既当 EventBus 又当 FileAccess：clone 共享内部 Arc<Mutex<_>>，
    // 副作用对 `host` 断言句柄可见。
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());

    let transfer = TransferManager::new(client.clone(), event_bus.clone(), db.clone(), file_access);
    let manager = NetManager::new(client, peer_id, paired, transfer);
    let transfer = manager.transfer_arc();

    // event_loop 驱动接收侧协议（IncomingTransferRuntime）+ 回填 listen_addrs。
    tokio::spawn(run_event_loop(
        receiver,
        manager.shared_refs(),
        event_bus,
        None,
    ));

    TestNode {
        peer_id,
        manager,
        transfer,
        host,
        db,
    }
}

/// 轮询直到谓词为真，超时 panic。固定步长，避免依赖墙钟。
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

/// 等节点监听地址就绪（端口 0 由 OS 分配，必须等 NodeEvent::Listening 回填）。
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
        Duration::from_secs(10),
        "节点开始监听",
    )
    .await;
    addr.expect("listen addr ready")
}

/// `from` 显式 dial `to`，等双方都报告已连接。
///
/// 用裸连接信号 `is_connected`（只看 PeerConnected），而非 connected_count——后者还要求
/// identify 把 agent_version 分类成 SwarmDrop 客户端，与连通性无关。
///
/// dial 在并行高负载下（多 runtime + 多组节点同跑）可能瞬时失败，故重试 dial 直到双方
/// 都连上，忽略单次 dial 错误（已连接时再 dial 是廉价 no-op 错误）——连接才是目标。
async fn connect(from: &TestNode, to: &TestNode) {
    let addr = wait_listen_addr(to).await;
    from.manager
        .client()
        .add_peer_addrs(to.peer_id, vec![addr])
        .await
        .expect("add_peer_addrs");

    let connected = |a: &TestNode, b: &TestNode| {
        a.manager.devices().is_connected(&b.peer_id) && b.manager.devices().is_connected(&a.peer_id)
    };
    for _ in 0..150 {
        if connected(from, to) {
            return;
        }
        let _ = from.manager.client().dial(to.peer_id).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("两节点未能在超时内建连");
}

/// 造一对互相已配对、已建连的节点（A=host_a、B=host_b，各自独立 sqlite::memory）。
async fn connected_paired_pair(host_a: MemoryHost, host_b: MemoryHost) -> (TestNode, TestNode) {
    let kp_a = Keypair::generate_ed25519();
    let kp_b = Keypair::generate_ed25519();
    let id_a = PeerId::from_public_key(&kp_a.public());
    let id_b = PeerId::from_public_key(&kp_b.public());

    let node_a = spawn_node(kp_a, host_a, make_db().await, vec![paired_info(id_b)]).await;
    let node_b = spawn_node(kp_b, host_b, make_db().await, vec![paired_info(id_a)]).await;
    connect(&node_a, &node_b).await;
    (node_a, node_b)
}

/// 预置一个 active 接收会话（create_session 直接写 phase=Active），供清理 / 信号测试复用。
async fn seed_active_session(db: &DatabaseConnection, session_id: Uuid) {
    let files = vec![FileInfo {
        file_id: 0,
        name: "a.bin".to_string(),
        relative_path: "a.bin".to_string(),
        size: 1024,
        checksum: "deadbeef".to_string(),
    }];
    ops::create_session(
        db,
        session_id,
        TransferDirection::Receive,
        "peer",
        "peer-name",
        &files,
        1024,
        Some(CoreSaveLocation::Path {
            path: "/recv".to_string(),
        }),
        None,
    )
    .await
    .expect("create_session");
}

/// 节点的 host 是否收到过某个 offer 的 TransferOfferReceived 事件。
fn received_offer(node: &TestNode, session_id: Uuid) -> bool {
    node.host.events().iter().any(|e| {
        matches!(e, CoreEvent::TransferOfferReceived { offer } if offer.session_id == session_id)
    })
}

/// 等某侧 DB 把 session 推进到 Terminal/Completed（双方完成最可靠的可断言事实）。
///
/// DB 查询是 async，不能塞进同步谓词的 `poll_until`（嵌套 runtime 会 panic），
/// 这里直接写原生 async 轮询循环。
async fn wait_completed(db: &DatabaseConnection, session_id: Uuid, who: &str) {
    for _ in 0..400 {
        if let Ok(Some(p)) = ops::get_transfer_projection(db, session_id).await {
            if p.phase == TransferPhase::Terminal
                && p.terminal_reason == Some(TerminalReason::Completed)
            {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("超时等待: {who} 完成 (Terminal/Completed)");
}

// ===== tests =====

/// 连通性 smoke：两个真实节点关 mDNS + 显式 dial 能建连。坐实路径 B 的最小前提。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_two_nodes_connect() {
    let (node_a, node_b) =
        connected_paired_pair(MemoryHost::new(test_paths()), MemoryHost::new(test_paths())).await;

    assert!(node_a.manager.devices().is_connected(&node_b.peer_id));
    assert!(node_b.manager.devices().is_connected(&node_a.peer_id));
}

/// 单文件传输 happy path：A prepare → send_offer → B accept → 拉取落盘 → 双方 Completed。
///
/// 断言三件事：两侧 DB 都到 Terminal/Completed、接收方落盘字节等于源、两侧都发了
/// TransferCompleted 事件。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_single_file_transfer() {
    let data = b"hello swarmdrop end-to-end transfer".to_vec();
    let source_id = FileSourceId("src-0".to_string());
    let meta = HostFileMetadata {
        name: "hello.txt".to_string(),
        relative_path: "hello.txt".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    // 发送方 host 预置源文件；接收方 host 空。
    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    // A: 哈希准备 + 发 Offer。
    let prepared_id = Uuid::new_v4();
    node_a
        .transfer
        .prepare(
            prepared_id,
            vec![HostEnumeratedFile {
                source_id: source_id.clone(),
                name: "hello.txt".to_string(),
                relative_path: "hello.txt".to_string(),
                size: data.len() as u64,
            }],
        )
        .await
        .expect("prepare");

    let StartSendResult { session_id } = node_a
        .transfer
        .send_offer(&prepared_id, &node_b.peer_id.to_string(), "node-a", &[0u32])
        .expect("send_offer");

    // B: 等 Offer 到达，accept 并开始拉取。
    poll_until(
        || received_offer(&node_b, session_id),
        Duration::from_secs(10),
        "B 收到 Offer",
    )
    .await;

    node_b
        .transfer
        .accept_and_start_receive(
            &session_id,
            CoreSaveLocation::Path {
                path: "/recv".to_string(),
            },
        )
        .await
        .expect("accept_and_start_receive");

    // 双方推进到 Terminal/Completed。
    wait_completed(node_a.db.as_ref(), session_id, "发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "接收方").await;

    // 接收方落盘字节 == 源（sink key = relative_path）。
    let sink = FileSinkId("hello.txt".to_string());
    assert_eq!(
        node_b.host.sink_bytes(&sink),
        Some(data.clone()),
        "接收方落盘内容应与源文件一致"
    );

    // 两侧都发了 TransferCompleted。
    let completed = |node: &TestNode| {
        node.host
            .events()
            .iter()
            .any(|e| matches!(e, CoreEvent::TransferCompleted { .. }))
    };
    assert!(completed(&node_a), "发送方应发 TransferCompleted");
    assert!(completed(&node_b), "接收方应发 TransferCompleted");

    // 3.3 对称性：收发两侧都发 Terminal/Completed 的 TransferProjection
    // （接收方此前不发 projection，本次接线 ReceiveSession 持 coordinator 后补齐）。
    let emitted_terminal_projection = |node: &TestNode| {
        node.host.events().iter().any(|e| {
            matches!(
                e,
                CoreEvent::TransferProjection { projection }
                    if projection.session_id == session_id
                        && projection.phase == TransferPhase::Terminal
                        && projection.terminal_reason == Some(TerminalReason::Completed)
            )
        })
    };
    assert!(
        emitted_terminal_projection(&node_a),
        "发送方应发 Terminal/Completed projection"
    );
    assert!(
        emitted_terminal_projection(&node_b),
        "接收方应发 Terminal/Completed projection（3.3 对称性）"
    );
}

/// 轮 4 task 2.5：应用重启的启动清理。
///
/// 上次运行被强杀、留下一个停在传输中的 active 会话；重启时 `cleanup_recoverable_sessions`
/// 应把它统一转为 recoverable suspended(AppRestarted)（而非 paused/failed 混用），并发
/// projection 让前端刷成"可恢复中断"。纯 DB 路径，不需要节点。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_startup_cleanup_active_to_suspended() {
    let db = make_db().await;
    let host = MemoryHost::new(test_paths());
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());

    // 预置一个 active 会话（create_session 直接写 phase=Active）。
    let session_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), session_id).await;

    // 重启清理：active → recoverable suspended(AppRestarted)。
    let coordinator = TransferCoordinator::new(db.clone(), event_bus);
    let converted = coordinator
        .cleanup_recoverable_sessions()
        .await
        .expect("cleanup");
    assert_eq!(converted, 1, "应有 1 个遗留 active 会话被转换");

    let p = ops::get_transfer_projection(db.as_ref(), session_id)
        .await
        .expect("projection query")
        .expect("session exists");
    assert_eq!(p.phase, TransferPhase::Suspended);
    assert_eq!(p.suspended_reason, Some(SuspendedReason::AppRestarted));
    assert!(p.recoverable, "重启遗留会话应可恢复");

    // 清理经 dispatch 发了 projection 事件（前端据此刷新）。
    assert!(
        host.events().iter().any(|e| matches!(
            e,
            CoreEvent::TransferProjection { projection } if projection.session_id == session_id
        )),
        "应发 TransferProjection 事件"
    );

    // 幂等：再跑一次已无 active 会话可转。
    let again = coordinator
        .cleanup_recoverable_sessions()
        .await
        .expect("cleanup again");
    assert_eq!(again, 0, "第二次清理无 active 会话");
}

/// 轮 4 task 3.3：对端 Pause/Cancel 经 `dispatch_network_current` 写"对端"reason，
/// 与本地 pause 的 LocalPaused 区分。这是 handle_pause_impl / handle_cancel_impl 接线的核心
/// 逻辑（跨节点 mid-transfer 取消对小文件有竞态，故直接在 coordinator 层确定性验证）。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_remote_signals_write_remote_reason() {
    let db = make_db().await;
    let host = MemoryHost::new(test_paths());
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(db.clone(), event_bus);

    // 对端暂停：active → suspended/RemotePaused/recoverable。
    let paused_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), paused_id).await;
    coordinator
        .dispatch_network_current(paused_id, NetworkSignal::RemotePaused)
        .await
        .expect("dispatch remote pause")
        .expect("应发生转换");
    let p = ops::get_transfer_projection(db.as_ref(), paused_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.phase, TransferPhase::Suspended);
    assert_eq!(
        p.suspended_reason,
        Some(SuspendedReason::RemotePaused),
        "对端暂停应写 RemotePaused（非 LocalPaused）"
    );
    assert!(p.recoverable);

    // 对端取消：active → terminal/cancelled/不可恢复。
    let cancelled_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), cancelled_id).await;
    coordinator
        .dispatch_network_current(cancelled_id, NetworkSignal::RemoteCancelled)
        .await
        .expect("dispatch remote cancel")
        .expect("应发生转换");
    let c = ops::get_transfer_projection(db.as_ref(), cancelled_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(c.phase, TransferPhase::Terminal);
    assert_eq!(c.terminal_reason, Some(TerminalReason::Cancelled));
    assert!(!c.recoverable);
}

/// 接收方拒绝 Offer：跨节点 reject 路径（确定性，不启动传输）。
///
/// A 发 Offer → B `reject_and_respond` 回 `OfferResult{accepted:false, UserDeclined}`
/// → A 发 `TransferRejected{reason: UserDeclined}`。两侧都不建 DB session。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_receiver_rejects_offer() {
    let data = b"to be rejected".to_vec();
    let source_id = FileSourceId("src-0".to_string());
    let meta = HostFileMetadata {
        name: "x.bin".to_string(),
        relative_path: "x.bin".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    let prepared_id = Uuid::new_v4();
    node_a
        .transfer
        .prepare(
            prepared_id,
            vec![HostEnumeratedFile {
                source_id: source_id.clone(),
                name: "x.bin".to_string(),
                relative_path: "x.bin".to_string(),
                size: data.len() as u64,
            }],
        )
        .await
        .expect("prepare");

    let StartSendResult { session_id } = node_a
        .transfer
        .send_offer(&prepared_id, &node_b.peer_id.to_string(), "node-a", &[0u32])
        .expect("send_offer");

    poll_until(
        || received_offer(&node_b, session_id),
        Duration::from_secs(10),
        "B 收到 Offer",
    )
    .await;

    // B 拒绝。
    node_b
        .transfer
        .reject_and_respond(&session_id)
        .await
        .expect("reject_and_respond");

    // A 应发 TransferRejected（reason 透传 UserDeclined）。
    poll_until(
        || {
            node_a.host.events().iter().any(|e| {
                matches!(
                    e,
                    CoreEvent::TransferRejected { event } if event.session_id == session_id
                )
            })
        },
        Duration::from_secs(10),
        "A 收到 TransferRejected",
    )
    .await;

    // reject 不建 DB session（两侧都查不到投影）。
    assert!(
        ops::get_transfer_projection(node_a.db.as_ref(), session_id)
            .await
            .expect("query a")
            .is_none(),
        "拒绝的 Offer 不应在发送方建 session"
    );
    assert!(
        ops::get_transfer_projection(node_b.db.as_ref(), session_id)
            .await
            .expect("query b")
            .is_none(),
        "拒绝的 Offer 不应在接收方建 session"
    );
}
