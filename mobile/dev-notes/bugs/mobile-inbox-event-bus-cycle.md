# Mobile Inbox 事件总线循环依赖

日期：2026-06-28

## 现象

Android dev build 在接入 Inbox 刷新 hook 后出现 Metro require-cycle warning：

```text
src/core/mobile-core.ts -> src/core/event-bus.ts -> src/stores/inbox-store.ts -> src/core/mobile-core.ts
```

## 原因

`event-bus.ts` 顶层静态导入了 `useInboxStore`，而 `inbox-store.ts` 又会导入 `getMobileCore()` 调用 native bridge，导致事件总线进入 mobile core 初始化环。

## 修复

将收件完成后的 Inbox 刷新改成事件处理时的延迟导入，避免在模块初始化阶段引入 `inbox-store`：

```ts
const { useInboxStore } = await import("@/stores/inbox-store");
await useInboxStore.getState().refresh();
```

后续如果要在 `event-bus.ts` 接入新的 store，要先检查该 store 是否反向依赖 `core/mobile-core`；若依赖，应使用延迟导入或拆出无 core 依赖的轻量 dispatcher。
