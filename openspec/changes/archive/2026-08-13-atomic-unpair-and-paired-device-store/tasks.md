# atomic-unpair-and-paired-device-store 任务分解

> 端口拆分是**编译期广播**：`KeychainProvider` 去掉两个方法的那一刻，三端所有
> `identity::*_paired_*` 调用点一起红。第 1~7 组要一次改完，中途不可编译是预期内的。
> 建议按「host → core → 桌面 → 移动 → Web」的顺序推进，每组结束跑一次 `cargo check`。
>
> **反过来，`CoreEvent::PairedDeviceRemoved` 的三端接线零编译提醒**（`#[non_exhaustive]`
> + 四处 catch-all，见 design D4 末尾的表）。5.7 / 6.6 / 6.7 / 7.6 漏做的表现是「什么都没发生」，
> 必须逐条勾。

## 1. 端口拆分（crates/host）

- [x] 1.1 `crates/host/src/ports.rs` 新增 `PairedDeviceStore` trait（`#[async_trait]`，
      `load_paired_devices` / `save_paired_devices` 两方法，签名照搬现有 :59-60）
- [x] 1.2 给 `PairedDeviceStore` 写文档注释：**为什么它不属于 `KeychainProvider`**
      （设备列表是可导出、会整份覆写的业务数据；密钥材料不出进程），并写明
      **端口刻意只有 load/save**——upsert / 改策略 / 移除的语义在 `swarmdrop_core::paired_devices`，
      端口实现不得自带业务判断（见 design D1）
- [x] 1.3 从 `KeychainProvider`(:43) 删掉 `load_paired_devices` / `save_paired_devices`(:59-60)，
      并在 trait 文档里补一句「已配对设备列表见 `PairedDeviceStore`」
- [x] 1.4 更新 `crates/host/src/lib.rs` 头部模块文档（端口清单 5 → 6）
- [x] 1.5 `cargo check -p swarmdrop-host` 单独过一次（此时下游必然红，属预期）

## 2. core：设备列表算法从 identity 拆出

- [x] 2.1 新建 `crates/core/src/paired_devices.rs`，迁入 `crates/core/src/identity.rs:77-150`
      的五个自由函数并改名：`load` / `save` / `upsert` / `update_policy` / `remove`
      （泛型约束 `S: PairedDeviceStore + ?Sized`）
- [x] 2.2 `upsert` 的「已存在条目保留 `trust_level` / `receive_policy` / `trust_confirmed`」
      规则**原样保留**，并把「为什么不整条替换」写进函数文档（配对成功回调传进来的是
      `PairedDeviceInfo::new(...)`，恒为 `Collaborator`——整条替换会静默重置用户设的策略）
- [x] 2.3 `crates/core/src/identity.rs` 瘦身：只留 `load_or_create_identity` /
      `load_or_create_webrtc_certificate`，模块文档改写为「只管密钥材料」
- [x] 2.4 `crates/core/src/lib.rs` 声明 `pub mod paired_devices;`
- [x] 2.5 **不留 re-export 别名**（共享契约：不考虑向后兼容），旧路径调用点靠编译错误逐个改
- [x] 2.6 把 `identity.rs:185-220` / `:254-268` 三条与设备列表相关的测试迁到新模块
      （`upsert_paired_device_should_insert_then_replace` / `update_paired_device_policy_should_confirm_trust`
      / `remove_paired_device_should_persist_filtered_list`）
- [x] 2.7 新增测试：`upsert` 对已存在条目**不重置 `receive_policy`**
      （现有测试只断言了 `trust_level`，`receive_policy` 没被锁死——那正是 Web 那条分叉
      能长出来的缝）
- [x] 2.8 `crates/core/src/host.rs`：`MemoryHost` 追加 `impl PairedDeviceStore`
      （复用已有的 `MemoryHostInner::paired_devices` 字段），并从 `impl KeychainProvider`
      里删掉那两个方法(:228-243)
- [x] 2.9 `host.rs` 测试 `memory_host_should_round_trip_identity_and_paired_devices`
      的 import 与调用跟随更新

## 3. core：原子 unpair + CoreEvent

