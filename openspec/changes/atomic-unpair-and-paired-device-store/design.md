# atomic-unpair-and-paired-device-store 设计

闭合三端抽象层重构里 R3（端口层缺域）的一半：已配对设备的持久化端口，以及「解除配对」
这条至今没有 owner 的写路径。解锁 GitHub #100。

依赖关系：本 change 在序列上排在 C2 / C3（transfer / inbox 端口补全）之外，**不与它们共享
任何文件**；C5（`device-config-port`）会再次改动 `PairingManager` 的构造签名，故本 change 先落。

---

## D0：核实主线审计（三处修正 + 四条新发现）

写代码前逐条核对了任务书给的 file:line。绝大部分吻合，三处需要修正：

| 任务书说法 | 实际 | 影响 |
|---|---|---|
| `CoreEvent` 定义在 `crates/host/src/...` :38-97 | 在 **`crates/core/src/host.rs:38`**。`crates/host/src/ports.rs` 头部注释明写「事件聚合（`CoreEvent` / `EventBus`）与测试用 `MemoryHost` 留在 `swarmdrop-core`——它们引用 network / transfer 域的 DTO，下沉到本 crate 会成环」 | 变体加在 core，不是 host。变体数（17 个）对 |
| Web 的 `*existing = device;` 在 `identity.rs:54` | `find` 在 :54，赋值在 **:55** | 无 |
| `PresenceSupervisor::reconcile_whitelist` 在 :300-332 | 函数签名 :300，**差集撤销那段在 :317-331** | 无 |

四条审计没提、但直接决定实现形态与验收方式的事实：

- **`paired_devices` 是一份 `Arc<DashMap>`，三个消费者共享**（`crates/core/src/network/manager.rs:64-90`）：
  `PairingManager` 读写、`DeviceManager` 只读（`device_manager.rs:64`）、`PresenceSupervisor`
  只读（`presence/supervisor.rs:104`）。所以「删内存表」这一个动作就同时让设备列表、
  presence 白名单、`PeerDirectory` 三者收敛，不需要分别通知。
- **`start_node` 今天收的是快照不是端口**（`crates/core/src/runtime.rs:84`），
  桌面还在其上叠了一层「keychain 为空则回退前端传来的列表」
  （`src-tauri/src/commands/lifecycle.rs:120-131`）。这决定了 D5。
- **加 `CoreEvent` 变体在三端都不是编译期强制的**，四处全有 catch-all——见 D4 末尾。
  这条改变了本 change 的验收方式（清单核对，不能指望编译器）。
- **`crates/web/README.md:147-158` 的两条描述已经过期**：它说 Web「无配对」、
  `PeerDirectory` 恒返回合成值。实际上 Web 走的是与桌面同源的 `runtime::start_node`，
  `build_router`（`crates/core/src/runtime.rs:151-156` / `:185`）把
  `manager.pairing_arc()` 当作真的 `PeerDirectory` 交给 `TransferCtrlService`
  （`crates/transfer/src/incoming.rs:345`）。**Web 的 `receive_policy` 是被真正裁决的**
  （`crates/transfer/src/policy.rs:76-85`）。这条既是 D7 的论据，也是 tasks 7.8 要一并修的文档债。

## D1：拆 `PairedDeviceStore`，端口保持「哑快照存储」

**选项**

- A. 不拆，Web 实现完整 `KeychainProvider`，密钥相关方法返回 `Unsupported`。
- B. 拆出 `PairedDeviceStore`，只含 `load_paired_devices` / `save_paired_devices` 两方法。
- C. 拆出，并把 `upsert` / `update_policy` / `remove` 也放进 trait（端口即仓储）。

**取舍**

A 是现状的延续，代价是 Web 要为两个方法实现六个假方法，而且 `load_identity()` 返回
`Ok(None)` 这种「实现了但永远不该被调用」的方法是最容易被误用的形态——某天有人在 Web 上
调了 `save_identity`，编译通过、运行时静默无效。

C 看着更「仓储」，但会把语义复制三份：`upsert` 里那条「已存在条目保留 `trust_level` /
`receive_policy`」的规则（`crates/core/src/identity.rs:100-109`，测试在 :186-203）是**业务规则**，
不是存储能力。它一旦下放到端口，三端各写一遍——**这正是本 change 要修的那个 bug 的成因**
（Web 的 `*existing = device;`）。

