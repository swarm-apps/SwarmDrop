## Context

移动端使用 Expo Router 表达页面结构，使用 Zustand 管理 mobile core 运行态、配对码、偏好、传输、收件箱、通知和 onboarding 等状态。当前主要问题集中在 store 使用方式：部分组件在事件 handler 中直接调用 `useXStore.getState().action()`，`pairing-code-store` 在 store 定义外反向调用自己的 `usePairingCodeStore.getState/setState`，网络设置页在 action 后回读 store 才能判断真实结果。与此同时，`src/core/event-bus.ts` 作为 UniFFI/native 事件桥接层确实需要命令式写入 store，这类入口应该保留并明确化。

Zustand v5 官方文档允许在 React 组件外用 `getState/setState/subscribe` 访问 store，也要求 React 组件使用 selector 取得响应式值；返回对象、数组或派生集合的 selector 需要 `useShallow` 或拆分 selector 来保持稳定。本设计把这个规则落到移动端，并把 native event bus、同步 utility、测试和真正的外部回调列为受控白名单。

## Goals / Non-Goals

**Goals:**

- 统一移动端 React 组件、store creator、native event bridge 和跨 store action 的 Zustand 使用方式。
- 清理 `pairing-code-store` 等自引用 store API，让 store 内部逻辑只依赖闭包 `set/get`。
- 让 action 返回明确结果，减少 UI 层为了判断成功/失败而回读全局 store。
- 规范 persist/partialize 边界，避免运行态或遗留字段被持久化。
- 为剩余 `getState/setState` 建立 allowlist 静态检查。

**Non-Goals:**

- 不改变 Expo Router 页面结构。
- 不重写 native UniFFI core、Rust event 语义或传输协议。
- 不删除 `event-bus` 这类合理命令式桥接层。
- 不引入新的状态管理库或运行时依赖。

## Decisions

### 1. 组件层统一 selector/action selector

移动端页面和组件需要读取 store 状态或 action 时都使用 `useXStore(selector)`。对于多个字段，优先使用 `useShallow`；对于单字段或 action，使用简单 selector。这样组件依赖关系会显式出现在 render/hook 层，也符合 React 响应式订阅模型。

替代方案是在 handler 中直接调用 `useXStore.getState().action()`。这会使组件绕过统一模式，且难以区分普通组件和外部桥接层，因此不采用。

### 2. Store 内部只用闭包 `set/get`

`pairing-code-store` 这类需要 timer、异步生成和自动续生的 store，应该把 `doGenerate/scheduleRefresh/markConsumed` 收束到 `create((set, get) => ...)` 可访问的闭包内，或让外部 helper 通过参数接收 `set/get`。timer 回调也通过闭包 action 续生，而不是引用导出的 hook。

### 3. Native event bus 是明确的命令式边界

`src/core/event-bus.ts` 接收 UniFFI/native 事件并将其投递给 mobile core、transfer、notification、inbox 等 store。这类代码没有 React hook 上下文，且需要写入最新 store，因此允许使用 `getState()`。保留条件是它必须在 allowlist 中标注为 native event bridge，并避免把 UI 导航状态塞入 store。

### 4. Action 返回结果而不是要求 UI 回读 store

当 action 内部吞掉错误并写入 store 时，调用方无法直接知道真实结果，容易出现 UI 在 action 后 `getState()` 回读的模式。需要反馈的 action 应返回 `ok/error` 或明确状态结果；全局 error 仍可用于页面展示，但不作为唯一控制流。

### 5. Persist 只保存 durable 状态

偏好 store、onboarding store、mobile core cache 等持久化 store 必须使用 `partialize` 或等价机制，只保存用户偏好、已完成引导、配对设备缓存等 durable 字段。运行态、错误、临时队列、历史兼容字段不应持久化。

### 6. 静态 allowlist 检查防止回归

实现阶段应新增轻量检查，验证 `useXStore.getState/setState` 只出现在 native event bridge、同步 utility、测试或其他白名单文件。组件和 store creator 文件新增非白名单调用时，检查必须失败。

## Risks / Trade-offs

- [Risk] action 返回值调整会触及多个调用方。→ Mitigation: 先改存在回读 store 的流程，再保持旧状态字段供页面展示。
- [Risk] event bus allowlist 太宽会掩盖其他非规范调用。→ Mitigation: allowlist 按具体文件和用途维护，不按整个 `src/core` 泛化。
- [Risk] persist partialize 可能丢弃历史字段。→ Mitigation: 仅移除非 durable 字段；需要迁移时提供默认值和版本兼容。
- [Risk] store 内部 timer 重构可能改变配对码续生行为。→ Mitigation: 当前仓库没有测试 runner，先用闭包化实现、typecheck、lint 和静态 allowlist 覆盖结构性回归；后续引入测试框架时再补 ensure、regenerate、clear、markConsumed 和自动续生路径的运行时单元测试。

## Migration Plan

1. 盘点移动端 `useXStore.getState/setState` 调用并分类为组件、store 内部、native event bridge、utility、测试。
2. 先重构组件层调用为 selector/action selector。
3. 重写 `pairing-code-store` 自引用逻辑，补齐 store 行为测试。
4. 调整需要 UI 反馈的 action 返回值，例如节点重启流程。
5. 收紧 persisted state，补充 `partialize` 或删除遗留字段。
6. 建立 allowlist 检查，确认剩余命中均为合理命令式边界。
7. 跑 lint、typecheck、相关测试和 OpenSpec 校验。

## Open Questions

- 移动端是否与桌面端共用同一份 allowlist 检查脚本结构，还是各仓独立维护？实现阶段可以按仓库脚本体系决定。
