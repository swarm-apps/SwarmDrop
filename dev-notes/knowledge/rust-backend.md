# Rust Backend

## 概览

Rust 端的项目特有约束：crates/core 与 src-tauri 边界、specta IPC 类型映射、SeaORM/SQLite、P2P。常规 Rust 风格查 `/rust-best-practices`，async 模式查 `/rust-async-patterns`，Tauri IPC 查 `/tauri-v2` 与 `/tauri-specta`，SeaORM 查 `/sea-orm-2`。

> **三个已被取代的旧概念**，本文早期条目可能仍按旧模型描述问题，遇到时按新的理解：
>
> - **`swarm-p2p-core` / `libs/`** 已删除 → 网络栈是自研 `crates/net` + `crates/net-base`，
>   API 是 `Endpoint`（见 [net-kernel.md](net-kernel.md)）
> - **6 位分享码** 已废弃 → PairInvite 一次性签名邀请（`crates/invite`）
> - **手写 `invoke` 封装** 已废弃 → IPC 走 tauri-specta 生成的 `src/lib/bindings.ts`
>
> 当前架构以 `CLAUDE.md` 为准。

## 外部打开（share-target 反向流）

### macOS 下 dev 进程与 release .app 并存时，「用 SwarmDrop 打开」会静默丢失文件路径

现象：双击文件 /「打开方式」后窗口只是被聚焦、停在当前页，不进选设备屏，且 dev 日志里没有 `external open: ingest paths`。

机制：macOS 的文件打开走 Apple Event（`RunEvent::Opened`）而非 argv。Launch Services 把 dev 裸二进制（`target/debug/swarmdrop`）和 release .app 视为不同 app → 启动一个新的 release 实例并把 Apple Event 发给它 → 新实例的 single-instance 插件发现已有实例（相同 identifier 的 dev 进程）就转发 argv 并退出——但 macOS 下路径在 Apple Event 里、argv 里没有（`external_open.rs` 的 `handle_second_instance` 在 macOS 是显式 no-op）→ 路径随退出的进程丢失。

**正确做法**：
- 测试「打开方式 / 右键发送」链路时确保只跑一个实例；release 单独运行（冷启动或已运行）链路都正常
- 调试该链路不必真右键：在 dev 里 emit `external-file-open` 事件即可全链路模拟（见 toolchain.md）
- 从终端直接跑 `swarmdrop.app/Contents/MacOS/swarmdrop` 可以看到 release 的 tracing stdout，且 LS 能正常把 Apple Event 发给它

**相关文件**：`src-tauri/src/external_open.rs`、`src-tauri/src/setup.rs`（single-instance 注册）

## 模块边界

### 业务逻辑放 crates/core，src-tauri 是薄壳

**桌面壳已经没有业务逻辑了**——传输逻辑已全部迁到独立 crate `crates/transfer/`，
存储实现迁到 `crates/storage-sql/`，`src-tauri` 只剩命令薄壳 + host adapter + MCP + 托盘。
当前完整边界见 `CLAUDE.md` 的「Workspace 布局」。

**正确做法**：
- 加新业务逻辑/类型默认放 `crates/core`，让 SwarmDrop-RN 也能复用
- 桌面特定（keychain / 文件系统路径 / Tauri command 包装）才放 src-tauri
- 改 core 时跑 `cargo check -p swarmdrop-core --features specta`，再跑 `cargo check -p swarmdrop` 确认桌面壳不破

**相关文件**：`crates/core/src/lib.rs`、`src-tauri/src/lib.rs`、`dev-notes/architecture/core-desktop-mobile-boundaries.md`

### 端口要有出口 —— 注入了却拿不回来的依赖会逼出宿主侧的影子副本

依赖倒置只做一半，比不做更糟。`TransferManager` 的 `store` 字段长期是 `pub(crate)` 且没有
accessor：宿主在组装点把 `Arc<dyn SessionStore>` 注进去，**之后再也拿不回来**。
后果不是「少了个便利方法」，而是宿主要做同一件事只能另开一条路：

| 端 | 影子副本 | 表现 |
|---|---|---|
| 桌面 | `app.manage(DatabaseConnection)` | 5 个传输命令 + 3 个 MCP 工具吃 `State<'_, DatabaseConnection>`，直接打 `storage-sql` 的自由函数 |
| Web | `session_store: Arc<PersistentSessionStore>` | 字段留具体类型而非 `dyn`，历史查询绕过 trait 直调实现的 inherent 方法 |

两条是同一个洞的两种长法：**端口存在、注入正确，但 trait 不是唯一的用法**。
危害会复利：影子副本那条路上没有端口约束，于是顺着它长出来的功能（历史列表 / 删除 /
清空）自然也不会进 trait —— 端口覆盖率因此永远补不齐，等到接第三端才发现
「换一个存储实现」只换掉一半。

**判据**：一个端口是不是真端口，看宿主**取回**它的路径是不是唯一。
补 trait 方法与开 accessor 必须在同一个 change 里做完 —— 只补方法，宿主拿不到 store，
照样打 ORM，账面覆盖率涨了而直连调用点一个没少，下一个人读到「端口已补齐」会得到完全
错误的结论。

**做法**（本仓现行形态）：

- accessor 与同类的 `endpoint()` / `file_access()` 同形，返回 `&Arc<dyn Trait>`；
  要跨 await 持有由调用方自己 `.clone()`
- 字段类型写 `Arc<dyn Trait>` 而不是 `Arc<具体实现>`。自己注入的东西自己也只该按端口用，
  留具体类型等于给未来的绕行留了门
- accessor 只服务**纯读**与**无生命周期语义的写**；带生命周期语义的操作走域方法
  （`TransferManager::delete_session`），判据函数（`is_deletable`）一处定义、
  UI 与后端共用 —— 守卫放各端 UI 则 MCP 与 wasm 导出都绕得过

**相关文件**：`crates/transfer/src/manager.rs`（`store()` / `delete_session()`）、
`crates/transfer/src/store.rs`（`SessionStore` / `is_deletable`）、
`src-tauri/src/database.rs`、`crates/web/src/node.rs`、
`dev-notes/knowledge/storage-abstraction.md`（端口覆盖范围）

### 账本类查询取 `State<TransferStoreState>`，不要经过 `NetManager`

上一条开了 `TransferManager::store()` 之后出现了一个反向偏差：几个**只读账本**的命令
顺手写成了 `get_transfer(&net).await?.store().xxx()`，于是**节点没启动就查不了历史**——
`get_transfer` 在 `net` 为 `None` 时直接 `node_not_started()`。前端 `/transfer` 路由挂载即
`loadProjections()`，start 失败或还没 start 时整页报错；MCP 那边则是「查一份账本却提示
请先启动网络」。

判据很简单：**这条命令碰不碰网络？** 碰 → 走 manager；不碰 → 直接吃
`State<'_, TransferStoreState>`（`src-tauri/src/database.rs`，`setup.rs` 已 `app.manage`）。
收件箱命令一直走的就是后者，传输那一半是漂过去的。

例外是**带生命周期语义的写**：`delete_transfer_session` 仍走 manager，因为「进行中不可删」
的域守卫在那里；`cancel` / `pause` / `resume` / `accept` / `reject` 同理真需要活 actor。

MCP 侧同一条判据：`list_transfers` / `get_transfer_status` 用
`app.try_state::<TransferStoreState>()`，未就绪的文案与其它 store 类工具对齐
（「数据库尚未就绪」而不是「请先启动网络」）。

**相关文件**：`src-tauri/src/commands/transfer.rs`、`src-tauri/src/mcp/tools.rs`、
`src-tauri/src/database.rs`

### 跨端共享的规则要有可执行锚点，不能只写「必须同义」

`swarmdrop_transfer::inbox::inbox_matches` 自称检索命中判据的**规范定义**、文档要求
`crates/storage-sql` 的 `LIKE` 复刻它。结果两者**从落地那天起就不一致**：SQL 匹配四列
（多一个 `fts.extracted_text`），而 `inbox_matches` 根本没有这个入参。一句「改这里必须
同步改那里」的注释，撑不过第一次改动。

修法不是把注释写得更严厉，而是给它一个**可执行的锚点**：

```rust
pub struct InboxMatchCase { name, query, title, source_name, files_text, extracted_text, expected }
pub const INBOX_MATCH_CASES: &[InboxMatchCase];   // 住在共享 crate
```

`crates/transfer` 的单测直调 `inbox_matches` 跑这批语料；`crates/storage-sql` 的单测把
同一批语料灌进内存 SQLite 走 `search_inbox`，断言同一 `expected`。两端断言的是同一个
常量，漂移当场变红。

两条经验：

1. **语料要能抓到真分叉，写完做一次变异验证**。这次做了两组：去掉 `escape_like` →
   「`a%b` 不得命中 `axxb`」变红；把 `fts.extracted_text LIKE ?` 换成 `fts.title LIKE ?`
   → 「extracted_text 独立命中」变红。没做过变异的 conformance 测试很容易是「永远绿」的。
2. **允许的差异要在签名或文档里写死，不要试图消除**。`to_lowercase()` 折叠 Unicode，
   SQLite 的 `LIKE` 默认只折叠 ASCII —— 「Ä」查「ä」Web 命中、桌面不命中。消除它要么给
   SQLite 编 ICU 扩展、要么把 Web 端退化成 ASCII 折叠，两条都比差异本身贵。语料因此只放
   ASCII 大小写用例，理由写进函数文档。
   同理 `extracted_text` 收 `Option<&str>` 而不是 `&str`：`None` 是「这一端没有这个能力」，
   空串是「抽过、没抽到」——**这条差异该在签名上看得见**。

**相关文件**：`crates/transfer/src/inbox.rs`、`crates/storage-sql/src/inbox.rs`

### 宿主端口层：`PairedDeviceStore` 与 `KeychainProvider` 的分工

两个端口装的是两种东西，不要再合并：

| 端口 | 装什么 | 桌面 | 移动 | Web |
|---|---|---|---|---|
| `KeychainProvider` | **秘密**：Ed25519 身份私钥、WebRTC 证书。不出进程、不可导出 | 系统钥匙串（dev 走 `dev-identity.json`） | iOS Keychain / Android EncryptedSharedPreferences | **没有实现** |
| `PairedDeviceStore` | **业务数据**：已配对设备列表。可导出、每次写都是整份快照覆写 | 同上后端的一个条目 | 同一个存储桥 | IndexedDB 的 `kv` |

**约定：端口只 load/save，算法在 core。** 列表语义（`load` / `save` / `upsert` /
`update_policy` / `remove`）唯一实现在 `swarmdrop_core::paired_devices`，对
`&dyn PairedDeviceStore` 操作；端口实现里不得出现任何业务判断。

**不遵守会长出什么——有实例。** 这两件事此前合在 `KeychainProvider` 上
（`load_paired_devices` / `save_paired_devices` 两个方法）。Web 端根本没有 keychain，
实现不了这个 trait，于是在 `crates/web/src/identity.rs` 自己长了一套平行实现，
其中 upsert 写的是 `*existing = device;`——整条替换。core 那份只更新 `os_info` /
`paired_at`，**保留 `trust_level` / `receive_policy` / `trust_confirmed`**。后果是：
对已配对设备再走一次邀请配对（`connect_invite` / `respond_pairing_request` 拿到的是
`PairedDeviceInfo::new(...)`，恒为 `Collaborator`），用户设的信任策略被静默重置回默认。
而 Web 的 `receive_policy` 不是展示字段——Web 与桌面共用 `runtime::start_node`，
入站 offer 经 `crates/transfer/src/policy.rs` 真被裁决。

**判据**：一个端口该不该拆，看两件事是不是同一种数据（秘密 vs 业务数据），
以及**有没有一端只能实现其中一半**。后者一旦出现，「实现了但永远不该被调用」的假方法
（`load_identity` 恒返回 `Ok(None)`）比编译错误更危险——它编译通过、运行时静默无效。
反过来，移动端两个后端本来就是同一个存储桥，拆到 uniffi 的 `ForeignKeychainProvider`
那一侧不产生解耦收益，所以拆分**止步于 Rust 端口层**。

**当前边界（2026-08-05 起）：三个写方向都在 core。** 新增与刷新走
`PairingManager::commit_paired_device`（配对达成的三个点 + `event_loop` 的 identify 刷新
共用它），移除走 `PairingManager::unpair`。**三端 host 只转发事件，不碰存储** ——
`MobileEventBusAdapter` 与 `WebEventBus` 因此都不再持有 `PairedDeviceStore` 字段。

> 它曾是「刻意的半步」：新增/刷新由三端 host 各自在 `CoreEvent::PairedDeviceAdded` 上回写。
> 收口的直接原因见下面「同一个写动作散在三端，会长出三种失败语义」。

**相关文件**：`crates/host/src/ports.rs`（两个 trait）、`crates/core/src/paired_devices.rs`
（唯一的列表算法）、`crates/core/src/identity.rs`（只剩密钥材料）、
`src-tauri/src/host.rs`（`keychain_provider` / `paired_device_store` 两个工厂）、
`crates/web/src/paired_devices.rs`

### 同一个写动作散在三端，会长出三种失败语义

「配对成功 → 把设备写进持久化 → 通知 UI」这段编排一度在三端各写一遍，结果是同一个产品动作
有三种失败行为：

| 端 | 落盘位置 | 写盘失败时 |
|---|---|---|
| 桌面 | 命令层，**逐字重复 3 遍** | `?` 冒泡 → 报错 |
| Web | 命令层，重复 2 遍 | `?` 冒泡 → 报错 |
| 移动 | event bus 里 | 只记一条 `warn` → 静默丢失 |

两种失败都是错的，但错法不同：

- **静默丢失**：用户以为配对好了，重启后设备不见，且没有任何线索。
- **报错更糟** —— 这是本条的重点：走到落盘那一步时**对端已经收到 `Success` 并把本机写进了
  它的列表**。本机此时报「配对失败」，两台设备对同一件事的认知就永久分叉了，而且没有任何
  一端会去纠正。用户看到失败去重试，对面却显示「已配对」。

