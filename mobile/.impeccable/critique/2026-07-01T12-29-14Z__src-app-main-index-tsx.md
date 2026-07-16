---
target: 主页(设备列表 Devices Tab)
total_score: 20
p0_count: 2
p1_count: 2
timestamp: 2026-07-01T12-29-14Z
slug: src-app-main-index-tsx
---
Method: dual-agent (A: aa05f4c21134e54cd · B: a29b36cf2f6dd6a86; B 的 CLI 扫描因初次路径解析误差由主线程用正确路径复核确认,结论一致)

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 2 | `runtimeState==="error"` 落入 else 分支,和 "节点未启动" 显示完全相同文案,只有右上角一个小色点区分 |
| 2 | Match System / Real World | 3 | ConnectionBadge 把"打洞"这种 NAT 协议黑话直接印在首屏已配对设备行上 |
| 3 | User Control and Freedom | 2 | 配对请求发出后禁用整份附近设备列表,无取消按钮、无超时兜底 |
| 4 | Consistency and Standards | 2 | `rounded-2xl`/`rounded-xl`/`rounded-[10px]`/`h-12` 等多个未登记进 DESIGN.md token 的圆角与高度值同屏并存 |
| 5 | Error Prevention | 3 | 配对流程有防重复点击等好设计,但"停止节点"这种破坏性动作零二次确认 |
| 6 | Recognition Rather Than Recall | 3 | 图标+颜色+文字三重编码识别成本低,但"打洞/中继"仍需专业知识才能"识别" |
| 7 | Flexibility and Efficiency of Use | 1 | 已配对列表无搜索/排序/长按菜单/下拉刷新,老手用户效率入口极窄 |
| 8 | Aesthetic and Minimalist Design | 2 | 运行态首屏同时铺开 5 个并列面板,密度接近仪表盘堆叠 |
| 9 | Help Users Recognize/Diagnose/Recover from Errors | 1 | 启动失败把原始 error 字符串直接丢进一次性 toast,无分类翻译、无持久提示 |
| 10 | Help and Documentation | 1 | 首屏无任何术语解释或帮助入口 |
| **Total** | | **20/40** | **Acceptable —需要显著改进才能让用户满意** |

## Anti-Patterns Verdict

**LLM 评估**:没有渐变、玻璃拟态、装饰性投影这些"一眼AI感"的滥用;TrustBadge/StatusPill/ConnectionBadge 的色彩纪律(10% tint pill、状态色只表状态)完全够得上 Linear/Stripe 水准。但组装成整屏后,精通同类工具的用户会在三处"心里咯噔一下":运行态首屏五个并列面板的仪表盘式堆砌(直接撞上 PRODUCT.md 反面参考)、圆角体系失控(`rounded-2xl`/`rounded-xl`/`rounded-[10px]` 无一在 DESIGN.md 的 sm/md/lg/full 表里)、错误文案未经打磨(原始 error 字符串直接丢给用户)。

**确定性扫描**:`detect.mjs --json` 对 index.tsx + device-card.tsx + status-pill.tsx + trust-badge.tsx + connection-badge.tsx 扫描,exit code 0,0 命中。这不代表"设计干净"——该检测器的正则规则库只匹配字面 Tailwind 调色板类名(如 `text-purple-600`、`border-l-4`)和需要整页 DOM 结构的 page-analyzer(要求 `<html>/<head>`),对 NativeWind 的 `gap-*`/`text-[Npx]`/`active:` 语法和 React Native 源码结构性地无法触达,因此 0 命中只能证明"没有踩中那几条字面规则",不能证明排版层级、间距节奏等问题不存在。

**可视化取证**:未执行。已确认 8081 端口无监听、无 booted 模拟器,这是纯原生 App,没有可截图的运行时 WebView 上下文,属于 fallback signal,非实测结果。

## Overall Impression

组件级别干净自律,尤其是状态类组件(TrustBadge/StatusPill)完全对得起"信任的门厅"这个北极星。但首屏编排的密度和几处未收尾的边缘(圆角体系分裂、错误态被吞掉、配对卡死无退出)会让懂行用户的信任感打折扣——不是"糊弄",而是"没收尾"。最大的机会点:把运行态首屏从"五个面板并列"收敛成"一个清晰焦点",并把 `error` 状态从 `stopped` 里独立出来。

## What's Working

- **TrustBadge 的"· 待确认"后缀处理**(trust-badge.tsx L32-37):用文字后缀而非新色相表达待定态,严格遵循 DESIGN.md 规则,是全屏最值得当范例的一处状态设计。
- **NearbyDeviceRow 的行级 loading**(index.tsx L708-716):只在被点击的那一行把按钮换成 ActivityIndicator,而不是全屏遮罩,异步反馈精确定位到用户刚做的动作上。
- **附近设备的渐进展开**(NEARBY_COLLAPSED_COUNT=4 + "查看全部/收起"):首屏保持轻量,把复杂度留给愿意深入的人,是"可见性优先于复杂度"北极星的正确落地。

