## 1. 阶段一 — 共享包接入验证（最小切片）

> 目标：只搬**一个**函数，把三端的构建/解析路径全部跑通。R1（turbopack root）与 R2（Metro symlink）
> 在这一步暴露。**本组未全绿之前不得搬第二个函数。**

- [x] 1.1 创建 `packages/shared-view/`：`package.json`（name `@swarmdrop/shared-view`，`type: module`，
      `exports` 指向 `src/index.ts`）、`tsconfig.json`、`src/index.ts` 空导出
- [x] 1.2 根 `pnpm-workspace.yaml` 的 `packages` 加 `packages/*`；根 `pnpm install` 通过
- [x] 1.3 把 `deviceDisplayName` 及其入参的结构化类型（D3）搬进共享包，附带从
      `src/lib/device-name.test.ts` 迁移的用例；包内单测跑绿
- [x] 1.4 **桌面接入**：`package.json` 加 `workspace:*` 依赖，`src/lib/device-name.ts` 改为 re-export；
      `pnpm test` + `pnpm build` 通过
- [x] 1.5 **Web 接入**：`docs/package.json` 加 `link:../../packages/shared-view`，
      `next.config.mjs` 的 `transpilePackages` 加一项；`docs` 下 `pnpm dev` 与 `pnpm build`（静态导出）
      **两者都**通过 ← R1 验证点
- [x] 1.6 **移动接入**：`mobile/package.json` 加 `link:`，`metro.config.js` 加 `watchFolders` 与
      `resolver.nodeModulesPaths`；`mobile` 下 `pnpm typecheck` 通过，且 `pnpm ios` 或 `pnpm android`
      能真机/模拟器启动到首屏 ← R2 验证点
- [x] 1.7 ~~回退到预构建 `dist/`~~ **不需要**。R1 的真因是 turbopack 的 `root` 是文件系统边界，
      与产物是 `.ts` 还是 `.js` 无关——实测把包换成预构建 `.js` 后照样 `Module not found`，
      所以预构建救不了它，只能省掉 `transpilePackages` 一行。修法是把 `turbopack.root`
      放到仓库根。R2 一行 `watchFolders` 即解决

## 2. 阶段二 — 共享包全量收口

> 目标：三端重复的纯视图逻辑收敛到一处，测试合并去重。**桌面与移动到此为止，后续阶段不再动。**

- [x] 2.1 搬入别名/分组解析与排序：合并 `src/lib/device-organization.ts`(77 行) 与
      `mobile/src/lib/device-organization.ts`(150 行)。**显式收敛两者的行为差异并在包内注释记录取舍**
      ← R7 的核心风险点，不得靠「哪个先写用哪个」蒙过
- [x] 2.2 搬入同名消歧的次级身份提示逻辑（`deviceIdentityHint` / `hasDuplicateOrganizedName` 等价物）
- [x] 2.3 搬入信任级别归一与「是否可发送」判定：以 `mobile/src/core/device-trust.ts` 为基准，
      桌面侧从 `trust-policy-dialog.tsx` 的 `trustConfig` 中把**纯数据部分**（级别枚举、可发送判定）
      剥离进来；图标与 className 留在各端
- [x] 2.4 搬入格式化：字节大小、传输速率、延迟、时长。**`formatUptime` 与 `formatRelativeTime`
      判出去了**——输出是各端有意不同的本地化文案，收进来必须改掉某一端的渲染（README 归属判据
      第 3 条）。`formatCountdown` 零调用点，直接删除而非搬迁
- [x] 2.5 合并三端现有测试进共享包并去重；共享包单测全绿
- [x] 2.6 桌面：`src/lib/{device-name,device-organization,format,format-uptime}.ts` 全部改 re-export
      或由调用点直接改 import；删除本地实现与已迁移的 `.test.ts`
- [x] 2.7 移动：`mobile/src/lib/{device-name,device-organization}.ts`、`mobile/src/core/device-trust.ts`
      同上；注意 `MobileDevice` 的字段名差异（`latencyMs`）在**调用点**适配，共享包不设适配层
