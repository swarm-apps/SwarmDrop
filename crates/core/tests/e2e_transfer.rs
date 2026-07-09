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
use sea_orm::{
    ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait, IntoActiveModel,
    Set,
};
use uuid::Uuid;

use swarm_p2p_core::NodeConfig;
use swarm_p2p_core::libp2p::identity::Keypair;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId, StreamProtocol};

use entity::{SuspendedReason, TerminalReason, TransferDirection, TransferPhase};

use swarmdrop_core::database::ops;
use swarmdrop_core::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_core::host::{
    CoreAppPaths, CoreEvent, CoreSaveLocation, EventBus, FileAccess, FileSinkId, FileSourceId,
    HostFileMetadata, MemoryHost,
};
use swarmdrop_core::network::NetManager;
use swarmdrop_core::network::config::{NetworkRuntimeConfig, create_candidate_manager};
use swarmdrop_core::network::event_loop::run_event_loop;
use swarmdrop_core::protocol::{
    AppRequest, AppResponse, FileInfo, OfferRejectReason, TransferOrigin,
};
use swarmdrop_core::transfer::coordinator::{
    ActorReport, CoordinatorInput, NetworkSignal, TransferCoordinator, TransferState, UserCommand,
};
use swarmdrop_core::transfer::incoming::IncomingTransferRuntime;
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::wire::data_frame::TRANSFER_DATA_PROTOCOL;
use swarmdrop_core::transfer::{CHUNK_SIZE, HostEnumeratedFile};

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
        .with_data_channel_protocols(vec![StreamProtocol::new(TRANSFER_DATA_PROTOCOL)])
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
    PairedDeviceInfo::new(peer_id, OsInfo::default(), 0)
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

    let (client, receiver, dc_receiver) =
        swarm_p2p_core::start::<AppRequest, AppResponse>(keypair, test_config())
            .expect("start node");

    // 同一个 MemoryHost 既当 EventBus 又当 FileAccess：clone 共享内部 Arc<Mutex<_>>，
    // 副作用对 `host` 断言句柄可见。
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());

    let transfer = TransferManager::new(
        client.clone(),
        event_bus.clone(),
        db.clone(),
        file_access,
        dc_receiver,
    );
    let network_config = NetworkRuntimeConfig::default();
    let candidate_manager = create_candidate_manager(&network_config);
    let manager = NetManager::new(
        client,
        peer_id,
        paired,
        transfer,
        network_config,
        candidate_manager,
    );
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

/// 预置一个 active 接收会话（create_session 直接写 phase=Active），供清理 / 信号 / 断连测试复用。
async fn seed_active_session(db: &DatabaseConnection, session_id: Uuid, peer_id: &str) {
    let files = vec![FileInfo {
        file_id: 0,
        name: "a.bin".to_string(),
        relative_path: "a.bin".to_string(),
        size: 1024,
        checksum: "deadbeef".to_string(),
    }];
    ops::create_session(
        db,
        ops::CreateSessionInput {
            session_id,
            direction: TransferDirection::Receive,
            peer_id,
            peer_name: "peer-name",
            files: &files,
            total_size: 1024,
            save_path: Some(CoreSaveLocation::Path {
                path: "/recv".to_string(),
            }),
            source_paths: None,
            lifecycle: TransferState::active(0),
            policy: None,
            origin: None,
        },
    )
    .await
    .expect("create_session");
}

