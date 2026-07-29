import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { SendPanel } from "../_components/send-panel";
import { NAV } from "../_lib/nav";

export const metadata: Metadata = { title: NAV.send.label };

// `SendPanel` 读 `?peerId=`（设备页带过来的预选目标），Suspense 边界包在面板自身里。
export default function SendPage() {
  return (
    <>
      <PageHeader item={NAV.send} />
      <SendPanel />
    </>
  );
}
