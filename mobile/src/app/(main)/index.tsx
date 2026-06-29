import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetModal,
  BottomSheetView,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { OTPInput, type OTPInputRef, type SlotProps } from "input-otp-native";
import {
  Activity as ActivityIcon,
  ChevronRight,
  Copy,
  Inbox,
  Keyboard,
  type LucideIcon,
  Power,
  Radio,
  RefreshCcw,
  Send,
  Smartphone,
} from "lucide-react-native";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { DeviceCard } from "@/components/device-card";
import {
  AppHeader,
  AppScreen,
  EmptyState,
  Surface,
} from "@/components/mobile/screen";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { StatusPill } from "@/components/status-pill";
import { Text } from "@/components/ui/text";
import { canSendToDevice } from "@/core/device-trust";
import { getMobileCore } from "@/core/mobile-core";
import { isProjectionActive } from "@/core/transfer-types";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { useThemeColors } from "@/hooks/useThemeColors";
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { cn, errorMessage } from "@/lib/utils";
import {
  mergePairedDevicesWithCache,
  type RuntimeState,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { usePairingCodeStore } from "@/stores/pairing-code-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function DevicesScreen() {
  const router = useRouter();
  const { t } = useLingui();
  const nodeSheetRef = useRef<NodeControlSheetRef>(null);

  const {
    devices,
    pairedDevicesCache,
    runtimeState,
    error,
    loadIdentity,
    initialized,
    setError,
    startNode,
  } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      runtimeState: s.runtimeState,
      error: s.error,
      loadIdentity: s.loadIdentity,
      initialized: s.initialized,
      setError: s.setError,
      startNode: s.startNode,
    })),
  );

  const projections = useTransferStore((s) => s.projections);
  const progressBySession = useTransferStore((s) => s.progressBySession);
  const loadProjections = useTransferStore((s) => s.loadProjections);

  useEffect(() => {
    if (!initialized) {
      void loadIdentity();
    }
  }, [initialized, loadIdentity]);

  useEffect(() => {
    void loadProjections();
  }, [loadProjections]);

  useEffect(() => {
    if (error !== null) {
      toast.error(error);
      setError(null);
    }
  }, [error, setError]);

  const pairedDevices = useMemo(() => {
    return mergePairedDevicesWithCache(devices, pairedDevicesCache).sort(
      (a, b) => {
        if (a.status !== b.status) {
          return a.status === "online" ? -1 : 1;
        }
        return deviceDisplayName(a).localeCompare(deviceDisplayName(b));
      },
    );
  }, [devices, pairedDevicesCache]);

  const nearbyDevices = useMemo(() => {
    if (runtimeState !== "running") return [];
    return devices
      .filter((device) => device.status === "online")
      .sort((a, b) => deviceDisplayName(a).localeCompare(deviceDisplayName(b)));
  }, [runtimeState, devices]);

  const activeProjections = useMemo(
    () =>
      Object.values(projections)
        .filter(isProjectionActive)
        .sort((a, b) => Number(b.updatedAt - a.updatedAt))
        .slice(0, 3),
    [projections],
  );

  const openDeviceDetail = useCallback(
    (device: DeviceInfo) => {
      router.push({
        pathname: "/device/[peerId]",
        params: { peerId: device.peerId },
      } as never);
    },
    [router],
  );

  const sendToDevice = useCallback(
    (device: DeviceInfo) => {
      if (!canSendToDevice(device)) {
        toast.info(t`设备当前不可发送`);
        return;
      }
      router.push({
        pathname: "/send/select-device",
        params: { peerId: device.peerId },
      } as never);
    },
    [router, t],
  );

  const openInbox = useCallback(() => {
    router.push("/inbox" as never);
  }, [router]);

  const openActivity = useCallback(() => {
    router.push("/activity" as never);
  }, [router]);

  const handleStartNode = useCallback(async () => {
    try {
      await startNode();
    } catch (err) {
      toast.error(t`启动节点失败`, errorMessage(err));
    }
  }, [startNode, t]);

  return (
    <AppScreen scroll testID="devices-screen" contentClassName="gap-5 pt-1">
      <AppHeader
        title={<Trans>设备中心</Trans>}
        subtitle={<Trans>连接附近设备，查看收件箱和活动</Trans>}
        right={
          <View className="flex-row items-center gap-2">
            <StatusPill
              state={runtimeState}
              onPress={() => nodeSheetRef.current?.present()}
            />
          </View>
        }
        testID="devices-header"
      />

      <HomeTransferPanel
        runtimeState={runtimeState}
        pairedCount={pairedDevices.length}
        nearbyCount={nearbyDevices.length}
        activeCount={activeProjections.length}
        onStart={() => void handleStartNode()}
        onOpenInbox={openInbox}
        onOpenActivity={openActivity}
      />

      {runtimeState === "running" ? (
        <AddDevicePanel nearbyDevices={nearbyDevices} onSend={sendToDevice} />
      ) : null}

      <View className="gap-3">
        <View className="flex-row items-center justify-between">
          <Text className="text-[15px] font-semibold text-foreground">
            <Trans>已配对设备</Trans>
            {pairedDevices.length > 0 ? ` (${pairedDevices.length})` : ""}
          </Text>
        </View>

        {pairedDevices.length === 0 ? (
          <EmptyState
            icon={Smartphone}
            title={<Trans>还没有配对设备</Trans>}
            description={
              <Trans>从上方附近设备发起配对，或让对方输入本机配对码。</Trans>
            }
            testID="devices-empty-state"
          />
        ) : (
          <DeviceGrid
            devices={pairedDevices}
            onPress={openDeviceDetail}
            onSend={sendToDevice}
          />
        )}
      </View>

      <View className="gap-3">
        <View className="flex-row items-center justify-between">
          <Text className="text-[15px] font-semibold text-foreground">
            <Trans>活跃传输</Trans>
          </Text>
          <Pressable
            onPress={openActivity}
            accessibilityRole="button"
            hitSlop={8}
            testID="devices-open-activity-button"
          >
            <Text className="text-[12px] font-semibold text-primary">
              <Trans>查看活动</Trans>
            </Text>
          </Pressable>
        </View>
        {activeProjections.length > 0 ? (
          <View className="gap-2">
            {activeProjections.map((projection) => (
              <RecentTransferRow
                key={projection.sessionId}
                projection={projection}
                progress={progressBySession[projection.sessionId]}
                onPress={(sessionId) =>
                  router.push({
                    pathname: "/transfer/[sessionId]",
                    params: { sessionId },
                  } as never)
                }
              />
            ))}
          </View>
        ) : (
          <EmptyState
            icon={Send}
            title={<Trans>没有正在进行的传输</Trans>}
            description={
              <Trans>开始发送或接收文件后，进度会显示在这里。</Trans>
            }
            className="min-h-48"
            testID="devices-empty-active-transfers"
          />
        )}
      </View>

      <NodeControlSheet ref={nodeSheetRef} />
    </AppScreen>
  );
}