#[expect(
    clippy::too_many_arguments,
    reason = "测试辅助：构造 suspended 会话需要完整字段"
)]
async fn seed_suspended_session(
    db: &DatabaseConnection,
    session_id: Uuid,
    direction: TransferDirection,
    peer_id: &str,
    peer_name: &str,
    files: &[FileInfo],
    total_size: u64,
    save_path: Option<CoreSaveLocation>,
    source_paths: Option<&[String]>,
) {
    ops::create_session(
        db,
        ops::CreateSessionInput {
            session_id,
            direction,
            peer_id,
            peer_name,
            files,
            total_size,
            save_path,
            source_paths,
            lifecycle: TransferState::active(0),
            policy: None,
            origin: None,
        },
    )
    .await
    .expect("create resume session");
    ops::mark_session_paused(db, session_id)
        .await
        .expect("seed suspended session");
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
        if let Ok(Some(p)) = ops::get_transfer_projection(db, session_id).await
            && p.phase == TransferPhase::Terminal
            && p.terminal_reason == Some(TerminalReason::Completed)
        {
            return;
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
        .send_offer(
            &prepared_id,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0u32],
            TransferOrigin::Human,
        )
        .await
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
    // （接收方此前不发 projection，本次接线 ReceiverActor 持 coordinator 后补齐）。
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

/// MCP 来源的传输完成后，接收端 inbox 应记为 `source_kind = Mcp`。
///
/// 覆盖 origin 全链：发送方以 `Mcp{client}` 发起 → origin 经 wire 序列化到 Offer →
/// 接收方写入接收会话的 `origin` 列 → 传输完成 → `ensure_inbox_item` 由 origin 派生
/// `source_kind = Mcp`。`Human` 路径由其余 e2e 用例（默认 PairedDevice）覆盖。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_mcp_origin_lands_as_mcp_inbox_source_kind() {
    let data = b"agent-delivered report".to_vec();
    let source_id = FileSourceId("src-mcp".to_string());
    let meta = HostFileMetadata {
        name: "report.pdf".to_string(),
        relative_path: "report.pdf".to_string(),
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
                name: "report.pdf".to_string(),
                relative_path: "report.pdf".to_string(),
                size: data.len() as u64,
            }],
        )
        .await
        .expect("prepare");

    // 关键：以 MCP 来源（带客户端名）发起。
    let StartSendResult { session_id } = node_a
        .transfer
        .send_offer(
            &prepared_id,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0u32],
            TransferOrigin::Mcp {
                client: Some("claude-desktop".to_string()),
            },
        )
        .await
        .expect("send_offer");

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

    wait_completed(node_a.db.as_ref(), session_id, "发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "接收方").await;

    // 接收端：完成会话落 inbox，source_kind 应由 origin(mcp) 派生为 Mcp。
    let detail = swarmdrop_core::database::inbox::ensure_inbox_item_for_completed_receive_session(
        node_b.db.as_ref(),
        session_id,
    )
    .await
    .expect("ensure inbox item")
    .expect("inbox item created");
    assert!(
        matches!(detail.item.source_kind, entity::InboxSourceKind::Mcp),
        "MCP 来源传输应在 inbox 记为 Mcp，实际 {:?}",
        detail.item.source_kind
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
    seed_active_session(db.as_ref(), session_id, "peer").await;

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
    seed_active_session(db.as_ref(), paused_id, "peer").await;
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
    seed_active_session(db.as_ref(), cancelled_id, "peer").await;
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

/// 轮 4 task 3.3 收尾：对端断连 → 该 peer 的 active 传输转 recoverable suspended(Interrupted)。
///
/// `handle_peer_disconnected` 是 IncomingTransferRuntime trait 方法，event_loop 在
/// `NodeEvent::PeerDisconnected` 时调它；这里直接调（模拟检测到断连）做确定性验证，
/// 不依赖真实网络断连时序。预置一个无真实传输的 active 会话 → 调 handler → 验状态。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_peer_disconnect_interrupts_active() {
    // 一个真实节点（拿 client 构 TransferManager）+ 一个仅取 PeerId 的"对端"。
    let fake_peer = PeerId::from_public_key(&Keypair::generate_ed25519().public());
    let node = spawn_node(
        Keypair::generate_ed25519(),
        MemoryHost::new(test_paths()),
        make_db().await,
        vec![paired_info(fake_peer)],
    )
    .await;

    // 该 peer 的 active 会话 + 另一个不相干 peer 的会话（验证按 peer 精确过滤）。
    let target = Uuid::new_v4();
    let other = Uuid::new_v4();
    seed_active_session(node.db.as_ref(), target, &fake_peer.to_string()).await;
    seed_active_session(node.db.as_ref(), other, "other-peer").await;

    // 模拟 event_loop 检测到 fake_peer 断连。
    node.transfer.handle_peer_disconnected(fake_peer).await;

    // 目标会话 → suspended/Interrupted/recoverable + 发 projection。
    let p = ops::get_transfer_projection(node.db.as_ref(), target)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.phase, TransferPhase::Suspended);
    assert_eq!(p.suspended_reason, Some(SuspendedReason::Interrupted));
    assert!(p.recoverable, "中断应可恢复");
    assert!(
        node.host.events().iter().any(|e| matches!(
            e,
            CoreEvent::TransferProjection { projection }
                if projection.session_id == target
                    && projection.suspended_reason == Some(SuspendedReason::Interrupted)
        )),
        "应发 Interrupted 的 TransferProjection"
    );

    // 不相干 peer 的会话不受影响，仍 active。
    let o = ops::get_transfer_projection(node.db.as_ref(), other)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o.phase, TransferPhase::Active, "其它 peer 的会话不应被中断");
}

