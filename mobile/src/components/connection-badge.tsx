import { Trans } from "@lingui/react/macro";
import { RadioTower, Wifi, Zap } from "lucide-react-native";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

type ConnectionKind = "lan" | "dcutr" | "relay";

/** tone 对应设计系统语义 token,图标色在组件内从 useThemeColors 取(随暗色切换,不硬编码 hex)。 */
const CONNECTION_META: Record<
  ConnectionKind,
  {
    icon: typeof Wifi;
    tone: "success" | "primary" | "warning";
    bg: string;
    text: string;
    label: () => React.ReactNode;
  }
> = {
  lan: {
    icon: Wifi,
    tone: "success",
    bg: "bg-success/10",
    text: "text-success",
    label: () => <Trans>局域网</Trans>,
  },
  dcutr: {
    icon: Zap,
    tone: "primary",
    bg: "bg-primary/10",
    text: "text-primary",
    label: () => <Trans>打洞</Trans>,
  },
  relay: {
    icon: RadioTower,
    tone: "warning",
    bg: "bg-warning/15",
    text: "text-warning",
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
  const colors = useThemeColors();
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
      <Icon size={compact ? 11 : 12} color={colors[meta.tone]} />
      <Text className={cn("text-[10px] font-medium", meta.text)}>
        <Label />
      </Text>
      {latency != null && Number.isFinite(latency) ? (
        <Text className={cn("text-[10px]", meta.text)}>{latency}ms</Text>
      ) : null}
    </View>
  );
}
