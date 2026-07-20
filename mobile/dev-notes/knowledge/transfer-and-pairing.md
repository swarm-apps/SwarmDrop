# Transfer & Pairing

## 概览

SwarmDrop-RN 的核心业务是 P2P 文件传输 + 设备配对。状态层在
[src/stores/mobile-core-store.ts](../../src/stores/mobile-core-store.ts)（zustand）+ Rust core
事件通过 [src/core/event-bus.ts](../../src/core/event-bus.ts) 路由。配对码用单例 store
（[src/stores/pairing-code-store.ts](../../src/stores/pairing-code-store.ts)）。

下面记录的都是"从代码看不出来的约束"——FFI 边界、core 端的语义、zustand 订阅的坑。

## 传输活动状态源

### RN 端只消费 TransferProjection，不再维护旧 history/status 模型

移动端传输列表、详情页和恢复入口统一走 shared core 的投影 API：

- `getTransferProjections()`
- `getTransferProjection(sessionId)`
- `clearTransferActivity()`
- `deleteTransferRecord(sessionId)`
- `resumeTransfer(sessionId)`

Rust bridge 的 [history.rs](../../packages/swarmdrop-core/rust/mobile-core/src/history.rs)
只负责把 `swarmdrop_core::transfer::store::TransferProjection` 镜像成
`MobileTransferProjection`。RN 端状态集中在
[src/stores/transfer-store.ts](../../src/stores/transfer-store.ts)，UI 只用
[src/core/transfer-types.ts](../../src/core/transfer-types.ts) 把 projection 映射成移动端展示状态。

旧的 `MobileSessionStatus` / `MobileTransferHistoryItem` / `listTransferHistory()` 不是新 UI 的状态源。
新增收件箱、设备信任策略或传输活动视图时，不要再从 RN 侧拼 history model；应该先让 core 发
`TransferProjectionUpdate`，再由 store 合并 projection。

### App 重启后的活跃传输标记为 AppRestarted

`reconcile_stale_sessions()` 在 mobile-core 初始化时会把 DB 里遗留的活跃会话统一过渡到
`Suspended/AppRestarted`。这比旧的 host-only `ERROR_APP_INTERRUPTED` 更适合移动端：
projection 自带 `recoverable`、`suspendedReason`、`policyAction/policyReason` 和保存位置，UI
可以直接决定是否显示「恢复」或后续策略提示。

**相关文件**：

- [packages/swarmdrop-core/rust/mobile-core/src/history.rs](../../packages/swarmdrop-core/rust/mobile-core/src/history.rs)
- [packages/swarmdrop-core/rust/mobile-core/src/events.rs](../../packages/swarmdrop-core/rust/mobile-core/src/events.rs)
- [src/stores/transfer-store.ts](../../src/stores/transfer-store.ts)
- [src/core/event-bus.ts](../../src/core/event-bus.ts)

## 收件箱

### 设备收件箱是内容库，不是状态说明页

移动端收件箱首页要优先支持“快速扫最近收到的内容”：顶部使用内容库概览，下方用横向滚动的快捷 chip
（全部/文件/图片/视频/文本/异常/已归档）收窄列表。`全部` 必须符合用户直觉，显示包含归档在内的所有内容；
`已归档` 只是同层快捷过滤入口，不要再做“当前/含归档”的二级范围开关。搜索是独立模式，入口放在首页顶部 icon，
进入 `/inbox/search` 后自动聚焦搜索框，并复用快捷分类 chip 和列表行。不要把搜索框常驻在首页。
归档显示不要做成设置项式大开关；“内容边界”这类产品说明不要作为页面底部卡片展示。

刷新时必须保持当前列表或空状态原位，只显示轻量 loading 反馈，不要切到整块“正在刷新”占位，避免页面闪烁。

**相关文件**：[src/app/(main)/inbox.tsx](../../src/app/(main)/inbox.tsx)

### 收件箱详情页优先呈现内容本体

