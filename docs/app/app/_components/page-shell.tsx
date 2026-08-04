// 各功能页共用的内容外壳：限宽、边距、全局提示条的位置，以及**这一页怎么滚**。
//
// ## 为什么需要一个原语而不是在 layout 里写死
//
// 应用外壳（`layout.tsx`）现在是 `h-dvh` 的受限高度容器，`main` 不自己滚。滚动归页面，
// 因为只有页面知道自己是哪一种：
//
//   · `scroll`（默认）—— 整页一起滚。设备、发送、设置这类竖向内容。
//   · `fill`         —— 页面自己管内部滚动区。收件箱、传输的主从布局属于这类：
//                        宽屏下左列表与右详情各滚各的，滚列表不该把详情带走。
//
// 两者的区别只有一处：`scroll` 在外面套一层 `overflow-y-auto`，`fill` 把高度原样传下去。
// 限宽 1240 与桌面 `master-detail-shell.tsx` 同一个数。
//
// ## `min-h-full` 不是可有可无的
//
// `scroll` 变体的内层是 `min-h-full`：内容短时它仍然撑满一屏，页面因此能用 `flex-1`
// 让空态在**垂直方向居中**。少了它，每个空态都是「顶部一张薄卡 + 下面大片空白」——
// 那正是这套页面此前的样子。

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { SecureContextBanner } from "./secure-context-banner";
import { WebErrorView } from "./web-error-view";

const CONTENT = "mx-auto flex w-full max-w-[1240px] flex-col gap-4 px-4 py-6 sm:px-6";

export function PageShell({
  variant = "scroll",
  children,
}: {
  variant?: "scroll" | "fill";
  children: ReactNode;
}) {
  // 全局提示条跟着页面走：它们是内容的一部分（讲的是「这一屏为什么不好使」），
  // 不是导航 chrome。
  const notices = (
    <>
      <SecureContextBanner />
      <WebErrorView />
    </>
  );

  if (variant === "fill") {
    return (
      // `h-full` 而不是 `flex-1`：两者在正常视口下等价，但 `h-full` 是**确定高度**，
      // 于是「极矮视口 / 顶部块很高」时内容可以超出它，由外层的 `overflow-y-auto` 兜住。
      //
      // 这个兜底不是装饰。收件箱页在主从之上还挂着「待处理请求」，那块的高度随请求条数
      // 变化；没有兜底时它一涨，主从就被压到接近 0（`min-h-0` 允许压到 0），而 `fill`
      // 本身不滚——已收到的文件整块**够不着**，页面上却没有任何滚动条提示还有东西。
      // 请求区自己也限了高（见 `incoming-offers-panel.tsx`），两道加起来才够。
      //
      // `min-h-[560px]`：请求区上限 + 页头 + 主从最小可用高。低于它就该出滚动条了，
      // 硬塞只会让每一块都不可用。
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className={cn(CONTENT, "h-full min-h-[560px]")}>
          {notices}
          {children}
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className={cn(CONTENT, "min-h-full")}>
        {notices}
        {children}
      </div>
    </div>
  );
}
