import { Trans, useLingui } from "@lingui/react/macro";
import type { NodeHealthCta } from "@swarmdrop/shared-view";
import { useRouter } from "expo-router";
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  RotateCw,
  ServerCog,
} from "lucide-react-native";
import { Fragment, useCallback, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { useShallow } from "zustand/react/shallow";
import { InfraLinkRowView } from "@/components/infra-link-row";
import { LanHelperAddresses } from "@/components/lan-helper-addresses";
import { AppScreen } from "@/components/mobile/screen";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { StatusPill } from "@/components/status-pill";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import { isNatMapped } from "@/core/network-discovery";
import {
  NODE_HEALTH_CTA_LABEL,
  type NodePresentation,
  resolveNodePresentation,
  TONE_DOT_CLASS,
  TONE_TEXT_CLASS,
} from "@/core/network-labels";
import { useNodeHealth } from "@/hooks/use-node-health";
import { useThemeColors } from "@/hooks/useThemeColors";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function NetworkScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const [restarting, setRestarting] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

  const { networkStatus, runtimeState, shutdownNode, startNode, setError } =
    useMobileCoreStore(
      useShallow((s) => ({
        networkStatus: s.networkStatus,
        runtimeState: s.runtimeState,
        shutdownNode: s.shutdownNode,
        startNode: s.startNode,
        setError: s.setError,
      })),
    );
  const {
    autoStart,
    autoDiscoverLanHelpers,
    provideLanHelper,
    publicReachability,
    customBootstrapNodes,
    setAutoStart,
    setAutoDiscoverLanHelpers,
    setProvideLanHelper,
    setPublicReachability,
  } = usePreferencesStore(
    useShallow((s) => ({
      autoStart: s.autoStart,
      autoDiscoverLanHelpers: s.autoDiscoverLanHelpers,
      provideLanHelper: s.provideLanHelper,
      publicReachability: s.publicReachability,
      customBootstrapNodes: s.customBootstrapNodes,
      setAutoStart: s.setAutoStart,
      setAutoDiscoverLanHelpers: s.setAutoDiscoverLanHelpers,
      setProvideLanHelper: s.setProvideLanHelper,
      setPublicReachability: s.setPublicReachability,
    })),
  );

  const nodeHealth = useNodeHealth();
  // 色档、状态词、后果句、CTA 出自同一次判定 —— `starting` / `error` 这两态健康判据
  // 表达不了（它把非 running 一律折成「节点未运行」），由生命周期覆盖层补。
  const presentation = resolveNodePresentation(
    runtimeState,
    nodeHealth.summary,
  );

  // 引导节点的增删**不在这张表里**：它们经 `add_infra_node` / `remove_infra_node`
  // 即时生效，不需要重启（这正是本轮改掉的东西）。剩下这四项仍然只在
  // `start_node` 时读一次——`InfraSupervisor::new` 把 `public_reachability` 拷进了
  // 自己的字段，`auto_discover_lan_helpers` 同理住在
  // `NetworkRuntimeConfig` 里，运行期改不动。
  const runtimeConfigChanged = useMemo(() => {
    if (runtimeState !== "running" || !networkStatus) return false;
    return (
      networkStatus.autoDiscoverLanHelpers !== autoDiscoverLanHelpers ||
      networkStatus.localLanHelperEnabled !== provideLanHelper ||
      networkStatus.publicReachabilityEnabled !== publicReachability
    );
  }, [
    autoDiscoverLanHelpers,
    networkStatus,
    provideLanHelper,
    publicReachability,
    runtimeState,
  ]);

  const restartNode = useCallback(async () => {
    setRestarting(true);
    try {
      const shutdown = await shutdownNode();
      if (!shutdown.ok) {
        toast.error(t`重启节点失败`, shutdown.error);
        setError(null);
        return;
      }
      const start = await startNode();
      if (start.ok && start.state === "running") {
        toast.success(t`节点已按新的网络设置重启`);
      } else {
        toast.error(t`重启节点失败`, start.ok ? undefined : start.error);
        // 错误已就地反馈，清掉全局 error 避免又延迟泄漏到设备页（重复提示）。
        setError(null);
      }
    } catch (err) {
      toast.error(t`重启节点失败`, getErrorMessage(err));
    } finally {
      setRestarting(false);
    }
  }, [shutdownNode, startNode, setError, t]);

  const handleStartNode = useCallback(async () => {
    const result = await startNode();
    if (!result.ok) toast.error(t`启动节点失败`, result.error);
  }, [startNode, t]);

  // 结论层至多一个 CTA。三档动作各自的落点由**本屏**决定：
  // `openSettings` 在这里恒为 null —— 用户已经站在那个开关旁边，再给一个「去设置」
  // 只是把他送回原地。
  const ctaHandlers: Record<NonNullable<NodeHealthCta>, (() => void) | null> = {
    startNode: () => void handleStartNode(),
    openSettings: null,
    openDiagnostics: () => setDiagnosticsOpen(true),
  };

  const localTruth: Array<{
    key: string;
    label: React.ReactNode;
    value: React.ReactNode;
  }> = [
    {
      key: "nat",
      label: <Trans>NAT 状态</Trans>,
      value: isNatMapped(networkStatus?.natStatus) ? t`映射成功` : t`未知`,
    },
    {
      key: "connected",
      label: <Trans>已连接设备</Trans>,
      value: String(networkStatus?.connectedPeers ?? 0),
    },
    {
      key: "discovered",
      label: <Trans>已发现设备</Trans>,
      value: String(networkStatus?.discoveredPeers ?? 0),
    },
    {
      key: "public-reachable",
      label: <Trans>公网可达</Trans>,
      value: networkStatus?.publicReachable ? t`可达` : t`仅局域网`,
    },
  ];

  return (
    <AppScreen
      scroll
      testID="network-settings-screen"
      header={<SettingsHeader title={t`网络`} />}
      contentClassName="gap-5 pt-2"
    >
      <SettingSection label={t`可达性`}>
        <View className="flex-row items-center gap-3 px-3.5 py-3">
          <View className="flex-1 gap-0.5">
            <Text className="text-[14px] text-foreground">
              <Trans>公网可达性</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              <Trans>
                允许通过公网中继被跨网设备访问；关闭后仅局域网可达，跨网设备找不到你。
              </Trans>
            </Text>
          </View>
          <Switch
            checked={publicReachability}
            onCheckedChange={setPublicReachability}
            accessibilityLabel={t`公网可达性`}
            testID="network-public-reachability-switch"
          />
        </View>
        <SettingDivider />
        <View className="flex-row items-center gap-3 px-3.5 py-3">
          <View className="flex-1 gap-0.5">
            <Text className="text-[14px] text-foreground">
              <Trans>自动发现局域网协助</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              <Trans>在本地网络发现可协助连接的节点。</Trans>
            </Text>
          </View>
          <Switch
            checked={autoDiscoverLanHelpers}
            onCheckedChange={setAutoDiscoverLanHelpers}
            accessibilityLabel={t`自动发现局域网协助`}
            testID="network-auto-lan-helper-switch"
          />
        </View>
      </SettingSection>

      {runtimeConfigChanged ? (
        <View
          className="gap-3 rounded-xl border border-warning/30 bg-warning/10 p-3.5"
          testID="network-restart-required"
        >
          <Text className="text-[13px] text-warning-ink">
            <Trans>发现设置已变更，重启节点后生效。</Trans>
          </Text>
          <Pressable
            onPress={restartNode}
            disabled={restarting}
            accessibilityRole="button"
            testID="network-restart-button"
            className="min-h-10 flex-row items-center justify-center gap-2 rounded-xl bg-card active:opacity-70 disabled:opacity-50"
          >
            {restarting ? (
              <ActivityIndicator color={colors.foreground} size="small" />
            ) : (
              <RotateCw color={colors.foreground} size={14} />
            )}
            <Text className="text-[13px] font-semibold text-foreground">
              {restarting ? <Trans>重启中</Trans> : <Trans>重启节点</Trans>}
            </Text>
          </Pressable>
        </View>
      ) : null}

      <SettingSection label={t`节点状态`}>
        <View className="flex-row items-center justify-between px-3.5 py-3">
          <Text className="text-[14px] text-foreground">
            <Trans>当前状态</Trans>
          </Text>
          <StatusPill state={runtimeState} health={nodeHealth.summary} />
        </View>
        <SettingDivider />
        <NetworkHint presentation={presentation} ctaHandlers={ctaHandlers} />
        <SettingDivider />
        <Pressable
          onPress={() => setDiagnosticsOpen((value) => !value)}
          accessibilityRole="button"
          accessibilityState={{ expanded: diagnosticsOpen }}
          testID="network-diagnostics-toggle"
          className="flex-row items-center justify-between px-3.5 py-3 active:bg-muted"
        >
          <Text
            className={cn(
              "text-[13px] font-medium",
              diagnosticsOpen ? "text-muted-foreground" : "text-primary-ink",
            )}
          >
            {diagnosticsOpen ? (
              <Trans>收起诊断详情</Trans>
            ) : (
              <Trans>查看诊断详情</Trans>
            )}
          </Text>
          {diagnosticsOpen ? (
            <ChevronUp color={colors.mutedForeground} size={16} />
          ) : (
            <ChevronDown color={colors.primary} size={16} />
          )}
        </Pressable>
        {diagnosticsOpen ? (
          <>
            <SettingDivider />
            {localTruth.map((row, idx) => (
              <Fragment key={row.key}>
                <View className="flex-row items-center justify-between gap-3 px-3.5 py-3">
                  <Text className="text-[14px] text-foreground">
                    {row.label}
                  </Text>
                  <Text
                    className="flex-1 text-right text-[13px] text-muted-foreground"
                    numberOfLines={1}
                  >
                    {row.value}
                  </Text>
                </View>
                {idx < localTruth.length - 1 ? <SettingDivider /> : null}
              </Fragment>
            ))}
            <SettingDivider />
            <View className="px-3.5 pt-3">
              <Text className="text-[13px] font-semibold text-foreground">
                <Trans>引导节点</Trans>
              </Text>
            </View>
            {nodeHealth.rows.length === 0 ? (
              <Text className="px-3.5 py-3 text-[13px] text-muted-foreground">
                <Trans>还没有引导节点进入候选表。</Trans>
              </Text>
            ) : (
              nodeHealth.rows.map((row) => (
                <InfraLinkRowView key={row.link.peerId} row={row} />
              ))
            )}
          </>
        ) : null}
      </SettingSection>

      <SettingSection label={t`通用`}>
        <View className="flex-row items-center justify-between gap-3 px-3.5 py-3">
          <View className="flex-1 gap-0.5">
            <Text className="text-[14px] text-foreground">
              <Trans>自动启动节点</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              <Trans>App 启动后自动启动 P2P 节点。</Trans>
            </Text>
          </View>
          <Switch
            checked={autoStart}
            onCheckedChange={setAutoStart}
            accessibilityLabel={t`自动启动节点`}
          />
        </View>
      </SettingSection>

      <SettingSection label={t`高级`}>
        <Pressable
          onPress={() => router.push("/settings/bootstrap-nodes" as never)}
          accessibilityRole="button"
          testID="network-bootstrap-advanced-entry"
          className="min-h-13 flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
        >
          <View className="size-8 items-center justify-center rounded-lg bg-muted">
            <ServerCog color={colors.mutedForeground} size={16} />
          </View>
          <View className="min-w-0 flex-1 gap-0.5">
            <Text className="text-[14px] text-foreground">
              <Trans>自定义引导节点</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              {customBootstrapNodes.length > 0 ? (
                <Trans>{customBootstrapNodes.length} 个自定义节点</Trans>
              ) : (
                <Trans>作为自动发现失败时的兜底路径。</Trans>
              )}
            </Text>
          </View>
          <ChevronRight color={colors.mutedForeground} size={16} />
        </Pressable>
        <SettingDivider />
        <View className="flex-row items-center justify-between gap-3 px-3.5 py-3">
          <View className="flex-1 gap-0.5">
            <Text className="text-[14px] text-foreground">
              <Trans>本机局域网协助</Trans>
            </Text>
            <Text className="text-[12px] text-muted-foreground">
              <Trans>
                让本机作为局域网协助节点。默认关闭，以避免后台和电量风险。
              </Trans>
            </Text>
          </View>
          <Switch
            checked={provideLanHelper}
            onCheckedChange={setProvideLanHelper}
            accessibilityLabel={t`本机局域网协助`}
            testID="network-provide-lan-helper-switch"
          />
        </View>
      </SettingSection>

      <LanHelperAddresses
        addresses={networkStatus?.lanHelperAdvertisedAddrs ?? []}
        peerId={networkStatus?.peerId}
      />
    </AppScreen>
  );
}

function NetworkHint({
  presentation,
  ctaHandlers,
}: {
  presentation: NodePresentation;
  ctaHandlers: Record<NonNullable<NodeHealthCta>, (() => void) | null>;
}) {
  const { t } = useLingui();
  const onCta = presentation.cta ? ctaHandlers[presentation.cta] : null;

  return (
    <View className="gap-2 px-3.5 py-3" testID="network-health-summary">
      <View className="flex-row items-start gap-2">
        <View
          className={cn(
            "mt-1.5 size-2 rounded-full",
            TONE_DOT_CLASS[presentation.tone],
          )}
        />
        <Text
          className={cn(
            "flex-1 text-[13px]",
            TONE_TEXT_CLASS[presentation.tone],
          )}
        >
          {t(presentation.sentence)}
        </Text>
      </View>
      {presentation.cta && onCta ? (
        <Pressable
          onPress={onCta}
          accessibilityRole="button"
          testID="network-health-cta"
          className="min-h-10 items-center justify-center self-start rounded-xl border border-border bg-card px-3.5 active:opacity-70"
        >
          <Text className="text-[13px] font-semibold text-foreground">
            {t(NODE_HEALTH_CTA_LABEL[presentation.cta])}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
