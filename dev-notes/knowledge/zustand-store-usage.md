# Zustand Store Usage

桌面端使用 Zustand v5 管理运行时领域状态，TanStack Router 管理可导航 UI 状态。`getState/setState`
不是禁用 API，但必须作为受控的命令式边界使用，不能散落在普通 React 组件里。

## 组件层只用 selector

React 组件和 hooks 需要状态或 action 时，使用 `useXStore(selector)`。事件 handler 里也先通过
selector 取 action，再调用 action。

**正确做法**：
- 单字段：`const status = useNetworkStore((s) => s.status)`
- 单 action：`const startNetwork = useNetworkStore((s) => s.startNetwork)`
- 多字段：拆成多个 selector，或用 `useShallow((s) => ({ ... }))`

**不要做**：
- 不要在普通组件里写 `useXStore.getState().someAction()`
- 不要让 selector 每次返回新的对象、数组、`map/filter` 结果却不包 `useShallow`

**相关文件**：`src/routes/_app/send/share-target.lazy.tsx`,
`src/routes/_app/devices/index.lazy.tsx`, `src/hooks/use-node-restart.ts`

## Store 内部只用 create 闭包

store creator 内部的 action、timer、helper 应使用 `create((set, get) => ...)` 传入的闭包访问自身状态。
如果 helper 定义在 create 外部，显式把需要的 action 或 `set/get` 传进去。

**正确做法**：
- action 内读当前状态用 `get()`
- action 内写状态用 `set(...)`
- timer 回调调用闭包内 action，而不是导出的 hook

**不要做**：
- 不要在 store 文件内部反向调用导出的 `useXStore.getState/setState`

**相关文件**：`src/stores/pairing-store.ts`

## getState/setState 白名单

允许 `getState/setState` 的场景必须是非 React 响应式边界：

- Tauri event bridge：窗口、托盘、外部打开、网络、传输事件
- TanStack Router `beforeLoad`
- 同步 utility：没有 hook 上下文且必须立即读取最新快照
- 测试 setup / 断言

新增调用前先跑：

```bash
pnpm check:zustand-access
```

如果脚本失败，优先改成 selector/action selector；只有确认属于命令式边界时，才更新
`scripts/check-zustand-store-access.mjs` 的 allowlist，并写清楚 boundary reason。

**相关文件**：`scripts/check-zustand-store-access.mjs`

## Route 拥有可导航 UI 状态

能刷新恢复、深链进入、参与返回/前进历史的 UI 状态必须放在 route path/search params 里。Zustand
只保存跨页面共享的领域状态、缓存、偏好和运行时状态。

**正确做法**：
- `/transfer?session=...&filter=...`
- `/inbox?item=...&q=...&archived=true`
- `/send?peerId=...&session=...`

**不要做**：
- 不要用 store 的 `selected/current` 字段模拟子页面路由
- 不要通过写 store 触发伪导航；外部入口也应走标准 route

**相关文件**：`src/routes/_app/transfer/index.tsx`, `src/routes/_app/send/index.tsx`,
`src/routes/_app/send/share-target.tsx`

## UI 需要结果时 action 要返回结果

如果 UI 需要根据启动/停止/重启等命令显示成功或失败，action 应返回明确结果或抛错。不要让 UI 调完
action 后再 `getState()` 回读 store 快照推断真实结果。

**相关文件**：`src/hooks/use-node-restart.ts`, `src/stores/network-store.ts`

## persist 的写入是 fire-and-forget —— 终止进程前必须 flush

`persist` 中间件在包装后的 `api.setState` 里**同步**调用 `storage.setItem()` 但**不 await**
它。所以「store 里改了」与「后端收到了」之间隔着一次 IPC 往返，而 `commands.quitApp()` /
`relaunch()` 都是同步命令、Rust 侧当场结束进程——同一个 tick 里「改完就退」必然丢。

**判据是时间尺度，不是「附近有没有写偏好」**：托盘「退出」与 macOS `Cmd+Q` 同样紧邻着偏好
写入（刚在设置页改完就按 Cmd+Q），但中间隔着一次人类操作（≥100ms，IPC 是 ~1ms），概率上
输不了；程序化的「改完立刻退」才是必输。新增终止路径时该问的是「中间隔着人吗」。

**正确做法**：
- 前端所有终止进程的路径都走 `src/lib/quit-app.ts`（`quitApp` / `relaunchApp`），
  它先 `await flushTauriStores()`。机器门禁是 `pnpm check:quit-entry`
- `StateStorage` 适配器要按文件**串行化**写入：persist 每次写的是全量快照，乱序到达
  等于后一次被前一次的旧快照覆盖（首次 `setItem` 要等 `load()`、紧随其后走缓存的第二次
  会先到，这个并发是真实的）
- 写入失败要显式 `console.error`：persist 丢弃了 `setItem()` 返回的 promise，不主动报
  就没有任何信号

**不要做**：
- 别把 `autoSave` 的 100ms 防抖当成丢失原因。值只要进了后端内存就安全了——
  tauri-plugin-store 会在 `RunEvent::Exit` 时保存所有 store，`Store::save` 还会取消防抖。
  漏的只有「那次 IPC 没赶上」，所以 flush 里不可替代的是 drain 而不是 `save()`
- 别指望把这件事下沉到 Rust：故障是「值还没到后端」，Rust 侧没有 API 能观测 webview 里
  有几个写还在路上，`quit_app` 里 `save()` 只会把旧值写盘、日志还全绿

**相关文件**：`src/lib/quit-app.ts`, `src/lib/tauri-store.ts`,
`scripts/check-quit-entry.mjs`, `src/components/close-behavior-manager.tsx`
