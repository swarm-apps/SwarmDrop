/**
 * 接收策略徽章
 *
 * 统一「是否显示 + 策略动作翻译 + Shield 图标」逻辑，供 Offer 弹窗与历史记录复用：
 * - variant="offer"：Offer 弹窗的胶囊样式，只要有 reason 就展示原因（不带动作标签）
 * - variant="history"：历史记录的行内样式，仅在 auto_accept / reject 时展示「动作：原因」
 */

import { Shield } from "lucide-react";
import { t } from "@lingui/core/macro";
import { cn } from "@/lib/utils";

/** 把策略动作标识翻译成文案。 */
export function policyActionLabel(policyAction: string | null): string {
  if (policyAction === "auto_accept") return t`自动接收`;
  if (policyAction === "reject") return t`策略拒绝`;
  return t`接收策略`;
}

/** 历史记录场景下：仅当存在原因且动作为 auto_accept / reject 时才展示。 */
function isPolicyActionDecided(policyAction: string | null): boolean {
  return policyAction === "auto_accept" || policyAction === "reject";
}

export function PolicyReasonBadge({
  policyAction,
  policyReason,
  variant = "history",
  className,
}: {
  policyAction: string | null;
  policyReason: string | null;
  variant?: "offer" | "history";
  className?: string;
}) {
  if (!policyReason) return null;
  if (variant === "history" && !isPolicyActionDecided(policyAction)) return null;

  if (variant === "offer") {
    return (
      <div
        className={cn(
          "inline-flex max-w-full items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-xs text-muted-foreground",
          className,
        )}
      >
        <Shield className="size-3.5 shrink-0" />
        <span className="truncate">{policyReason}</span>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground md:text-[12px]",
        className,
      )}
    >
      <Shield className="size-3.5 shrink-0" />
      <span className="truncate">
        {policyActionLabel(policyAction)}：{policyReason}
      </span>
    </div>
  );
}