**判据：一个动作跨过了「对方已经知道了」这条线之后，本地的后续失败就不能再表达成整体失败。**
它的真实后果要如实说（这里是「这台设备重启后会丢」），形态就是把「一半成功」变成返回值而不是
错误 —— `commit_paired_device` 返回 `PairedDeviceCommit { device, persisted }`，三端 UI 在
`persisted == false` 时提示「配对成功，但这条记录没保存下来」。仓里早有同构的先例：
`revoke_pair_invite_by_id` 返回「是否已落盘」，理由一模一样（本次运行内已生效、重启后会复活）。

**收口的副产品可以当验收信号**：把落盘移进 core 之后，`MobileEventBusAdapter` 和 `WebEventBus`
的 `paired_store` 字段双双变成 dead code，编译器直接报 `field is never read`。
**如果收口做对了，host 侧应该有东西变成死代码**；一个都没少，说明只是加了一层转发。

**落盘失败的回退路径也要走同一份合并规则。** 配对收口时踩过：失败分支直接把手里的
`device` 写进共享内存表，而它恒是 `PairedDeviceInfo::new` 的产物（`Collaborator` + 默认
收件策略）。那张表正是 `swarmdrop_transfer::policy` 裁决入站 offer 的事实源 ——
一次写库失败就会把用户设的 `Owned` 降回 `Collaborator`、收紧过的策略放回默认，
**本次运行内立即生效**。合并规则现在住在 `PairedDeviceInfo::merge_observation`，
落盘路径（`paired_devices::upsert`）与失败回退（`commit_paired_device`）共用同一份，
由 `commit_keeps_user_policy_in_memory_when_persist_fails` 钉着。

> **同型问题仍在传输接受路径上（未修，值得单独立项）**：
> `crates/transfer/src/flow/receive.rs` 里 `responder.send(OfferResult { accepted: true })`
> 之后还有 `self.coordinator.dispatch(..).await?` —— 写库失败会让本端 UI 报「接受失败」，
> 而对端已收到 `accepted:true` 并开始推数据；更糟的是 pending 表里那条 offer 已被消费，
> 用户连重试都做不到。判据与配对完全相同：**越过「对方已经知道了」这条线之后，
> 本地失败不能再表达成整体失败**。修法也同构 —— 降级成事件 + 返回值。

### 端口层现有 trait 清单：六个，且没有一格是空的

`crates/host/src/ports.rs` 现有 6 个 trait。动端口层之前先按这张表核一遍——
「数一数端口层有什么」是很多重构的论证前提，表里多一格假的就会得出反向的结论：

| 端口 | 装什么 | 桌面 | 移动 | Web |
|---|---|---|---|---|
| `KeychainProvider` | 密钥材料（Ed25519 身份 / WebRTC 证书） | `keyring`（dev 走 `dev-identity.json`） | iOS Keychain / Android EncryptedSharedPreferences | 无实现（身份自管在 `crates/web/src/identity.rs`） |
| `PairedDeviceStore` | 已配对设备列表（整份快照覆写） | 同 keychain 后端的一个条目 | 同一个存储桥 | IndexedDB `kv` |
| `DeviceConfig` | 用户设的设备名 | `device_config.json` | `data_dir/device_config.json`（同格式） | IndexedDB 键 `swarmdrop.deviceName.v1` |
| `FileAccess` | 文件读写（source 上半区 + sink 下半区） | 本地 FS + Android SAF | `MobileFileAccessAdapter` | OPFS |
| `Notifier` | 系统通知（core 发语义码、host 译） | `tauri-plugin-notification` | `expo-notifications` | 不传（`start_node` 收 `Option`） |
| `UpdateInstaller` | 应用更新 | Tauri updater → SwarmHive | no-op | 不传 |

`DeviceConfig` 的读取动作**由 core 的组合根 `start_node` 承担**，不是各 host 自己读完再把
值塞进 `OsInfo` —— 后者是它取代掉的旧形态，那样本机 `OsInfo` 依然没有唯一装配点。
配套的 `load` 不返回错误 / `save` 返回错误的不对称见 trait 上的 doc。

**曾经有第七个 `AppPaths`，已删（2026-07）。** 它的唯一实现是测试替身 `MemoryHost`，
唯一调用点在 `#[cfg(test)]` 里断言「MemoryHost 返回构造时传进去的路径」——一条只测自己的
测试。零生产实现、零生产消费、零 IPC 暴露。这种端口的害处不是占空间，是**它让端口层的
覆盖率看起来比实际高**。删它的连带改动全是机械的（`MemoryHost::new(paths)` →
`MemoryHost::new()`，约 30 处），编译期兜底。将来真要「默认下载目录」，重建成本十几行；
留一个假实现的成本是持续误导。

**相关文件**：`crates/host/src/ports.rs`、`crates/core/src/host.rs`（`MemoryHost`）、
`src-tauri/src/host.rs`（工厂 + `AppPaths` 的删除理由）

### 两个容易被臆测错的 API 事实

- **`TransferManager` 没有名为 Offer / Send / Resume 的三个入口**。真实的流程入口是 `flow/`
  下的四个模块：`prepare.rs` / `send.rs` / `receive.rs` / `resume/`。`manager.rs` 上的 pub 方法
  是 `new` / `set_receiving_paused` / `spawn_cleanup_task` / `endpoint` / `file_access` /
  `store` / `delete_session`；`handle_cancel` / `handle_pause` / `handle_peer_disconnected` /
  `cache_inbound_offer` 等都是私有 trait impl。
- **没有名为 `FileSink` 的 trait**。sink 是 `FileAccess` trait（`crates/host/src/ports.rs`）的
  下半区（`create_sink` / `write_sink_chunk` / `finalize_sink` / `cleanup_sink`）；
  `FileSinkId` 只是一个 newtype。桌面实现走 `.part` 临时文件再 finalize
  （`src-tauri/src/host/file_sink/path_ops.rs`），**不是直接流式落到最终文件**。

**正确做法**：涉及后端结构的判断先跑 `ls` / `grep` 坐实。文档描述架构，源码才是契约——
本仓的文档曾整整漂移一个大版本，这个教训值得记住。

**相关文件**：`crates/transfer/src/manager.rs`、`crates/host/src/ports.rs`

### NodeEvent 的消费全在 core，桌面壳一次都不碰

**「我们把 NodeEvent 通过 Tauri Channel / RN event emitter 往上抛」是过时的自我描述**，别照抄。

实测：`grep -rn "NodeEvent" src-tauri/src/` → **0 次命中**。

真实分层：

- `crates/core/src/network/event_loop.rs` 是唯一消费者。它完全平台无关：消费内核事件，
  更新网络/设备状态，通过 EventBus 把高层事件推送给 host；host 端（Tauri / RN / wasm）
  只需提供 EventBus + TransferRuntime + Notifier。内核事件由 `crates/net` 的 actor 产出，
  经 `Endpoint` 的 watch / 事件双轨暴露（见 [net-kernel.md](net-kernel.md)）
- 往上抛的是**高层 `CoreEvent`**（`crates/core/src/host.rs:62`，变体远不止 DevicesChanged /
  NetworkStatusChanged，还含 Pairing\* / Transfer\* 一系列），经 **`&dyn EventBus` trait**
  （`crates/core/src/host.rs:123`）
- `src-tauri` 里的 `tauri::ipc::Channel` **只**用在 `host/event_bus.rs` 的
  `Channel<PrepareProgressEvent>`（传输准备进度），**与 NodeEvent 无关**

**相关文件**：`crates/core/src/network/event_loop.rs`、`crates/core/src/host.rs`、`src-tauri/src/host/event_bus.rs`

### 加 `CoreEvent` 变体是清单工作，不是编译期工作

`CoreEvent`（`crates/core/src/host.rs`）是 `#[non_exhaustive]` 的，而三端消费点**全部**
带 catch-all。加一个变体不会有任何编译错误提醒你去接线，漏接的表现是
**「安静地什么都没发生」**——不是报错，也不是崩溃，是功能悄悄不生效。

| 消费点 | catch-all | 漏接后果 |
|---|---|---|
| `src-tauri/src/host/event_bus.rs` | `_ => {}` | 桌面静默丢弃，前端收不到 tauri typed event |
| `crates/web/src/event_bus.rs` | `other => tracing::debug!(…)` | Web 只多一行 debug 日志 |
| `mobile-core/src/events.rs` 的 `map_event` | `_ => return None` | 连 `MobileCoreEvent` 都不产出，FFI 那侧无从谈起 |
| `mobile/src/core/event-bus.ts` 的 switch | `default:` | RN 侧 `tsc` 同样不报错 |

**做法**：加变体时四处一起改，且分支必须写在 catch-all **之前**。桌面还要在
`src-tauri/src/events.rs` 定义 typed event、`setup.rs` 的 `collect_events!` 登记、
重新导出 bindings；移动端要 `pnpm --filter react-native-swarmdrop-core build:ios`
重生成 uniffi bindings，TS 侧才看得到新的 `MobileCoreEvent_Tags`。
另有一处易漏的第五点：命令在**节点未运行**时走的是 host 直连端口的路径，
core 的 event bus 根本不在场，typed event 得由命令自己补发
（`src-tauri/src/commands/pairing.rs` 的 `remove_paired_device`）。

**验收靠清单核对，不靠 `cargo check`。** 反过来记一笔：改端口 trait（例如从
`KeychainProvider` 上删两个方法）才是真正的编译期广播，三端调用点会一起红。
两件事的风险性质相反，别把「改 enum 就会红一片」的直觉套过来。

**相关文件**：`crates/core/src/host.rs`、`src-tauri/src/host/event_bus.rs`、
`crates/web/src/event_bus.rs`、`mobile/packages/swarmdrop-core/rust/mobile-core/src/events.rs`、
`mobile/src/core/event-bus.ts`

### checkpoint 只有接收端写，且早已是 range set 而非线性 offset

两条常被写反的事实，实测坐实：

**① 发送端根本不写 checkpoint**：

```
grep -rn "update_file_checkpoint_ranges" crates/ src-tauri/src/
  → 定义 crates/storage-sql/src/ops.rs:167
  → 测试 crates/core/tests/e2e_transfer.rs:1156
  → 唯一调用点 crates/transfer/src/actor/receiver.rs:457

grep -rnE "checkpoint|completed_ranges|completed_chunks" crates/transfer/src/actor/sender.rs
  → 0 命中
```

entity doc（`crates/entity/src/transfer_file.rs:30`）也写死「仅接收方使用，发送方为空 vec」。
**→ 「发送端认为发到 5MB、接收端只落盘 3MB」这类双端不一致场景在本仓不存在**，别为它设计防御。

**② checkpoint 事实源是 range，不是 bitmap、更不是线性 offset**：
`transfer_file.rs:27-35` 的 doc 明写「新数据面以 range 为 checkpoint 事实源；bitmap 仅作为旧拉取实现和
过渡适配」。`transfer/flow/resume/plan.rs:56` 的 `build_fetch_plan(manifest, checkpoint)` 是
**「清单减去已有」的集合减法**。（`plan.rs:29-31` 有一条 `transferred_bytes` 单连续 range 的过渡
fallback，仅在 `parse_completed_ranges` 为空时生效，不构成「线性单点模型」的定性。）

**相关文件**：`crates/entity/src/transfer_file.rs`、`crates/storage-sql/src/ops.rs`、
`crates/transfer/src/{actor/receiver.rs,actor/sender.rs,flow/resume/plan.rs}`

### `crates/transfer/src/` 按功能分三层子目录

传输模块（`cleanup-transfer-tech-debt` 重组）从平铺 17 文件分成三组，加新文件时按职责归位：

- **`actor/`** —— 运行时单会话执行：`sender`(`SenderActor`)/`receiver`(`ReceiverActor`)/`registry`(`ActorRegistry`)/`checkpoint`(bitmap 纯函数)
- **`flow/`** —— `TransferManager` 的生命周期方法（按阶段拆 `impl` 块）：`prepare`/`send`/`receive`/`resume/{mod,validation,plan}`
- **`wire/`** —— 数据面字节层：`data_frame`(帧编解码)/`data_plane`(路由+注册表簿记，纯路由)/`crypto`
- **顶层** —— 跨层核心类型：`manager`(`TransferManager` 结构+trait impl)/`coordinator`(状态机 reducer)/`epoch`(`EpochGuard`)/`progress`/`policy`/`incoming`

**正确做法**：
- 跨层引用一律用绝对路径 `crate::<组>::<模块>`（本 crate 内）或 `swarmdrop_transfer::…`（跨 crate），
  不用 `super::`（文件进子目录后 `super` 语义会变）
- 文件进子目录后，被跨组调用的 `pub(super)` 要放宽到 `pub(crate)`（`pub(super)` 只剩组内可见）
- 术语固定：运行时内存对象叫 **actor**（`SenderActor`/`ReceiverActor`），**session** 只指逻辑会话 id / DB 行
- `EpochGuard`（`epoch.rs`）是 epoch 比较单点：`is_stale`(迟到`<`)/`is_newer`(更新`>`)/`matches`(精确`==`)，不要再散写裸 `<`/`>`/`==`

**相关文件**：`crates/transfer/src/lib.rs`、`crates/transfer/src/{actor,flow,wire}/mod.rs`、`crates/transfer/src/epoch.rs`

### `FileAccess::read_source_chunk` 的 (offset, length) 是严格契约——宿主违约会炸进 blake3

2026-07 事故：桌面→移动传 >16KiB 文件（用户报「图片」）在发送端 prepare 直接
panic `the subtree starting at 16384 contains at most 16384 bytes`。根因是桌面
`TauriFileAccess::read_source_chunk` 包旧 256KiB `read_chunk(chunk_index)` 接口时
**忽略 length、把 offset 取整到 chunk**——旧传输路径恰好只按 CHUNK_SIZE 对齐调用，
违约被掩盖多时；wire v2 的 bao outboard 构建按 **16KiB leaf 粒度、非对齐 offset**
读，一读就炸。≤16KiB 的文件恰好读对，所以「小文本正常、图片必炸」。