function HomeTransferPanel({
  runtimeState,
  pairedCount,
  nearbyCount,
  activeCount,
  onStart,
  onOpenInbox,
  onOpenActivity,
}: {
  runtimeState: RuntimeState;
  pairedCount: number;
  nearbyCount: number;
  activeCount: number;
  onStart: () => void;
  onOpenInbox: () => void;
  onOpenActivity: () => void;
}) {
  const colors = useThemeColors();
  const isRunning = runtimeState === "running";
  const isStarting = runtimeState === "starting";
  const HeroIcon = isRunning ? Radio : Power;

  return (
    <Surface className="gap-4 rounded-2xl p-4" testID="devices-overview-panel">
      <View className="flex-row items-start gap-3">
        <View
          className={cn(
            "size-12 items-center justify-center rounded-2xl",
            isRunning ? "bg-primary/10" : "bg-muted",
          )}
        >
          <HeroIcon
            color={isRunning ? colors.primary : colors.mutedForeground}
            size={22}
          />
        </View>
        <View className="min-w-0 flex-1 gap-1">
          <Text className="text-[17px] font-semibold text-foreground">
            {isRunning ? (
              <Trans>节点运行中</Trans>
            ) : isStarting ? (
              <Trans>正在启动节点</Trans>
            ) : (
              <Trans>节点未启动</Trans>
            )}
          </Text>
          <Text className="text-[12px] leading-5 text-muted-foreground">
            {isRunning ? (
              <Trans>本机保持可发现，附近设备和配对码会持续更新。</Trans>
            ) : (
              <Trans>启动后会显示附近设备、本机配对码和输入配对码入口。</Trans>
            )}
          </Text>
        </View>
      </View>

      <View className="flex-row flex-wrap gap-x-4 gap-y-1">
        <InlineMetric value={pairedCount} label={<Trans>已配对</Trans>} />
        <InlineMetric
          value={isRunning ? nearbyCount : 0}
          label={<Trans>附近</Trans>}
        />
        <InlineMetric value={activeCount} label={<Trans>传输中</Trans>} />
      </View>

      <View className="flex-row items-center gap-2">
        {isRunning ? null : (
          <Pressable
            onPress={onStart}
            disabled={isStarting}
            accessibilityRole="button"
            testID="devices-start-node-button"
            className="h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-55"
          >
            {isStarting ? (
              <ActivityIndicator color={colors.primaryForeground} />
            ) : (
              <Power color={colors.primaryForeground} size={17} />
            )}
            <Text className="text-[14px] font-semibold text-primary-foreground">
              {isStarting ? <Trans>启动中</Trans> : <Trans>启动节点</Trans>}
            </Text>
          </Pressable>
        )}
        <HomeShortcut
          icon={Inbox}
          label={<Trans>收件箱</Trans>}
          onPress={onOpenInbox}
          testID="devices-open-inbox-button"
          className={isRunning ? "flex-1" : undefined}
        />
        <HomeShortcut
          icon={ActivityIcon}
          label={<Trans>活动</Trans>}
          onPress={onOpenActivity}
          testID="devices-open-activity-shortcut"
          className={isRunning ? "flex-1" : undefined}
        />
      </View>
    </Surface>
  );
}

