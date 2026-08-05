import type { Metadata } from "next";
import { ActiveTransfersSection } from "../_components/active-transfers-section";
import { DevicesSection } from "../_components/devices-section";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.devices) };

// 应用首页（`/app` 重定向到这里）：设备关系页，承接桌面端 `_app/devices` 的角色——
// 先有已配对设备，发送、收件、传输才谈得上。
//
// ## 版式：设备 → 配对 → 活跃传输
//
// 按**使用频率**排，不按功能亲缘：设备清单每次进来都要看；活跃传输是「现在正在发生什么」，
// 有内容时最该被看见；配对是一次性动作（配完即长期信任），使用频率比前两者低两个数量级。
//
// **配对块因此默认收起**，并且它的入口移到了设备网格末尾那张「添加设备」卡片
// （`devices-section.tsx`）——入口长在它要改变的那个列表旁边，比一块常驻的表单更好找。
// ⚠️ 它仍然是三个 `SectionShell` 中的第三个，只是收起态下只占一行标题：
// 「频率最低的排最后」这条与「入口要显眼」并不冲突，前者管的是块的位置，后者管的是入口。
//
// 配对与活跃传输之间的顺序由 `DevicesSection` 决定（它同时渲染网格与配对面板），
// 页面这一层只排两个兄弟块。
//
// 活跃传输那块同时是发送与传输两条子路由的落脚点：发送从设备卡片进（DESIGN.md 的
// Send Entry Contract），传输从这里进——两者都不在常驻导航里，理由见 `_lib/nav.ts`。
//
// ## 刻意不引入的一块
//
// 桌面端设备页还有「附近未配对设备」，这里不做：它依赖 mDNS 局域网发现，浏览器没有这个能力。
//
// 节点启停曾经也在这个「不做」清单里，理由是「给用户一个开关只会让人以为自己需要管它」。
// 那条判断仍然成立，所以它**没有**回到这一页——启停藏在常驻导航那枚状态徽章之后
// （`node-status-dialog.tsx`），要找才找得到，不会撞见。
export default function DevicesPage() {
  return (
    <PageShell>
      <PageHeader nav="devices" />
      <DevicesSection />
      <ActiveTransfersSection />
    </PageShell>
  );
}
