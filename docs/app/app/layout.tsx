import type { Metadata } from "next";
import type { ReactNode } from "react";
import { AppAmbientBackground } from "./_components/app-ambient-background";
import { AppBottomNav, AppMobileHeader, AppSidebar } from "./_components/app-nav";
import { AppI18nProvider } from "./_components/i18n-provider";
import { PairingRequestHost } from "./_components/pairing-request-host";
import { ReloadGuard } from "./_components/reload-guard";
import { TransferOfferHost } from "./_components/transfer-offer-host";
import { TextDeliveryAttentionHost } from "./_components/text-delivery-attention-host";
import { WebNodeBootstrap } from "./_components/web-node-bootstrap";
import { Toaster } from "@/components/ui/sonner";
import { WindowDropGuard } from "./_components/window-drop-guard";

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
      {/*
        `h-dvh` 而不是 `min-h-screen`：这是**整个应用区滚动行为的根**。
        原先是 min-h-screen + 内容自然流，于是各面板里写的 `min-h-0 + overflow-y-auto`
        全是死代码——祖先链上没有一个确定高度的包含块，它们永远不会独立滚动，只会把整页
        撑长（列表一滚，筛选条与操作按钮一起滚走）。桌面端对应的是 `_app.tsx` 的
        `h-svh flex flex-col` + `main flex-1 overflow-hidden`。

        用 `dvh` 不用 `svh`：移动浏览器地址栏收起时可视高度会变，dvh 跟随它，避免底部
        导航被顶出屏幕。

        `data-swarmdrop-app` 是 shadcn base 规则（默认边框/描边色）的作用域锚点，
        见 app/global.css 的 @layer base——不加这个属性，应用区的边框会落到 currentColor。
      */}
      <div data-swarmdrop-app className="app-shell flex h-dvh">
        {/*
          环境层（WebGL 极光）。它是 z-0，其余结构件靠自己的 `relative z-10` 压在上面
          ——`.app-shell` 不做通配提升，理由写在 global.css 那条规则旁边。

          挂 layout 与下面几个宿主同理，但它多一条硬理由：它持有 WebGL context，
          每路由一份会撞浏览器对同时存活 context 数的上限。
        */}
        <AppAmbientBackground />
        <WebNodeBootstrap />
        {/* 窗口级的误投放护栏——拖偏了不该把整个节点连页面一起弄没。挂这里的理由与上面两个
            宿主相同：它要在**任何路由**下都生效，而不只是发送页。 */}
        <WindowDropGuard />
        {/* 「非终态发送会话不跨刷新」的离开拦截。挂这里的理由与 WindowDropGuard 一样：
            关标签页可以发生在任何路由下，而传输页那句说明只有正看着会话时才可见。 */}
        <ReloadGuard />
        <PairingRequestHost />
        <TransferOfferHost />
        <TextDeliveryAttentionHost />
        {/* toast 宿主。挂 layout 与两个请求宿主同理：任何路由下的动作都要能给出反馈。
            **只在应用区挂**，文档站不需要也不该被它影响。 */}
        <Toaster />
        <AppSidebar />
        <div className="relative z-10 flex min-h-0 min-w-0 flex-1 flex-col">
          <AppMobileHeader />
          {/*
            `main` 只提供**受限高度**，自己不滚——滚动归页面，由 `PageShell` 的
            `scroll` / `fill` 两个变体决定（限宽、边距、全局提示条也在那里）。
            只有页面知道自己是「整页一起滚」还是「内部分区各滚各的」。
          */}
          <main className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</main>
          {/* fixed 导航 + 自带的等高 spacer，高度定义在 app-nav 里，layout 不参与。 */}
          <AppBottomNav />
        </div>
      </div>
    </AppI18nProvider>
  );
}
