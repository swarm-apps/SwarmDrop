## 1. 测试与模型基础

- [x] 1.1 扩展现有 `e2e/webdriver` 测试支持，为文件选择、Offer、projection、收件箱和大集合提供确定性 fixture 与稳定 accessibility id，不新增第二套 JS 测试 runner。
- [x] 1.2 新增 `FileBrowserItem`、`FileBrowserStatus`、`FileBrowserView`、`FileBrowserScope` 和显式 `FileBrowserActions` 类型，保持文件大小为 bigint、预览入口为显式 `previewUri`。
- [x] 1.3 实现 source/session/inbox 三类稳定 ID 与 relativePath 归一化工具，并通过 fixture 场景覆盖同名不同来源、跨 session 相同 fileId、Windows 分隔符、空路径段和目录边界。
- [x] 1.4 实现 selected、Offer、projection/progress、inbox adapters，并通过 WebDriver 状态断言覆盖 Offer 目录 marker、元数据保留、终态 completed/error/cancelled 和 suspended 文件状态。
- [x] 1.5 将 `mobile-core-store` 的选中文件去重改为 source identity，将文件/目录移除改为稳定 ID 与 segment 边界，并用端到端 fixture 验证 `foo/` 不会误删 `foobar/`。

## 2. FileBrowser 核心组件

- [x] 2.1 建立 `src/components/file-browser/` 公共入口、toolbar、空状态和统一容器，页面只传叶子 items、scope、view 和 actions。
- [x] 2.2 实现不可变目录树构建与 `flattenVisibleNodes`，覆盖目录计数/大小、稳定排序、同路径同名叶子、深层目录以及数据更新后的展开集合收敛。
- [x] 2.3 实现虚拟化 tree view、文件行和目录行，提供稳定 key、expanded accessibility state、路径层级和瞬时按压反馈。
- [x] 2.4 实现虚拟化 grid view 与文件卡片，按容器宽度计算列数，显式消费 previewUri，并在图片失败时回退文件类型图标。
- [x] 2.5 为普通页面接入 FlashList、为 Offer 接入 BottomSheetFlatList，共用 rows/renderItem/actions，并验证不使用 deprecated BottomSheetFlashList。
- [x] 2.6 实现 tree/grid 切换、列数变化重挂载和回到顶部行为，测试两种视图保持相同文件集合、状态、进度和 actions。
- [x] 2.7 在 `preferences-store` 增加 send/transfer/inbox 视图偏好、默认值、setter、partialize 与兼容旧存储的安全 merge，并测试 scope 隔离和无效值回退。
- [x] 2.8 补齐 FileBrowser 的 Lingui 文案、视图选中状态、文件状态/进度读屏文案和最小触摸目标测试。

## 3. 发送入口迁移

- [x] 3.1 将交互式发送选择页迁移到 send-scope FileBrowser，使文件列表占据 header 与 Safe Area footer 之间唯一的可伸缩滚动区域。
- [x] 3.2 保留添加文件/目录/媒体、逐文件移除和目录移除行为，并验证同名不同 sourceId 的文件会全部进入 prepare/send 管线。
- [x] 3.3 将发送页手写底栏迁移到共用 Safe Area action bar，验证扫描/哈希进度替换操作时不会挤压或遮挡文件区。
- [x] 3.4 将 Share Target 的私有 SharedFileRow 改为紧凑文件摘要 + 独立全屏 send-scope FileBrowser 检查页，设备列表保持唯一主滚动区。
- [x] 3.5 验证 Share Target 文件检查页的移除结果会回写 shared-files 状态，返回后设备选择与现有冷/热启动发送流程保持不变。

## 4. 接收 Offer 迁移

- [x] 4.1 抽取无容器耦合的 OfferContent，统一来源、策略、保存位置、FileBrowser 和拒绝/接收操作，避免 phone/tablet 分支复制业务逻辑。
- [x] 4.2 在小于 768dp 的设备上使用近全高 AppBottomSheet + BottomSheetFlatList + 固定 footer，在 tablet/大屏使用宽版 Dialog + 普通虚拟列表。
- [x] 4.3 使用 transfer-scope FileBrowser 展示全部 Offer 叶子文件，保持接收前无 previewUri、无打开/移除能力，并验证 10,000 文件仍可滚到底部。
- [x] 4.4 以 currentOffer.id 为边界重置保存目录覆盖、滚动位置、目录展开和临时 UI，同时保留 transfer-scope 视图偏好。
- [ ] 4.5 覆盖拒绝策略、接受、拒绝、保存位置选择和连续 Offer 队列的 phone/tablet 集成测试。

## 5. 传输详情迁移

- [x] 5.1 将传输详情摘要、总进度和诊断作为同一虚拟滚动面的 header，将 transfer-scope FileBrowser rows 作为主体，固定 TransferActionBar 保持在列表外。
- [x] 5.2 移除非 transferring 状态回退 select mode 的逻辑，统一通过 projection/progress adapter 展示 waiting、transferring、paused、completed、cancelled 和 error 文件状态。
- [x] 5.3 验证 terminal 时清理 progress overlay 后文件明细、完成状态、失败状态和相对路径仍由 projection 正确恢复。
- [ ] 5.4 保持暂停、取消、恢复、重新发送、打开收件箱和打开文件夹动作不变，并增加活动态、暂停态、成功态和失败态组件/导航测试。

## 6. 收件箱迁移

- [x] 6.1 将收件箱多文件详情的私有 FileRow/map 迁移为 inbox-scope FileBrowser，并让预览、摘要、详情与文件 rows 共享一个虚拟滚动面。
- [x] 6.2 通过显式 actions 注入现有系统打开、分享和 missing 标记逻辑，验证 FileBrowser 不自行推导本地 URI。
- [x] 6.3 保留单文件图片全屏查看、视频播放、文本摘录、详情面板、传输跳转和固定底部操作，不把富媒体预览实现成第二个文件集合。
- [ ] 6.4 覆盖可用文件、missing 文件、SAF/content URI、多文件树/grid 切换和 inbox/send/transfer 偏好隔离测试。

## 7. 清理与验证

- [x] 7.1 确认五个入口全部迁移后删除 `src/components/file-tree/`、页面私有 SharedFileRow/FileRow 和重复状态映射。
- [x] 7.2 移除 `@headless-tree/core`、`@headless-tree/react` 依赖并更新 lockfile，确认项目没有残留 import。
- [x] 7.3 为 1、100、1,000、10,000 文件建立可复用 fixture，验证 tree/grid 不全量挂载、可到达末项、切换视图回到顶部且稳定 key 无警告。
- [x] 7.4 运行相关 `pnpm e2e:ios` WebDriver specs、`pnpm check:zustand-access`、`pnpm typecheck`、`pnpm lint:ci`、`pnpm i18n:extract` 和 `openspec validate unify-mobile-file-browser`，修复所有失败并确认生成 catalog 已同步。
- [ ] 7.5 在 iOS 与 Android 真实设备验证小屏、tablet/大屏、横屏、大字体、键盘、Safe Area、系统返回手势、Offer 队列以及 10,000 文件滚动，并把结果记录到 change 验收说明。
