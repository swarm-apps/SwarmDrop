// 设置页的共享基元：分组（Section）→ 分组卡（Card）→ 设置行（Row）。
//
// **这是桌面 `src/routes/_app/settings/-settings-primitives.tsx` 的第二份实现，同一套标准。**
//
// ## 为什么设置页不用 `SectionShell`
//
// 应用区其余页面（设备 / 收件箱 / 传输）用的是 `section.tsx` 的 `SectionShell` +
// `SectionHeader`：24px 圆角的玻璃面板、标题带一枚玻璃图标片。设置页刻意**不复用**它，
// 桌面也是这么分的，理由是内容形状不同：
//
//   · 那几页装的是**列表与网格**——设备卡、收件箱行、传输会话。面板是它们的容器，
//     容器自己要有存在感。
//   · 设置装的是**一串带标签的小行**——每行一个「说明在左、控件在右」。这种内容需要的是
//     行与行之间的发丝线，不是一层套一层的面板。给每组设置都套一个 24px 玻璃面板，
//     结果就是四块巨大的卡片竖着堆，正是这一页此前的样子。
//
// 于是标题也降一档：14px semibold + 一枚**裸的**品牌色图标（不是玻璃片），因为它标的是
// 「一组设置」而不是「一块面板」——面板是下面那张卡。
//
// ## 与桌面的差异只有一处：`SettingsRow` 在窄屏折行
//
// 桌面 `SettingsRow` 恒为 `flex items-center justify-between`，那里没有手机视口。
// 这里的基线是手机浏览器，标题 + 控件在 375px 下并排会把控件挤没，所以
// `sm:` 以下改为竖排。其余（内边距 16px、`last:border-b-0` 的发丝线、20px 卡角）逐字一致。

import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/cn";

export function SettingsSection({
  title,
  icon: Icon,
  aside,
  children,
  className,
}: {
  title: ReactNode;
  icon?: ComponentType<{ className?: string }>;
  /** 标题右侧的附加内容（计数徽标、状态胶囊等）。 */
  aside?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    // `h-full` + 卡上的 `flex-1`：只有在网格用 `items-stretch` 时才起作用（同一行两块等高）。
    // 设置页目前用的是 `items-start`（理由见 `settings/page.tsx`），所以这两条是**为将来
    // 真的需要配平的行留的**——留着不产生任何视觉后果（`h-full` 在 auto 高度的网格项上等于 auto）。
    <section className={cn("flex h-full flex-col gap-[var(--space-in-group)]", className)}>
      <div className="flex items-center gap-2 px-1">
        {Icon && <Icon className="size-4 shrink-0 text-brand" aria-hidden />}
        <h2 className="text-sm font-semibold tracking-tight text-foreground">{title}</h2>
        {aside && <div className="ml-auto">{aside}</div>}
      </div>
      {children}
    </section>
  );
}

/**
 * 分组卡：一张玻璃卡，内部靠 [`SettingsRow`] 的发丝线划分，**不做卡片套卡片**。
 *
 * 20px 圆角与桌面同——落在 DESIGN.md 的面板档（18–24px）内，比 `SectionShell` 的 24
 * 收一档，因为它比那种页面级面板小一号。
 */
export function SettingsCard({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("glass-card flex flex-1 flex-col overflow-hidden rounded-[20px]", className)}>
      {children}
    </div>
  );
}

/**
 * 设置行：左侧标题 + 描述，右侧控件；同卡内靠下边框分隔，最后一行不画线。
 *
 * 描述是可选的，但**有描述时它属于标题**——`gap-0.5`（2px）是组内距，与行间的 16px
 * 内边距差了 8 倍，这就是「这句话在解释上面那行」的全部说明（见 global.css 的间距语义）。
 */
export function SettingsRow({
  title,
  description,
  action,
  children,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  /** 右侧控件。与 `children` 互斥使用：控件放得下就用 action，放不下就用 children 走整行。 */
  action?: ReactNode;
  /** 需要整行宽度的内容（输入框、地址、列表）——放在标题块下方，不挤右侧。 */
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("border-b p-4 last:border-b-0", className)}>
      <div
        className={cn(
          // 窄屏竖排：375px 下「标题 + 右侧控件」并排会把控件挤到没有可点区域。
          "flex flex-col gap-2",
          action && "sm:flex-row sm:items-center sm:justify-between sm:gap-4",
        )}
      >
        <div className="flex min-w-0 flex-col gap-0.5">
          <div className="text-sm font-medium text-foreground">{title}</div>
          {description && (
            <span className="text-xs leading-5 text-muted-foreground">{description}</span>
          )}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
      {children && <div className="mt-3">{children}</div>}
    </div>
  );
}