/// 轮 5 task 4.6：接收方点击恢复走新探测式协议。
///
/// 两端预置同一个 suspended session：A 是发送方、B 是接收方。B 调用 `initiate_resume`
/// 后必须先 ResumeProbe 获取 A 的 manifest/epoch，再 ResumeCommit 让 A 重建 SenderActor，
/// 最后 B 重建 ReceiverActor 拉块并完成。这个测试覆盖旧 `ResumeRequest/ResumeOffer`
/// 被移除后的核心用户路径。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_receiver_initiated_resume_probe_commit_completes() {
    let data = b"resume through probe commit".to_vec();
    let checksum = blake3::hash(&data).to_hex().to_string();
    let source_id = FileSourceId("resume-src".to_string());
    let meta = HostFileMetadata {
        name: "resume.txt".to_string(),
        relative_path: "resume.txt".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    let session_id = Uuid::new_v4();
    let files = vec![FileInfo {
        file_id: 0,
        name: "resume.txt".to_string(),
        relative_path: "resume.txt".to_string(),
        size: data.len() as u64,
        checksum,
    }];
    let source_paths = vec![source_id.0.clone()];

    seed_suspended_session(
        node_a.db.as_ref(),
        session_id,
        TransferDirection::Send,
        &node_b.peer_id.to_string(),
        "node-b",
        &files,
        data.len() as u64,
        None,
        Some(&source_paths),
    )
    .await;
    seed_suspended_session(
        node_b.db.as_ref(),
        session_id,
        TransferDirection::Receive,
        &node_a.peer_id.to_string(),
        "node-a",
        &files,
        data.len() as u64,
        Some(CoreSaveLocation::Path {
            path: "/recv".to_string(),
        }),
        None,
    )
    .await;

    let resumed = node_b
        .transfer
        .initiate_resume(session_id)
        .await
        .expect("receiver resume");
    assert_eq!(resumed.transferred_bytes, 0);

    wait_completed(node_a.db.as_ref(), session_id, "恢复发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "恢复接收方").await;

    assert_eq!(
        node_b
            .host
            .sink_bytes(&FileSinkId("resume.txt".to_string())),
        Some(data),
        "恢复后接收方落盘内容应与源文件一致"
    );

    let sender = ops::get_transfer_projection(node_a.db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    let receiver = ops::get_transfer_projection(node_b.db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sender.epoch, 1);
    assert_eq!(receiver.epoch, 1);
    assert!(node_a.host.events().iter().any(|e| matches!(
        e,
        CoreEvent::TransferResumed { event }
            if event.session_id == session_id
                && event.direction == swarmdrop_core::transfer::progress::RuntimeTransferDirection::Send
    )));
}

/// 发送方发起恢复：补齐 `initiate_resume` 的 Send 方向路径 E2E 覆盖（此前零覆盖）。
///
/// A=Send 主动发起（probe → validate → build_fetch_plan_from_report → 先重建本地
/// SenderActor 再 commit → dispatch → spawn 数据面推送），B=Receive 应答侧
/// `handle_resume_commit_impl` 重建 receiver actor 并发 `TransferResumed{Receive}`。
/// 两侧 epoch 升到 1、传输跑完落盘正确。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_sender_initiated_resume_probe_commit_completes() {
    let data = b"resume initiated by sender side".to_vec();
    let checksum = blake3::hash(&data).to_hex().to_string();
    let source_id = FileSourceId("resume-sender-src".to_string());
    let meta = HostFileMetadata {
        name: "resume.txt".to_string(),
        relative_path: "resume.txt".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    let session_id = Uuid::new_v4();
    let files = vec![FileInfo {
        file_id: 0,
        name: "resume.txt".to_string(),
        relative_path: "resume.txt".to_string(),
        size: data.len() as u64,
        checksum,
    }];
    let source_paths = vec![source_id.0.clone()];

    seed_suspended_session(
        node_a.db.as_ref(),
        session_id,
        TransferDirection::Send,
        &node_b.peer_id.to_string(),
        "node-b",
        &files,
        data.len() as u64,
        None,
        Some(&source_paths),
    )
    .await;
    seed_suspended_session(
        node_b.db.as_ref(),
        session_id,
        TransferDirection::Receive,
        &node_a.peer_id.to_string(),
        "node-a",
        &files,
        data.len() as u64,
        Some(CoreSaveLocation::Path {
            path: "/recv".to_string(),
        }),
        None,
    )
    .await;

    let resumed = node_a
        .transfer
        .initiate_resume(session_id)
        .await
        .expect("sender resume");
    assert_eq!(resumed.transferred_bytes, 0);

    wait_completed(node_a.db.as_ref(), session_id, "恢复发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "恢复接收方").await;

    assert_eq!(
        node_b
            .host
            .sink_bytes(&FileSinkId("resume.txt".to_string())),
        Some(data),
        "恢复后接收方落盘内容应与源文件一致"
    );

    let sender = ops::get_transfer_projection(node_a.db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    let receiver = ops::get_transfer_projection(node_b.db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sender.epoch, 1);
    assert_eq!(receiver.epoch, 1);
    // 应答侧（接收方 node_b）发 TransferResumed{Receive}。
    assert!(node_b.host.events().iter().any(|e| matches!(
        e,
        CoreEvent::TransferResumed { event }
            if event.session_id == session_id
                && event.direction == swarmdrop_core::transfer::progress::RuntimeTransferDirection::Receive
    )));
}

/// 接收方拒绝 Offer：跨节点 reject 路径（确定性，不启动传输）。
///
/// A 发 Offer → B `reject_and_respond` 回 `OfferResult{accepted:false, UserDeclined}`
/// → A 发 `TransferRejected{reason: UserDeclined}`。两侧 projection 均进入
/// terminal/rejected，前端不再需要用临时 session 猜测等待/拒绝态。
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
        .send_offer(
            &prepared_id,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0u32],
            TransferOrigin::Human,
        )
        .await
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

    let sender_projection = ops::get_transfer_projection(node_a.db.as_ref(), session_id)
        .await
        .expect("query a")
        .expect("sender projection");
    assert_eq!(sender_projection.phase, TransferPhase::Terminal);
    assert_eq!(
        sender_projection.terminal_reason,
        Some(TerminalReason::Rejected)
    );

    let receiver_projection = ops::get_transfer_projection(node_b.db.as_ref(), session_id)
        .await
        .expect("query b")
        .expect("receiver projection");
    assert_eq!(receiver_projection.phase, TransferPhase::Terminal);
    assert_eq!(
        receiver_projection.terminal_reason,
        Some(TerminalReason::Rejected)
    );
}

/// 多文件 + 多块 + 空文件 happy path：data-channel Ack 移除后的数据面回归网。
///
/// A 一并 Offer 三个文件——多块且末块不满 / 零字节 / 单块——B accept 落盘。源用位置
/// 相关字节模式构造，任何块乱序/错位都会让 finalize 的 blake3 校验失败 → 收不到
/// Completed（wait_completed 超时）。断言三个 sink 落盘与源逐字节一致。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_multichunk_multifile_transfer() {
    let patterned = |n: usize| -> Vec<u8> { (0..n).map(|i| (i % 251) as u8).collect() };
    let specs = [
        ("big.bin", patterned(4 * CHUNK_SIZE + 777)), // 5 块，末块不满
        ("empty.bin", Vec::<u8>::new()),              // 零字节文件
        ("small.bin", patterned(123)),                // 单块小文件
    ];

    let mut host_a = MemoryHost::new(test_paths());
    let mut enumerated = Vec::new();
    for (idx, (name, data)) in specs.iter().enumerate() {
        let sid = FileSourceId(format!("src-{idx}"));
        host_a = host_a.with_source(
            sid.clone(),
            HostFileMetadata {
                name: (*name).to_string(),
                relative_path: (*name).to_string(),
                size: data.len() as u64,
                modified_at: None,
                checksum: None,
                save_dir: None,
            },
            data.clone(),
        );
        enumerated.push(HostEnumeratedFile {
            source_id: sid,
            name: (*name).to_string(),
            relative_path: (*name).to_string(),
            size: data.len() as u64,
        });
    }

    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    let prepared_id = Uuid::new_v4();
    node_a
        .transfer
        .prepare(prepared_id, enumerated)
        .await
        .expect("prepare");
    let StartSendResult { session_id } = node_a
        .transfer
        .send_offer(
            &prepared_id,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0, 1, 2],
            TransferOrigin::Human,
        )
        .await
        .expect("send_offer");

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
        .expect("accept");

    wait_completed(node_a.db.as_ref(), session_id, "发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "接收方").await;

    for (name, data) in specs.iter() {
        assert_eq!(
            node_b
                .host
                .sink_bytes(&FileSinkId((*name).to_string()))
                .as_ref(),
            Some(data),
            "{name} 落盘应逐字节等于源"
        );
    }
}

/// 真实断点续传：接收方已落盘前 2 块 + DB checkpoint 标记前 2 块完成，恢复后只补传
/// 剩余块并校验落盘 == 完整源。覆盖 fetch_plan 跳过已完成前缀 + open_or_create 续写
/// 既有 .part（而非从头重传）。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_resume_with_partial_checkpoint_completes() {
    let total_chunks = 4usize;
    let done_chunks = 2usize;
    let done_bytes = done_chunks * CHUNK_SIZE;
    let data: Vec<u8> = (0..total_chunks * CHUNK_SIZE)
        .map(|i| (i % 251) as u8)
        .collect();
    let checksum = blake3::hash(&data).to_hex().to_string();
    let source_id = FileSourceId("partial-src".to_string());
    let meta = HostFileMetadata {
        name: "partial.bin".to_string(),
        relative_path: "partial.bin".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    let session_id = Uuid::new_v4();
    let files = vec![FileInfo {
        file_id: 0,
        name: "partial.bin".to_string(),
        relative_path: "partial.bin".to_string(),
        size: data.len() as u64,
        checksum: checksum.clone(),
    }];
    let source_paths = vec![source_id.0.clone()];

    seed_suspended_session(
        node_a.db.as_ref(),
        session_id,
        TransferDirection::Send,
        &node_b.peer_id.to_string(),
        "node-b",
        &files,
        data.len() as u64,
        None,
        Some(&source_paths),
    )
    .await;
    seed_suspended_session(
        node_b.db.as_ref(),
        session_id,
        TransferDirection::Receive,
        &node_a.peer_id.to_string(),
        "node-a",
        &files,
        data.len() as u64,
        Some(CoreSaveLocation::Path {
            path: "/recv".to_string(),
        }),
        None,
    )
    .await;

    // 预写前 done_chunks 块到 B 的 sink（模拟上次已落盘的 .part 前缀）。
    let b_fa: Arc<dyn FileAccess> = Arc::new(node_b.host.clone());
    let sink = b_fa
        .create_sink(HostFileMetadata {
            name: "partial.bin".to_string(),
            relative_path: "partial.bin".to_string(),
            size: data.len() as u64,
            modified_at: None,
            checksum: Some(checksum),
            save_dir: Some(CoreSaveLocation::Path {
                path: "/recv".to_string(),
            }),
        })
        .await
        .expect("seed sink");
    b_fa.write_sink_chunk(&sink, 0, data[..done_bytes].to_vec())
        .await
        .expect("seed partial bytes");

    // 设 B 的 DB checkpoint：前 done_chunks 块完成。
    let mut bitmap = vec![0u8; total_chunks.div_ceil(8)];
    for i in 0..done_chunks {
        bitmap[i / 8] |= 1 << (i % 8);
    }
    ops::update_file_checkpoint_ranges(
        node_b.db.as_ref(),
        session_id,
        0,
        bitmap,
        &[(0u64, done_bytes as u64)],
        done_bytes as i64,
    )
    .await
    .expect("seed checkpoint");

    // B 发起恢复：只应补传剩余块。
    let resumed = node_b
        .transfer
        .initiate_resume(session_id)
        .await
        .expect("resume");
    assert_eq!(
        resumed.transferred_bytes, done_bytes as i64,
        "恢复起点应从已完成的 checkpoint 续起"
    );

    wait_completed(node_a.db.as_ref(), session_id, "恢复发送方").await;
    wait_completed(node_b.db.as_ref(), session_id, "恢复接收方").await;

    assert_eq!(
        node_b
            .host
            .sink_bytes(&FileSinkId("partial.bin".to_string())),
        Some(data),
        "断点续传后落盘应等于完整源（保留前缀 + 补传剩余）"
    );
}

/// 过期回收 e2e：8 天前的 recoverable suspended 接收会话 + 其 sink 有遗留字节，
/// 经共享 core 原语 `reap_expired_suspended_receives` + 助手 `cleanup_expired_part_files`
/// 后会话转 terminal/不可恢复、sink 被清。验证两端复用的回收链路在真实 FileAccess 上生效。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_reap_expired_receive_cleans_part() {
    let db = make_db().await;
    let host = MemoryHost::new(test_paths());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());

    let session_id = Uuid::new_v4();
    let files = vec![FileInfo {
        file_id: 0,
        name: "old.bin".to_string(),
        relative_path: "old.bin".to_string(),
        size: 1024,
        checksum: "x".to_string(),
    }];
    seed_suspended_session(
        db.as_ref(),
        session_id,
        TransferDirection::Receive,
        "peer",
        "peer",
        &files,
        1024,
        Some(CoreSaveLocation::Path {
            path: "/recv".to_string(),
        }),
        None,
    )
    .await;

    // 把 updated_at 推到 8 天前（超过 7 天保留期）。
    let mut m = entity::TransferSession::find_by_id(session_id)
        .one(db.as_ref())
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    m.updated_at = Set(ops::now_ms() - 8 * 24 * 60 * 60 * 1000);
    m.update(db.as_ref()).await.expect("backdate updated_at");

    // 造一个有字节的遗留 sink（.part）。
    let sink = file_access
        .create_sink(HostFileMetadata {
            name: "old.bin".to_string(),
            relative_path: "old.bin".to_string(),
            size: 1024,
            modified_at: None,
            checksum: Some("x".to_string()),
            save_dir: Some(CoreSaveLocation::Path {
                path: "/recv".to_string(),
            }),
        })
        .await
        .expect("seed sink");
    file_access
        .write_sink_chunk(&sink, 0, vec![1u8; 512])
        .await
        .expect("seed bytes");
    assert!(host.sink_bytes(&sink).is_some(), "回收前 sink 应存在");

    let reaped = ops::reap_expired_suspended_receives(
        db.as_ref(),
        swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS,
    )
    .await
    .expect("reap");
    assert_eq!(reaped.len(), 1, "应回收 1 个过期接收会话");
    swarmdrop_core::transfer::cleanup_expired_part_files(&file_access, &reaped).await;

    let p = ops::get_transfer_projection(db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.phase, TransferPhase::Terminal);
    assert!(!p.recoverable, "回收后不可恢复");
    assert!(
        host.sink_bytes(&FileSinkId("old.bin".to_string()))
            .is_none(),
        "过期会话的遗留 .part 应被清理"
    );
}

