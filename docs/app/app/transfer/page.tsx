import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { TransferActivityPanel } from "../_components/transfer-activity-panel";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.transfer) };

// 选中态由 `?session=` 承载（静态导出下不能用动态路由段，理由见 transfer-activity-panel.tsx）。
// 读 query 所需的 Suspense 边界包在面板自身里。
export default function TransferPage() {
  return (
    // `fill`：同收件箱，宽屏下是主从布局，两栏独立滚动。
    <PageShell variant="fill">
      <PageHeader nav="transfer" />
      <TransferActivityPanel />
    </PageShell>
  );
}
