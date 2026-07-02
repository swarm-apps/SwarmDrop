import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Globe2,
  RadioTower,
  RotateCw,
  ServerCog,
  Wifi,
} from "lucide-react-native";
import { Fragment, useCallback, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useShallow } from "zustand/react/shallow";
import { SettingDivider, SettingSection } from "@/components/setting-row";
import { SettingsHeader } from "@/components/settings-header";
import { StatusPill } from "@/components/status-pill";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import {
  candidateSourceKey,
  type DiscoveryModePreference,
  discoveryModeFromNative,
} from "@/core/network-discovery";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { cn, errorMessage } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function NetworkScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const [restarting, setRestarting] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

  const { networkStatus, runtimeState, shutdownNode, startNode } =
    useMobileCoreStore(
      useShallow((s) => ({
        networkStatus: s.networkStatus,
        runtimeState: s.runtimeState,
        shutdownNode: s.shutdownNode,
        startNode: s.startNode,
      })),
    );
  const {
    autoStart,
    discoveryMode,
    autoDiscoverLanHelpers,
    provideLanHelper,
    customBootstrapNodes,
    setAutoStart,
    setDiscoveryMode,
    setAutoDiscoverLanHelpers,
    setProvideLanHelper,
  } = usePreferencesStore(
    useShallow((s) => ({
      autoStart: s.autoStart,
      discoveryMode: s.discoveryMode,
      autoDiscoverLanHelpers: s.autoDiscoverLanHelpers,
      provideLanHelper: s.provideLanHelper,
      customBootstrapNodes: s.customBootstrapNodes,
      setAutoStart: s.setAutoStart,
      setDiscoveryMode: s.setDiscoveryMode,
      setAutoDiscoverLanHelpers: s.setAutoDiscoverLanHelpers,
      setProvideLanHelper: s.setProvideLanHelper,
    })),
  );

  const runtimeConfigChanged = useMemo(() => {
    if (runtimeState !== "running" || !networkStatus) return false;
    return (
      discoveryModeFromNative(networkStatus.discoveryMode) !== discoveryMode ||
      networkStatus.autoDiscoverLanHelpers !== autoDiscoverLanHelpers ||
      networkStatus.localLanHelperEnabled !== provideLanHelper
    );
  }, [
    autoDiscoverLanHelpers,
    discoveryMode,
    networkStatus,
    provideLanHelper,
    runtimeState,
  ]);

  // 「网络状况:良好/受限」合成状态 —— 不引入新的原生字段,只从已有 networkStatus/
  // discoveryMode 派生:
  // - 节点未运行 → 受限(没有可用连接)。
  // - 已经有至少一个已连接节点 → 良好(已实际证明连通性,是最直接的信号)。
  // - 没有已连接节点时,看发现路径是否健康:auto 模式要求公网引导 + 中继都就绪;
  //   lanOnly 模式要求至少发现一个 LAN Helper。都不满足则视为受限。
  const networkQuality = useMemo<"good" | "limited">(() => {
    if (runtimeState !== "running" || !networkStatus) return "limited";
    if (networkStatus.connectedPeers > 0) return "good";
    if (discoveryMode === "lanOnly") {
      return networkStatus.lanHelperCount > 0 ? "good" : "limited";
    }
    return networkStatus.bootstrapConnected && networkStatus.relayReady
      ? "good"
      : "limited";
  }, [runtimeState, networkStatus, discoveryMode]);

  const restartNode = useCallback(async () => {
    setRestarting(true);
    try {
      await shutdownNode();
      await startNode();
      // startNode/shutdownNode 内部吞错并把失败写进 store（runtimeState=error）而不抛出，
      // 这里读回最新状态判断真实结果，避免重启失败仍弹「成功」。
      const { runtimeState: state, error } = useMobileCoreStore.getState();
      if (state === "running") {
        toast.success(t`节点已按新发现设置重启`);
      } else {
        toast.error(t`重启节点失败`, error ?? undefined);
        // 错误已就地反馈，清掉全局 error 避免又延迟泄漏到设备页（重复提示）。
        useMobileCoreStore.getState().setError(null);
      }
    } catch (err) {
      toast.error(t`重启节点失败`, errorMessage(err));
    } finally {
      setRestarting(false);
    }
  }, [shutdownNode, startNode, t]);

  const rows: Array<{
    key: string;
    label: React.ReactNode;
    value: React.ReactNode;
  }> = [
    {
      key: "nat",
      label: <Trans>NAT 状态</Trans>,
      value: networkStatus?.natStatus === "public" ? t`映射成功` : t`未知`,
    },
    {
      key: "connected",
      label: <Trans>已连接节点</Trans>,
      value: String(networkStatus?.connectedPeers ?? 0),
    },
    {
      key: "discovered",
      label: <Trans>已发现节点</Trans>,
      value: String(networkStatus?.discoveredPeers ?? 0),
    },
    {
      key: "candidates",
      label: <Trans>候选节点</Trans>,
      value: String(networkStatus?.bootstrapCandidateCount ?? 0),
    },
    {
      key: "lan-helper",
      label: <Trans>LAN Helper</Trans>,
      value: String(networkStatus?.lanHelperCount ?? 0),
    },
    {
      key: "relay",
      label: <Trans>中继</Trans>,
      value: (
        <RelayStatusLabel
          ready={networkStatus?.relayReady ?? false}
          source={networkStatus?.relaySource}
        />
      ),
    },
    {
      key: "bootstrap",
      label: <Trans>公网引导</Trans>,
      value: networkStatus?.bootstrapConnected ? t`已连接` : t`未连接`,
    },
  ];

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top"]}
      testID="network-settings-screen"
    >
      <SettingsHeader title={t`网络`} />
      <ScrollView
        contentContainerClassName="gap-5 px-5 pt-2 pb-8"
        showsVerticalScrollIndicator={false}
      >
        <SettingSection label={t`发现方式`}>
          <View className="gap-3 px-3.5 py-3">
            <View className="flex-row gap-2">
              <DiscoveryModeOption
                mode="auto"
                selected={discoveryMode === "auto"}
                onPress={() => setDiscoveryMode("auto")}
              />
              <DiscoveryModeOption
                mode="lanOnly"
                selected={discoveryMode === "lanOnly"}
                onPress={() => setDiscoveryMode("lanOnly")}
              />
            </View>
            <Text className="text-[11px] text-muted-foreground">
              {discoveryMode === "auto" ? (
                <Trans>自动模式会使用公网引导、中继和局域网协助节点。</Trans>
              ) : (
                <Trans>
                  LAN-only 只依赖局域网协助和自定义节点，跨网络可达性会受限。
                </Trans>
              )}
            </Text>
          </View>
          <SettingDivider />
          <View className="flex-row items-center gap-3 px-3.5 py-3">
            <View className="flex-1 gap-0.5">
              <Text className="text-[14px] text-foreground">
                <Trans>自动发现 LAN Helper</Trans>
              </Text>
              <Text className="text-[11px] text-muted-foreground">
                <Trans>在本地网络发现可协助连接的节点。</Trans>
              </Text>
            </View>
            <Switch
              checked={autoDiscoverLanHelpers}
              onCheckedChange={setAutoDiscoverLanHelpers}
              accessibilityLabel={t`自动发现 LAN Helper`}
              testID="network-auto-lan-helper-switch"
            />
          </View>
        </SettingSection>

        {runtimeConfigChanged ? (
          <View
            className="gap-3 rounded-xl border border-warning/30 bg-warning/10 p-3.5"
            testID="network-restart-required"
          >
            <Text className="text-[12px] text-warning-ink">
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
            <StatusPill state={runtimeState} />
          </View>
          <SettingDivider />
          <View className="flex-row items-center justify-between px-3.5 py-3">
            <Text className="text-[14px] text-foreground">
              <Trans>网络状况</Trans>
            </Text>
            <View
              className={cn(
                "flex-row items-center gap-1.5 rounded-full px-2.5 py-1",
                networkQuality === "good" ? "bg-success/10" : "bg-warning/10",
              )}
            >
              <View
                className={cn(
                  "size-2 rounded-full",
                  networkQuality === "good" ? "bg-success" : "bg-warning",
                )}
              />
              <Text
                className={cn(
                  "text-xs font-medium",
                  networkQuality === "good"
                    ? "text-success-ink"
                    : "text-warning-ink",
                )}
              >
                {networkQuality === "good" ? (
                  <Trans>良好</Trans>
                ) : (
                  <Trans>受限</Trans>
                )}
              </Text>
            </View>
          </View>
          <SettingDivider />
          <Pressable
            onPress={() => setDiagnosticsOpen((value) => !value)}
            accessibilityRole="button"
            testID="network-diagnostics-toggle"
            className="flex-row items-center justify-between px-3.5 py-3 active:bg-muted"
          >
            <Text
              className={cn(
                "text-[13px] font-medium",
                diagnosticsOpen ? "text-muted-foreground" : "text-primary",
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
              {rows.map((row, idx) => (
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
                  {idx < rows.length - 1 ? <SettingDivider /> : null}
                </Fragment>
              ))}
              <CandidateSourceList status={networkStatus} />
            </>
          ) : null}
        </SettingSection>

        <NetworkHint
          bootstrapReady={networkStatus?.bootstrapConnected ?? false}
          relayReady={networkStatus?.relayReady ?? false}
          discoveryMode={discoveryMode}
        />

        <SettingSection label={t`通用`}>
          <View className="flex-row items-center justify-between gap-3 px-3.5 py-3">
            <View className="flex-1 gap-0.5">
              <Text className="text-[14px] text-foreground">
                <Trans>自动启动节点</Trans>
              </Text>
              <Text className="text-[11px] text-muted-foreground">
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
              <Text className="text-[11px] text-muted-foreground">
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
                <Trans>本机 LAN Helper</Trans>
              </Text>
              <Text className="text-[11px] text-muted-foreground">
                <Trans>
                  让本机作为局域网协助节点。默认关闭，以避免后台和电量风险。
                </Trans>
              </Text>
            </View>
            <Switch
              checked={provideLanHelper}
              onCheckedChange={setProvideLanHelper}
              accessibilityLabel={t`本机 LAN Helper`}
              testID="network-provide-lan-helper-switch"
            />
          </View>
        </SettingSection>
      </ScrollView>
    </SafeAreaView>
  );
}