/// 回归：coordinator 驱动的 fatal_error 必须把失败原因持久化到 DB `error_message`。
///
/// 此前 `ActorReport::FatalError(msg)` 在 reduce 丢弃 msg、`apply_transition` 不写
/// `error_message`，导致发送到不可达 peer 等失败在活动详情里没有任何可见原因
/// （DB error_message=NULL）。修复后失败原因应落库。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_fatal_error_persists_message() {
    let db = make_db().await;
    let host = MemoryHost::new(test_paths());
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(db.clone(), event_bus);

    let session_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), session_id, "peer").await;

    coordinator
        .dispatch(
            session_id,
            CoordinatorInput::Actor {
                epoch: 0,
                report: ActorReport::FatalError("发送 Offer 失败: 对端不可达".into()),
            },
        )
        .await
        .expect("dispatch fatal")
        .expect("应发生转换");

    let model = ops::find_session(db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(model.phase, TransferPhase::Terminal);
    assert_eq!(model.terminal_reason, Some(TerminalReason::FatalError));
    assert_eq!(
        model.error_message.as_deref(),
        Some("发送 Offer 失败: 对端不可达"),
        "fatal_error 应把失败原因持久化到 error_message"
    );
}

/// 回归（cleanup 轮1 病根）：终态收口 dispatch 后，已 terminal 的会话在并发完成/取消下不被覆盖。
///
/// 此前完成走 `mark_session_completed` 直写 phase=terminal/completed，绕过 reduce 的
/// `is_terminal` 守卫——对端取消（dispatch terminal/cancelled）与并发的 finish（mark_completed）
/// 会互相覆盖（正是状态机要消灭的 bug）。改走 `dispatch(Actor{Completed})` 后，`is_terminal`
/// 守卫让先到的终态获胜、迟到的被拒绝（reduce 返回 None），与到达顺序无关。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_terminal_irreversible_under_concurrent_complete_cancel() {
    let db = make_db().await;
    let host = MemoryHost::new(test_paths());
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(db.clone(), event_bus);

    // 顺序 A：取消先到 → 迟到的完成被拒，终态保持 cancelled。
    let cancelled_first = Uuid::new_v4();
    seed_active_session(db.as_ref(), cancelled_first, "peer").await;
    coordinator
        .dispatch(cancelled_first, CoordinatorInput::User(UserCommand::Cancel))
        .await
        .expect("dispatch cancel")
        .expect("active→cancelled");
    let late_complete = coordinator
        .dispatch(
            cancelled_first,
            CoordinatorInput::Actor {
                epoch: 0,
                report: ActorReport::Completed,
            },
        )
        .await
        .expect("dispatch late complete");
    assert!(
        late_complete.is_none(),
        "已 cancelled 的会话不应再接受完成转换"
    );
    let m = ops::find_session(db.as_ref(), cancelled_first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.phase, TransferPhase::Terminal);
    assert_eq!(
        m.terminal_reason,
        Some(TerminalReason::Cancelled),
        "cancelled 不应被并发完成覆盖成 completed"
    );

    // 顺序 B：完成先到 → 迟到的取消被拒，终态保持 completed。
    let completed_first = Uuid::new_v4();
    seed_active_session(db.as_ref(), completed_first, "peer").await;
    coordinator
        .dispatch(
            completed_first,
            CoordinatorInput::Actor {
                epoch: 0,
                report: ActorReport::Completed,
            },
        )
        .await
        .expect("dispatch complete")
        .expect("active→completed");
    let late_cancel = coordinator
        .dispatch(completed_first, CoordinatorInput::User(UserCommand::Cancel))
        .await
        .expect("dispatch late cancel");
    assert!(
        late_cancel.is_none(),
        "已 completed 的会话不应再接受取消转换"
    );
    let m = ops::find_session(db.as_ref(), completed_first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.phase, TransferPhase::Terminal);
    assert_eq!(m.terminal_reason, Some(TerminalReason::Completed));
}