**正确做法**：
- 宿主实现必须精确返回 `[offset, offset+length)`（EOF 截断，越界读返回空）——
  桌面 `path_ops::read_at` 是参考实现，附契约单测（含事故真实尺寸 98061B）。
- 内核侧 `bao::FileAccessReader` 有契约防御：宿主超长返回 → 响错（不截断——超长
  通常伴随 offset 错位，截断会静默产出错误 hash）。
- 多步 fs 操作（open/seek/read）打包**一次** `spawn_blocking`，不逐步走 `tokio::fs`
  （它本质也是 spawn_blocking 包装，多步会付多次跨线程往返）。

**不要做**：
- 不要给 `FileAccess` 新增「按 chunk index」语义的读接口；任何取整/超读都会破坏
  bao 逐块验签。

**相关文件**：`src-tauri/src/host/file_source/path_ops.rs`、
`crates/transfer/src/bao.rs`（`FileAccessReader` + `roundtrip_from_16kib_offset`）

### OsInfo 有 native/display_name helper，别手写设备名回退

`OsInfo`（`crates/host/src/device.rs`）现有两个 helper，新代码优先用它们、别再手写：
- `OsInfo::native()`：native 端（桌面/移动）装配入口，纯 env 探测（hostname / os / platform / arch）。web 端另有 `web_os_info()`。
  **它不收设备名**——`name` 由 `start_node` 从 `DeviceConfig` 端口填，宿主没有 API 可以注入，这是「本机 OsInfo 只有一个装配点」的编译期保证，不是口头约定。
- `OsInfo::display_name()`：`name` 去空白后非空则用、否则回退 `hostname`，收敛 UI 显示名的回退语义。

**不要做**：手写 `name.filter(|n| !n.is_empty()).unwrap_or_else(|| hostname.clone())`——仓库里已有几处历史副本（`transfer/incoming.rs` / `mobile events.rs` / `pairing/manager.rs`）对「空串是否回退 / 是否 trim」处理已分叉，是遇到就该收编进 `display_name()` 的技术债，别再添新副本。

**更不要做**：`OsInfo::default()`。它产出的是占位主机名，而**需要本机 OsInfo 的地方全在
`PairingManager` 手上**（`self.os_info`，组合根注入的快照）：`request_pairing` 的
`PairingRequest.os_info`、`encode_invite` 的 `display_name` / `display_platform`。
这三处此前各自 `OsInfo::default()`，后果是**用户设的名字既进不了配对请求也进不了邀请串**
——邀请卡上恒为占位名、对端配对弹窗恒显示「Device · unknown」。修完之后
`encode_invite` 连 `display: &OsInfo` 参数都删了，取值来源在类型层面就传不错。
本机 `OsInfo` **在节点运行期可变**，唯一写口是 `PairingManager::set_device_name`
（字段是 `RwLock<OsInfo>`），编排在 `swarmdrop_core::device_name::rename_device`：
落盘 → 改本机快照 → `Endpoint::set_agent_version`（identify 逐连接下发）→ publish
`DeviceRenamed`。**改名不再重启节点**，连接不断、进行中的传输不中断。

> 这一段之前写的是「本机 `OsInfo` 在节点生命周期内不变，改名要重启节点，这是刻意的」，
> 理由是「若邀请能热更新而 `agent_version` 不能，会出现新邀请写新名字、对端 identify 到
> 旧名字的中间态」。那个理由在当时成立 —— 因为 libp2p 的 identify 只能构造期设
> `agent_version`。`identify-agent-version-runtime-update` 给 fork 加了
> `Behaviour::set_agent_version`（逐连接下发）之后，两者是**一起**更新的，前提消失了。

只开 `set_device_name` 这个窄写口、**不提供 `set_os_info(OsInfo)`**：整包替换会让
`caps=lan-helper` 有机会被静默抹掉（消费点在 `network/event_loop.rs`，抹掉的表现只是
「同网发现忽然变慢」，几乎不可能反查到改名这一步）。
`set_device_name_only_touches_the_name_field` 钉住这条。

**相关文件**：`crates/host/src/device.rs`（`impl OsInfo`）、`crates/core/src/runtime.rs`
（`start_node` 里的初值装配点）、`crates/core/src/device_name.rs`（`rename_device` 编排）、
`crates/core/src/pairing/manager.rs`（`os_info: RwLock<OsInfo>` + 回归测试）、
`crates/net/src/endpoint.rs`（`set_agent_version`）

### 设备名只能经 `DeviceName::parse` 构造 —— 它挡的是一条真实的注入路径

`OsInfo::to_agent_version()` 把本机信息拼成一行，经 libp2p Identify 广播给每个对端：

```
swarmdrop/0.10.2; name=书房 Mac; caps=lan-helper; os=macos; platform=macos; arch=aarch64; host=MacBook-Pro
```

`from_agent_version()` 按 `"; "` 切片、再按 `name=` / `caps=` 前缀分派。**分隔符就在数据
里，而设备名是用户能随便填的**：把设备名设成 `我的电脑; caps=lan-helper`，对端解析出的
就是一个本机并不具备的 capability。

这不是理论问题。`crates/core/src/network/event_loop.rs` 正是靠
`has_capability(LAN_HELPER_CAPABILITY)` 决定要不要 `add_infrastructure_peer(kad_server:
true, relay: true)`，于是一台自称 lan-helper 的普通设备会被同网对端当成基础设施节点，进
kad server 与 relay 候选。影响面有限（要同网 + 对端开了 `auto_discover_lan_helpers`，结果
是被当候选而非直接拿到数据），但它在桌面 / 移动上**一直是可触发的**——用户本来就能设任意
设备名。

**修法不是在每个入口 `replace(';', "")`。** 桌面命令、移动 uniffi 导出、wasm 导出、以及
未来任何新入口都要各调一次归一化，漏一个就退回原样；而「漏了一个」不会有任何编译或运行
信号。所以归一化做成 **newtype 的唯一构造入口**：

- `DeviceName::parse(&str) -> Option<Self>`：trim → 剥控制字符与 `;` → 按 **char** 截到
  40 → 空则 `None`。没有别的构造函数，字段私有。
- `DeviceConfig` 端口签名吃 `Option<DeviceName>`，**未归一化的 `String` 在类型层面就传不
  进去**。
- FFI / IPC 边界仍用 `Option<String>`（uniffi 与 specta 不必认识这个类型），进来第一件事
  就是 parse。

**四条容易写错的细节：**

- **截断按 char 不按 byte。** 中文名 40 字是 120 字节，按 byte 切会切碎 UTF-8 序列直接 panic。
- **load 侧也要 parse 一次。** `device_config.json` 与 IndexedDB 都是用户 / 开发者工具可
  手改的，只在 save 侧归一化等于没归一化。三端实现都在读出来之后再过一遍。
- **空串归 `None` 而不是 `Some("")`。** `None` 正是端口「清空、回退 hostname」的语义。
- **超长截断而不报错。** 三端 UI 一律 `maxLength=40` 拦在前面，后端截断只是防御纵深，为它
  造一条跨三端的错误路径不划算。

**回归锚点**：`crates/host/src/device.rs` 的
`device_name_blocks_agent_version_capability_injection` —— 含 `"; caps=lan-helper"` 的原始
串经 parse → `to_agent_version()` → `from_agent_version()` 走一圈，断言 `capabilities` 为
空。**这条红了是去补归一化，不是改断言。**

**相关文件**：`crates/host/src/device.rs`（`DeviceName` + 回归锚点）、
`crates/host/src/ports.rs`（`DeviceConfig`）、`crates/core/src/network/event_loop.rs`（
capability 的消费方）

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

**例外**：**跨设备的 wire 类型**保持 `i64` Unix 秒，以稳定线路格式 + 节省 record 体积
（当前是 DHT 的 `OnlineRecord::timestamp`）。From 转换里手写 `.timestamp()`。
区分标准是「跨 IPC」还是「跨设备」——前者用 `DateTime<Utc>`，后者用 `i64`。

**相关文件**：`crates/core/src/presence/mod.rs`（`OnlineRecord`）

### MCP 读取前端本机偏好时要容错降级

设备别名和分组属于本机 UI 偏好，不进入配对记录或 P2P 协议；MCP 要展示同一设备身份时，直接从
`tauri-plugin-store` 的 `preferences.json` 读取 Zustand 的 JSON 字符串，并将缺失、旧格式或解析失败
视为无组织数据。这样 MCP 不需要新增 IPC，也不会因本机偏好损坏而影响设备查询或发送。

**正确做法**：
- 读取 `preferences-store` 后先解析其 `{ state: ... }` 包装，再取 `state.deviceOrganization`
- 使用 `#[serde(default)]` 的本地反序列化结构；失败时 `unwrap_or_default()`
- MCP 返回 `displayName`、`groups` 与 `identityHint`，但操作仍只接受精确 PeerId

**不要做**：
- 不要将本机别名写入配对记录、设备 Identify 信息或 P2P 协议
- 不要按 MCP 的显示名自动发送；同名候选须以分组和身份提示向用户澄清

**相关文件**：`src-tauri/src/mcp/tools.rs`、`src/stores/preferences-store.ts`

### AppError 的 Serialize 是手写的，不是 derive —— 加 variant 字段不会炸 IPC

两个 `AppError` 都**只有** `#[derive(Debug, Error)]`（`crates/host/src/error.rs`、
`src-tauri/src/error.rs:28`），Serialize 是**手写的** `impl Serialize for AppError`
（core 在 `:55-82`、桌面在 `:98`，match 投影成 `{kind, message}`）。

**容易误认的那个**：`src-tauri/src/error.rs:21-26` 那个 `#[derive(Debug, Clone, Serialize, specta::Type)]`
挂的是 **`AppErrorPayload`** —— 一个独立的普通 struct，**不是 `AppError`**。

**推论**：往 `AppError` variant 里插字段**不会**因 `derive(Serialize)` 编译失败，
多出的字段会被手写 Serialize **静默忽略**（前端契约不变）。所以「加字段会炸 IPC」这个顾虑不成立，
真实成本在别处（例如某些宏方案要求具名字段，而我们的 variant 多是 tuple/unit 形式）。

**相关文件**：`crates/host/src/error.rs`、`src-tauri/src/error.rs`

### `AppError` 的 `kind` 是**用户文案的判别码**，不是日志分类

桌面按 `kind` 查表渲染本地化文案（`src/lib/errors.ts` 的 `KIND_MESSAGES`），`message` 只进
日志 —— 它是 Rust 侧的中文串，直接展示会在英文界面露馅。所以**给一个失败选 kind，
等于在选用户会看到的那句话**。

> ⚠️ **这条只在桌面完全成立。** Web 侧把 `AppError` 收敛成 `WebError` 的七个变体（有
> `kind` 但前端多数调用点直接显示 `message`）；**移动端根本没有 kind → 文案表** ——
> `mobile/src/lib/utils.ts` 的 `errorMessage` 只是把 `FfiError.Variant: inner` 拼成一个串
> 丢给 toast，于是 Rust 侧那些中文 message 会**原样出现在英文界面上**。
> 这是已知负债，不是这次能顺带修的（要给移动端补一张 variant → Lingui 文案的表）。
> 新增错误变体时**别假设移动端会本地化它**。

反面教材是 `Identity`。它一度是配对路径的垃圾桶，兜着 8 处毫不相关的失败：peer_id 解析、
multiaddr 解析、二维码生成、邀请标识 hex 格式、`SecretKey` 未就绪、邀请状态没落盘、
设备找不到。于是**用户点「接受配对」失败时，看到的是一句「设备身份初始化失败」** ——
与真实原因毫无关系，把排查引向钥匙串，而真凶在数据库写入。

讽刺的是这个坑被发现过一次：`decode_pair_invite` 的注释明确写着「包成 Identity 会让用户看到
『设备身份初始化失败』」并改成了 `InvalidCode` —— **但只修了那一处**，同文件里另外 5 处照旧。
单点修复对这类问题无效，因为病根是「有一个万能 kind 可以塞」。

现在的划分（`crates/host/src/error.rs`）：

| kind | 什么时候用 | 用户看到 |
|---|---|---|
| `Identity` | 密钥材料**真的**读写失败 | 设备身份读写失败 |
| `IdentityNotReady` | 私钥还没加载进内存 | 请重启应用后重试 |
| `InvalidArgument` | 参数解析失败（peer_id / multiaddr / hex） | 通用兜底（用户无能为力） |
| `InvitePersistFailed` | 邀请「已消费」状态没写成库 | 请重新生成邀请 |
| `DeviceNotFound` | 找不到指定的已配对设备 | 未找到该设备 |

两条纪律：

1. **新增失败模式时先问一句「这句话对用户成立吗」。** 不成立就别复用那个 kind，
   哪怕它在类型上装得下。
2. **`From<AppError>` 的转换写成穷尽 match，不留 `_ =>` catch-all。**
   Web 侧那个 catch-all 会把每一个新 kind 默默显示成「文件传输失败，请重试」；
   改成穷尽之后，加变体会在编译期逼人想一下「浏览器该怎么说这件事」。

**改 kind 时三端一起改，否则就是同一个坑再踩一次。** 这次修完桌面的 5 处之后，移动端
`utils.rs` / `pairing.rs` / `device.rs` 还留着 4 处一模一样的误用（同一句「邀请标识格式非法」
在三端是三个 kind），Web 的 `revoke_invite_by_id` 则把它报成 `network`（用户看到
「网络连接出现问题，请稍后重试」）。**单点修复对这类问题无效** —— 病根是有个万能 kind
可以塞，而三端各有各的万能 kind。

> **仍未清的同型负债**：`AppError::Transfer` 已经是新的垃圾桶，规模比 `Identity` 大 ——
> `crates/web/src/opfs.rs` 全文件（浏览器存储失败）、`crates/web/src/inbox.rs`、
> `crates/core/src/host.rs`、`crates/transfer/src/flow/receive.rs` 都在用它兜「找不到条目」
> 「序列化失败」「内部不变量」。桌面把 `Transfer` 渲染成「文件传输失败，请重试」，
> 于是用户打不开一条收件箱记录时看到的就是这句。该补的是 `Storage` 与
> `NotFound { resource }`（Web 侧已有无人可达的 `WebError::NotFound` 正好接上）。

