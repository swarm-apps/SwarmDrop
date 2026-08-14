## Context

桌面端已经使用 TanStack Router 表达主要页面，也使用 Zustand 管理网络节点、配对、设备、偏好、分享来源、传输投影和收件箱等运行时状态。当前问题不是 Zustand 选型错误，而是边界不一致：部分页面把选中详情、子页面或一次性动作藏在 store 或组件局部状态里，部分 React 组件绕过 hook selector 直接调用 `useXStore.getState()`，而 Tauri 事件回调、router guard、测试等真正需要命令式读取的位置又和这些普通组件调用混在一起。

Zustand v5 官方文档允许在 React 组件外通过 `getState/setState/subscribe` 访问 store，同时要求 React 组件通过 selector 订阅响应式值；当 selector 返回对象、数组或派生集合时，需要 `useShallow` 或拆分 selector 保持输出稳定。本设计把这个社区约定落到桌面端仓库：保留外部世界入口的 escape hatch，禁止组件和 store 实现层滥用它。

## Goals / Non-Goals

**Goals:**

- 让桌面端可导航 UI 状态具备统一归属：URL 能表达的状态必须由 route/search params 拥有。
- 让 React 组件内的 Zustand 使用回到 hook selector 模式，包括 action selector。
- 让 store creator 内部只依赖 `set/get` 闭包，避免自引用 `useXStore.getState/setState`。
- 明确 `getState/setState` 白名单，并用静态检查防止新增非规范调用。
- 在不改变现有 Rust core/IPC 行为的前提下完成前端架构收束。

**Non-Goals:**

- 不重写 Zustand 为其他状态库。
- 不改变传输协议、收件箱数据模型、设备信任策略或 Tauri 命令语义。
- 不为了追求零 `getState()` 删除 Tauri event、router guard、测试 setup 等合理命令式边界。
- 不重新设计页面视觉样式；只处理状态归属和导航结构。

## Decisions

### 1. Route 拥有可导航 UI 状态

收件箱详情、传输详情、设备详情、设置子页、筛选、搜索、当前选中项等可以被分享、刷新、返回/前进或从外部入口打开的状态，必须通过 route path 或 search params 表达。组件局部状态只保留瞬时控件态，例如弹窗开关、输入过程、hover/focus、正在提交等；Zustand 只保留跨页面共享的领域状态，例如设备列表、配对请求、传输投影、偏好、收件箱缓存。

替代方案是继续使用 store 保存选中项。这会让页面刷新、深链、历史栈和桌面端/移动端信息架构分叉，因此不采用。

### 2. React 组件通过 selector 读取状态和 action

组件需要状态时使用 `useXStore((state) => state.value)`；需要 action 时也使用 selector，例如 `const removePairedDevice = useSecretStore((state) => state.removePairedDevice)`。多字段选择器必须使用 `useShallow`，或者拆成多个简单 selector；selector 不得直接返回每次新建的对象、数组或映射结果，除非包裹浅比较。

替代方案是在事件 handler 中调用 `useXStore.getState().action()`。这通常可以工作，但会绕过组件依赖关系和一致的订阅模式，也让真正需要命令式读取的位置失去辨识度，因此组件层不采用。

### 3. Store creator 内部只使用闭包 `set/get`

store 文件内部的 action、helper 和 timer 逻辑应通过 `create((set, get) => ...)` 传入的闭包读写状态。若 helper 定义在 create 外部，则通过参数显式传入需要的 `set/get/action`，而不是反向 import 或引用导出的 hook。

替代方案是在 store module 中调用导出的 `useXStore.getState/setState`。这会制造初始化顺序和自引用耦合，也不利于测试，因此不采用。

### 4. `getState/setState` 作为受控 escape hatch

允许的命令式 store 访问仅包括：

- Tauri window/tray/file-open/network/transfer event 回调，需要读取最新快照而不是订阅渲染。
- TanStack Router `beforeLoad` 等非 React hook 环境。
- 测试 setup、测试断言、store reset helper。
- 少量同步 utility，在没有 React 上下文且调用点需要当前快照时使用。

保留点必须能被静态检查 allowlist 覆盖；新增调用必须先进入 allowlist 并说明边界类型。

### 5. 跨 store 编排通过 action API 或领域 helper 显式表达

当一个流程需要读写多个 store，例如设备名变更后重启节点、设备策略更新后刷新列表、发送完成后刷新传输投影，应优先把结果作为 action 返回值或通过领域 helper 编排。UI 不应为了判断成功/失败而在 action 后立即回读全局 store 快照。

### 6. 用静态检查守住规范

实现阶段应新增轻量检查，例如基于 `rg` 的 allowlist 脚本或测试，验证 `useXStore.getState/setState` 只出现在白名单文件/代码段。该检查应进入常用验证链，避免后续功能回归到旧模式。

## Risks / Trade-offs

- [Risk] 过度移动状态到 route 会让 URL 变复杂。→ Mitigation: 只移动可导航、可恢复、可深链状态；瞬时控件态仍留在组件。
- [Risk] 组件 selector 拆分后代码行数略增。→ Mitigation: 优先换取可读依赖关系和更稳定的渲染；多字段选择器用 `useShallow` 保持简洁。
- [Risk] 白名单检查可能误报合理的命令式入口。→ Mitigation: allowlist 按文件和用途维护，新增入口必须说明外部边界类型。
- [Risk] 跨 store action 返回值调整会触及多个页面。→ Mitigation: 先重构高风险调用点，再逐步收敛辅助流程，保持业务行为不变。

## Migration Plan

1. 盘点现有 `useXStore.getState/setState` 调用并按组件、store 内部、外部事件、router guard、测试分类。
2. 先改组件层调用：用 selector 取 action/state，并补齐 `useShallow`。
3. 再改 route-owned 状态：把收件箱、传输详情、发送/设备/设置中的子页面和选中项迁回 route/search params。
4. 重构 store 内部自引用和跨 store 编排，让 action 返回足够的结果。
5. 建立 allowlist 检查，确保剩余命中均为外部边界。
6. 跑类型检查、测试、构建和 OpenSpec 校验。

## Open Questions

- 是否把 `getState/setState` allowlist 做成独立脚本，还是先用测试内嵌正则检查即可？实现阶段可根据仓库现有脚本风格选择。
