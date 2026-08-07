## Why

net-kernel 重构后，「怎么把 net/transfer/host 积木拼成一个 SwarmDrop 节点」的组合根集中在 `crates/core/src/runtime.rs:49` 的 `start_node()`——桌面（`src-tauri/.../lifecycle.rs:64`）与移动（`mobile-core/src/network.rs:214`）都只调它、注入各自端口，随后三步收尾逐行对称，**零漂移**。

唯独 web 例外：`crates/web/Cargo.toml` **无 `swarmdrop-core` 依赖**，`WebNode::spawn`（`crates/web/src/node.rs:82`）把 `build_endpoint` + `build_router` 的活**内联手抄了一遍**——presets::Browser、显式 DHT 但无 infra、Router 只挂 2 协议（无 pairing）、用合成 `WebPeerDirectory`（`node.rs:122-125` 故意给 `Some` 绕过 `incoming.rs` 对 `None` 硬拒 `NotPaired` 的桌面安全边界）顶替配对目录。这是第三条独立、且**无 runtime+e2e 共享保护**的装配路径 → identify 协议、协议集、事件语义各自维护，契约漂移风险实打实。

同一个「endpoint 构造从没参数化」的缺口还制造了**第二份手抄**：`e2e_transfer.rs` 也复制了一份 `build_endpoint`（`runtime.rs:120` 注释明写 e2e 只共享了 `build_router`）。两处手抄同源。

时机已到：core 已 wasm-ready（openspec: `core-wasm-ready` 21/23），wasm 地基铺好（`core/Cargo.toml:40-44` wasm target-deps、`:11-12` 注释「wasm 上 tokio 只用 sync/macros、spawn/time 走 n0-future」、production 不依赖 sea-orm——`:54-55` 那俩只在 dev-dependencies 给 e2e）。

用户 2026-07-20 决策：**web 像桌面/移动那样「包一层 core」**，装配泛化成**可注入积木**（非单函数内分支）；且 web 走**完整配对**（NetManager + 3 协议 + 持久化信任设备），不是 demo 级无持久化。

## What Changes

- **抽 `EndpointProfile`（keystone）**：把 `build_endpoint`（`runtime.rs:151`）硬编码的 `presets::Native`（`:162`）/ `OnlineRecordLookup` address_lookup（`:166`）/ infra 注册循环（`:75-82`）/ `provide_lan_helper` 下的 `relay_server`（`:169`）收成可注入积木。`EndpointProfile::native()` = 现状；`EndpointProfile::browser()` = Browser preset + relay_client + 跳过 infra 注册。
- **`start_node` 参数化**：新收 `profile: EndpointProfile` 入参；`os_info` 从「`OsInfo::default()` env 探测」改为**显式入参**（wasm 下 env 探测恒 `unknown`，web 传 `web_os_info()`——`node.rs:345` 已在，且必须走 `to_agent_version()` 的 `AGENT_PREFIX` 契约，否则 Web 节点在对端设备列表里隐身）。NetManager 与 3 协议 router **保持通用**：因 web 走完整配对，这两层 web 直接复用，分叉收敛到 endpoint 轴。
- **消除 e2e 手抄副本**：`e2e_transfer.rs` 改调泛化后的 `build_endpoint`（test/native profile），删掉复制的那份。
- **web 包 core**：`crates/web` 加 `swarmdrop-core` 依赖；`WebNode::spawn` 改调 `start_node`，注入 `EndpointProfile::browser()` / `MemorySessionStore` / `OpfsFileAccess` / `WebEventSink` / `web_os_info()`。删除内联手抄的 endpoint+router 装配。
- **web 持久化配对**：退役合成 `WebPeerDirectory`，装真 `PairingManager` + **新写 web `PairingStore`**（IndexedDB 或 OPFS——三端第三个持久化后端）；接通 `connect_invite`（`node.rs:180` 已在）的 PairInvite capability 握手；web 事件循环补配对落库副作用（对齐桌面 `event_loop` 的信任设备持久化）。
- **非目标（后续 change）**：webrtc-direct 可达性（B 轨 `webrtc-direct-reachability`，正交并行）；web 前端 React UI（`docs/` 的 `/try` 路由，另立）；web `InboxStore` 持久化（当前 no-op，取舍另议）；6 位码下线（`pair-invite-protocol` 已办）。

## Capabilities

### New Capabilities

- `web-node`: 浏览器作为**完整** SwarmDrop 节点——包共享 `core::runtime::start_node`、Browser `EndpointProfile`、`NetManager` + 3 协议 router、持久化 invite 配对与信任设备。与桌面/移动共享同一装配组合根，identify/协议/事件契约零漂移。

## Impact

- **crates/core**：`runtime.rs` 抽 `EndpointProfile`（`native()`/`browser()` 两构造子），`build_endpoint`/`start_node` 收 `profile` + `os_info` 入参；`bootstrap_node_addrs`/infra 注册按 profile 跳过；wire 上 identify_protocol/协议集不变。**双 target——进 check-wasm 门禁**。
- **src-tauri / mobile-core**：调用点改传 `EndpointProfile::native()` + 显式 `os_info`（逐行对称、两处）；行为不变。
- **crates/web**：加 `swarmdrop-core` 依赖；`node.rs` 删内联装配改调 `start_node`；`peer.rs` 的 `WebPeerDirectory` 退役；新增 `pairing_store.rs`（IndexedDB/OPFS）；`events` 循环补配对落库。
- **e2e**：`e2e_transfer.rs` 去重，改调泛化 `build_endpoint`——**净减一份手抄**。
- **回归面**：桌面/移动节点启动全回归（装配来源变、行为不变）；`cargo test --workspace` + 六 crate wasm 门禁；web 浏览器 harness（`wasm-pack test --headless --chrome -p swarmdrop-web`）。
- **风险**：`start_node` 签名变更牵动两个调用点（可控、对称）；web 首次引入持久化配对，信任设备记录的 schema 需与桌面/移动语义对齐（PairedDeviceInfo）；web `PairingStore` 是新代码面，需单测（写入/读取/清理）。
- **依赖底座（不重复造）**：`core-wasm-ready`（wasm 地基）、`pair-invite-protocol`（web 配对握手复用其协议）、`extract-core-and-add-rn-mobile`（core 组合根来源）。