- [x] 2.8 Web：`docs/app/app/_lib/{device-name,format}.ts` 同上
- [x] 2.9 门禁做成两半：`tsconfig` 的 `lib: ["ES2022"]`（无 DOM、`types: []`）挡全局，
      `scripts/check-shared-view-imports.mjs` 挡 import。**第二道不能省**——实测 `lib` 挡不住
      `import react`，包嵌在仓库根之下，tsc 会一路向上解析到根 `node_modules`。合并为
      `pnpm check:shared-view`，正反例都验过
- [x] 2.10 三端全量回归：根 `pnpm test` + `pnpm build`、`mobile` 的 `pnpm typecheck`、
      `docs` 的 `pnpm typecheck` + `pnpm build` 全绿

## 3. 阶段三 — Web 底座（不碰业务组件）

> 目标：地基铺好，文档区零影响。**本组不重写任何业务组件。**

- [x] 3.1 **Lingui 阶梯 spike**（design D5）—— 优先级最高，其余 i18n 工作的前置。
      依次尝试：① `@lingui/swc-plugin` ② turbopack rule + `babel-loader`（作用域限 `app/app/**`）
      ③ 无 macro 显式 id。在一个 throwaway 组件上验证「宏能编译 + `pnpm i18n:extract` 能提取
      + `next build` 静态导出通过」三件事，把选中的档位与理由写进 change 的 design.md
- [x] 3.2 按 3.1 的结论落地 Lingui 接线：`docs/lingui.config.ts`（`sourceLocale: zh`，
      locales `zh` / `zh-TW` / `en`，catalog 落 `docs/src/locales/{locale}/messages`）、
      依赖入 `docs/package.json`（跟随移动端的 Lingui 6.x）、`docs/package.json` 加 `i18n:extract` 脚本
- [x] 3.3 实现客户端 locale 选择与持久化：首访读 `navigator.languages` 取最接近的受支持 locale，
      无匹配回退 `zh`；显式选择持久化到 `localStorage` 且优先于浏览器偏好；
      **不按 locale 预生成路由**（静态导出 + basePath 叠加）
- [x] 3.4 修复 `docs/lib/cn.ts`：`export { twMerge as cn }` → `clsx + twMerge` 组合。
      docs 此前没有测试运行器，为此补了最小的 `docs/vitest.config.ts`（Web 端即将成为主战场，
      值得有一个）；同时把 `docs/**` 加进仓库根 vitest 的 exclude，两者不重叠
- [x] 3.5 `docs/app/global.css` 加 `@theme inline` 映射层：`--color-fd-*` → shadcn 语义 token；
      `primary` 走品牌色而非 fumadocs primary；fumadocs 未覆盖的 token
      （`destructive` / `input` / `chart-*` / `sidebar-*`）在同一层自给，明暗各一
- [x] 3.6 **实测两端本来就是同一组色**（Web hex 正是桌面 oklch 的 sRGB 转换，最大通道差 0–1）。
      所以改的不是值而是**表达方式**：Web 改用与桌面逐字相同的 oklch 表达式，两份文件可肉眼比对；
      并在注释里写死三个变量的对应关系（`--brand`/`--brand-solid`/`--brand-ink` ↔ 桌面
      `--brand`/`--primary`/`--primary-foreground`）
- [x] 3.7 新增 `docs/components.json`，装入 11 个组件。**shadcn CLI 在 Node 24 下起不来**
      （传递依赖 `@modelcontextprotocol/sdk` 引 `zod/v3` 子路径，3.4.0 与 latest 同样报
      `ERR_PACKAGE_PATH_NOT_EXPORTED`），改为**从桌面 `src/components/ui/` 复制**——桌面那份已经是
      当前形态（统一 `radix-ui` 包，docs 已装同一个），离线确定，且天然保证两端组件行为逐字一致。
      另补 `tw-animate-css`（版本对齐桌面）与 `@layer base` 的默认边框色