function DiscoveryModeOption({
  mode,
  selected,
  onPress,
}: {
  mode: DiscoveryModePreference;
  selected: boolean;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  const Icon = mode === "auto" ? Globe2 : Wifi;
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      testID={
        mode === "auto"
          ? "network-discovery-auto"
          : "network-discovery-lan-only"
      }
      className={cn(
        "min-h-20 flex-1 gap-2 rounded-xl border px-3 py-3 active:opacity-70",
        selected ? "border-primary bg-primary/10" : "border-border bg-card",
      )}
    >
      <View className="flex-row items-center justify-between">
        <Icon
          color={selected ? colors.primary : colors.mutedForeground}
          size={18}
        />
        <View
          className={cn(
            "size-2 rounded-full",
            selected ? "bg-primary" : "bg-muted-foreground/40",
          )}
        />
      </View>
      <View className="gap-0.5">
        <Text className="text-[13px] font-semibold text-foreground">
          {mode === "auto" ? <Trans>自动</Trans> : <Trans>LAN-only</Trans>}
        </Text>
        <Text className="text-[10px] text-muted-foreground" numberOfLines={2}>
          {mode === "auto" ? (
            <Trans>公网 + 局域网</Trans>
          ) : (
            <Trans>仅本地网络</Trans>
          )}
        </Text>
      </View>
    </Pressable>
  );
}