**结论：B。** 端口只有 load/save 两个方法；列表算法（upsert / update_policy / remove）
留在 core 的 `paired_devices` 模块，对 `&dyn PairedDeviceStore` 操作。三端实现各自只有两个
方法、零业务判断，语义分叉在结构上不可能再发生。

代价明说：整份快照覆写有 read-modify-write 竞态。现状本来就是这样（三端全是整份覆写），
本 change 不引入新风险；调用点都在用户操作路径上（串行），不做加固。若将来出现并发写，
正确的修法是给 core 的写操作加一把锁，而不是把算法推给端口。

## D2：模块也要跟着拆——`identity.rs` 不该继续管设备列表

trait 拆了但函数还留在 `identity.rs`，读代码的人照样会把两件事当一件（这正是端口当初被
合并的原因）。所以把 `identity.rs:77-150` 的五个自由函数整体迁到新的
`crates/core/src/paired_devices.rs`，并借模块名简化：

| 迁移前（`identity`） | 迁移后（`paired_devices`） |
|---|---|
| `load_paired_devices` | `load` |
| `save_paired_devices` | `save` |
| `upsert_paired_device` | `upsert` |
| `update_paired_device_policy` | `update_policy` |
| `remove_paired_device` | `remove` |

泛型约束从 `P: KeychainProvider + ?Sized` 改成 `S: PairedDeviceStore + ?Sized`。
**不留 re-export 别名**——共享契约写明「不考虑向后兼容性」，留别名只会让两个名字并存。
`identity.rs` 此后只剩 `load_or_create_identity` / `load_or_create_webrtc_certificate`
与它们的测试。

## D3：`unpair` 的三个副作用，顺序是 fail-closed

`PairingManager::unpair(&self, peer_id) -> AppResult<Vec<PairedDeviceInfo>>`，按序：

```
1. paired_devices::remove(&*self.paired_store, peer_id).await?   // 持久化，失败即 return Err
2. self.paired_devices.remove(peer_id)                            // 共享 DashMap
3. self.event_bus.publish(CoreEvent::PairedDeviceRemoved { .. })  // 仅当 1 或 2 真的删掉了东西
```

**为什么持久化在前。** 两种顺序的失败态不对称：

| 顺序 | 持久化失败时的用户可见状态 |
|---|---|
| 先内存后持久化（**桌面现状**，`commands/pairing.rs:264 → :267`） | 本次运行里设备消失了、用户以为成功；重启后它回来了，且没有任何提示 |
| 先持久化后内存（本 change） | 操作报错、设备还在列表里；用户重试即可，两次运行的状态一致 |

后者是唯一能诚实告知用户的顺序。同一条准则在配对接受路径上已经用过一次：
`respond_pairing_request` 对 `InviteRejectReason::NotPersisted` 宁可让配对失败也不放行
（`crates/core/src/pairing/manager.rs:418-423`），理由一模一样——「本次运行生效、重启后失效」
是最坏的一种成功。

**幂等与事件。** 内存与持久化列表都不含该 peer 时，`unpair` 是 no-op：返回当前列表、
**不发事件**。让「事件 == 集合真的变了」保持为不变量，避免下游把重复点击当成两次变更。

**返回值。** 返回移除后的完整列表。移动端 `remove_paired_device` 的 FFI 签名要求
`Vec<MobileDevice>`（`mobile-core/src/device.rs:276`），桌面 / Web 忽略即可。

## D4：`CoreEvent::PairedDeviceRemoved` —— 补上对称的挂载点

新增变体 `PairedDeviceRemoved { peer_id: NodeId }`（specta 下 `type = String`，与
`PairingRequestReceived` 的 `peer_id` 同款标注）。

**为什么值得加一个变体，而不是复用 `DevicesChanged`。** 三端 host 全都在
`PairedDeviceAdded` 上挂了持久化回写（`src-tauri/src/host/event_bus.rs:111`、
`mobile-core/src/events.rs:196`、`crates/web/src/event_bus.rs`）——也就是说
「已配对集合变了」这件事在新增方向**已经是事件驱动**的，移除方向却连事件都没有。
`DevicesChanged` 携带的是 `Vec<Device>` 读模型（含 presence / 连接态），语义是
「设备视图刷新」，每秒可能发多次，不能用来表达「这台设备不再被信任了」。

