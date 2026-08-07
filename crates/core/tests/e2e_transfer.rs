//! crates/core 端到端集成测试。
//!
//! 不需要 Tauri / 真机：在纯 `cargo test` 里 spawn 两个**真实**的 `swarmdrop-net`
//! Endpoint（关 mDNS + 显式 connect），用现成的 [`MemoryHost`] 当 host adapter、
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

use swarmdrop_net::{Addr, DhtConfig, Endpoint, NodeAddr, NodeId, Router, SecretKey};

use entity::{SuspendedReason, TerminalReason, TransferDirection, TransferPhase};

use swarmdrop_core::device::{OsInfo, PairedDeviceInfo};
use swarmdrop_core::event_adapter::CoreTransferEvents;
use swarmdrop_core::host::{
    CoreEvent, CoreSaveLocation, EventBus, FileAccess, FileSinkId, FileSourceId, HostFileMetadata,
    MemoryHost,
};
use swarmdrop_core::network::NetManager;
use swarmdrop_core::network::config::{NetworkRuntimeConfig, create_candidate_manager};
use swarmdrop_core::network::event_loop::run_event_loop;
use swarmdrop_core::protocol::{
    FileInfo, OfferRejectReason, TRANSFER_DATA_PROTOCOL, TRANSFER_DATA_PROTOCOL_V2, TransferOrigin,
};
use swarmdrop_core::runtime::build_router;
use swarmdrop_core::transfer::coordinator::{
    ActorReport, CoordinatorInput, NetworkSignal, TransferCoordinator, TransferState, UserCommand,
};
use swarmdrop_core::transfer::failure::FailureCode;
use swarmdrop_core::transfer::incoming::IncomingTransferRuntime;
use swarmdrop_core::transfer::manager::{StartSendResult, TransferManager};
use swarmdrop_core::transfer::store::{CreateSessionInput, InboxStore, SessionStore};
use swarmdrop_core::transfer::{CHUNK_SIZE, HostEnumeratedFile};
use swarmdrop_storage_sql::{SqlSessionStore, ops};

// ===== harness =====

/// 一个已启动的测试节点：真实 P2P 节点 + 独立 sqlite::memory + MemoryHost。
struct TestNode {
    peer_id: NodeId,
    /// 保活：持有 endpoint / cancel_token / transfer Arc，drop 后 event_loop 退出。
    manager: NetManager<TransferManager>,
    /// 与 event_loop 共享同一实例，发送 / 接收侧都走它。
    transfer: Arc<TransferManager>,
    /// 断言入口：events() 取已发布 CoreEvent、sink_bytes() 取落盘字节。
    host: MemoryHost,
    db: Arc<DatabaseConnection>,
    /// 保活：drop 后入站流路由停止。
    _router: Router,
}