## Priority Issues

**[P0] 节点出错状态与"未启动"共用同一套文案**
- Why it matters:节点真正出错(端口占用/权限被拒)时,用户看到的首屏文案和"压根没点启动"一模一样,只会反复点"启动"却不明白为什么没反应。
- Fix:给 HomeTransferPanel 增加第四种视觉状态——error 时用 destructive 图标+"节点出错了"标题+错误摘要+"重试"按钮,不再与 stopped 共用分支。
- Suggested command: `/impeccable harden`

**[P0] 主 CTA 按钮文字对比度低于 WCAG AA**
- Why it matters:启动节点、发送、复制配对码这些最高频操作入口,浅色文字铺在 Trust Blue 背景上约 3.5:1 对比度,低于正文 4.5:1 门槛,PRODUCT.md 明确承诺 WCAG AA,低视力用户可能读不清最核心的按钮。
- Fix:参照 DESIGN.md 的 Flipped Ink Rule 思路,light mode 下也让 `--primary-foreground` 使用能过 4.5:1 的深色墨色,或单独定义已验证对比度的 on-primary token。
- Suggested command: `/impeccable colorize`

**[P1] 运行态首屏五个面板并列,密度接近仪表盘堆叠**
- Why it matters:直接撞上 PRODUCT.md 反面参考"企业SaaS后台仪表盘堆砌",认知负荷清单里"单一焦点/一次一件事/选择点≤4"三项同时不达标,对困惑新手和易被打断的移动用户尤其不友好。
- Fix:把"添加设备"收敛成一个轻量入口,点击后再展开附近设备/配对码/输入配对码,首屏顶层面板数控制在 3 个以内。
- Suggested command: `/impeccable distill`

**[P1] 配对请求卡住无退出路径**
- Why it matters:对端握手中途掉线或用户手滑点错设备时,整份附近列表被禁用且没有取消按钮或超时兜底,用户只能干等,是可被轻易复现的真实死路。
- Fix:给正在配对的行加"取消"点击区域(本地立即重置状态),并给配对请求包一层客户端超时(如15秒)自动失败并提示。
- Suggested command: `/impeccable harden`

**[P2] 发送按钮的 accessibilityLabel 不带设备名**
- Why it matters:屏幕阅读器用户逐行滑动已配对列表时会连续听到完全相同的"发送文件",无法分辨当前焦点在哪台设备,是无障碍标准里最容易验证的具体缺口。
- Fix:改为动态拼接,例如"向 {displayName} 发送文件"。
- Suggested command: `/impeccable clarify`

## Persona Red Flags

**Sam(屏幕阅读器/对比度)**:主 CTA 对比度约 3.5:1 未达标;发送按钮 accessibilityLabel 不带设备名,逐行滑动听不出差异;OTP 六个格子各自包一层 Pressable,屏幕阅读器可能逐个念"空"而不知道这是一个整体验证码输入。

**Riley(边界压力测试)**:强制切到 error 状态会发现英雄区仍显示"节点未启动";附近/已配对都为空时两套"暂无内容"文案会同屏重复出现;切换筛选 tab 不会清空上一次配对失败留下的过期错误文本。

**Casey(单手移动/易被打断)**:唯一能开关节点的 StatusPill 放在 AppHeader 最右上角,是单手持握时拇指最难触达的区域;配对进行中若被打断后回来手滑点了另一台设备,界面静默无响应,没有任何反馈。

## Minor Observations

- `HomeTransferPanel` 用 rounded-2xl,其余用 rounded-lg,同屏两套圆角语言。
- 启动按钮与 HomeShortcut 都用 h-12(48px),落在 DESIGN.md 定义的 36/40/44 三档之外。
- 活跃传输区硬编码 `.slice(0,3)`,超过3条无"还有N条"提示。
- `ConnectionBadge` 的"打洞"标签把协议黑话印在首屏,与设计原则3相悖。
- 页面同时存在两个独立的秒级计时器(配对码倒计时+运行时长),值得关注重渲染开销。

## Questions to Consider

- 如果"被动接收优先于主动操作"是北极星,为什么运行态首屏还要求用户同时消化五个并行信息源,而不是给一个"现在该做什么"的单一焦点?
- 英雄区已经区分 running/starting/stopped 三种状态讲故事,却唯独把 error 悄悄折叠进 stopped——一个出错的节点真的应该长得和从未启动过的节点一模一样吗?
- 把"打洞/中继/NAT"这类词汇印在首屏,是在服务多数普通用户,还是在为少数极客用户的口味牺牲了大多数人的"亲和优先于专业黑话"体验?