三端消费方式：

- 桌面：`event_bus.rs` 转成 tauri typed event `PairedDeviceRemoved`；
  `commands/pairing.rs` 里那句手工 `publish_devices_changed` 删掉。
- 移动：`MobileCoreEvent::PairedDeviceRemoved { peer_id }`。
- Web：`WebEventBus` 记日志。**不额外做推送**——Web 的设备清单是 1.5s 轮询
  （`docs/app/app/_lib/state-poll.ts`），而解除是本地同步操作，UI 在命令返回后主动
  刷一次即可；把 device 事件 surface 到 JS 是 `crates/web/README.md` 里另记的独立债。

**host 不得在事件里再删一次。** core 已经写过持久化了，重复删虽幂等，但会让「持久化失败」
这个错误被第二次成功掩盖掉。事件在移除方向只承担通知职责。

**加变体不会有任何编译错误提醒你去接线——四处 catch-all 全在。** 这一条必须写进 tasks，
因为它推翻了「改 enum 就会红一片」的直觉：

| 位置 | catch-all | 后果 |
|---|---|---|
| `crates/core/src/host.rs:37` | `#[non_exhaustive]` | 下游 match 本来就不被要求穷尽 |
| `src-tauri/src/host/event_bus.rs:165` | `_ => {}` | 桌面静默丢弃 |
| `crates/web/src/event_bus.rs:67` | `other => tracing::debug!(…)` | Web 只多一行 debug 日志 |
| `mobile-core/src/events.rs:315` | `_ => return None` | 移动端连 `MobileCoreEvent` 都不产出 |
| `mobile/src/core/event-bus.ts:167` | `default:` | RN 侧同样不会 `tsc` 报错 |

所以本 change 的「三端接线」是**清单核对**，不是编译器兜底。反过来说，
`KeychainProvider` 拆方法那一步才是真正的编译期广播（见 proposal 风险 1）——两件事
风险性质相反，不要混为一谈。

## D5：`start_node` 收端口，不再收快照

`runtime.rs::start_node` 的 `paired_devices: Vec<PairedDeviceInfo>`（:84）换成
`paired_device_store: Arc<dyn PairedDeviceStore>`，core 内部 `load` 后再交给
`NetManager::new`（构造是同步的，所以 `NetManager::new` 同时收 `Vec` 与 `Arc<dyn …>`：
前者建 `DashMap` 初值，后者转交 `PairingManager`）。

**为什么不能只加端口、保留 `Vec` 参数。** 那样 `DashMap` 的初值来自调用方给的快照，
而 `PairingManager` 之后写回的是端口——**两个事实源，从出生起就可能不一致**。桌面今天
恰好演示了这种不一致会长成什么样：`load_host_paired_devices`
（`src-tauri/src/commands/lifecycle.rs:120-131`）在 keychain 返回空列表时回退到前端
经 IPC 传来的 `paired_devices`。那份前端列表本身是后端的镜像（`secret-store` 从
`initializeIdentity()` 读），回退逻辑因此只在「keychain 读失败但没报错」时才有意义——
一个不该被静默兜底的场景。

连带变更：桌面 `start` 命令删掉 `paired_devices` 参数（`commands/lifecycle.rs:28`），
`src/stores/network-store.ts:147` 的调用点跟着改，bindings 重新导出。

这一节是本 change 里唯一「可以被独立砍掉」的部分（tasks 里单独成组）：砍掉它，
`PairedDeviceStore` 仍然可以经 `NetManager::new` 注入，只是多留一个冗余参数。

## D6：新增方向的持久化**不**在本 change 收进 core

`PairingManager` 拿到端口之后，理论上可以顺手把 `request_pairing` / `respond_pairing_request`
的配对成功写入也做成原子的，三端 event bus 里那三份 `upsert` 回写随之删掉。

**不做。** 理由三条：

1. **缺口深度不同。** 新增方向已经有 `CoreEvent::PairedDeviceAdded` 这个**唯一触发点**，
   三端实现虽然各写一遍但语义一致（本 change 顺带把 Web 那份的语义分叉修掉——见 D7）。
   移除方向此前连触发点都不存在，host 只能在命令里手拼——这才是 R3 说的「端口无出口」。