/// 关 mDNS + 只监听 127.0.0.1 随机端口的测试 Endpoint（开 DHT server 供在线记录）。
///
/// 关 mDNS 是路径 B 的核心：两个本机节点不能靠 mDNS 自动发现，否则会互相串扰
/// 状态；连接一律走显式 `add_addrs` + `connect`。
async fn test_endpoint(secret: SecretKey) -> Endpoint {
    Endpoint::builder()
        .secret_key(secret)
        .identify_protocol("/swarmdrop/2.0.0")
        .agent_version("swarmdrop/e2e")
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
fn paired_info(peer_id: NodeId) -> PairedDeviceInfo {
    PairedDeviceInfo::new(peer_id, OsInfo::default(), 0)
}

/// 复刻 `runtime::start_node` 的 body，但换成 [`test_endpoint`]（关 mDNS、无引导）。
///
/// secret 由 caller 先生成，这样两节点能在 spawn 前互相拿到 node_id 预置配对。
async fn spawn_node(
    secret: SecretKey,
    host: MemoryHost,
    db: Arc<DatabaseConnection>,
    paired: Vec<PairedDeviceInfo>,
) -> TestNode {
    let peer_id = secret.node_id();
    let endpoint = test_endpoint(secret).await;
    let events = endpoint.subscribe().await.expect("subscribe");

    // 同一个 MemoryHost 既当 EventBus 又当 FileAccess：clone 共享内部 Arc<Mutex<_>>，
    // 副作用对 `host` 断言句柄可见。
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());

    let transfer = TransferManager::new(
        endpoint.clone(),
        Arc::new(CoreTransferEvents(event_bus.clone())),
        Arc::new(SqlSessionStore::new(db.clone())),
        file_access,
    );
    let network_config = NetworkRuntimeConfig::default();
    let candidate_manager = create_candidate_manager(&network_config);
    let manager = NetManager::new(
        endpoint.clone(),
        OsInfo::default(),
        paired,
        transfer,
        network_config,
        candidate_manager,
        event_bus.clone(),
        None,
        std::sync::Arc::new(swarmdrop_invite::NoopInviteStore),
        Arc::new(host.clone()),
    );
    let transfer = manager.transfer_arc();

    // Router：三协议入站路由，复用 runtime 的装配（避免协议注册漂移）。
    let router = build_router(&endpoint, manager.pairing_arc(), transfer.clone(), None);

    // event_loop 驱动 devices/presence/infra + 回填 listen_addrs。
    tokio::spawn(run_event_loop(events, manager.shared_refs(), event_bus));

    TestNode {
        peer_id,
        manager,
        transfer,
        host,
        db,
        _router: router,
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

/// 等节点监听地址就绪（端口 0 由 OS 分配，必须等 watch_addrs 回填）。
async fn wait_listen_addr(node: &TestNode) -> Addr {
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
        .endpoint()
        .add_addrs(to.peer_id, vec![addr])
        .await
        .expect("add_addrs");

    let connected = |a: &TestNode, b: &TestNode| {
        a.manager.devices().is_connected(&b.peer_id) && b.manager.devices().is_connected(&a.peer_id)
    };
    for _ in 0..150 {
        if connected(from, to) {
            return;
        }
        let _ = from
            .manager
            .endpoint()
            .connect(NodeAddr::new(to.peer_id))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("两节点未能在超时内建连");
}

/// 造一对互相已配对、已建连的节点（A=host_a、B=host_b，各自独立 sqlite::memory）。
async fn connected_paired_pair(host_a: MemoryHost, host_b: MemoryHost) -> (TestNode, TestNode) {
    let sk_a = SecretKey::generate();
    let sk_b = SecretKey::generate();
    let id_a = sk_a.node_id();
    let id_b = sk_b.node_id();

    let node_a = spawn_node(sk_a, host_a, make_db().await, vec![paired_info(id_b)]).await;
    let node_b = spawn_node(sk_b, host_b, make_db().await, vec![paired_info(id_a)]).await;
    connect(&node_a, &node_b).await;
    (node_a, node_b)
}

/// 预置一个单文件接收会话，phase 由 `lifecycle` 一次写到位（建会话即目标状态）。
async fn seed_receive_session(
    db: &DatabaseConnection,
    session_id: Uuid,
    peer_id: &str,
    lifecycle: TransferState,
) {
    let files = vec![FileInfo {
        file_id: 0,
        name: "a.bin".to_string(),
        relative_path: "a.bin".to_string(),
        size: 1024,
        checksum: "deadbeef".to_string(),
    }];
    ops::create_session(
        db,
        CreateSessionInput {
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
            lifecycle,
            policy: None,
            origin: None,
        },
    )
    .await
    .expect("create_session");
}

/// active 是最常用的一档（清理 / 信号 / 断连测试都要），单列一层薄壳。
async fn seed_active_session(db: &DatabaseConnection, session_id: Uuid, peer_id: &str) {
    seed_receive_session(db, session_id, peer_id, TransferState::active(0)).await;
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
        CreateSessionInput {
            session_id,
            direction,
            peer_id,
            peer_name,
            files,
            total_size,
            save_path,
            source_paths,
            // 建会话时一次写到 suspended：状态直写的旁路已删，fixture 走 lifecycle 入参。
            lifecycle: TransferState {
                phase: TransferPhase::Suspended,
                suspended_reason: Some(SuspendedReason::LocalPaused),
                terminal_reason: None,
                epoch: 0,
                recoverable: true,
                failure: None,
            },
            policy: None,
            origin: None,
        },
    )
    .await
    .expect("create resume session");
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
    let (node_a, node_b) = connected_paired_pair(MemoryHost::new(), MemoryHost::new()).await;

    assert!(node_a.manager.devices().is_connected(&node_b.peer_id));
    assert!(node_b.manager.devices().is_connected(&node_a.peer_id));
}

/// **旧版数据面协议必须继续被服务**。
///
/// v0.12.0 及更早的客户端只会拨 `/swarmdrop/transfer-data/2`；`/3` 是这次为流控窗口帧新加
/// 的名字（加 tag 必须换名，理由见 `TRANSFER_DATA_PROTOCOL` 的文档）。摘掉 v2 的注册等于
/// 对所有存量客户端断供，而症状只会出现在跨版本的真机之间——CI 里两端永远同版本，别的测试
/// 一条都不会红。
///
/// 判据是 `open` 不返回 `UnsupportedProtocol`：能开出流就说明 Router 认这个名字。开完即丢，
/// 不发 Hello——本条只管协商，传输本身由其余 e2e 覆盖。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_legacy_data_protocol_is_still_served() {
    let (node_a, node_b) = connected_paired_pair(MemoryHost::new(), MemoryHost::new()).await;

    for protocol in [TRANSFER_DATA_PROTOCOL, TRANSFER_DATA_PROTOCOL_V2] {
        node_a
            .manager
            .endpoint()
            .open(node_b.peer_id, protocol.clone())
            .await
            .unwrap_or_else(|e| panic!("{protocol} 应当被服务，却拿到 {e}"));
    }
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
    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

    // 回归锚点：传完之后**两侧**的 transferredBytes 都应等于文件大小。
    //
    // 发送方向曾经恒为 0：projection 的 transferredBytes 是文件级 SUM
    // （`store::projection_of`），而发送侧的进度只活在内存 ProgressTracker 里——接收侧有
    // `persist_chunk` 逐块增量落库，发送侧只在 pause / interrupted 两条终态路径批量落一次，
    // **完成路径漏了**。表现是传完的会话在发送方 UI 上显示「已完成 0 B / 500 MB 0%」，
    // 接收方同一条却是 100%（2026-08-04 Web 端双 origin 实测）。修复是让
    // `SenderActor::on_completed` 与 `on_interrupted` 对称地先落进度再转终态。
    let transferred = |node: &TestNode| {
        node.host
            .events()
            .iter()
            .filter_map(|e| match e {
                CoreEvent::TransferProjection { projection }
                    if projection.session_id == session_id
                        && projection.phase == TransferPhase::Terminal =>
                {
                    Some(projection.transferred_bytes)
                }
                _ => None,
            })
            .next_back()
    };
    assert_eq!(
        transferred(&node_a),
        Some(data.len() as i64),
        "发送方终态 projection 的 transferredBytes 应等于文件大小，而不是 0"
    );
    assert_eq!(
        transferred(&node_b),
        Some(data.len() as i64),
        "接收方终态 projection 的 transferredBytes 应等于文件大小"
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
    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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
    let detail = SqlSessionStore::new(node_b.db.clone())
        .ensure_inbox_item_for_completed_receive_session(session_id)
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
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());

    // 预置一个 active 会话（create_session 直接写 phase=Active）。
    let session_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), session_id, "peer").await;

    // 重启清理：active → recoverable suspended(AppRestarted)。
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(db.clone())),
        Arc::new(CoreTransferEvents(event_bus)),
    );
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

