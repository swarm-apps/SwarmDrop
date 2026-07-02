---
target: 引导流程 + 设备页面
total_score: 25
p0_count: 1
p1_count: 2
timestamp: 2026-07-02T07-32-07Z
slug: src-app-onboarding-welcome-tsx
---
Method: dual-agent (A: a893be2b61bb7fef1 · B: aaa5cb91467bdda74)

# 引导流程 & 设备页面 设计评审

评审范围:onboarding 三屏(`welcome.tsx` / `device-name.tsx` / `setup.tsx`)+ 设备页面(`device/[peerId].tsx` 详情 + PolicyEditor BottomSheet、`device-card.tsx`、`device-info-card.tsx`、`trust-badge.tsx`、`connection-badge.tsx`、`pairing-sheet.tsx`)。

## Design Health Score

| # | 启发式 | 分 | 关键问题 |
|---|---|---|---|
| 1 | 系统状态可见 | 3 | setup 三态清晰、设备详情有在线/连接路径/延迟;但 block/unblock 只弹通用 toast「设备策略已更新」,没说改了谁 |
| 2 | 贴近真实世界 | 2 | Peer ID / NAT / 打洞 / 中继 / 延迟ms / "允许中继自动接收" 对普通消费者是黑话 |
| 3 | 用户控制与自由 | 3 | 取消配对有确认;但 setup 无返回键、dots 不可点、策略改动无 undo |
| 4 | 一致性与标准 | 1 | onboarding 全用 StyleSheet+硬编码 hex,主色 #2563EB≠系统 #3B82F6;badge 混用 yellow 原生调色板;primary 按钮图标白色 vs 文字深墨。全 app 最大一致性灾难 |
| 5 | 错误预防 | 3 | 大小校验、取消配对确认、保存目录可读性探测都做了;但"阻止设备"一击即生效无确认 |
| 6 | 识别优于回忆 | 3 | 信任选项 icon+标题+描述好;但"自动接收/需要确认"两互斥开关需自行推断联动 |
| 7 | 灵活高效 | 3 | 附近/生成/输入三 tab、设备名建议默认、复制 PeerID |
| 8 | 美学与极简 | 2 | PolicyEditor 一屏 ~13 控件;info-card 头像+三格仪表盘;全大写 eyebrow |
| 9 | 错误识别与恢复 | 3 | 配对码无效文案好;但 setup 把原始 error 字符串直接甩给用户 |
| 10 | 帮助与文档 | 2 | NAT/中继/打洞 零说明;PolicyHeadline 白话摘要是唯一亮点 |
| **总分** | | **25/40** | **Acceptable / Needs Work —— 短板集中在一致性(1)与真实世界语言(2)** |

## Anti-Patterns Verdict

**LLM 评估**:onboarding 是"克制但跑偏"(无渐变玻璃,但套了通用 Hexagon 图标 + 放大喊话 34/26px + 模板 dots);设备页有两处硬 slop:① 设置首页 `device-info-card` 的**英雄头像 initials + 三格大数字仪表盘**(`device-info-card.tsx:98-102, 185-203`)——同时踩中 DESIGN.md 明禁的"hero avatars"和"dashboard-stacking",正是产品的两个反面参照(社交头像 + SaaS 仪表盘);② PolicyEditor 里每个 section 上方 `uppercase tracking-wider` 全大写 eyebrow(`[peerId].tsx:548,566`,中文无大小写,纯装饰)。

**确定性扫描**:detector 对 9 个文件返回 `[]`、exit 0——**这是假阴性,不是干净**。Assessment B 复现了检测器内部:`.impeccable/design.json` 把整条 Tailwind/shadcn 色阶(44 个色)登记为"允许色",`colorsClose` 容差 6 内即放行,所以 onboarding 的 `#2563EB`(blue-600, d=0)、`#0F172A`、`#64748B` 等几乎全部命中放行;唯一真正越界的 `#F8FAFC` 又因检测器的颜色字面量识别**只认 CSS `color:` 冒号式、认不出 JSX `prop="#hex"` 和缩进的驼峰 `backgroundColor:`** 而漏掉。结论:RN StyleSheet/JSX 里的硬编码色,这条规则基本半盲,必须靠人工。

**视觉 overlay**:不可用(无已启动模拟器/真机,原生 App 无 localhost URL)。基于源码静态核查。

## Overall Impression

设备页面的**功能骨架扎实**(信任等级、接收策略、连接可见性、配对流都在),问题是**信息密度和黑话超出了普通消费者**,以及和刚修完的传输界面**同一类 token 违规在这里复发**(yellow-500 原生调色板、primary 按钮图标用 `colors.background`)。而 onboarding 是全 app 唯一脱离设计系统的孤岛——硬编码 hex、无暗色、主色跑偏、通用图标。**单一最大机会:把 onboarding 从"硬编码孤岛"改造成"token 合规样板间",顺带在首屏用一句人话价值主张取代技术卖点。**

