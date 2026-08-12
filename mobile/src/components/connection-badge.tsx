import { Trans } from "@lingui/react/macro";
import { transportLabel } from "@swarmdrop/shared-view";
import { ArrowLeftRight, RadioTower, Wifi, Zap } from "lucide-react-native";
import { View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";

/**
 * tone 对应设计系统语义 token,图标色在组件内从 useThemeColors 取(随暗色切换,不硬编码 hex)。
 *
 * 三色与桌面 / Web 同一套(DESIGN.md 的 One Accent Rule 把连接方式列为例外并点名 sky):
 * 局域网 success · 直连与打洞 info(sky) · 中继 warning。打洞此前借用 primary,
 * 于是同一枚徽标在这里是青绿、在另两端是天蓝——而青绿是品牌色,借出去就多了一个含义。
 *
 * `direct` 与 `dcutr` 共用 info 色、只靠图标与词区分——两者为什么不能合并、
 * 为什么不各占一个色相,见 DESIGN.md 的 Slot 6 vocabulary 与 crates/host 的
 * `ConnectionType`。
 */
/**
 * 变体清单写两处(这个 union 与表的键)是**刻意的**:两处互相校验。
 *
 * 曾经试过只留表、用 `keyof typeof` 派生 union,那会把最后一道 key 名检查也丢掉——
 * `satisfies Record<string, …>` 只校验值的形状,把 `dcutr` 敲成 `dcurt` 照样编译通过,
 * union 跟着变宽,于是每台打洞设备的徽标静默消失。`Record<ConnectionKind, …>` 则要求
 * 键**恰好**是这四个:少一个、多一个、拼错一个都当场报错。
 */
type ConnectionKind = "lan" | "direct" | "dcutr" | "relay";

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
  direct: {
    icon: ArrowLeftRight,
    tone: "info",
    bg: "bg-info/12",
    text: "text-info-ink",
    label: () => <Trans>直连</Trans>,
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

/**
 * 把 core 的连接类型字符串收敛成已知枚举;未知返回 null。
 *
 * ⚠️ **本端是三端里唯一不会因为漏改而编译失败的那个。** 另两端吃 specta /
 * wasm-bindgen 生成的联合类型,`Record` 缺 key 编译期就报;这里隔着 uniffi 的
 * `Option<String>`(`mobile-core/src/device.rs`),未知值只能在运行时收成 `null`,
 * 表现是那种连接的设备卡上整枚徽标消失。所以 core 侧加变体时**必须回来加一格**
 * ——加在 `CONNECTION_META` 里就够了,清单与判定都从它派生。
 *
 * 用 `Object.hasOwn` 而不是 `in`:后者会把 `"toString"` 这类原型键判成真。
 */
export function normalizeConnectionKind(
  connection?: string | null,
): ConnectionKind | null {
  return connection && Object.hasOwn(CONNECTION_META, connection)
    ? (connection as ConnectionKind)
    : null;
}

/**
 * 设备连接质量徽标:把 core 的 lan/direct/dcutr/relay 连接类型映射成本地化的
 * 局域网 / 直连 / 打洞 / 中继 彩色徽标,并可选地附带传输协议与测得的延迟(latencyMs 为 bigint)。
 * 连接类型未知时返回 null(交由调用方决定是否回退到「等待发现」)。
 *
 * ⚠️ 这里的枚举要与 `CONNECTION_META` 同步——本端是三端里唯一「漏一个变体不会编译失败、
 * 只是徽标静默消失」的那个,一份写少了的清单正是让人误以为表已完整的东西。
 *
 * `transport` 是专有名词(TCP/QUIC/WebRTC/WebTransport),三端同一份映射且刻意不进翻译
 * catalog——用户拿它去搜索、比对日志。完整链路证据在设备详情页的「链路详情」区块。
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
