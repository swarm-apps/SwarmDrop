## Context

桌面的自动启动写在 `src/routes/_app.tsx:67`：

```tsx
// 自动启动节点(解锁后首次进入时检查)      ← 「首次」只存在于注释里
useEffect(() => {
  if (autoStart && networkStatus === "stopped") {
    void startNetwork().then(...)
  }
}, [autoStart, networkStatus, startNetwork]);
```

`networkStatus` 在依赖数组里，于是它不是「进入布局时检查一次」，而是一个持续运行的
`stopped → running` 收敛环。`stopNetwork()` 的最后一步正是 `set({ status: "stopped" })`，
两者构成闭环：**开关打开时，「已停止」是一个不可达状态**。

三端的同一件事：

| 端 | 形态 | 位置 | 停止后被拉回 |
|---|---|---|---|
| Web | `useEffect(..., [])` 空依赖 | `docs/app/app/_components/web-node-bootstrap.tsx:49` | ❌ |
| 移动 | 冷启动序列中命令式读一次 | `mobile/src/stores/mobile-core-store.ts:176` | ❌ |
| 桌面 | `useEffect` 依赖 `networkStatus` | `src/routes/_app.tsx:67` | ✅ |

移动端那处的注释已经把目标判据写死了：「仅在用户开启「自动启动节点」时冷启动一次,
后续用户手动停止不会被这里重启。」本次是把桌面对齐到这条既有判据，不是发明新语义。

桌面已有一条冷启动序列，在 `src/main.tsx:34`：

```tsx
Promise.all([waitForPreferencesHydration(), rehydrateSecretStore()])
  .then(() => syncDeviceNameFromBackend())
  .finally(() => setIsLoaded(true));
```

它同时满足自动启动需要的两个前置条件：偏好已 hydrate（读得到 `autoStart`）、
设备身份已初始化（`useSecretStore.deviceId` 就绪 —— `startNetwork` 拿不到它会直接失败）。
`ReactDOM.createRoot(...).render(<App />)` **未包 `StrictMode`**，该 effect 只跑一次。

## Goals / Non-Goals

**Goals:**
- 用户显式停止节点后，节点保持停止，直到用户再次显式启动。
- `autoStart` 回归「冷启动时判定一次」的语义，与另外两端形态一致。
- 自动启动的判定逻辑可被单元测试直接调用，不依赖渲染 `_app` 布局。
- 消除 `useNodeRestart.restart()` 被自动启动 effect 抢跑的竞态。

**Non-Goals:**
- 不改 `autoStart` 的产品语义（不引入「保持在线 / 掉线自动重连」）。若将来要做，那是另一个
  需求，且需要先改设置文案——现文案「解锁后自动启动 P2P 网络节点」不支持那个解读。
- 不动 Web 与移动端（它们已是目标形态）。
- 不动任何 Rust 代码、IPC 契约、后端关停链路。节点关停链路上另外两处已知隐患
  （`NetManager::shutdown` 首步 `announce_offline` 无超时；`Endpoint::close` 的
  `actor_tx.send` 无超时）**不在本次范围**——它们的表现是「转圈不返回」，与本 change 修的
  「立刻被拉回」是不同故障，混进来会让验收判据失焦。
- 不为「打开开关的当下」补任何 UI 提示（见 D5）。

## Decisions

### D1：判定移进 `main.tsx` 的冷启动序列，而不是给 effect 加 ref 守卫

**选择**：删除 `_app.tsx` 的 effect，在 `main.tsx` 的启动序列末尾做一次性判定。

**备选与否决理由**：
- *ref 一次性守卫*（`autoStartedRef` 挡住第二次）：三行改完，但「只跑一次」是靠一个 ref
  碰巧成立的，语义仍然是「监听状态变化」。下一个读这段代码的人看到的还是一个收敛环，
  只是被压住了——本 bug 的成因正是「注释说一次、代码说持续」，用 ref 修等于把同一条缝
  从注释挪到 ref 上。
- *加 `stoppedByUser` 标志*：那是把 `autoStart` 重新解读为「保持在线」并给它一个 override。
  与现有设置文案冲突，属于产品语义变更，不是修 bug。

**理由**：`autoStart` 是**启动序列的一环**，不是 UI 状态的函数。`main.tsx` 的序列已经存在、
已经在做同类事情（偏好 hydration → 身份初始化 → 设备名同步），把它接在末尾是让代码位置
与语义对齐，而不是新增机制。

### D2：必须排在 `syncDeviceNameFromBackend()` 之后

