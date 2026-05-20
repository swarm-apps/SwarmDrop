import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetScrollView,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { OTPInput, type OTPInputRef, type SlotProps } from "input-otp-native";
import { Copy, RefreshCcw } from "lucide-react-native";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { errorMessage } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePairingCodeStore } from "@/stores/pairing-code-store";

export interface PairingSheetRef {
  present: () => void;
  dismiss: () => void;
}

/**
 * 配对 BottomSheet —— 双 tab:生成码 / 输入码。
 * - 生成 tab:展示 6 位码 + 倒计时 + 复制 + 重新生成。
 * - 输入 tab:OTP 输入,完成后 lookupDeviceByCode 并跳 found-device 页。
 */
export const PairingSheet = forwardRef<PairingSheetRef, object>(
  function PairingSheet(_props, ref) {
    const sheetRef = useRef<BottomSheetModal>(null);
    const colors = useThemeColors();

    useImperativeHandle(ref, () => ({
      present: () => sheetRef.current?.present(),
      dismiss: () => sheetRef.current?.dismiss(),
    }));

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

    return (
      <BottomSheetModal
        ref={sheetRef}
        enableDynamicSizing
        enablePanDownToClose
        backdropComponent={renderBackdrop}
        backgroundStyle={{ backgroundColor: colors.card }}
        handleIndicatorStyle={{ backgroundColor: colors.border }}
      >
        <BottomSheetView>
          <View className="px-5 pt-2 pb-6">
            <PairingTabs onDismiss={() => sheetRef.current?.dismiss()} />
          </View>
        </BottomSheetView>
      </BottomSheetModal>
    );
  },
);

function PairingTabs({ onDismiss }: { onDismiss: () => void }) {
  const { t } = useLingui();
  // selector 返回新数组会让 useSyncExternalStore 把 snapshot 判为不稳定 → 触发
  // Maximum update depth。用 useShallow 做逐元素 === 比较即可（同桌面端做法）。
  const nearbyDevices = useMobileCoreStore(
    useShallow((s) =>
      s.devices.filter((d) => !d.isPaired && d.status === "online"),
    ),
  );
  const isOnline = useMobileCoreStore((s) => s.runtimeState === "running");

  // 默认 tab：有附近设备 → nearby；否则 generate。仅在 mount 时取一次。
  const [tab, setTab] = useState(() =>
    isOnline && nearbyDevices.length > 0 ? "nearby" : "generate",
  );

  return (
    <Tabs value={tab} onValueChange={setTab} className="gap-5">
      <TabsList className="w-full flex-row">
        <TabsTrigger value="nearby" className="flex-1">
          <Text>
            {t`附近`}
            {nearbyDevices.length > 0 ? ` (${nearbyDevices.length})` : ""}
          </Text>
        </TabsTrigger>
        <TabsTrigger value="generate" className="flex-1">
          <Text>{t`生成`}</Text>
        </TabsTrigger>
        <TabsTrigger value="input" className="flex-1">
          <Text>{t`输入`}</Text>
        </TabsTrigger>
      </TabsList>

      <TabsContent value="nearby">
        <NearbyTab
          isOnline={isOnline}
          devices={nearbyDevices}
          onDismiss={onDismiss}
        />
      </TabsContent>
      <TabsContent value="generate">
        <GenerateTab />
      </TabsContent>
      <TabsContent value="input">
        <InputTab onDismiss={onDismiss} />
      </TabsContent>
    </Tabs>
  );
}

