## Why

桌面端目前同时存在 route、组件局部状态和 Zustand store 三种方式表达子页面/详情态，部分 React 组件还通过 `useXStore.getState()` 直接读取或调用 action，导致页面可寻址性、响应式订阅边界和跨端复用规则不够清晰。现在收件箱、传输详情和设备策略都在增长，如果不先收束状态归属，后续桌面端与移动端共享能力会继续积累隐性耦合。

## What Changes

- 将桌面端详情页、子页面、筛选、选中项等可导航 UI 状态标准化为 TanStack Router route/search params，而不是放入全局 store 扮演路由。
- 建立 Zustand 使用契约：React 组件通过 selector 订阅状态和 action；store creator 内部使用 `set/get`；`getState/setState` 仅保留给外部事件桥接、router guard、测试和少量命令式 utility。
- 清理现有非规范调用点，例如组件事件 handler 中的 `useSecretStore.getState().xxx()`、页面 effect 中直接 `useNetworkStore.getState().startNetwork()`、store 模块反向读取其他 store 时缺少边界说明等。
- 为保留的命令式 store 访问建立白名单检查，让后续实现可以用静态搜索验证不会再次扩散。
- 不改变 Rust core、Tauri IPC 命令语义或已定义的传输/收件箱业务能力；本变更只规范前端状态架构和页面寻址方式。

## Capabilities

### New Capabilities

- `desktop-route-owned-ui-state`: 定义桌面端哪些 UI 状态必须由 route/search params 拥有，以及收件箱、传输详情、设备/设置等页面如何通过标准路由表达子页面。
- `desktop-zustand-usage-contract`: 定义桌面端 Zustand store 的组件订阅、store 内部实现、跨 store 编排和 `getState/setState` 白名单规则。

### Modified Capabilities

- None

## Impact

- 影响桌面端 React/TanStack Router 页面：`src/routes/_app/**`、`src/components/**` 中与详情页、收件箱、发送、设备策略、设置和传输活动相关的状态入口。
- 影响桌面端 Zustand stores 与 hooks：`src/stores/**`、`src/hooks/**`、`src/lib/**` 中直接使用 `useXStore.getState/setState` 的位置。
- 可能新增轻量校验脚本或测试，用于限制 `getState/setState` 的剩余调用点。
- 不新增运行时依赖；继续使用现有 Zustand v5、TanStack Router、Tauri 事件和测试工具链。
