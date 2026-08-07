import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * 类名合并。与桌面 `src/lib/utils.ts` 和 Web `_lib/cn.ts` 是同一个实现——
 * 本包不能 import 任一端的版本（那会把该端变成另一端的依赖），所以在这里各留一份。
 *
 * `clsx` 与 `tailwind-merge` 都是纯函数、无模块级状态，即使在两端各打包一份也不会
 * 出问题（不像 React / Lingui 那样有 context 单例）。
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