详情页不是记录表单页。第一屏应优先展示收到的内容本体：单文件用预览 / 大图标作为主视觉，
文件名、来源、时间和状态紧随其后；多文件才展示“包含内容”列表。打开/分享、复制路径这类高频动作
放在底部拇指区；归档、查看传输诊断、删除记录和删除本地文件收进“更多”底部操作面板。
删除类操作要先关闭操作面板，再打开确认弹窗，不要堆叠多个 bottom sheet / dialog。
来源 peer id、完整保存路径、内容指纹等低频元信息放进可展开的“详情”，默认不要铺满第一屏。

**相关文件**：[src/app/inbox/[itemId].tsx](../../src/app/inbox/[itemId].tsx)

## Zustand selector

### 返回新对象/数组的 selector 必须包 useShallow

zustand v5 + `useSyncExternalStore` 把每次返回新引用的 selector 判为 snapshot 不稳定，进入
update→render→update 的死循环（`Maximum update depth exceeded`）。任何返回 `{ ... }` 字面量
或 `filter`/`map` 结果的 selector 都要包 `useShallow` 做逐元素 === 比较。

**正确做法**：

```tsx
const nearbyDevices = useMobileCoreStore(
  useShallow((s) => s.devices.filter((d) => !d.isPaired && d.status === "online")),
);
```

**不要做**：

```tsx
// 死循环！每次返回新数组
const nearbyDevices = useMobileCoreStore((s) =>
  s.devices.filter((d) => !d.isPaired && d.status === "online"),
);
```

**相关文件**：[src/components/pairing-sheet.tsx](../../src/components/pairing-sheet.tsx)
（项目里所有多字段 selector 都已统一用 useShallow，见 grep `useShallow`）

## 已配对设备视图

### 设备首页只放有业务闭环的快捷入口

运行态第一块应展示当前节点可用性、附近/配对/活跃传输的轻量概览，并只提供收件箱、活动这类已有
业务闭环的高频入口。不要在首页放全局“发送文件”入口，除非后续已经有完整的发送业务承接；发送仍应
从具体设备行或已配对设备详情这类有目标设备的上下文进入。随后直接展示附近设备、本机配对码和输入配对码入口。
未启动时第一块承担空状态和快捷启动，不再额外展示不可用的添加设备面板。

**相关文件**：[src/app/(main)/index.tsx](../../src/app/(main)/index.tsx)

### 已配对设备视图合并 keychain 和实时发现结果

NetManager（Rust 端 P2P 节点）的 `listDevices("all")` 是实时发现视图，不保证包含所有已配对设备；
节点运行时，如果某台已配对设备当前没有被发现，它不会出现在 `devices` 里。移动端要对齐桌面端
`storedPairedDevices + normalizedDevices` 的语义：已配对设备列表必须把 Rust `list_paired_devices()`
FFI 读到的 keychain cache 和实时发现结果合并。cache 提供完整离线清单；实时 `devices` 只负责覆盖
在线状态和最新运行时字段；如果某个实时设备 peerId 命中 cache，即使实时字段还没标 `isPaired`，
也要强制视为已配对并继承 cache 里的 trust/policy。zustand 存 cache 字段 `pairedDevicesCache`，
在 `loadIdentity` 完成时立即拉一次，之后由 **`PairedDeviceAdded` 事件**刷新。

> ⚠️ 这条规则适用于**每一个**消费已配对设备的界面，不只是设备列表。`app/device/groups.tsx`
> 的分组设备数曾经只读 cache 不 merge，结果「首页设备好好的、分组页说暂无设备」（v0.7.16 修复）。
> 判断标准：只要要回答「这个 peerId 是不是我的已配对设备」，就必须过 `mergePairedDevicesWithCache`。

**正确做法**：组件渲染时使用合并 helper，不要按运行态切换为纯实时列表：

```tsx
const pairedDevices = useMemo(() => {
  return mergePairedDevicesWithCache(devices, pairedDevicesCache).sort((a, b) => {
    if (a.status !== b.status) return a.status === "online" ? -1 : 1;
    return deviceDisplayName(a).localeCompare(deviceDisplayName(b));
  });
}, [devices, pairedDevicesCache]);
```

