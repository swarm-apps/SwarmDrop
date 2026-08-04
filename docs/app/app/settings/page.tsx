import type { Metadata } from "next";
import { AboutPanel } from "../_components/about-panel";
import { AppearancePanel } from "../_components/appearance-panel";
import { ConnectionPanel } from "../_components/connection-panel";
import { DevEventLog } from "../_components/dev-event-log";
import { NodePanel } from "../_components/node-panel";
import { PageHeader } from "../_components/page-header";
import { PageShell } from "../_components/page-shell";
import { NAV, navTitle } from "../_lib/nav";

export const metadata: Metadata = { title: navTitle(NAV.settings) };

// 低频配置 + 诊断区（#94）。这几块原先与收发主路径并列在单页里，使用频率却差两个数量级。
//
// ## 顺序：本机身份 → 连接 → 外观 → 关于 → （dev）诊断
//
// 此前是「语言 → 节点 → 连接 → 事件日志」，把全应用最低频的语言切换放在了第一屏，而用户
// 进设置页最常要找的是「我是谁 / 连上了没有」。现在按**「这台机器的事实」→「它怎么连出去」
// →「它长什么样」→「它是什么」**排，诊断垫底。
//
// 事件日志只在这里**展示**且**只在开发构建里渲染**，事件**消费**仍是 layout 单点
// （`WebNodeBootstrap`）——不要因为日志在这一页就在这里再起一次 `startEventConsumption`，
// 那会让同一事件被处理两次。
export default function SettingsPage() {
  return (
    <PageShell>
      <PageHeader nav="settings" />
      <NodePanel />
      <ConnectionPanel />
      <AppearancePanel />
      <AboutPanel />
      <DevEventLog />
    </PageShell>
  );
}
