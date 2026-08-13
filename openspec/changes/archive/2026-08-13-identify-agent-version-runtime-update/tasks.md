# identify-agent-version-runtime-update 任务分解

> 依赖 `device-config-port`（C5）：`DeviceConfig` 端口、`DeviceName` newtype、`PairingManager`
> 持有的本机 `OsInfo` 都由它提供。**阶段 1–3 是纯 net 层，零 core 依赖，可与 C5 并行**；
> 阶段 4 起必须等 C5 合并。
>
> **阶段 1 + 2（fork 补丁与 rev 升级）走独立 PR** —— CLAUDE.md 硬约束：升级任一 libp2p rev
> 必须独立 PR + 全量测试 + wasm check + 同步 Cargo.lock。

## 1. libp2p fork 补丁（外部仓 `yexiyue/rust-libp2p`）

- [x] 1.1 clone / 更新 fork，**以当前 pin 的 `262dea51` 为基线**开分支
      （`git checkout -b feat/identify-runtime-agent-version 262dea51`）。
      **不要顺手 rebase 到上游 master** —— 那会把 #6558 / #6560 / #6472 三个待合并 PR 的状态
      搅进这次变更，出问题时分不清是谁的锅
- [x] 1.2 `protocols/identify/src/handler.rs`：`InEvent` 枚举（当前 :107-111）新增
      `AgentVersionChanged(String)` 变体
- [x] 1.3 同文件 `on_behaviour_event`（:321-336）加分支
      `InEvent::AgentVersionChanged(v) => self.agent_version = v`
      （字段在 :93，`build_info()` 于 :242 读它）
- [x] 1.4 `protocols/identify/src/behaviour.rs`：`impl Behaviour` 新增
      `pub fn set_agent_version(&mut self, agent_version: String)`，**紧邻 `push`（:280）放**
- [x] 1.5 实现体第一句：值相等则直接 `return`（避免往 `self.events` 塞 N 条 NotifyHandler，
      对应 design D2 ③ 与 spec 的幂等要求）
- [x] 1.6 实现体其余：写 `self.config.agent_version`，再按 `on_swarm_event` 里
      `AddressesChanged` 的形状（:545-559）逐连接下发 ——
      **`NotifyHandler::One(*connection_id)` 不是 `Any`**（一个 peer 可能有 TCP + QUIC /
      relay + direct 多条连接，`Any` 只命中一条，其余连接继续报旧值）
- [x] 1.7 doc comment 用英文，写明「既有连接原地更新，新值用于下次 identify 交换；需要立刻
      广播请配合 `Behaviour::push`」。**不自动 push**（design D2 ①）。不带任何 SwarmDrop 语义
- [x] 1.8 `protocols/identify/tests/smoke.rs` 新增 `runtime_agent_version_update`：照 `periodic_identify`
      的 `libp2p-swarm-test` 形状（`Swarm::new_ephemeral_tokio` + `listen` + `connect` + `drive`），
      连上并完成首次 identify 后 A `set_agent_version` + `push`，断言 B 收到带新 agent 的
      `Event::Received`
- [x] 1.9 `protocols/identify/src/behaviour.rs` 的 `#[cfg(test)] mod tests`（:732）补一条：
      注册一条连接后以**相同值**调用 `set_agent_version`，断言 `self.events` 不新增
      `NotifyHandler`（幂等短路，对应 1.5）
- [x] 1.10 `protocols/identify/CHANGELOG.md` 加条目（顶部当前是 `## 0.48.0`），按上游惯例处理
      版本号 —— 为 design D11 的上游 PR 留形态，**本期不提 PR**
- [x] 1.11 fork 内跑 `cargo test -p libp2p-identify` 与 `cargo clippy -p libp2p-identify`
- [x] 1.12 **改为推分支、不合 master**：`feat/identify-runtime-agent-version` @
      `d858435cfede61115d705ed4797187eef0258861`（基线正是上一版 pin 的 `262dea51`）。
      Cargo 的 `rev =` 能 pin 远端任意可达 commit，不必是 master 上的，所以合并只会
      让 fork master 与上游多出一次分叉、对账时更难看清「哪些补丁提了上游、哪些没提」。
      要合可随时 `git merge`；要撤只需 `git push origin --delete feat/identify-runtime-agent-version`