**不要做**：
- 不要在运行态只用 `devices.filter((d) => d.isPaired)`，否则启动节点后未被实时发现的已配对设备会消失
- 不要在 `applyDevices` / `refreshDevices` 里把 `pairedDevicesCache` 跟 `devices` 一起覆盖——
  NetManager 还没发现的 paired 设备会被空覆盖，UI 闪烁
- 不要只订阅 `pairedDevicesCache` 就下「是否已配对」的结论——cache 有陈旧窗口（见下条）

### 配对成功必须 publish `PairedDeviceAdded`，不能只写 keychain

移动端 `pairing.rs` 的 `request_pairing` / `respond_pairing_request` 成功后**必须**
`event_bus_arc().publish(CoreEvent::PairedDeviceAdded { device })`，不要自己调
`upsert_paired_device`。原因是 `MobileEventBusAdapter::publish` 已经把「写 keychain + emit 给 JS」
两件事一起做了——一次 publish 就够，也不会重复写盘；而 JS 的 `pairedDevicesCache` **只在收到事件时
刷新**，只写 keychain 的话 JS 无从得知，cache 会停在冷启动的旧快照（节点停掉时永不自愈）。
桌面 `src-tauri/commands/pairing.rs` 一直是 emit 的，这是移植时漏掉的不对称。

**坑：`CoreEvent::PairingCompleted` 是死代码**——pinned core 里只有 enum 声明 + 桌面 event_bus
一个空 match 分支，**零 publisher**。别监听它，也别指望它触发刷新（曾在 `event-bus.ts` 里挂了个
永不执行的 handler）。

**坑：helper 不能放进 `#[uniffi::export] impl` 块**——参数是 core 侧类型（`PairedDeviceInfo`
没有 uniffi `Lift`）会被当成 FFI 方法，报 `trait bound Lift<UniFfiTag> is not satisfied`。
直接在调用点 inline 最省事。此改动 FFI 签名不变 → 绑定零 diff，但**必须重编原生库**。

**相关文件**：[packages/swarmdrop-core/rust/mobile-core/src/pairing.rs](../../packages/swarmdrop-core/rust/mobile-core/src/pairing.rs),
[src/core/event-bus.ts](../../src/core/event-bus.ts)

**相关文件**：[src/stores/mobile-core-store.ts](../../src/stores/mobile-core-store.ts),
[src/app/(main)/index.tsx](../../src/app/(main)/index.tsx),
[src/app/send/select-device.tsx](../../src/app/send/select-device.tsx)

## 节点生命周期

### presence 由 core 自治，host 不做任何 presence 调用

已配对设备的在线状态维持（DHT 在线宣告刷新、keep-alive 白名单、断连宽限重拨、
离线低频重探）全部由 `swarmdrop_core::presence::Supervisor` 承担，随 core 的
`run_event_loop` 自动拉起、随 `NetManager::shutdown()` 结束。在线语义：
`Connected | Probing(断连后 ~15s 宽限，UI 维持在线) → online`，`Unreachable → offline`。

**正确做法**：
- host 启动节点只调 `start_node`，停止只调 `manager.shutdown()`（内含 announce_offline）。
- 网络抖动 1-2 秒不会闪离线（宽限期语义）。死对端判离线时长按 transport 分：
  QUIC ≈25s（quinn 10s idle 判死 + 15s 宽限）；TCP ≈55s（无传输层判死，靠
  Supervisor 数 2 个 PingFailure 事件 ≈ 协议层连续 3 次失败 ≈40s，libp2p-ping
  handler 会吞掉第 1 次失败）。对端回归 ≤90s 被低频重探发现。

**不要做**：
- 不要在 host 侧再加 announce_online / check_paired_online 之类的一次性编排——
  这些 API 已从 `pairing::manager` 移除，职责在 `presence` 模块。
