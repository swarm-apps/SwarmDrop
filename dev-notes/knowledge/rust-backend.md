# Rust Backend

## 概览

Rust 端的项目特有约束：crates/core 与 src-tauri 边界、specta IPC 类型映射、SeaORM/SQLite、libp2p P2P。常规 Rust 风格查 `/rust-best-practices`，async 模式查 `/rust-async-patterns`，Tauri IPC 查 `/tauri-v2`，SeaORM 查 `/sea-orm-2`。

## 模块边界

### 业务逻辑放 crates/core，src-tauri 是薄壳

`src-tauri/src/lib.rs` 用 `pub use swarmdrop_core::pairing;` 等 alias 把 core 模块路径桥进 crate（所以代码里 `crate::pairing::*` / `crate::protocol::*` 仍然有效）。**桌面壳唯一保留业务逻辑的模块是 `transfer/`**，其它都已迁移到 core。

**正确做法**：
- 加新业务逻辑/类型默认放 `crates/core`，让 SwarmDrop-RN 也能复用
- 桌面特定（keychain / 文件系统路径 / Tauri command 包装）才放 src-tauri
- 改 core 时跑 `cargo check -p swarmdrop-core --features specta`，再跑 `cargo check -p swarmdrop` 确认桌面壳不破

**相关文件**：`crates/core/src/lib.rs`、`src-tauri/src/lib.rs`、`dev-notes/architecture/core-desktop-mobile-boundaries.md`

## IPC 类型 (specta)

### bindings.ts 是自动生成的

`src-tauri/src/setup.rs:104` 在 debug build 时调用 `specta.export(..., "../src/lib/bindings.ts")`，每次 `pnpm tauri dev` 启动都会重写。

**不要做**：
- 手动改 `src/lib/bindings.ts`——会被下次 dev 启动覆盖
- 把 bindings.ts 当成"前端可改的契约"

**正确做法**：
- 改 IPC 类型 → 改 Rust 端 struct → 运行一次 `pnpm tauri dev`（或 `cargo run`，会在 setup hook 里触发导出）→ bindings.ts 自动更新
- 临时手改 bindings.ts 只用于"先让 tsc 通过、稍后再启 dev 重新导出"

### specta 需要开 chrono feature

`Cargo.toml` 里 `specta` 必须含 `chrono` feature，否则 `chrono::DateTime<Utc>` 无法 `derive(specta::Type)`。SwarmDrop 已配置（见 `crates/core/Cargo.toml` + `src-tauri/Cargo.toml`）。

### 跨 IPC 的时间类型用 DateTime<Utc>

specta + chrono 会把 `DateTime<Utc>` 映射成 ISO 8601 字符串（前端 `string` 类型）。前端 `new Date(isoString)` 自动正确解析。

**不要做**：
- 用 `i64` 当 IPC 时间戳——前端容易把秒当毫秒（`new Date(秒数)` 解析成 1970 年附近），导致 timer 死循环（见配对码每秒重生 bug 的修复 commit `8d298e5`）

**例外**：DHT 跨设备记录（`ShareCodeRecord`）保持 `i64` Unix 秒以稳定线路格式 + 节省 record 体积。From 转换里手写 `.timestamp()`。

**相关文件**：`crates/core/src/pairing/code.rs`

## Clippy / dead_code

### 用 #[expect(...)] 替代 #[allow(...)]

项目里清一色用 `#[expect(clippy::xxx, reason = "...")]` 而非 `#[allow]`。Rust 1.81+ 的语义是：标了 expect 的 lint 一旦"自然消失"会反向报警，避免遗留的过期 allow。

**正确做法**：
```rust
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub fn insert_session(...) { ... }
```

**相关文件**：`crates/core/src/database/ops.rs`、`crates/core/src/transfer/receive.rs`

## P2P / 异步

### 启动顺序：plugin → updater → database → start command