## 2. rev 升级与依赖同步（独立 PR）

- [x] 2.1 `Cargo.toml`：`libp2p`(:75) / `libp2p-stream`(:76) / `libp2p-core`(:79) /
      `libp2p-swarm`(:80) / `libp2p-webrtc-utils`(:87) **五行同步**换新 rev
- [x] 2.2 `cargo update` 后提交 `Cargo.lock`（**必须与精确 rev 一起提交**）
- [x] 2.3 `Cargo.toml:48-74` 的注释块补一行「identify 运行时 agent_version setter —— 未提交上游
      （follow-up）」，**并改掉 :55 的「全部已提上游，无未提项」** —— 那句话本 change 之后就是错的
- [x] 2.4 `cargo check --workspace --all-targets` + `cargo test --workspace` 全量
- [x] 2.5 `./scripts/check-wasm.sh` 与 `./scripts/check-wasm.sh --clippy`
- [x] 2.6 确认 `crates/webrtc-p2p` 的 `certificate.rs` 跨实现兼容测试
      （`reads_official_pem_with_identical_certhash`）仍绿 —— 它红了说明存量地址会全部拨不通
- [x] 2.7 `mobile/packages/swarmdrop-core/rust/mobile-core` 随 workspace 一并 check（它是同一个
      workspace member，rev 升级会波及）

## 3. `crates/net` —— actor 命令与 Endpoint 方法

- [x] 3.1 `crates/net/src/actor.rs`：`ActorMessage`（:43-105）新增
      `SetAgentVersion { agent_version: String, reply: oneshot::Sender<Result<(), Error>> }`
- [x] 3.2 同文件 `handle_message`（:219 起）新增分支，**顺序写死**：
      ① `self.config.agent_version = v.clone()` → ② `behaviour_mut().identify.set_agent_version(v)`
      → ③ `behaviour_mut().identify.push(self.conns.keys().copied().collect::<Vec<_>>())`
      → ④ `reply.send(Ok(()))`
- [x] 3.3 分支里加注释说明**顺序不可交换**（design D4）：`push` 用 `NotifyHandler::Any`，
      先 push 后 set 会把旧值推出去，且失败是静默的（对端 5 分钟后才被周期交换纠正）
- [x] 3.4 同处加注释说明**两处副本的真值归属**（design D9）：identify Behaviour 是权威
      （新连接的 handler 从它 clone），`Actor.config`(:165) 只是诊断镜像，两者必须同命令更新
- [x] 3.5 `crates/net/src/endpoint.rs`：新增
      `pub async fn set_agent_version(&self, agent_version: String) -> Result<(), Error>`，
      沿用 `add_external_addr`（:227-230）的 `self.request(|reply| …)`（:399）形状
- [x] 3.6 doc comment 写清「立即向所有已连接对端主动 push；未连接的对端在下次连接建立时自然
      拿到新值，不排队补推」
- [x] 3.7 `crates/net/src/config.rs:113-114` 的 `agent_version` 字段注释改写：从「构造期配置」
      改成「运行时可变，权威在 identify Behaviour」
- [x] 3.8 `crates/net/src/endpoint/builder.rs:70-73` 的 setter doc 补一句「运行期改用
      `Endpoint::set_agent_version`」（否则读到 builder 的人会以为只能构造期设）
- [x] 3.9 `crates/net/tests/common/mod.rs`：给 `spawn_node` 加一个可指定初始 agent_version 的
      变体（`spawn_node_with_agent`），**不要让各测试各拼一份 builder**
- [x] 3.10 新增 `crates/net/tests/identify_agent_version.rs`：双节点连上 → 等首次
      `NetEvent::PeerIdentified` → A 调 `set_agent_version` → 断言 B 在**秒级**内收到第二条
      `PeerIdentified` 且 `agent` 是新值。**这是本 change 的核心验收**