### crates/web 的 specta 导出不支持 `skip_serializing_if`

`swarmdrop-web` 的 TS 导出（`tests/specta_export.rs`）走 `specta_serde::Format`，
JS 可见类型（`crates/web/src/types.rs`）里给字段挂 `#[serde(skip_serializing_if)]` 会让
导出测试炸掉：`Invalid phased type usage ... unified mode cannot represent conditional omission`。

**正确做法**：可选字段用 `Option<T>` 恒序列化——TS 形状是 `T | null`，运行期
serde_wasm_bindgen 把 `None` 序列化成 `undefined`，JS 侧 `obj.field ?? fallback` 无感。

**相关文件**：`crates/web/src/types.rs`（`RelayInfoJson` 是样例）、`crates/web/tests/specta_export.rs`

## Clippy / dead_code

### 用 #[expect(...)] 替代 #[allow(...)]

项目里清一色用 `#[expect(clippy::xxx, reason = "...")]` 而非 `#[allow]`。Rust 1.81+ 的语义是：标了 expect 的 lint 一旦"自然消失"会反向报警，避免遗留的过期 allow。

**正确做法**：
```rust
#[expect(clippy::too_many_arguments, reason = "DB 写入需要完整上下文")]
pub fn insert_session(...) { ... }
```

**相关文件**：`crates/storage-sql/src/ops.rs`、`crates/transfer/src/flow/receive.rs`

## 数据库 schema 与迁移

> 2026-08-05 把 12 个增量迁移 squash 成了一份全量 init（`m20260805_000001_init`），
> 并且**整个 migration crate 零 `execute_unprepared`**。下面两条是那次的产物。

### schema 约束尽量写在 entity 上，migration 只负责「把它建出来」

sea-orm 2.0 的 entity 能表达的约束比直觉多，本仓 7 个索引里 6 个都能声明式表达：

| 属性 | 生成什么 | 本仓例子 |
|---|---|---|
| `#[sea_orm(indexed)]` | 单列非唯一索引，名字是 `idx-{table}-{col}` | `inbox_item.received_at` |
| `#[sea_orm(unique)]` | 单列唯一索引 | `inbox_item.transfer_session_id` |
| `#[sea_orm(unique_key = "名字")]` | **复合唯一索引**——同名的多个列合成一条 | `transfer_file` 的 `(session_id, file_id)` |
| `belongs_to` 上的 `on_delete = "SetNull"` / `"Cascade"` | 外键的删除行为 | `inbox_item.transfer_session` |

`db.get_schema_builder().register(E).apply(db)` 会把这些一并建出来，并按外键依赖**自动拓扑
排序**建表顺序（SQLite 不支持后加外键，顺序错就是硬错误，别手写顺序）。

**entity 唯一表达不了的是复合非唯一索引**（`indexed` 是列级属性、`unique_key` 只组唯一键）。
本仓只有一条 `(deleted_at, archived_at)`，用 `Index::create()` 的 sea-query DSL 补 ——
仍然不是裸 SQL。

**为什么值得较真**：约束只写在 migration 的裸 SQL 里、entity 不表达，会形成一种
**只在「从零建库」时才暴露的漂移**。本仓踩过：`inbox_items.transfer_session_id` 的
`ON DELETE SET NULL`——「清空传输历史不动收件箱」这条三端不变量的实现基础——此前只存在于
`m20260627_000002_drop_inbox` 的裸 SQL 里，entity 从未写过 `on_delete`。改用 schema builder
从 entity 建表的那一刻，这条约束会**静默消失**：不报错、不失败，只是删会话时行为变了。

判据：**凡是数据库强制的约束，entity 上必须读得到。**
钉法是行为级测试（真删一行看结果），不是解析 DDL 文本 ——
见 `m20260805_000001_init` 的 `deleting_a_session_nulls_the_inbox_link_instead_of_cascading`。

### squash 迁移会让所有存量库「启动即失败」，必须配自愈

sea-orm 的 `get_migration_with_status` 算 `已应用 − 代码里有的` 差集，非空就返回
`DbErr::Custom("Migration file of version '…' is missing, this migration has been applied
but its file is missing")`。这发生在任何 DDL 之前 ——
**库本身是好的，只是这份代码认不出它的历史**。但 `Migrator::up` 的错误直接冒泡到 setup，
所以表现不是「数据丢了」而是**应用打不开**。删迁移文件前必须想清楚这一条。

本仓的处理是 `migration::connect_and_migrate()`：连库 → 迁移 → 撞上这条错误就删库重建。
桌面与移动共用它（两端的启动路径本来逐字相同）。三个细节：

- **判据要窄**。只认 `DbErr::Custom` + 那句固定措辞。把任意 `Custom` 当成「该删库」，
  会让真正的迁移失败（写坏的 DDL、磁盘满、权限）变成一次静默的数据清除。
- **先关连接再删文件**。Windows 上打开中的文件删不掉，而「删了个寂寞又重连到同一个旧库」
  会再报同样的错 —— 变成启动死循环。
- **`-wal` / `-shm` 一起删**。journal 模式是写在库文件头里的持久设置，
  只删主文件而留下历史版本的 `-wal`，新库会读到一段本该消失的旧事务。

代价的边界值得写清楚（用户会问）：丢的是**这个库里的**传输历史、收件箱、邀请注册表；
设备身份与已配对设备在 keychain / `dev-identity.json` / 平台安全存储里，**配对关系不丢**，
已落盘的文件也不动。

### 目录式迁移**不能**用 `DeriveMigrationName` —— 它会把版本名变成 `mod`

`DeriveMigrationName` 展开成 `get_file_stem(file!())`。迁移写成单文件时没问题
（`m20260730_000001_pair_invites.rs` → 版本名就是文件名）；但**时间胶囊必须是目录 +
`mod.rs`**，于是 stem 变成 `"mod"` —— 而历史上每一个目录式迁移记进 `seaql_migrations`
的都是同一个 `"mod"`。

这次 squash 就踩了：新 init 与被删掉的旧 `m20260228_000001_init/mod.rs` **撞名**。
后果静默且致命 —— 停在 v0.3.3 ~ v0.4.2 的库里只有一行 `mod`，升级后
`migration_in_fs` 与 `migration_in_db` 都是 `{mod}`，pending 空、missing 也空，
`Migrator::up` 返回 `Ok(())`，[`connect_and_migrate`] 的自愈**永不触发**，
应用继续跑在 2026-02 的两表 schema 上，第一次查收件箱就 `no such table`。

**正确做法**：目录式迁移手写名字。

```rust
pub struct Migration;   // 不要 #[derive(DeriveMigrationName)]

impl MigrationName for Migration {
    fn name(&self) -> &str { "m20260805_000001_init" }
}
```

**两类测试都发现不了它**，别指望：空库上跑 `Migrator::up` 一切正常；往
`seaql_migrations` 塞一条「未来版本」也只覆盖到 missing 那条分支。要钉住它需要
**按古董库的真实形态**造数据（塞 `('mod', 0)` 且不建任何本版表），见
`connect_and_migrate_rebuilds_ancient_database_named_mod`。

### `crates/migration` 的冻结 entity 快照不要改成 `use entity::...`

`m20260805_000001_init/entity/` 是**时间胶囊**：主 crate 的 entity 之后怎么演进，这个迁移
建出来的表都不变。直接引用主 entity 会让「从零建库」跟着最新 entity 走、而「增量升级」走
历史路径，两条路建出不同的 schema。这是 sea-orm 官方对「migration 里用 SchemaBuilder」的
既定要求。

推论：**加了新列或新表，不要回头改 init 的快照**，写一个新的增量迁移。

## P2P / 异步

### 启动顺序：plugin → updater → database → start command

`src-tauri/src/setup.rs` 里 plugin 在 Builder::default() 注册；updater + database 在 setup() hook 里初始化并注入 Tauri state。**P2P 节点不在启动期自动起**——前端调 `commands::start()` 才 bind `Endpoint` 并创建 `NetManager`（内含 `PairingManager`）。

**相关文件**：`src-tauri/src/setup.rs`、`src-tauri/src/lib.rs` 的 `start` 命令

### 断点续传恢复走 Probe → Commit → Ack

`redesign-transfer-lifecycle` 已废弃旧的 `ResumeRequest` / `ResumeOffer` 双入口恢复路径。恢复统一走：
`ResumeProbe` 获取对端 phase / epoch / manifest / checkpoint / source fingerprint，再由发起方发送
`ResumeCommit { new_epoch, key, fetch_plan }`，对端校验后返回 `ResumeAck`。

**正确做法**：
- `new_epoch = max(local_epoch, peer_epoch) + 1`；Coordinator 只接受 `new_epoch > current_epoch`
- `ResumeReport` 必须携带 manifest 与 terminal_reason，才能区分 cancelled / fatal_error / source_modified
- 被动端 `ResumeCommit` 校验通过后按本端 direction 重建 `SenderActor` 或 `ReceiverActor`，再经 `NetworkSignal::ResumeCommitted` 转 active
- `ResumeRejectReason::PeerUnavailable` 不改本地状态，保留 suspended 供稍后重试；cancelled/source/checkpoint/session 缺失按语义转 terminal

**不要做**：
- 不要再新增 `ResumeRequest` / `ResumeOffer` 分支，旧路径会绕过 probe 阶段导致两端恢复事实不一致。
- 不要直接调用 `mark_session_transferring` 恢复；phase/epoch 必须由 Coordinator 写入并发布 projection。

**相关文件**：`crates/transfer/src/flow/resume/mod.rs`、`crates/core/src/protocol/`、`crates/core/tests/e2e_transfer.rs`

### 主动取消必须通知对端并写 cancelled

取消不是本地停止任务：本端要取消 live session、通知对端 `TransferRequest::Cancel`、写入 DB `Cancelled`，对端收到后也要标记 cancelled 并发出友好的 UI 提示。

**正确做法**：
- 发送方 `cancel_send` 也要像接收方一样发送 `Cancel`，不能只 `session.cancel()`
- 发送方 `waiting_accept` 还没有 `SenderActor`，必须通过 `outbound_offers` 记录并在 Offer 异步返回后撤回，避免对端已接受后继续隐藏传输
- 取消状态写入放在 `crates/core`，Tauri / RN host 只做薄命令封装
- 前端收到 `TransferFailedEvent` 中的 `对方取消` 时按 info toast 展示，不按错误处理

**相关文件**：`crates/transfer/src/flow/send.rs`、`crates/transfer/src/flow/receive.rs`、`src/stores/transfer-store.ts`

### transfer-data Finish 只是信号，完成事实必须由接收端证明

`TransferDataFrame::Finish` 只能表示发送端认为 fetch_plan 已写完，不能直接驱动本地
`Completed`。接收端必须用初始 checkpoint bitmap + 本次收到的 `BlockData` bitmap 证明所有非零文件
都完整后，才允许 finalize sink、`mark_file_completed`、`mark_session_completed` 并发布 projection。

**正确做法**：
- 首次传输调用处显式传 `full_fetch_plan(...)`；`SenderActor::run_data_channel` 把传入的 `fetch_plan` 当精确计划，不把空计划隐式扩展成全量。
- data-channel 接收端收到 `Finish` 后先跑完整性校验；缺块/缺 bitmap 走 `Interrupted`，不能把 bitmap 补成全完成。
- 零字节文件可在完成阶段创建空 sink 并标记完成；非零文件没有 live sink 时只能依赖已完整的恢复 checkpoint，不能创建空文件冒充完成。
- 当前 libp2p stream 是可靠有序流，`BlockRequest` 只保留协议帧位；生产路径不要假装支持同流重传，解密/范围错误应中断。

**相关文件**：`crates/transfer/src/{actor/sender.rs,actor/receiver.rs,wire/data_frame.rs}`

### 测试需显式声明 tokio rt-multi-thread，别靠 workspace feature unification

`#[tokio::test(flavor = "multi_thread")]` 需要 tokio `rt-multi-thread`，而某些 crate 的
`[dependencies] tokio` 只开了 `rt`。测试能编过往往是靠 workspace feature unification
（其他成员把 feature 带进来）——**一旦单独 `cargo clippy -p <crate> --all-targets` 或
单独构建，就会报 `runtime flavor multi_thread requires rt-multi-thread`**。

**正确做法**：在该 crate 的 `[dev-dependencies]` 里显式声明 `tokio = { features = ["rt-multi-thread", "time", "macros"] }`，不要依赖 unification。移动端单独复用 core 时同理。

### mobile-core 从不自建 tokio runtime —— 所有 spawn 挤在 async-compat 的单线程上

**实测**（2026-07 iroh 调研副产物，与迁不迁 iroh 无关，是我们既有代码的缺陷）：

```
grep -rnE "Runtime::new|runtime::Builder|new_multi_thread|block_on|Handle" \
  mobile/packages/swarmdrop-core/rust/mobile-core/src/          →  0 命中
```

mobile-core 从不自建 runtime，全靠 uniffi `async_runtime = "tokio"` 背后 async-compat 的那个
全局 TOKIO1 —— **一根 `new_current_thread` 的专用线程**（线程名 `async-compat/tokio-1`）。
而 `mobile-core/Cargo.toml` 明写 `tokio = { features = ["rt-multi-thread", "sync", "macros"] }`：
**feature 开了，多线程 runtime 一个也没造。**

⚠️ **修法不是「在 core 构造时建一个 multi-thread Runtime 并持有 Handle」—— 机制上不成立**：
`Handle::try_current()` 读的是**当前线程**的线程本地上下文，把 Runtime/Handle 存成字段
不会让轮询线程进入该 runtime 的上下文，`try_current()` 依旧 Err、依旧回落 TOKIO1。
要真正绕开必须让轮询线程**进入上下文**（每次调用里 `handle.enter()`）或把活儿显式
`handle.spawn(..)` 出去。

**相关文件**：`mobile/packages/swarmdrop-core/rust/mobile-core/{Cargo.toml,src/}`

