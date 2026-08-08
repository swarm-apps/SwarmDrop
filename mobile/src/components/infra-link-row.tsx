import { useLingui } from "@lingui/react/macro";
import type { InfraLinkPresentation } from "@swarmdrop/shared-view";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { Copy } from "lucide-react-native";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import { candidateSourceKey } from "@/core/network-discovery";
import {
  CANDIDATE_SCOPE_LABEL,
  CANDIDATE_SOURCE_LABEL,
  INFRA_LINK_LABEL,
  INFRA_ROLE_LABEL,
  TONE_DOT_CLASS,
  TONE_TEXT_CLASS,
} from "@/core/network-labels";
import type { InfraLinkRow } from "@/hooks/use-node-health";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";

/** 地址太长时中间省略，但复制拿到的始终是完整串。 */
export function truncateAddr(addr: string): string {
  if (addr.length <= 46) return addr;
  const p2pIdx = addr.indexOf("/p2p/");
  if (p2pIdx === -1) return `${addr.slice(0, 24)}…${addr.slice(-14)}`;
  const prefix = addr.slice(0, Math.min(p2pIdx, 24));
  const peerId = addr.slice(p2pIdx + 5);
  const shortPeerId =
    peerId.length > 12 ? `${peerId.slice(0, 6)}…${peerId.slice(-6)}` : peerId;
  return `${prefix}/p2p/${shortPeerId}`;
}

/** 状态点 **加词**——契约里光一个色点不算数。 */
export function InfraLinkStatusChip({
  presentation,
}: {
  presentation: InfraLinkPresentation;
}) {
  const { t } = useLingui();
  return (
    <View className="flex-row items-center gap-2">
      <View
        className={cn("size-2 rounded-full", TONE_DOT_CLASS[presentation.tone])}
      />
      <Text
        className={cn(
          "text-[13px] font-medium",
          TONE_TEXT_CLASS[presentation.tone],
        )}
      >
        {t(INFRA_LINK_LABEL[presentation.state])}
      </Text>
    </View>
  );
}

/**
 * 归因（来源 · 范围 · 角色）与 **`lastError` 原文 + 复制按钮**。
 *
 * 原文**不翻译**——排查时用户要贴进 issue 的就是这一句，翻过的串贴过去没人认得；
 * 也必须可复制，否则就是一段长得像可点、点了没反应的文本。
 */
export function InfraLinkDetail({ row }: { row: InfraLinkRow }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const { link, presentation } = row;

  const attribution = [
    ...link.sources.map((source) =>
      t(CANDIDATE_SOURCE_LABEL[candidateSourceKey(source)]),
    ),
    t(CANDIDATE_SCOPE_LABEL[row.view.scope]),
    ...(link.roles.kadServer ? [t(INFRA_ROLE_LABEL.kadServer)] : []),
    ...(link.roles.relayServer ? [t(INFRA_ROLE_LABEL.relayServer)] : []),
  ].join(" · ");

  const copyError = async () => {
    if (!presentation.detail) return;
    await Clipboard.setStringAsync(presentation.detail);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制错误详情`);
  };

  return (
    <View className="gap-1.5">
      <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
        {attribution}
      </Text>
      {presentation.detail ? (
        <Pressable
          onPress={() => void copyError()}
          accessibilityRole="button"
          accessibilityLabel={t`复制错误详情`}
          className="flex-row items-start gap-2 rounded-lg bg-muted px-2.5 py-2 active:opacity-70"
        >
          <Text className="flex-1 font-mono text-[11px] text-muted-foreground">
            {presentation.detail}
          </Text>
          <Copy size={13} color={colors.mutedForeground} />
        </Pressable>
      ) : null}
    </View>
  );
}

/**
 * 诊断层里的一条基础设施关系（单地址形态）。
 *
 * 契约要求的三样东西齐全：状态词、归因、`lastError` 原文 + 复制。引导节点管理页
 * 需要一个节点挂多条地址，那边自己拼 [`InfraLinkStatusChip`] + [`InfraLinkDetail`]。
 */
export function InfraLinkRowView({
  row,
  addr,
  badge,
  action,
}: {
  row: InfraLinkRow;
  /** 覆盖显示的地址；缺省取内核记着的第一条。 */
  addr?: string;
  /** 右上角标记（如「默认」）。 */
  badge?: React.ReactNode;
  /** 右上角动作（移除入口）。调用点自己按 `row.link.removable` 门控。 */
  action?: React.ReactNode;
}) {
  const { link } = row;
  const shown = addr ?? link.addrs[0] ?? link.peerId;

  return (
    <View className="gap-1.5 px-3.5 py-3">
      <View className="flex-row items-center gap-2">
        <InfraLinkStatusChip presentation={row.presentation} />
        <View className="flex-1" />
        {badge}
        {action}
      </View>
      <Text
        className="font-mono text-[12px] text-muted-foreground"
        numberOfLines={1}
      >
        {truncateAddr(shown)}
      </Text>
      <InfraLinkDetail row={row} />
    </View>
  );
}
