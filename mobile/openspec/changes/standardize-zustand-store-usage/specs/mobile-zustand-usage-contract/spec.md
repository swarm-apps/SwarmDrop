## ADDED Requirements

### Requirement: React Native 组件必须通过 selector 使用 store
移动端 React Native 页面、组件和 hooks MUST 通过 `useXStore(selector)` 读取 Zustand 状态和 action。组件层 MUST NOT 直接调用 `useXStore.getState()` 或 `useXStore.setState()`，除非该文件被明确列入命令式边界白名单。

#### Scenario: 组件 handler 调用 action
- **WHEN** 组件在按钮、sheet、dialog 或 effect 中需要调用 store action
- **THEN** 组件 MUST 通过 selector 取得该 action，并调用 selector 返回的方法

#### Scenario: 多字段 selector
- **WHEN** 组件从同一个 store 选择多个字段、action 或派生集合
- **THEN** 组件 MUST 使用多个简单 selector，或使用 `useShallow` 包裹返回对象、数组或派生集合的 selector

### Requirement: Store creator 内部必须使用闭包 set/get
移动端 Zustand store 的 action、timer、异步 helper 和内部状态流转 MUST 使用 `create((set, get) => ...)` 提供的 `set/get` 访问自身状态。store module 内部 MUST NOT 通过导出的 `useXStore.getState/setState` 反向访问自身。

#### Scenario: 配对码自动续生
- **WHEN** 配对码 store 在过期前自动续生或在 accept 后标记已消耗
- **THEN** 该 store MUST 通过闭包 action、`set` 和 `get` 完成状态读取与更新，而不是调用导出的 store hook

#### Scenario: Store helper 定义在 create 外部
- **WHEN** store helper 因可读性需要定义在 create 调用之外
- **THEN** helper MUST 通过参数接收所需的 `set/get/action`，而不是引用 `useXStore.getState/setState`

### Requirement: 命令式 store API 必须受白名单约束
移动端 `useXStore.getState/setState` MUST 只出现在 native event bridge、同步 utility、测试 setup 或测试断言等非 React 响应式边界，并且 MUST 被静态 allowlist 检查覆盖。

#### Scenario: Native event bus 投递 core 事件
- **WHEN** UniFFI/native event bus 收到网络、配对、传输、通知或收件箱事件
- **THEN** event bus MAY 使用 `getState()` 调用 store action 写入最新状态，但调用点 MUST 在 allowlist 中标注为 native event bridge

#### Scenario: 普通组件新增 getState
- **WHEN** 开发者在 `src/app/**` 或 `src/components/**` 中新增非白名单 `useXStore.getState()`
- **THEN** 静态检查 MUST 失败，直到该调用改为 selector/action selector 或被证明属于合理命令式边界

### Requirement: Store action 必须返回 UI 所需的控制流结果
移动端会被 UI 直接触发、且 UI 需要展示成功/失败反馈的 store action MUST 返回明确结果或抛出错误。UI 层 MUST NOT 依赖 action 完成后立即 `getState()` 回读全局 store 快照来判断真实结果。

#### Scenario: 节点重启反馈
- **WHEN** 设置页触发节点重启以应用网络配置
- **THEN** 重启 action MUST 返回成功或失败结果，使 UI 能直接展示反馈

#### Scenario: Action 同步全局 error
- **WHEN** action 需要同时写入全局 error 供其他页面展示
- **THEN** action MAY 更新 store error 字段，但仍 MUST 向调用方返回或抛出足够的控制流结果

### Requirement: Persisted store 必须只保存 durable 字段
移动端持久化 Zustand store MUST 使用 `partialize` 或等价机制，只保存用户偏好、已完成引导、配对设备缓存等 durable 字段。运行态、错误、临时队列和历史遗留字段 MUST NOT 被持久化。

#### Scenario: 偏好 store 持久化
- **WHEN** 偏好 store 写入持久化存储
- **THEN** 持久化 payload MUST 只包含用户可配置且需要跨启动保留的偏好字段

#### Scenario: Mobile core 运行态
- **WHEN** mobile core store 记录节点运行态、错误、设备列表或临时文件授权
- **THEN** 这些运行态字段 MUST NOT 被持久化为下次启动的初始事实
