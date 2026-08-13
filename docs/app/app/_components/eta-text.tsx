"use client";

// 活跃传输的第四个信息位：**还要多久**。
//
// 它单独成一个组件，是因为「算不出来的时候说什么」必须只有一个答案：三个紧凑表面
// （设备页的活跃区、传输列表行、发送页的「已发出」卡）都要摆这一格，各写一遍迟早会变成
// 一处「—」、一处空白、一处「未知」。
//
// **这一格永远不消失**（DESIGN.md 的 Transfer Progress Contract）：`formatEta` 算不出时返回
// `null`（占位文案要翻译，归调用点给），而它恰好在传输停滞时算不出来——那正是用户最需要
// 界面说点什么的时刻，让整格塌掉读起来像布局坏了。
//
// **只管「说什么」，不管「什么时候说」**：契约规定这一格只在会话 `active` 时出现（暂停与
// 等待接受的会话没有速率，摆一个剩余时间是在报告一段并不存在的等待），那个判据留在调用点
// ——它们各自知道自己那条会话的阶段。
//
// 同理，**「这个数还算不算数」也不在这里判**：一帧进度躺久了它的剩余时间就作废了，那条
// 时效判据住在 `_lib/use-usable-rates.ts`（连同「陈旧那一刻怎么触发重算」）。调用点把
// 过完时效的值传进来，算不出就是 `null`，本组件照样给占位。

import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { formatEta } from "@swarmdrop/shared-view";

/**
 * 算不出剩余时间时说的那句话。
 *
 * 导出它是因为**不是每个表面都用得上这个组件**：详情侧的指标区是 label/value 版式，
 * 值那一格要的是一个字符串（还要据此决定套不套 `font-mono`），塞不进一个 JSX 组件——
 * 于是它此前自己又写了一遍源串。msgId 相同所以译文不会分家，风险全在源串：改一处措辞
 * 就会分叉成「计算中」与别的什么。
 *
 * 存的是 `msg` 描述符而不是展开好的字符串——本仓那条「翻译宏只在组件里展开」：
 * 模块顶层求值发生在 locale 切换之前，展开出来的会是一个永远不变的中文。
 */
export const ETA_PENDING = msg`计算中`;

export function EtaText({ seconds }: { seconds: number | null | undefined }) {
  const { t } = useLingui();
  const eta = formatEta(seconds);
  if (eta === null) return <>{t(ETA_PENDING)}</>;
  // ⚠️ `String(eta)` 不是多余的转换，删掉会静默改掉 msgId。
  //
  // Lingui 对插值 `${eta}`（裸标识符）生成**具名**占位 `剩余 {eta}`，对调用表达式才生成
  // **位置**占位 `剩余 {0}`。而契约钉死的那条是 `剩余 {0}`，桌面那份也已经是它
  // （来自 `t\`剩余 ${formatDuration(progress.eta)}\``）。两者是两条不同的 msgId，
  // 一旦分家，四份 catalog 里就会各存一半、各翻各的。
  return <>{t`剩余 ${String(eta)}`}</>;
}
