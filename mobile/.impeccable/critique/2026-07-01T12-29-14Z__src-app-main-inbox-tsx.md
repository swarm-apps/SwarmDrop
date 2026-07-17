---
target: 收件箱(Inbox Tab + 详情/搜索)
total_score: 22
p0_count: 1
p1_count: 1
timestamp: 2026-07-01T12-29-14Z
slug: src-app-main-inbox-tsx
---
Method: dual-agent (A: a8ab2f16d3e4258f4 · B: a37e32f0b6fcb4697)

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 2 | store 里认真维护的 `lastError` 在 inbox.tsx/search.tsx/[itemId].tsx 三个文件里被引用 0 次,失败时只 console.warn |
| 2 | Match System / Real World | 3 | 日常文案口语友好,但详情面板把"AI 代理"和裸露协议缩写"(MCP)"并列展示 |
| 3 | User Control and Freedom | 2 | 进入关键词搜索后,之前选好的类型筛选被静默忽略且无处找回 |
| 4 | Consistency and Standards | 2 | 浏览态 InboxRow 与搜索命中态 InboxHitRow 对同一份数据渲染不同的状态徽标 |
| 5 | Error Prevention | 3 | "仅删除记录"与"删除记录和本地文件"两个高风险选项紧挨,仅靠文字区分 |
| 6 | Recognition Rather Than Recall | 3 | FilterRail 每个 chip 都标计数,但详情页标题恒为静态"收件箱详情",不显示当前记录名 |
| 7 | Flexibility and Efficiency of Use | 1 | 归档/删除一条记录需 3-4 次点击,无 swipe 手势、无批量操作 |
| 8 | Aesthetic and Minimalist Design | 3 | ContentPreview 对非图片内容渲染大面积纯装饰留白,无真实状态信息 |
| 9 | Help Users Recognize/Diagnose/Recover from Errors | 1 | "加载失败"与"记录真的被删除"共用同一句文案,且无重试按钮 |
| 10 | Help and Documentation | 2 | DetailsPanel 展示裸露的 peer id 和 hash,无任何解释 |
| **Total** | | **22/40** | **Acceptable —需要显著改进才能让用户满意** |

## Anti-Patterns Verdict

**LLM 评估**:日常操作层面(卡片/徽标/确认弹层/空态文案)可信,颜色几乎只用于表状态,严格遵守 DESIGN.md 的 Don't 清单。但三处偏离暴露"设计系统没被真正当回事":详情标题 22px、工具栏计数 26px 两个游离字号(文档把 30px Display 明确限定给传输进度这一处);圆角出现任意值 `rounded-[28px]` 及多处 `rounded-xl`/`rounded-2xl`;ContentPreview 对文本类内容的大幅纯图标留白在精神上与 PRODUCT.md 排斥的"社交类视觉焦点"相似。更深的诚实度问题:错误态和"真的没有/真的被删除"共用一模一样的文案与空态。

**确定性扫描**:对 inbox.tsx + [itemId].tsx + search.tsx 扫描,exit code 0,0 命中。已交叉核实:检测器的正则规则库不含任何会匹配 `gap-*`/`text-[Npx]`/`active:` 等 NativeWind 语法的规则,8 个 page-analyzer(单一字体/扁平层级/间距单调等)因要求 `<html>/<head>` 整页结构而对 .tsx 文件结构性不可达。因此 0 命中不能证明排版层级、间距节奏等问题不存在——本次 Assessment A 发现的字号/圆角偏离,恰恰是这套检测器覆盖不到的盲区,LLM 评审在此正好补上了自动化工具的缺口。

**可视化取证**:未执行。已确认 8081/19000/19006 端口均无监听,无 booted 模拟器,无 adb 设备,fallback signal。

## Overall Impression

删除确认流程和文件缺失/修复闭环是全篇处理得最好的高风险节点,直接体现"安心"品牌承诺。但"加载失败"与"记录真的被删除"共用同一套死路 UI 是本次审查里最伤信任的一处——用户拿到一个说不清楚的坏结果,却被告知"可能已经被删除"。最大机会点:把 `lastError` 从 store 里实际渲染出来,让错误态和"真的没有"分开说话。

## What's Working

- **删除确认的两级文案**(ConfirmDialog):对"仅删除记录"与"删除记录和本地文件"给出完全不同的后果描述,是本次审查里高风险节点处理最好的地方。
- **文件缺失/修复闭环**:InboxToolbar 异常 banner、ContentPreview 专属缺失文案、FileRow"缺失"徽标三处联动,完整覆盖"文件被外部移动"这个真实边界情况。
- **DetailsPanel 的渐进展开**:默认只显示接收时间/保存位置,协议细节(peer id、hash)收在"详情"展开态,精准对应设计原则3。

## Priority Issues

