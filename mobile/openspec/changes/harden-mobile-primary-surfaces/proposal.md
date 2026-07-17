## Why

`/impeccable critique` 对三个核心界面(主页/收件箱/设置)做了独立设计评审,发现 14 项优先问题(3 P0 / 5 P1 / 4 P2 / 2 P3)。这些问题不是零散的视觉瑕疵,而是集中撞上 PRODUCT.md 的两条核心设计原则:「状态先于装饰」(节点 error 态、收件箱加载失败、设置检查更新的"进行中"态,分别被伪装成 stopped/已删除/静止图标)和「掌控感来自可见性而非管理复杂度」(主页运行态与设置 Network 页都出现了 PRODUCT.md 明确排斥的"仪表盘堆砌")。此外,DESIGN.md 已承认的 light-mode 主色按钮对比度缺陷(~3.5:1,低于 WCAG AA 4.5:1)在三个界面的最高频 CTA 上反复出现,是唯一横跨全部界面的无障碍阻断项。需要一次系统性修复,而不是逐个界面零散打补丁。

## What Changes

- **主页(Devices Tab)**:节点 `error` 状态新增独立视觉态(不再与 `stopped` 共用文案);"添加设备"面板收敛为分步展开,降低运行态首屏并列面板数;配对流程加客户端超时与"取消"退出入口;已配对设备的发送按钮 `accessibilityLabel` 携带设备名。
- **收件箱**:区分"加载失败"与"记录已被删除"两种状态,渲染 store 已维护但从未使用的 `lastError`,并补上重试入口;`FilterRail` 收敛为≤4 个主筛选 + "更多筛选"入口;进入关键词搜索后保留已选筛选并给搜索结果补齐状态徽标;非图片内容的 `ContentPreview` 改为内联文字摘录,替代纯装饰性大留白;详情标题与工具栏计数字号回归 DESIGN.md 已登记的 Title/Headline 档位。
- **设置**:Network 页的协议级指标(NAT/候选节点/LAN Helper/中继等)折叠进"查看诊断详情"二级入口,首屏只给一句合成状态;"检查更新"的静态 Loader 换成真正旋转的 `ActivityIndicator`;"删除自定义引导节点"与"恢复默认接收位置"的确认强度对调,让红色警告匹配真实风险;`NetworkHint` 的硬编码警示色改用 `colors.warning` token;Settings 家族内的圆角统一到 DESIGN.md 已登记的 token(不再混用未登记的 `rounded-xl`)。
- **跨界面(设计 token)**:修复 light-mode `--primary-foreground` 对比度,使其满足 WCAG AA 4.5:1——这张 token 同时驱动三个界面的主 CTA(启动节点/发送/复制配对码、收件箱主按钮、设置"添加"按钮)。
- 无 **BREAKING** 变更:均为界面内行为、文案与视觉 token 调整,不改变对外 API、数据结构或已持久化的用户设置。

## Capabilities

### New Capabilities
- `devices-surface-hardening`: 主页设备列表在节点状态可见性、信息密度、配对流程可控性、无障碍标签上的行为要求
- `inbox-integrity-hardening`: 收件箱在"错误"与"真实缺失/已删除"的区分、筛选一致性、内容预览诚实度上的行为要求
- `settings-hardening`: 设置页在网络诊断信息分层、"进行中"状态真实性、破坏性操作风险信号对齐上的行为要求
- `accessible-primary-contrast`: 主色按钮文字在亮色模式下必须满足 WCAG AA 对比度的跨界面视觉 token 要求

### Modified Capabilities
(无——`openspec/specs/` 目前没有已归档的基线 spec,三个既有变更 `redesign-mobile-foundation`/`add-mobile-drop-inbox`/`sync-mobile-network-discovery` 尚未归档,故本次全部按新能力登记。)

## Impact

- **代码**:
  - `src/app/(main)/index.tsx`、`src/components/{device-card,status-pill,trust-badge,connection-badge}.tsx`
  - `src/app/(main)/inbox.tsx`、`src/app/inbox/[itemId].tsx`、`src/app/inbox/search.tsx`、`src/stores/inbox-store.ts`
  - `src/app/(main)/settings.tsx`、`src/app/settings/{general,language,theme,network,about,bootstrap-nodes}.tsx`、`src/components/{setting-row,device-info-card}.tsx`
- **设计 token**:`src/global.css` 的 light-mode `--primary-foreground`;`DESIGN.md` 与 `.impeccable/design.json` 需同步更新对比度说明与(如新增)圆角 token。
- **验证**:`pnpm typecheck`、`biome lint`;这几个屏没有自动化 UI 测试覆盖,需人工或 Maestro 走查确认状态区分、筛选行为、确认弹层强度符合预期;修复后建议重跑 `/impeccable critique` 复核三个 slug(`src-app-main-index-tsx`/`src-app-main-inbox-tsx`/`src-app-main-settings-tsx`)分数提升。