/// 全局「暂停接收」：B 暂停后，A 的 Offer 被自动婉拒（reason=ReceivingPaused），
/// B 不收 Offer 事件、不建会话；B 恢复后，新的 Offer 照常到达 B。
/// 这覆盖 pause-receiving spec 的「暂停期间婉拒」与「恢复后正常接收」。
/// （默认未暂停不破坏既有路径，由本文件其余全部 e2e 测试通过即证。）
#[tokio::test(flavor = "multi_thread")]
async fn e2e_paused_offer_declined_then_resumes_on_resume() {
    let data = b"paused payload".to_vec();
    let source_id = FileSourceId("src-0".to_string());
    let meta = HostFileMetadata {
        name: "p.bin".to_string(),
        relative_path: "p.bin".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new(test_paths()).with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new(test_paths())).await;

    // —— 暂停接收 ——
    node_b.transfer.set_receiving_paused(true);
    assert!(node_b.transfer.is_receiving_paused());

    let prepared_id = Uuid::new_v4();
    node_a
        .transfer
        .prepare(
            prepared_id,
            vec![HostEnumeratedFile {
                source_id: source_id.clone(),
                name: "p.bin".to_string(),
                relative_path: "p.bin".to_string(),
                size: data.len() as u64,
            }],
        )
        .await
        .expect("prepare");

    let StartSendResult { session_id } = node_a
        .transfer
        .send_offer(
            &prepared_id,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0u32],
            TransferOrigin::Human,
        )
        .await
        .expect("send_offer");

    // A 应收到 TransferRejected，且 reason 透传 ReceivingPaused。
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
        "A 收到 TransferRejected(ReceivingPaused)",
    )
    .await;

    let rejected_reason = node_a.host.events().iter().find_map(|e| match e {
        CoreEvent::TransferRejected { event } if event.session_id == session_id => {
            Some(event.reason.clone())
        }
        _ => None,
    });
    assert_eq!(
        rejected_reason,
        Some(Some(OfferRejectReason::ReceivingPaused)),
        "暂停期间婉拒的 reason 必须是 ReceivingPaused"
    );

    // B 不应收到 Offer 事件、也不应为该会话建任何 projection（未缓存、未落盘）。
    assert!(
        !received_offer(&node_b, session_id),
        "暂停期间不得向用户弹出 Offer"
    );
    assert!(
        ops::get_transfer_projection(node_b.db.as_ref(), session_id)
            .await
            .expect("query b")
            .is_none(),
        "暂停期间不得为被婉拒的 offer 建会话"
    );

    // —— 恢复接收 ——
    node_b.transfer.set_receiving_paused(false);
    assert!(!node_b.transfer.is_receiving_paused());

    let prepared_id2 = Uuid::new_v4();
    node_a
        .transfer
        .prepare(
            prepared_id2,
            vec![HostEnumeratedFile {
                source_id: source_id.clone(),
                name: "p.bin".to_string(),
                relative_path: "p.bin".to_string(),
                size: data.len() as u64,
            }],
        )
        .await
        .expect("prepare 2");

    let StartSendResult {
        session_id: session_id2,
    } = node_a
        .transfer
        .send_offer(
            &prepared_id2,
            &node_b.peer_id.to_string(),
            "node-a",
            &[0u32],
            TransferOrigin::Human,
        )
        .await
        .expect("send_offer 2");

    // 恢复后，新 Offer 应照常到达 B（要求用户确认 → received_offer 为真）。
    poll_until(
        || received_offer(&node_b, session_id2),
        Duration::from_secs(10),
        "恢复后 B 收到新 Offer",
    )
    .await;
}

