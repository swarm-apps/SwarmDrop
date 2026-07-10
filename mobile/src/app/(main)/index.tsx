import { useBottomSheetInternal } from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { OTPInput, type OTPInputRef, type SlotProps } from "input-otp-native";
import {
  ArrowLeftRight,
  ChevronRight,
  Copy,
  Keyboard,
  OctagonAlert,
  Plus,
  Power,
  Radar,
  Radio,
  RefreshCcw,
  SearchX,
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
import {
  ActivityIndicator,
  findNodeHandle,
  Pressable,
  TextInput as RNTextInput,
  View,
} from "react-native";
import Animated from "react-native-reanimated";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { DeviceCard } from "@/components/device-card";
import {
  AppHeader,
  AppScreen,
  BottomActionArea,
  EmptyState,
  InlineEmptyState,
  Surface,
} from "@/components/mobile/screen";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { RecentTransferRow } from "@/components/recent-transfer-row";
import { StatusPill } from "@/components/status-pill";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { Text } from "@/components/ui/text";
import { canSendToDevice } from "@/core/device-trust";
import { getMobileCore } from "@/core/mobile-core";
import {
  compareProjectionsByUpdatedAtDesc,
  isProjectionActive,
} from "@/core/transfer-types";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { usePulseOpacity } from "@/hooks/usePulseOpacity";
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
  const addDeviceSheetRef = useRef<AddDeviceSheetRef>(null);

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

  // store 的 error 是瞬时 toast 通道(下面的 effect 里读到就立刻清空)，
  // 这里单独留一份"最近一次失败原因"给 error 态面板长期展示用。
  const [lastNodeError, setLastNodeError] = useState<string | null>(null);

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
      setLastNodeError(error);
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
        .sort(compareProjectionsByUpdatedAtDesc)
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

  const unpairedNearbyCount = useMemo(
    () => nearbyDevices.filter((device) => !device.isPaired).length,
    [nearbyDevices],
  );

  const openActivity = useCallback(() => {
    router.push("/activity" as never);
  }, [router]);

  const openTransfer = useCallback(
    (sessionId: string) => {
      router.push({
        pathname: "/transfer/[sessionId]",
        params: { sessionId },
      } as never);
    },
    [router],
  );

  const handleStartNode = useCallback(async () => {
    try {
      const result = await startNode();
      if (!result.ok) {
        toast.error(t`启动节点失败`, result.error);
      }
    } catch (err) {
      toast.error(t`启动节点失败`, errorMessage(err));
    }
  }, [startNode, t]);

  return (
    <AppScreen
      scroll
      testID="devices-screen"
      contentClassName="gap-5 pt-1"
      footer={
        <HomeDock
          runtimeState={runtimeState}
          unpairedNearbyCount={unpairedNearbyCount}
          onStart={() => void handleStartNode()}
          onAddDevice={() => addDeviceSheetRef.current?.present()}
        />
      }
    >
      <AppHeader
        title={<Trans>设备中心</Trans>}
        subtitle={<Trans>自家设备都在这儿，东西到了会自动收下</Trans>}
        right={
          <StatusPill
            state={runtimeState}
            onPress={() => nodeSheetRef.current?.present()}
            testID="devices-manage-node-button"
          />
        }
        testID="devices-header"
      />

      {runtimeState !== "running" ? (
        <NodeStatePanel
          runtimeState={runtimeState}
          errorSummary={lastNodeError}
        />
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
              <Trans>点下方「添加设备」，把你的电脑或另一台手机接进来。</Trans>
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
            <Text className="text-[12px] font-semibold text-primary-ink">
              <Trans>查看全部</Trans>
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
                onPress={openTransfer}
              />
            ))}
          </View>
        ) : (
          <InlineEmptyState
            icon={ArrowLeftRight}
            title={<Trans>现在没有正在进行的传输</Trans>}
            description={<Trans>收到文件会第一时间出现在这儿</Trans>}
            testID="devices-empty-active-transfers"
          />
        )}
      </View>

      {__DEV__ ? (
        <Pressable
          onPress={() => router.push("/e2e/file-browser" as never)}
          accessibilityRole="button"
          accessibilityLabel="Open file browser fixture"
          testID="devices-open-file-browser-fixture"
          className="min-h-11 items-center justify-center rounded-lg border border-dashed border-border active:opacity-70"
        >
          <Text className="text-[11px] text-muted-foreground">
            WebDriver · FileBrowser fixture
          </Text>
        </Pressable>
      ) : null}

      <NodeControlSheet ref={nodeSheetRef} />
      <AddDeviceSheet
        ref={addDeviceSheetRef}
        nearbyDevices={nearbyDevices}
        onSend={sendToDevice}
      />
    </AppScreen>
  );
}

