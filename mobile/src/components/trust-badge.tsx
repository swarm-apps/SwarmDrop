import { Trans } from "@lingui/react/macro";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import type { TrustLevel } from "@/core/device-trust";
import { cn } from "@/lib/utils";

interface TrustBadgeProps {
  level: TrustLevel;
  compact?: boolean;
  /** 信任是否已双向确认。`false` 时追加「· 待确认」标记;`undefined/null/true` 不显示。 */
  confirmed?: boolean | null;
}

export function TrustBadge({ level, compact, confirmed }: TrustBadgeProps) {
  const meta = TRUST_META[level];
  return (
    <View
      className={cn(
        "self-start rounded-full",
        compact ? "px-2 py-0.5" : "px-2.5 py-1",
        meta.bg,
      )}
    >
      <Text
        className={cn(
          "font-medium",
          compact ? "text-[11px]" : "text-[12px]",
          meta.text,
        )}
      >
        <TrustLabel level={level} />
        {confirmed === false ? (
          <>
            {" · "}
            <Trans>待确认</Trans>
          </>
        ) : null}
      </Text>
    </View>
  );
}

export function TrustLabel({ level }: { level: TrustLevel }) {
  switch (level) {
    case "owned":
      return <Trans>本人设备</Trans>;
    case "temporary":
      return <Trans>临时设备</Trans>;
    case "blocked":
      return <Trans>已阻止</Trans>;
    case "collaborator":
      return <Trans>协作设备</Trans>;
    default:
      return <Trans>协作设备</Trans>;
  }
}

const TRUST_META: Record<TrustLevel, { bg: string; text: string }> = {
  owned: { bg: "bg-primary/10", text: "text-primary-ink" },
  collaborator: { bg: "bg-success/10", text: "text-success-ink" },
  temporary: { bg: "bg-warning/15", text: "text-warning-ink" },
  blocked: { bg: "bg-destructive/15", text: "text-destructive-ink" },
};