- [x] 3.1 `crates/core/src/host.rs` 的 `CoreEvent`(:38) 新增
      `PairedDeviceRemoved { peer_id: NodeId }`，加 `#[cfg_attr(feature = "specta", specta(type = String))]`
      （与 `PairingRequestReceived::peer_id`(:46) 同款标注）。
      **加完立刻在本文件顶部的接线清单上记一笔**——enum 是 `#[non_exhaustive]`(:37)，
      三端都不会因为漏接线而编译失败
- [x] 3.2 `crates/core/src/pairing/manager.rs`：`PairingManager` 加字段
      `paired_store: Arc<dyn PairedDeviceStore>`，`new()`(:151) 加同名参数
- [x] 3.3 实现 `PairingManager::unpair(&self, peer_id: &NodeId) -> AppResult<Vec<PairedDeviceInfo>>`，
      顺序**先持久化 → 再删 `DashMap` → 再 publish**
- [x] 3.4 `unpair` 的文档注释写三件事：① fail-closed 的顺序理由（持久化失败则整体报错、
      内存表不动，避免「本次运行解除、重启复活」）；② 删 `DashMap` 是 presence 撤销的**唯一开关**
      （见 `presence/supervisor.rs:317-331` 的差集判据）；③ 节点未运行时改调
      `paired_devices::remove`（见 D11）
- [x] 3.5 幂等语义：内存与持久化列表都不含该 peer → no-op，**不发事件**，返回当前列表
- [x] 3.6 在 `unpair` 文档里显式记下：本端口当前**只服务移除这一条写路径**，
      新增/刷新方向仍在三端 event bus 里（design D6），这是刻意的半步
- [x] 3.7 `PairingManager::remove_paired_device`(:480) 降为私有或直接并入 `unpair`
      —— 不能留一个「只删内存」的 `pub` 方法在旁边，否则下一个人还会拼两步
- [x] 3.8 `crates/core/src/network/manager.rs`：`NetManager::new`(:54) 加
      `paired_store: Arc<dyn PairedDeviceStore>` 参数并转交 `PairingManager::new`
- [x] 3.9 core 单测：`unpair` 三副作用齐全（`MemoryHost` 断言持久化列表 + `DashMap` +
      `events()` 里有 `PairedDeviceRemoved`）
- [x] 3.10 core 单测：`unpair` 对未配对 peer 幂等且不发事件
- [x] 3.11 core 单测：持久化失败时 `DashMap` **不变**且返回 `Err`
      （用一个会在 `save_paired_devices` 报错的测试替身）

## 4. core：start_node 收端口而不是收快照

> 本组可独立砍掉（砍掉则保留 `Vec` 参数，端口仍经 `NetManager::new` 注入）。见 design D5。

- [x] 4.1 `crates/core/src/runtime.rs`：`start_node`(:80) 的
      `paired_devices: Vec<PairedDeviceInfo>`(:84) 换成
      `paired_device_store: Arc<dyn PairedDeviceStore>`
- [x] 4.2 `start_node` 内部 `load` 一次，把 `Vec` 与 `Arc` 一并交给 `NetManager::new`
      （构造是同步的，所以两个都要传；在注释里写明这不是冗余而是 sync 构造的代价）
- [x] 4.3 `start_node` 文档补一句：已配对设备的事实源是端口，调用方不再预先加载

## 5. 桌面切换（src-tauri + src）

- [x] 5.1 `src-tauri/src/host/keychain.rs`：`DesktopKeychainProvider` 拆出
      `impl PairedDeviceStore`（沿用 `PAIRED_DEVICES_USER` 条目，**不做数据搬家**）
- [x] 5.2 `src-tauri/src/host/file_keychain.rs`：`FileKeychainProvider` 同款拆分
- [x] 5.3 `src-tauri/src/host.rs` 新增 `paired_device_store(app) -> Arc<dyn PairedDeviceStore>`
      工厂，cfg 分叉与 `keychain_provider`(:32) 同形；更新模块头部注释的端口清单
- [x] 5.4 `src-tauri/src/commands/pairing.rs`：`remove_paired_device`(:255) 改为
      节点运行 → `manager.pairing().unpair(&peer_id).await?`；节点未运行 →
      `swarmdrop_core::paired_devices::remove(&*store, &peer_id).await?`
