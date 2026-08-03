"use client";

// 相对时间（「刚刚」「3 分钟前」）。
//
// **刻意不进 `@swarmdrop/shared-view`**：输出是本地化文案，而三端各说各的（桌面
// `src/lib/format.ts` 的 `formatRelativeTime` 硬编码中文、移动端返回 `<Trans>` 节点）。
// 收进共享包就必须改掉其中一端的渲染输出——正是那个包 README「归属判据 3」判出去的形状。
//
// 写成组件而不是 `_lib/` 的纯函数，是因为复数形式只有翻译宏表达得了，而宏只在组件里展开
// （`_lib/` 下一律只能存描述符，见知识库「Lingui 接 Next」第 3 条）。
//
// **预渲染不会撞上它**：时间戳全部来自运行时 store（传输投影 / 收件箱），构建期一条都没有，
// 所以静态导出的 HTML 里不含本组件的输出，`useNowSeconds` 的构建期初值也就不会与客户端
// 首帧对不上。将来若有构建期就有内容的调用点，这条要重新考虑。

import { Plural, Trans } from "@lingui/react/macro";
import { useMemo } from "react";
import { useNowSeconds } from "../_lib/use-now-seconds";

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function RelativeTime({
  /** Unix 毫秒（内核下发的时间戳单位，与 `projection.finishedAt` 等一致）。 */
  timestamp,
  className,
}: {
  timestamp: number;
  className?: string;
}) {
  // 节拍是全进程共享的：同一屏里每一行读同一个 now，不会各自建一个定时器，也不会出现
  // 相邻两行一个说「1 分钟前」一个还冻在「刚刚」（见 use-now-seconds 的说明）。
  const now = useNowSeconds();
  const elapsed = Math.max(0, now - Math.floor(timestamp / 1000));

  // `title` 给绝对时刻——相对时间便于扫读，但排查问题时要的是准确到分钟的那个值。
  // 两者都在，谁也不用为对方让位。
  //
  // 缓存住：`toLocaleString` 要过一趟 Intl，而本组件跟着所在的行重渲染——那一行可能正被
  // 每秒十余次的进度事件推着走，而这两个字符串只随 `timestamp` 变。
  const absolute = useMemo(
    () => {
      const date = new Date(timestamp);
      return { iso: date.toISOString(), local: date.toLocaleString() };
    },
    [timestamp],
  );

  return (
    <time dateTime={absolute.iso} title={absolute.local} className={className}>
      {elapsed < MINUTE ? (
        <Trans>刚刚</Trans>
      ) : elapsed < HOUR ? (
        <Plural value={Math.floor(elapsed / MINUTE)} other="# 分钟前" />
      ) : elapsed < DAY ? (
        <Plural value={Math.floor(elapsed / HOUR)} other="# 小时前" />
      ) : (
        <Plural value={Math.floor(elapsed / DAY)} other="# 天前" />
      )}
    </time>
  );
}