- [x] 3.8 **文档区零影响验证**（R4）——改用**静态不变量**而非截图对比：截图需要 baseline
      且只能抽样，而影响发生的机制是可以穷举的。三条全部成立：① 源码里零处改写 `--color-fd-*`
      的值；② 唯一新增的 base 规则限定在 `[data-swarmdrop-app]` 作用域内（桌面那份是全局 `*`，
      这里不能照抄）；③ `tw-animate-css` 的类名在文档区产物里出现 0 次
- [x] 3.9 `docs` 的 `pnpm typecheck` + `pnpm build` 通过

## 4. 阶段四 — 契约成文（与设备页同批）

- [x] 4.1 `DESIGN.md` 第 5 节新增 **Device Card Contract**：8 项信息位清单 + 各端允许的形态差异边界
- [x] 4.2 `DESIGN.md` 新增 **Send Entry Contract**：发送必须从设备进入；发送页选择器只是落点与纠错
- [x] 4.3 `DESIGN.md` 新增 **Cross-platform UI Review Checklist**：可逐条勾选的形态（非散文）
- [x] 4.4 `DESIGN.md` 的 Don't-list 补一条：不得以「布局太挤」为由省略契约要求的信息位
- [x] 4.5 核查发现**桌面端违反契约**：`device-card.tsx` 把信任徽标与连接徽标写成三元
      （`connConfig && latency != null ? 连接徽标 : 信任徽标`），于是**已连接的设备永远看不到信任级别**
      ——而那正是最需要看到它的时刻。移动端两个都出。按非目标不在本次修，已记进 `DESIGN.md`
      的「Known gap (desktop, tracked separately)」，待独立 change 处理

## 5. 阶段五 — Web 设备页重写

- [x] 5.1 新建 `docs/app/app/_components/device-card.tsx`：满足契约全部 8 项信息位
      （设备图标 / 显示名 / 在线点+文案 / 次级身份行 / 信任徽标 / 连接徽标+延迟 / 发送 / 溢出入口）
- [x] 5.2 设备图标与连接徽标：新建 Web 侧的 `os → icon` 与 `connection → icon+label` 映射
      （纯数据部分若可复用则取自共享包，图标与 className 留 Web）
- [x] 5.3 契约的整卡语义在 Web 上落成**卡内显式发送按钮**而非整卡可点：触摸设备上整卡可点
      与「滚动时误触」几乎无法区分，而 Web 端的基线视口就是手机。契约允许这种形态差异，
      前提是发送**直接在卡上可达**（不是两跳），这里满足。离线卡视觉降级 + 按钮禁用并说明原因
- [x] 5.4 响应式网格：<640 单列 · 640–919 两列 · ≥920 三列；触摸目标 ≥44×44
- [x] 5.5 取消配对改用 `AlertDialog`；保留「失败后留在确认态、错误就地显示」的既有语义
      （`device-list.tsx` 顶部注释里的判据仍然成立）
- [x] 5.6 **已补齐**（用户明确要求处理，因此突破了提案「Rust 零改动」的非目标）。
      `crates/web/src/node.rs` 新增 `update_paired_device_policy`，走的是与桌面命令**同一条**
      core 路径（`paired_devices::set_receive_policy`）——落盘 + 节点在跑时推进共享内存表，
      后半句不能省，否则「策略已保存、本次运行仍按旧策略放行」。
      **架构上值得记的一条**：Web 端**没有**抄「各信任级别的默认策略」那张表。桌面
      （`defaultPolicyForTrust`）与移动（`defaultReceivePolicy`）各抄了一份、权威版在 Rust 的
      `for_trust_level`，已经三份。新 API 的 `receivePolicy` 是可选的，不传就由内核派生——
      于是切换级别在前端只是「少传一个参数」。代价（切级别会重置开关）在 UI 上明说，不藏。
      顺带给 `serialize.rs` 补了入站方向的 `from_js` 与两条 wasm 回归守卫，与既有的
      序列化守卫成对。三条链路已重跑（specta 无变化 → `build:wasm` → `view-types.ts`）
