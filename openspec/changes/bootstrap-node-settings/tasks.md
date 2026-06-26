## 1. 后端：NetworkStatus 新增 bootstrapConnected 字段

- [x] 1.1 `network/mod.rs` — `NetworkStatus` 结构体新增 `bootstrap_connected: bool` 字段
- [x] 1.2 `network/manager.rs` — `NetManager` / `SharedNetRefs` 新增 `bootstrap_connected: Arc<RwLock<bool>>` 和对应引导节点 PeerId 集合
- [x] 1.3 `network/event_loop.rs` — 在 ConnectionEstablished / ConnectionClosed 事件中检查是否为引导节点 PeerId，更新 `bootstrap_connected`
- [x] 1.4 `network/manager.rs` — `build_network_status()` 填充 `bootstrap_connected` 字段

## 2. 后端：start 命令接受自定义引导节点参数

- [x] 2.1 `network/config.rs` — `create_node_config` 新增 `custom_bootstrap_nodes: Vec<String>` 参数，解析并与默认节点合并
- [x] 2.2 `commands/mod.rs` — `start` 命令新增 `custom_bootstrap_nodes: Option<Vec<String>>` 参数，传递给 `create_node_config`
- [x] 2.3 同步将引导节点 PeerId 集合传入 `NetManager`，供事件循环判断连接状态

## 3. 前端：preferences-store 新增自定义引导节点字段

- [x] 3.1 `stores/preferences-store.ts` — 新增 `customBootstrapNodes: string[]` 字段、`addBootstrapNode` / `removeBootstrapNode` actions，包含在 `partialize` 中持久化

## 4. 前端：network 命令层适配

- [x] 4.1 `commands/network.ts` — `start()` 函数新增 `customBootstrapNodes` 可选参数
- [x] 4.2 `commands/network.ts` — `NetworkStatus` 接口新增 `bootstrapConnected: boolean` 字段
- [x] 4.3 `stores/network-store.ts` — `startNetwork` 读取 `preferences-store` 中的 `customBootstrapNodes` 传给 `start()`

## 5. 前端：设置页引导节点管理 UI

- [x] 5.1 创建 `settings/-bootstrap-nodes-section.tsx` 组件：展示默认节点（只读）和自定义节点（可删除），提供添加输入框和 Multiaddr 格式校验
- [x] 5.2 添加「重启节点」逻辑：节点运行中修改列表后显示提示和重启按钮，点击执行 stopNetwork + startNetwork
- [x] 5.3 `settings/index.lazy.tsx` — 在网络设置区域下方插入 `BootstrapNodesSection`

## 6. 前端：桌面端设备页网络状态栏

- [x] 6.1 增强 `NetworkStatusBar` 组件：节点运行时展示引导节点连接、中继就绪、NAT 穿透三项状态指示器
- [x] 6.2 `devices/index.lazy.tsx` — 桌面端 `DesktopDevicesView` 在 header 下方（isOnline 时）插入 `NetworkStatusBar`

## 7. 国际化

- [x] 7.1 运行 `pnpm i18n:extract` 提取新增的翻译字符串

## 8. 验证

- [x] 8.1 `cargo build` 后端编译通过
- [x] 8.2 `pnpm build` 前端编译通过
