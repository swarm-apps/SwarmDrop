import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { OTPInput, type OTPInputRef, type SlotProps } from "input-otp-native";
import {
  Activity as ActivityIcon,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Inbox,
  Keyboard,
  type LucideIcon,
  OctagonAlert,
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
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
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
        errorSummary={lastNodeError}
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
  errorSummary,
  onStart,
  onOpenInbox,
  onOpenActivity,
}: {
  runtimeState: RuntimeState;
  pairedCount: number;
  nearbyCount: number;
  activeCount: number;
  errorSummary: string | null;
  onStart: () => void;
  onOpenInbox: () => void;
  onOpenActivity: () => void;
}) {
  const colors = useThemeColors();
  const isRunning = runtimeState === "running";
  const isStarting = runtimeState === "starting";
  const isError = runtimeState === "error";
  const HeroIcon = isRunning ? Radio : isError ? OctagonAlert : Power;

  return (
    <Surface className="gap-4 rounded-2xl p-4" testID="devices-overview-panel">
      <View className="flex-row items-start gap-3">
        <View
          className={cn(
            "size-12 items-center justify-center rounded-2xl",
            isRunning
              ? "bg-primary/10"
              : isError
                ? "bg-destructive/10"
                : "bg-muted",
          )}
        >
          <HeroIcon
            color={
              isRunning
                ? colors.primary
                : isError
                  ? colors.destructive
                  : colors.mutedForeground
            }
            size={22}
          />
        </View>
        <View className="min-w-0 flex-1 gap-1">
          <Text className="text-[17px] font-semibold text-foreground">
            {isRunning ? (
              <Trans>节点运行中</Trans>
            ) : isStarting ? (
              <Trans>正在启动节点</Trans>
            ) : isError ? (
              <Trans>节点出错</Trans>
            ) : (
              <Trans>节点未启动</Trans>
            )}
          </Text>
          <Text
            className={cn(
              "text-[12px] leading-5",
              isError ? "text-destructive" : "text-muted-foreground",
            )}
            numberOfLines={isError ? 3 : undefined}
          >
            {isRunning ? (
              <Trans>本机保持可发现，附近设备和配对码会持续更新。</Trans>
            ) : isError ? (
              (errorSummary ?? <Trans>节点异常停止，请重试启动。</Trans>)
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
            testID={
              isError
                ? "devices-retry-node-button"
                : "devices-start-node-button"
            }
            className="h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary px-4 active:opacity-70 disabled:opacity-55"
          >
            {isStarting ? (
              <ActivityIndicator color={colors.primaryForeground} />
            ) : isError ? (
              <RefreshCcw color={colors.primaryForeground} size={17} />
            ) : (
              <Power color={colors.primaryForeground} size={17} />
            )}
            <Text className="text-[14px] font-semibold text-primary-foreground">
              {isStarting ? (
                <Trans>启动中</Trans>
              ) : isError ? (
                <Trans>重试</Trans>
              ) : (
                <Trans>启动节点</Trans>
              )}
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
  const [expanded, setExpanded] = useState(false);
  const [pairingPeer, setPairingPeer] = useState<string | null>(null);
  const [pairingError, setPairingError] = useState<string | null>(null);

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
      <Surface className="gap-4" testID="devices-add-panel">
        <Pressable
          onPress={() => setExpanded((value) => !value)}
          accessibilityRole="button"
          accessibilityState={{ expanded }}
          testID="devices-add-panel-toggle"
          className="flex-row items-start justify-between gap-3 active:opacity-70"
        >
          <View className="min-w-0 flex-1">
            <Text className="text-[15px] font-semibold text-foreground">
              <Trans>添加设备</Trans>
            </Text>
            <Text className="mt-0.5 text-[12px] text-muted-foreground">
              {expanded ? (
                <Trans>附近设备和本机配对码会保持可见</Trans>
              ) : (
                <Trans>展开查看附近设备、本机配对码和输入配对码</Trans>
              )}
            </Text>
          </View>
          <View className="flex-row items-center gap-2">
            <View className="flex-row items-center gap-1.5 rounded-full bg-primary/10 px-2.5 py-1">
              <Radio color={colors.primary} size={13} />
              <Text className="text-[11px] font-medium text-primary">
                {nearbyDevices.length}
              </Text>
            </View>
            {expanded ? (
              <ChevronUp color={colors.mutedForeground} size={18} />
            ) : (
              <ChevronDown color={colors.mutedForeground} size={18} />
            )}
          </View>
        </Pressable>

        {expanded ? (
          <>
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
                    <InlineEmptyText>
                      <Trans>
                        没有符合条件的附近设备，换一个筛选条件试试。
                      </Trans>
                    </InlineEmptyText>
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
                          <Text className="text-[12px] font-semibold text-primary">
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
                <Text className="text-[12px] text-destructive">
                  {pairingError}
                </Text>
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
          </>
        ) : null}
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
      <AppBottomSheet ref={sheetRef}>
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
