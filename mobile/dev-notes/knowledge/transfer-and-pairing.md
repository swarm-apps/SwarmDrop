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
只负责把 `swarmdrop_core::database::ops::TransferProjection` 镜像成
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

### 离线兜底视图走 keychain，不依赖 NetManager

NetManager（Rust 端 P2P 节点）未启动时，`listDevices` 返回空。已配对设备的"离线视图"靠
Rust `list_paired_devices()` FFI 直接读 keychain，独立于节点状态。zustand 存 cache 字段
`pairedDevicesCache`，在 `loadIdentity` 完成时立即拉一次，PairingCompleted 事件再刷新。

**正确做法**：组件渲染时按 `runtimeState === "running"` 切换数据源：

```tsx
const pairedDevices = useMemo(() => {
  if (runtimeState === "running") return devices.filter((d) => d.isPaired);
  return summariesToOfflineDevices(pairedDevicesCache);
}, [runtimeState, devices, pairedDevicesCache]);
```

**不要做**：在 `applyDevices` / `refreshDevices` 里把 `pairedDevicesCache` 跟 `devices` 一起
覆盖——NetManager 还没发现的 paired 设备会被空覆盖，UI 闪烁。

**相关文件**：[src/stores/mobile-core-store.ts](../../src/stores/mobile-core-store.ts),
[src/app/send/select-device.tsx](../../src/app/send/select-device.tsx)

## 节点生命周期

### 不在 AppState 切换时自动 shutdown/start

文件选择器、系统弹窗等"瞬间退台"场景会反复重建 NetManager 打断传输，UI 还会出现"还没有配对设备"
的窗口期。节点开关由用户在 `NodeControlSheet` 显式控制；iOS 后台会自然挂起 socket，Android
由 Doze 限制，无需主动关。

**未来工作**：大文件长传保活靠 Android Foreground Service / iOS BGTask，跟 AppState 监听解耦。

**相关文件**：[src/app/_layout.tsx](../../src/app/_layout.tsx)

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
