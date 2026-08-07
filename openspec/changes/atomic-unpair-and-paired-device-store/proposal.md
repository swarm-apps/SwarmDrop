## Why

「解除配对」在三端都是**由 host 手工拼出来的两步操作**，core 内部没有任何一处把它们串起来。

`crates/core` 里有两个互不相识的「移除」：

- `PairingManager::remove_paired_device`（`crates/core/src/pairing/manager.rs:480`）——
  只从共享 `DashMap` 里删一条，不碰持久化、不发事件。
- `identity::remove_paired_device`（`crates/core/src/identity.rs:139`）——
  只 load → retain → save 一次持久化快照，不碰内存表、不发事件。

于是每个 host 自己拼，**顺序还相反**：

| 端 | 位置 | 顺序 |
|---|---|---|
| 桌面 | `src-tauri/src/commands/pairing.rs:264` → `:267`（`persist_paired_device_removal` → `:347`） | 先删内存，再写持久化 |
| 移动 | `mobile-core/src/device.rs:278` → `:284` | 先写持久化，再删内存 |
| Web | —— | **两步都没有**：`crates/web/src/node.rs` 上没有任何移除导出 |

桌面那个顺序恰好是错的：持久化写失败时内存表已经被清空，用户以为解除成功，
重启后设备又回来了——这是最难向用户解释的一类状态。

三条连带后果：

1. **Web 端根本解不了配对**（GitHub #100，设备列表只增不减）。`docs/app/app/_components/device-list.tsx`
   整个组件只有「发送」一个动作，wasm 侧也没有可调的导出。
2. **Web 的 presence 订阅永远撤不掉。** `PresenceSupervisor::reconcile_whitelist`
   （`crates/core/src/presence/supervisor.rs:317-331`）每 1s tick 算 `presence − paired` 差集，
   **触发判据只有「内存 `DashMap` 少了 key」**；差集命中才会 `set_keep_alive(false)` + `disconnect`。
   Web 既然连删除路径都没有，那台设备的保活与重探就一直在跑。（同样地，任何「只删持久化不删内存」
   的实现都撤销不掉 presence——这条判据必须写进验收。）
3. **`CoreEvent` 有 `PairedDeviceAdded` 却没有 `PairedDeviceRemoved`**
   （`crates/core/src/host.rs:38-97`，17 个变体）。三端的 event bus 都在 `PairedDeviceAdded`
   上挂了持久化回写（`src-tauri/src/host/event_bus.rs:111`、`mobile-core/src/events.rs:196`、
   `crates/web/src/event_bus.rs` 的 `PairedDeviceAdded` 分支），移除方向却连挂载点都没有——
   host 想统一处理也无从下手，只能在命令里手拼。

再往下一层，端口本身也划错了地方。`crates/host/src/ports.rs` 的 `KeychainProvider`（:43）
把 `load_paired_devices` / `save_paired_devices`（:59-60）和身份私钥、WebRTC 证书 PEM、
迁移状态塞在同一个 trait 里。**已配对设备列表和身份私钥不是一回事**：前者是可导出、可展示、
会频繁整份覆写的业务数据，后者是绝不出进程的密钥材料。这个错误的合并有一个直接受害者——
Web 端至今没有 `KeychainProvider` 实现，因为它**不需要 keychain，只需要设备列表持久化**，
于是 `crates/web/src/identity.rs` 自建了三个自由函数（:33 / :45 / :52）直打 IndexedDB
（key `swarmdrop.pairedDevices.v1`），而 `swarmdrop_core::identity::*` 那一组在 Web 端
**一次都没被调用过**。`crates/web/README.md:147` 早就把这条记成待办了。

两套实现于是各写各的语义：core 版 `upsert_paired_device`（`crates/core/src/identity.rs:100-109`）
对已存在条目**只更新 `os_info` / `paired_at`，刻意保留 `trust_level` / `receive_policy`**
（:186-203 的测试锁死了这条）；Web 版（`crates/web/src/identity.rs:55`）是 `*existing = device;`
**整条替换**。`connect_invite` 对一台已配对设备再走一次邀请时，传进来的是
`PairedDeviceInfo::new(...)`（`crates/host/src/device.rs:337`，恒为 `Collaborator` +
`trust_confirmed: true`），Web 会把用户调过的信任级别与收件策略静默重置回默认。
Web 侧的 `receive_policy` 是**被真正消费的**（`crates/transfer/src/policy.rs:78`、
`incoming.rs:194` 经 `PeerDirectory`），不是纯展示字段。

