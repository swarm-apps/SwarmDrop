---
target: 传输页面/发送进度/相关弹窗/节点状态弹窗
total_score: 25
p0_count: 0
p1_count: 2
timestamp: 2026-07-02T06-52-46Z
slug: src-app-transfer-sessionid-tsx
---
Method: dual-agent (A: a96030a6a7a173742 · B: a15ea2e0db8c6bd94)

# 传输界面设计评审 — 发送进度页 / 传输列表 / 相关弹窗 / 节点状态弹窗

评审范围:`transfer/[sessionId].tsx`(发送进度/详情页)、`activity.tsx`(实际的传输列表,`transfer/index.tsx` 只是重定向到此)、`transfer/shared.tsx`、`activity-projection-card.tsx`、`transfer-offer-host.tsx`(收到文件弹窗)、`node-control-sheet.tsx`(节点控制弹窗)、`status-pill.tsx`、`ui/confirm-dialog.tsx`。

## Design Health Score

| # | 启发式 | 分 | 关键问题 |
|---|---|---|---|
| 1 | 系统状态可见性 | 4 | 实时 %/速率/进度条/状态徽章/节点 pill+uptime 全覆盖 —— 北极星做得最好 |
| 2 | 系统与现实匹配 | 2 | 传输面友好,但节点弹窗满屏 Peer ID/NAT/LAN Helper 黑话,两套语域割裂 |
| 3 | 用户控制与自由 | 3 | 取消/删除/清空都有确认;但失败发送无"重试",停止节点反而无确认 |
| 4 | 一致性与标准 | 2 | StatusBadge 用原生调色板 vs StatusPill 用语义 token;spinner 颜色 3 处用错;peerId 一处全展开一处截断;3 套操作栏布局 |
| 5 | 错误预防 | 3 | 破坏性操作有确认;接收前 `dir.list()` 探测可读性是真防错;但停止节点无确认 |
| 6 | 识别优于回忆 | 3 | 各屏自包含;完整 peerId 是识别负担但非记忆依赖 |
| 7 | 灵活与高效 | 3 | 卡片直接恢复、完成后打开文件夹;无批量、无重发 |
| 8 | 美学与极简 | 2 | 节点弹窗 11 行表 + activity 4-section 常驻空卡 + 详情页完整 peerId 都在往反面参照走 |
| 9 | 帮助识别/恢复错误 | 2 | 失败态只给"删除";`LocalizedError` 是空实现,把后端原始串直接丢给用户 |
| 10 | 帮助与文档 | 1 | 满屏网络术语零解释、无 tooltip、无帮助入口 |
| **总分** | | **25/40** | **Acceptable —— 能用、局部亮眼,但有真实缺口** |

## Anti-Patterns Verdict

**LLM 评估**:不是"一眼假"的重度 slop —— DESIGN.md 的禁令(无侧边色条、无渐变文字、无玻璃拟态)被遵守。但有**两处直接踩中产品自己写死的反面参照**:① 节点控制弹窗 running 态一次铺 11 行网络诊断表(`node-control-sheet.tsx:160-229`),是"企业 SaaS 数据表格"的教科书形态;② activity 页只要有 1 条记录就永远同时渲染 4 个 section、空组各渲染占位卡(`activity.tsx:95-130`),是"永远在场的 section 脚手架"。另有一处对中文无意义的 eyebrow(`[sessionId].tsx:302` 的 `uppercase tracking-wider` 用在中文"文件"上)。

**确定性扫描**:detector 对 8 个文件返回 `[]`、exit 0(表面"干净"),但这是**规则覆盖盲区,不是真干净**——检测器的 `design-system-color` 规则只匹配字面 hex/rgb,**不识别 Tailwind 原生调色板类名**(`bg-blue-500`/`text-yellow-600`/`orange-500`)。用反例 probe 验证了检测器本身工作正常(能报出 side-tab/gradient-text/design-system-font)。所以最关键的"语义色绕过"违规被检测器 100% 漏报,靠人工核查才抓到(见下 P2)。

**视觉 overlay**:不可用。无已启动的模拟器/真机(`xcrun simctl`/`adb devices` 均空),原生 RN App 无 localhost 网页 URL,无法注入网页版 detect overlay。本次评审基于源码静态核查。

## Overall Impression

这套界面的**骨架是对的**:实时进度、状态徽章、确认弹窗、接收前的可读性探测,都体现出对"状态先于装饰""被动接收优先"的理解。最大的问题是**一致性塌陷和两处越界**——同一个"状态色",StatusPill 用语义 token、StatusBadge 却用原生调色板还凭空多出橙色;同一个"实色按钮上的图标",详情页用对了 token、另外三处用错了;而节点控制弹窗把一个消费级开关做成了 libp2p 诊断台。单一最大机会:**把"节点诊断"从消费级弹窗里收走**,让它回归门廊隐喻的一句诚实状态。

## What's Working

