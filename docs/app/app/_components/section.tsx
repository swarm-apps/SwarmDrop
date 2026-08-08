// 区块外壳与区块标题。对应桌面 `src/components/layout/section-primitives.tsx`——
// 两份实现、同一套标准。
//
// ## 玻璃只给结构容器（DESIGN.md 的 Flat-Control, Glass-Chrome Rule）
//
// 「你点它、往里打字」的东西一律扁平（`shadow-xs` 而已）；「装着别的东西」的东西才上玻璃。
// 所以：页面级面板 = [`SectionShell`]（`glass-panel` + 24px 圆角），面板里的设备卡 / 列表行 /
// 文件行一律 `bg-card` + 1px 边框。**永远不要给 button / input 加 `backdrop-filter`。**
//
// 圆角是两套词汇不是一套：控件 6–14px、面板 18–24px。Web 应用区此前全塌成 `rounded-lg`
// 与 `rounded-xl`，面板与按钮的圆角一样大，于是层级只能靠边框颜色暗示。
//
// ## 为什么 Web 上没有 WebGL 极光背景
//
// 桌面的玻璃是浮在一层缓慢流动的 aurora 上的，那层背景承担了大部分「活着的网络」的观感。
// Web 端刻意不搬它：`ogl` 是 +30KB 且持续占 GPU，而这里的基线视口是手机浏览器、
// 典型会话是「打开标签页快速收发一次文件」。玻璃本身（纯 CSS、零依赖）保留，
// 因为它承担的是**层级**而不是氛围。

import { ChevronDown, type LucideIcon } from "lucide-react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { FileBrowserView } from "@swarmdrop/shared-view";
import { cn } from "@/lib/cn";

/**
 * 主从两栏（列表栏 / 详情栏）共用的**面板皮**：玻璃 + 18px 圆角。
 *
 * 只收敛「皮」，布局（flex 方向、min-h-0、怎么滚、内边距）留在调用点——因为那部分**本来
 * 就各不相同**：列表栏 `overflow-hidden`（内部自己有滚动区），详情栏 `overflow-y-auto`
 * 且带 24px 内边距。把差异也塞进一个吃 variant 的组件，读的人反而要跳一次才知道这一栏怎么滚。
 *
 * 与 [`SectionShell`] 的分工只剩**圆角**：那个是页面级面板（24px），这个是主从布局里的栏
 * （18px，`--radius-panel-sm`）。两者同为「装着别的东西」的结构容器，所以同为玻璃。
 *
 * ## 它此前是 `bg-card`（实心），2026-08-06 改玻璃
 *
 * 旧注释写的理由是「它装满了列表行，玻璃会和行的边框打架」。那个顾虑没有成立：
 * `SectionShell`（同样是玻璃）里本来就放着一模一样的列表行（见 `active-transfers-section`），
 * 玻璃面 + 实心行恰恰是这套设计里表达层级的标准写法——**外层透、内层实**。
 *
 * 而代价是实打实的：收件箱与传输页整页只有主从两栏，它们一实心，那两页就完全没有玻璃，
 * 与设备/设置页并排看像两个产品。
 *
 * ⚠️ **行不要用这个常量**，否则就是玻璃套玻璃（DESIGN.md 明确禁止卡片套卡片）。行用
 * [`ROW_SURFACE`]。
 */
export const PANEL_SURFACE = "glass-panel rounded-[var(--radius-panel-sm)]";

/**
 * 面板**内部**一行的皮：实心卡底 + 1px 边框 + 一层浅投影。
 *
 * 与 [`PANEL_SURFACE`] 是一对：外层透（玻璃承担层级），内层实（内容坐在实底上才读得清）。
 * 这正是 `PANEL_SURFACE` 改玻璃之前它自己的值——那时两者共用一个常量，于是「栏」和「行」
 * 想分开调都做不到。
 */
export const ROW_SURFACE = "rounded-[var(--radius-panel-sm)] border bg-card shadow-xs";

/**
 * 主从列表里「这一行是当前选中项」的表达（品牌色边框 + accent 底 + 一圈淡 ring）。
 *
 * **两个分支一起收**，不是只收选中态：未选中态那句 `hover:bg-accent` 与选中态是一对
 * ——只搬一半，两边的悬停反馈迟早会各改各的，而它们是同一个列表在两条路由上的两种内容。
 * 收件箱行与传输会话行本来就该长得一样。
 */
export function selectedRowClass(selected: boolean): string {
  return selected
    ? "border-[var(--brand)]/40 bg-accent ring-1 ring-[var(--brand)]/20"
    : "hover:bg-accent";
}

/**
 * 详情侧**内嵌**的文件浏览器该多高（传输详情与收件箱详情共用）。
 *
 * ## 上限，不是下限
 *
 * 两处此前写的都是 `min-h-[320px]`，理由是「树形视图内部虚拟滚动，要有确定高度才算得出
 * 可见行」。但那个下限在绝大多数条目上都是净损失——一两个文件时，详情里固定挂着一块 320px
 * 的空槽，正是 `DESIGN.md`「Empty states size to their role」点名的 cavity。
 *
 * `max-h` 同样给得出确定高度（内容超上限时容器被钉死，`clientHeight` 有值，虚拟化照常
 * 只挂可见行），而内容少时贴着内容收。下限只在「这一节就是整屏主体」时才对。
 *
 * 顺带修掉的一件事：没有上限时，文件多的条目会把下方的动作区（归档 / 删除 / 暂停）推到
 * 几十行之外，用户要滚过整份清单才够得着。
 *
 * ## 为什么按视图分两档
 *
 * 树形一行 40px，网格一行约 220px（卡片 4:3 + 文件名两行 + 12px 间距，见
 * `file-grid-view.tsx` 的 `estimatedRowHeight`）。同一个数必然让其中一种难看：
 * 340 在网格下只露一行半，460 在树形下又是十一行半的高墙。
 *
 * 两个数都**刻意留半行/一小截露头**（树 8.5 行、网格 2 行 + 20px），那截被切掉的内容就是
 * 「下面还有，可以滚」的唯一提示——正好卡在整数行上反而像列表到此为止。
 */