## What Changes

- **从 `KeychainProvider` 拆出 `PairedDeviceStore` 端口**（`crates/host/src/ports.rs`）。
  两个方法照搬（`load_paired_devices` / `save_paired_devices`），`KeychainProvider` 收缩回
  纯密钥材料。拆分让 Web 第一次能**只实现自己需要的那一半**，而不是为了两个方法去假装有 keychain。
  端口刻意保持「整份快照 load/save」的哑存储形态——列表算法（upsert / 改策略 / 移除）留在 core，
  三端实现各自只有两个方法。

- **设备列表算法从 `identity.rs` 拆到新的 `crates/core/src/paired_devices.rs`**。
  身份模块此后只管 keypair / WebRTC 证书 PEM / 迁移状态。模块边界跟着端口边界走，
  否则 trait 拆了、函数还挤在 `identity` 里，读代码的人仍然会把两件事当一件。

- **`PairingManager::unpair(peer)`：一次做完三件事**——写持久化 → 删共享 `DashMap` → 发
  `CoreEvent::PairedDeviceRemoved`。顺序是 **fail-closed**：持久化失败就整体报错、内存表不动
  （宁可这次没解除，也不要「本次运行解除了、重启又回来」）。这与邀请注册表在
  `respond_pairing_request` 里对 `InviteRejectReason::NotPersisted` 的处理是同一条准则
  （`crates/core/src/pairing/manager.rs:421`）。为此 `PairingManager` 持一份
  `Arc<dyn PairedDeviceStore>`。

- **`CoreEvent` 新增 `PairedDeviceRemoved { peer_id }`**（`crates/core/src/host.rs`）。
  移除因此有了与新增对称的 host 挂载点：桌面转成 tauri typed event，移动转成
  `MobileCoreEvent`，Web 记日志（Web 的设备清单是 1.5s 轮询，见
  `docs/app/app/_lib/state-poll.ts`）。桌面 `remove_paired_device` 命令里那句手工
  `publish_devices_changed` 随之消失。

- **`start_node` 收端口而不是收快照**：`paired_devices: Vec<PairedDeviceInfo>` 参数换成
  `Arc<dyn PairedDeviceStore>`，由 core 自己 load。留着 `Vec` 参数意味着 `DashMap` 的初值
  和 `PairingManager` 将要写回的那个存储**是两个事实源**，从出生起就可能不一致。
  连带删掉桌面 `start` 命令的 `paired_devices` IPC 参数与 `load_host_paired_devices`
  的「keychain 空则回退前端列表」（`src-tauri/src/commands/lifecycle.rs:120-131`）——
  那份前端列表本来就是后端的镜像。

- **Web 首次实现 `PairedDeviceStore`**：新建 `crates/web/src/paired_devices.rs`
  （承接 `identity.rs` 的 :33 / :45 / :52 与 localStorage 迁移兜底），`identity.rs` 只剩密钥。
  `node.rs` 与 `event_bus.rs` 里三处 `identity::upsert_paired_device` 改走 core 的 upsert，
  **整条替换的语义分叉就此消失**（不是「两边都改成一样」，是只剩一份实现）。

- **三端切到同一条路径**：桌面 `commands/pairing.rs:255`、移动 `device.rs:276` 的两步手拼
  换成 `unpair`；Web 新增 wasm 导出 `remove_paired_device` 并在
  `docs/app/app/_components/device-list.tsx` 加入口 + **行内二次确认**，措辞对齐桌面
  （`src/routes/_app/devices/-components/device-card.tsx:419-458` 的「取消配对 / 确认取消配对 /
  取消后需要重新配对才能传输文件」）。

- **节点未运行时仍可解除**：此时没有 `PairingManager`，host 直接调
  `paired_devices::remove(store, peer)`。这一步本身只有持久化一个副作用，天然原子；
  它是「core 没起来」这个事实的分支，不是又一次手拼两步。

**非目标**：

- **设备名**（→ C5 `device-config-port`）。本 change 不碰 `OsInfo::default()` 那三处，
  也不引入 `DeviceConfig` 端口。
- **传输历史与收件箱的端口补全**（→ C2 / C3）。`SessionStore` / `InboxStore` 一个方法不动。
- **新增/刷新方向的持久化收口。** 三端 event bus 仍各自在 `PairedDeviceAdded` 上回写。
  理由见 design D6：新增方向**已经有唯一触发点**（那个事件），移除方向此前连触发点都没有，
  两者缺口深度不同；把新增也收进 core 会牵动三端六处配对成功回调与桌面的 emit 时序，
  与 #100 的验收面无关。它是紧随其后的独立增量。
