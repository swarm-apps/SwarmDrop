import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ArrowLeft, MonitorSmartphone } from "lucide-react-native";
import { useEffect } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { PeerSummaryCard } from "@/components/pairing/peer-summary-card";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";

/**
 * 邀请配对确认页——受邀方粘贴/扫码邀请后，本地解码验签的对端信息（store.pending）
 * 展示确认卡，用户确认后 confirmInvite（连接 + 出示凭证）。
 */
export default function FoundDevice() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const pending = usePairingInviteStore((s) => s.pending);
  const confirming = usePairingInviteStore((s) => s.confirming);
  const error = usePairingInviteStore((s) => s.error);
  const confirmInvite = usePairingInviteStore((s) => s.confirmInvite);
  const cancelPreview = usePairingInviteStore((s) => s.cancelPreview);

  // 无 pending（直接进入本页 / 刷新）→ 返回
  useEffect(() => {
    if (pending === null && !confirming) router.back();
  }, [pending, confirming, router]);

  const preview = pending?.preview;

  const onConfirm = async () => {
    const accepted = await confirmInvite();
    if (accepted && preview) {
      router.replace({
        pathname: "/pairing/success" as never,
        params: {
          peerId: preview.peerId,
          name: preview.displayName,
          hostname: preview.displayName,
          os: preview.displayPlatform,
          platform: preview.displayPlatform,
          arch: "",
        },
      } as never);
    }
  };

  const onCancel = () => {
    cancelPreview();
    router.back();
  };

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top", "bottom"]}
    >
      <View className="flex-row items-center justify-between px-4 pt-2">
        <Pressable
          onPress={onCancel}
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

        {preview ? (
          <PeerSummaryCard
            name={preview.displayName}
            hostname={preview.displayName}
            os={preview.displayPlatform}
            platform={preview.displayPlatform}
            arch=""
            peerId={preview.peerId}
            showPlatform
          />
        ) : null}

        {error !== null ? (
          <Text className="mt-3 text-center text-[13px] text-destructive-ink">
            {error}
          </Text>
        ) : null}
      </View>

      {/* 移动端惯例:双键横排,取消(次要)在左、确认配对(主要)在右 */}
      <View className="flex-row gap-2.5 px-6 pb-6">
        <Pressable
          onPress={onCancel}
          disabled={confirming}
          accessibilityRole="button"
          accessibilityLabel={t`取消`}
          className="min-h-[52px] flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <Text className="text-base font-medium text-foreground">
            <Trans>取消</Trans>
          </Text>
        </Pressable>
        <Pressable
          onPress={onConfirm}
          disabled={confirming}
          accessibilityRole="button"
          accessibilityLabel={t`确认配对`}
          accessibilityState={{ busy: confirming, disabled: confirming }}
          className="min-h-[52px] flex-1 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
        >
          {confirming ? (
            <ActivityIndicator color={colors.primaryForeground} />
          ) : (
            <Text className="text-base font-semibold text-primary-foreground">
              <Trans>确认配对</Trans>
            </Text>
          )}
        </Pressable>
      </View>
    </SafeAreaView>
  );
}
