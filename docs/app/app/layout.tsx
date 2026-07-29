import type { Metadata } from "next";
import type { ReactNode } from "react";
import { AppBottomNav, AppMobileHeader, AppSidebar } from "./_components/app-nav";
import { SecureContextBanner } from "./_components/secure-context-banner";
import { WebErrorView } from "./_components/web-error-view";
import { WebNodeBootstrap } from "./_components/web-node-bootstrap";

export const metadata: Metadata = {
  title: "Web 应用",
  description: "在浏览器里直接收发文件：与桌面/移动端同源的 SwarmDrop 传输端。",
};

// Web 应用区外壳，独立于 fumadocs 的 (home)/docs。持久侧边栏 + 多路由（#88/#90）——
// 与桌面端的 breadcrumb-only 顶栏是**有意分叉**，理由见 `_lib/nav.ts` 与 DESIGN.md。
//
// 这里是运行时单例的唯一挂载点：`WebNodeBootstrap` 内部同时负责 spawn 节点、
// `startEventConsumption`、`startStatePoll` 与 `ensureConfiguredRelays`。layout 跨路由不重挂，
// 所以切页不会重启节点、不会重复消费事件流。**不要把它下放到任何 page.tsx**——
// 那会让每个路由各起一份，同一事件被处理多次。
export default function AppLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-screen">
      <WebNodeBootstrap />
      <AppSidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <AppMobileHeader />
        <main className="mx-auto w-full max-w-4xl flex-1 space-y-4 px-4 py-6">
          <SecureContextBanner />
          <WebErrorView />
          {children}
        </main>
        {/* fixed 导航 + 自带的等高 spacer，高度定义在 app-nav 里，layout 不参与。 */}
        <AppBottomNav />
      </div>
    </div>
  );
}
