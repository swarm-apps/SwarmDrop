import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { SendPanel } from "../_components/send-panel";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.send) };

// `SendPanel` 读 `?peerId=`（设备页带过来的预选目标），Suspense 边界包在面板自身里。
export default function SendPage() {
  return (
    <>
      <PageHeader nav="send" />
      <SendPanel />
    </>
  );
}
