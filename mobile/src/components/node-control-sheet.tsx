import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { ChevronDown, Copy, Power } from "lucide-react-native";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import Animated, { FadeIn } from "react-native-reanimated";
import { useShallow } from "zustand/react/shallow";
import { StatusPill } from "@/components/status-pill";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import {
  candidateSourceKey,
  discoveryModeFromNative,
} from "@/core/network-discovery";
import { useThemeColors } from "@/hooks/useThemeColors";
import { formatUptime } from "@/lib/format-uptime";
import { toast } from "@/lib/toast";
import { truncateMiddle } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

export interface NodeControlSheetRef {
  present: () => void;
  dismiss: () => void;
}

/**
 * 节点控制 BottomSheet —— 对齐桌面 StopNodeSheet/StartNodeSheet。
 * - running:展示详情 + 「停止节点」(destructive)
 * - stopped/error:展示精简提示 + 「启动节点」
 * - starting:加载中提示
 *
 * 高度走 AppBottomSheet 的内容自适应(不再写死百分比)——停止态只有一句提示时,
 * sheet 会收缩到贴合内容,不再撑出大片留白。
 */
export const NodeControlSheet = forwardRef<NodeControlSheetRef, object>(
  function NodeControlSheet(_props, ref) {
    const sheetRef = useRef<AppBottomSheetRef>(null);

    useImperativeHandle(ref, () => ({
      present: () => sheetRef.current?.present(),
      dismiss: () => sheetRef.current?.dismiss(),
    }));

    return (
      <AppBottomSheet ref={sheetRef} contentTestID="node-control-sheet-content">
        <NodeControlContent onDismiss={() => sheetRef.current?.dismiss()} />
      </AppBottomSheet>
    );
  },
);