/// 回归（僵尸节点治本）：停止节点后 run_event_loop 随 cancel_token 退出，
/// swarm 被释放、连接断开——对端必须在宽限期后判其离线，而不是被
/// keep-alive 白名单钉死的僵尸连接骗成永久在线。
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_node_goes_offline_on_peer() {
    let (node_a, node_b) =
        connected_paired_pair(MemoryHost::new(test_paths()), MemoryHost::new(test_paths())).await;
    let id_b = node_b.peer_id;

    // 双方 presence 就绪：A 视角 B 在线
    poll_until(
        || {
            node_a
                .manager
                .devices()
                .get_devices(swarmdrop_core::device_manager::DeviceFilter::Paired)
                .iter()
                .any(|d| {
                    d.peer_id == id_b
                        && matches!(d.status, swarmdrop_core::device::DeviceStatus::Online)
                })
        },
        Duration::from_secs(15),
        "A 视角 B 在线",
    )
    .await;

    // 停止 B（host 停节点的完整语义：shutdown + drop，对应 guard.take()）
    node_b.manager.shutdown().await;
    drop(node_b);

    // A 应在宽限期（15s）+ 判死/传播余量内看到 B 离线
    poll_until(
        || {
            node_a
                .manager
                .devices()
                .get_devices(swarmdrop_core::device_manager::DeviceFilter::Paired)
                .iter()
                .any(|d| {
                    d.peer_id == id_b
                        && matches!(d.status, swarmdrop_core::device::DeviceStatus::Offline)
                })
        },
        Duration::from_secs(45),
        "停止节点后 A 视角 B 离线",
    )
    .await;
}
