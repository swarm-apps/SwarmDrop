import { Trans, useLingui } from "@lingui/react/macro";
import { Link, ShieldCheck } from "lucide-react-native";
import { useCallback, useState } from "react";
import {
  ActivityIndicator,
  Dimensions,
  Platform,
  Pressable,
  View,
} from "react-native";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { useThemeColors } from "@/hooks/useThemeColors";
import { truncateMiddle } from "@/lib/utils";
import { useNotificationStore } from "@/stores/notification-store";
import { usePairingCodeStore } from "@/stores/pairing-code-store";

const REQUEST_TTL_SECS = 60;

export function PairingRequestHost() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const current = useNotificationStore((s) => s.current);
  const respondStore = useNotificationStore((s) => s.respond);
  const [responding, setResponding] = useState(false);

  const open = current !== null && current.type === "pairing-request";
  const payload = open ? current.payload : null;
  const expiresAt = payload
    ? payload.receivedAt + REQUEST_TTL_SECS * 1000
    : null;

  const onExpire = useCallback(() => {
    if (current) respondStore(current.id);
  }, [current, respondStore]);

  const remaining = useExpiresCountdown(expiresAt, onExpire);

  const respondToRequest = useCallback(
    async (accept: boolean) => {
      if (!current || !payload || responding) return;
      setResponding(true);
      try {
        await getMobileCore().respondPairingRequest(
          payload.pendingId,
          payload.code ?? undefined,
          accept,
        );
        // Code 模式 accept 后，后端已消耗 active_code（参考 SwarmDrop
        // pairing/manager.rs:282）；通知 store 续生新码（reject 不消耗，不动）
        if (accept && payload.code !== undefined) {
          usePairingCodeStore.getState().markConsumed();
        }
      } catch (err) {
        console.warn(
          `[pairing-host] ${accept ? "accept" : "reject"} failed:`,
          err,
        );
      } finally {
        respondStore(current.id);
        setResponding(false);
      }
    },
    [current, payload, respondStore, responding],
  );

  if (!open || !payload) return null;

  return (
    <AlertDialog open={open}>
      <AlertDialogContent
        style={[
          {
            borderRadius: 20,
            borderWidth: 0,
            width: Math.min(Dimensions.get("window").width * 0.86, 480),
          },
          Platform.OS === "android"
            ? { elevation: 24 }
            : {
                shadowColor: "#000",
                shadowOffset: { width: 0, height: 16 },
                shadowOpacity: 0.2,
                shadowRadius: 40,
              },
        ]}
        className="max-w-none gap-4 bg-card p-5 sm:max-w-none"
      >
        <View className="flex-row items-center gap-3">
          <View className="size-11 items-center justify-center rounded-full bg-primary/10">
            <Link color={colors.primary} size={22} />
          </View>
          <AlertDialogTitle className="flex-1 text-foreground text-base font-bold">
            <Trans>配对请求</Trans>
          </AlertDialogTitle>
          <Text className="text-xs text-muted-foreground">
            {remaining > 0 ? `${remaining}s` : t`已过期`}
          </Text>
        </View>

        <Text className="text-sm text-muted-foreground">
          <Trans>另一台设备请求与你建立配对</Trans>
        </Text>

        <View className="flex-row items-center gap-3 rounded-xl bg-muted p-3.5">
          <View className="size-10 items-center justify-center rounded-full bg-background">
            <ShieldCheck color={colors.primary} size={18} />
          </View>
          <View className="flex-1 gap-0.5">
            <Text
              className="text-[13px] font-medium text-foreground"
              numberOfLines={1}
            >
              {truncateMiddle(payload.peerId, 12, 8)}
            </Text>
            {payload.code ? (
              <Text className="text-[11px] text-muted-foreground">
                <Trans>配对码 {payload.code}</Trans>
              </Text>
            ) : null}
          </View>
        </View>

        <View className="gap-2.5">
          <Pressable
            onPress={() => respondToRequest(true)}
            disabled={responding || remaining === 0}
            accessibilityRole="button"
            accessibilityLabel={t`接受`}
            className="h-12 items-center justify-center rounded-xl bg-primary active:opacity-80 disabled:opacity-50"
          >
            {responding ? (
              <ActivityIndicator color={colors.background} />
            ) : (
              <Text className="text-base font-semibold text-primary-foreground">
                <Trans>接受</Trans>
              </Text>
            )}
          </Pressable>
          <Pressable
            onPress={() => respondToRequest(false)}
            disabled={responding}
            accessibilityRole="button"
            accessibilityLabel={t`拒绝`}
            className="h-12 items-center justify-center rounded-xl border border-border bg-card active:opacity-80 disabled:opacity-50"
          >
            <Text className="text-base font-medium text-foreground">
              <Trans>拒绝</Trans>
            </Text>
          </Pressable>
        </View>
      </AlertDialogContent>
    </AlertDialog>
  );
}