1. **系统状态可见性是全项目最强项**(`[sessionId].tsx:343-362`):实时百分比(`text-3xl` 指标读数,恰好是 DESIGN.md 允许的唯一大字号用法)+ 速率 + 进度条 + 状态徽章,把北极星"通过可见性获得掌控"落到了实处。
2. **接收 offer 的防错闭环 + 门廊式信息架构**(`transfer-offer-host.tsx:88-101, 171-273`):接收前 `dir.list()` 探测 SAF 目录可读性,避免"接收到一半发现存不进去";弹窗结构(谁发的→信任等级→存到哪→文件清单→接收/拒绝)干净回答了门廊隐喻的三个问题,是全项目最贴合北极星的一屏。
3. **确认弹窗文案是"友好房东"人格的正确示范**(`[sessionId].tsx:194`、`activity.tsx:146-149`):不用"警告/错误"吓人,而是解释后果和边界("仅从本机活动记录中删除,不影响对端"),取消项用"继续"而非冷冰冰的"取消"。

## Priority Issues

### [P1] 节点控制弹窗把消费级开关做成了 libp2p 诊断台
- **What**:running 态一次铺 11 行网络诊断表(`node-control-sheet.tsx:160-229`),术语含 Peer ID / NAT 状态 / LAN Helper / 引导节点 / 候选节点 / 中继 / 本机 Helper(B 确认 ~11 处技术黑话)。
- **Why it matters**:同时违反反面参照(SaaS 数据表格)、设计原则 3(亲和优先于黑话)、原则 4(掌控感≠管理复杂度)。弹窗本职只是"启/停节点",诊断把停止按钮挤到屏幕最底,首次用户完全看不懂。
- **Fix**:默认只留 2-3 行消费级信息("在线,已连接 N 台设备""运行时长"),其余 8 行折叠进"网络诊断"渐进披露开关,或整体移到 设置 > 高级/诊断 页。
- **Suggested command**:`/impeccable distill`(剥离复杂度) → `/impeccable clarify`(黑话文案)

### [P1] 实色按钮的图标/spinner 用错颜色 token(对比度回退 + 图文撞色)
- **What**:多处在 Trust-Blue / 红底按钮上用 `color={colors.background}`:`transfer-offer-host.tsx:299`(接收 spinner)、`:301`(下载图标);`activity-projection-card.tsx:119`(恢复图标);`node-control-sheet.tsx:262`(停止 spinner)、`:278`(启动 spinner)。同按钮的文字却用 `text-primary-foreground`(暗墨)。
- **Why it matters**:`--background` 亮色=白、`--primary-foreground`=暗墨(两模式同值)。① 亮色下白图标 on Trust-Blue ≈ 3.5:1,正是 DESIGN.md "Unified Ink Rule" 明确修掉并禁止回退的 sub-AA 配色;② 白图标紧挨暗墨文字,同按钮图文两色,像渲染故障;③ 暗色模式下红底"停止"按钮的近黑 spinner 几乎不可见。详情页 `[sessionId].tsx:600` 用 `iconColor`(随 variant 走 primaryForeground)是**正确范例**,反衬出这三处的错。
- **Fix**:primary 按钮图标/spinner 一律 `colors.primaryForeground`;destructive 按钮用白色/`destructiveForeground`。
- **Suggested command**:`/impeccable polish`

