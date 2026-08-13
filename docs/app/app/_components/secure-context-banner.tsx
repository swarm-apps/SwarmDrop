"use client";

// 非 secure context 预警横幅：接收落盘会失败（OPFS / WebCrypto 缺失）。amber 是警告级语义色。
// 只在客户端探测到真值且不达标时出现——不在 SSR 预渲染 HTML 里闪错误警告。

import { Trans } from "@lingui/react/macro";
import { isReceiveCapable } from "../_lib/secure-context";
import { useWebNode } from "../_lib/store";

export function SecureContextBanner() {
  const secure = useWebNode((s) => s.secure);
  if (!secure || isReceiveCapable(secure)) return null;

  return (
    <div
      role="alert"
      className="rounded-lg border border-warning/40 bg-warning/12 px-4 py-3 text-sm text-warning-ink"
    >
      <p className="font-medium">
        <Trans>当前非 secure context，接收文件会失败。</Trans>
      </p>
      <p className="mt-1 text-warning-ink/90">
        {/* API 名与 URL scheme 是机器值，留在句子里由译者调整位置即可，不单独抽成占位。 */}
        <Trans>
          浏览器在此环境不提供 <code className="font-mono">navigator.storage</code> /{" "}
          <code className="font-mono">crypto.subtle</code>，接收方落盘无法完成。请改用{" "}
          <span className="font-mono">https</span>、
          <span className="font-mono">http://localhost</span> 或{" "}
          <span className="font-mono">http://127.0.0.1</span> 访问，不要用 http 私网 IP。
        </Trans>
      </p>
    </div>
  );
}
