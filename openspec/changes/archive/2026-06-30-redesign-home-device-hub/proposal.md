## Why

当前桌面端首页的视觉重心更像“最近传输列表”，但用户进入 SwarmDrop 时最常见的目标是发现目标设备、配对新设备、向已配对设备发送文件，并查看是否有正在传输的任务。将历史记录降级为独立入口，并把设备发现与配对提升为首页主内容，可以让首页更符合“文件传输操作台”的使用心智。

## What Changes

- 将 `/devices` 重设计为桌面端首页设备中心。
- 将附近未配对设备从“添加设备”弹出层提升为首页可见区域。
- 保留已配对设备作为主要发送目标列表，并优先展示在线设备。
- 在顶栏设置按钮左侧新增传输历史图标入口。
- 从首页移除最近传输历史列表。
- 在首页新增仅展示活跃传输会话的“正在传输”区域。
- 保留 `/transfer` 作为传输历史、筛选、清空和详情导航页面。
- 保留当前节点未启动时的离线流程，但调整空状态文案，使其围绕设备发现和接收文件。

## Capabilities

### New Capabilities
- `home-device-hub`: 桌面端首页设备中心行为，覆盖附近设备、已配对设备、快速配对、活跃传输和传输历史入口。

### Modified Capabilities

## Impact

- 影响 `src/routes/_app/devices/` 下的首页路由与组件。
- 影响 `src/components/layout/app-topbar.tsx` 的顶栏导航入口。
- 复用 `src/stores/transfer-store.ts` 的活跃传输会话渲染首页传输摘要。
- 复用 `src/stores/network-store.ts`、`src/stores/secret-store.ts` 和 `src/stores/pairing-store.ts` 的设备发现与配对数据。
- 需要在实现后运行国际化提取，更新 `src/locales/**/messages.po`。
- 预计不需要修改 Rust 后端 API。
