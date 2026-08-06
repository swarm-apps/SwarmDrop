import type { Metadata } from "next";
import { AboutPanel } from "../_components/about-panel";
import { AppearancePanel } from "../_components/appearance-panel";
import { ConnectionPanel } from "../_components/connection-panel";
import { NodePanel } from "../_components/node-panel";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.settings) };

// 低频配置区（#94）。**版式与桌面 `src/routes/_app/settings/index.lazy.tsx` 对齐**：
// bento 不规则卡片网格 + `SettingsSection / SettingsCard / SettingsRow` 三级基元
// （见 `_components/settings-primitives.tsx` 的头注释，那里写了为什么设置页不复用
// 应用区其余页面的 `SectionShell`）。
//
// ## 三档响应式，与桌面同一组断点
//
//   <768px   单列
//   ≥768px   两列（`md:grid-cols-2`，满宽卡跨两列）
//   ≥1024px  六列（`lg:grid-cols-6`，靠 col-span 拼出不规则行）
//
// 限宽跟全站走 `PageShell` 的 1240（2026-08-06 起）。此前这里单独用 1040（取自桌面），
// 但三档不同的 `max-w` 都居中，结果是内容左缘随路由跳——本页当时比设备/收件箱/传输页
// 右移 83px。理由与取舍见 `page-shell.tsx` 的 `COLUMN`。bento 的三档栅格不受影响：
// 它按 `md:` / `lg:` 分列，1240 只是让 `lg:grid-cols-6` 那档每格宽一点。
//
// ## 行的划分与顺序：与桌面逐行对应
//
//   row1  本机节点（满宽英雄）  ← 桌面 row1 设备信息
//   row2  关于（满宽）          ← 桌面 row2 关于，置顶展示产品定位
//   row3  外观 ｜ 连接          ← 桌面 row3 外观 ｜ 通用+传输 ｜ 网络
//
// 「连接」对应桌面的「网络」，同样排在偏好行的右侧。
//
// **这一行用 `items-start` 而不是桌面的 `items-stretch`**，是唯一一处刻意分叉。桌面那条
// 「等高」成立是有前提的：它的中列**竖叠了两块**（通用 + 传输）来把高度配平，所以拉伸只补
// 最后几十像素。Web 只有四块，偏好行左边就一块「外观」，右边的「连接」高出一截（中继清单 +
// 添加中继 + 可达地址）——拉伸会在「外观」卡底部留出约 110px 的空玻璃，占这张卡的三成，
// 看起来像还没写完，而不是「底部由玻璃底色延伸」。配平的前提没有，就不该照抄结论。
//
// 此前这里是「本机身份 → 连接 → 外观 → 关于」（#94），理由是 Web 的「连接」是功能前提
// 而非介绍性内容。改成与桌面一致是**明确要求**，两条路由的用户看到的应当是同一个产品；
// 「连接」并没有被藏起来——它仍在首屏之内，只是不再压在「关于」之前。
//
// ## 事件流面板已删除（2026-08-05）
//
// `DevEventLog` 原先挂在这一页末尾。它只在开发构建里渲染（生产被 tree-shake），
// 但那正是问题：桌面设置页没有对应物，而开发时它又长在真·用户设置的下面。
// 排查事件流用浏览器控制台或 store 快照，不需要一块常驻 UI。
export default function SettingsPage() {
  return (
    <PageShell>
      <PageHeader nav="settings" />

      {/* 卡与卡之间用 `--space-in-panel`(16px)，不是 `--space-section`(32px)：
          它们同属「设置」这一个区块，32 是**区块与区块**之间的距离（页头 ↔ 网格用的就是它）。
          同一层级关系两个数会让 Gestalt 分组失效——四档刻度见 global.css。 */}
      <div className="grid grid-cols-1 items-start gap-[var(--space-in-panel)] md:grid-cols-2 lg:grid-cols-6">
        <div className="md:col-span-2 lg:col-span-6">
          <NodePanel />
        </div>
        <div className="md:col-span-2 lg:col-span-6">
          <AboutPanel />
        </div>
        <div className="md:col-span-1 lg:col-span-3">
          <AppearancePanel />
        </div>
        <div className="md:col-span-1 lg:col-span-3">
          <ConnectionPanel />
        </div>
      </div>
    </PageShell>
  );
}
