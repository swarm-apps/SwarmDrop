import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import type { NodeHealthCta } from "@swarmdrop/shared-view";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import { ChevronDown, Copy, Power } from "lucide-react-native";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { ActivityIndicator, Platform, Pressable, View } from "react-native";
import Animated, { FadeIn } from "react-native-reanimated";
import { useShallow } from "zustand/react/shallow";
import { InfraLinkRowView, truncateAddr } from "@/components/infra-link-row";
import { StatusPill } from "@/components/status-pill";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { isNatMapped } from "@/core/network-discovery";
import {
  NODE_HEALTH_CTA_LABEL,
  resolveNodePresentation,
  TONE_TEXT_CLASS,
} from "@/core/network-labels";
import { useNodeHealth } from "@/hooks/use-node-health";
import { useThemeColors } from "@/hooks/useThemeColors";
import { formatUptime } from "@/lib/format-uptime";
import { toast } from "@/lib/toast";
import { cn, truncateMiddle } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

export interface NodeControlSheetRef {
  present: () => void;
  dismiss: () => void;
}

/**
 * 身份存放位置（诊断层的本机真值之一）。
 *
 * `expo-secure-store` 两端落在不同的系统设施上（iOS 钥匙串 / Android 由 Keystore
 * 加密的偏好存储），说成同一个词会让排查「换机后身份没了」的人找错地方。
 * `default` 只是 RN 类型上的兜底分支——本 App 只发这两端。
 */
const IDENTITY_STORAGE: MessageDescriptor = Platform.select({
  ios: msg`系统钥匙串`,
  android: msg`Android 密钥库`,
  default: msg`系统安全存储`,
});

/**
 * 节点状态面（移动端形态：bottom sheet）。
 *
 * 按 `DESIGN.md` 的 Node Status Contract 分**两层**：
 * - **结论层**（常驻）：状态点 + 词 · 可达性后果句 · 已配对 N·在线 M · 至多一个 CTA
 * - **诊断层**（一个折叠，默认收起）：引导节点逐条（状态 · 归因 · `lastError` 原文 +
 *   复制）+ 本机真值（节点 ID / 公网地址 / 已发现设备 / NAT / 局域网协助 /
 *   身份存放位置 / 运行时长 / 监听地址）。**这张清单由契约钉死**，原生端没有豁免项
 *   （只有 Web 可以省掉 NAT 与已发现设备两格，因为那两个值在浏览器里恒为未知）。
 *
 * 运行时长**属于诊断层**——它回答不了「别人现在能不能连到我」。
 *
 * 高度走 AppBottomSheet 的内容自适应 + **内滚**(`scrollable`)：停止态只有一句提示时
 * sheet 仍收缩到贴合内容，而展开诊断后（本机真值 8 行 + 引导节点逐条，失败的那条还带
 * `lastError` 代码块）内容会超过屏高——不滚的 `BottomSheetView` 到那时会把末几条 link
 * 与底部两颗按钮压到屏外。契约写死「不得因为版面紧就丢信息位：折叠、在 sheet 内滚动，
 * 或下放到诊断层」，桌面同一问题也是按折叠 + 内滚改的。
 */
export const NodeControlSheet = forwardRef<NodeControlSheetRef, object>(
  function NodeControlSheet(_props, ref) {
    const sheetRef = useRef<AppBottomSheetRef>(null);

    useImperativeHandle(ref, () => ({
      present: () => sheetRef.current?.present(),
      dismiss: () => sheetRef.current?.dismiss(),
    }));

    return (
      <AppBottomSheet
        ref={sheetRef}
        scrollable
        contentTestID="node-control-sheet-content"
      >
        <NodeControlContent onDismiss={() => sheetRef.current?.dismiss()} />
      </AppBottomSheet>
    );
  },
);

