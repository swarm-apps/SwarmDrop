/**
 * 配对指引步骤列表。
 *
 * 编号在这里承载真实顺序（对方先做什么、再做什么），不是装饰性的分节标记——
 * 配对是双设备协作，只说「安全」不说「怎么做」的话，用户会卡在第二台设备上。
 */

import type { ReactNode } from "react";

export function PairingSteps({ steps }: { steps: ReactNode[] }) {
  return (
    <ol className="grid gap-2.5">
      {steps.map((step, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: 静态步骤列表，无重排
        <li key={index} className="flex gap-2.5 text-left">
          <span className="mt-px flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 font-mono text-[11px] font-medium text-brand tabular-nums">
            {index + 1}
          </span>
          <span className="text-[13px] leading-5 text-pretty text-muted-foreground">
            {step}
          </span>
        </li>
      ))}
    </ol>
  );
}