- [x] 5.7 新增别名与分组编辑（对齐桌面 `device-organization-dialogs.tsx` 的能力，逻辑取自共享包）
- [x] 5.8 重写 `/app/devices` page：卡片网格 + 空态（教学文案指向配对）+ 配对入口折叠为次级区块；
      **不引入**「附近未配对设备」与节点启停 sheet（Web 上不成立，design D6）
- [x] 5.9 删除 `docs/app/app/_components/device-list.tsx`
- [x] 5.10 三道机器门禁：`pnpm check:zustand-access`、`docs` 的 `pnpm typecheck`、`pnpm build`

## 6. 阶段六 — Web 发送流程改为设备优先

- [x] 6.1 发送主入口改为设备卡片；`/app/send?peerId=` 降级为直达落点
- [x] 6.2 `<select>` 降级为**两种形态**：带着有效 `?peerId=` 进来时呈现只读的目标卡 +「更换」；
      直接访问或点了「更换」才出下拉框，且同时给一条回设备页的路。让用户确认自己刚在设备页
      做过的选择，是表单优先的残留
- [x] 6.3 发送面板改用 shadcn 原语（`Button` / `Card` / `Select`），拖拽区与 prepare 进度保留
- [x] 6.4 移动优先重排：窄屏单列、触摸目标 ≥44×44、无 hover-only 交互
- [x] 6.5 三道机器门禁通过

## 7. 阶段七 — Web 收件箱与传输页改主从布局

- [x] 7.1 新建 `docs/app/app/_components/master-detail.tsx`：`(min-width: 920px)` 双栏 / 窄屏详情占满
      + 列表抽屉。注释与桌面 `src/hooks/use-media-query.ts` 的 `MASTER_DETAIL_QUERY` 互指
- [x] 7.2 收件箱改主从：`receive-panel.tsx`(605 行) 拆为列表列 + 详情列；选中态走 query param
- [x] 7.3 传输页改主从：`transfer-activity-panel.tsx` 同上；选中态继续走 `?session=`。
      顺带改掉一处语义：手风琴时代点已展开项会收起，主从下**点选即选中不再 toggle**——
      详情是独立一栏（窄屏更是整屏），把它「收起」只留下一个空面板
- [x] 7.4 两页的 `useSearchParams()` 均包在 `<Suspense>` 内（静态导出既有约束）
- [x] 7.5 收件箱/传输的分工不变（结果 vs 过程，文件生命周期属收件箱侧）——重构不得改变这条
- [x] 7.6 验证断点一致性：视口在 920px 附近变化时两页同时切换形态
- [x] 7.7 三道机器门禁通过

## 8. 阶段八 — Web 设置页与外壳收尾

- [x] 8.1 设置页三块（节点 / 连接 / 事件日志）改用 shadcn 原语；
      事件消费仍只在 layout 单点，本页只展示
- [x] 8.2 新增 locale 切换入口到设置页
- [x] 8.3 复核 `WebNodeBootstrap` 仍是运行时单例的唯一挂载点，未因组件拆分而下沉到任一 page
- [x] 8.4 复核导航单一事实源：全站无手拼 `/app/xxx` 字面量（含新增的主从页 query builder）
- [x] 8.5 `nav.ts` 的 `label` / `description` 改成**可翻译描述符**，一份同时服务两处：
      组件运行时 `t(item.label)` 展开、构建期的 `metadata` 走 `navTitle()` 取源文。
      静态导出下 `<title>` 只能是源 locale（构建期没有「当前用户」），这是正确行为不是漏翻。
      另修一处连带问题：`PageHeader` 变 client 后不能再收整个导航项——`AppNavItem` 带 `icon`
      函数组件，函数跨不了 RSC 边界（`next build` 预渲染直接报错），改成收一个 key

## 9. i18n 全量与验收