function CandidateSourceList({
  status,
}: {
  status: ReturnType<typeof useMobileCoreStore.getState>["networkStatus"];
}) {
  if (!status || status.candidateSources.length === 0) return null;
  return (
    <>
      <SettingDivider />
      <View className="gap-2 px-3.5 py-3">
        <Text className="text-[13px] font-semibold text-foreground">
          <Trans>候选来源</Trans>
        </Text>
        <View className="flex-row flex-wrap gap-2">
          {status.candidateSources.map((item) => (
            <View
              key={candidateSourceKey(item.source)}
              className="rounded-full bg-muted px-2.5 py-1"
            >
              <Text className="text-[11px] text-muted-foreground">
                <CandidateSourceLabel source={item.source} /> ·{" "}
                {String(item.count)}
              </Text>
            </View>
          ))}
        </View>
      </View>
    </>
  );
}

function NetworkHint({
  bootstrapReady,
  relayReady,
  discoveryMode,
}: {
  bootstrapReady: boolean;
  relayReady: boolean;
  discoveryMode: DiscoveryModePreference;
}) {
  const colors = useThemeColors();
  if (bootstrapReady && relayReady) return null;
  return (
    <View className="flex-row gap-3 rounded-xl border border-border bg-card p-3.5">
      <RadioTower size={17} color={colors.warning} />
      <Text className="flex-1 text-[12px] text-muted-foreground">
        {!bootstrapReady && discoveryMode === "auto" ? (
          <Trans>公网引导尚未连接，跨网络发现可能暂时不可用。</Trans>
        ) : !relayReady ? (
          <Trans>中继尚未就绪，非同一网络的设备可能无法直连。</Trans>
        ) : (
          <Trans>当前发现能力受限。</Trans>
        )}
      </Text>
    </View>
  );
}

function RelayStatusLabel({
  ready,
  source,
}: {
  ready: boolean;
  source?: Parameters<typeof CandidateSourceLabel>[0]["source"];
}) {
  if (!ready) return <Trans>未就绪</Trans>;
  return source ? (
    <>
      <Trans>就绪</Trans> · <CandidateSourceLabel source={source} />
    </>
  ) : (
    <Trans>就绪</Trans>
  );
}

function CandidateSourceLabel({
  source,
}: {
  source: NonNullable<
    ReturnType<typeof useMobileCoreStore.getState>["networkStatus"]
  >["candidateSources"][number]["source"];
}) {
  switch (candidateSourceKey(source)) {
    case "userCustom":
      return <Trans>自定义</Trans>;
    case "mdnsLanHelper":
      return <Trans>LAN Helper</Trans>;
    default:
      return <Trans>公网</Trans>;
  }
}