### libp2p-request-response 锁在 0.29.0：失败变体带 `connection_id`，别照抄网上旧签名

`Cargo.lock:4283-4284` 锁的是 **libp2p-request-response 0.29.0**（libp2p 0.56.0 在 `:3910-3911`）。
该版本的变体是 `InboundFailure { peer, connection_id, request_id, error }` —— **有 `connection_id` 字段**，
照抄网上旧签名（无 `connection_id`）会编译不过。

错误模型是 `InboundFailure` / `OutboundFailure` 两分，经 Behaviour 的 Event 流报上来，
**在事件循环里集中处理**（与「handler 就近 `?` 传播」的模型相反——迁移时这层的重试/统计要自己接手）。

**相关文件**：`crates/core/src/network/event_loop.rs`、`Cargo.lock`

### 传输生命周期：Coordinator reducer + 增量过渡（phase/reason 与旧 SessionStatus 并存）

`redesign-transfer-lifecycle` 把传输状态从扁平 `SessionStatus`（5 态）重构为 `phase`（offered/waiting_accept/active/suspended/terminal）+ `suspended_reason`/`terminal_reason` + `epoch` + `recoverable`。采用**增量过渡**：新字段与旧 `SessionStatus` 列并存、逐步迁移、最后删旧——每步编译通过、不破坏现有传输系统。

**正确做法**：
- 状态机核心是纯函数 reducer（`transfer/coordinator.rs::reduce`）：`(state, input) → Some(new)/None`，无 DB/网络依赖，可独立单元测试（epoch 校验、terminal 不可逆都 hoist 到这一层）。`TransferCoordinator::dispatch` 才做 I/O（load→reduce→persist）。
- **过渡期 status 与 phase 必须同步**：`apply_transition` 写 phase 时经 `TransferPhase::legacy_status(terminal_reason)`（entity 单一映射来源）一并写旧 `status`，否则 coordinator 转换后前端旧路径读到滞留状态。这是 simplify altitude review 抓到的漂移坑。
- `dispatch` 已 load 的 Model 直接传给 `apply_transition(&Model, ...)` 用 `into_active_model` 更新，**不要**在 apply 里二次 `find_by_id`（省一次 SELECT）。
- migration 加列用 `ALTER TABLE ... ADD COLUMN ... NOT NULL DEFAULT ...`；开发期 `DELETE FROM transfer_files/transfer_sessions` 清空旧历史（design 允许），避免处理旧行默认值。
- sea-orm 2.0 entity 用 `ActiveModel::builder().set_xxx()`；加 NOT NULL 字段后在 `create_session` 补 `.set_phase/.set_epoch/.set_recoverable`，未 set 字段走 DB default（builder 不强制）。

**相关文件**：`crates/transfer/src/coordinator.rs`、`crates/storage-sql/src/ops.rs`（apply_transition/projection）、`crates/entity/src/lib.rs`（`TransferPhase::legacy_status`）

### 接线 mark_session_* → Coordinator：本地/对端 reason 区分 + 取消优先于 error

**所有 session 级 phase 转换一律走 `coordinator.dispatch` → `reduce` → `apply_transition`，禁止 `mark_session_*` 直写终态**（`cleanup-transfer-tech-debt` 轮1 收口）。文件级副作用（`mark_file_completed`/finalize/checkpoint/inbox 索引）在 dispatch **之前**完成，但 session 终态由 reducer 统一写。早期「complete/fail 走 mark_* + `publish_projection`」是迁移没收口的过渡写法，已废弃。

**正确做法**：
- **完成/失败/拒绝走 dispatch**：收发完成→`dispatch(Actor{epoch, Completed})`；接收方校验失败→`dispatch(Actor{epoch, FatalError(msg)})`；策略拒绝→`create_session(offered)` 后 `dispatch(User{Reject})`。`epoch` 用 actor 自己的 epoch——旧 epoch actor 在 resume 后才完成会被 epoch 守卫忽略。
- **完成事件/收件箱索引 gate 在 dispatch 返回 `Some`**：`is_terminal` 守卫让先到的终态获胜、迟到的被拒（reduce 返回 None），所以被取消/旧 epoch 抢先时不发 `TransferCompleted`/不建 inbox。这修了「取消后并发完成把 cancelled 覆盖成 completed」的竞态 bug（回归测试 `e2e_terminal_irreversible_under_concurrent_complete_cancel`）。
- **本地 vs 对端 reason 必须区分**：本地操作→`User{Pause/Cancel}`（写 LocalPaused/Cancelled）；对端发来的 Pause/Cancel→`Network{RemotePaused/RemoteCancelled}`（写 RemotePaused）。
- 入站控制消息（req_resp 的 Cancel/Pause）当前**不携带 epoch**，用 `dispatch_network_current`（读 session 当前 epoch 再 dispatch，等价无 stale 保护）；待数据面帧协议带 epoch 后收紧。
- **`publish_projection` 只剩「新建会话首投影」一个合法用途**（offered/waiting_accept，创建不是 reduce 输入、没有 from-state）；状态转换的 projection 由 dispatch 在 reduce 成功后自动发，别再用它给终态补投影。
- **对端断连 → `Network{Interrupted}`**：event_loop 在 `NodeEvent::PeerDisconnected` 调 `IncomingTransferRuntime::handle_peer_disconnected(peer)`（新 trait 方法，默认 no-op），impl 里 `find_active_session_ids_by_peer` + 取消内存会话 + dispatch。发送端会话本就 idle（只应答 ChunkRequest），**只能靠这个 hook 感知断连**。
- **取消优先于 error**（`actor/receiver.rs::run_data_channel` 收尾）：被 `cancel_token` 取消的传输即使有 in-flight chunk 错误也返回 `Ok(false)`（取消），**不能**先判 error 返回 Err——否则断连/取消 teardown 时的 chunk 错误会触发 `fail_session` 写 terminal/failed，盖掉 Interrupted/Cancelled。这也修了「主动取消时若 chunk 报错会变 failed」的潜在 bug。
- **`transferred_bytes` 是 projection 派生、不在 session 列维护**（`cleanup-transfer-tech-debt` 轮4）：`get_transfer_projection` 直接 `SUM(files.transferred_bytes)`；文件级进度由 `persist_chunk`（接收）/ `save_sender_file_progress`（发送）增量落库。**不要**在 pause/cancel/disconnect 前手工把文件进度 sync 回 session 列（已删 `sync_session_transferred_bytes`），那是会漂移的二次写。
- **发送终态副作用归 actor**：发送完成/中断的 dispatch + 落库 + 完成事件下沉到 `SenderActor::on_completed`/`on_interrupted`（与接收 `finish_data_channel`/`fail_session` 对称），`wire/data_plane` 只做纯路由 + `remove_send_if_epoch` 注册表簿记（按 epoch 移除防旧任务误删 resume 后新 actor）。

**相关文件**：`crates/transfer/src/{coordinator.rs,flow/receive.rs,actor/receiver.rs,actor/sender.rs,flow/send.rs,wire/data_plane.rs}`、`crates/core/src/network/event_loop.rs`、`crates/storage-sql/src/ops.rs`、`crates/core/tests/e2e_transfer.rs`（remote-reason / peer-disconnect / sender-resume 确定性测试）

### 桌面启动清理只保留过期文件清理，active 统一交给 Coordinator

`src-tauri` 启动时不再把 sender/receiver transferring 会话直接改成 failed/paused。`setup.rs` 先注入
`TauriEventBus`，再调用 `cleanup_stale_sessions(db, event_bus)`；该函数用
`TransferCoordinator::cleanup_recoverable_sessions` 把所有 `phase=active` 会话转为
`suspended/app_restarted` 并发布 projection。

**正确做法**：
- `cleanup_stale_sessions` 的 active 清理只经 Coordinator，不直接写 `status`
- 桌面特有的 `.part` 文件清理仅用于过期 receiver suspended 会话，清理后调用 `mark_session_failed`
- `start` 命令复用 managed `TauriEventBus`，不要在 setup 已注入后再创建一个不共享 prepare channel 的 bus

**相关文件**：`src-tauri/src/{setup.rs,database.rs,commands/lifecycle.rs}`

### 前端传输列表只消费 TransferProjection

`redesign-transfer-lifecycle` 中，前端传输列表、详情页和设备页不再读取
`get_transfer_history` / `get_transfer_session`，也不再维护 `sessions + dbHistory`
双状态源。后端提供 `get_transfer_projections` 和 `"transfer-projection-update"` 事件，
前端 store 只保存 `projections`、`progressBySession`、`pendingOffers`。

**正确做法**：
- 发送方 `start_send` 时就在 core 内创建 `phase=WaitingAccept` projection；接收方收到 Offer 时创建 `phase=Offered` projection
- 用户接受/拒绝、对端接受/拒绝、暂停/取消/恢复都经 Coordinator 转换 phase/reason 并发布 projection
- `transfer-progress` 事件只允许更新 projection 的进度字段，不允许前端据此推断 completed/failed/suspended
- UI 文案统一从 `TransferProjection.phase + reason` 派生，避免列表、详情和两端设备各自解释旧 `SessionStatus`

**不要做**：
- 不要在前端重新拼接 active sessions 与 DB history
- 不要在发送/接收页面手工构造 transient transfer session；如果缺等待态，优先让后端补 projection

**相关文件**：`crates/transfer/src/{flow/send.rs,flow/receive.rs,coordinator.rs}`、`crates/storage-sql/src/ops.rs`、`src/stores/transfer-store.ts`、`src/lib/transfer-projection.ts`

### ActorRegistry 是运行时 actor 唯一入口

SenderActor / ReceiverActor 的内存生命周期不再散落在 `TransferManager` 的裸
DashMap 操作里。`ActorRegistry` 统一管理创建、替换、移除、取消和 epoch 准入；
Coordinator 负责 DB 状态，ActorRegistry 负责内存 actor 唯一性。

**正确做法**：
- 插入 actor 必须带 session epoch：首传 epoch=0，ResumeCommit 后使用 `new_epoch`
- 同 epoch 或旧 epoch 的 actor 插入会被拒绝并取消；更高 epoch actor 会取消并替换旧 actor
- ReceiverActor 后台任务结束时按 `(session_id, epoch)` 移除，不能只按 session_id 移除，避免旧任务结束误删恢复后的新 actor
- 业务代码通过 `TransferManager::{get,insert,remove}_{send,receive}_session` helper 访问 actor，不直接碰 registry 内部 map

**不要做**：
- 不要新增 `send_sessions` / `receive_sessions` 裸 map 操作
- 不要在 resume/data-channel 路径创建 actor 时漏传 new_epoch

**相关文件**：`crates/transfer/src/actor/registry.rs`、`crates/transfer/src/{manager.rs,flow/send.rs,flow/receive.rs,flow/resume/mod.rs}`

### crates/core 端到端集成测试：两个真实节点 + MemoryHost + sqlite::memory（不需要 Tauri/真机）

完整传输链路（offer→transfer→pause→resume→cancel）可在纯 `cargo test` 里跑通，**零生产代码改动**。
`libp2p-swarm-test` **不适用**——它测 raw `Swarm` + 自定义 `NetworkBehaviour`，和本项目
`Endpoint` 的封装层级对不上。正解是「两个真实 `Endpoint` + 关 mDNS + 显式建连」。

**harness 已实现，直接读 `crates/core/tests/e2e_transfer.rs`（约 1600 行）扩展**，
里面有 `test_endpoint` / `spawn_node` / `connect` / `connected_paired_pair` 四个现成 helper。
下面只记那些**读代码看不出来、踩过才知道**的约束：

**正确做法**：
- **关 mDNS + 显式建连消除时序**：`Endpoint::builder().mdns(false).relay_client(false)
  .listen(["/ip4/127.0.0.1/tcp/0"])`。两个本机节点若靠 mDNS 自动发现会互相串扰状态，
  必须 `endpoint.add_addrs(peer, [addr])` + `endpoint.connect(NodeAddr::new(peer))`。
- 端口用 `/ip4/127.0.0.1/tcp/0`（OS 分配），建连前先轮询拿到实际绑定的 listen addr。
- 断言分两路：`MemoryHost.events()` 查发出的 projection / Transfer\* 事件；`db` 查
  phase/epoch/checkpoint 验状态机。中断模拟 = drop 一侧 event_loop task；
  重启 = 用同一 `db` 重新 spawn 节点（内存库单连接钉死即可跨重启保活，不需要 tempfile）。

**不要做**：
- 不要忘**双向** `is_paired`：Offer 要求已配对，两侧都要互相塞 `PairedDeviceInfo`，
  否则直接被 `OfferRejectReason::NotPaired` 拒。`is_paired` 唯一运行时依据是
  `PairingManager` 的内存 DashMap，**不查 DB / keychain**。
- **连接判定不要用 `connected_count()` / `NetworkStatus::connected_peers`**：它们额外要求
  identify 把 `agent_version` 分类成 SwarmDrop 客户端（`OsInfo::is_swarmdrop_agent`），
  测试的 agent_version 不匹配会**恒为 0**。改用 `manager.devices().is_connected(&peer_id)`
  （只看裸 `PeerConnected`）。
- **建连在并行 `cargo test` 下会瞬时失败**：多组节点同跑抢 CPU 时，到 `127.0.0.1:port` 的
  连接尝试会瞬时失败，单次 `connect().expect()` 是 flaky 的（串行 `--test-threads=1` 不复现，
  但 CI 默认并行）。`connect` helper 必须**重试到 `is_connected` 双向为真**、忽略单次错误
  ——连接才是目标，不是单次调用成功。
- **不要在同步谓词里 `block_on` async DB 查询**：`#[tokio::test]` 已在 runtime 上，
  嵌套 runtime 会 panic（"Cannot start a runtime from within a runtime"）。DB 等待写原生
  async 轮询循环，只有连接/事件这类同步状态才用同步谓词轮询。

**相关文件**：`crates/core/tests/e2e_transfer.rs`、`crates/core/src/host.rs`（MemoryHost）、
`crates/core/src/runtime.rs`（`start_node` 组合根，测试即复刻它）