- 不要把「在线」再当成瞬时 libp2p 连接镜像来改——libp2p 0.52+ 的 ping 不保活，
  裸连接 60s 空闲必被回收，那正是 v0.7.5 之前「闲置自动离线」bug 的根因。

**相关文件**：桌面仓 `crates/core/src/presence/`（rev dd46c7d），
[packages/swarmdrop-core/rust/mobile-core/src/network.rs](../../packages/swarmdrop-core/rust/mobile-core/src/network.rs)

### 基础设施链路同样是收敛式的（infra::Supervisor），LanOnly ≠ 网络隔离

跨网关拓扑（手机 LAN Helper + LanOnly 电脑 + 公网设备）的可达性由
`swarmdrop_core::infra::Supervisor` 收敛维持（rev 5c1e209，change
harden-cross-network-reachability）：relay reservation 断线自动退避重建
（helper 挂起/重启恢复后秒级收敛，候选地址刷新即重置退避）；identify 识别
bootstrap agent 自动纳管为 Learned 候选。

**关键语义**：
- `discovery_mode=LanOnly` 只是"不主动连内置公网引导"；新设置
  `public_reachability`（默认开）才管"允不允许经公网中继被跨网访问"。
  LanOnly+默认开 = 设备经 LAN Helper 学到公网中继后自动做直连
  reservation，跨网设备**一跳可拨、零打洞依赖**——这是跨网 presence
  可靠的根基，别关。
- OnlineRecord 是结构化可达性声明（direct_addrs/relay_addrs/relays hint），
  不再是裸 listeners；重探对 circuit 地址会先修 relay 直连再拨。
- relay 限额已适配文件传输：bytes 无限、duration 12h（旧默认 64MiB/30min
  会掐断大文件中继传输）。

**不要做**：
- 不要在 host/UI 层对 reservation/候选做任何一次性编排——断线重建、
  退避、纳管全部在 core 收敛层。
- 单机联调 LAN Helper 时记得 `LanHelperConfig.announce_loopback_addrs=true`，
  否则 reservation 响应无地址、客户端以 NoAddressesInReservation 拒绝
  （生产局域网有私网地址不受影响）。
- 不要手写 Multiaddr 分类（私网/公网/loopback/circuit 跳数）——统一用
  `swarm_p2p_core::addr` 谓词，曾因五处手写漂移出过 IPv6 link-local 误判。
- libs 层不做任何自动 reservation（策略归 InfraSupervisor 的
  wants_reservation 门控），否则 public_reachability 设置会被绕过。

### 节点生命周期契约：shutdown().await + drop(manager) = 完整拆除

`run_event_loop` 随 `cancel_token` 退出（v0.7.3/v0.7.7 审查轮治本）：
退出后 client/receiver 全部 drop → libs 事件循环退出 → swarm 断开全部
连接。此前僵尸节点的连接被 keep-alive 白名单钉死，**对端会永远看到已
停止的节点在线**。

**正确做法**：
- host 停节点 = `manager.shutdown().await` + `guard.take()`（两步缺一不可，
  只 cancel 不 drop 则 client 句柄拖住 swarm 不死）。
- 任何新的 core 后台循环必须 `tokio::select!` 挂 `shared.cancel_token`。
- 重复 start 前必须先 shutdown 旧 manager（桌面 lifecycle / 移动
  set_net_manager 已内置该保护）——静默覆盖 = cancel_token 永不触发的
  永久泄漏。
- 回归测试锚点：`e2e_transfer.rs::shutdown_node_goes_offline_on_peer`。

**相关文件**：桌面仓 `crates/core/src/infra/`、`libs/core`（reservation
生命周期机制）；[src/app/settings/network.tsx](../../src/app/settings/network.tsx)（公网可达性开关与状态行）

### 不在 AppState 切换时自动 shutdown/start

文件选择器、系统弹窗等"瞬间退台"场景会反复重建 NetManager 打断传输，UI 还会出现"还没有配对设备"
的窗口期。节点开关由用户在 `NodeControlSheet` 显式控制；iOS 后台会自然挂起 socket，Android
由 Doze 限制，无需主动关。

