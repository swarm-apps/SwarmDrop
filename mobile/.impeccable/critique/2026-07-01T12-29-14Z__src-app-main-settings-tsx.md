---
target: 设置(Settings Tab + 子页面)
total_score: 24
p0_count: 0
p1_count: 2
timestamp: 2026-07-01T12-29-14Z
slug: src-app-main-settings-tsx
---
Method: dual-agent (A: a0c7cdc4214a860b5 · B: a9cd0c37cd7eaa576)

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 3 | about.tsx 用静态不旋转的 Loader 图标表示"正在检查更新",看起来像卡死 |
| 2 | Match System / Real World | 2 | network.tsx 把六七个协议原生指标(NAT/候选节点/LAN Helper等)平铺给所有用户,无类比说明 |
| 3 | User Control and Freedom | 3 | 编辑设备名无显式取消按钮,只能靠 onBlur 或手动改回退出编辑态 |
| 4 | Consistency and Standards | 2 | settings.tsx 主列表用 rounded-lg,几乎所有子页卡片却用 DESIGN.md 未定义的 rounded-xl |
| 5 | Error Prevention | 2 | 自定义引导节点校验过松、删除零确认;而低风险的"恢复默认接收位置"却套了红色警告 |
| 6 | Recognition Rather Than Recall | 3 | 语言/主题选择有 Check 图标高亮,但 network.tsx 的发现方式选中态缺少同等的图标信号 |
| 7 | Flexibility and Efficiency of Use | 2 | 高频的"暂停接收"埋在 设置→通用→传输 第二行,需两次导航跳转 |
| 8 | Aesthetic and Minimalist Design | 3 | 整体紧扣设计系统纪律,但 NetworkHint 硬编码裸十六进制警示色,绕过 token |
| 9 | Help Users Recognize/Diagnose/Recover from Errors | 3 | 多数失败路径有 toast 兜底,但语言/主题偏好读取失败被静默吞掉 |
| 10 | Help and Documentation | 1 | 五个子页无任何上下文内帮助,NAT/候选节点/LAN Helper/multiaddr 全靠自行理解 |
| **Total** | | **24/40** | **Acceptable —需要显著改进才能让用户满意** |

## Anti-Patterns Verdict

**LLM 评估**:Settings 首页+5个子页确实遵守 DESIGN.md 纪律——单一 Threshold Line 边框灰、紧凑字号、icon-chip 复用、几乎处处 shadow-sm 的近乎扁平。前3屏(主页/通用/语言/外观)会让懂行的人感到舒服。但 Network 和 Bootstrap Nodes 两屏信任感打折扣:协议级指标平铺成类似仪表盘的数据表(直接撞上 PRODUCT.md 反面参考);用静态图标表示"进行中"是典型的"没做完"手感,与同 App 内其他地方正确使用 ActivityIndicator 形成鲜明对比;NetworkHint 硬编码裸十六进制警示色绕过 token 体系;圆角在 Settings 家族内分裂成 rounded-lg 和 rounded-xl 两套(DESIGN.md 根本没有 xl 这一级)。

**确定性扫描**:对 settings.tsx + general/language/theme/network/about/bootstrap-nodes.tsx 共 7 个文件扫描,exit code 0,0 命中。已用一个人工构造的含 border-l-4/text-purple-600/animate-bounce 的 sanity-test.tsx 文件验证同一 CLI 路径能正确产出 5 条 finding 并以 exit code 2 退出,证明工具本身工作正常、0 命中是真实结果而非静默失败——但同样受限于规则库覆盖范围,无法检测本次发现的字号/圆角/风险信号错位等问题。

**可视化取证**:未执行。已确认无 dev server、无 booted 模拟器/真机,fallback signal。

## Overall Impression

这是一套认真执行了设计系统大方向、但恰恰在"状态先于装饰"和"不做数据表格"两条最核心原则上,在最需要克制的 Network 页出现偏离的界面。DeviceInfoCard 是把"控制感来自可见性"落地得最好的组件。最大机会点:把 Network 页的协议级指标折叠进"查看诊断详情"的二级入口,只给普通用户一句合成状态。

## What's Working

- **SettingSection/SettingRow/SettingDivider 三件套的高度复用**:在 5 个子页里保持单一边框、统一 icon-chip 尺寸,让整个 Settings IA 读起来像一套真正的系统。
- **DeviceInfoCard**:在线圆点+平台角标+行内可编辑设备名+可复制 PeerID+三列指标 footer 全部挤进一张卡片却不显堆叠,编辑体验直接、touch-first。
- **PauseReceivingRow 的防御式降级**:旧原生绑定不支持时整行直接隐藏而非渲染会报错的开关,体现了对边界情况的认真考虑。

## Priority Issues