**[P0] 加载详情失败与记录被删除共用同一套死路 UI**
- Why it matters:直接违背 PRODUCT.md"对传输状态与历史保持清晰掌控"和"安心"品牌承诺——用户看到一句不真实的"可能已删除",实际可能只是瞬时错误,既误导又无恢复路径;store 里的 `lastError` 从未被三个页面读取渲染。
- Fix:在加载失败与合法 not-found 之间加区分状态,前者展示"加载失败,请重试"+重试按钮,后者才展示"可能已被删除";refresh()/runSearch() 失败路径也要把 lastError 曝光出来,而不是只 console.warn。
- Suggested command: `/impeccable harden`

**[P1] FilterRail 7 个筛选 chip 平铺一行无分组提示**
- Why it matters:同时违反"选择点≤4"和"组块化≤4项一组",且"已归档/异常"这类救援性筛选完全没有"还能横滚"的视觉提示,容易被彻底忽略。
- Fix:收敛为主行≤4个高频筛选+一个"更多筛选"入口,或至少加尾部渐隐/滚动箭头提示。
- Suggested command: `/impeccable clarify`

**[P2] 关键词搜索会静默丢弃已选筛选,且搜索结果不带状态徽标**
- Why it matters:违反一致性与用户控制——"先筛类型再搜关键词"的心智模型被无声打断,搜索结果可能悄悄指向一条"文件缺失"的记录,用户点进去才发现。
- Fix:让搜索叠加当前筛选(或对结果做客户端二次过滤),切到关键词搜索时用一行小字说明;给搜索结果行补齐与浏览态相同的状态徽标。
- Suggested command: `/impeccable harden`

**[P2] ContentPreview 对文本/剪贴板内容是纯装饰性留白**
- Why it matters:不承载状态信息,与"视觉表现让位于状态清晰度"原则有摩擦,也拖慢了"一眼看到内容"的核心目标——文本类内容甚至看不到收到的文字本身。
- Fix:非图片内容把预览框压缩到较矮的定长高度,文本/剪贴板类型直接内联展示前 N 个字符的正文摘录。
- Suggested command: `/impeccable distill`

**[P3] 两个游离字号(22px/26px)未登记进 DESIGN.md**
- Why it matters:削弱"10-15px 承载几乎全部层级"这条 Named Rule 的纪律性,是本次审查里最直接的设计系统偏离证据。
- Fix:详情标题降到已有的 Title(15px)档位或启用 Headline(24px);工具栏计数收回 Title/Body 量级,用色彩/字重制造强调。
- Suggested command: `/impeccable typeset`

## Persona Red Flags

**Alex(效率型老手)**:归档/删除必须"详情页→更多操作→二次确认"至少 3-4 次点击,无 swipe/长按菜单;进入关键词搜索后想"在已归档里搜"做不到。

**Riley(边界压力测试)**:详情加载失败与记录被删除共用同一句文案且无重试按钮;FTS 搜索异常后落入和"真的搜不到"一样的空结果态;错误 toast 直接把底层协议字符串塞进面向用户的提示。

**Sam(屏幕阅读器/对比度)**:主 CTA 按钮同样受 3.5:1 对比度缺陷影响;多个纯图标按钮的 accessibilityLabel 是硬编码中文字面量而非走 i18n,切换语言时屏幕阅读器仍只念中文。

**陈姨(项目专属·55岁普通消费者)**:一进收件箱可能先看到红色"发现N条异常内容"的提示,容易让她觉得手机出问题了;"来源设备"十六进制ID和"内容指纹"哈希没有任何解释;记录打不开时统一显示"可能已被删除",会让她以为弄丢了孩子的照片,产生不必要的焦虑。

## Minor Observations

- StatePill 一次只能显示 missing 或 archived 二选一,若某条记录两者皆是,"已归档"信息会被"缺失"盖掉。
- FilterChip 即使计数为 0 也照常显示,横向行更拥挤。
- rounded 出现多处未登记取值(rounded-xl/rounded-2xl/任意值 rounded-[28px])。
- 修复中状态下 toolbar 文案仍固定显示"发现N条异常内容",只有图标换成 spinner。
- DetailsPanel 的"保存位置"直接展示 decodeURIComponent 后的原始文件系统路径,对普通消费者是陌生格式。

## Questions to Consider

- 如果收件箱列表和搜索结果用的是同一份数据模型,为什么两条渲染路径对状态的展示不一致——这是技术债,还是产品认为搜索场景不需要这些状态?
- 当加载/搜索失败和"真的没有数据"在 UI 上长得一模一样时,用户要如何相信这个 App 真的在"让我随时清楚传输状态"?
- 7 个筛选 chip 平铺在一条可以无提示横滚的行里,这是最快的分诊方式,还是只是把桌面端标签搬过来、还没为移动端重新设计过?