移动端在 `mobile-core-store.ts:170` 留了这条判据：「必须在下面 autoStart 之前跑完,
否则冷启动那一次节点用的还是旧名字。」桌面同理——`syncDeviceNameFromBackend()` 把后端
（事实源）的设备名推进前端缓存，节点启动时 `identify` 广播的 `agent_version` 取的就是它。
顺序颠倒的表现是**冷启动那一次对端看到的是旧名字**，且要等下次改名或重启才纠正。

因此落点是 `.then(() => syncDeviceNameFromBackend())` 之后，不是 `Promise.all` 里并行。

### D3：不阻塞首屏

自动启动挂在序列尾部但**不进入决定 `setIsLoaded(true)` 的链路**（fire-and-forget）。
节点启动含网络绑定与 bootstrap 拨号，把它 await 进首屏门禁会让「开了自动启动的用户」
每次冷启动多盯几秒白屏。失败反馈沿用现状：`startNetwork()` 内部已 `set({status:"error"})`
并 toast 原因，调用点不需要第二套提示。

### D4：onboarding 未完成时不自动启动

原落点 `_app.tsx` 是主布局，隐含「已过 onboarding」这个前提；移到 `main.tsx` 后这个前提
消失了，必须显式给判据。

**判据取 `useSecretStore.deviceId` 是否就绪**，而不是「是否在 onboarding 路由上」：
`startNetwork()` 本来就以 `deviceId` 为前置条件（`network-store.ts:123`，缺失时置 error
并 toast「节点启动失败」）。首启用户身份初始化成功但还没设备名的场景下，`autoStart` 默认
`false`，构造上不会命中；真要命中也只是启动一个匿名节点，不构成数据风险。

**不引入路由判断**：`main.tsx` 在 `RouterProvider` 之前跑，此时问路由既拿不到也不该拿。

### D5：「打开开关的当下」不再立即启动节点 —— 有意的行为变化

现状下，用户在节点停着时打开这个开关会立刻启动节点（effect 命中）。改后要等下次冷启动。

这是**有意保留的行为变化**，不做补偿提示：开关的文案就是「自动启动」（描述的是下次启动
的行为），而「我现在就想启动」有节点状态弹窗那颗按钮，就在同一个操作半径内。加一条
「下次启动生效」的说明是在为一个用户并不会困惑的地方增加噪音。

### D6：判定逻辑提取成可直接调用的函数

放在 `network-store` 或 `src/lib/` 下的一个具名函数（如 `autoStartNodeIfEnabled()`），
`main.tsx` 只负责在序列末尾调它。理由是可测性：`_app.tsx` 那个 effect 现在**没有任何测试
覆盖**——要测它得渲染整个布局路由，这正是它带着这个 bug 活到今天的原因之一。提取之后，
「开关关时不启动 / 开关开时启动一次 / 停止后不再启动」三条都能在 `network-store.test.ts`
的同一套 mock 上直接断言。

### D7：`restart()` 的竞态靠删除 effect 自然消失，但要补测试锁定

`useNodeRestart.restart()` 是 `await stopNetwork()` → `await startNetwork()`。现状下
stop 一落地 effect 就抢先启动，`restart()` 自己那次撞上 `status === "starting"` 的幂等
门禁返回 `true` —— **真正生效的是 effect 那次启动**，两次读的都是同一份偏好所以结果碰巧
一致。effect 删除后这条路径回到 `restart()` 自己手里。

不改 `restart()` 的代码，但补一条测试断言「restart 期间 `commands.start` 恰好被调用一次」，
把这条竞态钉死——否则将来有人以任何形式把自动启动接回响应式，它会静默复活。

## Risks / Trade-offs

- **[将来给 `App` 包上 `StrictMode`，effect 跑两次]** → `startNetwork()` 的
  `status === "running" || "starting" → return true` 幂等门禁已覆盖；D6 的提取函数不额外
  持有状态，重复调用无副作用。

- **[用户期待「自动启动 = 一直在线」，改后掉线不再自动恢复]** → 现状其实也不提供这个：
  effect 只在 `status === "stopped"` 时触发，而异常掉线走的是 `status: "error"` 分支
  （`network-store.ts:175`），本来就不会被拉起。**本次不改变掉线后的行为**，只改变
  「用户主动停止后」的行为。

- **[自动启动失败时用户看不到原因]** → 沿用 `startNetwork()` 内部的 toast + `error` 状态，
  与手动启动失败同一条反馈路径。原 effect 里那句 `console.warn("[auto-start] ...")`
  是纯开发日志，删除不影响用户可见性。

- **[`main.tsx` 承担的职责变多]** → 它已经是冷启动序列的所在地（偏好 / 身份 / 设备名），
  再接一步同类事项不改变其性质。判定逻辑本身住在 D6 的具名函数里，`main.tsx` 只有一行调用。

## Open Questions

无。三个决策点（落点、顺序、开关当下是否立即启动）在 design 内已定，其余是实现细节。