- [x] 5.5 删掉 `persist_paired_device_removal`(:345-349) 与 `remove_paired_device` 里
      那句 `publish_devices_changed`(:268)（后者由 `PairedDeviceRemoved` 事件接管）
- [x] 5.6 `src-tauri/src/commands/pairing.rs` 的
      `update_paired_device_policy`(:284) 改调 `paired_devices::update_policy` + 新 store
- [x] 5.7 `src-tauri/src/host/event_bus.rs`：`PairedDeviceAdded` 分支(:111) 的
      `identity::upsert_paired_device`(:116) 改成 `paired_devices::upsert` +
      `paired_device_store(app)`；新增 `CoreEvent::PairedDeviceRemoved` 分支 → emit tauri event
      （**不再删一次持久化**）。分支要加在 `_ => {}`(:165) **之前**——落到 catch-all 里
      不会报错，只会什么都不发生
- [x] 5.8 `src-tauri/src/events.rs` 新增 `PairedDeviceRemoved(pub String)` typed event
      （紧邻 `PairedDeviceAdded`(:46)）
- [x] 5.9 `src-tauri/src/setup.rs` 的 `collect_events!`(:106) 登记新事件
- [x] 5.10 `src-tauri/src/commands/lifecycle.rs`：`start`(:25) 删 `paired_devices` 参数，
      删 `load_host_paired_devices`(:120-131)，改为注入 `paired_device_store(&app)`
- [x] 5.11 重新导出 bindings：`cargo test -p swarmdrop export_ts_bindings`
      （**勿手改 `src/lib/bindings.ts`**）
- [x] 5.12 `src/stores/network-store.ts:147` 的 `commands.start(pairedDevices, networkOptions)`
      去掉第一个实参
- [x] 5.13 `src/routes/_app/devices/index.lazy.tsx` 的 `handleUnpair`(:191-195)：
      `commands.removePairedDevice` 改为 `await` + 失败提示（现在是 fire-and-forget，
      持久化失败会被完全吞掉）；`clearDeviceOrganization` 保持不变
- [x] 5.14 桌面前端订阅 `PairedDeviceRemoved` 事件同步 `secret-store`
      （与 `handleUnpair` 里的本地移除二选一，不要两条路径各删一次）

## 6. 移动端切换（mobile/）

- [x] 6.1 `mobile-core/src/keychain.rs`：`MobileKeychainAdapter`(:27) 追加
      `impl PairedDeviceStore`，`impl KeychainProvider`(:38) 去掉那两个方法(:88-102)。
      `ForeignKeychainProvider`(:16，`*_paired_devices_json` 在 :23-24) **保持不动**
      （design D10），注释写明理由
- [x] 6.2 `mobile-core/src/app.rs` 加 `pub(crate) fn paired_device_store(&self) -> &dyn PairedDeviceStore`
      访问器（与 `keychain()`(:77) 并列）
- [x] 6.3 `mobile-core/src/device.rs`：`remove_paired_device`(:276) 改为
      节点运行 → `unpair`；节点未运行 → `paired_devices::remove`，返回值形状不变
- [x] 6.4 `mobile-core/src/device.rs` 的 `update_paired_device_policy`(:248) 改调新模块
- [x] 6.5 `mobile-core/src/network.rs:200` 删掉预加载，`start_node`(:218) 改传 store
- [x] 6.6 `mobile-core/src/events.rs`：`MobileCoreEvent`(:112) 新增
      `PairedDeviceRemoved { peer_id: String }`，`map_event`(:212) 补分支
      （**它的结尾是 `_ => return None`(:315)——不补分支就静默丢事件，编译不报错**）；
      `publish`(:191) 里 `PairedDeviceAdded` 的回写(:196-197) 改调 `paired_devices::upsert`
- [x] 6.7 RN 侧事件分支：`mobile/src/core/event-bus.ts` 在 `PairedDeviceAdded`(:70) 旁
      补 `PairedDeviceRemoved`（从 store 移除该设备）。
      **该 switch 有 `default:`(:167)，`tsc` 不会提醒**——靠这条清单
