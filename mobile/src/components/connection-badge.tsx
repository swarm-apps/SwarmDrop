import { Trans } from "@lingui/react/macro";
import { transportLabel } from "@swarmdrop/shared-view";
import { RadioTower, Wifi, Zap } from "lucide-react-native";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

type ConnectionKind = "lan" | "dcutr" | "relay";

/**
 * tone 对应设计系统语义 token,图标色在组件内从 useThemeColors 取(随暗色切换,不硬编码 hex)。
 *
 * 三色与桌面 / Web 同一套(DESIGN.md 的 One Accent Rule 把连接方式列为例外并点名 sky):
 * 局域网 success · 打洞 info(sky) · 中继 warning。打洞此前借用 primary,
 * 于是同一枚徽标在这里是青绿、在另两端是天蓝——而青绿是品牌色,借出去就多了一个含义。
 */
const CONNECTION_META: Record<
  ConnectionKind,
  {
    icon: typeof Wifi;
    tone: "success" | "info" | "warning";
    bg: string;
    text: string;
    label: () => React.ReactNode;
  }
> = {
  lan: {
    icon: Wifi,
    tone: "success",
    bg: "bg-success/10",
    text: "text-success-ink",
    label: () => <Trans>局域网</Trans>,
  },
  dcutr: {
    icon: Zap,
    tone: "info",
    bg: "bg-info/12",
    text: "text-info-ink",
    label: () => <Trans>打洞</Trans>,
  },
  relay: {
    icon: RadioTower,
    tone: "warning",
    bg: "bg-warning/15",
    text: "text-warning-ink",
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
 * 局域网 / 打洞 / 中继 彩色徽标,并可选地附带传输协议与测得的延迟(latencyMs 为 bigint)。
 * 连接类型未知时返回 null(交由调用方决定是否回退到「等待发现」)。
 *
 * `transport` 是专有名词(TCP/QUIC/WebRTC),三端同一份映射且刻意不进翻译 catalog
 * ——用户拿它去搜索、比对日志。完整链路证据在设备详情页的「链路详情」区块。
 */
export function ConnectionBadge({
  connection,
  transport,
  latencyMs,
  compact,
}: {
  connection?: string | null;
  transport?: string | null;
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
  const transportName = transportLabel(transport);

  return (
    <View
      className={cn(
        "flex-row items-center gap-1 self-start rounded-full",
        compact ? "px-1.5 py-0.5" : "px-2 py-0.5",
        meta.bg,
      )}
    >
      <Icon size={compact ? 11 : 12} color={colors[meta.tone]} />
      <Text className={cn("text-[11px] font-medium", meta.text)}>
        <Label />
      </Text>
      {transportName ? (
        <Text className={cn("text-[11px] opacity-70", meta.text)}>
          {transportName}
        </Text>
      ) : null}
      {latency != null && Number.isFinite(latency) ? (
        <Text className={cn("text-[11px]", meta.text)}>{latency}ms</Text>
      ) : null}
    </View>
  );
}