- [x] 3.11 同文件补幂等用例：以**相同值**调用 `set_agent_version` 时，B 不应收到新的
      `PeerIdentified`（对应 1.5 / 1.9）

## 4. `crates/core` —— 本机 OsInfo 可变化 + `rename_device` 编排

- [x] 4.1 `crates/core/src/pairing/manager.rs`：把 C5 引入的 `os_info: OsInfo` 字段改成
      `os_info: RwLock<OsInfo>`（`std::sync::RwLock`，与 `network/manager.rs:1` 同风格）
- [x] 4.2 同文件所有读点（`request_pairing`(:274) 的 `PairingRequest.os_info`、`encode_invite`
      的 display）改为 `self.os_info.read().clone()`，**guard 不得跨 `.await` 持有**
      （std 的 guard 非 `Send`，跨 await 会编译期红——这是特性不是障碍）
- [x] 4.3 同文件新增窄写口
      `pub fn set_device_name(&self, name: Option<DeviceName>) -> OsInfo`：只改 `name` 字段并
      返回更新后的完整快照。**不提供 `set_os_info(OsInfo)`** —— 整包替换会让 `caps=lan-helper`
      有机会被静默抹掉（design D7，消费点在 `network/event_loop.rs:123`）
- [x] 4.4 新增 `crates/core/src/device_name.rs` 并在 `lib.rs` 声明：
      `pub async fn rename_device<T: TransferRuntime>(name: Option<DeviceName>,
      device_config: &dyn DeviceConfig, events: &dyn EventBus, net: Option<&NetManager<T>>)
      -> AppResult<()>`
- [x] 4.5 实现四步，顺序**不可调换**（design D6）：① `device_config.save_device_name` →
      ② `pairing.set_device_name`（`net` 为 `None` 时跳过 ②③）→
      ③ `endpoint.set_agent_version(os_info.to_agent_version())` → ④ publish `DeviceRenamed`
- [x] 4.6 方法 doc 写清「① 失败即整体失败、不推网络」的理由：广播成功而落盘失败会让名字在
      下次启动自己回滚，是最难向用户解释的状态
- [x] 4.7 `crates/core/src/host.rs` 的 `CoreEvent`（:38-97）新增
      `DeviceRenamed { name: Option<String>, display_name: String }`；`display_name` 取
      `OsInfo::display_name()`（`crates/host/src/device.rs:194`），省得三端各写一遍
      `name || hostname` 的回退
- [x] 4.8 `crates/core/tests/` 新增双节点端到端用例：A、B 配对并保持连接 → A `rename_device`
      → 断言 B 侧收到 `CoreEvent::PairedDeviceAdded` 且 `device.os_info.name` 是新值。
      这条钉死 design D5 的接收链路（它目前**零覆盖**，断了是静默失效）
- [x] 4.9 同用例补「改回等于 hostname」一条：`to_agent_version()` 在 `name == hostname` 时不写
      `name=` 槽位（`crates/host/src/device.rs:239-246`），所以那也是一次真实变更
- [x] 4.10 补一条单测：`rename_device` 在 `net = None` 时只落盘、不 panic、返回 `Ok`
      （onboarding 路径）

## 5. 桌面接线（`src-tauri` + `src/`）

- [x] 5.1 `src-tauri/src/commands/identity.rs`：`set_device_name`(:59) 改为取
      `NetManagerState`（`Mutex<Option<NetManager>>`，`src-tauri/src/network.rs:15`）后
      转调 `swarmdrop_core::device_name::rename_device(...)`，把 `guard.as_ref()` 直接当
      `Option<&NetManager>` 传进去 —— **宿主侧不写 if/else**
- [x] 5.2 **整段重写 :50-56 的 doc comment** —— 那三行「前端在本命令返回后自己调 shutdown +
      start」正是本 change 要消灭的契约，留着比没有更糟
- [x] 5.3 `src-tauri/src/events.rs` 加 `DeviceRenamed` typed event；`setup.rs` 的
      `collect_events![]` 登记
