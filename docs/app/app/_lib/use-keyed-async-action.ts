// 收敛「N 个列表项各自独立可操作」的样板：pending 键集合、逐键错误、finally 里清 pending。
// 与 useAsyncAction 的区别在维度——那个管单个动作实例（用 seq 丢弃过期结果），这个按 key
// 管并发的多个（配对请求逐条确认、收件箱逐文件下载、活动列表逐会话续传）。
//
// 两种错误展示位都给：`errorFor(key)` 供逐项展示（下载 / 续传），`latestError` 供列表外
// 单一错误卡片展示（配对确认 / offer 决策）——后者与被替换的手写实现同语义：每次发起清空、
// 失败时覆盖。

import { useCallback, useState } from "react";
import { toWebError, type WebError } from "./view-types";

export function useKeyedAsyncAction() {
  const [pendingKeys, setPendingKeys] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, WebError>>({});
  const [latestError, setLatestError] = useState<WebError | null>(null);

  // 全程函数式 setState，故依赖为空——引用稳定，调用方可以安全地把它包进自己的 useCallback，
  // 让 React.memo 的列表项不因每次渲染新建回调而失效。
  const run = useCallback(async (key: string, fn: () => Promise<void>) => {
    setPendingKeys((prev) => new Set(prev).add(key));
    setLatestError(null);
    setErrors((prev) => {
      if (!(key in prev)) return prev;
      const next = { ...prev };
      delete next[key];
      return next;
    });
    try {
      await fn();
    } catch (e) {
      const error = toWebError(e);
      setErrors((prev) => ({ ...prev, [key]: error }));
      setLatestError(error);
    } finally {
      setPendingKeys((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  }, []);

  return {
    isPending: (key: string) => pendingKeys.has(key),
    errorFor: (key: string): WebError | undefined => errors[key],
    latestError,
    run,
  };
}
