import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { IncomingOffersPanel, InboxPanel } from "../_components/receive-panel";
import { NAV } from "../_lib/nav";

export const metadata: Metadata = { title: NAV.inbox.label };

// 与 /app/transfer 的分工（#96）：收件箱是**结果**——已落盘、可下载、可长期回看；
// 传输页是**过程**——进行中、可续传、含发送方向。同一个会话在两处各出现一次是刻意的，
// 但两处都不承担对方的职责。
export default function InboxPage() {
  return (
    <>
      <PageHeader item={NAV.inbox} />
      <IncomingOffersPanel />
      <InboxPanel />
    </>
  );
}