2. **验收面会糊。** 收新增要动三端六处配对成功回调，还要重排桌面 `PairedDeviceAdded`
   的 emit 时序（现在 `commands/pairing.rs:203/243/329` 手工 emit 一次、event bus 又 emit
   一次）。这些都与 #100「无法解除配对」毫无关系，混在一起会让这个 change 没法被单独验证。
3. **它是一条纯内聚的后续增量。** 端口已经在 `PairingManager` 手上，做的时候不需要再动
   host 层的任何签名。

代价明说：本 change 结束后，`PairingManager` 持有的端口只服务移除这一条写路径。这是刻意的
半步，不是遗漏——写进 `unpair` 的文档注释里。

## D7：Web 的 upsert 语义统一到 core（顺带修，但必须修）

`crates/web/src/identity.rs:55` 是 `*existing = device;`——整条替换。
core 版（`crates/core/src/identity.rs:100-109`）只更新 `os_info` / `paired_at`，
保留 `trust_level` / `receive_policy` / `trust_confirmed`。

这不是风格差异。**触发路径只有一条，要指准**：

- ✅ **再次配对**（`connect_invite`，`crates/web/src/node.rs:298`，upsert 在 :309；
  `respond_pairing_request` 的 upsert 在 :434）。`request_pairing` 成功后返回的是
  `PairedDeviceInfo::new(peer_id, OsInfo::unknown_from_peer_id(…), now)`
  （`pairing/manager.rs:306-310`），而 `PairedDeviceInfo::new`
  （`crates/host/src/device.rs:337-347`）恒为 `Collaborator` + `trust_confirmed: true`。
  Web 的整条替换于是把用户调过的信任级别与收件策略静默重置回默认。
- ❌ **identify 刷新设备名**（`CoreEvent::PairedDeviceAdded` 那条，`event_bus.rs:60`）
  **不受影响**——`refresh_paired_device_os_info`（`pairing/manager.rs:468-478`）返回的是
  共享 `DashMap` 里那条记录的 clone，`trust_level` / `receive_policy` 本来就带着正确值，
  整条替换写回去也一样。

把这条区分写进 design 是因为 Web 侧那个 `PairedDeviceAdded` 分支的注释里正好写着
「实际语义是对端 identify 后刷新 OS 信息」——照着它去复现 bug 会复现不出来，
然后得出「Web 没问题」的错误结论。**验收必须走再次配对那条**（spec 里对应的是
「对已配对设备再次消费邀请」，不是「identify 刷新设备名」）。

Web 侧 `receive_policy` 是**被真正裁决的**，不是展示字段：Web 与桌面共用
`runtime::start_node`，`build_router`（`crates/core/src/runtime.rs:185`）把
`manager.pairing_arc()` 作为 `PeerDirectory` 交给 `TransferCtrlService`
（`crates/transfer/src/incoming.rs:345`，`PairingManager` 的 impl 在
`pairing/manager.rs:494`），入站 offer 经 `crates/transfer/src/policy.rs:76-85` 与
`incoming.rs:194` 判定。（`crates/web/README.md:147-158` 那两条「Web 无配对 /
`PeerDirectory` 恒返回合成值」是 2026-07-19 的旧记录，已被 Web 接入 core 组合根推翻，
tasks 7.8 一并改。）

Web 今天没有调信任级别的 UI，所以存量数据恒为默认值、影响是潜伏的；
但存储层的语义必须先对，否则 Web 一旦加上信任 UI 就是一个静默提权。

修法不是「把两边改成一样」，而是**只剩一份实现**：Web 的三处调用点改走
`swarmdrop_core::paired_devices::upsert`。分叉在结构上消失。

## D8：Web 侧新建的文件叫 `paired_devices.rs`，不叫 `keychain.rs`

任务书写的是「新建 `crates/web/src/keychain.rs`」。**改名为
`crates/web/src/paired_devices.rs`**（类型名 `WebPairedDeviceStore`）。

理由就是这个 change 的论点本身：拆分的依据是「设备列表不是身份私钥」，而 Web 恰恰是
**没有 keychain** 的那一端——它落在 IndexedDB 的 `kv` store 里，没有任何操作系统密钥库参与。
新建一个叫 `keychain.rs` 却与 keychain 无关的文件，会把刚拆开的两个概念在文件名层面重新粘回去。
命名与 core 侧新模块 `paired_devices.rs` 一致。