- [x] 6.8 重生成 uniffi bindings：`pnpm --filter react-native-swarmdrop-core build:ios`
      （Android 同步），随后 `mobile/` 下 `pnpm typecheck`

## 7. Web 端（crates/web + docs）

- [x] 7.1 新建 `crates/web/src/paired_devices.rs`（**不叫 `keychain.rs`**，见 design D8）：
      `WebPairedDeviceStore` 实现 `PairedDeviceStore`，承接 `identity.rs:33`(load) /
      `:45`(save) / `:63`(decode) / `:67`(localStorage 迁移兜底) 与常量 `PAIRED_DEVICES_KEY`(:21)
- [x] 7.2 `crates/web/src/identity.rs` 瘦身：删 `load_paired_devices` / `save_paired_devices` /
      `upsert_paired_device`(:33-61) 与迁移兜底，模块文档改写为「只管 SecretKey」
- [x] 7.3 `crates/web/src/lib.rs` 加 `#[cfg(wasm_browser)] mod paired_devices;`
- [x] 7.4 `crates/web/src/node.rs`：`spawn()`(:153) 构造 `Arc<WebPairedDeviceStore>`，
      删掉 `identity::load_paired_devices()`(:168)，改传给 `start_node`
- [x] 7.5 `crates/web/src/node.rs` 的 `connect_invite`(:298，upsert 在 :309) 与
      `respond_pairing_request`(:412，upsert 在 :434) 两处 `identity::upsert_paired_device`
      改调 `swarmdrop_core::paired_devices::upsert`（**整条替换的语义分叉在此消失**，见 D7。
      这两条才是分叉的真实触发路径；identify 刷新那条不受影响，别拿它复现）
- [x] 7.6 `crates/web/src/event_bus.rs`：`WebEventBus::new()` 接收 store，
      `PairedDeviceAdded` 分支(:60) 改调 core 的 `upsert`；新增 `PairedDeviceRemoved` 分支
      （只记日志，不删持久化——core 已写过）。分支要加在 `other =>`(:67) **之前**；
      顺手把 :56-59 那段「实际语义是 identify 刷新」的注释补一句：**它不是策略被重置的成因**
- [x] 7.7 `crates/web/src/node.rs` 新增 wasm 导出
      `pub async fn remove_paired_device(&self, peer_id: String) -> Result<(), JsValue>`
      （解析 base58 失败 → `WebError::invalid_input`；内部走 `unpair`）
- [x] 7.8 `crates/web/README.md` 两条过期记录一并修：
      ① :147-152 的「identity 未走 `KeychainProvider` 端口……配对持久化工程时做完整
      `WebKeychainProvider`」改写为已落地，并说明落地形态是 `PairedDeviceStore` 而非
      完整 keychain（拆分理由见 design D1/D8）；
      ② :157-161 的「Web 侧当前不实现按信任级别的收件策略，`PeerDirectory` 恒返回合成值」
      **是错的**——Web 走同源 `runtime::start_node`，`build_router`(`runtime.rs:185`) 传的是
      真的 `manager.pairing_arc()`，`receive_policy` 经 `transfer/src/policy.rs:76-85` 真被裁决。
      留着它会让下一个人以为 D7 的分叉「反正 Web 用不上」
- [x] 7.9 `docs/` 下 `pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web`
      （`WebNode` 的 TS 类型由 wasm-bindgen 产出，不重生成前端看不到新方法）
- [x] 7.10 `docs/app/app/_components/device-list.tsx`：每行加「解除配对」入口 +
      **行内二次确认**（点击后该行切换成「确认解除 / 取消」，不引入模态——应用区没有 dialog 原语）
- [x] 7.11 用 `useKeyedAsyncAction`（`docs/app/app/_lib/use-keyed-async-action.ts`）
      管每行的 pending / 逐项错误，与收件箱下载、活动续传的形态一致
- [x] 7.12 文案对齐桌面（`src/routes/_app/devices/-components/device-card.tsx:419-458`）：
      「取消配对」/「确认取消配对」/「取消后需要重新配对才能传输文件」
- [x] 7.13 解除成功后立即 `webNodeActions.setPairedDevices(node.paired_devices())`，
      不等 1.5s 轮询（`docs/app/app/_lib/state-poll.ts`）
