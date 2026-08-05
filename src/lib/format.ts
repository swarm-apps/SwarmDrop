import { i18n } from "@lingui/core";
import { t } from "@lingui/core/macro";
import {
  formatTimeLeft as formatTimeLeftIn,
  formatTransferRate,
} from "@swarmdrop/shared-view";

/**
 * 桌面端格式化模块。
 *
 * **跨端一致的部分已收口到 `@swarmdrop/shared-view`**（字节大小、百分比、时长、延迟），
 * 这里只留两类：绑定桌面 i18n 运行时的薄封装，以及尚未跨端统一的本地化文案。
 */

export { calcPercent, formatDuration, formatFileSize, formatLatency } from "@swarmdrop/shared-view";

/**
 * 传输速度。共享的 [`formatTransferRate`] 在算不出速率时返回 `null`（占位是一句要翻译的
 * 文案，不该烤进格式化函数），桌面在这里补上自己的破折号。
 */
export function formatSpeed(bytesPerSec: number | null): string {
  return formatTransferRate(bytesPerSec) ?? "—";
}

/**
 * 剩余时长的人类文案（邀请有效期用），跟随 Lingui 的当前 locale。
 *
 * 共享实现显式收一个 locale——包里没有 i18n 运行时，也不该有。这层薄封装的存在意义就是
 * 把「当前 locale 从哪来」这件桌面特有的事留在桌面。
 *
 * 早期版本返回硬编码中文，导致英文 UI 出现「Expires in 23 小时 59 分」这种混排——这个函数的
 * 产物**一定**落在 `<Trans>` 的插值位，所以它自己必须是本地化的。
 */
export function formatTimeLeft(seconds: number): string {
  return formatTimeLeftIn(seconds, i18n.locale || "zh");
}

/**
 * 「首文件名 等 N 个文件」——一批文件的统称，收件箱条目与传输会话共用同一句。
 *
 * 收在这里是因为 Lingui 的 msgid 由模板 + **占位名**生成：两处各写各的
 * （`{firstFileName} 等 {fileCount} 个文件` vs `{primaryFileName} 等 {itemCount} 个文件`）
 * 会在三份 catalog 里各留下两条内容相同的条目，zh-TW 一度真的把同一句翻译了两遍。
 *
 * 调用方各自保留自己的单数分支与兜底文案（收件箱是「空传输」、传输会话是「未知文件」）
 * ——那两句语义不同，不该被这个函数吸进来。
 */
export function fileGroupLabel(firstFileName: string, count: number): string {
  return t`${firstFileName} 等 ${count} 个文件`;
}

/**
 * 相对时间。
 *
 * **刻意不进共享包**：输出是本地化文案，而三端各说各的（移动端返回的甚至是 `<Trans>` 节点，
 * 不是字符串）。收进共享包就必须改掉其中一端的渲染输出。
 *
 * 这里的中文是硬编码的——它先于 Lingui 接入存在，属于既有负债，不在共享包收口的范围内。
 */
export function formatRelativeTime(date: Date | number): string {
  const now = Date.now();
  const ts = typeof date === "number" ? date : date.getTime();
  const diff = Math.floor((now - ts) / 1000);

  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}