/**
 * 非运行态说明面板:只解释状态,不放操作——启动/重试都在底部 HomeDock,
 * 运行态整块不渲染(设备列表即主屏主体)。
 */
function NodeStatePanel({
  runtimeState,
  errorSummary,
}: {
  runtimeState: RuntimeState;
  errorSummary: string | null;
}) {
  const colors = useThemeColors();
  const isStarting = runtimeState === "starting";
  const isError = runtimeState === "error";
  const Icon = isError ? OctagonAlert : isStarting ? Radio : Power;

  return (
    <Surface className="gap-3" testID="devices-overview-panel">
      <View className="flex-row items-start gap-3">
        <View
          className={cn(
            "size-12 items-center justify-center rounded-full",
            isError ? "bg-destructive/10" : "bg-muted",
          )}
        >
          <Icon
            color={isError ? colors.destructive : colors.mutedForeground}
            size={22}
          />
        </View>
        <View className="min-w-0 flex-1 gap-1">
          <Text className="text-[15px] font-semibold text-foreground">
            {isStarting ? (
              <Trans>正在启动</Trans>
            ) : isError ? (
              <Trans>刚才没能启动</Trans>
            ) : (
              <Trans>SwarmDrop 还没上线</Trans>
            )}
          </Text>
          <Text
            className={cn(
              "text-[12px] leading-5",
              isError ? "text-destructive-ink" : "text-muted-foreground",
            )}
            numberOfLines={isError ? 3 : undefined}
          >
            {isStarting ? (
              <Trans>马上就好，正在让附近设备能找到你。</Trans>
            ) : isError ? (
              (errorSummary ?? <Trans>出了点问题，点下方「再试一次」。</Trans>)
            ) : (
              <Trans>启动后，你的其他设备才能找到这台手机。</Trans>
            )}
          </Text>
        </View>
      </View>
    </Surface>
  );
}

/**
 * 底部常驻操作条:任一时刻只有一个主动作(启动/再试一次/添加设备),全部落在拇指区。
 * testID 沿用旧的启动/重试值以保持 Maestro 流程;运行态对齐 mobile-foundation 的
 * devices-add-device-button。
 */
function HomeDock({
  runtimeState,
  unpairedNearbyCount,
  onStart,
  onAddDevice,
}: {
  runtimeState: RuntimeState;
  unpairedNearbyCount: number;
  onStart: () => void;
  onAddDevice: () => void;
}) {
  const colors = useThemeColors();
  const isRunning = runtimeState === "running";
  const isStarting = runtimeState === "starting";
  const isError = runtimeState === "error";

  return (
    <BottomActionArea>
      <Pressable
        onPress={isRunning ? onAddDevice : onStart}
        disabled={isStarting}
        accessibilityRole="button"
        testID={
          isRunning
            ? "devices-add-device-button"
            : isError
              ? "devices-retry-node-button"
              : "devices-start-node-button"
        }
        className="h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-50"
      >
        {isStarting ? (
          <ActivityIndicator color={colors.primaryForeground} />
        ) : isRunning ? (
          <Plus color={colors.primaryForeground} size={17} />
        ) : isError ? (
          <RefreshCcw color={colors.primaryForeground} size={17} />
        ) : (
          <Power color={colors.primaryForeground} size={17} />
        )}
        <Text className="text-[14px] font-semibold text-primary-foreground">
          {isStarting ? (
            <Trans>正在启动…</Trans>
          ) : isRunning ? (
            <Trans>添加设备</Trans>
          ) : isError ? (
            <Trans>再试一次</Trans>
          ) : (
            <Trans>启动</Trans>
          )}
        </Text>
        {isRunning && unpairedNearbyCount > 0 ? (
          <Text className="text-[11px] font-medium text-primary-foreground">
            <Trans>· 附近发现 {unpairedNearbyCount} 台</Trans>
          </Text>
        ) : null}
      </Pressable>
    </BottomActionArea>
  );
}

