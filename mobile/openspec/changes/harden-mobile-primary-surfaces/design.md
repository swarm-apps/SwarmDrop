## Context

`/impeccable critique` 对主页(Devices)、收件箱(Inbox)、设置(Settings) 三个界面做了独立设计评审(Assessment A 设计评审 + Assessment B 检测器取证,均无法做实机可视化,因为当前无 dev server/无 booted 模拟器),报告落在 `.impeccable/critique/`。三份报告的 14 项 Priority Issues 反复撞在同两条 PRODUCT.md 原则上——「状态先于装饰」和「掌控感来自可见性而非管理复杂度」——而不是零散的视觉瑕疵。这份设计把 14 项问题归到 4 个能力域,并给出跨界面共享的技术决策(尤其是对比度 token),避免同一类问题在三个界面各修一遍、修出三套不一致的方案。

当前实现约束:NativeWind v5 + Tailwind v4(`src/global.css` 的 CSS 变量是唯一色彩真相源)、shadcn 风格组件(`src/components/ui/*`)、Zustand store(`src/stores/*`)、无自动化 UI 测试覆盖这几个屏(验证只能靠人工/Maestro 走查 + 重跑 critique 打分)。

## Goals / Non-Goals

**Goals:**
- 让「进行中/出错/已停止/真的没有」这几种系统状态在视觉上互相区分,不再共用同一句文案或同一个静态图标。
- 把已知的 light-mode 主色按钮对比度缺陷(~3.5:1)一次性修到 WCAG AA(≥4.5:1),覆盖三个界面的所有 `bg-primary` 按钮。
- 降低主页运行态首屏和设置 Network 页的信息密度,把协议级/管理级细节收进二级入口,而不是从产品里砍掉这些信息。
- 让破坏性操作的确认强度和真实风险对齐(该拦的拦住,不该拦的别拦)。
- 收敛本次评审中新发现的 DESIGN.md token 漂移(圆角、两个游离字号)。

**Non-Goals:**
- 不新增功能性能力(不做批量操作、不做 swipe 手势、不做搜索排序等 Flexibility/Efficiency 类的深度改造——这些在报告里多是 P2 及以下或未被列为 Priority Issue,留给后续单独的 `/impeccable` 迭代)。
- 不改动 uniffi 绑定层或 Rust 核心协议行为,只消费已有的状态/错误信息。
- 不做桌面端对齐调整——本次范围是移动端这三个屏。
- 不引入新的 design token 层级(如新增 `rounded-xl`);优先向现有 DESIGN.md 已登记的 token 收敛。

## Decisions

### 1. `--primary-foreground`(light mode)统一改用深色墨色,而不是加深 `--primary`

`src/global.css` 里 dark mode 已经证明:深色文字(`#0F172A`)铺在 `--primary`(`#3B82F6`)上约 4.9:1 对比度,比浅色文字的 ~3.5:1 更好。方案是把 light mode 的 `--primary-foreground` 也改成同一档深色墨色(`222.2 47.4% 11.2%`,和 dark mode 完全一致),而不是加深 `--primary` 本身。

**为什么不选加深 `--primary`**:`--primary` 同时驱动 Trust Blue 的强调色身份(`bg-primary/10` 徽标、focus ring、图标着色等),改它的色相/明度会让"信任的门厅"整套调色板的锚点跟着漂移,影响面远大于对比度修复本身要解决的问题;而只调 `--primary-foreground` 是一次隔离、可逆、影响面明确的 token 改动。

**副作用**:DESIGN.md 里"Flipped Ink Rule"这条 Named Rule 需要同步更新——light/dark 不再是"翻转",而是"统一用深色墨色",规则名和描述都要改,避免文档和代码脱节。

**备选方案(已否决)**:只在按钮组件层用 `className` 覆盖("哪里用哪里改"),不动全局 token。否决理由:三个界面的按钮、Badge、图标着色都读同一个 CSS 变量,组件级覆盖会造成三套不一致的临时方案,且未来任何新用到 `bg-primary` 的地方还会继续踩坑——修 token 一次性解决,修组件是在埋雷。