/// D4：「进行中不可删」是域不变量，守卫在 `TransferManager::delete_session`。
///
/// 三端的删除入口（桌面命令 / MCP 工具 / wasm 导出）都只调这一条域方法，所以拦截必须
/// 在这里——UI 的按钮可见性拦不住 MCP 客户端，也拦不住一份陈旧的前端状态。放行 suspended
/// 是刻意的：它没有活 actor，代价只是断点信息一并消失（确认文案已这么写）。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_delete_session_rejects_active_allows_terminal_and_suspended() {
    let db = make_db().await;
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());
    // 守卫只碰 store 与 phase，装一个裸 manager 即可（不需要 router / event_loop）。
    let transfer = TransferManager::new(
        test_endpoint(SecretKey::generate()).await,
        Arc::new(CoreTransferEvents(event_bus)),
        Arc::new(SqlSessionStore::new(db.clone())),
        file_access,
    );

    let active = Uuid::new_v4();
    let suspended = Uuid::new_v4();
    let terminal = Uuid::new_v4();
    seed_active_session(db.as_ref(), active, "peer").await;
    seed_receive_session(
        db.as_ref(),
        suspended,
        "peer",
        TransferState {
            phase: TransferPhase::Suspended,
            suspended_reason: Some(SuspendedReason::LocalPaused),
            terminal_reason: None,
            epoch: 0,
            recoverable: true,
            failure: None,
        },
    )
    .await;
    seed_receive_session(
        db.as_ref(),
        terminal,
        "peer",
        TransferState {
            phase: TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            failure: None,
        },
    )
    .await;

    let err = transfer
        .delete_session(active)
        .await
        .expect_err("进行中的会话不可删");
    assert!(
        err.to_string().contains("取消"),
        "错误应指向「请先取消」而不是一句无从下手的失败：{err}"
    );
    assert!(
        ops::get_transfer_projection(db.as_ref(), active)
            .await
            .unwrap()
            .is_some(),
        "被拒绝的删除不能留下半删状态"
    );

    for (id, label) in [(terminal, "终态"), (suspended, "挂起")] {
        transfer
            .delete_session(id)
            .await
            .unwrap_or_else(|e| panic!("{label}会话应可删: {e}"));
        assert!(
            ops::get_transfer_projection(db.as_ref(), id)
                .await
                .unwrap()
                .is_none(),
            "{label}会话应已删除"
        );
    }
}

