"use client";

// 非 secure context 预警横幅：接收落盘会失败（OPFS / WebCrypto 缺失）。amber 是警告级语义色。
// 只在客户端探测到真值且不达标时出现——不在 SSR 预渲染 HTML 里闪错误警告。

import { isReceiveCapable } from "../_lib/secure-context";
import { useWebNode } from "../_lib/store";

export function SecureContextBanner() {
  const secure = useWebNode((s) => s.secure);
  if (!secure || isReceiveCapable(secure)) return null;

  return (
    <div
      role="alert"
      className="mx-auto mt-4 w-full max-w-3xl rounded-lg border border-amber-500/40 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-500/30 dark:bg-amber-950/40 dark:text-amber-200"
    >
      <p className="font-medium">当前非 secure context，接收文件会失败。</p>
      <p className="mt-1 text-amber-800/90 dark:text-amber-200/80">
        浏览器在此环境不提供 <code className="font-mono">navigator.storage</code> /{" "}
        <code className="font-mono">crypto.subtle</code>，接收方落盘无法完成。请改用{" "}
        <span className="font-mono">https</span>、<span className="font-mono">http://localhost</span>{" "}
        或 <span className="font-mono">http://127.0.0.1</span> 访问，不要用 http 私网 IP。
      </p>
    </div>
  );
}