export function fileSectionHeightClass(view: FileBrowserView): string {
  return view === "grid" ? "max-h-[460px]" : "max-h-[340px]";
}

/**
 * 页面级面板外壳：玻璃 + 24px 圆角。
 *
 * 用 `ComponentPropsWithRef` 而不是 `HTMLAttributes`：后者不含 `ref`，而 React 19 起
 * 函数组件的 `ref` 就是一个普通 prop——`{...props}` 本来就会把它转发到 `<section>`，
 * 差的只是类型放行。配对面板要拿它滚到视口里。
 */
export function SectionShell({
  children,
  className,
  ...props
  // 左半边只为把 `children` 变必填——`ComponentPropsWithRef<"section">` 自带的是可选的，
  // 而没有内容的面板外壳没有意义。`className` 不必重复声明，那边已经有了。
}: { children: ReactNode } & ComponentPropsWithRef<"section">) {
  return (
    <section
      {...props}
      className={cn(
        // 面板内分区 16px、面板内边距 20px。**两个数不能相等**——相等时内容离边缘和
        // 内容离彼此一样远，面板就不再像一个「装着东西的容器」，而像一张网格纸。
        // 内边距由容器角色决定（面板 20 / 卡片 14），不由内容决定，见 global.css 的间距语义。
        "glass-panel flex flex-col gap-[var(--space-in-panel)] rounded-[var(--radius-panel)] p-[var(--space-panel)]",
        className,
      )}
    >
      {children}
    </section>
  );
}

/**
 * 区块标题：图标片 + 标题 + 可选描述 + 右侧计数。
 *
 * 计数不是装饰：设备网格此前直接就是一个 `<ul>`，「已配对设备」这个概念只在空态文案里
 * 出现过一次——非空时用户看到的是一堆卡片，没有任何东西说它们是什么、有几台。
 */
export function SectionHeader({
  title,
  count,
  icon: Icon,
  description,
  action,
  disclosure,
}: {
  title: ReactNode;
  count?: number;
  icon?: LucideIcon;
  description?: ReactNode;
  /** 右上角的区块级动作（如「管理分组」）。 */
  action?: ReactNode;
  /**
   * 让整个标题块变成展开/收起开关。用于**使用频率远低于同页其它区块**的面板
   * （配对：一次性动作，配完即长期信任），收起后它只占一行而不是半屏。
   *
   * 形态是 ARIA APG 的手风琴：**button 在 `<h2>` 里面**，不是反过来。反过来写
   * （`<button><h2>…</h2></button>`）是无效 HTML——button 的内容模型是 phrasing content，
   * 标题是 flow content——而且读屏会丢掉这一层的标题语义。
   */
  disclosure?: { open: boolean; onToggle: () => void; controls: string };
}) {
  // 包着图标的小片是「装东西的容器」，所以它也吃玻璃——同桌面。
  const iconChip = Icon ? (
    <span className="glass-control mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full text-brand">
      <Icon className="size-3.5" aria-hidden />
    </span>
  ) : null;

  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      {disclosure ? (
        <div className="flex min-w-0 flex-1 gap-2.5">
          {iconChip}
          <div className="min-w-0 flex-1">
            {/*
              **button 在 `<h2>` 里面**（ARIA APG 的手风琴形态），且**只包标题**。
              把描述也包进去，标题的可访问名就变成「配对 与另一台设备互换一次邀请即可，
              之后长期有效。」——读屏按标题导航时会读到一整段。描述留在按钮外面。
            */}
            <h2>
              <button
                type="button"
                aria-expanded={disclosure.open}
                // 被控元素是条件渲染的：收起时它不在 DOM 里，这时留着 `aria-controls`
                // 就是一个悬空 IDREF（axe 的 aria-valid-attr-value 会报）。
                aria-controls={disclosure.open ? disclosure.controls : undefined}
                onClick={disclosure.onToggle}
                // `-m-1 p-1`：把命中区往外扩一圈但不改变视觉位置。
                className="focus-ring -m-1 flex w-full min-w-0 items-center gap-2 rounded-lg p-1 text-left transition-colors hover:bg-foreground/[0.03] dark:hover:bg-white/[0.04]"
              >
                <span className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-foreground">
                  {title}
                </span>
                <ChevronDown
                  className={cn(
                    "size-4 shrink-0 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.25,1,0.5,1)]",
                    disclosure.open && "rotate-180",
                  )}
                  aria-hidden
                />
              </button>
            </h2>
            {description && (
              <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
            )}
          </div>
        </div>
      ) : (
        <div className="flex min-w-0 gap-2.5">
          {iconChip}
          <div className="min-w-0">
            <h2 className="truncate text-[15px] font-semibold tracking-tight text-foreground">
              {title}
            </h2>
            {description && (
              <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
            )}
          </div>
        </div>
      )}
      <div className="flex shrink-0 items-center gap-2">
        {typeof count === "number" && (
          <span className="rounded-full bg-foreground/[0.045] px-2.5 py-1 text-[11px] font-semibold tabular-nums text-muted-foreground dark:bg-white/[0.06]">
            {count}
          </span>
        )}
        {action}
      </div>
    </div>
  );
}
