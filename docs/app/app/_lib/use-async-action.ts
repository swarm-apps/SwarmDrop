// 收敛 connect/reserve/consume-invite 三处重复的样板：pending 状态、错误统一走
// toWebError()、seq 计数器丢弃过期结果（await 期间可能发起新一轮调用，旧结果不该覆盖新状态）。
// 不适用于「响应入站配对请求」——那是 N 个列表项各自独立可操作，不是单个动作实例。

import { useRef, useState } from "react";
import { toWebError, type WebError } from "./view-types";

export function useAsyncAction() {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<WebError | null>(null);
  const seq = useRef(0);

  function run<T>(fn: () => Promise<T>, onSuccess: (value: T) => void) {
    const mySeq = ++seq.current;
    setPending(true);
    setError(null);
    fn().then(
      (value) => {
        if (mySeq !== seq.current) return;
        onSuccess(value);
        setPending(false);
      },
      (e) => {
        if (mySeq !== seq.current) return;
        setError(toWebError(e));
        setPending(false);
      },
    );
  }

  return { pending, error, run };
}