function InlineMetric({
  value,
  label,
}: {
  value: number;
  label: React.ReactNode;
}) {
  return (
    <View className="flex-row items-baseline gap-1">
      <Text className="text-[13px] font-semibold text-foreground">{value}</Text>
      <Text className="text-[11px] text-muted-foreground">{label}</Text>
    </View>
  );
}

function HomeShortcut({
  icon: Icon,
  label,
  onPress,
  testID,
  className,
}: {
  icon: LucideIcon;
  label: React.ReactNode;
  onPress: () => void;
  testID: string;
  className?: string;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      testID={testID}
      className={cn(
        "h-12 min-w-[70px] flex-row items-center justify-center gap-1.5 rounded-xl border border-border px-2.5 active:opacity-70",
        className,
      )}
    >
      <Icon color={colors.foreground} size={15} />
      <Text className="text-[12px] font-semibold text-foreground">{label}</Text>
    </Pressable>
  );
}

function AddDevicePanel({
  nearbyDevices,
  onSend,
}: {
  nearbyDevices: DeviceInfo[];
  onSend: (device: DeviceInfo) => void;
}) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const pairingCodeSheetRef = useRef<PairingCodeSheetRef>(null);
  const [pairingPeer, setPairingPeer] = useState<string | null>(null);
  const [pairingError, setPairingError] = useState<string | null>(null);

  const handlePair = useCallback(
    async (device: DeviceInfo) => {
      if (device.isPaired) {
        onSend(device);
        return;
      }
      if (pairingPeer !== null) return;
      setPairingError(null);
      setPairingPeer(device.peerId);
      try {
        const result = await getMobileCore().requestPairing(
          device.peerId,
          undefined,
          [],
        );
        if (!result.accepted) {
          setPairingError(result.reason ?? t`配对被拒绝`);
          return;
        }
        router.push({
          pathname: "/pairing/success",
          params: {
            peerId: device.peerId,
            name: device.name ?? "",
            hostname: device.hostname,
            os: device.os,
            platform: device.platform,
            arch: device.arch,
          },
        });
      } catch (err) {
        setPairingError(errorMessage(err));
      } finally {
        setPairingPeer(null);
      }
    },
    [onSend, pairingPeer, router, t],
  );

  return (
    <>
      <Surface className="gap-4" testID="devices-add-panel">
        <View className="flex-row items-start justify-between gap-3">
          <View className="min-w-0 flex-1">
            <Text className="text-[15px] font-semibold text-foreground">
              <Trans>添加设备</Trans>
            </Text>
            <Text className="mt-0.5 text-[12px] text-muted-foreground">
              <Trans>附近设备和本机配对码会保持可见</Trans>
            </Text>
          </View>
          <View className="flex-row items-center gap-1.5 rounded-full bg-primary/10 px-2.5 py-1">
            <Radio color={colors.primary} size={13} />
            <Text className="text-[11px] font-medium text-primary">
              {nearbyDevices.length}
            </Text>
          </View>
        </View>

        <View className="gap-2.5">
          <Text className="text-[12px] font-semibold text-muted-foreground">
            <Trans>附近设备</Trans>
          </Text>
          {nearbyDevices.length === 0 ? (
            <InlineEmptyText>
              <Trans>
                暂无附近设备。确认对端 SwarmDrop 已启动，或使用下方配对码。
              </Trans>
            </InlineEmptyText>
          ) : (
            <View className="gap-2">
              {nearbyDevices.slice(0, 4).map((device) => (
                <NearbyDeviceRow
                  key={device.peerId}
                  device={device}
                  pairing={pairingPeer === device.peerId}
                  disabled={pairingPeer !== null}
                  onPress={handlePair}
                />
              ))}
              {nearbyDevices.length > 4 ? (
                <Text className="text-center text-[11px] text-muted-foreground">
                  <Trans>
                    还有 {nearbyDevices.length - 4} 台设备，可在配对完成后查看。
                  </Trans>
                </Text>
              ) : null}
            </View>
          )}
          {pairingError !== null ? (
            <Text className="text-[12px] text-destructive">{pairingError}</Text>
          ) : null}
        </View>

        <View className="h-px bg-border" />

        <PairingCodeCard />

        <Pressable
          onPress={() => pairingCodeSheetRef.current?.present()}
          accessibilityRole="button"
          testID="devices-open-input-code-sheet-button"
          className="min-h-[64px] flex-row items-center gap-3 rounded-xl border border-border bg-muted px-4 py-3 active:opacity-70"
        >
          <View className="size-11 items-center justify-center rounded-xl bg-card">
            <Keyboard color={colors.foreground} size={18} />
          </View>
          <View className="min-w-0 flex-1">
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>输入配对码</Trans>
            </Text>
            <Text
              className="mt-0.5 text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              <Trans>输入另一台设备显示的 6 位数字</Trans>
            </Text>
          </View>
          <ChevronRight color={colors.mutedForeground} size={18} />
        </Pressable>
      </Surface>
      <PairingCodeSheet ref={pairingCodeSheetRef} />
    </>
  );
}

