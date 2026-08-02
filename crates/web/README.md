# swarmdrop-web —— 浏览器传输端

让浏览器成为真正的 SwarmDrop 传输端：**offer / accept / 续传 / bao 逐块验证全量复用**
`swarmdrop-transfer` 内核，端口（`SessionStore` / `FileAccess` / `TransferEventSink`）用 Web
实现填充（IndexedDB / OPFS / ReadableStream）。身份、已配对设备与传输会话都跨刷新持久化。

## crate 形态

- `#![cfg(wasm_browser)]` 门控整 crate：native target 下是**空壳**（`cargo check --workspace`
  秒过、进 CI），只有 `wasm32-unknown-unknown` 下是真身。
- 进 `scripts/check-wasm.sh` 的五 crate 双 target 门禁。

## 构建

产物是一个 npm 包（`swarmdrop-web`），构建到 **`docs/packages/swarmdrop-web/`**（提交入库），
docs 经 pnpm workspace 以 `workspace:*` 引用。UI 是 docs 里的 Web 应用区
（`docs/app/app/`，路由 `/app`），本 crate 不自带 HTML/JS harness。

```sh
# 依赖：wasm-pack；macOS 还需 Homebrew LLVM（Apple clang 无 wasm backend，ring 等 C 依赖编 wasm 必挂）
brew install llvm   # macOS only

# macOS：指向 Homebrew LLVM（Linux 通常可省）。
export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar

# getrandom 的 wasm_js backend 由仓库根 `.cargo/config.toml` 中的 wasm target
# 配置统一提供；不要在构建脚本或环境变量中重复设置，避免各端配置分叉。

# 从仓库根跑（docs 是构建产物家；也可用 docs 的 `pnpm build:wasm` 便捷脚本）
wasm-pack build crates/web --target web --release --out-dir ../../docs/packages/swarmdrop-web

# 跑测试页（docs 独立 workspace）
cd docs && pnpm install && pnpm dev   # http://localhost:3000/app
```

> `--out-dir` 相对**crate 目录**（`crates/web`）解析，故 `../../docs/...`。改 Rust 后重跑本命令
> 重生成 pkg 并提交。

> ⚠ **必须用 secure context 访问（https 或 localhost / 127.0.0.1），否则接收方落盘失败。**
>
> 非 secure 源（**http 私网 IP** 如 `http://192.168.x.x:8080`）下浏览器**不提供 `navigator.storage`
> 与 `crypto.subtle`**——接收方 finalize 走 OPFS 落盘时 `getDirectory()` 的 promise 永久 pending
> （现已 5s 超时 + 启动横幅预警，不再静默挂死）。secure context 仅含 `https://` /
> `http://localhost` / `http://127.0.0.1`。**端到端收发实测请用 `http://127.0.0.1:8080`**。
>
> 张力：测「私网 ws 的 mixed-content 豁免」那格需要私网 IP，但那样 OPFS 落盘用不了——两者
> 不可兼得，按测试目标选 origin。（记录见 `dev-notes/knowledge/libp2p-wasm.md`。）

## 使用（页面）

1. **connect**：填一个 helper 的 `ws` / `webrtc-direct` 地址（带 `/p2p/<id>` 尾段）→ connect；
   再 **reserve** 拿到 circuit 地址（浏览器被动接收连接的唯一入口，供对端拨回）。
2. **分享码**（可选）：需已连一个 DHT-capable helper（浏览器不可达 TCP bootstrap）；lookup 后
   自动回填对端 node id + 地址。
3. **发送**：填对端 node id、选文件、send —— 走内核 prepare（blake3 checksum + bao outboard）
   → Offer → 对端接受后推送（每块带 bao proof）。
4. **接收**：对端 Offer 到达 → 「收到的 Offer」区出现条目 → 接受/拒绝。接受后逐块验证落 OPFS，
   完成后出现下载链接（读回 OPFS 建 blob URL）。

## API 面（`WebNode`，wasm-bindgen）