type NearbyFilter = "all" | "unpaired" | "paired";

const NEARBY_COLLAPSED_COUNT = 4;
// 客户端配对超时兜底：原生 req_resp 层本身有约 120s 超时,这里的 15s 只是 UI 层面
// 更快地恢复列表/给出提示,不重复取消底层原生请求(先触发者为准,详见 design.md)。
const PAIRING_TIMEOUT_MS = 15_000;

function NearbyFilterLabel({ value }: { value: NearbyFilter }) {
  switch (value) {
    case "unpaired":
      return <Trans>可配对</Trans>;
    case "paired":
      return <Trans>已配对</Trans>;
    default:
      return <Trans>全部</Trans>;
  }
}

interface AddDeviceSheetRef {
  present: () => void;
}

/**
 * 「添加设备」bottom sheet:附近设备 + 本机配对码 + 输入配对码,配对全流程发生在拇指区。
 * 原主屏 AddDevicePanel 的展开态内容原样搬入;配对状态机(attempt 计数器防迟到结果)不变。
 */
const AddDeviceSheet = forwardRef<
  AddDeviceSheetRef,
  {
    nearbyDevices: DeviceInfo[];
    onSend: (device: DeviceInfo) => void;
  }
>(function AddDeviceSheet({ nearbyDevices, onSend }, ref) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const sheetRef = useRef<AppBottomSheetRef>(null);
  const pairingCodeSheetRef = useRef<PairingCodeSheetRef>(null);
  const [pairingPeer, setPairingPeer] = useState<string | null>(null);
  const [pairingError, setPairingError] = useState<string | null>(null);

  useImperativeHandle(ref, () => ({
    present: () => sheetRef.current?.present(),
  }));

  // "当前尝试" 计数器 + 超时定时器：取消/超时都只推进这个计数器让旧结果失效,
  // 不会二次调用原生取消,也不会让迟到的 resolve/reject 覆盖新一轮配对状态。
  const pairingAttemptRef = useRef(0);
  const pairingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearPairingTimeout = useCallback(() => {
    if (pairingTimeoutRef.current !== null) {
      clearTimeout(pairingTimeoutRef.current);
      pairingTimeoutRef.current = null;
    }
  }, []);

  useEffect(() => clearPairingTimeout, [clearPairingTimeout]);

  const handleCancelPairing = useCallback(() => {
    pairingAttemptRef.current += 1;
    clearPairingTimeout();
    setPairingPeer(null);
    setPairingError(null);
  }, [clearPairingTimeout]);

  const handlePair = useCallback(
    async (device: DeviceInfo) => {
      if (device.isPaired) {
        onSend(device);
        return;
      }
      if (pairingPeer !== null) return;
      setPairingError(null);
      setPairingPeer(device.peerId);
      const attempt = ++pairingAttemptRef.current;
      clearPairingTimeout();
      pairingTimeoutRef.current = setTimeout(() => {
        if (pairingAttemptRef.current !== attempt) return;
        pairingAttemptRef.current += 1;
        setPairingPeer(null);
        setPairingError(t`对方无响应，请重试`);
      }, PAIRING_TIMEOUT_MS);
      try {
        const result = await getMobileCore().requestPairing(
          device.peerId,
          undefined,
          [],
        );
        // 期间被取消或已超时——这轮结果已过期,不再处理
        if (pairingAttemptRef.current !== attempt) return;
        clearPairingTimeout();
        setPairingPeer(null);
        if (!result.accepted) {
          setPairingError(result.reason ?? t`配对被拒绝`);
          return;
        }
        // 成功先收起 sheet 再跳成功页,返回时不残留半开的 sheet
        sheetRef.current?.dismiss();
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
        if (pairingAttemptRef.current !== attempt) return;
        clearPairingTimeout();
        setPairingPeer(null);
        setPairingError(errorMessage(err));
      }
    },
    [onSend, pairingPeer, router, t, clearPairingTimeout],
  );

  const [nearbyFilter, setNearbyFilter] = useState<NearbyFilter>("all");
  const [showAllNearby, setShowAllNearby] = useState(false);

  // 「输入配对码」要等本 sheet 收起动画结束再唤起,避免两个 modal 叠加——
  // 收起完成的时机由 onDismiss 给出,不硬编码动画时长。
  const pendingInputCodeRef = useRef(false);

  const openInputCodeSheet = useCallback(() => {
    pendingInputCodeRef.current = true;
    sheetRef.current?.dismiss();
  }, []);

  const handleSheetDismiss = useCallback(() => {
    if (!pendingInputCodeRef.current) return;
    pendingInputCodeRef.current = false;
    pairingCodeSheetRef.current?.present();
  }, []);

  const filteredNearby = useMemo(() => {
    if (nearbyFilter === "unpaired") {
      return nearbyDevices.filter((device) => !device.isPaired);
    }
    if (nearbyFilter === "paired") {
      return nearbyDevices.filter((device) => device.isPaired);
    }
    return nearbyDevices;
  }, [nearbyDevices, nearbyFilter]);

  const visibleNearby = showAllNearby
    ? filteredNearby
    : filteredNearby.slice(0, NEARBY_COLLAPSED_COUNT);
  const hiddenNearbyCount = filteredNearby.length - visibleNearby.length;

  return (
    <>
      <AppBottomSheet
        ref={sheetRef}
        scrollable
        contentTestID="pairing-sheet-content"
        onDismiss={handleSheetDismiss}
      >
        <View className="gap-4 px-5 pt-2 pb-8">
          <View className="items-center gap-2">
            <View className="size-12 items-center justify-center rounded-full bg-primary/10">
              <Radio color={colors.primary} size={22} />
            </View>
            <View className="items-center gap-1">
              <Text className="text-base font-bold text-foreground">
                <Trans>添加设备</Trans>
              </Text>
              <Text className="text-center text-[12px] leading-5 text-muted-foreground">
                <Trans>附近的设备会自动出现；也可以用 6 位配对码互相认识</Trans>
              </Text>
            </View>
          </View>

          <View className="gap-2.5">
            <Text className="text-[12px] font-semibold text-muted-foreground">
              <Trans>附近设备</Trans>
            </Text>
            {nearbyDevices.length === 0 ? (
              <InlineEmptyState
                icon={Radar}
                pulse
                title={<Trans>正在留意附近的设备</Trans>}
                description={
                  <Trans>确认对端 SwarmDrop 已启动，或使用下方配对码</Trans>
                }
              />
            ) : (
              <>
                <View
                  className="flex-row gap-1 rounded-lg bg-muted p-0.5"
                  testID="nearby-filter-control"
                >
                  {(["all", "unpaired", "paired"] as const).map((key) => (
                    <Pressable
                      key={key}
                      onPress={() => {
                        setNearbyFilter(key);
                        setShowAllNearby(false);
                      }}
                      accessibilityRole="button"
                      accessibilityState={{ selected: nearbyFilter === key }}
                      testID={`nearby-filter-${key}`}
                      className={cn(
                        "flex-1 items-center rounded-md px-2 py-1.5 active:opacity-70",
                        nearbyFilter === key ? "bg-card" : "",
                      )}
                    >
                      <Text
                        className={cn(
                          "text-[11px] font-medium",
                          nearbyFilter === key
                            ? "text-foreground"
                            : "text-muted-foreground",
                        )}
                      >
                        <NearbyFilterLabel value={key} />
                      </Text>
                    </Pressable>
                  ))}
                </View>
                {filteredNearby.length === 0 ? (
                  <InlineEmptyState
                    icon={SearchX}
                    title={<Trans>没有符合条件的附近设备</Trans>}
                    description={<Trans>换一个筛选条件试试</Trans>}
                  />
                ) : (
                  <View className="gap-2">
                    {visibleNearby.map((device) => (
                      <NearbyDeviceRow
                        key={device.peerId}
                        device={device}
                        pairing={pairingPeer === device.peerId}
                        disabled={pairingPeer !== null}
                        onPress={handlePair}
                        onCancel={handleCancelPairing}
                      />
                    ))}
                    {hiddenNearbyCount > 0 ? (
                      <Pressable
                        onPress={() => setShowAllNearby(true)}
                        accessibilityRole="button"
                        testID="nearby-show-all-button"
                        className="min-h-9 items-center justify-center rounded-lg active:opacity-70"
                      >
                        <Text className="text-[12px] font-semibold text-primary-ink">
                          <Trans>查看全部 ({filteredNearby.length})</Trans>
                        </Text>
                      </Pressable>
                    ) : showAllNearby &&
                      filteredNearby.length > NEARBY_COLLAPSED_COUNT ? (
                      <Pressable
                        onPress={() => setShowAllNearby(false)}
                        accessibilityRole="button"
                        testID="nearby-collapse-button"
                        className="min-h-9 items-center justify-center rounded-lg active:opacity-70"
                      >
                        <Text className="text-[12px] font-semibold text-muted-foreground">
                          <Trans>收起</Trans>
                        </Text>
                      </Pressable>
                    ) : null}
                  </View>
                )}
              </>
            )}
            {pairingError !== null ? (
              <Text className="text-[12px] text-destructive-ink">
                {pairingError}
              </Text>
            ) : null}
          </View>

          <View className="h-px bg-border" />

          <PairingCodeCard />

          <Pressable
            onPress={openInputCodeSheet}
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
        </View>
      </AppBottomSheet>
      <PairingCodeSheet ref={pairingCodeSheetRef} />
    </>
  );
});

