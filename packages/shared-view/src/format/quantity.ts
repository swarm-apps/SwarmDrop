/**
 * 数量的展示语言：字节、速率、百分比。
 *
 * 三者的单位（`B`/`KB`/`MB`/`GB`、`/s`、`%`）都是跨语言通用的，所以整块可以共享；
 * 需要翻译的部分（「等待数据」「未知」这类占位）**一律不在这里**——见下面各函数的说明。
 */

/** 内核在移动端经 uniffi 下发的是 `bigint`，桌面 / Web 是 `number`。三端同一个入口。 */
export type ByteCount = number | bigint;

function toNumber(value: ByteCount): number {
  return typeof value === "bigint" ? Number(value) : value;
}

const KIB = 1024;
const MIB = KIB * 1024;
const GIB = MIB * 1024;

/**
 * 字节大小。1024 进制，小数位随量级递增（KB/MB 一位、GB 两位）——
 * 大文件的传输里第二位小数是有信息量的，小文件则只是噪声。
 */
export function formatFileSize(bytes: ByteCount): string {
  const n = toNumber(bytes);
  if (n < KIB) return `${n} B`;
  if (n < MIB) return `${(n / KIB).toFixed(1)} KB`;
  if (n < GIB) return `${(n / MIB).toFixed(1)} MB`;
  return `${(n / GIB).toFixed(2)} GB`;
}

/**
 * 传输速率。**算不出来时返回 `null`，不返回占位文案**——占位是一句要翻译的 UI 文案
 * （桌面「—」/ Web「等待数据」/ 英文 `Unknown`），烤进格式化函数就等于三端必然吵起来。
 *
 * 算不出来的三种：尚未估算（`null` / `undefined`）、非有限数、非正数。最后一种是刻意的：
 * 0 B/s 在进度条上和「还没开始」无法区分，说「0 B/s」只是把一个占位换成另一个。
 */
export function formatTransferRate(bytesPerSecond: ByteCount | null | undefined): string | null {
  if (bytesPerSecond === null || bytesPerSecond === undefined) return null;
  const n = toNumber(bytesPerSecond);
  if (!Number.isFinite(n) || n <= 0) return null;
  return `${formatFileSize(n)}/s`;
}

/**
 * 进度百分比，0–100 的整数。
 *
 * **上限夹到 100**：`transferred > total` 只可能出现在统计出错时，而一个显示 137% 的进度条
 * 比一个停在 100% 的更难解释。收口前桌面与 Web 都没有这个夹取，移动端有——取移动端的。
 */
export function calcPercent(done: ByteCount, total: ByteCount): number {
  const totalNumber = toNumber(total);
  if (!(totalNumber > 0)) return 0;
  return Math.min(100, Math.round((100 * toNumber(done)) / totalNumber));
}