`src-tauri/src/setup.rs` 里 plugin 在 Builder::default() 注册；updater + database 在 setup() hook 里初始化并注入 Tauri state。**P2P 节点不在启动期自动起**——前端调 `commands::start()` 才创建 `NetClient` + `PairingManager`。

**相关文件**：`src-tauri/src/setup.rs`、`src-tauri/src/lib.rs` 的 `start` 命令

### 断点续传恢复必须双端发布 TransferResumed

恢复协议有两条入口：接收方发起 `ResumeRequest`，发送方发起 `ResumeOffer`。无论哪一端被动收到恢复请求，只要本端重建了 live session，都必须发布 `CoreEvent::TransferResumed`，让 host 把 paused 历史重新提升为 active session。

**正确做法**：
- `handle_resume_request_impl` 重建 `SendSession` 后发布 `TransferResumed { direction: Send }`
- `handle_resume_offer_impl` 重建 `ReceiveSession` 后发布 `TransferResumed { direction: Receive }`

**不要做**：
- 只标记 DB 为 `Transferring` 或只重建 core session；前端 store 不会自动从 history 推断 live session，另一端会停留在 paused 状态。

**相关文件**：`crates/core/src/transfer/resume.rs`、`src/stores/transfer-store.ts`

### 主动取消必须通知对端并写 cancelled

取消不是本地停止任务：本端要取消 live session、通知对端 `TransferRequest::Cancel`、写入 DB `Cancelled`，对端收到后也要标记 cancelled 并发出友好的 UI 提示。

**正确做法**：
- 发送方 `cancel_send` 也要像接收方一样发送 `Cancel`，不能只 `session.cancel()`
- 发送方 `waiting_accept` 还没有 `SendSession`，必须通过 `outbound_offers` 记录并在 Offer 异步返回后撤回，避免对端已接受后继续隐藏传输
- 取消状态写入放在 `crates/core`，Tauri / RN host 只做薄命令封装
- 前端收到 `TransferFailedEvent` 中的 `对方取消` 时按 info toast 展示，不按错误处理

**相关文件**：`crates/core/src/transfer/send.rs`、`crates/core/src/transfer/receive.rs`、`src/stores/transfer-store.ts`

### libp2p-stream 数据通道：不在 stable facade，需直接依赖

`swarm-p2p-core` 用 `libp2p::stream` 承载文件传输等数据面字节流，但 **libp2p 0.56 stable facade 没有 `stream` feature**（libp2p-stream 仍是 `0.4.0-alpha`）。必须直接依赖 `libp2p-stream = "0.4.0-alpha"`（与 libp2p 0.56 同期，对齐 libp2p-swarm 0.47.x，无 multiple-versions 冲突）。

**正确做法**：
- `Behaviour::new_control(&self)` 返回 `Control`（可 clone、跨任务共享）；`Control::accept(proto)` 返回 `IncomingStreams`（生命周期独立于临时 control），`Control::open_stream(peer, proto).await` 打开出站流。
- `Stream` 是 `libp2p_swarm::Stream` re-export（`libp2p::Stream`，**非 feature-gated**），impl `futures::AsyncRead + AsyncWrite`；`DataChannel` 用 `stream_mut()` / `into_stream()` 暴露它，避免 Pin 投影。
- `IncomingStreams` 必须持续 poll：放进 core 中央 `select!`（多协议用 `futures::stream::select_all` 合并 + protocol 标签 + `if !is_empty()` 守卫防 busy-loop），accept 出的流用 `try_send` 转交，**绝不阻塞 swarm 循环**（否则拖死 ping / kad）。
- 开流级背压破损（yamux 静默丢流）：用 runtime 层计数登记表（`ChannelRegistry` + drop guard）显式 limit + 报 typed error，而非依赖底层丢弃。
- `OpenStreamError` 是 `#[non_exhaustive]`（`UnsupportedProtocol(_)` / `Io(_)`），match 必须带 `_`。

**不要做**：
- 不要手写自定义 `NetworkBehaviour + ConnectionHandler`——薄封装 `libp2p-stream` 即可，poll 负担由 core event loop 吸收，对下游透明。
- 不要把帧编解码放进 `libs/core`——它只传裸字节，帧协议在 `crates/core`（应用层）。

