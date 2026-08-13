"use client";

// 行尺度的渐进披露：收起时是「标题 +（可选）机器值」，展开露出解释。
//
// ## 为什么要有这个文件
//
// 此前应用区有两处各自手搓的 `<details>`——设置页的「身份存储」与节点弹窗的「网络诊断」，
// 两处都直接用浏览器默认的 ▶ 三角标记。那是整个产品里仅有的两处非 Lucide 图形语汇，
// 既没有 hover、没有像样的命中区，也没有展开时的方向反馈，看起来像还没写完的 HTML。
//
// 应用区本来就有一套披露语汇（`section.tsx` 的 `SectionHeader`：`ChevronDown` + 180° 旋转
// + ease-out-quart），缺的只是它在**行**这个尺度上的对应物。两处各写各的正是「缺基元」的
// 典型症状，所以修法是补基元而不是分别调样式。
//
// ## 仍然用原生 `<details>`
//
// 键盘操作、读屏的 disclosure 语义、以及「JS 没跑起来也能展开」都是白拿的，换成 React state
// 一样也换不来。只把 marker 关掉、自己画箭头——`list-none` 管 Chrome/Firefox，
// `::-webkit-details-marker` 管旧 WebKit，两条缺一个就会漏出默认三角。
//
// ## 两档尺寸
//
// 默认档是**设置卡里的一行**：`p-4` 与 `SettingsRow` 逐字相同，整行可点、hover 有底色。
// `compact` 档给弹窗等空间紧张处：字号降到 11–12px、不铺 hover 底色（它外面通常套着一个
// 圆角边框盒，铺底色要么被裁要么盖住圆角，为一处诊断入口不值得）。

import { ChevronDown } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

export function Disclosure({
  label,
  value,
  children,
  compact = false,
  className,
  open,
  onOpenChange,
}: {
  /** 收起态的标题。**用名词短语**，与卡里其余标签（设备名 / 节点 ID / 可达地址）同一种口吻。 */
  label: ReactNode;
  /**
   * 收起时就摆在右侧的机器值——有它的话用户不点开也拿得到答案，折叠起来的就只剩「所以呢」。
   * 按 Mono Truth Rule 恒为等宽，所以**只传字面值**（ID、路径、API 名），别传散文。
   */
  value?: ReactNode;
  children: ReactNode;
  /** 紧凑档：见文件头。 */
  compact?: boolean;
  className?: string;
  /**
   * 受控展开态。**不传就是不受控**（原生 `<details>` 自己管，键盘与无 JS 都照常工作，
   * 那是这个基元用原生标签的全部理由）。
   *
   * 传它的场景只有一个：别处有个动作要**把这一节打开**（节点弹窗里「打开诊断」那个 CTA）。
   * 那时必须连 `onOpenChange` 一起传，否则用户自己点收起会被下一次渲染顶回去。
   */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <details
      className={cn("group", className)}
      open={open}
      onToggle={
        onOpenChange ? (e) => onOpenChange(e.currentTarget.open) : undefined
      }
    >
      <summary
        className={cn(
          "focus-ring flex cursor-pointer list-none items-center gap-3 transition-colors [&::-webkit-details-marker]:hidden",
          // 焦点环**画在里面**。`.focus-ring` 的 `outline-offset: 2px` 是给独立控件用的，
          // 而这一行是通栏的：它两侧紧贴容器边缘，而容器（`SettingsCard` / 这里的圆角边框盒）
          // 都是 `overflow-hidden`，往外 2px 的环左右两段会被整段裁掉，只剩上下两条横线。
          // 同一个环、同一种颜色宽度，只把偏移翻个号。
          "focus-visible:-outline-offset-2",
          // 底色定在 summary 上、只有标题单独提亮，是为了让**箭头与值跟着整行走**：
          // compact 档的 hover 反馈是提亮文字，箭头若自带 `text-muted-foreground`，
          // 就会出现「标题亮了、箭头没亮」的半截反馈。
          "text-muted-foreground",
          compact ? "px-3 py-2.5 text-xs hover:text-foreground" : "p-4 text-sm hover:bg-accent/40",
        )}
      >
        <span className={cn("shrink-0 font-medium", !compact && "text-foreground")}>{label}</span>
        {/* 没有 value 时这个 span 是空的，但 `flex-1` 仍然把箭头顶到右边——
            省掉一个「有没有 value」的分支。 */}
        <span className="min-w-0 flex-1 truncate text-right font-mono text-xs">{value}</span>
        <ChevronDown
          className="size-4 shrink-0 transition-transform duration-200 ease-[var(--ease-out-quart)] group-open:rotate-180 motion-reduce:transition-none"
          aria-hidden
        />
      </summary>
      <div className={cn(compact ? "px-3 pb-2.5" : "px-4 pb-4")}>{children}</div>
    </details>
  );
}
