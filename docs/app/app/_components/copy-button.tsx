"use client";

// 「复制这串机器真值」的按钮。节点 ID、circuit 可达地址这类东西是用来跟对方核对、贴进 issue 的
// ——桌面点一下就有，这边不能只让用户手动选中一串 52 字符的 base58。
//
// ## 自带状态，换代靠 `key`
//
// 复制态（`copied`）住在**本组件**里，所以 `key={value}` 才真的能重置它。若状态住在父组件、
// key 挂在子 `<button>` 的 DOM 节点上，换代什么也重置不了：按钮重新挂载，state 原封不动
// （知识库「复制态的换代 key 必须挂在持有状态的组件」）。
//
// 具体到可达地址：连接从 relay 升级成直连后地址会换，而按钮还挂着上一条的「已复制」——
// 用户照着粘出去的是一条已经不在用的地址。
//
// ## 为什么应用区还有另外两个复制按钮
//
// `connection-badge.tsx` 与 `invite-share.tsx` 各有一个，它们**不该合并进来**：那两处是
// 全宽的边框按钮、长在各自的卡片版式里，形态由所在布局决定。合并它们需要给本组件加
// variant 参数，那是为了「看起来只有一份」而造的抽象。复制**逻辑**本来就已经共享了
// （`_lib/use-copy.ts`），共享的边界到此为止。

import { Trans } from "@lingui/react/macro";
import { Check, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useCopyToClipboard } from "../_lib/use-copy";

export function CopyButton({
  value,
  label,
  className = "",
}: {
  value: string;
  /** 可访问名（调用方 `t` 展开后的串）。可见文本只有「复制」，读屏要听得出复制的是什么。 */
  label: string;
  className?: string;
}) {
  const { state, copy } = useCopyToClipboard();

  return (
    <Button
      size="sm"
      variant="ghost"
      onClick={() => void copy(value)}
      aria-label={label}
      title={label}
      className={`shrink-0 gap-1.5 text-xs ${className}`}
    >
      {state === "copied" ? (
        <Check className="size-3.5" aria-hidden />
      ) : (
        <Copy className="size-3.5" aria-hidden />
      )}
      {state === "copied" ? (
        <Trans>已复制</Trans>
      ) : state === "failed" ? (
        <Trans>复制失败</Trans>
      ) : (
        <Trans>复制</Trans>
      )}
    </Button>
  );
}