**相关文件**：`libs/core/src/data_channel.rs`、`libs/core/src/runtime/{node,event_loop}.rs`、`libs/core/src/client/mod.rs`

### swarm-p2p-core 测试需显式声明 tokio rt-multi-thread

`#[tokio::test(flavor = "multi_thread")]` 需要 tokio `rt-multi-thread`，而 `swarm-p2p-core` 的 `[dependencies] tokio` 只有 `rt`。测试一直靠 workspace feature unification（其他成员带进来）才能编译——**单独 `cargo clippy -p swarm-p2p-core --all-targets` 或单独构建会报 `runtime flavor multi_thread requires rt-multi-thread`**。已在 `[dev-dependencies]` 显式声明 `rt-multi-thread + time`。RN 端单独复用 core 时同理。

**相关文件**：`libs/core/Cargo.toml`

### 传输生命周期：Coordinator reducer + 增量过渡（phase/reason 与旧 SessionStatus 并存）

`redesign-transfer-lifecycle` 把传输状态从扁平 `SessionStatus`（5 态）重构为 `phase`（offered/waiting_accept/active/suspended/terminal）+ `suspended_reason`/`terminal_reason` + `epoch` + `recoverable`。采用**增量过渡**：新字段与旧 `SessionStatus` 列并存、逐步迁移、最后删旧——每步编译通过、不破坏现有传输系统。

**正确做法**：
- 状态机核心是纯函数 reducer（`transfer/coordinator.rs::reduce`）：`(state, input) → Some(new)/None`，无 DB/网络依赖，可独立单元测试（epoch 校验、terminal 不可逆都 hoist 到这一层）。`TransferCoordinator::dispatch` 才做 I/O（load→reduce→persist）。
- **过渡期 status 与 phase 必须同步**：`apply_transition` 写 phase 时经 `TransferPhase::legacy_status(terminal_reason)`（entity 单一映射来源）一并写旧 `status`，否则 coordinator 转换后前端旧路径读到滞留状态。这是 simplify altitude review 抓到的漂移坑。
- `dispatch` 已 load 的 Model 直接传给 `apply_transition(&Model, ...)` 用 `into_active_model` 更新，**不要**在 apply 里二次 `find_by_id`（省一次 SELECT）。
- migration 加列用 `ALTER TABLE ... ADD COLUMN ... NOT NULL DEFAULT ...`；开发期 `DELETE FROM transfer_files/transfer_sessions` 清空旧历史（design 允许），避免处理旧行默认值。
- sea-orm 2.0 entity 用 `ActiveModel::builder().set_xxx()`；加 NOT NULL 字段后在 `create_session` 补 `.set_phase/.set_epoch/.set_recoverable`，未 set 字段走 DB default（builder 不强制）。

**相关文件**：`crates/core/src/transfer/coordinator.rs`、`crates/core/src/database/ops.rs`（apply_transition/projection）、`crates/entity/src/lib.rs`（`TransferPhase::legacy_status`）

### crates/core 端到端集成测试：两个真实节点 + MemoryHost + sqlite::memory（不需要 Tauri/真机）

完整传输链路（offer→transfer→pause→resume→cancel）可在纯 `cargo test` 里跑通，**零生产代码改动**。调研结论：`libp2p-swarm-test` **不适用**——它测 raw `Swarm` + 自定义 `NetworkBehaviour`，和 SwarmDrop 的 `NetClient`/`EventReceiver` 封装层级对不上，且 `CoreBehaviour` 含只能 `with_relay_client` 造的 `relay::client::Behaviour`，传不进 swarm-test。正解是「两个真实 `start()` 节点 + 关 mDNS + 显式 dial」。