| 方法 | 说明 |
|---|---|
| `spawn()` | 持久化身份（Window=localStorage / Worker=OPFS）+ IndexedDB 恢复已配对设备与传输会话 → Browser preset + DHT client → 装配 TransferManager + Router → 启动清理（遗留 active 转 suspended） |
| `node_id()` | 本机 base58 身份 |
| `connect(addr)` | 拨地址 → `ConnectionJson`（`{ path: "local"\|"direct"\|"relayed", addr }`） |
| `relays_ensure(helper_addr)` / `relays_drop(id)` | 登记 / 撤销 relay 常驻可达意图（circuit reservation 是持续状态，非一次性 RPC） |
| `relays_state()` / `relays_changed()` / `relays_until_active(id, signal?)` | reservation 快照 / 变化流 / 等首次 active（得到 circuit 地址） |
| `lookup_share_code(code)` | DHT 查分享码 → `NodeAddrJson`（`{ id, addrs }`） |
| `send_files(to, files)` | 登记文件源 → prepare → Offer；返回 session_id |
| `pending_offers()` | 当前挂起入站 offer → `OfferJson[]` |
| `accept_offer(sid)` / `reject_offer(sid)` | 接受（落 OPFS）/ 拒绝 |
| `resume(sid)` | 手动发起断点续传 |
| `download_url(relative_path)` | 完成后读回 OPFS 建 blob URL 供下载 |
| `transfer_history()` | 已持久化的会话投影 → `TransferProjection[]`（按 `startedAt` 倒序），刷新后回补活动视图 |
| `inbox_items(include_archived)` | 收件箱条目 → `InboxItemSummary[]`（`receivedAt` 倒序，排除软删项） |
| `inbox_item(id)` / `inbox_item_by_session(sid)` | 收件箱详情（含文件清单与关联投影）→ `InboxItemDetail \| null` |
| `search_inbox(query, limit?, include_archived)` | 子串检索 → `InboxSearchHit[]`（判据与 SQL 侧同义）。`limit` 可选，缺省/超限都取 `INBOX_SEARCH_LIMIT` |
| `mark_inbox_item_opened(id)` / `archive_inbox_item(id, archived)` / `delete_inbox_item(id, delete_local_files)` | 标记打开 / 归档 / 软删。**删记录默认不动文件**，`delete_local_files=true` 时连 OPFS 副本一起删（前端恒传 true——OPFS 文件用户无从访问，留着只是泄漏配额） |
| `paired_devices()` | 已配对设备清单 → `Device[]`（与桌面 `list_devices` 同源的 `DeviceManager` 读模型，含在线状态/连接类型） |
| `remove_paired_device(peer_id)` | 解除配对（core 的 `unpair`：先落盘 → 再删共享内存表 → 再发事件；持久化失败即报错且内存表不动） |
| `events()` | `ReadableStream<WebTransferEvent>`（**只能取一次**） |
| `close()` | 关停 |

**TS 类型端到端**：`src/types.rs` 的 JS 可见类型（`WebTransferEvent` / `OfferJson` /
`ConnectionJson` / `NodeAddrJson` / `WebError`）由 specta 导出成 `bindings/bindings.ts`
（`cargo test -p swarmdrop-web --features specta` 生成，入库），node.rs 经
`typescript_custom_section` 注入 .d.ts 并用 `typescript_type` 把方法签名接到具名类型——
`.d.ts` 里无 `any`。错误 reject 的是 `WebError`（`{ kind, message }`），Worker 桥原样透传
（client.js 把 kind 挂回 Error 实例）。

## 端口实现取舍

