import { t } from "@lingui/core/macro";

/**
 * 条目标题：`title` 列存的是**首个文件名**，「等 N 个文件」这句由这里生成。
 *
 * 曾经是 Rust 侧渲染好整句再落库（`inbox_title`），于是历史条目的标题永远冻结在
 * 写入时的语言。库里现在只存与语言无关的文件名，切语言立刻生效。
 *
 * 三端各写一份而不进 `packages/shared-view`：它的返回值是**本地化文案**，
 * 而该包的归属判据第 3 条把这类东西判在外面（`formatRelativeTime` 同理）。
 */
export function inboxItemTitle(title: string, itemCount: number): string {
  if (itemCount === 0) return t`空传输`;
  if (itemCount === 1) return title;
  return t`${title} 等 ${itemCount} 个文件`;
}