**✅ 已落地实证**：`crates/core/tests/e2e_transfer.rs` 已实现并通过——连通性 smoke（`e2e_two_nodes_connect`）+ 完整单文件传输（`e2e_single_file_transfer`：prepare→send_offer→accept→拉取落盘→Complete→Ack→两侧 DB 都到 `Terminal/Completed` + 接收方 `sink_bytes` 等于源 + 两侧都发 `TransferCompleted`）。dev-deps 只需补 `tokio`（macros+rt-multi-thread）+ `migration`（其余 sea-orm/entity/uuid/swarm-p2p-core 已是普通 dep）。`tempfile` 暂未用到（内存库单连接钉死即可跨重启保活）。

**正确做法**：
- 现成资产：`MemoryHost::new(paths)`（`crates/core/src/host.rs`，实现全 6 个 host trait + `with_source()` 预载文件 + `events()` 取回 CoreEvent）；`Database::connect("sqlite::memory:")` + `migration::Migrator::up(&db, None)`；`swarm_p2p_core::start` + `TransferManager::new` + `NetManager::new` + `run_event_loop` 全 public，复刻 `runtime::start_node` 即可。
- **关 mDNS + 显式 dial 消除时序**：`NodeConfig::new(...).with_mdns(false).with_relay_client(false).with_dcutr(false).with_autonat(false).with_listen_addrs(["/ip4/127.0.0.1/tcp/0"])`；建连用 `client.add_peer_addrs(peer, [listen_addr]) + client.dial(peer)`，不靠 mDNS `PeerDiscovered`（这也是 `data_channel.rs` 并行串扰的根治法）。
- 每节点 `tokio::spawn(run_event_loop(receiver, mgr.shared_refs(), host, None))` 驱动接收方协议处理（IncomingTransferRuntime）。
- 断言：`MemoryHost.events()` 查发出的 projection / Transfer* 事件；`db` 查 phase/epoch/checkpoint 验状态机。中断模拟 = drop 一侧 event_loop task；重启 = 用同一 `db` 重新 spawn 节点。
- dev-deps：`migration`（workspace）、`sea-orm`、`tokio`（rt-multi-thread+macros）、`swarm-p2p-core`、`tempfile`。

**不要做**：
- 不要 mock `AppNetClient`（= `NetClient<AppRequest,AppResponse>`，必须两个真实建连节点）。
- 不要忘 `is_paired` 校验：Offer 要求已配对，`NetManager::new` 的 `paired_devices` 要互相塞 `PairedDeviceInfo`（双向），否则 Offer 直接被 `OfferRejectReason::NotPaired` 拒。`is_paired` 唯一运行时依据是 `PairingManager` 的内存 DashMap，不查 DB / keychain。
- 不要等 mDNS 发现事件触发连接——改用 `dial()` 的精确 await。
- **连接判定不要用 `connected_count()` / `get_network_status().connected_peers`**：它额外要求 identify 把 `agent_version` 分类成 SwarmDrop 客户端（`OsInfo::is_swarmdrop_agent`），测试给的 agent_version 不匹配会恒为 0。改用 `manager.devices().is_connected(&peer_id)`（只看裸 `PeerConnected`，与连通性/req_resp/配对都无关）。
- **不要在同步谓词里 `block_on` async DB 查询**：`#[tokio::test]` 已在 runtime 上，再建嵌套 runtime block_on 会 panic（"Cannot start a runtime from within a runtime"）。DB 等待写原生 `async` 轮询循环（`loop { get_transfer_projection().await; sleep().await }`），连接/事件这类同步状态才用同步谓词轮询。
- 端口用 `/ip4/127.0.0.1/tcp/0`（OS 分配），dial 前必须先轮询 `get_network_status().listen_addrs` 拿到实际绑定地址（`run_event_loop` 处理 `NodeEvent::Listening` 时回填）。
- **`client.dial()` 在并行 `cargo test` 下会瞬时失败**：多个 `#[tokio::test(multi_thread)]` + 多组节点同跑抢 CPU 时，到 `127.0.0.1:port` 的连接尝试瞬时失败，`dial().expect()` 会 flaky（串行 `--test-threads=1` 不复现，但 CI 默认并行）。`connect` helper 要**重试 dial 直到 `devices().is_connected(&peer)` 双向为真**、忽略单次 dial 错误（已连接时再 dial 是廉价 no-op 错误）——连接才是目标，不是单次 dial 调用成功。

