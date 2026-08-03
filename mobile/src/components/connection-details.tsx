import { Trans, useLingui } from "@lingui/react/macro";
import { transportLabel } from "@swarmdrop/shared-view";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { ChevronDown, ChevronRight, Copy } from "lucide-react-native";
import { useState } from "react";
import { Pressable, View } from "react-native";
import type { MobileConnectionDetails } from "react-native-swarmdrop-core";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";

/**
 * 链路详情 —— 默认折叠的排障区块。
 *
 * 与上方的连接徽标分工：徽标回答「怎么连的」一句话，这里回答「凭什么这么说」
 * ——走的哪条地址、哪种传输、经不经中继、经的是谁。对普通用户是噪音，所以默认
 * 收起；对排障的人是全部证据，所以一次给全并且可复制。
 *
 * 三端同一形态：桌面与 Web 用 Popover（那两端有指针，悬浮层不挡内容），移动端
 * 用就地展开（触摸设备上的浮层会盖住半屏，而这本来就是一屏详情页）。
 */
export function ConnectionDetailsSection({
  details,
}: {
  details: MobileConnectionDetails;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const [expanded, setExpanded] = useState(false);

  const transport = transportLabel(details.transport);
  // 提到闭包外：TS 在 `details.relay ? ...` 里窄化出的非空，进了 onCopy 的闭包就没了
  const relay = details.relay;

  const copy = async (value: string) => {
    await Clipboard.setStringAsync(value);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制到剪贴板`);
  };

  return (
    <View className="gap-2">
      <Pressable
        onPress={() => setExpanded((v) => !v)}
        accessibilityRole="button"
        accessibilityState={{ expanded }}
        testID="connection-details-toggle"
        className="min-h-11 flex-row items-center justify-between gap-3 active:opacity-70"
      >
        <Text className="text-[13px] text-muted-foreground">
          <Trans>链路详情</Trans>
        </Text>
        {expanded ? (
          <ChevronDown color={colors.mutedForeground} size={16} />
        ) : (
          <ChevronRight color={colors.mutedForeground} size={16} />
        )}
      </Pressable>

      {expanded ? (
        <View className="gap-2.5 rounded-lg bg-muted px-3.5 py-3">
          <DetailField label={<Trans>传输</Trans>}>
            {transport ?? (
              // 入站中继连接的 send_back_addr 只有 /p2p/<src> 一段，地址里确实
              // 没有传输信息——照实说「未知」，不编一个默认值。
              <Text className="text-[12px] text-muted-foreground">
                <Trans>未知</Trans>
              </Text>
            )}
          </DetailField>
          {relay ? (
            <DetailField
              label={<Trans>中继节点</Trans>}
              onCopy={() => void copy(relay)}
              copyLabel={t`复制中继节点 ID`}
              mono
            >
              {relay}
            </DetailField>
          ) : null}
          <DetailField
            label={<Trans>远端地址</Trans>}
            onCopy={() => void copy(details.remoteAddr)}
            copyLabel={t`复制远端地址`}
            mono
          >
            {details.remoteAddr}
          </DetailField>
        </View>
      ) : null}
    </View>
  );
}

function DetailField({
  label,
  children,
  mono,
  onCopy,
  copyLabel,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
  mono?: boolean;
  onCopy?: () => void;
  copyLabel?: string;
}) {
  const colors = useThemeColors();
  return (
    <View className="gap-1">
      <View className="flex-row items-center justify-between gap-2">
        <Text className="text-[12px] uppercase tracking-wider text-muted-foreground">
          {label}
        </Text>
        {onCopy ? (
          <Pressable
            onPress={onCopy}
            accessibilityRole="button"
            accessibilityLabel={copyLabel}
            hitSlop={8}
            className="active:opacity-70"
          >
            <Copy color={colors.mutedForeground} size={14} />
          </Pressable>
        ) : null}
      </View>
      {typeof children === "string" ? (
        // 地址要能整条读出来，**不截断**——截断的 multiaddr 贴进 issue 是废的
        <Text
          className={
            mono
              ? "font-mono text-[12px] leading-4 text-foreground"
              : "text-[13px] text-foreground"
          }
          selectable
        >
          {children}
        </Text>
      ) : (
        children
      )}
    </View>
  );
}
