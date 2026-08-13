import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { MonitorSmartphone } from "lucide-react-native";
import { useEffect, useRef } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { HeaderBackButton } from "@/components/header-back-button";
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
  const confirmReject = usePairingInviteStore((s) => s.confirmReject);
  const confirmInvite = usePairingInviteStore((s) => s.confirmInvite);
  const cancelPreview = usePairingInviteStore((s) => s.cancelPreview);

  // 仅「一进本页就没有 pending」（深链/刷新直达）时返回；confirm 成功清空 pending 由
  // onConfirm 的 router.replace 导航，拒绝保留 pending 展示 error——都不触发本兜底。
  const hadPending = useRef(pending !== null);
  useEffect(() => {
    if (!hadPending.current && !confirming) router.back();
  }, [confirming, router]);

  const preview = pending?.preview;

  const onConfirm = async () => {
    const { accepted, persisted } = await confirmInvite();
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
          // 「配对成功但没落盘」要出现在成功页上 —— 只弹 toast 会被转场动画盖住，
          // 而那一屏通篇是绿色对勾。route param 只能是字符串。
          persisted: persisted ? "1" : "0",
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
      {/* 居中标题的导航条(与 SettingsHeader 的左对齐不同,这是确认流的形态),
          但返回入口用同一个 HeaderBackButton;右侧 size-11 占位保证标题真正居中。 */}
      <View className="flex-row items-center justify-between px-5 pt-2">
        <HeaderBackButton onPress={onCancel} disabled={confirming} />
        <Text className="text-[17px] font-bold text-foreground">
          <Trans>确认设备</Trans>
        </Text>
        <View className="size-11" />
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

        {/* 文案按判别码本地化 —— store 里存的是 `userRejected` / `failed`，不是后端串。
            后端曾经把 `{reason:?}` 的 Rust 裸标识符送到这里显示。 */}
        {confirmReject !== null ? (
          <Text className="mt-3 text-center text-[13px] text-destructive-ink">
            {confirmReject === "userRejected" ? (
              <Trans>对方拒绝了配对请求</Trans>
            ) : (
              <Trans>配对没有成功，请确认两端都在线后重试</Trans>
            )}
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

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
