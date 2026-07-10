## 1. 统一模型与模块骨架

- [x] 1.1 创建 `src/components/file-browser/` 模块及单一公共出口，定义 `FileBrowserView`、`FileBrowserStatus`、`FileBrowserItem`、目录目标和操作能力类型
- [x] 1.2 实现从扁平 `FileBrowserItem[]` 构建目录节点、子节点映射、目录文件数与累计大小的纯函数，并保持目录优先、同类按名称排序
- [x] 1.3 为稳定 ID、Windows / Unix 相对路径规范化、同名不同路径和目录聚合补充单元测试
- [x] 1.4 实现发送扫描文件、接收 Offer、传输投影和收件箱文件到统一模型的适配器
- [x] 1.5 为各适配器补充状态、进度、missing、localPath、previewUrl 与稳定 ID 映射测试

## 2. 精修树形视图

- [x] 2.1 将现有 `FileTree` 的 headless-tree 与虚拟滚动逻辑迁移为内部 `FileTreeView`
- [x] 2.2 重构 `FolderRow`，移除展开状态的常驻背景，仅保留 chevron、打开文件夹图标、缩进和低对比度引导线
- [x] 2.3 统一文件行与目录行的高度、圆角、图标、hover、focus-visible 和 focus-within 操作反馈
- [x] 2.4 将删除、重试、打开和定位操作改为显式能力驱动，并保证次要操作键盘可达且不会误触发行主操作
- [x] 2.5 保留目录前缀删除能力，并验证嵌套目录展开、折叠、Enter / Space 操作和大量节点虚拟滚动
- [x] 2.6 为树形默认、展开、hover、焦点、传输中、完成、失败和 missing 状态补充组件测试

## 3. 新增统一网格视图

- [x] 3.1 抽取 `FileCard`，实现 4:3 预览区、文件类型图标、文件名、大小和必要的相对目录信息
- [x] 3.2 实现 previewUrl 图片懒加载、加载失败回退图标和 missing 文件降级，不在组件内从任意本地路径生成 URL
- [x] 3.3 在卡片中实现 waiting、transferring、completed、error 与 missing 状态，并与树形视图使用同一状态模型
- [x] 3.4 在卡片中接入删除、打开、定位和重试能力，保证主操作与次要操作事件隔离及键盘可达
- [x] 3.5 实现响应式列数计算和按行虚拟化的 `FileGridView`，在容器宽度变化时重新测量
- [x] 3.6 验证大型网格仅挂载可视行、独立滚动、切换后从顶部开始，并为列数变化和大集合补充测试

## 4. FileBrowser 容器与视图偏好

- [x] 4.1 实现 `FileBrowserHeader`，统一标题、文件数量、总大小和可选视图切换控件
- [x] 4.2 实现带可访问名称和 pressed 状态的树形 / 网格切换，只有多个可用视图时才显示
- [x] 4.3 实现 `FileBrowser` 容器，协调可用视图、空状态、滚动区、树形与网格渲染，并在非法视图偏好时安全回退
- [x] 4.4 在 `preferences-store` 增加 `send`、`inbox`、`transfer` 三个独立视图偏好和 setter，并纳入持久化 partialize
- [x] 4.5 为默认视图、按场景持久化、不可用视图回退和空文件集合补充测试

## 5. 发送流程迁移

- [x] 5.1 将 `/send` 文件选择区迁移到 `FileBrowser`，默认 tree，可切换 grid，保持文件和目录移除、统计、内部滚动与底部命令栏行为
- [x] 5.2 将 `/send/share-target` 宽屏待发文件栏和窄屏抽屉迁移到 `FileBrowser`，共用 send 视图偏好
- [x] 5.3 验证发送网格对任意本地路径默认使用文件类型图标，不扩大 asset protocol scope
- [ ] 5.4 更新发送相关 Vitest 和桌面 E2E 选择器，覆盖树形 / 网格切换、移除文件和大量文件滚动
- [x] 5.5 调整发送进度页高度链，固定顶部任务栏和底部操作栏，让成功摘要与稳定高度的文件明细在中间区域滚动，并补充布局回归测试
- [x] 5.6 审计任务页高度链，为 `TaskContent` 增加固定 footer，并迁移发送选择、配对输入和配对码页面；确认快捷发送无需重复修改

## 6. 收件箱与传输流程迁移

- [x] 6.1 将收件箱详情文件网格迁移到 `FileBrowser`，默认 grid，可切换 tree，并保留打开、定位、missing 与安全缩略图
- [x] 6.2 删除收件箱页面私有 `FileCard`、图片判断和可迁移的缩略图辅助逻辑，改由统一模块与 inbox adapter 提供
- [x] 6.3 将 `SessionFileSection` 迁移到 `FileBrowser`，覆盖传输活动详情和发送进度，默认 tree，可切换 grid
- [x] 6.4 将接收 Offer 弹窗迁移到统一 `FileBrowser`，支持 tree / grid 切换并保持限高内部滚动
- [x] 6.5 为收件箱打开 / 定位、传输状态 / 重试、Offer 只读确认和场景默认视图补充回归测试
- [x] 6.6 重构接收 Offer 弹窗为紧凑横向摘要头、宽版文件浏览区和固定保存 / 确认区域，并复用 transfer 视图偏好
- [x] 6.7 调整传输详情右栏，让摘要与稳定高度文件区滚动、会话操作固定在宽屏详情底部

## 7. 清理与一致性

- [x] 7.1 删除所有页面对旧 `FileTree` 的直接导入和临时 re-export，确认文件展示只通过统一 `FileBrowser` 公共 API
- [x] 7.2 清理过时的 `src/components/file-tree/` 文件或迁移后残留，并更新相关注释和项目知识库
- [x] 7.3 对账旧 `openspec/changes/file-tree-component` 已实现内容，记录由本变更取代的要求，避免重复 capability 归档
- [x] 7.4 运行 `pnpm i18n:extract`，补齐树形、网格、操作与空状态新增文案的 zh、zh-TW、en catalog
- [ ] 7.5 检查亮色 / 暗色下的图标、状态色、卡片对比度、焦点和滚动条，并验证窄栏、抽屉、Dialog 与 920px 断点

## 8. 验证

- [x] 8.1 运行 `pnpm exec tsc --noEmit` 和 `pnpm test`
- [x] 8.2 运行 `pnpm build` 并确认无新增 chunk 或依赖异常
- [ ] 8.3 运行发送流程、快捷发送、收件箱、传输详情和接收 Offer 的桌面端定向 E2E / 手动 smoke test
- [ ] 8.4 使用大量嵌套文件验证树形和网格虚拟化、视图切换、内滚动、删除与底部操作栏稳定性
- [x] 8.5 运行 `git diff --check` 和 `openspec validate unify-file-browser-views --strict`
