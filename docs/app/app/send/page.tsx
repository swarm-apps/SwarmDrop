import type { Metadata } from "next";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { SendPanel } from "../_components/send-panel";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.send) };

// `SendPanel` 读 `?peerId=`（设备页带过来的预选目标），Suspense 边界包在面板自身里。
//
// 页宽跟全站走 1240（2026-08-06 起）。此前这一页单独用 860，理由是「表单不是内容板」——
// 那条判断没错，但它被写在了**页面级 `max-w`** 上，于是本页内容比设备页右移 178px，
// 侧栏一切换就是一次横跳。表单该窄这件事现在由面板自己负责（`send-panel.tsx` 里的
// `max-w`），页面只管左缘对齐。推导见 `page-shell.tsx` 的 `COLUMN`。
export default function SendPage() {
  return (
    <PageShell>
      <PageHeader nav="send" />
      <SendPanel />
    </PageShell>
  );
}
