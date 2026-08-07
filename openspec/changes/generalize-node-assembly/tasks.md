# generalize-node-assembly 任务分解

## Phase 1 — `EndpointProfile` 抽象（core，keystone）✅

- [x] 定义 `EndpointProfile { Native, Browser }`（**精化为 enum 判别**——`preset(impl Preset)` 立即 apply、无法作为值延迟存储，故非「存 preset 值的 struct」；见 design D2）+ `registers_infra()` 判别
- [x] `build_endpoint` 收 `profile: EndpointProfile`（Copy 传值）：preset/address_lookup 按 profile 分支，relay_server 仅 Native+LanHelper，DHT server_mode 仍由 provide_lan_helper 决定
- [x] infra 注册循环按 `profile.registers_infra()` 跳过（Browser 无内置引导）
- [x] 单测 `runtime::tests::both_profiles_bind`：Native profile 装配 bind 成功（回归锚点）+ Browser profile（空 listen）bind 成功 —— **Browser 的浏览器 harness 端到端测归 Phase 3**（web crate 的 wasm-bindgen-test）
- [x] 双 target：`scripts/check-wasm.sh` 全绿（含 `-p swarmdrop-core`）——**顺带坐实 Phase 3 可行**：泛化后的 runtime 编 wasm 干净，web 可调 `start_node`

## Phase 2 — `start_node` 参数化 + 调用点迁移 ✅

- [x] `start_node` 新收 `profile: EndpointProfile` + `os_info: OsInfo`（删内部 `OsInfo::default()` 探测，改入参）；LAN Helper 能力叠加与 `to_agent_version()` 契约保留
- [x] 桌面 `lifecycle.rs`：构造 `OsInfo { name: device_name, ..Default::default() }` + 传 `EndpointProfile::Native`
- [x] 移动 `network.rs`：同上，逐行对称
- [x] ~~`e2e_transfer.rs` 删手抄 endpoint 副本~~ **调整：不 dedup**——e2e 的 `test_endpoint`（只 listen 127.0.0.1、关 mDNS、关 relay_client、DHT server on）是**合法的测试专用配置**，非 `build_endpoint` 手抄；且它**已复用 `build_router`**（真正会漂移的协议注册已共享）。强塞 EndpointProfile 要么引入 `Test` 变体污染生产码、要么让测试因 mDNS 开启而串扰。真正的 endpoint 手抄是 **web 那份**（Phase 3 修掉）。见 design D6 修订。
- [x] 回归：`cargo test -p swarmdrop-core`（31 lib + 16 e2e_transfer 全绿）+ 桌面/移动 `cargo check` 通过（装配行为不变）

## Phase 3 — web 包 core

- [x] `crates/web/Cargo.toml` 加 `swarmdrop-core` 依赖（wasm target-deps）+ getrandom 0.4 wasm_js feature-forcer（wasm-pack 独立构建坑）
- [x] `WebNode::spawn` 改调 `start_node`：注入 `EndpointProfile::Browser` / `MemorySessionStore` / `OpfsFileAccess` / `WebEventSink` / `web_os_info` / 最小 `WebEventBus`
- [x] 删 `node.rs` 内联手抄的 endpoint + router 装配（+ 删 `peer.rs`）—— **这才是本 change 真正收口的手抄副本**（见 design D6）
- [x] 校验 identify/协议/agent_version 前缀契约（保留 `to_agent_version()`）—— 编译级过；对端设备列表可见性属**浏览器运行时**验证
- [x] wasm harness 冒烟：spawn → 配对 → send/accept —— **端到端实测通过**（2026-07-20，agent-browser 真 Chrome + tauri MCP 真桌面，同机）：浏览器 spawn 无 panic → 消费桌面 invite 经 `127.0.0.1/ws` 拨通 → `pair_with_invite` 握手 + 桌面确认 → 配对后 browser→desktop 传两文件（49B + 266KB 多块）**字节级 SHA-256 一致**落盘。identify 证浏览器带全 3 协议（pairing/transfer-ctrl/transfer-data）。附带修 LanOnly 消 bootstrap 空拨噪音。

## Phase 4 — web 配对

- [x] 退役合成 `WebPeerDirectory`（删 `peer.rs`），装真 `PairingManager`（经 `start_node` 的完整 `NetManager`）
- [x] 接通 `connect_invite` → `pairing().pair_with_invite`（真 capability 握手，确认在邀请方桌面侧），返回已配对 NodeId
- [x] Router 由 2 协议升 3 协议（含 pairing，经 `start_node`）；入站未配对设备走**真** `NotPaired` 决策（合成 `Some` 已退役）
- [ ] **推迟**：web `PairingStore`（IndexedDB 持久化 `PairedDeviceInfo`）—— 当前**内存态**起步（`PairingManager` 内存 DashMap，刷新即丢），先验证 e2e 配对+互传；持久化 + 事件循环落库副作用作为后续
- [ ] **推迟**：browser-as-inviter（`encode_invite` + 本机 `PairingRequestReceived` 确认 UI）+ 设备列表事件 surface（当前 `WebEventBus` 丢弃 pairing/device 事件，consume 路径确认在桌面侧不需要）

## Phase 5 — 收尾

- [ ] 浏览器 ↔ 桌面 双端冒烟：web 生成/消费 invite → 双确认 → 信任设备落 IndexedDB → 刷新后重连
- [ ] `cargo test --workspace` + 六 crate wasm 门禁 + `wasm-pack test --headless --chrome -p swarmdrop-web` 全绿
- [ ] 知识库：`net-kernel.md`（EndpointProfile 装配点、web 包 core 后的三端对称表）、`libp2p-wasm.md`（web 消费 core 组合根落地）
- [ ] `crates/web/README.md` 更新（不再「无配对持久化」的措辞）