**相关文件**：[src/app/_layout.tsx](../../src/app/_layout.tsx)

## 通知与后台保活

### 通知栈用 react-native-notify-kit,不是 expo-notifications

`expo-notifications` 不支持进度条 / ongoing / 前台服务,且 Notifee 已于 2026-04 archive;统一用
其官方钦点的社区维护 fork **`react-native-notify-kit`**(API 与 Notifee 一致,`import notifee` 用法不变)。
所有本地通知走这一条栈:告警(配对/传输 offer)、Android 前台服务保活、传输进度通知。

**关键约束/坑**：

- **前台展示不需要 handler**:notify-kit 的 `displayNotification` 前台直接展示(不像 expo-notifications
  必须 `setNotificationHandler` 否则前台被抑制)。
- **FGS 保活靠"空转 JS runner"**:`registerForegroundService(() => new Promise(() => {}))` —— runner
  永不 resolve,保活由 `stopForegroundService()` 显式拆除。FGS 是**进程级**构造,进程被钉在前台优先级即让
  native tokio/libp2p 线程继续跑,JS 不需要在后台干活。**服务必须同进程**(无 `android:process=":remote"`),
  否则看不到主进程堆里的 `net_manager`,退化成保活空壳。
- **FGS 类型用 `connectedDevice`**:规避 Android 15 对 `dataSync` 的 ~6h/24h 累计运行上限。notify-kit 库
  自带的 `app.notifee.core.ForegroundService` 声明未带 `foregroundServiceType`、也没声明
  `FOREGROUND_SERVICE_CONNECTED_DEVICE` 权限,由本地 config plugin
  [plugins/with-android-foreground-service.js](../../plugins/with-android-foreground-service.js) 补上
  (Android 14+ 必须显式声明,否则 `MissingForegroundServiceTypeException`)。
- **FGS 生命周期绑 `runtimeState`**:`mobile-core-store` 的 `startNode` running 后 `startForegroundKeepAlive`、
  `shutdownNode` 时 `stopForegroundKeepAlive`(node running ⇔ FGS up)。同一条 FGS 通知在 active transfer
  期间由 `event-bus` 的 `TransferProgress` 驱动更新进度(进度条 + pause/cancel action),结束回 idle 文案。
- **iOS 深后台收不到,是平台边界不是 bug**:进程挂起后 libp2p relay 预约掉线、无 store-and-forward,对端寻址
  不到;不引服务器 APNs。iOS 只保证前台/刚退台;传输进度 iOS 仅应用内(Live Activities 另立项)。所有
  foreground-service 调用都 `Platform.OS === "android"` 守卫。
- **权限申请时机**:onboarding 完成「进入」时申请(主,有上下文,优于冷启动裸弹;iOS 只有一次机会)+ 节点
  启动兜底 + 设置页「通知」深链系统设置。不要在 app 冷启动第一帧裸弹。

**相关文件**：[src/core/notifier.ts](../../src/core/notifier.ts),
[src/core/foreground-service.ts](../../src/core/foreground-service.ts),
[src/core/notifications.ts](../../src/core/notifications.ts),
[src/core/notification-router.ts](../../src/core/notification-router.ts),
[src/stores/mobile-core-store.ts](../../src/stores/mobile-core-store.ts)

## 分享到 SwarmDrop（入站 share intent）

### 从别的 App 分享文件进来走 expo-share-intent + 现有 startSend

系统分享(iOS Share Extension / Android `ACTION_SEND`)由 `expo-share-intent` 接管:根布局
[src/app/_layout.tsx](../../src/app/_layout.tsx) 包 `ShareIntentProvider`,`ShareIntentHandler`
把 `shareIntent.files` 映射成 `TransferFile[]`([src/core/share-intent.ts](../../src/core/share-intent.ts))
塞进**非持久化**的 [share-store](../../src/stores/share-store.ts) → push
[src/app/send/share-target.tsx](../../src/app/send/share-target.tsx)(选设备屏)→ 复用
`transfer-store.startSend({ files, peerId, peerName })`。

