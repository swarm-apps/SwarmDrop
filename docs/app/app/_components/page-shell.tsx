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

// 区块与区块之间用 `--space-section`（32px）而不是此前的 `gap-4`（16px）。
// 16 曾经与 `SectionShell` 的面板内间距、面板内边距完全相等，三档一样大 ——
// 分组关系于是只能靠边框说，页面读起来是几个等大的盒子在堆叠。语义与比例见 global.css。
const CONTENT =
  "mx-auto flex w-full flex-col gap-[var(--space-section)] px-4 py-6 sm:px-6";

/**
 * 内容列宽。**全站一个数**（2026-08-06 起），与桌面 `master-detail-shell.tsx` 同源。
 *
 * ## 它此前是三档，为什么合并
 *
 * 原来是 `board` 1240 / `settings` 1040 / `form` 860，按「这一页装的是什么」分。分档的
 * 出发点是对的（行长该受控），但**落地方式**有个它自己解决不了的副作用：三个宽度都
 * `mx-auto` 居中，于是内容左缘随路由跳——实测 1440 视口下设备/收件箱/传输在 224，
 * 设置在 307，发送在 402，**最远跳 178px**。而这四个入口就在侧栏里挨着，用户是连续切的。
 *
 * 一个稳定的左缘对「这是一个应用」的观感，比每页各自的理想行长更要紧；居中又是明确要保的，
 * 那么在「居中 + 左缘稳定」之下，唯一的解就是同宽。
 *
 * ## 行长控制没有丢，它换了层
 *
 * 行长本来就是**排版**属性，不是容器属性——绑在 `max-w` 上等于让一段文字的可读性取决于
 * 它碰巧住在哪个页面。现在归文字自己：正文块用 `max-w-[65ch]` 一类的字符宽度限制
 * （见 `send-panel.tsx`）。这样同一段说明无论放在哪一页都不会拉成长条，
 * 而网格、主从这些**该吃满宽度**的东西也不再被页面级 `max-w` 连坐。
 */
const COLUMN = "max-w-[1240px]";

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
        <div className={cn(CONTENT, COLUMN, "h-full min-h-[560px]")}>
          {notices}
          {children}
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className={cn(CONTENT, COLUMN, "min-h-full")}>
        {notices}
        {children}
      </div>
    </div>
  );
}