## What's Working

1. **取消配对确认对话框**(`[peerId].tsx:438-450`):白话讲清后果("取消后需要重新配对…")、destructive 标红、有 testID——"可信门廊"人格的范本。
2. **保存目录可读性探测**(`[peerId].tsx:516-522`):选目录后先 `dir.list()` 试读,失败给"此目录不可读"而非事后 Bad-fd,诚实防错。
3. **PolicyHeadline 白话摘要**(`[peerId].tsx:889-900`)+ 配对 sheet 情境化默认 tab(`pairing-sheet.tsx:111-113`):黑话堆里难得的白话锚点。

## Priority Issues

### [P0] onboarding 三屏硬编码 hex + StyleSheet:暗色模式全废 + 品牌色跑偏
- **What**:welcome/device-name/setup 全部 `StyleSheet.create` + 写死 hex(12 个去重色值),0 个 `className`、0 个 `dark:`、0 个 token 读取。bg `#F8FAFC`、文字 `#0F172A`/`#475569`、主色按钮 `#2563EB`、disabled `#94A3B8`。
- **Why it matters**:① **暗色模式彻底失效**——bg/文字写死为亮色常量,dark OS 下 onboarding 仍刺眼白底黑字(app 其余靠 token 自动适配),对 Sam 是致盲级;② 主色 `#2563EB`(blue-600)≠ 系统 primary `#3B82F6`(trust-blue),首屏品牌色就暗一档;③ 与全 app className+token 写法割裂,三份 StyleSheet(screen/primaryButton/dots)还逐字重复三遍。(注:white-on-#2563EB≈5.17:1 过 AA,所以是 token/暗色/一致性三重灾难,不是对比度 bug。)
- **Fix**:三屏改写为 NativeWind className + token(`bg-background`/`text-foreground`/`text-muted-foreground`/`bg-primary`/`text-primary-foreground`/`disabled:bg-muted`),图标色走 `useThemeColors()`,主色统一 `#3B82F6`;抽共享 `<OnboardingScreen>`/`<Dots>` 消重。
- **Suggested command**:`/impeccable adapt`(暗色/token 化)→ `/impeccable polish`

### [P1] 设置首页设备卡:英雄头像 + 三格仪表盘,双违 DESIGN.md
- **What**:`device-info-card.tsx:98-102` initials 头像(`text-xl font-bold text-primary`)+ `:185-203` "已连节点/配对设备/NAT" 三格大数字行。
- **Why it matters**:DESIGN.md Don't 清单直接禁止"hero avatars"和"dashboard-stacking";头像还把 primary-blue 当装饰(违反"色彩只用于状态")。产品北极星要躲的两个反面参照同时出现在设置首屏。
- **Fix**:头像换中性平台图标 chip(`bg-muted` + `devicePlatformIcon`,与 `device-card.tsx:53-55` 一致,去蓝色装饰);三格指标降权——并进文本行或只留"配对设备",NAT/已连节点收进网络详情,数字别用 `text-base font-bold` 放大。
- **Suggested command**:`/impeccable distill`

### [P1] PolicyEditor 认知过载 + 互斥开关反模式
- **What**:`[peerId].tsx` 策略 sheet 一屏同时 ~13-14 个可交互控件(4 信任 + 4 开关 + 大小 + 有效期 + 保存位置 + footer 3 按钮)。其中 `:570-598` "自动接收/需要确认"是**两个互斥布尔开关表达三选一语义**,联动藏在 onCheckedChange 里(开一个静默关另一个)。
- **Why it matters**:违反"掌控感来自可见性而非管理复杂度"——此处掌控感恰恰来自**看不见的**联动;用户开"自动接收",眼看"需要确认"被静默关却不知为何。对屏幕阅读器用户尤其糊。
- **Fix**:"接收方式"改单个三选一分段控件(自动接收/需要确认/仅手动);"允许中继自动接收/最大大小/有效期"折进默认收起的"高级"分区。
- **Suggested command**:`/impeccable distill` → `/impeccable clarify`

### [P2] 阻止设备无确认 + 反馈不诚实
- **What**:`[peerId].tsx:179-181` handleBlock 一击即阻,无确认;成功只弹通用「设备策略已更新」(`:164`)。
- **Why it matters**:与"取消配对"的确认级别严重不对称;阻止是敏感信任动作,零摩擦易误触(Casey 口袋误触即断信任),且无针对性反馈。
- **Fix**:给"阻止设备"接 ConfirmDialog(复用 `:438` 模式),文案说明后果+可解除;block/unblock 给专属 toast「已阻止 {name}」/「已解除阻止」。
- **Suggested command**:`/impeccable harden`

### [P2] token 违规复发(与传输界面同一类):badge yellow + 按钮图标 colors.background
- **What**:① `trust-badge.tsx:61-64` temporary 用 `bg-yellow-500/15 text-yellow-600 dark:text-yellow-400`;`connection-badge.tsx:36-37` relay 同样 yellow,且图标硬编码 `#22c55e`/`#3b82f6`/`#f59e0b`(`:21/28/35`)。② primary 实色按钮内图标/spinner 用 `colors.background`(亮色=白,on `#3B82F6`≈2.6:1),而同按钮文字用 `text-primary-foreground` 深墨——`[peerId].tsx:389,750,752`、`pairing-sheet.tsx:324,363`。`device-card.tsx:115/195` 却用对了 `colors.primaryForeground`。
- **Why it matters**:和我刚修完的传输界面(StatusBadge yellow、spinner colors.background)是**同一类 bug**,说明是系统性的。white-on-primary 恰是 Unified Ink Rule 当初判定不达标而废弃的配色;connection-badge 图标绿(`#22c55e` 暗档)还与文字绿(`#16A34A` 亮档)不一致。
- **Fix**:temporary/relay 改 `bg-warning/15 text-warning`;connection-badge 图标色改 `colors.success`/`colors.primary`/`colors.warning`;所有 primary 按钮内图标/spinner 统一 `colors.primaryForeground`。
- **Suggested command**:`/impeccable polish`

## Persona Red Flags

**Jordan(首次使用者)**:welcome 首屏无一句"这是干嘛的"人话,直接上"P2P 直连/端到端加密"技术标签(`welcome.tsx:14-16`);品牌图标是陌生 Hexagon,与桌面装的"松鼠橡果盾"对不上,怀疑装错;setup 展示截断的设备 ID 哈希对新手零意义;进 app 后设备详情满屏 Peer ID/NAT/打洞。

**Sam(暗色/对比度/屏幕阅读器)**:onboarding 暗色模式全废(P0);placeholder `#94A3B8` on 白输入框 ≈2.76:1 远低于 4.5:1(`device-name.tsx:87`);进度 dots 仅靠颜色+尺寸传达当前步、无 label、无"第 2 步/共 3 步"文本;primary 按钮白图标 ≈2.6:1;互斥开关不告知"需要确认 已自动关闭"。

**Casey(分心移动用户)**:"阻止设备"一击即生效无确认(`[peerId].tsx:179`)口袋误触即断信任;PolicyEditor 在 72–90% sheet 里 ~13 控件无法快速扫读;互斥开关联动静默改写安全预期。

## Minor Observations

- setup 把原始 `error` 字符串直接渲染给用户(`setup.tsx:75`),失败时会瞬间击穿"友好·安心"人格;且 setup 无返回键无法回退。
- 触控目标 <44:`[peerId].tsx:695-700` 保存位置"选择"按钮(~24px)、`:683-693` 重置(~33px);`pairing-sheet.tsx:348-367` 重新生成/复制(40px 压线);`device-info-card.tsx:129-137,161-166` 编辑名/复制 PeerID(~28px)。
- setup 重试/进入按钮缺 `accessibilityRole`(`setup.tsx:80,90`);device-name 返回键缺 role(`:53`)。
- `device-name.tsx:72` 副标题用硬 `{"\n"}` 手动换行,i18n 下不同语种会错位。
- onboarding 用 `SafeAreaView edges={["top","bottom"]}`,app 其余走 `AppScreen edges={["top"]}`——又一处小割裂。
- device-card row/card 两变体大量重复(在线点、发送按钮块、badge 组合)。
- `[peerId].tsx:317-322` "延迟 {n}ms" 直出裸数字,无"网络质量"人话包装;`device-info-card.tsx:201` NAT 只有 public→映射成功 / 其余→未知 两态,抹平真实 NAT 类型。

## Questions to Consider

1. onboarding 到底要不要"英雄化"?若定位普通消费者的"可信门廊",第一屏该不该收掉大字号+通用图标,换成一句大白话价值主张 + 品牌标?
2. "信任级别"和"接收策略"是正交两维,还是一维的不同粒度?能否让信任级别直接决定接收方式,把策略编辑降级为极少数人才碰的高级项?
3. Peer ID / NAT / 打洞 / 中继 是"给极客的安心感"还是"给普通人的噪音"?可否默认收进"技术详情"折叠区?
4. "阻止"与"取消配对"在用户心智里差别是什么?现在一个零确认、一个重确认——UI 摩擦有没有和可逆性/风险等级匹配?
5. onboarding 是首因效应最强的地方,却是全 app 唯一脱离 token 的区域——是不是应该反过来,把它做成 token 合规度最高的样板间?
