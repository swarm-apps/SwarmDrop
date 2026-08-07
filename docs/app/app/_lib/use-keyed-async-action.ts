// 收敛「N 个列表项各自独立可操作」的样板：pending 键集合、逐键错误、finally 里清 pending。
// 与 useAsyncAction 的区别在维度——那个管单个动作实例（用 seq 丢弃过期结果），这个按 key
// 管并发的多个（配对请求逐条确认、收件箱逐文件下载、活动列表逐会话续传）。
//
// 两种错误展示位都给：`errorFor(key)` 供逐项展示（下载 / 续传），`latestError` 供列表外
// 单一错误卡片展示（配对确认 / offer 决策）——后者与被替换的手写实现同语义：每次发起清空、
// 失败时覆盖。

import { useCallback, useMemo, useState } from "react";
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

  // **返回值本身要 memo**。调用方拿它派生集合（收件箱按待处理的 key 算 `pendingIds`），
  // 每次渲染换一个新对象就等于宣告「什么都变了」，那些派生只能跟着每帧重算，
  // 下游行组件的 memo 也一起被打穿。三个字段真正变化的时机是 pending / errors 变化时。
  return useMemo(
    () => ({
      isPending: (key: string) => pendingKeys.has(key),
      errorFor: (key: string): WebError | undefined => errors[key],
      /** 原始的待处理键集合，供调用方自己做派生（比逐个 `isPending` 快，也能进依赖）。 */
      pendingKeys,
      latestError,
      run,
    }),
    [pendingKeys, errors, latestError, run],
  );
}
