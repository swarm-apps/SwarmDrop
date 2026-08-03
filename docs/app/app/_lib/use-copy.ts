"use client";

import { useEffect, useRef, useState } from "react";

export type CopyState = "idle" | "copied" | "failed";

/**
 * 复制到剪贴板 + 一次性结果提示。
 *
 * 应用区没有 toast 系统，结果只能就地显示在按钮上，于是每个复制入口都要重复
 * 同一串取舍。抽在这里，它们只写一次：
 *
 * 1. **同步失败也要接住**。非安全上下文下 `navigator.clipboard` 直接是 `undefined`，
 *    取 `.writeText` 就是一次同步 TypeError——挂在 promise 上的 onRejected 根本
 *    轮不到，按钮会一动不动地骗人。而「非安全上下文」恰是复制不可用最常见的成因。
 * 2. **只有成功态自动收回**。失败提示 2 秒后自己消失等于没提示，留到下次点击时覆盖。
 * 3. **计时器放在 handler 里，不放 effect**。依赖 state 的 effect 在连点时写入同值
 *    会被 React bail out，那 2 秒不会重新计时；连点必须顶掉上一个 timer，否则前一次
 *    的到点会把这一次的确认提前掐掉。
 */
/** 成功确认自动收回的时长。两个调用点都用它——需要各自不同再加参数。 */
const RESET_MS = 2000;

export function useCopyToClipboard() {
  const [state, setState] = useState<CopyState>("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  // 组件在确认还挂着时卸载，定时器要一起走
  useEffect(() => () => clearTimeout(resetTimer.current), []);

  const copy = async (text: string) => {
    clearTimeout(resetTimer.current);
    try {
      await navigator.clipboard.writeText(text);
      setState("copied");
      resetTimer.current = setTimeout(() => setState("idle"), RESET_MS);
    } catch {
      setState("failed");
    }
  };

  return { state, copy };
}