- [x] 7.14 selector 自查：`device-list.tsx` 的 `useWebNode` 只取原始值或 store 内稳定引用
      （`_lib/create-store.ts` 是自研 store，`pnpm check:zustand-access` **不扫 docs/**）

## 8. presence 撤销的验收与文档

- [x] 8.1 `crates/core/src/presence/supervisor.rs` 的 `reconcile_whitelist`(:296-332)
      文档补一句：**撤销的触发判据是内存 `paired` 表**，只删持久化不会生效
- [x] 8.2 supervisor 测试：从 `ctx.paired` 移除一个 peer 后跑一轮 reconcile，
      断言 `presence` 条目消失、`ping_failures` 清空（可参考现有 :729 附近的用法）
- [x] 8.3 core 集成向测试：`unpair` 之后 `paired` 表不含该 peer
      （把「presence 会跟着撤销」的前置条件钉死在 unpair 这一侧）

## 9. 门禁与验收

- [x] 9.1 `cargo fmt --all`
- [x] 9.2 `cargo check --workspace --all-targets`
- [x] 9.3 `cargo test --workspace`
- [x] 9.4 `cargo clippy --workspace`
- [x] 9.5 `./scripts/check-wasm.sh`
- [x] 9.6 `./scripts/check-wasm.sh --clippy`
- [x] 9.7 `pnpm exec tsc --noEmit` + `pnpm test`（仓库根）
- [x] 9.8 `pnpm check:zustand-access`（本 change 动了 `src/stores/network-store.ts` 与
      `src/routes/_app/devices/`）
- [x] 9.9 `docs/` 下 `pnpm build`（静态导出必须过）
- [x] 9.10 `mobile/` 下 `pnpm typecheck`
- [ ] 9.11 **手动验收 · 桌面**：解除一台设备 → 列表移除 → 重启应用 → **不复活**
- [ ] 9.12 **手动验收 · Web**：解除 → 行内确认 → 列表立即移除 → 刷新页面 → 不复活
- [ ] 9.13 **手动验收 · 移动**：同 9.11
- [ ] 9.14 **手动验收 · 节点未运行**：三端在节点停止状态下解除，重启节点后设备不出现
- [ ] 9.15 **手动验收 · presence 撤销**：解除后观察日志，2s 内出现 `set_keep_alive(false)` +
      `disconnect`，此后不再有对该 peer 的重探
- [ ] 9.16 **手动验收 · 单方语义**：A 解除 B 后，B 端仍显示 A（不自动消失、无通知）；
      B 向 A 发起传输被拒（`OfferRejectReason::NotPaired`）；B 重新发起配对时 A 弹出
      **完整的配对确认**，不是静默恢复
- [ ] 9.17 **回归 · 信任策略（桌面）**：把一台设备调成 `Owned` → 对端经 identify 刷新设备名 →
      策略仍是 `Owned`（`upsert` 保留语义没被拆碎）
- [ ] 9.18 **回归 · Web 的 upsert 分叉（D7 的真实复现路径）**：手工把一条 IndexedDB
      `swarmdrop.pairedDevices.v1` 记录的 `trustLevel` 改成非默认值 → 对同一台设备
      **再走一次邀请配对**（`connect_invite`）→ 该值仍保持。
      **不要用 identify 刷新去验**，那条路径本来就不丢策略（design D7）
- [x] 9.19 **接线核对（无编译器兜底）**：逐个确认 `PairedDeviceRemoved` 在四处都被显式消费——
      桌面 `event_bus.rs` 分支 + tauri event、Web `event_bus.rs` 分支、
      `mobile-core` 的 `map_event`、RN `event-bus.ts` 的 case。
      任一处漏掉的现象都是「安静地什么都没发生」，不是报错
- [x] 9.20 知识库：`dev-notes/knowledge/rust-backend.md` 补两节——
      ①「宿主端口层：`PairedDeviceStore` 与 `KeychainProvider` 的分工」，写明「端口只
      load/save、算法在 core」这条约定；②「加 `CoreEvent` 变体是清单工作不是编译期工作」，
      连 catch-all 的四个位置一起记下