### LAN Helper 三节点测试需要真实私有网卡地址

`auto-discover-lan-helper-nodes` 的三节点集成测试会启动 A/B 普通节点和 C LAN Helper，
用 mDNS + Identify 事件把 C 注册为 infrastructure peer，再通过 Kad record 验证 A 写、B 读。
由于生产逻辑会过滤 loopback 和 link-local，测试不能只监听 `127.0.0.1`，否则无法覆盖真实
LAN Helper 路径。

**正确做法**：
- 测试用 `if-addrs` 枚举 operational up、非 loopback、非 p2p 的私有 IPv4 网卡并绑定 `/ip4/<private>/tcp/0`
- 找不到可绑定私有 IPv4 时打印说明并跳过真实 LAN 流程，避免无网卡 CI 假失败
- LAN Only 测试要额外断言 `NetworkStatus.candidate_sources` 不包含 `BuiltInPublic`

**相关文件**：`crates/core/tests/e2e_lan_helper.rs`、`crates/core/Cargo.toml`

## 配对安全

### `PairingMethod::Direct` 的授权依据是 mDNS 观测，不是对端自报地址

Direct（局域网点击配对，`/devices` 页「连接」按钮）没有配对码做凭证，唯一的授权依据是
**「对端确实和本机在同一局域网」**。这个判据由 `DeviceManager::is_lan_discovered()` 提供，
而它成立**完全依赖一个隐式前提**：`PeerInfo.addrs` 的唯一写入来源是
`NodeEvent::PeersDiscovered`（mDNS 多播实际观测到的地址）。

`handle_event` 的 `IdentifyReceived` 分支**只取 `agent_version`、用 `..` 忽略 `listen_addrs`**——
这不是疏漏，是安全前提：identify 里的 `listen_addrs` 是对端**自报**的，远程攻击者谎报一个
`192.168.x.x` 就能冒充同网段设备。

**正确做法**：
- Direct 的校验必须在 `event_loop` 的 `publish(PairingRequestReceived)` **之前**——否则任意远程
  peer 都能靠一个 Pairing 请求让本机弹窗 + 推系统通知（骚扰面），且 UI 上的设备名完全由对端
  `os_info` 自报，用户正等着某台设备时很可能直接点接受。
- 拒绝时**不回响应**，不向扫描者泄露本机是否在线。
- `handle_pairing_request` 里对 `PairingMethod` 必须用**穷尽 match**。原先是
  `if let PairingMethod::Code { code } = method`，导致 `Direct` 静默 fall-through 到无条件
  `paired_devices.insert`——**新增任何变体都会自动获得一条免校验的配对通道**。穷尽 match 强制
  每个变体对「凭什么信任对方」表态。
- `cache_inbound_request` 是 `pending_inbound` 的唯一写入口且只被 event_loop 调用，所以那道
  校验是单点且充分的，`manager` 层不需要（也拿不到 `DeviceManager` 引用去）复查。

**不要做**：
- **不要让 `IdentifyReceived` 分支消费 `listen_addrs` 写进 `addrs`**。回归测试
  `self_reported_identify_addrs_must_not_grant_lan_status` 会失败——那不是测试过时，是重新打开了
  配对绕过漏洞。（已用 mutation 验证：注入该改动后测试精确失败。）
- 不要用 `connection_info()` 判断是否局域网——它在 `hole_punched == true` 时直接返回 `Dcutr`，
  会掩盖 `Lan`。要用 `infer_connection_type(&addrs)`（`has_lan` 优先）。

**相关文件**：`crates/core/src/device_manager.rs`（`is_lan_discovered` + tests）、
`crates/core/src/network/event_loop.rs`（入站校验）、`crates/core/src/pairing/manager.rs`（穷尽 match）

### 发布到公共 DHT 的记录不得携带设备信息

`OnlineRecord`（presence 在线宣告）的 key = `SHA256("/swarmdrop/online/" ‖ peer_id)`——peer_id 是公开的，
所以**任何加入网络的节点都能算出这个 key 并读取记录**，且记录无签名。它只能携带「让已配对设备拨得通」
所必需的地址。

`os_info` 是历史遗留的**死字段**：写入端发 `OsInfo::default()`（含 `COMPUTERNAME`/`HOSTNAME`，常含真名），
而读取端（`supervisor.rs` 的重探路径）只用 `dialable_addrs()`，**从不消费它**——等于每 150 秒
（`ONLINE_RECORD_TTL_SECS / 2`）向公开 keyspace 广播一次主机名，零收益。

**正确做法**：
- 发 `OsInfo::redacted()`（全空占位），**不要**发 `OsInfo::default()`
- 记录构造抽成纯函数 `build_online_record()`，让「不含设备信息」这条约束可被单测锁住
- 换 iroh **不自动解决**这类问题（pkarr 同样是「知道 EndpointId 就查得到地址」）——这是产品层的
  可见性设计，不是传输层能代劳的

**不要做**：
- **不要直接删掉 `os_info` 字段**。`OsInfo` 的 `hostname`/`os`/`platform`/`arch` 都没有
  `#[serde(default)]`（只有 `name`/`capabilities` 有），删字段会让存量客户端反序列化**整条记录**失败
  → 连 `direct_addrs` 一起丢 → 退化成盲拨。发空值则 wire 格式不变、存量客户端零影响。
  字段本身随 presence 重写（改为「只对已配对设备可见」）时一并移除。
- 不要把 `OsInfo::default()` 改回来。回归测试 `online_record_must_not_carry_device_info` 会失败——
  那不是测试过时（已用 mutation 验证：注入 `redacted()`→`default()` 后测试精确失败）。

**相关文件**：`crates/host/src/device.rs`（`OsInfo::redacted`）、
`crates/core/src/presence/supervisor.rs`（`build_online_record` + 测试）、`crates/core/src/presence/mod.rs`

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

## 系统托盘

### 三态托盘图标要拿到 `TrayIconBuilder::build()` 的返回值，且不能用 `icon_as_template`

`TrayIconBuilder::build(app)` 返回 `tauri::Result<TrayIcon<R>>`——早期实现里这个返回值被直接丢弃（`builder.build(app)?;`），导致后续没有句柄可以调 `set_icon` 动态换图标，只能在创建时定死一次。要支持运行时切换图标，必须把这个返回值存进长期持有的状态（本项目是 `TrayState`），否则和 `MenuItem` 句柄一样会因为没人持有导致效果消失。

另外，`icon_as_template(true)`（macOS 的单色模板图标，跟随系统深浅色自动着色）和"用颜色区分状态"是互斥的——template 图标会被系统强制去色成单色轮廓，图标本身的颜色信息不会显示。如果三态要靠颜色区分（而不是纯形状区分），三个平台都不能用 template 模式，直接传全彩 PNG。

**正确做法**：
```rust
let tray_icon = builder.build(app)?;  // 存返回值，不要丢弃
app.manage(TrayState { status_item, pause_item, tray_icon });

// 运行时切换：
match tauri::image::Image::from_bytes(png_bytes) {
    Ok(icon) => { let _ = state.tray_icon.set_icon(Some(icon)); }
    Err(e) => warn!("托盘图标解码失败，保留上一次的图标: {e}"),
}
```
- `Image::from_bytes` 需要 `image-png`（或 `image-ico`）feature，本项目 `Cargo.toml` 已开。
- 三态图标用 `include_bytes!` 编译期嵌入（避免运行时文件路径依赖），放 `src-tauri/icons/tray/`。
- 状态图标设计手法：复用品牌 logo 本身的双色剪影结构（不额外加徽章/角标、不整体去色），每个状态只换配色（比如离线态灰调、正常态品牌色、警示态琥珀色），形状不变——比在图标上叠加小圆点更耐小尺寸（22×22 菜单栏图标叠角标很容易糊）。
- 状态→(文案, 图标) 的派生只写一处：用一个 `TrayStatus` 枚举 + `from_flags(online, paused)` 做唯一的三分支匹配，`text()`/`icon_bytes()` 挂在枚举上；不要在 `status_text` 之外再单独写一个结构相同的 `match (online, paused)` 去选图标字节，两份独立的三分支匹配迟早会在加新状态时漏改一处。
- 图标解码失败的处理分两种场合：`build_tray` 里的初始图标字节是编译期常量，解码失败只可能是资产本身损坏，属于构建期 bug，应该让它经 `?` 直接让托盘创建失败、快速暴露问题；`refresh_tray` 没有 `Result` 可传播，退化成 `warn!` 日志 + 保留上一次图标，不要用 `if let Ok(..)` 静默吞掉（本文件其它地方对真正无害的操作才用 `let _ = ...`，图标解码失败不算无害）。

**相关文件**：`src-tauri/src/tray.rs`、`src-tauri/icons/tray/{online,offline,paused}.png`

## 依赖升级

### 判断"是否真落后"看 Cargo.lock 解析版本，不看 requirement 字面

很多依赖写宽松约束（`tauri = "2"` / `axum = "0.8"` / `tokio = "1.49"`），`cargo update` 早把它们解析到最新。真正需要动手的只有被版本号**卡住**的（major / 0.x 跨段）。审计方法：`Cargo.lock` 实测解析版本 + crates.io max_stable 对比，再用 `cargo tree -i <crate>@<ver>` 看旧版来源。

**正确做法**：
- 区分"直接依赖解析到最新"（无需动）与"requirement 上限低于最新"（要改 Cargo.toml）。
- 多版本并存常见且无害：`sha2 0.10` / `chacha20poly1305 0.10` 等旧版由 `libp2p → ed25519-dalek` 等传递依赖钉住，**无法与我们直接依赖的 0.11 统一**，只增编译体积、不冲突。`cargo update -p sha2 --precise X` 会因多版本报 `ambiguous`，属正常。

### RustCrypto 0.11 波（chacha20poly1305 / sha2）：aead::OsRng 移除 → Generate trait

升级 chacha20poly1305 0.10→0.11（aead 0.5→0.6）时**唯一硬编译错误**是 `chacha20poly1305::aead::OsRng` 不再 re-export（rand_core 升 0.10，OS 随机改走 getrandom）。

**正确做法**：
- 随机 key 生成改用 `Generate` trait（无 rng 参数、getrandom 后端）：
  ```rust
  use chacha20poly1305::aead::{Generate, Key};
  Key::<XChaCha20Poly1305>::generate().into()  // -> [u8; 32]
  ```
- `XNonce::from_slice(&nonce)` 在 hybrid-array 下已 deprecated，`-D warnings` 会变硬错误 → 改 `&XNonce::from(nonce)`（`[u8;24]` 走 `From<[u8;N]>`）。
- `XChaCha20Poly1305::new(key.into())`、`Sha256::digest(...).to_vec()` **无需改**：hybrid-array `Array` 仍 `Deref<[u8]>` + 提供 `From<&[u8;N]>`。SHA256 摘要值逐字节不变 → DHT key 兼容旧节点。
- 这俩同属 RustCrypto 协调波，**一起升**避免 generic-array/hybrid-array 长期并存。需 edition 2024 / MSRV 1.85（本仓已满足）。

**相关文件**：`crates/transfer/src/protocol.rs`（传输加密）、`crates/net/src/dht.rs`（DhtKey）

### rmcp 1.x→2.0：类型改名 + streamable-HTTP 新增 Host/Origin 白名单

src-tauri 是 rmcp 唯一直接依赖方（`tauri-plugin-mcp-bridge` **不**依赖 rmcp），升 2.0 无传递冲突。编译期改动仅两处机械改名：

**正确做法**：
- `rmcp::model::Content::text(..)` → `ContentBlock::text(..)`（v2 把 `Content` 改名 `ContentBlock`）。
- `RawResource::new(..).…​.no_annotation()` → 直建 `Resource::new(..).with_description(..).with_mime_type(..)`（v2 删除 `Annotated<T>`/`AnnotateAble`/`RawResource`）。
- 宏 `#[tool]`/`#[tool_router]`/`#[tool_handler]`、`ServerHandler` trait、`StreamableHttpService`、`Parameters<T>`、axum 0.8 兼容性**均不变**。

**注意（运行时、非编译）**：v2 给 streamable-HTTP 加了 DNS-rebinding 防护，`StreamableHttpServerConfig` 新增 `allowed_hosts`（默认含 `127.0.0.1`/`localhost`/`::1`）和 `allowed_origins`。本地绑定通常无碍，但升级后需冒烟连一次 MCP client；被拒就显式放行。v2 还含 streamable-HTTP session leak 安全修复（#934）。

**相关文件**：`src-tauri/src/mcp/{tools,resources,server}.rs`

### keyring 3.x→4.x：feature 体系重构为 v1 facade（旧 feature 全删）+ 仅 release 可验证

keyring 4.x 不是无脑 bump：把后端拆成 `keyring-core` + 各平台独立 store crate，默认 `v1` feature 按 target 自动 set_default_store。**旧 feature（apple-native/windows-native/linux-native-sync-persistent/crypto-rust/vendored）全部移除**，保留会编译失败。

**正确做法**：
- 删掉三个 `[target.'cfg(...)'.dependencies]` keyring 块，合并为 `[dependencies]` 单行 `keyring = "4.1.2"`（不要再写 default-features=false 或旧 feature 名）。
- `keychain.rs` 源码**零改动**：`Entry::new` / `set_secret`/`get_secret` / `set_password`/`get_password` / `delete_credential` / `KeyringError::NoEntry` / `{error}` Display 全兼容。`pub mod keychain` 无条件编译，故 debug `cargo check` 即可覆盖；release cfg 工厂分支用 `cargo check --release` 坐实。
- Linux 后端由 dbus-secret-service 换纯 Rust zbus，不再链接 libdbus/OpenSSL；`release.yml` 无 keyring 专属 apt 依赖、无需改。

**验证盲区（务必真机）**：keyring 仅 release build 生效（debug 走 `file_keychain`）+ macOS ad-hoc 签名进程被 Keychain 拒读 → `cargo test`/`pnpm tauri dev` **覆盖不到真实路径**，编译通过≠功能正确。必须出签名 release 包在三平台手测身份读写 + 重启 PeerId 稳定。跨版本 store 实现全换，老用户旧条目可能读不到 → 走"找不到即重建"（[见上 keychain 段]）→ PeerId 重置需重新配对，release note 要提示。