/// 回归锚点：**先到的 `Interrupted` 会把后到的 `RemotePaused` 永久挡在门外。**
///
/// `reduce_network` 的两条守卫都要求 `state.is_active()`，所以一旦 Interrupted 先把会话转成
/// suspended，随后到达的 RemotePaused 就不再满足守卫、被静默丢弃——会话永远停在
/// 「连接中断」而不是「对方暂停」。
///
/// 这正是 `pause_send` / `pause_receive` 里 **`notify_pause` 必须早于 cancel actor** 的
/// 全部理由：关闭数据流不携带原因，对端只会当成 Interrupted，而控制帧要走一个 RTT，永远晚
/// 一步。那不是偶发竞态，是确定性的顺序错误（2026-08-04 Web 端双 origin 实测：接收方显示
/// 「连接中断」，console 里只有 `data channel 在完成前关闭`，没有任何暂停通知）。
///
/// 本测试**不主张这条守卫应该放宽**——放宽会让「某些 suspended 可被覆盖」渗进状态机语义。
/// 它锁定的是后果，好让下一个想重排那两个函数的人先看到代价。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_interrupted_first_shuts_out_late_remote_paused() {
    let db = make_db().await;
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(db.clone())),
        Arc::new(CoreTransferEvents(event_bus)),
    );

    let session_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), session_id, "peer").await;

    // 数据流先断（本地立即生效）。
    coordinator
        .dispatch_network_current(session_id, NetworkSignal::Interrupted)
        .await
        .expect("dispatch interrupted")
        .expect("active → suspended 应发生转换");

    // 控制帧一个 RTT 之后才到——此时会话已非 active。
    let transition = coordinator
        .dispatch_network_current(session_id, NetworkSignal::RemotePaused)
        .await
        .expect("dispatch remote paused");
    assert!(
        transition.is_none(),
        "会话已是 suspended，RemotePaused 不应再触发转换"
    );

    let p = ops::get_transfer_projection(db.as_ref(), session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        p.suspended_reason,
        Some(SuspendedReason::Interrupted),
        "迟到的 RemotePaused 覆盖不了 Interrupted —— 所以通知必须发在关流之前"
    );
}

