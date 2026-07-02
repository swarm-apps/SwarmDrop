import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import { ArrowLeft, MonitorSmartphone } from "lucide-react-native";
import { useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { PeerSummaryCard } from "@/components/pairing/peer-summary-card";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function FoundDevice() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const params = useLocalSearchParams<{
    peerId: string;
    code: string;
    name?: string;
    hostname: string;
    os: string;
    platform: string;
    arch: string;
  }>();
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onConfirm = async () => {
    setError(null);
    setConfirming(true);
    try {
      const result = await getMobileCore().requestPairing(
        params.peerId,
        params.code,
        [],
      );
      if (!result.accepted) {
        setError(
          result.reason ? t`配对被拒绝:${result.reason}` : t`配对被拒绝`,
        );
        return;
      }
      router.replace({
        pathname: "/pairing/success" as never,
        params: {
          peerId: params.peerId,
          name: params.name ?? "",
          hostname: params.hostname,
          os: params.os,
          platform: params.platform,
          arch: params.arch,
        },
      } as never);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setConfirming(false);
    }
  };

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top", "bottom"]}
    >
      <View className="flex-row items-center justify-between px-4 pt-2">
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          disabled={confirming}
          accessibilityRole="button"
          accessibilityLabel={t`返回`}
          className="size-10 items-center justify-center active:opacity-70 disabled:opacity-50"
        >
          <ArrowLeft color={colors.foreground} size={22} />
        </Pressable>
        <Text className="text-[17px] font-bold text-foreground">
          <Trans>确认设备</Trans>
        </Text>
        <View className="size-10" />
      </View>

      <View className="flex-1 items-center justify-center gap-3 px-6">
        <View className="mb-2 size-[72px] items-center justify-center rounded-full bg-primary/10">
          <MonitorSmartphone color={colors.primary} size={36} />
        </View>
        <Text className="text-[22px] font-extrabold text-foreground">
          <Trans>找到设备</Trans>
        </Text>
        <Text className="mb-2 text-center text-sm text-muted-foreground">
          <Trans>确认这是你要配对的设备?</Trans>
        </Text>

        <PeerSummaryCard
          name={params.name}
          hostname={params.hostname}
          os={params.os}
          platform={params.platform}
          arch={params.arch}
          peerId={params.peerId}
          showPlatform
        />

        {error !== null ? (
          <Text className="mt-3 text-center text-[13px] text-destructive-ink">
            {error}
          </Text>
        ) : null}
      </View>

      <View className="gap-2.5 px-6 pb-6">
        <Pressable
          onPress={onConfirm}
          disabled={confirming}
          accessibilityRole="button"
          accessibilityLabel={t`确认配对`}
          accessibilityState={{ busy: confirming, disabled: confirming }}
          className="min-h-[52px] items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
        >
          {confirming ? (
            <ActivityIndicator color={colors.primaryForeground} />
          ) : (
            <Text className="text-[17px] font-bold text-primary-foreground">
              <Trans>确认配对</Trans>
            </Text>
          )}
        </Pressable>
        <Pressable
          onPress={() => router.back()}
          disabled={confirming}
          accessibilityRole="button"
          accessibilityLabel={t`取消`}
          className="min-h-[52px] items-center justify-center rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <Text className="text-base font-semibold text-foreground">
            <Trans>取消</Trans>
          </Text>
        </Pressable>
      </View>
    </SafeAreaView>
  );
}