- **WebTransferStore**（`SessionStore` + `InboxStore`）：**内存读缓存 + IndexedDB 写穿**。
  entity `Model` 是纯 scalar 结构，直接手构造；投影直接构造 `TransferProjection`（绕开
  `ModelEx` 的 `HasMany`），故本 crate **不直接依赖 sea-orm**；`Model` 不可直接序列化
  （`#[sea_orm::model]` 不转发用户 derive），落库形态用 serde remote derive 声明在 `store.rs`
  末尾——entity 加列时那份声明**编译不过**，而手写 DTO 只会静默丢字段。
  **落库范围**：终态会话（收发双向，作历史与收件箱）+ 非终态的**接收**会话（OPFS 里的 `.part`
  与 checkpoint 都在，可续传）。非终态**发送**会话与待决 offer 不落库——见「遗留 / 取舍」。
  `InboxStore` 那一半整体委托给 `inbox.rs` 的 **`WebInboxTable`**：**独立的 `inbox` object
  store**（一条条目一个 key），不是「接收 + 已完成」的会话投影。真表的实质收益是它**不参与
  `HISTORY_CAP` 淘汰、也不随「清空传输历史」消失**——三端共有的那条不变量在浏览器上才成立。
  条目的标题 / 内容指纹 / 聚合文本 / 命中判据 / 片段一律调 `swarmdrop_transfer::inbox` 的
  共享规则，两端不各写一份。
  **存量数据不迁移**（openspec: inbox-store-port-completion 的 D7）：升 `DB_VERSION` 只为让
  `onupgradeneeded` 建出新 store，老库里已完成的接收会话不回填条目——落地后收件箱从空开始
  是预期结果，不是 bug。
- **OpfsFileAccess**：主线程 async OPFS（`navigator.storage.getDirectory / createWritable`；
  **禁用 SyncAccessHandle**——Worker-only，与 webrtc-websys 主线程约束冲突）。JsValue `!Send`
  用 `send_wrapper::SendWrapper` 裹 JsFuture 满足端口 Send。接收侧**流式落盘**：`create_sink`
  开 `createWritable` 句柄常驻，每 chunk `WriteParams{position,data}` positioned 直写（单次
  Promise 往返），`finalize` 时 `close` 提交；续传走 `keepExistingData:true`。大文件不进内存。
- **WebEventSink**：`TransferEvent` 走无界 channel（`Send`）→ `events()` 的 ReadableStream 单点
  消费、serde-wasm-bindgen 序列化（镜像 `WebTransferEvent`，`tag="type"` camelCase）。
- **身份**：`SecretKey` protobuf 编码 hex 存 localStorage。
- **已配对设备**：`PairedDeviceInfo[]` 存 IndexedDB（`swarmdrop-web` / `kv` / `swarmdrop.pairedDevices.v1`），刷新后由 `start_node` 经端口读回。
  实现是 `src/paired_devices.rs` 的 `WebPairedDeviceStore`（`swarmdrop_host::PairedDeviceStore`），
  **只有 load / save 两个动作**；upsert / 改策略 / 移除的语义统一在 `swarmdrop_core::paired_devices`。
- **传输会话**：一条会话一个 key，存 IndexedDB 的 `sessions` store（同库 v2）。IndexedDB 的
  低层读写（开库 / 升级 / `IDBRequest` → future / 错误取人话）收在 `src/idb.rs`，身份、配对与
  会话三处共用；连接进程内缓存（接收侧 checkpoint ≈ 12 次/秒，每次重开会把
  `indexedDB.open()` 往返压进接收热路径），回调句柄用完即 drop 而**不 `forget()`**——那条
  路径上泄漏一个 `Closure` 就等于泄漏它捕获的连接，量正比于传输字节数。

## 遗留 / 取舍

- ~~**无配对**：`PeerDirectory` 对任意对端返回合成 `PairedDeviceInfo`~~ —— **这条已作废**
  （2026-07-19 的旧记录）。Web 现在走的是与桌面同源的 `runtime::start_node`，`build_router`
  把**真的** `manager.pairing_arc()` 当作 `PeerDirectory` 交给 `TransferCtrlService`：
  未配对对端照样被 `incoming.rs` 硬拒 `NotPaired`，已配对对端的 `receive_policy` 经
  `swarmdrop_transfer::policy` **真被裁决**。别据此以为「信任策略在 Web 上只是展示」——
  存储层的语义分叉在这里是会真出事的（见下面 `paired_devices()` 那条）。
