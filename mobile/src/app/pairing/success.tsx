import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import { CheckCircle2 } from "lucide-react-native";
import { Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import {
  PeerSummaryCard,
  peerDisplayName,
} from "@/components/pairing/peer-summary-card";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function PairingSuccess() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const params = useLocalSearchParams<{
    peerId: string;
    name?: string;
    hostname: string;
    os: string;
    platform: string;
    arch: string;
    /** `"0"` = 配对成功但记录没落盘（见下方 caveat）。route param 只能是字符串。 */
    persisted?: string;
  }>();
  const displayName = peerDisplayName(params.name, params.hostname);
  // 缺省视为已落盘：老的导航调用点没带这个 param 时不该凭空吓用户一跳。
  const notPersisted = params.persisted === "0";

  const finish = () => {
    router.dismissAll();
  };

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top", "bottom"]}
    >
      <View className="flex-1 items-center justify-center gap-3 px-6">
        <View className="mb-3 size-24 items-center justify-center rounded-full bg-success/15">
          <CheckCircle2 color={colors.success} size={56} strokeWidth={2} />
        </View>
        <Text className="text-2xl font-extrabold text-foreground">
          <Trans>配对成功</Trans>
        </Text>
        <Text className="mb-3 text-center text-sm text-muted-foreground">
          <Trans>已与 {displayName} 建立安全连接</Trans>
        </Text>

        {/*
          「一半成功」必须出现在这一屏上。此前只在跳转**之前**弹一条 toast，而转场动画会
          盖掉它的前半段、这一屏又通篇是绿色对勾 + 「配对成功」——用户带走的结论是「成了」，
          重启后设备消失时无从解释。
        */}
        {notPersisted && (
          <View className="mb-3 rounded-xl border border-warning/30 bg-warning/10 px-4 py-3">
            <Text className="text-center text-xs text-warning-ink">
              <Trans>但这条记录没能保存，重启应用后需要重新配对。</Trans>
            </Text>
          </View>
        )}

        <PeerSummaryCard
          name={params.name}
          hostname={params.hostname}
          os={params.os}
          platform={params.platform}
          arch={params.arch}
          peerId={params.peerId}
        />
      </View>

      <View className="px-6 pb-6">
        <Pressable
          onPress={finish}
          accessibilityRole="button"
          accessibilityLabel={t`完成`}
          className="min-h-[52px] items-center justify-center rounded-xl bg-primary active:opacity-70"
        >
          <Text className="text-[17px] font-bold text-primary-foreground">
            <Trans>完成</Trans>
          </Text>
        </Pressable>
      </View>
    </SafeAreaView>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
