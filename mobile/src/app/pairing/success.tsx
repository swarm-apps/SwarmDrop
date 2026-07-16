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
  }>();
  const displayName = peerDisplayName(params.name, params.hostname);

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