## D9：presence 撤销必须靠删内存表，不能靠删持久化

`reconcile_whitelist`（`crates/core/src/presence/supervisor.rs:317-331`）算的是
`presence − paired` 差集，`paired` 就是那份共享 `DashMap`。差集命中才会
`set_keep_alive(peer, false)` + `disconnect(peer)` 并清 `ping_failures`。

推论有两条，都要写进验收：

1. **只写持久化不删内存的实现撤销不掉 presence**——保活与重探会一直跑到进程退出。
   Web 今天就是这个状态的极端版（连持久化删除都没有）。
2. **撤销不是同步的**，靠 1s tick。验收写「一个 tick 内（≤2s 容差）」，不写「立即」，
   否则测试必然 flaky。

`unpair` 的第 2 步（删 `DashMap`）因此不是可选优化，是这条能力的唯一开关。
顺带在 `reconcile_whitelist` 的文档注释里补一句判据说明，免得后来人以为改持久化就够了。

## D10：移动端 `ForeignKeychainProvider`（uniffi）不拆

`MobileKeychainAdapter`（`mobile-core/src/keychain.rs:27`）在 Rust 侧同时实现
`KeychainProvider` 与（本 change 之后）`PairedDeviceStore` —— 一个结构体两个 impl，
`MobileCore` 加一个 `paired_device_store()` 访问器即可。

外侧的 `#[uniffi::export(with_foreign)] trait ForeignKeychainProvider`（:14-25）**保持不动**。
拆它要改 `MobileCore::new` 的构造签名、重生成 bindings、改 RN 侧的实现类——而移动端
两个后端（iOS Keychain / Android EncryptedSharedPreferences）本来就是同一个存储桥，
拆分在那一侧不产生任何解耦收益。Web 的情形与之相反（它压根没有 keychain），
所以拆分的价值在 Rust 端口层，不在 FFI 边界。

## D11：节点未运行时的解除路径

三端都有「节点没起来但用户想解除配对」的场景（桌面
`commands/pairing.rs:262-265` 的注释已经写明这个意图）。此时没有 `PairingManager`，
host 直接调 `swarmdrop_core::paired_devices::remove(&*store, peer_id)`。

这看起来还是一个 if/else，但性质不同：它分支的是「core 组合根在不在」这个客观事实，
而不是手工拼装两个副作用。节点没起来时**内存表根本不存在**，持久化删除就是全部副作用，
本身即原子。这条会写进 spec，并在 `unpair` 的文档注释里指路。

## wasm 三条硬约束的自检

| 约束 | 本 change 是否触碰 | 说明 |
|---|---|---|
| **`crates/core` 零 sea-orm** | **否** | 新增的 `PairedDeviceStore` 只吃 `Vec<PairedDeviceInfo>` / `NodeId`，`PairedDeviceInfo` 定义在 `crates/host/src/device.rs`，与 sea-orm 无关。新模块 `paired_devices.rs` 不引入任何依赖 |
| **`crates/transfer` 零 network 依赖** | **否** | 本 change 不改 `crates/transfer` 的任何文件。`PeerDirectory` 的实现方仍是 core 的 `PairingManager`（方向不变：transfer 定义端口、core 实现） |
| **`crates/invite` 零 core 依赖** | **否** | 不改 `crates/invite`。`PairingManager` 对 `InviteRegistry` 的依赖方向不变 |

额外自检：

- **新增 trait 方法签名不吃 `entity::transfer_file::ModelEx`** —— 本 change 的端口只有
  `Vec<PairedDeviceInfo>`，与 transfer 域无关，天然满足。
- **`crates/host` 的轻依赖不变** —— 只增一个 trait，不新增 crate 依赖。
- **`crates/web` 改动面大（新模块 + node.rs + event_bus.rs + lib.rs）**，
  `./scripts/check-wasm.sh`（含 `--clippy`）是本 change 的硬门禁，写进 tasks 最后一组。
- **Web 端「只有接收方向能续传」** 与本 change 无关（不碰会话落库）。
- **版本号三处同步** 不涉及（本 change 不发布）。
- **libp2p rev** 不涉及（那是 C6）。