- [x] 9.1 `docs/app/app/**` 全量文案改 i18n 宏：正文/标题、按钮/菜单、空态/教学、错误/状态提示、
      无障碍属性（`aria-label` / `title` / `alt`）
- [x] 9.2 确认机器值未进 catalog：PeerId、multiaddr、哈希、文件名、原始字节数
- [x] 9.3 提取出 223 条，`zh-TW` 与 `en` 各译满 223、0 缺失。**繁中不是逐字转码**——
      按台湾用语转（檔案 / 裝置 / 網路 / 連線 / 儲存 / 搜尋 / 封存 / 剪貼簿 / 工作階段）。
      另跑了一道占位符校验：`{0}` / `{left}` / `<0>…</0>` 在两份译文里逐条与 msgid 对齐，0 处不一致
- [x] 9.4 三个 locale × 五条路由已走查（起**静态产物** server 而非 dev server，一个进程、验完即停）。
      **抓到一处真漏**：收件箱的「已接收」标题有两处、只包了一处，`en` 下裸露源文——
      catalog 因此从 223 涨到 224。补齐后三个 locale 全部干净：`en` 下零中文残留、
      `zh-TW` 下零简体字泄漏。设置页的「简体中文 / 繁體中文」是语言**自称**，刻意不翻
      （给英文用户看 "Simplified Chinese" 他反而认不出）
- [x] 9.5 验证路由数不随 locale 增长（`next build` 产物清单与单 locale 时一致）
- [x] 9.6 文档正文与营销页未被纳入 i18n，呈现与接入前一致

## 10. 收尾

- [x] 10.1 全量门禁：根 `pnpm test` + `pnpm build`、`pnpm check:zustand-access`、
      `cargo check --workspace --all-targets`（应零变化）、`mobile` 的 `pnpm typecheck`、
      `docs` 的 `pnpm typecheck` + `pnpm build`
- [x] 10.2 四档视口已验，断点**精确在 920 翻转**（探的是 computed style，不是 DOM 存在性——
      侧栏与底部导航都常驻 DOM、靠 CSS 切换）：
      | 视口 | 侧栏 | 底部导航 | 主从 |
      |---|---|---|---|
      | 375 | none | block | 单栏 |
      | 768 | flex | none | 单栏 |
      | **920** | flex | none | **340px + 内容列** |
      | 1440 | flex | none | 双栏 |
- [x] 10.3 `dev-notes/knowledge/web-app-frontend.md` 更新：shadcn 底座与 token 映射层、
      920 断点、移动优先基准、Lingui 接入档位与踩坑
- [x] 10.4 **并入 `toolchain.md`** 而不是新建文件：归属判据（什么该进/不该进）本就该住在
      包自己的 README（改包的人先看它），而跨 workspace 接线的坑是 toolchain 的事。
      新建一个薄文件会让同一件事有两个入口
- [x] 10.5 `CLAUDE.md` 更新：workspace 布局新增 `packages/shared-view`；
      Web 端章节的「手写原生元素」描述改为 shadcn 底座；i18n 章节补 Web 端
- [x] 10.6 机器门禁 ✓ · `/simplify` ✓（自审四角度，修了 4 处：`_lib/format.ts` 的纯转发层
      与两个零消费者的再导出、设备网格同名判定 O(n²)→O(n)、主从遮罩顶着抽屉标签的全屏 button
      改 `aria-hidden` div）。
      **`/code-review` 需要你触发**——它是用户触发且计费的命令，我起不了。
      我自查了最终形态，抓到并修了 3 处正确性问题：
      ① `connectionLabel` 的兜底 `msg\`\`` 每次调用新建对象、② `renderRow` 每帧现造 `onSelect`
      箭头函数——两者都会打穿 `TransferActivityItem` 的 memo，而那个 memo 正是「一个会话每秒
      十余次进度事件只重渲染它自己那一行」的依据；
      ③ `useLocaleSwitcher` 先持久化再激活，chunk 加载失败会把偏好写死在一个用不了的 locale 上
