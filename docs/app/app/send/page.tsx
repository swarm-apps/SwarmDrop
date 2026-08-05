import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { SendPanel } from "../_components/send-panel";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.send) };

// `SendPanel` 读 `?peerId=`（设备页带过来的预选目标），Suspense 边界包在面板自身里。
export default function SendPage() {
  return (
    // `column="form"`：这一页是「选设备 / 选文件 / 发送」三步的表单，不是内容板。
    <PageShell column="form">
      <PageHeader nav="send" />
      <SendPanel />
    </PageShell>
  );
}
