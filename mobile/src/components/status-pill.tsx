import { Trans } from "@lingui/react/macro";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import { cn } from "@/lib/utils";
import type { RuntimeState } from "@/stores/mobile-core-store";

interface StatusPillProps {
  state: RuntimeState;
  onPress?: () => void;
  size?: "sm" | "md";
  testID?: string;
}

/**
 * 节点运行状态 pill(running/starting/stopped/error)。
 * 点击可启停节点(由父组件接入 onPress)。
 */
export function StatusPill({
  state,
  onPress,
  size = "sm",
  testID,
}: StatusPillProps) {
  const dotClass = DOT_CLASS[state];
  const textClass = TEXT_CLASS[state];
  const bgClass = BG_CLASS[state];

  const Wrapper = onPress ? Pressable : View;

  return (
    <Wrapper
      onPress={onPress}
      testID={testID}
      accessibilityRole={onPress ? "button" : undefined}
      {...(onPress ? { hitSlop: 10 } : {})}
      className={cn(
        "flex-row items-center self-start rounded-full",
        size === "sm" ? "gap-1.5 px-2.5 py-1" : "gap-2 px-3 py-1.5",
        bgClass,
        onPress ? "active:opacity-70" : null,
      )}
    >
      <View
        className={cn(
          "rounded-full",
          size === "sm" ? "size-2" : "size-2.5",
          dotClass,
        )}
      />
      <Text
        className={cn(
          "font-medium",
          size === "sm" ? "text-[13px]" : "text-sm",
          textClass,
        )}
      >
        {LABEL[state]}
      </Text>
    </Wrapper>
  );
}

const LABEL: Record<RuntimeState, React.ReactNode> = {
  running: <Trans>运行中</Trans>,
  starting: <Trans>启动中</Trans>,
  stopped: <Trans>未启动</Trans>,
  error: <Trans>错误</Trans>,
};

const DOT_CLASS: Record<RuntimeState, string> = {
  running: "bg-success",
  starting: "bg-warning",
  stopped: "bg-muted-foreground",
  error: "bg-destructive",
};

const TEXT_CLASS: Record<RuntimeState, string> = {
  running: "text-success-ink",
  starting: "text-warning-ink",
  stopped: "text-muted-foreground",
  error: "text-destructive-ink",
};

const BG_CLASS: Record<RuntimeState, string> = {
  running: "bg-success/10",
  starting: "bg-warning/10",
  stopped: "bg-muted",
  error: "bg-destructive/10",
};