function NearbyTab({
  isOnline,
  devices,
  onDismiss,
}: {
  isOnline: boolean;
  devices: DeviceInfo[];
  onDismiss: () => void;
}) {
  const router = useRouter();
  const colors = useThemeColors();
  const [pairingPeer, setPairingPeer] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onPair = async (d: DeviceInfo) => {
    if (pairingPeer !== null) return;
    setError(null);
    setPairingPeer(d.peerId);
    try {
      // 不传 code → mobile-core 走 PairingMethod::Direct
      const result = await getMobileCore().requestPairing(
        d.peerId,
        undefined,
        [],
      );
      if (!result.accepted) {
        setError(result.reason ?? "配对被拒绝");
        return;
      }
      onDismiss();
      router.push({
        pathname: "/pairing/success",
        params: {
          peerId: d.peerId,
          name: d.name ?? "",
          hostname: d.hostname,
          os: d.os,
          platform: d.platform,
          arch: d.arch,
        },
      });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPairingPeer(null);
    }
  };

  if (!isOnline) {
    return (
      <View className="gap-2 items-center py-6">
        <Text className="text-center text-[13px] text-muted-foreground">
          <Trans>节点未启动 · 启动后才能发现附近设备</Trans>
        </Text>
      </View>
    );
  }

  if (devices.length === 0) {
    return (
      <View className="gap-2 items-center py-6">
        <ActivityIndicator color={colors.mutedForeground} />
        <Text className="text-center text-[13px] text-muted-foreground">
          <Trans>暂未发现附近设备{"\n"}确保对端 SwarmDrop 已启动</Trans>
        </Text>
      </View>
    );
  }

  return (
    <BottomSheetScrollView
      contentContainerStyle={{ gap: 8, paddingVertical: 4 }}
    >
      {devices.map((d) => {
        const Icon = devicePlatformIcon(`${d.os} ${d.platform}`);
        const isThisPairing = pairingPeer === d.peerId;
        const disabled = pairingPeer !== null;
        return (
          <Pressable
            key={d.peerId}
            onPress={() => onPair(d)}
            disabled={disabled}
            accessibilityRole="button"
            className="flex-row items-center gap-3 rounded-xl border border-border bg-card p-3 active:opacity-70 disabled:opacity-50"
          >
            <View className="size-9 items-center justify-center rounded-full bg-muted">
              <Icon color={colors.foreground} size={16} />
            </View>
            <View className="flex-1 gap-0.5">
              <Text
                className="text-[14px] font-semibold text-foreground"
                numberOfLines={1}
              >
                {deviceDisplayName(d)}
              </Text>
              <Text
                className="text-[11px] text-muted-foreground"
                numberOfLines={1}
              >
                {d.platform} · {d.os}
              </Text>
            </View>
            {isThisPairing ? (
              <ActivityIndicator color={colors.primary} />
            ) : (
              <Text className="rounded-md bg-primary/10 px-2.5 py-1 text-[11px] font-semibold text-primary">
                <Trans>配对</Trans>
              </Text>
            )}
          </Pressable>
        );
      })}
      {error !== null ? (
        <Text className="pt-1 text-center text-[12px] text-destructive">
          {error}
        </Text>
      ) : null}
    </BottomSheetScrollView>
  );
}

