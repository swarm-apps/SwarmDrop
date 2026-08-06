import type { Metadata } from "next";
import { DeviceStats } from "../_components/device-stats";
import { DevicesSection } from "../_components/devices-section";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.devices) };

// 应用首页（`/app` 重定向到这里）：设备关系页，承接桌面端 `_app/devices` 的角色——
// 先有已配对设备，发送、收件、传输才谈得上。
//
// ## 版式：页头（含三格概览）+ 主栏 / 配对侧栏
//
// ≥1280px 分两栏：**主栏**是设备网格与活跃传输，**侧栏**是配对。窄屏回落成竖排，顺序不变。
// 分栏的推导（为什么是 1280 而不是全局那个 920、为什么配对仍然不做成弹窗或子路由）
// 写在 `devices-section.tsx` 的文件头，这里不重复。
//
// 竖排下的相对顺序按**使用频率**排，不按功能亲缘：设备清单每次进来都要看；活跃传输是
// 「现在正在发生什么」，有内容时最该被看见；配对是一次性动作（配完即长期信任），使用频率
// 比前两者低两个数量级，所以它排在最后且默认收起。
//
// 配对的入口另外长在设备网格末尾那张「添加设备」卡片上（`device-grid.tsx`）——
// 入口长在它要改变的那个列表旁边，比一块常驻的表单更好找。
//
// ⚠️ **DOM 顺序是「设备网格 → 活跃传输 → 配对」**（`devices-section.tsx` 的两栏里，主栏
// 两块在前、侧栏在后）。写 e2e 选择器时以此为准，别按视觉位置猜：分栏时配对在**右**，
// 但它在 DOM 里排在活跃传输之后。
//
// 活跃传输那块同时是发送与传输两条子路由的落脚点：发送从设备卡片进（DESIGN.md 的
// Send Entry Contract），传输从这里进——两者都不在常驻导航里，理由见 `_lib/nav.ts`。
//
// ## 刻意不引入的一块
//
// 桌面端设备页还有「附近未配对设备」，这里不做：它依赖 mDNS 局域网发现，浏览器没有这个能力。
// 页头那三格概览因此也与桌面端不同——「附近」换成了「在线」，理由见 `device-stats.tsx`。
//
// 节点启停曾经也在这个「不做」清单里，理由是「给用户一个开关只会让人以为自己需要管它」。
// 那条判断仍然成立，所以它**没有**回到这一页——启停藏在常驻导航那枚状态徽章之后
// （`node-status-dialog.tsx`），要找才找得到，不会撞见。
export default function DevicesPage() {
  return (
    <PageShell>
      {/* `<DeviceStats />` 是 client component 的 element，本页是 server component——
          传 element 只递一个 client reference，不必序列化任何数据（对比：把设备数组读出来
          再传，就得让这个 server component 认识运行时 store，而那是拿不到的）。 */}
      <PageHeader nav="devices" aside={<DeviceStats />} />
      <DevicesSection />
    </PageShell>
  );
}