**相关文件**：`src-tauri/src/host/keychain.rs`、`src-tauri/src/host.rs`

### 桌面「用本应用打开文件」（share-target 入口）：文件用 Tauri fileAssociations（按扩展名），别用 public.data 通配

**macOS「打开方式」只显示声明了「与该文件 UTI 具体匹配」的 app，不显示只声明通用 `public.data`/`public.item` 的 app**（Apple 论坛 + 实测：xlsx/md/sql 都被"归属抑制"压掉，只有无归属的随机文件才偶尔显示）。macOS 15.4+ 里 `public.data` 单条目还行、多加几条 UTI 还会触发 Gatekeeper。所以**「声明 public.data 覆盖任意文件」在 Open With 上根本走不通**——这是 macOS 设计、不是实现问题。真·任意文件只能走原生 Share Extension（Tauri 不脚手架，重活，单独立项）。

**正解：文件用 Tauri 官方 `bundle.fileAssociations`（按 `ext` 扩展名列表）**——为每个扩展名生成具体 UTI 声明，Open With 可靠显示，且三平台注册由 Tauri 统一生成（macOS CFBundleDocumentTypes+LSHandlerRank / Windows 注册表 / Linux .desktop MimeType）。用 `role=Viewer`+`rank=Alternate`（出现但不抢默认）。代价：只覆盖列举的扩展名（列一批广的即可：Office/文档/图片/视频/音频/压缩/代码…），极冷门/无扩展名文件不显示。
- ⚠️ Tauri 曾漏生成 `LSHandlerRank`（issue #13159）导致 macOS 不进 Open With，需 **tauri ≥ 2.6 左右**（本仓 2.11.3 已含修复）。
- **别再自定义 `src-tauri/Info.plist` 塞 CFBundleDocumentTypes**：会与 Tauri 生成的合并/覆盖冲突。

**文件夹**（fileAssociations 按扩展名，表达不了目录）走 `external_open::register_open_with` 后台线程单独最小注册：
- **Windows**：HKCU 注册表 `Software\Classes\Directory\shell\<Verb>\command`（`winreg`，`[target.'cfg(windows)'.dependencies]`，幂等短路）。文件不用手写注册表了，交给 Tauri。
- **Linux**：`~/.local/share/applications/*.desktop` 的 `MimeType=inode/directory;` + `update-desktop-database`（best-effort、`.spawn()` 不等子进程）。文件的 MimeType 由 Tauri 生成的 .desktop 承载。
- **macOS**：本轮不做文件夹 Open With（自定义 plist 有合并冲突风险，且 macOS 文件夹 Open With 本就少见）。

**路径送达机制三平台也不同**（`external_open::ingest_paths` 统一入口 + ~200ms 去抖合并）：
- macOS 走 `RunEvent::Opened { urls }`——**必须把 `lib.rs` 的 `.run(generate_context!())` 改成 `.build(ctx)?.run(|handle, event| ...)`** 才能接到；冷启动不经 argv。
- Windows/Linux 冷启动读 `std::env::args()`；热启动读 `single_instance` 回调的 `args`（原本被 `_args` 丢弃，是天然挂载点）。
- **冷启动竞态**：`RunEvent::Opened`/argv 可能早于前端订阅 → Rust 侧缓冲 + 前端 mount 时调 `take_pending_external_open()` 拉取（取走即清空、标记就绪）。前端务必**先挂事件监听、再拉 pending**，否则 take 标记就绪后、订阅前到达的路径会丢。
- **⚠️ 缓冲必须用进程级全局（`OnceLock`），不能用 Tauri 托管 state**：macOS「退出 app 后用『打开方式』打开文件」是冷启动 + 窗口状态恢复，`application:openURLs:`（→`RunEvent::Opened`）可能**早于 `setup()` 的 `app.manage(...)`** 到达。此时若在 Opened handler 里 `app.state::<T>()`，会 panic「state not managed」；而该 handler 在 ObjC `extern "C"` 边界上、panic 不可 unwind → 直接 `SIGABRT`（崩溃栈：`tao::...application_open_urls` → `panic_cannot_unwind` → `abort`，release 下我们的帧被内联，看着像 tao 自崩）。Tauri 官方 Opened 示例用托管 state 只在「app 已运行」场景成立。解法：缓冲放模块内 `OnceLock<Mutex<..>>`，Opened 冷路径完全不碰 `AppHandle`；并给 Opened handler 外包一层 `std::panic::catch_unwind(AssertUnwindSafe(..))` 兜底。
- **唤窗只在前端就绪（热态/托盘隐藏）时做**：常驻托盘、窗口隐藏时来了外部打开要 `show_main_window` 否则用户啥都看不到；但**别在冷启动 Opened 早期路径调 AppKit 窗口操作**（状态恢复中，有风险），且冷启动窗口本就默认显示。用「缓冲里的 `frontend_ready` 标志」区分冷/热，仅热态唤窗。
- app 自定义命令/事件走 `core:default`，**不需要**在 `capabilities/default.json` 加权限（只有 plugin 命令才要）。

**验证盲区（务必真机、逐平台）**：文件关联注册 + 路径送达全部**只能打包安装后在对应系统手测**，`cargo check`/`pnpm tauri dev` 覆盖不到；且 Windows 注册表代码在 mac 上因 `cfg(windows)` **连编译都不过**（Linux 同理）。macOS 侧还要注意 ad-hoc 签名/未公证的 dev 包在 Finder「打开方式」里的行为可能与正式包不同。

**相关文件**：`src-tauri/tauri.conf.json`（`bundle.fileAssociations` 扩展名列表）、`src-tauri/src/external_open.rs`（文件夹注册 + 路径入口/缓冲）、`src-tauri/src/{lib,setup}.rs`、`src-tauri/Cargo.toml`（winreg）、前端 `src/components/external-open-handler.tsx`

### develop 基线可能带 clippy/fmt 漂移（clippy/rustfmt 版本更新所致）

工具链升级（如 clippy/rustfmt 1.95）会新增/收紧 lint，使**之前干净**的已提交代码在全量重建时冒出警告（too_many_arguments、derivable_impls、items_after_test_module、collapsible_if、unused_imports 等）和 fmt 漂移。它们不是本次改动引入的。

**正确做法**：
- 证明"本次改动 0 新警告"：`git stash` 前后各跑一次 `cargo clippy --workspace`，比对计数。
- too_many_arguments 按本仓约定加 `#[expect(clippy::too_many_arguments, reason = "...")]`（async_trait 方法上的 `#[expect]` 会随宏展开保留、能命中）。
- 只在 test 用到的 import 移进 `#[cfg(test)] mod tests` 局部，别留在模块顶层（否则 lib 构建报 unused，即便 test mod 有 `use super::*`）。

**相关文件**：`crates/core/src/{device.rs,network/event_loop.rs,transfer/incoming.rs,transfer/flow/receive.rs}`、`src-tauri/src/database.rs`

## 国际化 (i18n)

### 后端字符串本地化：分两桶——「前端渲染」走 Lingui、「Rust 渲染」走 rust-i18n

后端面向用户的字符串按**谁渲染**分两桶，不要用一套解法：

- **① 错误 / 一切经 IPC 让前端展示的文本** → 前端 Lingui 翻译。后端只发稳定 `kind`（+ 结构化参数），**永不返回预翻译散文**。前端 `src/lib/errors.ts` 的 `getErrorMessage` 按 `err.kind` 查 Lingui 描述符表（`msg\`...\``），技术类 kind（Io/Serialization/Database/TaskJoin/P2p/Tauri）统一「出错了，请重试」，后端 `message` 降级为日志/详情用技术细节。core 错误的 `#[error("...")]` 一律写**语言无关英文**（曾有 `ExpiredCode`/`InvalidCode` 塞中文散文，已改掉——那是「翻译发生在错误的层」的反例）。
- **② 托盘菜单 / 系统通知等 Rust/OS 直接渲染、前端够不着的** → 桌面壳侧 rust-i18n 翻译（下条）。

原则一句话：**后端发码、边缘翻译**。错误 `kind` 与通知语义枚举同构。

**相关文件**：`src/lib/errors.ts`、`crates/host/src/error.rs`、`src-tauri/locales/`、`src-tauri/src/i18n.rs`

### rust-i18n 集成：`i18n!` 在 lib.rs 根、per-locale TOML、`%{var}` 插值

托盘 + 通知用 `rust-i18n = "4"`。**只覆盖 Rust 直接渲染的 ~20 条字符串**，不与前端 Lingui 重叠。

**正确做法**：
- `rust_i18n::i18n!("locales", fallback = "zh")` **必须在 crate 根**（`src-tauri/src/lib.rs`）调用一次——`t!` 展开成 `crate::_rust_i18n_translate(...)`，放子模块里路径解析不到。目录相对 `CARGO_MANIFEST_DIR`（= `src-tauri/`）。
- per-locale 文件 `src-tauri/locales/{zh,zh-TW,en}.toml`，文件名即 locale code（`zh-TW.toml` → locale `zh-TW`，与前端 `LocaleKey` 对齐）。嵌套表 `[tray]` / `[tray.status]` / `[notif.pairing]` 自动扁平成点分键 `tray.status.offline`。
- 插值：消息里 `%{name}`，调用 `t!("notif.pairing.body", hostname = value)`（named-arg 用 `=`；也支持 `"name" => value`）。
- `t!` 返回 `Cow<'static, str>`，`set_text` / `MenuItem::with_id` 收 `AsRef<str>` 直接吃；要 `String` 时 `.to_string()`（比 `.into_owned()` 稳，对 Cow/String/&str 都成立）。
- **locale 只有 3 个**（zh/zh-TW/en，见 `lingui.config.ts`），Rust 目录照 3 个来，别按 CLAUDE.md 顶部的「8 locale」。

**不要做**：
- 别在 core 里做 rust-i18n / 塞语言散文——core 平台中立，通知走语义枚举交给 host 译（下条）。

**相关文件**：`src-tauri/src/lib.rs`（`i18n!`）、`src-tauri/locales/*.toml`、`src-tauri/src/{tray.rs,host/notifier.rs}`

### locale 交付：Rust 启动读 tauri-store 的「双层编码」JSON 字符串，必须在 build_tray 之前

前端 `preferences-store` 是 locale 权威源。Rust 两个时机拿 locale：启动读持久化、切换经命令。

**正确做法**：
- 启动：`crate::i18n::init_locale_from_store` 用 `app.store("preferences.json")` 读 key `"preferences-store"`。⚠️ **该值是 zustand persist 经 JSONStorage 序列化后的 JSON 字符串**（`store.get` 拿到的是 `Value::String("{...}")`，不是对象）——要 `.as_str()` 再 `serde_json::from_str` 一次，才能取 `["state"]["locale"]`。读不到回退 `i18n!` 的 fallback zh。**必须在 `build_tray` 之前调用**（setup.rs），否则托盘首帧闪一下默认语言。
- 切换：前端 `preferences-store.setLocale` 在 `dynamicActivate` 后 `commands.setLocale(locale)`（try/catch best-effort）；后端 `set_locale` 命令 = `rust_i18n::set_locale` + `crate::tray::relocalize_tray`。
- 托盘要「切语言即时重绘」：**全部** `MenuItem` 句柄（open/pause/open_folder/settings/quit + status）都要存进 `TrayState`，否则换不了词；状态行/暂停项文案依赖当前 `(online,paused)`，用 `TrayState` 里的 `AtomicBool` 缓存这俩，`relocalize_tray` 才能重新派生。

**相关文件**：`src-tauri/src/i18n.rs`、`src-tauri/src/setup.rs`、`src-tauri/src/tray.rs`、`src/stores/preferences-store.ts`

### 通知语义枚举：core 发 `Notification` 码、host 译；改 `Notifier` 签名不破 RN

core 的系统通知从 `NotificationRequest{title,body}`（拼好的中文散文）改成语义枚举
`Notification::{PairingRequest{hostname}, IncomingTransfer{device_name}}`，desktop `DesktopNotifier`
`match` + `t!()` 译成当前 locale。core 彻底不碰语言。

**关键（曾误判为跨仓破坏点）**：改 `Notifier` trait 的方法签名**不会破 SwarmDrop-RN**。核实：RN
`mobile-core/src/events.rs` 对 `run_event_loop(..., Option<Arc<dyn Notifier>>)` 传 `None`（「移动端无
窗口聚焦概念，不需要 Notifier」），**RN 根本不实现 `Notifier`、不引用 `NotificationRequest`**（后者
无 uniffi 导出，只有 desktop-only specta derive）。trait 名与 `run_event_loop` 签名都没变 → RN 的
`None` 调用不受影响。且 RN `Cargo.toml` 通常 pin 在 `swarmdrop-core` 的 **git rev**（非本地 path），本地
改动对 RN 零即时影响。**动 core 的 host trait 前，先确认 RN 到底实不实现它、是不是传 None——别默认「改 core trait 必炸 RN」**。

**相关文件**：`crates/core/src/host.rs`（`Notification` + `Notifier`）、`crates/core/src/network/event_loop.rs`、`src-tauri/src/host/notifier.rs`、`../SwarmDrop-RN/packages/swarmdrop-core/rust/mobile-core/src/events.rs`

## 信任级别的默认接收策略：表只有一份，在内核

`DeviceReceivePolicy::for_trust_level(level, previous)`（`crates/host/src/device.rs`）是**三端
唯一的事实源**。三端各经自己的 binding 取它，**不许再抄一份到 JS**：

| 端 | 入口 |
|---|---|
| 桌面 | `commands.defaultReceivePolicy(level, previous)`（tauri-specta，纯函数命令不取 State） |
| 移动 | `defaultReceivePolicy(level, previous)`（uniffi 自由函数，**同步**，无 async 涟漪） |
| Web | `node-runtime.ts` 的 `defaultReceivePolicy(level, previous)`（包一层 `getModule()`） |

### 为什么值得专门收一次