- **非终态发送会话与待决 offer 不跨刷新**（2026-07-27 `#81`，**物理约束不是取舍**）：发送侧的
  文件内容来自用户选中的 `File` 对象（`OpfsFileAccess::register_source`），页面刷新后 JS 上下文
  销毁、无法在未经用户重新选择的前提下再读同一个文件——恢复出来只能给一个点了必失败的
  「续传」按钮，故干脆不落库。待决 offer 同理：`pending_offers()` 是 `TransferManager` 的内存态，
  刷新后对端的 offer 已无处应答。**接收方向不受此限**（OPFS 的 `.part` 与 checkpoint 都在，
  `resume(sid)` 可续）。
- **bao outboard 不落库**（1 GiB 文件 ≈ 4 MiB）：唯一消费方是发送侧，而发送侧本就不跨刷新恢复；
  载入恒 `None`，发送端缺失时按源文件重算并回存（内核既有路径）。
- **checkpoint 写放大随文件大小平方增长**（未优化）：接收侧每 10 个 chunk（2.5 MB）落一次盘，
  每次把整条会话序列化成 JSON 覆写，而 `completed_chunks` 位图以 JSON 数字数组编码有 2–4x 膨胀。
  1 GiB 传输全程约 0.6 MB IDB 写入（无感），**10 GiB 约 61 MB**——总量 ∝ `(size/256KiB)²`。
  彻底的解法是把 checkpoint 拆成独立记录、value 用 `Uint8Array`（IndexedDB 结构化克隆原生支持
  二进制，零编码膨胀），或让 `persist` 只置脏标记、后台以 ≤2 Hz flush（顺带把 IDB 往返移出接收
  热路径）。当前量级下没做。
- **`FileAccess::cleanup_sink` 的端口契约仍没写清**（Web 侧已不再留残件）：端口的 doc 没写
  「要删除部分产物」、默认实现是 no-op，各端只能各自约定——桌面靠 `file_source.rs` 的
  `part_file.cleanup()` 履约，`src-tauri` 的过期回收甚至绕开 `FileAccess` 直接
  `tokio::fs::remove_file`，移动端未核实。
  Web 侧已在 `file_access.rs` 补上：`abort()` 写句柄放锁 → `opfs::remove_path` 删掉那条
  OPFS 文件（顺序不可换，锁没放 `remove_entry` 会撞 `NoModificationAllowedError`）。
  这条尤其必要，因为 Web **没有 `.part` 中间态**，写的就是最终路径，残件是个文件名正确、
  内容截断的东西。
  **留给后续 change**：给 `FileAccess` 加一条显式的「丢弃部分产物」方法把语义写进契约，
  顺带把桌面那处绕行收编回来——那要动端口、三端一起改，不该和一条用户可见缺陷绑在一起。
- **会话记录的过期回收已与桌面对齐**：`WebTransferStore::load` 用与
  `reap_expired_suspended_receives` 相同的命中条件（recoverable + 接收 + 超
  `SUSPENDED_RECEIVE_RETENTION_SECS`），转 `Terminal`/`FatalError` 并写「超过 N 天未恢复」，
  记录留在历史里而非凭空消失。差别只剩**过期回收不清 OPFS 残件**：Web 不调
  `swarmdrop_transfer::cleanup_expired_part_files`（它先 `open_or_create_sink` 再
  `cleanup_sink`，随上面那条一起变成对 Web 真能工作了，接过来即可）。