### 2. 状态区分统一走"新增独立分支",不复用现有分支加条件判断

主页的 `error` 态、收件箱的"加载失败 vs 已删除"、设置的"检查中"态,统一的实现模式是:在渲染逻辑里为每种状态开一个显式的、互斥的分支(而不是在现有分支里加 `if`),每个分支有自己的图标/文案/操作按钮。这样每个状态都可以独立做视觉设计和无障碍标注,不会因为共享分支而被迫共用文案。

**收件箱"加载失败 vs 已删除"的具体决策**:这需要 `loadDetail` 能区分"调用抛出异常(网络/DB 瞬时问题)"和"调用正常返回但记录确实不存在"两种情况——目前 `inbox-store.ts` 的 catch 分支是否已经能做这个区分,是一个需要在 apply 阶段先确认的 Open Question(见下)。设计上默认假设:能区分则展示两种文案+对应操作(重试 / 无操作只提示);不能区分则至少先把 `lastError` 渲染出来(不再只 console.warn),文案上把"记录不存在"改成不做"已删除"这种确定性断言的更谨慎措辞(如"暂时无法打开,请重试"),等绑定层补齐区分能力后再上线更精确的双态文案——即"诚实地不确定"优于"自信地说错"。

### 3. 信息密度收敛统一用"渐进展开"模式,不删除信息

主页的"添加设备"面板和设置 Network 页的协议指标,统一用同一个交互模式:默认只展示一个轻量入口/合成状态,点击后展开完整信息,而不是从产品里砍掉这些数据(它们对 Alex/极客用户仍然有价值,只是不该是所有人的默认视图)。这与已经在这两个界面里跑通的其他渐进展开先例(附近设备的"查看全部/收起"、DetailsPanel 的展开态)保持同一套交互语言,不是发明新模式。

**Network 页"合成状态"的具体判定逻辑**:用什么字段合成"良好/受限"这一句话,是一个需要在 apply 阶段读 `network.tsx` 现有 state 形状后再定的 Open Question——设计上只约束"必须是从已有字段派生,不需要新的原生绑定字段"。

### 4. 破坏性操作确认强度:按"是否可逆 + 是否影响连通性/数据"重新分级,而不是维持现状

`ConfirmDialog` 的 `destructive` 样式只用于"不可逆 或 影响文件/网络可用性"的操作。据此:"恢复默认接收位置"(可逆的本地偏好)降级为中性确认;"删除自定义引导节点"(可能影响连通性)提级为轻量确认+完成 toast。这条分级规则同时写进 DESIGN.md 的 Do's/Don'ts,避免未来新加的设置项再次出现风险信号错位。

### 5. Rounded token:向 DESIGN.md 已登记的 `lg`(10px)收敛,不新增 `xl` 层级

Settings 家族里出现的 `rounded-xl` 统一改回 `rounded-lg`,而不是把 `xl` 正式补进 DESIGN.md 的 rounded 尺度。

**为什么不新增 `xl`**:DESIGN.md 的 Named Rule("Almost-Flat Rule"/"Small-Print Confidence Rule")一贯的纪律是"克制、封闭的小 token 集",本次评审里其它界面(主页的 `rounded-2xl`、收件箱的任意值 `rounded-[28px]`)同样是 token 集之外的漂移值——如果这次为了图省事新增一级,等于承认"token 集会持续膨胀",与设计系统的既定纪律相悖。收敛到已有值是唯一和现有纪律一致的选择。主页/收件箱里其余漂移的圆角与字号值,留给各自能力域的 spec 逐条列出并收敛到已登记 token(sm/md/lg/full,Title/Headline/Body/Label)。

## Risks / Trade-offs