### [P2] StatusBadge 绕过语义 token,引入设计系统里根本不存在的橙色
- **What**:`shared.tsx:59-107`:transferring=`blue-500`、paused/waiting/offered=`yellow-500/600`、interrupted/peer_offline/app_restarted=`orange-500/600`。而 `status-pill.tsx:62-81` 同样是"状态"却规规矩矩用 `success/warning/destructive/muted` token。
- **Why it matters**:DESIGN.md 只批准 4 个语义色。`yellow-500`(#eab308)≠ warning/caution-amber(#F59E0B),是另一色相;**`orange` 在整个设计系统里没有任何对应,凭空多出第 5-6 个色**;blue-500 无 dark 变体、paused 却写了 dark 变体——同组件内深色处理都不一致。这就是 DESIGN.md 明禁的"第二套色"。检测器漏报,人工核查确认。
- **Fix**:transferring→`primary`,paused/waiting/offered/interrupted 家族→`warning`,删除所有 `orange-*`;让 StatusBadge 与 StatusPill 共用同一套 token 映射。
- **Suggested command**:`/impeccable polish`(token 收敛)

### [P2] 详情页 ActionBar 等宽平铺,破坏性操作没被降权
- **What**:`[sessionId].tsx:448-519`,`flex-row flex-wrap` + 每个按钮 `flex-1`(597)。[暂停/取消] 各占 50% 红色破坏性"取消"与"暂停"等权;终态只剩"删除"时 `flex-1` 让一个红色破坏性动作撑满整行变 hero;长标签 + 多按钮时 `flex-wrap` 不可预测换行。
- **Why it matters**:主/次/破坏三级层级被压平,分心用户容易误点破坏性动作。
- **Fix**:主 CTA(恢复/打开文件夹)占主导;删除/取消降为 ghost 或纯文字、固定较窄宽度;禁止单个破坏性动作 `flex-1` 撑满全宽。
- **Suggested command**:`/impeccable layout`

### [P2] 失败态无恢复出路 + 错误文案是空壳
- **What**:失败终态操作栏只有"删除"(`[sessionId].tsx:504-516`);`canResend`(`shared.tsx:300-302`)定义了却没接进 ActionBar;`LocalizedError`(`shared.tsx:247-254`)只是 `<Text>{message}</Text>`,名不副实。
- **Why it matters**:情感低谷时把用户逼进死胡同,还可能展示未翻译的技术原始报错——违反启发式 9 和"友好房东"人格。
- **Fix**:失败/中断的发送接入"重新发送"(用 `canResend`);把常见错误码映射成友好中文,让 `LocalizedError` 名副其实。
- **Suggested command**:`/impeccable harden` → `/impeccable clarify`

## Persona Red Flags

**Jordan(首次使用者)**
- 点开"节点控制"想找一个开关,迎面 Peer ID / NAT 状态 / LAN Helper / 引导节点 / 候选节点 / 中继 / 本机 Helper 七八个术语,零解释(`node-control-sheet.tsx:160-229`)。
- activity 页看到"完成诊断"section + 副标题"…诊断记录"(`activity.tsx:92, 123`)——"我一个传文件的 App 为什么要诊断?"
- 详情页"对端"直接铺完整 base58 peerId(`[sessionId].tsx:285`,numberOfLines=3)——一串看不懂的乱码。

**Casey(分心的移动用户)**
- ActionBar 里"暂停"和红色"取消"同宽同重(`[sessionId].tsx:451-475`),扫一眼容易点错破坏性动作。
- 接收弹窗亮色下白色下载图标 + 暗墨"接收"文字(`transfer-offer-host.tsx:301/303`)、恢复卡白图标 + 暗墨文字(`activity-projection-card.tsx:119/120`),一瞥像坏图。
- activity 页要在 3 张空占位卡里找那 1 条真记录(`activity.tsx:194-200`)。

**Riley(边界压力测试者)**
- 发送失败 → 详情页只有"删除",没有重发,只能从头再来(`canResend` 未接)。
- 触发后端错误 → 看到未翻译原始串(`LocalizedError` 空实现)。
- 单击"停止节点"无二次确认即断所有连接(`node-control-sheet.tsx:126-134, 253-268`),而破坏性更小的"取消传输"却要确认——安全网前后矛盾。
- 暗色模式点"停止节点"→ spinner 近黑 on 深红,几乎看不见(`node-control-sheet.tsx:262`)。

## Minor Observations

- `formatBytes` 在 `transfer-offer-host.tsx:353-359` 与 `shared.tsx:189-195` 重复两份(该文件未 import shared 版),易漂移——删局部版改 import。
- 完成态三等分 Stat(文件/总大小/**用时**,`[sessionId].tsx:385-395`)是较轻的 hero-metric 擦边;"用时"把一次私密传输悄悄变成跑分,与"用户更在意信任而非速度"错位(非禁令级,18px 非 30px)。
- "更改保存位置"按钮 `px-2.5 py-1.5` 无 min-height,估算 ~30px < 44pt 触控建议(`transfer-offer-host.tsx:243-253`)。
- section 命名"完成诊断/需要注意"+ 副标题"诊断记录"偏技术,建议软化(如"已完成/需处理")。
- 详情页 peerId 全展开 vs node-sheet `truncateMiddle`——同一 ID 两种展示。
- 停止节点无二次确认,而破坏性更小的取消传输要确认——安全网不一致。
- node-sheet 停止/启动按钮 busy 时切成纯 spinner(无 text/label),加载态短暂丢失无障碍名(`:262/:278`)。

## Questions to Consider

1. 节点的职责只是"可被找到",用户真的需要看见 11 个 libp2p 指标吗?一行"在线,已连接 3 台设备"+ 可选诊断页,是不是更贴合门廊隐喻?
2. 完成页的"用时/总大小"是在庆祝用户的结果,还是把一次私密传输变成跑分?一个强化"信任"而非"速度"的峰终时刻会长什么样?
3. 传输失败时唯一的门是"删除"。"友好房东"面对失败的正确反应是什么?为什么 `canResend` 存在于代码却不出现在任何界面?
4. 为什么应用里有两套状态色系统(token 化的 StatusPill vs 原生调色板的 StatusBadge)和三套操作栏布局?一个什么样的统一组件契约能把它们收敛成一个?
5. activity 页永远展示 4 个 section(哪怕 3 个是空的)——这个脚手架服务的是"用户想找到某次传输",还是开发者对状态机的心智模型?