2026-08 之前是**三份各不相同**的实现：

- 内核那份：切级别时一个字段都不保留。
- 桌面 JS：保留 `defaultSaveLocation` 与 `allowMcpAcceptFromDevice`。
- 移动 JS：注入 `resolveReceiveLocation()`，不保留 MCP 授权。

于是「切换信任级别」这一个产品动作有三种行为，而**内核那份反而是错的**：
`default_save_location` 为空时 `evaluate_receive_policy` 一律退回手动确认，所以「升到本人设备」
会把自动接收静默关掉——UI 上那个开关还开着。两条内核路径
（`PairedDeviceInfo::apply_trust_level_defaults`、`paired_devices::update_policy` 的
`receive_policy = None` 分支）都踩着这个坑，桌面 UI 只是靠自己那份 JS 副本绕开了它。

### 收口后的分工

- **内核**：默认表 + carry-forward 规则（保留 `default_save_location` 与
  `allow_mcp_accept_from_device`；`Blocked` 是唯一例外，两项都清零——「已阻止」必须是不留
  后门的终态）。守卫在 `crates/host/src/device.rs` 的
  `switching_trust_level_preserves_user_set_fields` / `blocking_clears_preserved_fields`。
- **宿主**：只补内核知道不了的东西。目前只有一处——移动端的
  `withHostSaveLocation()`：内核给不出「这台手机把文件放哪」，那是用户偏好 `receivePath` 加
  应用文档目录的回退。**只在 `autoAccept` 时补**，所以 `blocked` 天然不会被补回一个落点。

### 加字段时的连锁

`MobileDeviceReceivePolicy`（uniffi Record）用**穷尽解构**镜像内核结构体，加字段时那里会编译
失败，强制同步。注意它**刻意不携带** `allow_mcp_accept_from_device`（移动端不管理该策略，
回写恒 fail-closed 为 false）——所以内核为该字段做的 carry-forward 在移动端这条路径上看不到。
那是既有的类型边界，不是 bug。

**相关文件**：`crates/host/src/device.rs`、`crates/core/src/paired_devices.rs`、
`src-tauri/src/commands/pairing.rs`、`crates/web/src/node.rs`、
`mobile/packages/swarmdrop-core/rust/mobile-core/src/device.rs`、`mobile/src/core/device-trust.ts`

## 设备 DTO 的连接侧字段必须整份产出（2026-08-03）

`Device` 的 `status` / `connection` / `connectionDetails` / `latency` 四项是**同一次连接快照的
四个面**，不是四个独立字段。`device_manager` 有两条构造 `Device` 的分支
（`DeviceFilter::Paired` 与 `peer_to_device`），此前各自拼三元组；加上链路详情后，分开算会配出
「显示局域网直连，详情却是一条早已失效的 circuit 地址」这类互相矛盾的组合。

**正确做法**：`ConnectionSnapshot` 三个构造函数覆盖三种情形，两条分支只能整份取用。

- `offline()` —— 连接侧一切不适用
- `online_unknown()` —— presence 宽限期内 peer 已被清出内核表
- `from_peer(&PeerInfo)` —— 有运行时记录

**降级刻意不对称**：断连宽限期内 `connection` 回退到 mDNS 地址推断（局域网设备据此仍显示 LAN），
而 `details` 直接为 `None`——链路已经没了，给出旧地址只会让人对着一条失效的连接排查。

### 两张地址表不能混

`PeerInfo` 里 `addrs` 与 `conn_addr` 各存各的，**这条不能省**：

- `addrs` 只由 mDNS `Discovered` 写入，是 `is_lan_discovered` 的授权判据
  （`PairingMethod::Direct` 唯一的凭证——远程 peer 进不了本机多播域，因此伪造不了）；
- `conn_addr` 是链路快照，**对端 identify 自报的地址也会出现在这里**。

把后者并进前者，等于把配对授权判据交给对端自报。

**相关文件**：`crates/core/src/device_manager.rs`、`crates/host/src/device.rs`

## 暂停必须「先告知对端，再关流」——关闭数据流不携带原因

`pause_send` / `pause_receive` 里 **`notify_pause` 必须排在 cancel actor 之前**。这不是风格
偏好，是因果约束：

- 关闭数据流这个动作本身**不携带原因**，对端只看到「流没了」，按
  [`NetworkSignal::Interrupted`] 处理；
- 而 `reduce_network` 的两条守卫都要求 `state.is_active()`——先到的 Interrupted 把会话钉死在
  `suspended(Interrupted)` 之后，随后到达的 `RemotePaused` 守卫不满足、**被静默丢弃**。

`cancel()` 是本地立即生效，控制帧要走一个 RTT，所以旧顺序下 Interrupted **永远**先到。
这不是偶发竞态而是确定性错误。表现是对端 UI 显示「连接中断」而非「对方暂停」，用户转头去查
网络——`SUSPENDED_LABEL` 那条注释早就写过这个误导，只是没人想到暂停路径自己会踩中。
2026-08-04 Web 端双 origin 实测确认：接收方 console 里只有
`data channel 在完成前关闭`，没有任何暂停通知。

修复后的顺序（两个方向一致）：

1. `dispatch(Pause)` —— 本机状态先转，UI 立即响应，不必等 RTT
2. `notify_pause` —— 告知对端，等 Ack
3. cancel actor
4. 落进度（仅发送方向）→ remove actor

第 1、2 步之间 actor 仍在发数据，两条路都安全：若这期间恰好传完则转 completed（用户确实晚了
一步）；若因对端已 suspended 而写入失败，`on_interrupted` 的 Interrupted 也会因本机已非
active 被忽略。

**不要改成放宽 `reduce_network` 的守卫来容忍乱序**——那会让「某些 suspended 可被覆盖」渗进
状态机语义，而真正的问题是发送方没在关流前说明意图。锚点测试
`e2e_interrupted_first_shuts_out_late_remote_paused` 锁的就是这条推导。

**相关文件**：`crates/transfer/src/flow/send.rs`、`crates/transfer/src/flow/receive.rs`、
`crates/transfer/src/coordinator.rs`

## 发送侧进度只在终态批量落库，完成路径曾经漏了

projection 的 `transferredBytes` 是**文件级 SUM**（`store::projection_of`），而两个方向的进度
落库方式不对称：

| 方向 | 落库方式 |
|---|---|
| 接收 | `persist_chunk` 逐块增量落库，任何时刻都是准的 |
| 发送 | 只活在内存 `ProgressTracker`，仅在**终态路径**批量落一次 |

于是发送方向每多一条终态路径，就多一个「忘了落库」的机会。`on_completed` 就漏了——传完的会话
在发送方 UI 上显示「已完成 0 B / 500 MB 0%」，接收方同一条却是 100%（文件本身是好的，
hash 一致，纯显示问题）。

修复是让 `on_completed` 与 `on_interrupted` **对称**：都先落进度再转终态。新增发送侧终态路径
时照这个模式来。回归锚点在 `e2e_single_file_transfer` 尾部（断言两侧 transferredBytes 都等于
文件大小）。

**相关文件**：`crates/transfer/src/actor/sender.rs`、`crates/transfer/src/wire/data_plane.rs`

## 对端声明的 `relative_path` 必须过 `is_safe_relative_path`（安全）

接收侧最终做的是 `save_dir.join(relative_path)`（桌面
`src-tauri/src/host/file_sink/path_ops.rs:70`，Web 是 OPFS 的目录链）。而
`Path::join` **遇到绝对路径会把 base 整段丢弃**：

```
/Users/me/Downloads/SwarmDrop  .join("/etc/cron.d/evil")   →  /etc/cron.d/evil
/Users/me/Downloads/SwarmDrop  .join("../../../../.ssh/x") →  …/../../../../.ssh/x
```

`resolve_paths` 还会 `create_dir_all(parent)` 把目标目录建出来。这条校验缺席时（2026-08-04
之前），一个**已配对的对端可以往接收方磁盘任意位置写文件**。配对不蕴含这个权限——产品自己
就定义了 `temporary` / `collaborator` 这些低于 `owned` 的信任级别。

**收口点只有一个**：`crates/transfer/src/incoming.rs` 的 `TransferRequest::Offer` 分支，
即 wire 数据进入领域层的唯一入口。放在这里三端（桌面 `join`、移动 SAF、Web OPFS）一次覆盖，
没有哪个宿主可能忘记。

三条不显见的取舍：

- **必须在策略评估之前**。`evaluate_receive_policy` 要读 `files`（按扩展名 / 目录判定），
  拿一条 `../../..` 去比对「允许的文件夹」是拿脏数据做安全决策。
- **判据是纯字符串的，故意不用 `std::path`**。`std::path` 的语义随目标平台变：`..\..\x`
  在 Unix 上是**一个**普通文件名，在 Windows 上是两级穿越。收这条 offer 的可能是任意一端，
  判据必须三端逐字一致，否则同一条恶意路径在 Linux 上被放行、到了 Windows 才生效。
  所以两种分隔符都当分隔符，盘符前缀（`C:\x` 与 `C:x`）一并拒。
- **拒绝而不是「修正」**。静默归一化会把攻击变成一次「文件莫名其妙落在别处」的怪事，
  而合法发送端**永远**不会产生这些路径——浏览器的 `webkitRelativePath` 与桌面的枚举器都不会。

拒绝原因单独给了 `OfferRejectReason::UnsafePath`，不并进 `PolicyRejected`：前者对发送方是
「你的客户端发了非法数据」，后者是「换个设置或问问对方」，含义完全不同。

**相关文件**：`crates/transfer/src/protocol.rs`（判据 + 单测）、
`crates/transfer/src/incoming.rs`（收口点）、
`crates/core/tests/e2e_transfer.rs::e2e_offer_with_escaping_relative_path_is_rejected`

## Web 发送源的 id 不能用文件名

`crates/web/src/file_access.rs` 的源表是 `HashMap<FileSourceId, SourceEntry>`。id 曾经就是
`file.name()`，于是一次发送里挑两个同名文件（不同目录下的 `report.pdf` 极常见）时**后者
顶掉前者**：两条 entry 指向同一个 `File` 却各自声明着不同的 size——轻则 `prepare` 报
「read_source_chunk 返回长度异常」，重则**发出错误的文件内容**，而发送侧没有任何报错
（接收端验签才暴露，且归因指向网络）。

现在 id 是 `{prepared_id}/{idx}`（唯一、跨多次发送不冲突、日志与
`transfer_file.source_path` 里认得出是哪一批的第几个），**路径单独存在 `SourceEntry` 里**。
桌面不会撞是因为它的 id 本来就是绝对路径。

配套：`source_path` 会随会话落库，续传时由 `build_prepared_files_from_db` 读回重建
`FileSourceId`——所以 id 只要能往返就行，不需要有语义。

**相关文件**：`crates/web/src/file_access.rs`、`crates/web/src/node.rs::send_files`

## 挂起的入站 offer 有两条命必须一起收：内存条目 + 会话状态（2026-08-04）

`TransferManager::run_cleanup` 每 60 秒回收超过 `PENDING_OFFER_TIMEOUT_SECS`（170s）的
挂起 offer。**只摘内存条目是不够的**，因为一条 offer 在 `cache_inbound_offer` 时就已经
做了两件事：

1. `create_offered_inbound_session` —— 会话**落库**成 `offered`
2. `publish_projection` —— UI 据此把它挂进「待处理请求」

对端那侧是自洽的：`PendingOffer.responder` 一 drop，transfer-ctrl 的 handler 就得
`RecvError` 并回复婉拒。**本端 UI 完全在这条链之外**——它只订阅 projection。所以旧实现下
超时之后：

- 待处理请求永远挂在收件箱列表里（那是常驻列表，不像弹窗还能关掉）
- 点「接受」「拒绝」都撞上「会话不存在」，怎么点都消不掉
- 会话记录永远停在 `offered`，成为一条谁都推不动的僵尸

**正确做法**：`remove_expired` 返回被摘掉的 id，调用方逐条
`coordinator.dispatch(id, CoordinatorInput::Timeout(TimeoutSignal::OfferExpired))`。
状态机把 `offered` 推成 `terminal(TerminalReason::Expired)`，projection 随之下发，
三端前端的「终态时丢掉待决 offer」那条归约才有东西可吃——在此之前那段代码**从未被触发过**。

### `TerminalReason::Expired` 不能并进 `Rejected`

对端看到的确实是一次婉拒，但**本端用户什么都没做**。记成「已拒绝」等于在他自己的传输
历史里写一条他没做过的决定，而那正是他事后想确认「我拒过这个人吗」时会去查的地方。
新增变体的连锁面（都有编译期或类型检查兜底）：

| 位置 | 改什么 |
|---|---|
| `crates/entity/src/lib.rs` | 枚举 + `legacy_status` 映射（归 `Cancelled`：旧扁平枚举没有「没答复」这档） |
| `src/lib/bindings.ts` | `cargo test -p swarmdrop export_ts_bindings` 自动再生 |
| `crates/web/bindings/bindings.ts` | `cargo test -p swarmdrop-web --features specta --test specta_export`，**再 `pnpm build:wasm`**，否则 `packages/swarmdrop-web/*.d.ts` 还是旧的 |
| uniffi | `cargo build -p swarmdrop-mobile-core` → `ubrn generate jsi bindings --library <dylib>`（在 `rust/mobile-core` 目录里跑）→ `npx bob build`。**必须做**：TS 侧枚举是 ordinal 映射，少一个变体时 `FfiConverter.read` 遇到新 ordinal 会抛 |
| 三端 UI | 桌面 `transfer-projection.ts` + `session-panel.tsx`；Web `TERMINAL_LABEL`（Record 是 exhaustive 的，漏了编译期就红）；移动 `ProjectionStatus` + 三处判断 |

`default:` 分支是这类改动的头号陷阱——它让「漏了一个变体」编译期看不出来，运行时静默
落进「失败」。上面桌面那两处、移动那三处**都是**这种形态。

**相关文件**：`crates/transfer/src/manager.rs`、`crates/transfer/src/coordinator.rs`、
`crates/entity/src/lib.rs`