- DHT 查分享码需先连 DHT-capable helper（浏览器不可达 TCP bootstrap，故 spawn 不加 bootstrap）。
- **配对持久化已走端口，但落地形态不是完整 keychain**（2026-07-31 修正 2026-07-19 的记录）：
  已配对设备列表由 `swarmdrop_host::PairedDeviceStore` 这个**独立端口**承接，实现在
  `src/paired_devices.rs`（`WebPairedDeviceStore`，IndexedDB `kv` store 整份快照）。
  没有做 `WebKeychainProvider`：那个 trait 只管密钥材料，而浏览器根本没有钥匙串——为存一份
  设备列表去实现六个永远不该被调用的密钥方法，只会多出一堆「实现了但不能用」的方法。
  拆分理由见 `openspec/changes/atomic-unpair-and-paired-device-store/design.md` 的 D1 / D8。
  列表算法（upsert / 改策略 / 移除）**不在本 crate**，统一在 `swarmdrop_core::paired_devices`。
  **仍未走端口的是 identity 本身**：`SecretKey` 的读写还在 `src/identity.rs` 里直连
  localStorage / OPFS（`KeychainProvider` 是 8 方法的密钥端口，Web 只用得上其中 3 个）。
- **2026-07-19 全 crate 审查记录为后续的项**（其余两条）：方法名 snake_case 与桌面
  bindings.ts 的 camelCase 不一致（`js_name` 可改，随 React UI 一并）；`content_root_of` 与
  transfer 版重复（泛化 transfer 签名可归一，涉及三 crate 调用点）。
- **`connect()` / `reserve()` 的 20 秒可取消超时**（2026-07-25 `#84`）：WebNode 在内核层
  设定等待上限。`connect()` 超时会清理 actor 等待者；若没有其他调用者或基础设施角色，直接
  abort libp2p pending dial。`reserve()` 超时还会撤销 circuit listener 与 relay 自动重建意图，
  并在没有其他用途时中止 helper 拨号。前端直接等待 WebNode Promise，不能再用 `Promise.race`
  只取消 UI 等待，否则会重新制造后台残留。
- **`paired_devices()` 复用桌面 `Device` 读模型，未做 Web 专属裁剪**（2026-07-21 `#77` 新增）：
  直接返回 `DeviceManager::get_devices(DeviceFilter::Paired)`，字段含 `trustLevel` /
  `receivePolicy` 等桌面概念。**它们不是展示字段**（2026-07-31 修正）：Web 与桌面共用
  `runtime::start_node`，`receive_policy` 经 `swarmdrop_transfer::policy` 真被裁决。
  缺的只是**调整信任级别的 UI**——后端能力已经在，前端消费即可；也正因为如此，存储层
  绝不能自带一份「整条替换」的 upsert（那会是一次静默提权，而不只是显示不对）。
- **`paired_devices()` 是轮询不是推送**（2026-07-21 `#77`）：`CoreEvent::DevicesChanged` /
  `PairedDeviceAdded` 已经带着桌面推送同款的完整数据，但 `WebEventBus::publish`
  （`src/event_bus.rs`）目前用一个 `other => { tracing::debug!(...) }` 通配分支把它们连同其他
  device/network 事件一起吞进日志，只挑出 `PairingRequestReceived` 落进队列。前端因此只能靠
  `docs/app/app/_lib/state-poll.ts` 每 1.5s 轮询 `paired_devices()`。这是与 `pending_pairing_requests()`
  一致的既有模式（浏览器侧 pairing 域本就未走推送流），本次新增的 `paired_devices()` 顺着同一
  模式走，未推动它变成推送——真要做推送，需要决定新事件走哪条通道（塞进 `WebTransferEvent` 会
  把 transfer 域和 device 域混在一起，不合适；更可能是给 `WebEventBus` 单开一条队列/流，
  与 `pending_pairing_requests()` 同构）。