- [x] 5.4 `src-tauri/src/host/event_bus.rs`：加 `CoreEvent::DeviceRenamed` → Tauri 事件转发
      分支（对齐 :111 的 `PairedDeviceAdded` 写法）
- [x] 5.5 `cargo test export_ts_bindings` 再生 `src/lib/bindings.ts`（**不手改**）
- [x] 5.6 `src/lib/device-name.ts`：`applyDeviceName`(:24-38) **删掉 :29-37 的 stopNetwork +
      startNetwork 整段**，只留 `commands.setDeviceName` + 前端缓存同步
- [x] 5.7 同文件 :18-23 的函数注释改写（现在写着「节点在跑则重启」）；确认 `useNetworkStore`
      的 import(:3) 若因此变成未使用则删掉
- [x] 5.8 `src/routes/_app/settings/-device-info-section.tsx:91-103` 的 `handleSaveName`：
      核对错误路径 —— 改造后失败一定从 core 的 `AppResult` 抛上来，不再有「toast 成功但节点
      已停」的中间态
- [x] 5.9 前端订阅 `DeviceRenamed` 更新 `preferences-store`（多窗口 / MCP 改名时 UI 同步）

## 6. 移动接线（`mobile/`）

- [x] 6.1 `mobile/packages/swarmdrop-core/rust/mobile-core/src/device.rs`（或新建
      `device_config.rs` 侧）新增 `#[uniffi::export] pub async fn rename_device(&self,
      name: Option<String>)`，内部 `DeviceName::parse` 后转调 core 的 `rename_device`，
      `net` 取 `self.net_manager.lock().await.as_ref()`（`app.rs:32`）
- [x] 6.2 `mobile-core/src/events.rs`：`MobileCoreEvent` 加 `DeviceRenamed` 变体 +
      `CoreEvent::DeviceRenamed` 的转换分支（对齐 :233-240 的 `PairedDeviceAdded` 写法）
- [x] 6.3 `pnpm --filter react-native-swarmdrop-core build:ios`（与 Android 侧）重建 uniffi 桥接
- [x] 6.4 `mobile/src/lib/device-name.ts`：`applyDeviceName`(:34-50) **删掉 :38-49 的
      shutdownNode + startNode 整段**，改为调 core 的 `renameDevice`
- [x] 6.5 `usePreferencesStore` 的写入（:36）**挪到 core 成功之后** —— 现在是先写缓存再重启，
      失败时缓存已脏（与桌面 :27 的顺序也不一致）
- [x] 6.6 `mobile/src/components/device-info-card.tsx:54-66` 的 `handleSaveName`：错误路径核对
- [x] 6.7 `mobile/src/app/onboarding/device-name.tsx` 的首启路径：节点此时未启动，确认走的是
      4.10 的「只落盘」分支且不报错

## 7. Web 接线（`crates/web` + `docs/`）

- [x] 7.1 `crates/web/src/node.rs`：`WebNode` 加 `#[wasm_bindgen] pub async fn rename_device`，
      转调 core 的 `rename_device`（`net = Some(&self.net_manager)`）
- [x] 7.2 `crates/web/src/event_bus.rs`：加 `CoreEvent::DeviceRenamed` 分支（对齐 :60 的
      `PairedDeviceAdded`）
- [x] 7.3 `docs/app/app/_lib/node-runtime.ts` 加 `renameDevice` 包装（与 `closeNode`(:51) 同层）：
      `getNode()` 有节点走 `rename_device`，没有则退回 C5 的模块级 `set_device_name`
      —— **这一行分支在 JS 是形态决定的**（节点句柄只活在 JS 里，design D10）
- [x] 7.4 `docs/app/app/_components/node-panel.tsx`：**删除 C5 留下的「刷新页面后生效」提示**，
      改成即时反馈
- [x] 7.5 `docs/app/app/_lib/store.ts` 的 `deviceName` 域接上 `DeviceRenamed` 事件
      （**selector 只返回原始值 / 稳定引用**，`_lib/create-store.ts` 无机器兜底）
