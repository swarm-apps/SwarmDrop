"use client";

import { cn } from "./cn";

/**
 * 进度条。**刻意不依赖 radix**——桌面的 `ui/progress.tsx` 走 `radix-ui`，Web 有自己的
 * `ProgressBar`，两者都只是「一条轨道 + 一段填充」。为这点结构引入一个带模块级状态的
 * 依赖，等于给本包多开一个实例分裂的口子（见 README 的 `file:` 那节）。
 *
 * `value` 为 `null` 时渲染成不确定态的空轨道（而不是 0%）——「还不知道进度」与
 * 「进度是 0」是两件事。
 */
export function Progress({
  value,
  className,
  label,
}: {
  value: number | null;
  className?: string;
  /**
   * 可访问名。传 `null` 表示**装饰性**：调用方已经在祖先元素上表达了进度
   * （例如整行是个 `<button>`，其可访问名里就带着百分比）。
   * ARIA 会丢弃交互控件的后代角色，那时再挂一个 progressbar 只是噪声。
   */
  label: string | null;
}) {
  const percent = value === null ? null : Math.min(100, Math.max(0, value));

  return (
    <div
      className={cn(
        "relative h-1.5 w-full overflow-hidden rounded-full bg-primary/20",
        className,
      )}
      {...(label === null
        ? { "aria-hidden": true }
        : {
            role: "progressbar",
            "aria-label": label,
            "aria-valuemin": 0,
            "aria-valuemax": 100,
            ...(percent === null ? {} : { "aria-valuenow": Math.round(percent) }),
          })}
    >
      <div
        className="h-full rounded-full bg-primary transition-[width] duration-300 ease-out"
        style={{ width: `${percent ?? 0}%` }}
      />
    </div>
  );
}
