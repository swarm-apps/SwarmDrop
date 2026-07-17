## Why

移动端已经有比较清晰的 Expo Router 页面结构，但 Zustand 使用方式还不够统一：部分组件绕过 selector 调 action，部分 store 在定义外反向调用自己的 `useXStore.getState/setState`，还有 action 失败结果需要 UI 再回读 store 才能判断。随着移动端继续同步桌面端能力，这些隐式边界会让状态更新、错误反馈和跨端 core 接入变得难以维护。

## What Changes

- 建立移动端 Zustand 使用契约：React 组件通过 selector 读取状态/action，多字段 selector 使用 `useShallow` 或拆分；store creator 内部只使用 `set/get`；`getState/setState` 只允许出现在 native event bus、同步 utility、测试等命令式边界。
- 重构现有非规范调用点，重点包括 `pairing-code-store` 自引用 store API、设置网络页重启后回读 store、组件 handler 直接调用 `usePairingCodeStore.getState().markConsumed()` 等。
- 调整 store action API，让需要反馈给 UI 的命令式操作返回明确结果，减少 UI 层为了判断成功/失败而读取全局 store 快照。
- 规范持久化边界，只持久化真正 durable 的偏好/缓存字段，避免运行态或历史遗留字段混入 persisted state。
- 为保留的命令式 store 访问建立白名单检查，确保 native/Rust 事件桥接仍清晰可控。

## Capabilities

### New Capabilities

- `mobile-zustand-usage-contract`: 定义移动端 Zustand store 的组件订阅、store 内部实现、native event bridge、跨 store 编排、持久化边界和 `getState/setState` 白名单规则。

### Modified Capabilities

- None

## Impact

- 影响移动端 Zustand stores：`src/stores/**`，尤其是配对码、mobile core、preferences、transfer/inbox 相关 store。
- 影响 React Native/Expo 页面与组件：`src/app/**`、`src/components/**` 中直接访问 store API 的位置。
- 影响 native/core 事件桥接：`src/core/event-bus.ts` 保留为允许的命令式边界，但需要明确白名单。
- 可能新增轻量校验脚本或测试，用于限制 `getState/setState` 的剩余调用点。
- 不新增运行时依赖；继续使用现有 Zustand v5、Expo Router、React Native 和 UniFFI core 接入方式。