- [x] 7.6 确认改名调用**不在** `WebNodeBootstrap` 里 —— 它是 layout 单例，只放 spawn /
      事件消费 / relay 接线，用户动作不进那里
- [x] 7.7 `docs/` 下 `pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web`

## 8. 门禁

- [x] 8.1 `cargo fmt --all`
- [x] 8.2 `cargo check --workspace --all-targets`
- [x] 8.3 `cargo test --workspace`（含 3.10 / 3.11 / 4.8 / 4.9 / 4.10 五条新用例）
- [x] 8.4 `cargo clippy --workspace`
- [x] 8.5 `./scripts/check-wasm.sh`
- [x] 8.6 `./scripts/check-wasm.sh --clippy`
- [x] 8.7 `pnpm exec tsc --noEmit` + `pnpm test`
- [x] 8.8 `pnpm check:zustand-access`（本 change 碰了仓库根 `src/lib/device-name.ts` 与 store）
- [x] 8.9 `mobile/` 下 `pnpm typecheck`
- [x] 8.10 `docs/` 下 `pnpm build`

## 9. 文档与知识库

- [x] 9.1 `dev-notes/knowledge/net-kernel.md:85-96` 的 fork 补丁表加一行（identify 运行时
      agent_version setter，上游 PR 状态「未提交」），**同时改掉 :87 那句「全部自有补丁都已提交
      上游 PR，无未提项」** —— 不改会让下一个对账的人以为这条是别人偷塞进来的
- [x] 9.2 同文件「退出条件」段补一句：identify 补丁是**独立于** #6558 / #6560 / #6472 的第四条，
      它未合并不阻塞前三条的阶段 1 退出判定，但阻塞「删掉 fork pin」
- [x] 9.3 `crates/net/src/endpoint.rs` 模块文档补一句：agent_version 是目前唯一可运行时修改的
      identify 字段（protocol_version 刻意不开，design D2 ②）
- [x] 9.4 `openspec/changes/device-config-port`（C5）的 spec 里那条「改名需重启 / 刷新页面」的
      过渡限制，在本 change 归档时随之失效 —— 归档流程里确认 `specs/` 合并后不留矛盾条款

## 10. 人工验收（需 GUI / 真机）

- [ ] 10.1 **核心场景**：桌面 A 与桌面 B 已配对且**保持连接**；A 改名 → B 的设备列表在**秒级**内
      显示新名字，**两端都不重启、连接不断**（观察 B 的连接指示与 A 的 relay 状态）
- [ ] 10.2 跨端矩阵：桌面 ↔ 移动、桌面 ↔ Web、移动 ↔ Web 各走一次改名
- [ ] 10.3 **回退路径**：把名字改成与本机 hostname 相同 → 对端同样要更新（对应 4.9，最容易漏）
- [ ] 10.4 **清空**：名字设为空串 → 对端显示回落到 hostname
- [ ] 10.5 传输中改名：正在传一个大文件时改名 → 传输**不中断**、进度不回退
      （这正是重启方案做不到的事，是本 change 的价值证明）
- [ ] 10.6 离线对端：B 离线时 A 改名 → B 上线后连上即显示新名字（靠新连接的 handler 取新值，
      不依赖 push）
- [ ] 10.7 首启 onboarding 改名（节点未启动）三端各走一次，确认不报错
- [ ] 10.8 Web 端改名后**不刷新页面**确认对端更新，且本页 relay reservation 不掉
- [ ] 10.9 **LAN Helper 回归**：把一台桌面配成 LAN Helper（`provide_lan_helper`）→ 改名 →
      对端仍认得它的 `caps=lan-helper`（`event_loop.rs:123` 不返回早退）。这条防的是 design D7
      里那个静默失效
- [ ] 10.10 改名后新生成的邀请串 `display_hint` 带新名字；改名前发出的旧邀请仍带旧名字并在
      TTL 后失效 —— **这是预期行为，不是 bug**（design D13）
- [ ] 10.11 Windows 上过一遍构建与冒烟（Rust CI 只跑 ubuntu，rev 升级的平台问题要到打 tag 才暴露）