**关键约束/坑**：

- **必须有 `src/app/+native-intent.tsx`(否则 Unmatched Route 404)** —— iOS Share Extension 用
  `swarmdrop://dataUrl=<getShareExtensionKey()>?nonce=…` 拉起主 App,这个 URL **不是路由**;不拦截的话
  expo-router 会当页面路径解析 → 白屏 "Unmatched Route"。`redirectSystemPath` 识别到 `dataUrl=<key>`
  就 `return "/"`(其余原样放行,不破坏 `swarmdrop://` 深链),分享数据由原生模块 keyed 保存,交给
  `ShareIntentHandler` 的 `hasShareIntent` 再 push。**iOS 模拟器 Maestro E2E 实测抓到的坑**。
- **库给的是 `file://` path,不是 content://** —— `expo-share-intent` 已把分享项拷成 App 拥有的
  `file://`(Android content:// 读权限是临时的、不能 `takePersistableUriPermission`,撑不过长传;
  iOS 经 App Group 容器)。正好等于 `TransferFile.sourceId` 形态,直接喂 `prepareSend`,**core 零改动**。
- **不要在发送后删这份拷贝** —— `startSend` 返回时传输才启动,`ForeignFileAccess.readSourceChunk`
  后续还在读;删了会毁传输。v1 靠 OS cache 清理,主动清理须挂「传输完成」事件。
- **Android `androidIntentFilters` 只能 `text/*|image/*|video/*|*/*`** —— 要接通用文件必须 `*/*`,
  但这会让 SwarmDrop 也出现在**文本分享**里。所以 handler 对「无文件的分享」(纯文本/URL)直接
  `toast` 提示 + `resetShareIntent()`,不进发送流。iOS 侧靠 `iosActivationRules`(不含 text)精确排除。
- **iOS 需 App Group** `group.com.yexiyue.swarmdrop`(插件 `iosAppGroupIdentifier`),要在 Apple
  账号注册 + provisioning;Share Extension 有内存/时限,超大文件拷入 App Group 有被杀风险(已知限制)。
- **原生**:`expo-share-intent` 是 config plugin + 原生模块 → 改动后须 `prebuild` + 重建原生桥
  (`build:ios/android`)+ app build。未 build 的包 import 它会崩(iOS 尤甚)。未过引导时 v1 直接
  toast「先完成设置」+ 放弃本次分享(不暂存)。

**相关文件**：[src/app/_layout.tsx](../../src/app/_layout.tsx),
[src/app/+native-intent.tsx](../../src/app/+native-intent.tsx),
[src/app/send/share-target.tsx](../../src/app/send/share-target.tsx),
[src/core/share-intent.ts](../../src/core/share-intent.ts),
[src/stores/share-store.ts](../../src/stores/share-store.ts),[app.json](../../app.json)

## 发送流程

### sendPrepared 的 file_ids 必须传 prepared 的全量（除非 UI 提供子集选择）

`prepared.files` 是 core prepareSend 后产出的文件列表，`sendPrepared(..., fileIds)` 的 fileIds
是用来做"子集筛选"的——传空数组会被 core 当作"未选任何文件"拒绝（见 `send.rs`）。当前 UI 没有
子集选择，必须传 `prepared.files.map((f) => f.fileId)`。

**正确做法**：

```ts
const prepared = await getMobileCore().prepareSend(selectedFiles);
const result = await getMobileCore().sendPrepared(
  prepared.preparedId,
  peerId,
  peerName,
  prepared.files.map((f) => f.fileId),
);
```

**相关文件**：[src/app/send/select-device.tsx](../../src/app/send/select-device.tsx)

## 配对码

### 设备首页直接暴露附近设备和本机配对码

移动端设备首页应跟桌面端设备中心的信息架构一致：`添加设备` 是首页常驻区块，里面直接展示
附近设备、本机配对码和输入配对码入口。入口要直出，但不要把 6 位输入框内嵌在卡片里；点击
`输入配对码` 后用 bottom sheet 承载输入、校验和跳转，避免首页卡片被表单状态撑开。sheet 也适合
设备确认、节点控制这类二级流程。

