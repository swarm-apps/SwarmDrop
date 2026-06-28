import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import { SendHorizontal, Shield, SlidersHorizontal } from "lucide-react-native";
import { useCallback, useMemo, useRef } from "react";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  AppScreen,
  BottomActionArea,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { TrustBadge } from "@/components/trust-badge";
import { Text } from "@/components/ui/text";
import {
  canSendToDevice,
  policySummaryForDevice,
  resolveTrustLevel,
} from "@/core/device-trust";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";

export default function DeviceDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { peerId } = useLocalSearchParams<{ peerId: string }>();
  const policySheetRef = useRef<BottomSheetModal>(null);

  const { devices, pairedDevicesCache } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
    })),
  );

  const device = useMemo<DeviceInfo | null>(() => {
    if (!peerId) return null;
    return (
      devices.find((item) => item.peerId === peerId) ??
      summariesToOfflineDevices(pairedDevicesCache).find(
        (item) => item.peerId === peerId,
      ) ??
      null
    );
  }, [peerId, devices, pairedDevicesCache]);

  const renderBackdrop = useCallback(
    (props: BottomSheetBackdropProps) => (
      <BottomSheetBackdrop
        {...props}
        opacity={0.4}
        appearsOnIndex={0}
        disappearsOnIndex={-1}
        pressBehavior="close"
      />
    ),
    [],
  );

  if (!device) {
    return (
      <AppScreen testID="device-detail-missing-screen">
        <SettingsHeader title={t`设备详情`} />
        <View className="flex-1 items-center justify-center gap-3">
          <Text className="text-[13px] text-muted-foreground">
            <Trans>设备未找到</Trans>
          </Text>
          <Pressable
            onPress={() => router.back()}
            accessibilityRole="button"
            className="min-h-11 items-center justify-center rounded-xl bg-primary px-4 active:opacity-70"
          >
            <Text className="text-[13px] font-semibold text-primary-foreground">
              <Trans>返回</Trans>
            </Text>
          </Pressable>
        </View>
      </AppScreen>
    );
  }

  const displayName = deviceDisplayName(device);
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const trustLevel = resolveTrustLevel(device);
  const policy = policySummaryForDevice(device);
  const sendable = canSendToDevice(device);

  return (
    <AppScreen testID="device-detail-screen" contentClassName="gap-4 px-0 pb-0">
      <SettingsHeader title={t`设备详情`} />

      <View className="flex-1 gap-4 px-5">
        <Surface className="gap-4">
          <View className="flex-row items-center gap-3">
            <View className="size-14 items-center justify-center rounded-2xl bg-muted">
              <Icon color={colors.foreground} size={25} />
            </View>
            <View className="min-w-0 flex-1 gap-1">
              <Text
                className="text-[18px] font-semibold text-foreground"
                numberOfLines={1}
              >
                {displayName}
              </Text>
              <Text
                className="text-[12px] text-muted-foreground"
                numberOfLines={1}
              >
                {device.os} · {device.platform}
              </Text>
            </View>
            <TrustBadge level={trustLevel} />
          </View>

          <View className="grid gap-2">
            <InfoRow
              label={<Trans>连接状态</Trans>}
              value={
                device.status === "online" ? (
                  <Trans>在线</Trans>
                ) : (
                  <Trans>离线</Trans>
                )
              }
            />
            <InfoRow
              label={<Trans>连接路径</Trans>}
              value={device.connection ?? <Trans>等待发现</Trans>}
            />
            <InfoRow
              label={<Trans>Peer ID</Trans>}
              value={device.peerId}
              mono
            />
          </View>
        </Surface>

        <Surface className="gap-3">
          <View className="flex-row items-center gap-2">
            <Shield color={colors.primary} size={18} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>信任与接收策略</Trans>
            </Text>
          </View>
          <Text className="text-[12px] text-muted-foreground">
            {policy.note === "blocked" ? (
              <Trans>该设备当前被阻止，不能发起传输。</Trans>
            ) : (
              <Trans>
                该设备暂按协作设备处理。自动接收、询问和阻止策略会在后续同步。
              </Trans>
            )}
          </Text>
          <Pressable
            onPress={() => policySheetRef.current?.present()}
            accessibilityRole="button"
            testID="device-policy-entry"
            className="min-h-11 flex-row items-center justify-center gap-2 rounded-xl border border-border active:opacity-70"
          >
            <SlidersHorizontal color={colors.foreground} size={16} />
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>策略设置</Trans>
            </Text>
          </Pressable>
        </Surface>
      </View>

      <BottomActionArea>
        <Pressable
          onPress={() => {
            if (!sendable) {
              toast.info(t`设备当前不可发送`);
              return;
            }
            router.push({
              pathname: "/send/select-device",
              params: { peerId: device.peerId },
            } as never);
          }}
          accessibilityRole="button"
          disabled={!sendable}
          className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <SendHorizontal
            color={sendable ? colors.background : colors.mutedForeground}
            size={17}
          />
          <Text
            className={
              sendable
                ? "text-[14px] font-semibold text-primary-foreground"
                : "text-[14px] font-semibold text-muted-foreground"
            }
          >
            <Trans>发送文件</Trans>
          </Text>
        </Pressable>
      </BottomActionArea>

      <BottomSheetModal
        ref={policySheetRef}
        enableDynamicSizing
        enablePanDownToClose
        backdropComponent={renderBackdrop}
        backgroundStyle={{ backgroundColor: colors.card }}
        handleIndicatorStyle={{ backgroundColor: colors.border }}
      >
        <BottomSheetView>
          <View className="gap-4 px-5 pt-2 pb-6" testID="device-policy-sheet">
            <View className="items-center gap-2">
              <View className="size-12 items-center justify-center rounded-full bg-primary/10">
                <Shield color={colors.primary} size={23} />
              </View>
              <Text className="text-[16px] font-semibold text-foreground">
                <Trans>设备策略</Trans>
              </Text>
              <Text className="text-center text-[12px] text-muted-foreground">
                <Trans>
                  这里会承载自动接收、询问、阻止和信任模板。当前版本先保留入口和容器。
                </Trans>
              </Text>
            </View>
            <View className="rounded-lg bg-muted px-3.5 py-3">
              <Text className="text-[12px] text-muted-foreground">
                <Trans>
                  持久化策略编辑将在 add-mobile-device-trust-policies 中接入。
                </Trans>
              </Text>
            </View>
          </View>
        </BottomSheetView>
      </BottomSheetModal>
    </AppScreen>
  );
}

function InfoRow({
  label,
  value,
  mono,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <View className="flex-row items-center justify-between gap-3">
      <Text className="text-[12px] text-muted-foreground">{label}</Text>
      <Text
        className={
          mono
            ? "flex-1 text-right font-mono text-[11px] text-foreground"
            : "flex-1 text-right text-[12px] text-foreground"
        }
        numberOfLines={1}
      >
        {value}
      </Text>
    </View>
  );
}
