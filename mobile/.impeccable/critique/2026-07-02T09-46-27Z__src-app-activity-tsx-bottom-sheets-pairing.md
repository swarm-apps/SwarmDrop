---
target: 活动页面 + 所有 bottom sheet + 配对弹窗
total_score: 28
p0_count: 0
p1_count: 1
timestamp: 2026-07-02T09-46-27Z
slug: src-app-activity-tsx-bottom-sheets-pairing
---
Method: dual-agent (A: aaaa04bca6ffdbd5a · B: a8ee1d54bd7c9bd39)

# 活动页 + 全部 Bottom Sheet + 配对弹窗 — 设计评审

## Design Health Score

| # | 启发式 | 分 | 关键问题 |
|---|--------|----|---------|
| 1 | 系统状态可见性 | 3 | 状态 pill/运行时长/倒计时/进度齐全;但 node-control 停止态把"发生了什么"埋在 62% 空场,活动卡终态进度条误导"仍在传输" |
| 2 | 匹配现实世界 | 3 | 文案友好;libp2p 黑话靠"网络诊断"渐进披露挡住,方向对 |
| 3 | 用户控制与自由 | 3 | pan-down-close/取消/解除阻止/恢复齐全;但固定 snapPoint 逼用户手动拖 handle |
| 4 | 一致性与标准 | 2 | 重灾区:sheet 高度 2 动态/3 固定;破坏性确认 Alert vs ConfirmDialog 分裂;pairing-sheet 死代码 |
| 5 | 错误预防 | 3 | 破坏性动作皆有二次确认;策略大小上限有输入校验并禁用保存 |
| 6 | 识别而非回忆 | 3 | 图标+标签+内联释义到位 |
| 7 | 灵活与效率 | 3 | 复制路径/恢复/更多操作 sheet/OTP 自动聚焦 |
| 8 | 美学与极简 | 2 | 留白/密度两头失衡:固定 sheet 撑空场;PolicySheet 信息密度陡峭;活动卡终态多一根装饰进度条 |
| 9 | 错误识别/诊断/恢复 | 3 | inbox"可重试异常 vs 确认不存在"分流标杆级 |
| 10 | 帮助与文档 | 3 | 工具类靠内联提示足矣 |
| **Total** | | **28/40** | **Good(中上段)** |

## Anti-Patterns Verdict

**整体不 slop。** 检测器对 7 个文件零命中(色彩/token 全合规、无硬编码 hex、无按钮前景色误用),这是前两轮整改的成果——本 App 残留的问题是**结构性**的,不是装饰性的。

两处"形式大于内容"的 AI 味残留,恰都出在被点名的固定高度 sheet 上:
- node-control 停止态只有「1 图标 + 1 行提示 + 2 按钮」却被 62% 屏高撑开,中间死白 → 像"没加载完/坏了"。
- 活动卡终态仍画进度条,违背"状态先于装饰"。

## Priority Issues

### P1 — Sheet 高度策略不一致 + 固定百分比留白(用户核心痛点)
5 个 sheet:2 个(InboxActions/PairingCode)已 `enableDynamicSizing` 内容自适应,3 个(node-control `["62%","88%"]`、PolicySheet `["72%","90%"]`、死掉的 pairing-sheet `["58%","84%"]`)写死百分比 + `enableDynamicSizing={false}`。node-control 停止态占 62% 屏高全是空白;诊断展开又超 62% 要手拖到 88%。
- **Why**:直接砸 Nielsen #4/#8,制造"是不是坏了"的错误信号。context7 确认 v5 正解:`enableDynamicSizing`(默认 true)自动按内容测高生成 snap point,`maxDynamicContentSize` 封顶。
- **Fix**:node-control + PolicySheet 改 `enableDynamicSizing`,删固定 snapPoints(PolicySheet 因内容可能长+带 footer,保留 `maxDynamicContentSize`≈90% 屏高 + BottomSheetScrollView 溢出滚动 + footer 用 bottomInset 贴底)。与 InboxActionsSheet 基线对齐。

