import { useLingui } from "@lingui/react/macro";
import type { NodeHealthSummary } from "@swarmdrop/shared-view";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import {
  resolveNodePresentation,
  TONE_BG_CLASS,
  TONE_DOT_CLASS,
  TONE_TEXT_CLASS,
} from "@/core/network-labels";
import { cn } from "@/lib/utils";
import type { RuntimeState } from "@/stores/mobile-core-store";

interface StatusPillProps {
  state: RuntimeState;
  /**
   * 网络健康（`summarizeNodeHealth` 的结果）。
   *
   * pill 说的是「别人现在能不能连到你」而不只是「进程起没起来」——后者在节点跑着但
   * 全部中继都挂了的时候会显示成绿的。`starting` / `error` 这两态健康判据表达不了，
   * 由 `resolveNodePresentation` 的生命周期覆盖层补上：**必传**，因为色档、状态词与
   * 后果句必须出自同一次判定，各算各的正是「pill 说启动中、下一行说未运行」的来源。
   */
  health: NodeHealthSummary;
  onPress?: () => void;
  size?: "sm" | "md";
  testID?: string;
}

/**
 * 节点状态 pill —— 契约结论层的信息位 1（色点**与词**，光有色点不算数）。
 * 点击可打开节点状态面（由父组件接入 onPress）。
 */
export function StatusPill({
  state,
  health,
  onPress,
  size = "sm",
  testID,
}: StatusPillProps) {
  const { t } = useLingui();
  const presentation = resolveNodePresentation(state, health);

  const label = t(presentation.word);
  // 屏幕阅读器拿不到色点，状态词又只有两三个字——把后果句一并读出来。
  const accessibilityLabel = `${label} · ${t(presentation.sentence)}`;

  const Wrapper = onPress ? Pressable : View;

  return (
    <Wrapper
      onPress={onPress}
      testID={testID}
      accessibilityRole={onPress ? "button" : undefined}
      accessibilityLabel={accessibilityLabel}
      accessibilityHint={onPress ? t`查看节点状态与诊断` : undefined}
      {...(onPress ? { hitSlop: 10 } : {})}
      className={cn(
        "flex-row items-center self-start rounded-full",
        size === "sm" ? "gap-1.5 px-2.5 py-1" : "gap-2 px-3 py-1.5",
        TONE_BG_CLASS[presentation.tone],
        onPress ? "active:opacity-70" : null,
      )}
    >
      <View
        className={cn(
          "rounded-full",
          size === "sm" ? "size-2" : "size-2.5",
          TONE_DOT_CLASS[presentation.tone],
        )}
      />
      <Text
        className={cn(
          "font-medium",
          size === "sm" ? "text-[13px]" : "text-sm",
          TONE_TEXT_CLASS[presentation.tone],
        )}
      >
        {label}
      </Text>
    </Wrapper>
  );
}