节点控制不要再做首页独立卡片。右上角 `StatusPill` 已经承担节点状态和入口；未启动时，添加设备区
退化为空状态卡片，只保留“节点未启动”、启动节点快捷按钮，以及启动后会显示附近设备/本机配对码的说明。
首页其他空状态也应保持模块感：使用图标、标题、描述和稳定高度，不要退化成一行短提示。

**相关文件**：[src/app/(main)/index.tsx](../../src/app/(main)/index.tsx)

### 配对码全局单例 + 持久化 + 过期/被消耗自动续生

`pairing-code-store` 管理一个全局 `MobilePairingCode`：UI 多处展示同一份；过期或被消耗后
自动重新生成。TTL = 600s。具体生命周期约定在 store 顶部 docstring（参考 Rust 端
`pairing/manager.rs:271-285`）。

**正确做法**：UI 直接订阅 store，不要自己调 `generatePairingCode`，避免出现多份冲突的码。

**相关文件**：[src/stores/pairing-code-store.ts](../../src/stores/pairing-code-store.ts)

## 接收文件保存位置

### `receivePath` 持久化 + `resolveReceiveLocation()` 单一入口

接收方保存目录由用户在「设置 → 通用 → 传输 → 接收位置」配置：调用
`Directory.pickDirectoryAsync()` 拿到目录 URI（iOS file://、Android SAF content://）持久化到
`preferences-store.receivePath`。未配置时退到 `getMobilePaths().transfersInboxUri`（应用私有
Documents/transfers）。

**正确做法**：所有 `acceptReceive(sessionId, location)` 的 location 一律走
`resolveReceiveLocation()`（在 [src/core/paths.ts](../../src/core/paths.ts)），不要再直接读
`getMobilePaths().transfersInboxUri`——那样会绕过用户的配置。

**相关文件**：

- [src/stores/preferences-store.ts](../../src/stores/preferences-store.ts)
- [src/core/paths.ts](../../src/core/paths.ts)
- [src/components/transfer-offer-host.tsx](../../src/components/transfer-offer-host.tsx)
- [src/app/settings/general.tsx](../../src/app/settings/general.tsx)

### SAF (content://) chunk write —— 必须保持 FileHandle 打开

Android 用户选「Downloads」「Movies」这类系统目录时，picker 返回的是 SAF
`content://com.android.externalstorage.documents/tree/...`。expo-file-system 56 通过
`ContentResolver.openFileDescriptor` 真正支持 SAF chunk write，但有两个硬约束：

1. **SAF 不能用 `FileMode.ReadWrite`**：只允许 `WriteOnly / Append / Truncate / ReadOnly`
2. **SAF "w" mode open 时大概率 truncate**：DocumentsProvider 实现普遍如此。
   如果按 chunk 反复 open/close，每次都丢失之前内容 → 文件最终只剩最后一个 chunk

**正确做法**：sink 生命周期内**保持 FileHandle 打开**，所有 chunk 复用同一个 handle。
[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts) 的 OpenSink
在 createSink/openOrCreateSink 阶段 open，writeSinkChunk 复用 sink.handle 仅 seek + write，
finalize/cleanup 才 close。file:// 路径也走同一逻辑（持久 handle 比每 chunk open/close 更快）。

**SAF 路径不能拼 path**：`new File(dir, "a/b/c.txt")` 在 SAF tree 下不工作。要逐层
`dir.createDirectory(name)` 建子目录，叶子用 `dir.createFile(name, "application/octet-stream")`。
`ensureSafSinkFile` 实现了这个逻辑。

**`createFile` 的 mimeType 必须传 `"application/octet-stream"`，不能传 null**：

- 看起来 null 应该让 provider 按文件名后缀推断，但 expo-file-system 56 的 Android 端
  在 `FileSystemDirectory.kt:79` 写的是 `file.createFile(mimeType ?: "text/plain", fileName)` ——
  null 被兜底成 `text/plain`。