- **[风险] 把主页"添加设备"面板收起为二级入口,可能让首次使用的新用户少看到一次"这里能配对"的提示** → 缓解:入口本身仍在首屏最显眼位置(一个大按钮,不是藏进菜单),只是子任务(附近设备/本机码/输入码)延迟到点击后展开,不影响"配对"这个动作本身的可发现性。
- **[风险] 客户端配对超时(如 15s)可能在弱网下把仍在进行的真实握手误判为失败** → 缓解:超时只触发 UI 恢复(重新启用列表、给出"对方无响应,可重试"提示),不取消底层原生请求本身;若原生 SDK 本身也有超时机制,以先触发者为准,apply 阶段需要先确认原生侧是否已有超时语义,避免两层超时打架。
- **[风险] 修改 `--primary-foreground` 影响全 App 所有 `bg-primary` 表面,不止这三个屏** → 缓解:dark mode 已验证过同一改法不会破坏可读性(它已经在用深色文字);仍建议 apply 完成后对全 App(不限于这三屏)做一次亮色模式下的 `bg-primary` 视觉扫读,避免遗漏。
- **[风险] 收件箱"加载失败 vs 已删除"的区分依赖绑定层能否提供足够信息,如果暂时做不到精确区分** → 缓解:设计里已经给了降级方案(先展示 `lastError`+更谨慎的措辞,不做"确定性删除"断言),不阻塞本次上线,精确区分留待绑定层能力补齐后再迭代。
- **[风险] ContentPreview 从"大留白+图标"改成"内联文字摘录",对已经习惯当前视觉的用户是一次可感知的界面变化** → 缓解:范围限定在文本/剪贴板/多文件合集这几类"本来就该看到内容"的类型,图片类型的预览保持不变。

## Migration Plan

无数据/schema 迁移,全部是 UI 状态分支、交互结构与视觉 token 的调整,可按能力域独立上线、独立回滚:

1. **先落地 `accessible-primary-contrast`**(改 `src/global.css` 一个 token + 同步 DESIGN.md/`.impeccable/design.json`):影响面最广但改动最小、风险最低,优先验证。
2. **`devices-surface-hardening`**:error 视觉态 → 面板收敛 → 配对超时/取消 → a11y label,四项可分别提交,每项都能独立验证。
3. **`inbox-integrity-hardening`**:先做"至少渲染 `lastError`"这个最小改动止血,再视绑定层调研结果决定是否上线精确的双态文案;FilterRail 收敛、搜索筛选一致性、ContentPreview、字号可并行推进。
4. **`settings-hardening`**:静态 Loader→ActivityIndicator 和 NetworkHint 硬编码色是纯本地小改动,可最先做;风险信号对调、Network 页折叠、圆角统一其次。
5. 全部落地后,重跑 `/impeccable critique` 复核 `src-app-main-index-tsx` / `src-app-main-inbox-tsx` / `src-app-main-settings-tsx` 三个 slug,用 `trend` 验证分数相比本次基线(20/22/24)提升,且此前的 P0/P1 清零。

回滚策略:每个能力域各自的改动互不依赖(除了都读同一个 `--primary-foreground` token),出问题可按能力域单独 revert,不影响其它域已上线的修复。

## Open Questions

- `inbox-store.ts` 的 `loadDetail`/`refresh`/`runSearch` 目前的 catch 逻辑,是否已经能区分"调用异常"与"合法的 not-found"?还是两者目前就是同一条路径?需要在 apply 阶段读实际代码确认,决定 P0 修复是做"精确双态"还是先做"至少露出 lastError"的降级版本。
- `handlePair`/`requestPairing` 背后的原生调用(uniffi 绑定)是否已有内建超时?如果有,客户端 15s 超时的取值需要避免和原生超时冲突或抢跑。
- `network.tsx` 现有的 state 字段里,哪些已有信号可以合成"网络状况:良好/受限"这一句话(不新增原生字段的前提下)?需要读该文件的实际 state 形状后再定判定逻辑。
