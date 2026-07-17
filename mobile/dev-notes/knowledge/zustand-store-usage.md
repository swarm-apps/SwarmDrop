# Zustand Store Usage

SwarmDrop-RN 使用 Zustand v5 管理 mobile core 运行态、配对码、传输、收件箱、通知和偏好。Expo
Router 负责页面结构；store 不承担可导航 UI 状态。`getState/setState` 只作为 native/event/utility
边界使用，普通组件和 store creator 内部都要避免。

## 组件层只用 selector

React Native 页面、组件和 hooks 需要状态或 action 时，使用 `useXStore(selector)`。事件 handler
里也先通过 selector 取 action，再调用 action。

**正确做法**：
- 单字段：`const runtimeState = useMobileCoreStore((s) => s.runtimeState)`
- 单 action：`const markConsumed = usePairingCodeStore((s) => s.markConsumed)`
- 多字段：拆成多个 selector，或用 `useShallow((s) => ({ ... }))`

**不要做**：
- 不要在普通组件里写 `useXStore.getState().someAction()`
- 不要让 selector 每次返回新的对象、数组、`map/filter` 结果却不包 `useShallow`

**相关文件**：`src/components/pairing-request-host.tsx`, `src/app/settings/network.tsx`

## Store 内部只用 create 闭包

store creator 内部的 action、timer、异步 helper 应使用 `create((set, get) => ...)` 传入的闭包访问自身状态。
如果 helper 定义在 create 外部，显式把需要的 action 或 `set/get` 传进去。

**正确做法**：
- action 内读当前状态用 `get()`
- action 内写状态用 `set(...)`
- timer 回调用闭包 action 续生，而不是引用导出的 hook

**不要做**：
- 不要在 store 文件内部反向调用导出的 `useXStore.getState/setState`

**相关文件**：`src/stores/pairing-code-store.ts`

## getState/setState 白名单

允许 `getState/setState` 的场景必须是非 React 响应式边界：

- native / UniFFI event bridge：`src/core/event-bus.ts`
- 同步 utility：例如保存路径、设备名这类没有 hook 上下文的同步流程
- mobile-core 生命周期编排：启动、停止、读取偏好、清理配对码
- 测试 setup / 断言

新增调用前先跑：

```bash
pnpm check:zustand-access
```

如果脚本失败，优先改成 selector/action selector；只有确认属于命令式边界时，才更新
`scripts/check-zustand-store-access.mjs` 的 allowlist，并写清楚 boundary reason。

**相关文件**：`scripts/check-zustand-store-access.mjs`, `src/core/event-bus.ts`

## UI 需要结果时 action 要返回结果

如果 UI 需要根据启动、停止、重启等命令显示成功或失败，store action 应返回明确结果或抛错。不要让 UI
调完 action 后再 `getState()` 回读 store 快照推断真实结果。

**正确做法**：
- `startNode()` / `shutdownNode()` 返回 `{ ok, state, error? }`
- 设置页、节点控制 sheet、设备名修改直接消费 action 返回值

**相关文件**：`src/stores/mobile-core-store.ts`, `src/app/settings/network.tsx`,
`src/components/node-control-sheet.tsx`, `src/lib/device-name.ts`

## Persist 只保存 durable 字段

持久化 store 必须用 `partialize` 或等价机制，只保存用户偏好、onboarding 状态、已配对设备缓存等
durable 字段。运行态、错误、临时队列、历史兼容字段不要持久化。

**正确做法**：
- `preferences-store` 只持久化用户偏好和接收目录
- `mobile-core-store` 只持久化 `pairedDevicesCache`
- `onboarding-store` 只持久化 `hasOnboarded`

**不要做**：
- 不要把 `runtimeState`、`error`、`selectedFiles`、通知队列、全局 `autoAccept` 这类运行态/遗留字段持久化

**相关文件**：`src/stores/preferences-store.ts`, `src/stores/mobile-core-store.ts`,
`src/stores/onboarding-store.ts`
