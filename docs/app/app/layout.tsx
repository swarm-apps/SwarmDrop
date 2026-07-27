import type { Metadata } from "next";
import type { ReactNode } from "react";
import { appName, appIconPath } from "@/lib/shared";
import { NodeStatusPill } from "./_components/node-status-pill";
import { SecureContextBanner } from "./_components/secure-context-banner";
import { WebNodeBootstrap } from "./_components/web-node-bootstrap";

export const metadata: Metadata = {
  title: "Web 应用",
  description: "在浏览器里直接收发文件：与桌面/移动端同源的 SwarmDrop 传输端。",
};

// Web 应用区外壳，独立于 fumadocs 的 (home)/docs。顶栏 = 品牌 + 节点状态 pill（DESIGN 的
// breadcrumb-only 导航模式：无持久 sidebar）。WebNodeBootstrap 挂在这里，随 /app 存活。
export default function AppLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen flex-col">
      <WebNodeBootstrap />
      <header className="sticky top-0 z-10 border-b border-fd-border bg-fd-background">
        <div className="mx-auto flex w-full max-w-3xl items-center justify-between px-4 py-3">
          <span className="inline-flex items-center gap-2 font-semibold text-fd-foreground">
            <img src={appIconPath} alt="" className="size-5" />
            {appName}
            <span className="font-normal text-fd-muted-foreground">Web</span>
          </span>
          <NodeStatusPill />
        </div>
      </header>
      <SecureContextBanner />
      <main className="mx-auto w-full max-w-3xl flex-1 px-4 py-6">{children}</main>
    </div>
  );
}
