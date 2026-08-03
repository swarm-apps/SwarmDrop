import type { Metadata } from "next";
import type { ReactNode } from "react";
import { AppBottomNav, AppMobileHeader, AppSidebar } from "./_components/app-nav";
import { AppI18nProvider } from "./_components/i18n-provider";
import { PairingRequestHost } from "./_components/pairing-request-host";
import { TransferOfferHost } from "./_components/transfer-offer-host";
import { SecureContextBanner } from "./_components/secure-context-banner";
import { WebErrorView } from "./_components/web-error-view";
import { WebNodeBootstrap } from "./_components/web-node-bootstrap";

// 静态导出下 `metadata` 在**构建期**求值——那一刻没有「当前用户的 locale」可言，
// `<title>` / `<meta>` 只能是源 locale。这不是漏翻，是这套部署形态的正确行为；
// 运行时的界面文案全部走 i18n（见 `_components/i18n-provider.tsx`）。
export const metadata: Metadata = {
  title: "Web 应用",
  description: "在浏览器里直接收发文件：与桌面/移动端同源的 SwarmDrop 传输端。",
};

// Web 应用区外壳，独立于 fumadocs 的 (home)/docs。持久侧边栏 + 多路由（#88/#90）——
// 与桌面端的 breadcrumb-only 顶栏是**有意分叉**，理由见 `_lib/nav.ts` 与 DESIGN.md。
//
// 这里是**两个运行时单例**的唯一挂载点，都靠「layout 跨路由不重挂」成立：
//
//   WebNodeBootstrap   spawn 节点 + startEventConsumption + startStatePoll + ensureConfiguredRelays
//   AppI18nProvider    i18n 边界与 locale 解析
//
// **不要把它们下放到任何 page.tsx**——那会让每个路由各起一份：节点被反复重启、
// 同一事件被处理多次，locale 每次切页重解析一遍。
//
// 两个入站请求宿主（配对 / 文件）同样只能挂这里，但理由不同：它们要在**任何路由**下
// 都能弹出来。此前这两块内联在设备页与收件箱页里，用户正在 /app/send 挑文件时收到请求
// 就完全看不见——桌面与移动早就是全局宿主，Web 是三端里唯一的分叉。
export default function AppLayout({ children }: { children: ReactNode }) {
  return (
    <AppI18nProvider>
      {/* `data-swarmdrop-app` 是 shadcn base 规则（默认边框/描边色）的作用域锚点，
          见 app/global.css 的 @layer base——不加这个属性，应用区的边框会落到 currentColor。 */}
      <div data-swarmdrop-app className="flex min-h-screen">
        <WebNodeBootstrap />
        <PairingRequestHost />
        <TransferOfferHost />
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
    </AppI18nProvider>
  );
}