function NodeControlContent({ onDismiss }: { onDismiss: () => void }) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const router = useRouter();
  const {
    runtimeState,
    networkStatus,
    peerId,
    startedAt,
    devices,
    pairedDevicesCache,
    startNode,
    shutdownNode,
  } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      networkStatus: s.networkStatus,
      peerId: s.peerId,
      startedAt: s.startedAt,
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      startNode: s.startNode,
      shutdownNode: s.shutdownNode,
    })),
  );

  const nodeHealth = useNodeHealth();

  const [working, setWorking] = useState(false);
  // 停止节点二次确认 —— 与阻止设备/清空活动一样走 in-app ConfirmDialog(不再用系统原生 Alert)。
  const [confirmStopOpen, setConfirmStopOpen] = useState(false);
  // 诊断层默认收起（契约：两层披露，不是四层）。
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

  // 复制的永远是**完整值**——屏幕上那串是中间省略过的，贴出去等于一条写错的地址。
  const copyValue = async (value: string, toastLabel: string) => {
    await Clipboard.setStringAsync(value);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(toastLabel);
  };

  const isRunning = runtimeState === "running";
  const isStarting = runtimeState === "starting";

  const publicAddr = networkStatus?.publicAddr ?? null;
  const listenAddrs = networkStatus?.listenAddrs ?? [];

  const pairedCount = pairedDevicesCache.length;
  const onlineCount = devices.filter(
    (device) => device.isPaired && device.status === "online",
  ).length;

  // 结论层至多一个 CTA。`startNode` 恒为 null —— 底部那颗主按钮已经是它，
  // 同一句话在一张卡片上说两遍只会让人怀疑哪个才算数。
  const ctaHandlers: Record<NonNullable<NodeHealthCta>, (() => void) | null> = {
    startNode: null,
    openSettings: () => {
      onDismiss();
      router.push("/settings/network" as never);
    },
    openDiagnostics: () => setShowDiagnostics(true),
  };
  // 结论层的四个信息位统一出自 `resolveNodePresentation`：色档、状态词、后果句、CTA
  // 必须是同一次判定的结果，否则 `starting` / `error` 下 pill 与后果句会各说各的。
  const presentation = resolveNodePresentation(
    runtimeState,
    nodeHealth.summary,
  );
  const onCta = presentation.cta ? ctaHandlers[presentation.cta] : null;

  return (
    <>
      <View className="gap-4 px-5 pt-2 pb-6">
        {/* ── 结论层 ───────────────────────────────────────────── */}
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
          <StatusPill
            state={runtimeState}
            health={nodeHealth.summary}
            size="md"
          />
          <Text
            className={cn(
              "px-4 text-center text-[13px]",
              TONE_TEXT_CLASS[presentation.tone],
            )}
            testID="node-control-health-sentence"
          >
            {t(presentation.sentence)}
          </Text>
          <Text className="text-[12px] text-muted-foreground">
            <Trans>
              已配对 {pairedCount} · 在线 {onlineCount}
            </Trans>
          </Text>
          {presentation.cta && onCta ? (
            <Pressable
              onPress={onCta}
              accessibilityRole="button"
              testID="node-control-health-cta"
              className="min-h-9 items-center justify-center rounded-xl border border-border bg-card px-3.5 active:opacity-70"
            >
              <Text className="text-[13px] font-semibold text-foreground">
                {t(NODE_HEALTH_CTA_LABEL[presentation.cta])}
              </Text>
            </Pressable>
          ) : null}
        </View>

        {/* ── 诊断层（一个折叠，默认收起）─────────────────────── */}
        {isRunning ? (
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
                  transform: [{ rotate: showDiagnostics ? "180deg" : "0deg" }],
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
                  onCopy={
                    peerId
                      ? () => void copyValue(peerId, t`已复制 Peer ID`)
                      : undefined
                  }
                  copyAccessibilityLabel={t`复制 Peer ID`}
                />
                <Divider />
                {/* 本机真值：公网地址是「别人拿什么连我」的原始答案，与上面那句
                    后果句互为表里——后果句说结论，这一行给可以贴出去的证据。 */}
                <Row
                  label={<Trans>公网地址</Trans>}
                  value={publicAddr ? truncateAddr(publicAddr) : "—"}
                  mono
                  onCopy={
                    publicAddr
                      ? () => void copyValue(publicAddr, t`已复制公网地址`)
                      : undefined
                  }
                  copyAccessibilityLabel={t`复制公网地址`}
                />
                <Divider />
                <Row
                  label={<Trans>已发现设备</Trans>}
                  value={String(networkStatus?.discoveredPeers ?? 0)}
                />
                <Divider />
                <Row
                  label={<Trans>NAT 状态</Trans>}
                  value={
                    isNatMapped(networkStatus?.natStatus)
                      ? t`映射成功`
                      : t`未知`
                  }
                />
                <Divider />
                <Row
                  label={<Trans>本机局域网协助</Trans>}
                  value={
                    networkStatus?.localLanHelperRunning ? t`运行中` : t`未启用`
                  }
                />
                <Divider />
                <Row
                  label={<Trans>身份存放位置</Trans>}
                  value={t(IDENTITY_STORAGE)}
                />
                <Divider />
                {/* 运行时长在诊断层：它回答不了「别人能不能连到我」 */}
                <Row
                  label={<Trans>运行时长</Trans>}
                  value={<UptimeValue startedAt={startedAt} />}
                />
                <Divider />
                <View className="flex-row items-center justify-between px-3.5 pt-2.5">
                  <Text className="text-[13px] font-medium text-foreground">
                    <Trans>监听地址</Trans>
                  </Text>
                  <Text className="text-[12px] tabular-nums text-muted-foreground">
                    {listenAddrs.length}
                  </Text>
                </View>
                {listenAddrs.length === 0 ? (
                  <Text className="px-3.5 py-2.5 text-[12px] text-muted-foreground">
                    <Trans>还没有监听地址。</Trans>
                  </Text>
                ) : (
                  listenAddrs.map((addr) => (
                    <Pressable
                      key={addr}
                      onPress={() => void copyValue(addr, t`已复制监听地址`)}
                      accessibilityRole="button"
                      accessibilityLabel={t`复制监听地址`}
                      className="min-h-11 flex-row items-center gap-1.5 px-3.5 py-2 active:opacity-60"
                    >
                      <Text
                        className="min-w-0 flex-1 font-mono text-[12px] text-muted-foreground"
                        numberOfLines={1}
                      >
                        {truncateAddr(addr)}
                      </Text>
                      <Copy size={13} color={colors.mutedForeground} />
                    </Pressable>
                  ))
                )}
                <Divider />
                <View className="px-3.5 pt-2.5">
                  <Text className="text-[13px] font-medium text-foreground">
                    <Trans>引导节点</Trans>
                  </Text>
                </View>
                {nodeHealth.rows.length === 0 ? (
                  <Text className="px-3.5 py-2.5 text-[12px] text-muted-foreground">
                    <Trans>还没有引导节点进入候选表。</Trans>
                  </Text>
                ) : (
                  nodeHealth.rows.map((row) => (
                    <InfraLinkRowView key={row.link.peerId} row={row} />
                  ))
                )}
              </Animated.View>
            ) : null}
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
    <View className="min-h-11 flex-row items-center justify-between gap-3 px-3.5 py-2.5">
      <Text className="shrink-0 text-[13px] text-muted-foreground">
        {label}
      </Text>
      {/* 值这一侧吃掉剩余宽度并右对齐：公网地址这类长机器串要能截断而不是把标签挤走。 */}
      <View className="min-w-0 flex-1 flex-row items-center justify-end gap-1.5">
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
