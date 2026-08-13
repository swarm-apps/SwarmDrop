## ADDED Requirements

### Requirement: React 组件必须通过 selector 使用 store
桌面端 React 组件和 hooks MUST 通过 `useXStore(selector)` 读取 Zustand 状态和 action。组件层 MUST NOT 直接调用 `useXStore.getState()` 或 `useXStore.setState()`，除非该文件被明确列入命令式边界白名单。

#### Scenario: 组件事件 handler 调用 action
- **WHEN** 组件按钮 handler 需要调用 store action
- **THEN** 组件 MUST 先通过 selector 取得 action，再在 handler 中调用该 action

#### Scenario: 组件读取多个字段
- **WHEN** 组件从同一个 store 读取多个字段或 action
- **THEN** 组件 MUST 使用多个简单 selector，或使用 `useShallow` 包裹返回对象/数组的 selector

### Requirement: Store 实现必须使用 create 闭包 set/get
桌面端 Zustand store 的 action、内部 helper、timer 和异步流程 MUST 使用 `create((set, get) => ...)` 提供的 `set/get` 读写自身状态。store module 内部 MUST NOT 通过导出的 `useXStore.getState/setState` 反向访问自身。

#### Scenario: Store 内部异步 action 更新状态
- **WHEN** store action 在异步命令完成后需要写入状态
- **THEN** action MUST 使用闭包 `set` 更新状态，并使用闭包 `get` 读取当前状态

#### Scenario: Store 外部 helper 需要状态访问
- **WHEN** store helper 被定义在 create 调用之外
- **THEN** helper MUST 通过参数接收需要的 `set/get/action`，而不是引用导出的 store hook

### Requirement: 命令式 store API 必须受白名单约束
桌面端 `useXStore.getState/setState` MUST 只出现在外部事件桥接、router guard、同步 utility、测试 setup 或测试断言等非 React 响应式边界，并且 MUST 被静态 allowlist 检查覆盖。

#### Scenario: Tauri 回调读取最新偏好
- **WHEN** Tauri window close、tray、file-open 或 transfer event 回调需要读取最新 store 快照
- **THEN** 该回调 MAY 使用 `getState()`，但调用点 MUST 在 allowlist 中标注为外部事件边界

#### Scenario: 新增非白名单 getState
- **WHEN** 开发者在普通组件或 store 实现文件中新增 `useXStore.getState()`
- **THEN** 静态检查 MUST 失败，直到该调用被改成 selector/set/get 或被明确加入合理白名单

### Requirement: 跨 store 编排必须显式返回结果
桌面端跨 store 或跨 IPC 的 action MUST 尽量通过返回值、领域 helper 或明确的 orchestration action 传递成功/失败和必要数据。UI 层 MUST NOT 依赖 action 后立即 `getState()` 回读全局快照来判断流程结果。

#### Scenario: 重启节点后反馈结果
- **WHEN** UI 触发会改变节点状态的 action
- **THEN** action MUST 返回足够的结果让 UI 展示成功或失败反馈，而不是要求 UI 读取 store 快照推断结果

#### Scenario: 多 store 流程更新
- **WHEN** 一个流程需要同时更新偏好、网络、设备或传输 store
- **THEN** 编排 MUST 位于 action 或领域 helper 中，并以参数和返回值表达依赖关系