- **移动端 `ForeignKeychainProvider`（uniffi）不拆。** 拆它要改 `MobileCore` 构造签名、
  重生成 bindings、改 RN 侧两处实现，而 iOS Keychain / Android EncryptedSharedPreferences
  本来就是同一个存储桥。见 design D3。
- **存储后端不迁移。** 桌面已配对设备继续存在系统 keychain 的 `paired-devices` 条目里
  （`src-tauri/src/host/keychain.rs:11`），不做数据搬家。

## Capabilities

### New Capabilities

- `paired-device-lifecycle`: 已配对设备的持久化端口与生命周期——列表存储与身份密钥存储分离；
  解除配对是 core 内的单一原子操作（持久化 + 内存表 + 事件），并因此撤销 presence 维持；
  三端（桌面 / 移动 / Web）走同一条路径；解除是单方动作，语义明确。

## Impact

- **`crates/host`**：`ports.rs` 新增 `PairedDeviceStore`，`KeychainProvider` 去掉两方法。
- **`crates/core`**：新模块 `paired_devices.rs`；`identity.rs` 瘦身；`host.rs` 的 `CoreEvent`
  加变体、`MemoryHost` 加 impl；`pairing/manager.rs` 持端口 + `unpair`；
  `network/manager.rs` 与 `runtime.rs` 的构造签名。
- **`src-tauri`**：`host.rs` 加 `paired_device_store()` 工厂；`keychain.rs` / `file_keychain.rs`
  各加一个 impl；`commands/pairing.rs` 与 `commands/lifecycle.rs`；`events.rs` + `setup.rs`
  注册新事件；`bindings.ts` 重新导出。
- **`src/`**：`network-store.ts` 的 `commands.start` 调用点；`devices/index.lazy.tsx` 的
  `handleUnpair` 改为 await + 失败提示（现在是 fire-and-forget，见 :190-195）。
- **`mobile/`**：`keychain.rs` / `app.rs` / `device.rs` / `network.rs` / `events.rs`；
  `MobileCoreEvent` 新增变体 → **必须重生成 uniffi bindings**；RN 侧事件分支。
- **`crates/web` + `docs/`**：新建 `paired_devices.rs`；`identity.rs` / `node.rs` /
  `event_bus.rs` / `lib.rs`；`pnpm build:wasm` 重新生成 `docs/packages/swarmdrop-web`；
  `device-list.tsx` 加解除入口。
- **回归**：`./scripts/check-wasm.sh`（含 `--clippy`）是硬门禁——本 change 同时动
  core / host / web / transfer 的公共依赖面。

**风险**：

1. **端口拆分是编译期广播**。`KeychainProvider` 去掉两方法后，三端所有 `identity::*_paired_*`
   调用点一次性红掉。这是好事（不会漏），但要一次改完，中途 commit 不可编译。
2. **加 `CoreEvent` 变体恰恰相反：零编译提醒。** `CoreEvent` 是 `#[non_exhaustive]`
   （`crates/core/src/host.rs:37`），三端 event bus 又各有 catch-all
   （桌面 `_ => {}` `src-tauri/src/host/event_bus.rs:165`、Web `other =>`
   `crates/web/src/event_bus.rs:67`、移动 `_ => return None` `mobile-core/src/events.rs:315`），
   RN 侧 switch 还有 `default:`（`mobile/src/core/event-bus.ts:167`）。
   `PairedDeviceRemoved` 漏接线的表现是**静默无事发生**，不是编译失败——
   三端接线必须按 tasks 清单逐条核对。`MobileCoreEvent` 加变体另需重生成 uniffi bindings。
3. **presence 撤销靠 1s tick**，不是同步生效。验收要按「一个 tick 内」而不是「立即」写，
   否则会写出必然 flaky 的测试。
4. **Web 的 `upsert` 语义变更会改变存量数据的演化路径**（不再整条替换）。存量 IndexedDB
   记录本身不需要迁移——字段全在，只是从此不会被默认值覆盖。
   注意复现路径只有「再次配对」一条，identify 刷新那条本来就不丢策略（design D7），
   照着 Web `event_bus.rs:56-59` 的注释去复现会得出错误结论。
