# Transfer & Pairing

## 概览

SwarmDrop-RN 的核心业务是 P2P 文件传输 + 设备配对。状态层在
[src/stores/mobile-core-store.ts](../../src/stores/mobile-core-store.ts)（zustand）+ Rust core
事件通过 [src/core/event-bus.ts](../../src/core/event-bus.ts) 路由。配对码用单例 store
（[src/stores/pairing-code-store.ts](../../src/stores/pairing-code-store.ts)）。

下面记录的都是"从代码看不出来的约束"——FFI 边界、core 端的语义、zustand 订阅的坑。

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
`dir.createDirectory(name)` 建子目录，叶子用 `dir.createFile(name, null)`。`ensureSafSinkFile`
实现了这个逻辑。

**为什么这么做**：P2P 传输按 chunk + 任意 offset 写入（断点续传 / 并发），不是顺序追加。
Append 模式 SAF 下不能 seek（文档明说），所以唯一能 work 的就是 WriteOnly + 持久 handle。

**相关文件**：[src/core/foreign-file-access.ts](../../src/core/foreign-file-access.ts)