function GenerateTab() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const codeInfo = usePairingCodeStore((s) => s.codeInfo);
  const generating = usePairingCodeStore((s) => s.generating);
  const error = usePairingCodeStore((s) => s.error);
  const ensure = usePairingCodeStore((s) => s.ensure);
  const regenerate = usePairingCodeStore((s) => s.regenerate);

  // 进入 tab 时确保有活跃码（已有未过期就 no-op；store 自带过期前续生）
  useEffect(() => {
    void ensure();
  }, [ensure]);

  // 倒计时仅展示用；过期续生由 store timer 负责，UI 不需 onExpire 回调
  const remaining = useExpiresCountdown(
    codeInfo ? Number(codeInfo.expiresAt) * 1000 : null,
  );

  const code = codeInfo?.code ?? null;

  const handleCopy = async () => {
    if (!code) return;
    await Clipboard.setStringAsync(code);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制配对码`);
  };

  if (!code) {
    return (
      <View className="gap-4 items-center py-2">
        <Text className="text-center text-[13px] text-muted-foreground">
          <Trans>生成一个 6 位配对码,让其他设备输入它建立配对</Trans>
        </Text>
        <Pressable
          onPress={() => void regenerate()}
          disabled={generating}
          accessibilityRole="button"
          className="w-full h-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
        >
          {generating ? (
            <ActivityIndicator color={colors.background} />
          ) : (
            <Text className="text-[15px] font-semibold text-primary-foreground">
              <Trans>生成配对码</Trans>
            </Text>
          )}
        </Pressable>
        {error !== null ? (
          <Text className="text-[12px] text-destructive">{error}</Text>
        ) : null}
      </View>
    );
  }

  const expiryLabel =
    remaining > 0 ? t`${formatMmss(remaining)} 后过期` : t`已过期,请重新生成`;

  return (
    <View className="gap-4 items-center py-2">
      <Text className="text-[36px] font-extrabold tracking-[8px] text-foreground">
        {code}
      </Text>
      <Text className="text-[12px] text-muted-foreground">{expiryLabel}</Text>
      <View className="flex-row gap-2 w-full">
        <Pressable
          onPress={() => void regenerate()}
          accessibilityRole="button"
          className="flex-1 h-10 flex-row items-center justify-center gap-1.5 rounded-xl border border-border bg-card active:opacity-70"
        >
          <RefreshCcw color={colors.foreground} size={14} />
          <Text className="text-[13px] font-medium text-foreground">
            <Trans>重新生成</Trans>
          </Text>
        </Pressable>
        <Pressable
          onPress={handleCopy}
          accessibilityRole="button"
          className="flex-1 h-10 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70"
        >
          <Copy color={colors.background} size={14} />
          <Text className="text-[13px] font-semibold text-primary-foreground">
            <Trans>复制</Trans>
          </Text>
        </Pressable>
      </View>
      <Text className="text-center text-[11px] text-muted-foreground">
        <Trans>正在等待对端连接...</Trans>
      </Text>
    </View>
  );
}

const SLOT_KEYS = ["s0", "s1", "s2", "s3", "s4", "s5"] as const;

function InputTab({ onDismiss }: { onDismiss: () => void }) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const otpRef = useRef<OTPInputRef>(null);
  const [code, setCode] = useState("");
  const [looking, setLooking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onLookup = async (filled: string) => {
    if (looking || filled.length !== 6) return;
    setError(null);
    setLooking(true);
    try {
      const remote = await getMobileCore().lookupDeviceByCode(filled);
      onDismiss();
      router.push({
        pathname: "/pairing/found-device",
        params: {
          peerId: remote.peerId,
          code: filled,
          name: remote.name ?? "",
          hostname: remote.hostname,
          os: remote.os,
          platform: remote.platform,
          arch: remote.arch,
        },
      });
    } catch (err) {
      setError(t`配对码无效或已过期`);
      setCode("");
      otpRef.current?.clear();
      console.warn(
        "[pairing-sheet] lookupDeviceByCode failed:",
        errorMessage(err),
      );
    } finally {
      setLooking(false);
    }
  };

  return (
    <View className="gap-4 items-center py-2">
      <Text className="text-center text-[13px] text-muted-foreground">
        <Trans>输入对方设备上显示的 6 位配对码</Trans>
      </Text>
      <OTPInput
        ref={otpRef}
        maxLength={6}
        value={code}
        onChange={setCode}
        onComplete={onLookup}
        textAlign="center"
        render={({ slots }) => (
          <View className="flex-row items-center justify-center gap-2">
            {slots.map((slot, i) => (
              <Pressable
                key={SLOT_KEYS[i]}
                onPress={() => otpRef.current?.focus()}
              >
                <OtpSlot {...slot} />
              </Pressable>
            ))}
          </View>
        )}
      />
      {error !== null ? (
        <Text className="text-[12px] text-destructive">{error}</Text>
      ) : looking ? (
        <ActivityIndicator color={colors.mutedForeground} />
      ) : null}
    </View>
  );
}

function OtpSlot({ char, isActive }: SlotProps) {
  return (
    <View
      className={
        isActive
          ? "h-14 w-11 items-center justify-center rounded-[10px] bg-muted border-2 border-primary"
          : "h-14 w-11 items-center justify-center rounded-[10px] bg-muted border border-border"
      }
    >
      {char !== null ? (
        <Text className="text-[24px] font-bold text-foreground">{char}</Text>
      ) : null}
    </View>
  );
}

function formatMmss(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