function NearbyDeviceRow({
  device,
  pairing,
  disabled,
  onPress,
}: {
  device: DeviceInfo;
  pairing: boolean;
  disabled: boolean;
  onPress: (device: DeviceInfo) => void;
}) {
  const colors = useThemeColors();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);

  return (
    <Pressable
      onPress={() => onPress(device)}
      disabled={disabled}
      accessibilityRole="button"
      className="min-h-14 flex-row items-center gap-3 rounded-lg bg-muted px-3 py-2.5 active:opacity-70 disabled:opacity-55"
    >
      <View className="size-10 items-center justify-center rounded-full bg-card">
        <Icon color={colors.foreground} size={18} />
      </View>
      <View className="min-w-0 flex-1">
        <Text
          className="text-[13px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {deviceDisplayName(device)}
        </Text>
        <Text
          className="mt-0.5 text-[11px] text-muted-foreground"
          numberOfLines={1}
        >
          {device.isPaired ? (
            <Trans>已配对，可直接发送</Trans>
          ) : (
            <Trans>可配对</Trans>
          )}
        </Text>
      </View>
      {pairing ? (
        <ActivityIndicator color={colors.primary} />
      ) : (
        <View className="min-w-14 items-center rounded-full bg-primary px-3 py-1.5">
          <Text className="text-[12px] font-semibold text-primary-foreground">
            {device.isPaired ? <Trans>发送</Trans> : <Trans>配对</Trans>}
          </Text>
        </View>
      )}
    </Pressable>
  );
}

function PairingCodeCard() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const codeInfo = usePairingCodeStore((s) => s.codeInfo);
  const generating = usePairingCodeStore((s) => s.generating);
  const error = usePairingCodeStore((s) => s.error);
  const ensure = usePairingCodeStore((s) => s.ensure);
  const regenerate = usePairingCodeStore((s) => s.regenerate);

  useEffect(() => {
    void ensure();
  }, [ensure]);

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

  return (
    <View
      className="gap-3 rounded-lg bg-primary/5 p-3.5"
      testID="devices-local-code"
    >
      <View className="flex-row items-start justify-between gap-3">
        <View className="min-w-0 flex-1">
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>本机配对码</Trans>
          </Text>
          <Text className="mt-0.5 text-[11px] text-muted-foreground">
            <Trans>让另一台设备输入这组 6 位数字</Trans>
          </Text>
        </View>
        {code !== null ? (
          <Text className="rounded-full bg-card px-2 py-1 text-[11px] text-muted-foreground">
            {remaining > 0 ? t`${formatMmss(remaining)} 后过期` : t`已过期`}
          </Text>
        ) : null}
      </View>

      <View className="min-h-14 items-center justify-center rounded-lg border border-border bg-card px-3">
        {generating ? (
          <ActivityIndicator color={colors.primary} />
        ) : code !== null ? (
          <Text className="font-mono text-[28px] font-bold tracking-[7px] text-foreground">
            {code}
          </Text>
        ) : (
          <Text className="text-[12px] text-muted-foreground">
            {error ?? t`暂未生成配对码`}
          </Text>
        )}
      </View>

      <View className="flex-row gap-2">
        <Pressable
          onPress={() => void regenerate()}
          disabled={generating}
          accessibilityRole="button"
          className="h-10 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <RefreshCcw color={colors.foreground} size={14} />
          <Text className="text-[12px] font-semibold text-foreground">
            {code === null ? <Trans>生成</Trans> : <Trans>刷新</Trans>}
          </Text>
        </Pressable>
        <Pressable
          onPress={handleCopy}
          disabled={code === null}
          accessibilityRole="button"
          className="h-10 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <Copy
            color={
              code !== null ? colors.primaryForeground : colors.mutedForeground
            }
            size={14}
          />
          <Text
            className={
              code !== null
                ? "text-[12px] font-semibold text-primary-foreground"
                : "text-[12px] font-semibold text-muted-foreground"
            }
          >
            <Trans>复制</Trans>
          </Text>
        </Pressable>
      </View>
    </View>
  );
}

