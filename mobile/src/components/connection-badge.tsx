import { Trans } from "@lingui/react/macro";
import { RadioTower, Wifi, Zap } from "lucide-react-native";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { cn } from "@/lib/utils";

type ConnectionKind = "lan" | "dcutr" | "relay";

const CONNECTION_META: Record<
  ConnectionKind,
  {
    icon: typeof Wifi;
    iconColor: string;
    bg: string;
    text: string;
    label: () => React.ReactNode;
  }
> = {
  lan: {
    icon: Wifi,
    iconColor: "#22c55e",
    bg: "bg-success/10",
    text: "text-success",
    label: () => <Trans>局域网</Trans>,
  },
  dcutr: {
    icon: Zap,
    iconColor: "#3b82f6",
    bg: "bg-primary/10",
    text: "text-primary",
    label: () => <Trans>打洞</Trans>,
  },
  relay: {
    icon: RadioTower,
    iconColor: "#f59e0b",
    bg: "bg-yellow-500/15",
    text: "text-yellow-600 dark:text-yellow-400",
    label: () => <Trans>中继</Trans>,
  },
};

/** 把 core 的连接类型字符串收敛成已知枚举;未知返回 null。 */
export function normalizeConnectionKind(
  connection?: string | null,
): ConnectionKind | null {
  switch (connection) {
    case "lan":
    case "dcutr":
    case "relay":
      return connection;
    default:
      return null;
  }
}

/**
 * 设备连接质量徽标:把 core 的 lan/dcutr/relay 连接类型映射成本地化的
 * 局域网 / 打洞 / 中继 彩色徽标,并可选地附带测得的延迟(latencyMs 为 bigint)。
 * 连接类型未知时返回 null(交由调用方决定是否回退到「等待发现」)。
 */
export function ConnectionBadge({
  connection,
  latencyMs,
  compact,
}: {
  connection?: string | null;
  latencyMs?: bigint | null;
  compact?: boolean;
}) {
  const kind = normalizeConnectionKind(connection);
  if (!kind) return null;
  const meta = CONNECTION_META[kind];
  const Icon = meta.icon;
  const latency = latencyMs != null ? Number(latencyMs) : null;
  const Label = meta.label;

  return (
    <View
      className={cn(
        "flex-row items-center gap-1 self-start rounded-full",
        compact ? "px-1.5 py-0.5" : "px-2 py-0.5",
        meta.bg,
      )}
    >
      <Icon size={compact ? 11 : 12} color={meta.iconColor} />
      <Text className={cn("text-[10px] font-medium", meta.text)}>
        <Label />
      </Text>
      {latency != null && Number.isFinite(latency) ? (
        <Text className={cn("text-[10px]", meta.text)}>{latency}ms</Text>
      ) : null}
    </View>
  );
}
