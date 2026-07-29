import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { TransferActivityPanel } from "../_components/transfer-activity-panel";
import { NAV } from "../_lib/nav";

export const metadata: Metadata = { title: NAV.transfer.label };

// 选中态由 `?session=` 承载（静态导出下不能用动态路由段，理由见 transfer-activity-panel.tsx）。
// 读 query 所需的 Suspense 边界包在面板自身里。
export default function TransferPage() {
  return (
    <>
      <PageHeader item={NAV.transfer} />
      <TransferActivityPanel />
    </>
  );
}
