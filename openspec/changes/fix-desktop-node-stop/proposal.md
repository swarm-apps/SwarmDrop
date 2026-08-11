## Why

桌面端在「自动启动节点」开关打开时，用户**无法停止节点**：`stopNetwork()` 把状态置为
`stopped` 的那一刻，`src/routes/_app.tsx:67` 那个依赖 `networkStatus` 的 effect 立即命中
`autoStart && status === "stopped"` 并重新启动，UI 表现为「点了停止、状态一闪就回到运行中」。

根因是语义与实现的错位：设置文案（「解锁后自动启动 P2P 网络节点」）与该 effect 自己的注释
（「解锁后**首次进入时**检查」）表达的都是**一次性的冷启动意图**，但代码写成了
`stopped → running` 的**持续收敛环**——于是「已停止」这个状态在开关打开时根本不可达。

三端里只有桌面写成了响应式：Web 端 `docs/app/app/_components/web-node-bootstrap.tsx:49`
是空依赖 `useEffect`，移动端 `mobile/src/stores/mobile-core-store.ts:176` 是冷启动序列里
命令式读一次。本次让桌面回到同一形态。

## What Changes

- **删除** `src/routes/_app.tsx` 中依赖 `networkStatus` 的自动启动 effect。
- **新增** 冷启动阶段的一次性自动启动判定，落点是 `src/main.tsx` 已有的启动序列
  （`waitForPreferencesHydration()` + `rehydrateSecretStore()` 之后）——该位置同时满足
  `autoStart` 偏好已 hydrate、设备身份已初始化（`startNetwork` 的前置条件）两个条件。
  自动启动**不阻塞首屏**（不 await 进 `setIsLoaded` 的链路）。
- **消除** `useNodeRestart.restart()` 的抢跑竞态：现状是 `await stopNetwork()` 一落地，
  上述 effect 抢先启动节点，`restart()` 自己那次 `startNetwork()` 撞上
  `status === "starting"` 的幂等门禁直接返回 `true`——真正生效的是 effect 那次启动。
  effect 删除后，`restart()` 的两步回到它自己手里。
- **不改**任何 Rust 代码、IPC 契约或后端行为。

## Capabilities

### New Capabilities
- `node-autostart`: 应用冷启动时按用户偏好自动启动 P2P 节点的行为，及其对偶不变量
  ——「用户显式停止后，节点保持停止直到用户再次启动」。含三端形态一致性判据。

### Modified Capabilities
（无。`node-control-sheets` 的「Confirm stop triggers network shutdown」这条需求本身没变，
变的是停止之后的稳态，那条不变量归属新建的 `node-autostart`。）

## Impact

| 文件 | 改动 |
|---|---|
| `src/routes/_app.tsx` | 删除自动启动 effect 及其 `autoStart` / `networkStatus` 订阅 |
| `src/main.tsx` | 冷启动序列末尾追加一次性自动启动判定 |
| `src/hooks/use-node-restart.ts` | 无需改动，但其竞态随 effect 删除而消失（补测试锁定） |
| `src/stores/network-store.ts` | 可能新增可测的启动入口；`startNetwork` 的幂等门禁保持不变 |

- **平台范围**：仅桌面。Web 与移动端不动（它们的形态就是本次的目标形态）。
- **风险**：onboarding 未完成的首启用户不应被自动启动。`autoStart` 默认 `false`，新用户
  构造上不会命中，但落点从路由布局移到 `main.tsx` 后失去了「已进入主布局」这个隐含守卫，
  需在 design 中给出显式判据。
- **无 breaking change**：偏好键、IPC、持久化格式均不变。
