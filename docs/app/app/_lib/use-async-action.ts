// 收敛 connect/reserve/consume-invite 三处重复的样板：pending 状态、错误统一走
// toWebError()、seq 计数器丢弃过期结果（await 期间可能发起新一轮调用，旧结果不该覆盖新状态）。
// 不适用于「响应入站配对请求」——那是 N 个列表项各自独立可操作，不是单个动作实例。

import { useRef, useState } from "react";
import { toWebError, type WebError } from "./view-types";

export function useAsyncAction() {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<WebError | null>(null);
  const seq = useRef(0);

  /**
   * `onSuccess` 可省：动作的全部结果已经由 store 表达（如节点启停）时，调用方没有额外要做的
   * ——此时传一个空函数只是噪音。绝大多数调用点仍然需要它（把返回值落进 store）。
   *
   * `onSettled` 是「无论成败都要收的尾」，成功与失败**两条路都会调**（成功时在 `onSuccess`
   * 之后）。需要它是因为把清理写进 `onSuccess` 会漏掉失败路径，而那正是清理最要紧的时候
   * ——发送准备失败后不收掉进度行，它会永久停在半路。
   */
  function run<T>(
    fn: () => Promise<T>,
    onSuccess?: (value: T) => void,
    onSettled?: () => void,
  ) {
    const mySeq = ++seq.current;
    setPending(true);
    setError(null);
    fn().then(
      (value) => {
        if (mySeq !== seq.current) return;
        onSuccess?.(value);
        onSettled?.();
        setPending(false);
      },
      (e) => {
        if (mySeq !== seq.current) return;
        setError(toWebError(e));
        onSettled?.();
        setPending(false);
      },
    );
  }

  /**
   * 作废在途结果并清空 pending / error。
   *
   * `run` 的 seq 只覆盖「新一轮顶掉旧一轮」，**不覆盖「转入无动作态」**——调用方若在不发起
   * 新调用的情况下退出（检索框被清空、面板收起），旧 promise resolve 时 `mySeq` 仍等于
   * `seq.current`，会把过期结果写回一个已经不该显示它的界面。那条路径必须显式作废。
   */
  function cancel() {
    seq.current++;
    setPending(false);
    setError(null);
  }

  return { pending, error, run, cancel };
}
