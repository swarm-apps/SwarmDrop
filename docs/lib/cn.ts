import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * 类名合并。**两步缺一不可**：
 *
 * 1. `clsx` 把条件形参（对象 / 数组 / 假值）摊平成字符串；
 * 2. `twMerge` 解决 Tailwind 的冲突（后写的 `px-4` 顶掉前面的 `px-2`）。
 *
 * 此前这里只有 `export { twMerge as cn }`，少了第一步——传对象进去会被
 * `String({active: true})` 变成 `"[object Object]"` 混进 class 属性里，不报错、样式静默失效。
 * shadcn/ui 的组件大量依赖条件形参，接入前必须先补上（openspec: web-ux-alignment 阶段三）。
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