**[P1] Network 页把六七项协议指标无差别平铺给所有用户**
- Why it matters:直接落入 PRODUCT.md 反面参考"企业SaaS后台数据表格堆砌",违反"亲和优先于专业黑话"——普通用户打开网络设置只想确认"能不能正常收发文件",却看到一整屏无解释的网络工程指标。
- Fix:默认只展示一句合成状态(如"网络状况:良好/受限"),把 NAT/候选节点/LAN Helper/中继等指标收进"查看诊断详情"的可展开二级入口。
- Suggested command: `/impeccable distill`

**[P1] "检查更新"用静态图标而非旋转动画表示进行中**
- Why it matters:用户无法分辨这是"正在工作"还是"卡死",尤其弱网场景下检查耗时较长时会持续给出错误信号,与同 App 内其他地方正确使用 ActivityIndicator 的重启按钮形成不一致。
- Fix:替换为 `<ActivityIndicator size="small" .../>` 或给 Loader 套持续旋转动画。
- Suggested command: `/impeccable polish`

**[P2] 风险信号错位:低风险操作有红色警告,高风险操作零确认**
- Why it matters:"恢复默认接收位置"(完全可逆)被红色 ConfirmDialog 拦下,而"删除自定义引导节点"(可能影响实际联网能力)零确认零反馈——这种错位会让用户对"红色=危险"的信任逐渐失效。
- Fix:把接收位置重置改为中性样式;给引导节点删除加轻量确认或至少一条完成提示 toast。
- Suggested command: `/impeccable harden`

**[P2] NetworkHint 硬编码裸十六进制警示色,绕过语义 token**
- Why it matters:DESIGN.md 要求状态色在深色模式下有独立调整值,硬编码绕过后深色模式下会和同屏正确使用 token 的横幅呈现两种不同的琥珀色,是隐性的"第二种警示色"违规。
- Fix:把 `color="#f59e0b"` 改为 `color={colors.warning}`,与同文件其他用法保持一致。
- Suggested command: `/impeccable harden`

**[P3] Settings 家族内圆角体系分裂(rounded-lg vs rounded-xl)**
- Why it matters:从主页进入任意子页会出现一次未被文档承认、也无视觉必要性的圆角跳变,稀释了"克制统一"的系统感。
- Fix:统一改为 DESIGN.md 已文档化的 rounded-lg,或正式把 xl 补进 DESIGN.md 作为卡片容器的官方 token。
- Suggested command: `/impeccable audit`

## Persona Red Flags

**Jordan(困惑新手)**:"候选节点:3"不知道是好是坏;"NAT:映射成功/未知"出现在设置首页第一屏就能看到的卡片里;"自定义引导节点"整套概念没有任何说明它是什么、什么时候需要用。

**Sam(屏幕阅读器/对比度)**:"添加"按钮同样受已知的对比度缺陷影响;多个纯图标控件(恢复默认/删除)触控盒偏小;发现方式选中态只靠边框色+背景色+小圆点区分,色弱用户难以确认当前选中项。

**Riley(边界压力测试)**:自定义节点地址校验过松,"/p2p/"这种残缺地址也会被判合法存入;设备名清空后失焦会被无声丢弃保存,没有任何提示;断网环境下触发检查更新会看到长时间不动的图标,无法区分"正在检查"还是"卡死"。

**Min(项目专属·格外看重隐私的普通消费者)**:全设置页唯一提到"端到端加密"的地方只是 about.tsx 里一句营销口吻的静态文字,没有可点入的信任说明;"本机 LAN Helper"开关文案只从电量角度提醒,完全没回答"开启后我的文件流量会不会经过我的手机"这个真正的隐私问题;修改设备名时没有提示这个名字会广播给附近设备可见。

## Minor Observations

- `handleSaveName` 同时挂在 onBlur 和 onSubmitEditing,某些 RN 版本上可能触发两次异步保存,存在竞态风险。
- "检查更新"按钮在 iOS 上整体隐藏,且没有一句说明"iOS 版本通过 App Store 更新"。
- 候选来源 chips 在无候选来源时完全消失,没有占位说明。
- 设置主列表的 NavRow 只有 icon+label+chevron,没有像标准系统设置那样在标签下加一行摘要。

## Questions to Consider

- 如果把 Network 页那六七项协议级指标全部砍掉,只留一句"网络状况:良好/受限"和一个可选的"查看技术详情"入口,核心用户画像里真的会有多少人察觉"功能被削弱",而不是觉得这个 App 突然变轻松了?
- "暂停接收"是和"被动接收优先"这条核心原则直接绑定的高频开关,为什么要埋在 设置→通用→传输 第二行,而不是让用户在离主流程更近的地方一步触达?
- 如果删除一个可能实质影响联网能力的引导节点不需要确认,而重置一个纯本地文件夹偏好却弹出红色警告——下一次真正需要用户郑重对待的红色按钮(比如拉黑某台设备)出现时,他们还会认真看一眼吗?
