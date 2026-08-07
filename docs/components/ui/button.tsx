import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/cn"

/**
 * **与桌面 `src/components/ui/button.tsx` 的一处有意分叉：尺寸是响应式的。**
 *
 * 桌面壳是纯指针环境（Tauri 窗口），固定 `h-9` / `h-8` 合适。Web 应用区的基线视口是
 * **手机浏览器**，DESIGN.md 的跨端复查清单要求触摸目标 ≥44×44 CSS px——所以这里的
 * 每一档都是 `min-h-11`（44px）起，`sm:` 之后才降回桌面那套密度。
 *
 * 烤进变体而不是在调用点写：此前是每个 `<Button>` 各带一串 `min-h-11 sm:min-h-9`，
 * 仓库里重复了三十多次，而漏写的地方没有任何东西会提醒你。规则属于按钮，不属于调用点。
 *
 * 用 `min-h-*` 而非 `h-*`：文案换行时不裁切（英文/繁中比中文长，窄屏更容易折行）。
 *
 * **从桌面同步这个文件时不要覆盖掉这段。**
 */

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-white hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
        outline:
          "border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ghost:
          "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
        link: "text-brand underline-offset-4 hover:underline",
      },
      size: {
        default: "min-h-11 px-4 py-2 has-[>svg]:px-3 sm:min-h-9",
        xs: "min-h-11 gap-1 rounded-md px-2 text-xs has-[>svg]:px-1.5 sm:min-h-6 [&_svg:not([class*='size-'])]:size-3",
        sm: "min-h-11 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5 sm:min-h-8",
        lg: "min-h-11 rounded-md px-6 has-[>svg]:px-4 sm:min-h-10",
        icon: "size-11 sm:size-9",
        "icon-xs": "size-11 rounded-md sm:size-6 [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-11 sm:size-8",
        "icon-lg": "size-11 sm:size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
