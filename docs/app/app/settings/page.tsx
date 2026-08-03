import type { Metadata } from "next";
import { ConnectionPanel } from "../_components/connection-panel";
import { DevEventLog } from "../_components/dev-event-log";
import { LocaleSwitcher } from "../_components/locale-switcher";
import { NodePanel } from "../_components/node-panel";
import { PageHeader } from "../_components/page-header";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.settings) };

// 低频配置 + 诊断区（#94）。这三块原先与收发主路径并列在单页里，使用频率却差两个数量级。
//
// 事件日志只在这里**展示**，事件**消费**仍是 layout 单点（`WebNodeBootstrap`）——
// 不要因为日志搬到了本页就在这里再起一次 `startEventConsumption`，那会让同一事件被处理两次。
export default function SettingsPage() {
  return (
    <>
      <PageHeader nav="settings" />
      <LocaleSwitcher />
      <NodePanel />
      <ConnectionPanel />
      <DevEventLog />
    </>
  );
}