**相关文件**：`crates/core/tests/e2e_transfer.rs`（**已实现的 harness，直接参照/扩展**）、`crates/core/src/host.rs`（MemoryHost）、`crates/core/src/runtime.rs`（start_node 可复刻）、`crates/core/src/network/event_loop.rs`（run_event_loop）、`libs/core/tests/data_channel.rs`（现有双节点模式参考）

## 身份存储 (keychain)

### dev 用文件后端、release 用系统 keychain（ad-hoc 签名导致 keychain 拒读）

`pnpm tauri dev` 编译的是 **ad-hoc 签名（linker-signed）二进制**——`codesign -dvvv target/debug/swarmdrop` 显示 `flags=0x20002(adhoc,linker-signed)`、`TeamIdentifier=not set`，且 `Identifier` 带内容 hash **每次 rebuild 都变**。macOS login keychain 对 ad-hoc 签名进程访问限制极严，所有 `keyring` 请求（**连查询一个不存在的条目**）都返回 `errSecInteractionNotAllowed`（"Platform secure storage failure: User interaction is not allowed."，不弹授权框直接硬拒）。

表现：设备身份起不来 → `initialize_identity` 抛错 → core `identity.rs` 的 `provider.load_identity().await?` 直接 `?` 传播（`keychain.rs` 只把 `NoEntry` 转 `Ok(None)`，其它错误一律 `Err`，连"生成新身份"退路都没有）→ 前端 `deviceId` 为 null → 点"启动节点"静默无反应。**删 keychain 条目无效**（是签名问题、非条目问题，新签名读旧条目/连查询都被拒）。

**正确做法**：
- 身份存储后端按 build 类型分叉，cfg 边界**唯一集中**在工厂 `crate::host::keychain_provider(&app)`：
  - `#[cfg(debug_assertions)]` → `FileKeychainProvider`（`app_data_dir/dev-identity.json` 明文持久，写后 `chmod 0600`）
  - `#[cfg(not(debug_assertions))]` → `DesktopKeychainProvider`（系统 keychain）
- 工厂返回 `Arc<dyn KeychainProvider>` 统一两分支静态类型（cfg 分支返回不同具体类型，`-> impl Trait` 无法统一）；core 函数签名是 `P: KeychainProvider + ?Sized`，用 `&*provider` 传入。
- 文件后端必须**持久**（keypair 存盘、复用），否则每次重启换 PeerId 破坏配对测试。`load_identity` 在文件缺失/keypair 空时返回 `Ok(None)`（绝不 `Err`），让 core 走"生成新身份并 save"路径。
- 调用 `Arc<dyn KeychainProvider>` 的 trait 方法**不需要** `KeychainProvider` 在 scope（trait object 走 vtable）；从具体 struct 换成 `Arc<dyn>` 后记得删掉原 `use ...::KeychainProvider`，否则 unused import warning。

**不要做**：
- 不要在 `DesktopKeychainProvider` 内部塞 `if-cfg` 降级——release 也可能在 keychain 偶发报错时误把明文私钥落盘；且降级逻辑散落每个方法。独立 provider + cfg 门控 `#[cfg(debug_assertions)] pub mod file_keychain;` 让 release 二进制根本不含文件后端代码。
- 给新增 `#[tauri::command]` 透传 `app: AppHandle` 改变了命令签名（如 `remove_paired_device` 补 app），但 Tauri 按类型注入、不占前端参数位，前端 invoke 不变；改后跑一次 `pnpm tauri dev` 重新导出 bindings 即可。

**相关文件**：`src-tauri/src/host/file_keychain.rs`、`src-tauri/src/host.rs`（`keychain_provider` 工厂）、`crates/core/src/identity.rs`、`src-tauri/src/host/keychain.rs`
