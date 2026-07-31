import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { IncomingOffersPanel, InboxPanel } from "../_components/receive-panel";
import { NAV } from "../_lib/nav";

export const metadata: Metadata = { title: NAV.inbox.label };

// 与 /app/transfer 的分工（#96）：收件箱是**结果**——已落盘、可下载、可长期回看；
// 传输页是**过程**——进行中、可续传、含发送方向。同一次接收在两处各出现一次是刻意的，
// 但两处都不承担对方的职责。
//
// 这条分工现在有了存储层的支撑：**两张各自的表**（收件箱条目 / 传输会话），而不再是
// 同一份 projection 的两种过滤。差别是可观测的——清空传输历史、以及历史触到 100 条上限
// 被淘汰时，收件箱条目都不受影响。
//
// 这条分工也定死了文件的归属：**文件的生命周期属于收件箱侧**，传输页的删除/清空只删
// 账本（三端一致，不因 Web 的文件在 OPFS 里就分叉）。要释放 OPFS 空间的入口将来开在这里，
// 而不是在「过程」页删掉「结果」页还在展示、还能下载的东西。
export default function InboxPage() {
  return (
    <>
      <PageHeader item={NAV.inbox} />
      <IncomingOffersPanel />
      <InboxPanel />
    </>
  );
}