function NodeControlContent({ onDismiss }: { onDismiss: () => void }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const {
    runtimeState,
    networkStatus,
    peerId,
    startedAt,
    startNode,
    shutdownNode,
  } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      networkStatus: s.networkStatus,
      peerId: s.peerId,
      startedAt: s.startedAt,
      startNode: s.startNode,
      shutdownNode: s.shutdownNode,
    })),
  );

  const [working, setWorking] = useState(false);
  // 停止节点二次确认 —— 与阻止设备/清空活动一样走 in-app ConfirmDialog(不再用系统原生 Alert)。
  const [confirmStopOpen, setConfirmStopOpen] = useState(false);
  // 网络诊断默认收起 —— 消费级弹窗只默认展示"在线/已连接/运行时长",
  // libp2p 细节(Peer ID / NAT / 中继 / 引导节点…)藏进渐进披露。
  const [showDiagnostics, setShowDiagnostics] = useState(false);

  const handleStart = async () => {
    setWorking(true);
    try {
      const result = await startNode();
      if (result.ok) {
        onDismiss();
      } else {
        toast.error(t`启动节点失败`, result.error);
      }
    } finally {
      setWorking(false);
    }
  };

  const performStop = async () => {
    setWorking(true);
    try {
      const result = await shutdownNode();
      if (result.ok) {
        onDismiss();
      } else {
        toast.error(t`停止节点失败`, result.error);
      }
    } finally {
      setWorking(false);
    }
  };

  const handleCopyPeerId = async () => {
    if (!peerId) return;
    await Clipboard.setStringAsync(peerId);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制 Peer ID`);
  };

  const isRunning = runtimeState === "running";
  const isStarting = runtimeState === "starting";

  return (
    <>
      <View className="gap-4 px-5 pt-2 pb-6">
        <View className="items-center gap-2">
          <View
            className={
              isRunning
                ? "size-14 items-center justify-center rounded-full bg-success/10"
                : "size-14 items-center justify-center rounded-full bg-muted"
            }
          >
            <Power
              color={isRunning ? colors.success : colors.mutedForeground}
              size={26}
            />
          </View>
          <Text className="text-base font-bold text-foreground">
            <Trans>网络节点</Trans>
          </Text>
          <StatusPill state={runtimeState} size="md" />
        </View>

        {isRunning ? (
          <View className="gap-3">
            {/* 消费级摘要 —— 始终可见,只回答"有几台设备连得上""开了多久" */}
            <View className="overflow-hidden rounded-xl border border-border">
              <Row
                label={<Trans>已连接设备</Trans>}
                value={String(networkStatus?.connectedPeers ?? 0)}
              />
              <Divider />
              <Row
                label={<Trans>已发现设备</Trans>}
                value={String(networkStatus?.discoveredPeers ?? 0)}
              />
              <Divider />
              <Row
                label={<Trans>运行时长</Trans>}
                value={<UptimeValue startedAt={startedAt} />}
              />
            </View>

            {/* 网络诊断 —— 渐进披露,默认收起,libp2p 细节留给需要的人 */}
            <View className="overflow-hidden rounded-xl border border-border">
              <Pressable
                onPress={() => setShowDiagnostics((v) => !v)}
                accessibilityRole="button"
                accessibilityLabel={t`网络诊断`}
                accessibilityState={{ expanded: showDiagnostics }}
                testID="node-control-diagnostics-toggle"
                className="min-h-11 flex-row items-center justify-between px-3.5 py-2.5 active:opacity-70"
              >
                <Text className="text-[13px] font-medium text-foreground">
                  <Trans>网络诊断</Trans>
                </Text>
                <ChevronDown
                  size={16}
                  color={colors.mutedForeground}
                  style={{
                    transform: [
                      { rotate: showDiagnostics ? "180deg" : "0deg" },
                    ],
                  }}
                />
              </Pressable>
              {showDiagnostics ? (
                <Animated.View entering={FadeIn.duration(160)}>
                  <Divider />
                  {/* Peer ID 可点按复制 —— P2P App 里"复制自己的身份"是刚需 */}
                  <Row
                    label={<Trans>Peer ID</Trans>}
                    value={peerId ? truncateMiddle(peerId, 6, 4) : "—"}
                    mono
                    onCopy={peerId ? handleCopyPeerId : undefined}
                    copyAccessibilityLabel={t`复制 Peer ID`}
                  />
                  <Divider />
                  <Row
                    label={<Trans>发现方式</Trans>}
                    value={
                      discoveryModeFromNative(networkStatus?.discoveryMode) ===
                      "auto"
                        ? t`自动`
                        : t`LAN-only`
                    }
                  />
                  <Divider />
                  <Row
                    label={<Trans>候选节点</Trans>}
                    value={String(networkStatus?.bootstrapCandidateCount ?? 0)}
                  />
                  <Divider />
                  <Row
                    label={<Trans>LAN Helper</Trans>}
                    value={String(networkStatus?.lanHelperCount ?? 0)}
                  />
                  <Divider />
                  <Row
                    label={<Trans>NAT 状态</Trans>}
                    value={
                      networkStatus?.natStatus === "public"
                        ? t`映射成功`
                        : t`未知`
                    }
                  />
                  <Divider />
                  <Row
                    label={<Trans>中继</Trans>}
                    value={
                      networkStatus?.relayReady ? (
                        <RelayReadyLabel source={networkStatus.relaySource} />
                      ) : (
                        t`未就绪`
                      )
                    }
                  />
                  <Divider />
                  <Row
                    label={<Trans>引导节点</Trans>}
                    value={
                      networkStatus?.bootstrapConnected ? t`已连接` : t`未连接`
                    }
                  />
                  <Divider />
                  <Row
                    label={<Trans>本机 Helper</Trans>}
                    value={
                      networkStatus?.localLanHelperRunning
                        ? t`运行中`
                        : t`未启用`
                    }
                  />
                </Animated.View>
              ) : null}
            </View>
          </View>
        ) : (
          <Text className="text-center text-[13px] text-muted-foreground px-4">
            <Trans>启动节点后才能发现和连接其他设备</Trans>
          </Text>
        )}

        <View className="flex-row gap-2.5">
          <Pressable
            onPress={onDismiss}
            accessibilityRole="button"
            testID="node-control-cancel-button"
            className="flex-1 h-11 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
          >
            <Text className="text-[14px] font-medium text-foreground">
              <Trans>取消</Trans>
            </Text>
          </Pressable>
          {isRunning ? (
            <Pressable
              onPress={() => setConfirmStopOpen(true)}
              disabled={working || isStarting}
              accessibilityRole="button"
              accessibilityLabel={t`停止节点`}
              testID="node-control-stop-button"
              className="flex-1 h-11 items-center justify-center rounded-xl bg-destructive active:opacity-70 disabled:opacity-50"
            >
              {working ? (
                <ActivityIndicator color={colors.destructiveForeground} />
              ) : (
                <Text className="text-[14px] font-semibold text-destructive-foreground">
                  <Trans>停止节点</Trans>
                </Text>
              )}
            </Pressable>
          ) : (
            <Pressable
              onPress={handleStart}
              disabled={working || isStarting}
              accessibilityRole="button"
              accessibilityLabel={t`启动节点`}
              testID="node-control-start-button"
              className="flex-1 h-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
            >
              {working || isStarting ? (
                <ActivityIndicator color={colors.primaryForeground} />
              ) : (
                <Text className="text-[14px] font-semibold text-primary-foreground">
                  <Trans>启动节点</Trans>
                </Text>
              )}
            </Pressable>
          )}
        </View>
      </View>

      <ConfirmDialog
        open={confirmStopOpen}
        onOpenChange={setConfirmStopOpen}
        title={<Trans>停止网络节点？</Trans>}
        description={
          <Trans>停止后将断开所有连接，其他设备将无法发现你。</Trans>
        }
        actionLabel={<Trans>停止节点</Trans>}
        destructive
        onAction={() => {
          setConfirmStopOpen(false);
          void performStop();
        }}
        contentTestID="node-control-stop-confirmation"
        actionTestID="node-control-stop-confirm-button"
      />
    </>
  );
}

function Row({
  label,
  value,
  mono,
  onCopy,
  copyAccessibilityLabel,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
  onCopy?: () => void;
  copyAccessibilityLabel?: string;
}) {
  const colors = useThemeColors();
  const body = (
    <View className="min-h-11 flex-row items-center justify-between px-3.5 py-2.5">
      <Text className="text-[13px] text-muted-foreground">{label}</Text>
      <View className="min-w-0 flex-row items-center gap-1.5">
        <Text
          className={
            mono
              ? "font-mono text-[13px] text-foreground"
              : "text-[13px] font-medium text-foreground"
          }
          numberOfLines={1}
        >
          {value}
        </Text>
        {onCopy ? <Copy size={13} color={colors.mutedForeground} /> : null}
      </View>
    </View>
  );
  if (!onCopy) return body;
  return (
    <Pressable
      onPress={onCopy}
      accessibilityRole="button"
      accessibilityLabel={copyAccessibilityLabel}
      className="active:opacity-60"
    >
      {body}
    </Pressable>
  );
}

function RelayReadyLabel({
  source,
}: {
  source:
    | NonNullable<
        NonNullable<
          ReturnType<typeof useMobileCoreStore.getState>["networkStatus"]
        >["relaySource"]
      >
    | undefined;
}) {
  if (!source) return <Trans>就绪</Trans>;
  switch (candidateSourceKey(source)) {
    case "userCustom":
      return <Trans>就绪 · 自定义</Trans>;
    case "mdnsLanHelper":
      return <Trans>就绪 · LAN Helper</Trans>;
    default:
      return <Trans>就绪 · 公网</Trans>;
  }
}

/**
 * 运行时长每秒自增 —— 独立叶子组件,只让这一行每秒重渲染,
 * 不再带动整个 sheet(含诊断表)每秒重算。
 */
function UptimeValue({ startedAt }: { startedAt: number | null }) {
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!startedAt) return;
    const id = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [startedAt]);
  return <>{startedAt ? formatUptime(startedAt) : "—"}</>;
}

function Divider() {
  return <View className="h-px bg-border" />;
}