interface PairingCodeSheetRef {
  present: () => void;
  dismiss: () => void;
}

const PairingCodeSheet = forwardRef<PairingCodeSheetRef, object>(
  function PairingCodeSheet(_props, ref) {
    const sheetRef = useRef<BottomSheetModal>(null);
    const colors = useThemeColors();
    const [focusToken, setFocusToken] = useState(0);

    useImperativeHandle(ref, () => ({
      present: () => {
        sheetRef.current?.present();
        setFocusToken((value) => value + 1);
      },
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
          <View className="gap-4 px-5 pt-2 pb-6">
            <View className="items-center gap-2">
              <View className="size-12 items-center justify-center rounded-full bg-primary/10">
                <Keyboard color={colors.primary} size={22} />
              </View>
              <View className="items-center gap-1">
                <Text className="text-base font-bold text-foreground">
                  <Trans>输入配对码</Trans>
                </Text>
                <Text className="text-center text-[12px] leading-5 text-muted-foreground">
                  <Trans>输入另一台设备显示的 6 位数字</Trans>
                </Text>
              </View>
            </View>

            <PairingCodeInput
              focusToken={focusToken}
              onResolved={() => sheetRef.current?.dismiss()}
            />
          </View>
        </BottomSheetView>
      </BottomSheetModal>
    );
  },
);

const SLOT_KEYS = ["s0", "s1", "s2", "s3", "s4", "s5"] as const;

function PairingCodeInput({
  focusToken,
  onResolved,
}: {
  focusToken?: number;
  onResolved?: () => void;
}) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const otpRef = useRef<OTPInputRef>(null);
  const [code, setCode] = useState("");
  const [looking, setLooking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!focusToken) return;
    const id = setTimeout(() => otpRef.current?.focus(), 250);
    return () => clearTimeout(id);
  }, [focusToken]);

  const onLookup = async (filled: string) => {
    if (looking || filled.length !== 6) return;
    setError(null);
    setLooking(true);
    try {
      const remote = await getMobileCore().lookupDeviceByCode(filled);
      onResolved?.();
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
      console.warn("[devices] lookupDeviceByCode failed:", errorMessage(err));
    } finally {
      setLooking(false);
    }
  };

  return (
    <View className="items-center gap-3 rounded-lg border border-border bg-muted p-3.5">
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
          ? "h-12 w-10 items-center justify-center rounded-[10px] border-2 border-primary bg-muted"
          : "h-12 w-10 items-center justify-center rounded-[10px] border border-border bg-muted"
      }
    >
      {char !== null ? (
        <Text className="text-[22px] font-bold text-foreground">{char}</Text>
      ) : null}
    </View>
  );
}

function InlineEmptyText({ children }: { children: React.ReactNode }) {
  return (
    <View className="rounded-lg bg-muted px-3 py-3">
      <Text className="text-[12px] leading-5 text-muted-foreground">
        {children}
      </Text>
    </View>
  );
}

function formatMmss(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function DeviceGrid({
  devices,
  onPress,
  onSend,
}: {
  devices: DeviceInfo[];
  onPress: (d: DeviceInfo) => void;
  onSend: (d: DeviceInfo) => void;
}) {
  return (
    <View className="gap-2.5">
      {devices.map((device, idx) => (
        <DeviceCard
          key={device.peerId}
          device={device}
          variant="row"
          testID={`device-card-${idx}`}
          onPress={onPress}
          onSend={onSend}
        />
      ))}
    </View>
  );
}