### P2 — 活动卡对终态记录仍渲染进度条(状态先于装饰被违反)
`activity-projection-card.tsx:79-84` 无条件画 ProgressBar + "已传/总量"行。已完成组永远满条,已取消/已拒绝组半截条(会误读成"暂停中")。
- **Fix**:仅 active/recoverable 组显示进度条;终态组用一行状态摘要("已于 X 完成 · N 文件 · 大小")替代。

### P2 — 破坏性二次确认模式不统一
停止节点用原生 `Alert.alert`(node-control-sheet.tsx:143),而阻止设备/取消配对/清空活动都用 in-app `ConfirmDialog`。最危险的动作反而套了最通用陌生的系统弹窗,与"可信门廊"温暖人格断裂;且停止警告文案在 caption(L297-299)与 Alert(L143-146)一字重复两遍。
- **Fix**:停止节点改 `ConfirmDialog`(destructive),文案去重。

### P3 — 死代码 pairing-sheet.tsx
3-tab(附近/生成/输入)完整配对 sheet,全仓无 import(唯一引用是 store 注释)。当前线上配对是"生成=首页内联卡 + 输入=PairingCodeSheet"拆分方案。死代码含一个线上没有的"附近设备"tab,会诱导后人误接一套已分叉的设计,且又是一处固定百分比坏样例。
- **Fix**:删除;若"附近一键配对"是想要的能力,单独立项接入。

### P3 — 活动页副标题漏一个分组
`activity.tsx:91-93` 副标题"查看进行中、可恢复,以及已完成的传输"只列 3 组,实际 4 组(缺"需要注意",而它恰是最该预告的)。
- **Fix**:补齐或改一句概括。

## Persona Red Flags

- **Casey(单手移动)**:打开 node-control 想看诊断,sheet 停 62%,9 行诊断被裁,须单手够顶部小 handle 拖到 88%;而下半屏可达区是一片点不了的灰白——可达性最好的区域被浪费,信息却够不着。
- **Jordan(首次)**:节点未启动点开 node-control,迎面 62% 高、中间大空白、一句话两按钮,空得像没加载完;配对心智里"生成=内联卡/输入=弹层"入口形态不一致。
- **Riley(压力测试)**:一眼看出停止节点弹系统原生 Alert、阻止设备弹 App 自有 ConfirmDialog——同为最高危动作,"停止节点"确认反而更轻更陌生,危险分级信号传反。
- **隐私极客(项目专属)**:网络诊断是给他的糖,但要拖到 88% 才看全;node-control 里 Peer ID 被 truncateMiddle 截断且不可复制,而 device 详情页显示完整——同一实体两种呈现且都点不了复制,"复制自己 Peer ID"对 P2P App 是刚需却缺失。

## Minor Observations

- 分组顺序 active→recoverable→attention→completed:"需要注意"(出错)排在"可恢复"之后,按"可见性优先"应更靠前。
- PolicySheet 选信任级别会静默改写下游策略默认(policyWithTrustDefaults),用户改完接收方式再切级别可能被悄悄覆盖——切级别时给轻提示。
- pairing-sheet.tsx:351/361 的"重新生成"/"复制"按钮 h-10(40px)无 min-h,<44pt 触控目标(该文件若删除则自然消除)。
- 两个动态 sheet(InboxActions/PairingCode)均未设 maxDynamicContentSize,内容极端长时会顶到屏顶——可顺手补上限。
- InboxActionsSheet 是本仓最佳 sheet 范例(动态高度+分组+破坏项独立成组+destructive 分隔线),建议作为统一基线模板。

## Questions to Consider

1. 三个 sheet 写死百分比,是为了绕开动态高度下的某个真实缺陷(键盘遮挡?footer 抖动?初始高度跳动?),还是"当时先这么写了"?若前者应修根因而非全局退回固定档。
2. "生成配对码=内联卡 / 输入=弹层"的分裂入口,真正要的是"重做统一入口"(即死掉的 pairing-sheet 想做的事)还是"删掉幻影"?
3. 网络诊断里,隐私极客真正会据以**行动**的字段有几个?若多为"安心装饰",它与反面参照"SaaS 仪表盘堆砌"只隔一层折叠。
4. 破坏性动作确认外壳想传达的"危险梯度"是什么?当前 Alert vs ConfirmDialog 的分裂是否正好把梯度传反了?