/// 轮 4 task 3.3：对端 Pause/Cancel 经 `dispatch_network_current` 写"对端"reason，
/// 与本地 pause 的 LocalPaused 区分。这是 handle_pause_impl / handle_cancel_impl 接线的核心
/// 逻辑（跨节点 mid-transfer 取消对小文件有竞态，故直接在 coordinator 层确定性验证）。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_remote_signals_write_remote_reason() {
    let db = make_db().await;
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(db.clone())),
        Arc::new(CoreTransferEvents(event_bus)),
    );

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
    // 一个真实节点（拿 endpoint 构 TransferManager）+ 一个仅取 NodeId 的"对端"。
    let fake_peer = SecretKey::generate().node_id();
    let node = spawn_node(
        SecretKey::generate(),
        MemoryHost::new(),
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

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

    let mut host_a = MemoryHost::new();
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

    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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
    let host = MemoryHost::new();
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

    let reaped = SqlSessionStore::new(db.clone())
        .reap_expired_suspended_receives(swarmdrop_core::transfer::SUSPENDED_RECEIVE_RETENTION_SECS)
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
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(db.clone())),
        Arc::new(CoreTransferEvents(event_bus)),
    );

    let session_id = Uuid::new_v4();
    seed_active_session(db.as_ref(), session_id, "peer").await;

    coordinator
        .dispatch(
            session_id,
            CoordinatorInput::Actor {
                epoch: 0,
                report: ActorReport::FatalError(FailureCode::OfferFailed),
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
        model.error_message.as_deref().map(FailureCode::from_column),
        Some(FailureCode::OfferFailed),
        "fatal_error 应把失败判别码持久化到 error_message 列"
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
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let coordinator = TransferCoordinator::new(
        Arc::new(SqlSessionStore::new(db.clone())),
        Arc::new(CoreTransferEvents(event_bus)),
    );

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

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

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

/// 安全回归：对端声明的 `relative_path` 若会逃出保存目录，整条 Offer 必须被拒。
///
/// 接收侧最终做的是 `save_dir.join(relative_path)`，而 `Path::join` 遇到绝对路径会把 base
/// **整段丢弃**、`..` 会向上穿越，`create_dir_all(parent)` 还会把目标目录建出来——这条校验
/// 缺席时，一个已配对的对端可以往本机任意位置写文件（`~/.ssh/authorized_keys`、
/// `/etc/cron.d/...`）。配对不蕴含这个权限：产品自己就有 `temporary` / `collaborator`
/// 这些低于 `owned` 的信任级别。
///
/// 断言的三件事与「暂停接收」那条同构：A 拿到 `UnsafePath`、B **不弹给用户**、B **不落库**。
/// 后两条同样重要——一条被拒的攻击不该在受害者的收件箱里留下痕迹。
#[tokio::test(flavor = "multi_thread")]
async fn e2e_offer_with_escaping_relative_path_is_rejected() {
    let data = b"evil payload".to_vec();
    let source_id = FileSourceId("src-evil".to_string());
    // 宿主侧的元数据用正常路径：攻击点在 **wire 上声明的 relative_path**，
    // 由下面的 `HostEnumeratedFile` 直接给出，不经过本机文件系统。
    let meta = HostFileMetadata {
        name: "authorized_keys".to_string(),
        relative_path: "authorized_keys".to_string(),
        size: data.len() as u64,
        modified_at: None,
        checksum: None,
        save_dir: None,
    };

    let host_a = MemoryHost::new().with_source(source_id.clone(), meta, data.clone());
    let (node_a, node_b) = connected_paired_pair(host_a, MemoryHost::new()).await;

    let prepared_id = Uuid::new_v4();
    node_a
        .transfer
        .prepare(
            prepared_id,
            vec![HostEnumeratedFile {
                source_id: source_id.clone(),
                name: "authorized_keys".to_string(),
                relative_path: "../../../../.ssh/authorized_keys".to_string(),
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
        || {
            node_a.host.events().iter().any(|e| {
                matches!(
                    e,
                    CoreEvent::TransferRejected { event } if event.session_id == session_id
                )
            })
        },
        Duration::from_secs(10),
        "A 收到 TransferRejected(UnsafePath)",
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
        Some(Some(OfferRejectReason::UnsafePath)),
        "路径逃逸的拒绝原因必须是 UnsafePath——并进 PolicyRejected 会让发送方以为\
         是对方的偏好设置问题，而这其实是「你的客户端发了非法数据」"
    );

    assert!(
        !received_offer(&node_b, session_id),
        "路径逃逸的 offer 不得弹给用户——用户没有能力判断这件事，问了也只是把风险转嫁给他"
    );
    assert!(
        ops::get_transfer_projection(node_b.db.as_ref(), session_id)
            .await
            .expect("query b")
            .is_none(),
        "路径逃逸的 offer 不得落库：一条被拒的攻击不该在受害者的记录里留下痕迹"
    );
}

/// 回归（僵尸节点治本）：停止节点后 run_event_loop 随 cancel_token 退出，
/// swarm 被释放、连接断开——对端必须在宽限期后判其离线，而不是被
/// keep-alive 白名单钉死的僵尸连接骗成永久在线。
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_node_goes_offline_on_peer() {
    let (node_a, node_b) = connected_paired_pair(MemoryHost::new(), MemoryHost::new()).await;
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

// ===== 越线规则（openspec: failure-semantics-contract）=====
//
// 「越线点」= `responder.send(OfferResult)`：过了它对端的状态就已经改变，本机撤不回来。
// 这两条测试守的是它两侧的行为，**互为对偶**，改 `accept_and_start_receive` 的语句顺序
// 一定会红掉其中一条。

/// 装一个只有 store 的裸 manager（不起 router / event_loop）。
///
/// 越线规则只涉及 pending 表、store 与 coordinator，不需要真连上对端。
async fn bare_manager(db: Arc<DatabaseConnection>) -> (Arc<TransferManager>, MemoryHost) {
    let host = MemoryHost::new();
    let event_bus: Arc<dyn EventBus> = Arc::new(host.clone());
    let file_access: Arc<dyn FileAccess> = Arc::new(host.clone());
    let transfer = Arc::new(TransferManager::new(
        test_endpoint(SecretKey::generate()).await,
        Arc::new(CoreTransferEvents(event_bus)),
        Arc::new(SqlSessionStore::new(db)),
        file_access,
    ));
    (transfer, host)
}

/// 删掉一条会话及其文件行。**顺序不能反** —— `transfer_files → transfer_sessions`
/// 的外键刻意没带 `ON DELETE`，先删父行会被 SQLite 的 FK 约束拦下。
async fn drop_session_rows(db: &DatabaseConnection, session_id: Uuid) {
    use sea_orm::{ColumnTrait, QueryFilter};
    entity::TransferFile::delete_many()
        .filter(entity::transfer_file::Column::SessionId.eq(session_id))
        .exec(db)
        .await
        .expect("删除文件行");
    entity::TransferSession::delete_by_id(session_id)
        .exec(db)
        .await
        .expect("删除会话行");
}

/// 缓存一条待用户决定的入站 offer，返回它的应答通道。
async fn cache_offer(
    transfer: &TransferManager,
    session_id: Uuid,
    peer_id: NodeId,
) -> tokio::sync::oneshot::Receiver<swarmdrop_core::protocol::TransferResponse> {
    transfer
        .cache_inbound_offer(
            peer_id,
            "对端".to_string(),
            session_id,
            vec![FileInfo {
                file_id: 0,
                name: "a.bin".to_string(),
                relative_path: "a.bin".to_string(),
                size: 1024,
                checksum: "deadbeef".to_string(),
            }],
            1024,
            TransferOrigin::Human,
            swarmdrop_core::transfer::policy::ReceivePolicyDecision {
                action: swarmdrop_core::transfer::policy::ReceivePolicyAction::RequireConfirmation,
                reason: "测试".to_string(),
                save_location: None,
            },
        )
        .await
        .expect("cache_inbound_offer")
}

/// **越线之前**失败 → offer 必须能重试。
///
/// 回归的是这条真实路径：`pending.remove` 之后还有可失败的步骤，一旦失败，那条 offer
/// 从 UI 上消失、`responder` 随 `offer` 一起 drop（对端 RPC 当场断），用户想再点一次
/// 「接受」都没得点。修法是失败时把 offer 放回 `pending`。
///
/// 造失败的方式是删掉库里的会话行，让 `update_session_save_path` 报 `SessionNotFound`
/// —— 用真实的失败路径，而不是往代码里塞一个测试专用的注入点。
///
/// 删之前要先删 `transfer_files`：那条外键**故意没有** `ON DELETE`（删会话是应用层的事，
/// 见 `entity::transfer_file`），所以直接删父行会被 SQLite 拦下。
#[tokio::test(flavor = "multi_thread")]
async fn accept_before_the_line_keeps_the_offer_retryable() {
    let db = make_db().await;
    let (transfer, _host) = bare_manager(db.clone()).await;
    let session_id = Uuid::new_v4();
    let peer_id = SecretKey::generate().node_id();
    let mut rx = cache_offer(&transfer, session_id, peer_id).await;

    // 抽掉会话行 → 越线前的第一步必失败。
    drop_session_rows(db.as_ref(), session_id).await;

    let err = transfer
        .accept_and_start_receive(
            &session_id,
            CoreSaveLocation::Path {
                path: "/tmp".to_string(),
            },
        )
        .await
        .expect_err("越线前失败必须冒泡");
    assert!(
        matches!(err, swarmdrop_core::AppError::SessionNotFound(_)),
        "应报「不存在」而不是别的 kind: {err}"
    );

    assert_eq!(
        transfer.pending_offer_peer(&session_id),
        Some(peer_id),
        "越线还没发生，offer 必须留在待决表里让用户重试"
    );
    assert!(
        matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ),
        "对端不该收到任何应答 —— 通道既没送值也没被 drop"
    );
}

/// **越线未发生**（应答通道已关闭）→ 回滚，不留活的会话。
///
/// 对端 RPC 超时后 handler 会 drop 掉接收端，此时 `responder.send` 失败 = 对端根本没收到
/// 「接受」。原实现是 `let _ =` 直接忽略，于是本机会话停在 `active`、ReceiverActor 挂着
/// 等一份永远不会来的数据。现在要求：撤掉 actor、把会话推到终态、返回 Err。
#[tokio::test(flavor = "multi_thread")]
async fn accept_rolls_back_when_the_peer_already_hung_up() {
    let db = make_db().await;
    let (transfer, _host) = bare_manager(db.clone()).await;
    let session_id = Uuid::new_v4();
    let peer_id = SecretKey::generate().node_id();

    // drop 应答通道的接收端 = 模拟对端 RPC 已超时、handler 已退出。
    drop(cache_offer(&transfer, session_id, peer_id).await);

    let err = transfer
        .accept_and_start_receive(
            &session_id,
            CoreSaveLocation::Path {
                path: "/tmp".to_string(),
            },
        )
        .await
        .expect_err("对端已断开，接受送不出去");
    assert!(
        err.to_string().contains("对端已断开"),
        "错误该指向对端断开，而不是一句泛泛的传输失败: {err}"
    );

    assert!(
        transfer.get_receive_actor(&session_id).is_none(),
        "回滚必须撤掉已注册的 ReceiverActor，否则它会一直等一份不会来的数据"
    );
    let projection = ops::get_transfer_projection(db.as_ref(), session_id)
        .await
        .expect("查询 projection")
        .expect("会话仍在");
    assert_eq!(
        projection.phase,
        TransferPhase::Terminal,
        "会话必须落到终态，不能停在 active"
    );
}

/// 拒绝路径同样把状态转换放在越线之前：转换失败时 offer 放回，用户可以再点一次。
///
/// 反过来（转换写在应答之后）的后果是：用户看到「拒绝失败」，而对端已经按拒绝收尾了，
/// 再点一次只会得到「offer 不存在」。
#[tokio::test(flavor = "multi_thread")]
async fn reject_before_the_line_keeps_the_offer_retryable() {
    let db = make_db().await;
    let (transfer, _host) = bare_manager(db.clone()).await;
    let session_id = Uuid::new_v4();
    let peer_id = SecretKey::generate().node_id();
    let mut rx = cache_offer(&transfer, session_id, peer_id).await;

    // 关掉连接池 → `dispatch` 的第一句 `find_session` 直接报错。
    //
    // 这里**不能**照 accept 那条测试删会话行：`dispatch` 查不到 session 时返回的是
    // `Ok(None)`（视作「无事可做」），拒绝会静默成功，测不到任何东西。
    db.close_by_ref().await.expect("关闭连接池");

    transfer
        .reject_and_respond(&session_id)
        .await
        .expect_err("越线前失败必须冒泡");

    assert_eq!(
        transfer.pending_offer_peer(&session_id),
        Some(peer_id),
        "拒绝没写成，offer 必须留着"
    );
    assert!(
        matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ),
        "对端不该收到拒绝应答"
    );
}
