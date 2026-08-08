/**
 * 诊断层里「一条基础设施关系」的两个共享零件。
 *
 * 节点状态面（`node-status-sheet.tsx`）与设置页的可编辑版（`-bootstrap-nodes-section.tsx`）
 * 渲染的是同一条 `InfraLink`，此前各写各的：一处只画来源徽标、另一处的复制按钮借用了父组件
 * 的「引导节点地址已复制」toast——复制的内容对，确认语说的却是另一件事。合成两个零件后，
 * 归因的组成与复制的确认语都只有一处定义。
 */

import { useLingui } from "@lingui/react/macro";
import { Copy } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { copyText } from "@/lib/clipboard";
import {
  INFRA_ROLE_LABEL,
  INFRA_SCOPE_LABEL,
  INFRA_SOURCE_LABEL,
} from "@/lib/node-status";
import { cn } from "@/lib/utils";
import type { InfraLink } from "@/lib/bindings";

/**
 * 归因行：**来源 · 范围 · 角色**（`DESIGN.md` 的 Node Status Contract 逐字要求这三项）。
 *
 * 少一项就丢可判别性：只画来源时，被 `CandidateScope::infer` 判成 `Lan` 的手填节点与
 * `Public` 的长得一模一样，而 scope 正是 `configuredLanOnly` 那一档的判据；`kadServer`
 * 则完全不可见。
 */
export function InfraLinkAttribution({
  link,
  className,
}: {
  link: InfraLink;
  className?: string;
}) {
  const { t } = useLingui();
  // 两个角色正交，同一条关系可以既是 DHT 种子又是中继，所以是列举不是二选一。
  const roles = [
    link.roles.kadServer ? t(INFRA_ROLE_LABEL.kadServer) : null,
    link.roles.relayServer ? t(INFRA_ROLE_LABEL.relayServer) : null,
  ].filter((role): role is string => role !== null);
  const parts = [
    ...link.sources.map((source) => t(INFRA_SOURCE_LABEL[source])),
    t(INFRA_SCOPE_LABEL[link.scope]),
    ...roles,
  ];

  return (
    <span
      data-testid="infra-link-attribution"
      className={cn("truncate text-[11px] text-muted-foreground", className)}
    >
      {parts.join(" · ")}
    </span>
  );
}

/**
 * 原样的 `lastError` + 复制按钮。
 *
 * **不翻译**：排查时用户要贴进 issue、跟日志比对的就是这一句。复制的确认语必须说的是
 * 「错误信息」——借用别处的 toast 文案等于告诉用户他复制到了另一样东西。
 */
export function InfraLinkError({ detail }: { detail: string }) {
  const { t } = useLingui();
  return (
    <div className="flex items-start gap-2 rounded-lg bg-muted/60 px-2 py-1.5">
      <code className="min-w-0 flex-1 break-all font-mono text-[11px] leading-relaxed text-muted-foreground">
        {detail}
      </code>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t`复制错误信息`}
        title={t`复制错误信息`}
        className="size-6 shrink-0 text-muted-foreground"
        onClick={() => {
          copyText(detail).then(
            () => toast.success(t`错误信息已复制`),
            () => toast.error(t`复制失败`),
          );
        }}
      >
        <Copy className="size-3" />
      </Button>
    </div>
  );
}
