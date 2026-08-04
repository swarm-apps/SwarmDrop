import * as SwitchPrimitives from "@rn-primitives/switch";
import { Platform } from "react-native";
import { cn } from "@/lib/utils";

function Switch({
  className,
  ...props
}: React.ComponentProps<typeof SwitchPrimitives.Root>) {
  return (
    <SwitchPrimitives.Root
      className={cn(
        "flex h-[1.15rem] w-8 shrink-0 flex-row items-center rounded-full border border-transparent shadow-sm shadow-black/5",
        Platform.select({
          web: "focus-visible:border-ring focus-visible:ring-ring/50 peer inline-flex outline-none transition-all focus-visible:ring-[3px] disabled:cursor-not-allowed",
        }),
        props.checked ? "bg-primary" : "bg-input dark:bg-input/80",
        props.disabled && "opacity-50",
        className,
      )}
      {...props}
    >
      <SwitchPrimitives.Thumb
        className={cn(
          "bg-background size-4 rounded-full transition-transform",
          Platform.select({
            web: "pointer-events-none block ring-0",
          }),
          // 拨钮两态**都是浅色**，只有位置在变——它是「旋钮」，不是状态色的载体，
          // 状态由轨道（`bg-primary` / `bg-input`）表达。
          //
          // 开态曾经写 `dark:bg-primary-foreground`，那时它恰好是白色所以看不出问题；
          // 2026-08-04 把 `--primary-foreground` 对齐成深墨（青绿实心底恒配深墨字，
          // 见 DESIGN.md 的 Brand Fidelity Rule）之后，暗色下一拨到「开」拨钮就从近白
          // 翻成近黑——同一个控件两个状态明暗颠倒，读起来像坏了。
          //
          // **别再让拨钮引用 `--primary-foreground`**：那个 token 的语义是「青绿实心底上的
          // 文字色」，拨钮压的是轨道不是文字。
          props.checked ? "translate-x-3.5" : "translate-x-0",
        )}
      />
    </SwitchPrimitives.Root>
  );
}

export { Switch };