- **`send_files()` 不回传 `preparedId`，前端只能用「最近一条」近似追踪 prepare 进度**
  （2026-07-21 `#78`）：内部生成的 `prepared_id`（`node.rs` 的 `send_files`）只用于内部
  `manager.prepare()` 调用，从不透出给 JS——`send_files(to, files): Promise<string>` 只在
  prepare + 发 Offer 全部完成后返回最终 session_id。前端 `docs/app/app/_lib/store.ts` 因此
  只能维护一个全局的 `latestPrepareProgress`（「最近一条 `prepareProgress` 事件」），MVP
  下（单个活跃发送、按钮 pending 时锁定）够用，但**如果同时有两个并发 `send_files()` 在跑，
  两者的 prepare 进度事件会交替覆盖同一个字段，进度条张冠李戴**。桌面端 `src-tauri` 的
  `prepare_send`（返回 `prepared_id`）+ `start_send` 两步式命令已经验证过「把 preparedId
  交回调用方」这个模式；Web 若要支持并发发送，应照这个先例把 `send_files` 拆成两步。判定为
  当前 UI 不可触发（只有一个 `SendPanel` 实例、发送中按钮锁定）的已知限制，未现在拆分。

## 基准（`static/bench.html` + `scripts/web-bench/driver.mjs`）

自动化传输基准：headless Chrome 双 tab（同 origin，send 侧 spawn 前清 localStorage 身份），经
本机 helper 走 relay circuit 互传，测速率 / 主线程卡顿（longtask）/ SHA-256 字节一致性。

```sh
# 前置：wasm-pack build 完成；net-web-smoke helper 运行中；http.server 8080 -d static
node scripts/web-bench/driver.mjs "<helper-ws-addr>/p2p/<id>" 268435456 1
```

2026-07-18 实测（M2 MacBook 同机三跳 A→relay→B，headless，最不利 CPU 竞争配置）；
`recvMode` 第 4 参数选接收侧运行模式（`main` | `worker`）：

| 大小 | 接收模式 | 落盘 | 接收耗时 | 均速 | 接收侧 longtask | hash |
|---|---|---|---|---|---|---|
| 256 MB | 主线程 | createWritable | 8.2s | 31.3 MB/s | **0** | ✅ 一致 |
| 1 GB | 主线程 | createWritable | 32.1s | 31.9 MB/s | **0** | ✅ 一致 |
| 256 MB | Worker | createWritable | 7.4s | 34.8 MB/s | **0** | ✅ 一致 |
| 1 GB | Worker | createWritable | 33.2s | 30.9 MB/s | **0** | ✅ 一致 |
| 256 MB | Worker | **SyncAccessHandle** | 8.2s | 31.1 MB/s | **0** | ✅ 一致 |
| 1 GB | Worker | **SyncAccessHandle** | 33.6s | 30.5 MB/s | **0** | ✅ 一致 |

六组全部 ~31±3 MB/s（run 间噪声级差异）——**瓶颈在网络链路（relay 三跳 + noise），
不在落盘**：SyncAccessHandle 同步直写对吞吐无增益，但每 chunk 省一次 wasm↔JS Promise
调度、写即落盘（无 staging，崩溃丢失面更小），Worker 版保留它。速率不随文件大小衰减
（无内存压力）；接收全程主线程零长任务——**31 MB/s 档收流 + 流式落盘不卡 UI**。
发送侧仅准备段一次 longtask（`new File([buf])` 构造测试数据，非传输热路径）。

## Worker 运行模式（`static/worker.js` + `static/client.js`）

同一份 wasm 双环境通吃：`WebNode.spawn()` 在 Window 直跑（webrtc+ws 双 transport），
在 Web Worker 里跑则自动 ws-only（`env.rs` 探测；**webrtc-websys 在 Worker 装着都不行**——
它的 dial 在地址格式检查之前就碰 window，经 or_transport 拨任何地址都 panic，故 transport
组装按环境裁剪）。身份持久化双轨：Window=localStorage、Worker=OPFS 文件（Worker 全自治，
无需主线程注入）。落盘句柄也按环境二选一（`SinkHandle`）：Window 用 `createWritable`
staging 流，Worker 用 **`SyncAccessHandle`**（同步零 Promise 直写、独占锁须显式 close、
写即落盘无 staging）。`client.js` 提供与 `WebNode` 同形状的主线程桥（postMessage RPC，
事件用 `onEvent` 回调），调用方两版无感切换；`File` 对象经 structured clone 传入 Worker。