- 然后 `DocumentsContract.createDocument(mimeType="text/plain", "foo.md")` 进入
  AOSP `FileUtils#splitFileName`：发现 `.md` 没在 MimeTypeMap 里、跟传入的 `text/plain` 不匹配，
  按规则强制追加 mimeType 的「标准扩展」`.txt` → 落盘变成 `foo.md.txt`。
- `application/octet-stream` 是 `ContentResolver.MIME_TYPE_DEFAULT`，`splitFileName`
  对它特判 `extFromMimeType=null`，于是 displayName 原样保留 —— SAF 下「不要动我文件名」
  的标准约定。任何不想被 SAF 改后缀的二进制 / 任意类型文件都该用这个。

**为什么这么做**：P2P 传输按 chunk + 任意 offset 写入（断点续传 / 并发），不是顺序追加。
Append 模式 SAF 下不能 seek（文档明说），所以唯一能 work 的就是 WriteOnly + 持久 handle。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)

### 文件树（headless-tree）展开/折叠必须 forceUpdate

`@headless-tree/core` 的 `expand()` / `collapse()` 直接 mutate 内部 `state` 对象（只换
`state.expandedItems` 的数组引用，外层对象引用不变），然后回调 `config.setState(state)`。
React 的 `useState` 通过 `Object.is` 判等，看到同一引用 → bail out → 不重渲染 → 文件夹
点击没反应。

桌面端不受影响：用 `@tanstack/react-virtual` 直接订阅 itemMeta 变化，不依赖 React state
触发；移动端非虚拟化路径直接 `tree.getItems()` map，必须 React 重渲才能看到新顺序。

**正确做法**：把 expand/collapse 收敛到一个 handler，里面 `forceUpdate()` 触发重渲：

```tsx
const [, forceUpdate] = useReducer((x: number) => x + 1, 0);
const handleToggle = useCallback((item) => {
  if (item.isExpanded()) item.collapse(); else item.expand();
  forceUpdate();
}, []);
```

**为什么 setConfig 那条路也不顶用**：useTree 在 setConfig 里检查
`newConfig.state.expandedItems !== state.expandedItems`，但 mutate 后两边指向同一个新数组，
判等还是 `false` → 也不 rebuild。直接 forceUpdate 最简单可靠。

**相关文件**：[src/components/file-tree/file-tree.tsx](../../src/components/file-tree/file-tree.tsx)

### SAF 接收偶发 "Bad file descriptor"（root cause 未确认）

Android SAF 接收一批文件时偶发卡 0% + `FileChannelImpl.position0` 报
`java.io.IOException: Bad file descriptor`。同一目录下文件多时更容易触发，少时常常 OK。

**已确认的修复**（生效在 `ensureSafSinkFile`）：truncate 路径不再 `delete()` + `createFile()`，
而是直接复用 existing entry —— 让 `open(WriteOnly)` 自己 truncate-on-open。原写法会让
SAF 的异步 delete 没生效就被 createFile 命中 race，生成 `foo (1).txt` 或返回不可写 fd。

**未确认的假设**：

1. **dot 前缀 entry**：`.git/.vscode/.gitignore` 等 dotfile/dotdir。SAF DocumentsProvider
   对 dot 前缀 name 的处理某些实现下不规范（早期 55 版本 `.swarmdrop-probe` 探测时被识别成
   folder 就是同样症状）。**暂未在 56 上确认是否复现**，先不在发送/接收侧硬过滤。

2. **批量 race**：N 个 sink 同时 open/close SAF fd，DocumentsProvider 内部 fd table
   出现冲突。如果是这个，要做的是 limit 并发 sink 数 / 序列化 createFile 调用。

**调查建议**：下次复现时记录失败文件的 relativePath，确认是不是 dot 前缀；同时观察并发的
sink 数。如果 dot 前缀确认，发送侧默认跳过 + 接收侧防御都加上。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)