function NearbyDeviceRow({
  device,
  pairing,
  disabled,
  onPress,
  onCancel,
}: {
  device: DeviceInfo;
  pairing: boolean;
  disabled: boolean;
  onPress: (device: DeviceInfo) => void;
  onCancel: () => void;
}) {
  const colors = useThemeColors();
  const { t } = useLingui();
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);

  return (
    <Pressable
      onPress={() => onPress(device)}
      disabled={pairing ? false : disabled}
      accessibilityRole="button"
      className="min-h-14 flex-row items-center gap-3 rounded-lg bg-muted px-3 py-2.5 active:opacity-70 disabled:opacity-50"
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
          {pairing ? (
            <Trans>正在配对...</Trans>
          ) : device.isPaired ? (
            <Trans>已配对，可直接发送</Trans>
          ) : (
            <Trans>可配对</Trans>
          )}
        </Text>
      </View>
      {pairing ? (
        <View className="flex-row items-center gap-2">
          <ActivityIndicator color={colors.primary} />
          <Pressable
            onPress={(event) => {
              event.stopPropagation();
              onCancel();
            }}
            accessibilityRole="button"
            accessibilityLabel={t`取消配对`}
            hitSlop={8}
            testID={`nearby-cancel-pairing-${device.peerId}`}
            className="min-w-14 items-center rounded-full border border-border px-3 py-1.5 active:opacity-70"
          >
            <Text className="text-[12px] font-semibold text-muted-foreground">
              <Trans>取消</Trans>
            </Text>
          </Pressable>
        </View>
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
          // 骨架条镜像 6 位配对码的占位形状,避免生成时框内闪 spinner
          <Skeleton className="h-8 w-44" />
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
    const sheetRef = useRef<AppBottomSheetRef>(null);
    const colors = useThemeColors();
    const [focusToken, setFocusToken] = useState(0);

    useImperativeHandle(ref, () => ({
      present: () => {
        sheetRef.current?.present();
        setFocusToken((value) => value + 1);
      },
      dismiss: () => sheetRef.current?.dismiss(),
    }));

    return (
      <AppBottomSheet
        ref={sheetRef}
        keyboardBehavior="interactive"
        keyboardBlurBehavior="restore"
      >
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
      </AppBottomSheet>
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

  // OTP 库内部是普通 TextInput(非 BottomSheetTextInput),而 sheet 的键盘避让被
  // keyboardState.target 门控 —— 无 target 时 keyboardWillShow 只被缓存,sheet 纹丝不动
  // 被键盘盖住。focus 落地后手动把当前聚焦的原生输入登记为目标,缓存事件会被重放,
  // sheet 随键盘上移。
  const { animatedKeyboardState } = useBottomSheetInternal();

  useEffect(() => {
    if (!focusToken) return;
    const id = setTimeout(() => {
      otpRef.current?.focus();
      requestAnimationFrame(() => {
        // Fabric 下 currentlyFocusedInput() 返回 ReactNativeElement,findNodeHandle
        // 运行时兼容但类型签名未更新,cast 绕过
        const focused = RNTextInput.State.currentlyFocusedInput();
        const node = focused ? findNodeHandle(focused as never) : null;
        if (node != null) {
          animatedKeyboardState.set((state) => ({ ...state, target: node }));
        }
      });
    }, 250);
    return () => clearTimeout(id);
  }, [focusToken, animatedKeyboardState]);

  // 卸载(sheet dismiss)时注销键盘目标,不残留给下一个 sheet
  useEffect(
    () => () => {
      animatedKeyboardState.set((state) => ({ ...state, target: undefined }));
    },
    [animatedKeyboardState],
  );

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
    <View className="items-center gap-3">
      <OTPInput
        ref={otpRef}
        maxLength={6}
        value={code}
        onChange={setCode}
        onComplete={onLookup}
        textAlign="center"
        render={({ slots }) => (
          <View className="flex-row items-center justify-center gap-2.5">
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
        <Text className="text-[12px] text-destructive-ink">{error}</Text>
      ) : looking ? (
        <ActivityIndicator color={colors.mutedForeground} />
      ) : null}
    </View>
  );
}

// active 只换颜色不加粗边框(1px 恒定,框体零跳动);数字走 font-mono,与本机配对码
// 展示卡(28px mono)同一视觉家族;插入点用闪烁 caret 指示,空 active 框不再无声。
function OtpSlot({ char, isActive, hasFakeCaret }: SlotProps) {
  return (
    <View
      className={cn(
        "h-14 w-11 items-center justify-center rounded-lg border",
        isActive ? "border-primary bg-primary/5" : "border-border bg-muted",
      )}
    >
      {char !== null ? (
        <Text className="font-mono text-2xl font-bold text-foreground">
          {char}
        </Text>
      ) : hasFakeCaret ? (
        <OtpCaret />
      ) : null}
    </View>
  );
}

function OtpCaret() {
  const colors = useThemeColors();
  const style = usePulseOpacity({ min: 0, duration: 500 });

  return (
    <Animated.View
      style={[
        style,
        {
          width: 2,
          height: 26,
          borderRadius: 1,
          backgroundColor: colors.primary,
        },
      ]}
    />
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
