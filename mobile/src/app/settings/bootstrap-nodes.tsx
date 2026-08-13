import { Trans, useLingui } from "@lingui/react/macro";
import { Plus, Trash2 } from "lucide-react-native";
import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, TextInput, View } from "react-native";
import {
  MobileInfraAddrError,
  MobileInfraAddrError_Tags,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  InfraLinkDetail,
  InfraLinkRowView,
  InfraLinkStatusChip,
  truncateAddr,
} from "@/components/infra-link-row";
import { AppScreen } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { MOBILE_BOOTSTRAP_NODES } from "@/core/bootstrap-nodes";
import { peerIdFromMultiaddr } from "@/core/infra-view";
import { initMobileCore } from "@/core/mobile-core";
import { type InfraLinkRow, useNodeHealth } from "@/hooks/use-node-health";
import { useThemeColors } from "@/hooks/useThemeColors";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

interface ConfiguredEntry {
  addr: string;
  builtin: boolean;
}

/**
 * 一个**节点**（不是一条地址）。
 *
 * 聚合根是「本机与某个远端之间的一段基础设施关系」，同一个节点的 TCP 与 QUIC 地址
 * 是同一段关系的两条路径——内核 upsert 会合并它们，状态也只有一份。按地址列会让
 * 内置的那两条地址显示成两个各自「已就绪」的东西。
 */
interface ConfiguredGroup {
  key: string;
  peerId: string | null;
  entries: ConfiguredEntry[];
  hasBuiltin: boolean;
  row: InfraLinkRow | undefined;
}

export default function BootstrapNodesScreen() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const { customBootstrapNodes, addBootstrapNode, removeBootstrapNode } =
    usePreferencesStore(
      useShallow((s) => ({
        customBootstrapNodes: s.customBootstrapNodes,
        addBootstrapNode: s.addBootstrapNode,
        removeBootstrapNode: s.removeBootstrapNode,
      })),
    );
  const { runtimeState, refreshNetworkStatus } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      refreshNetworkStatus: s.refreshNetworkStatus,
    })),
  );
  const nodeHealth = useNodeHealth();
  const isRunning = runtimeState === "running";

  const [inputValue, setInputValue] = useState("");
  const [showInput, setShowInput] = useState(false);
  const [inputError, setInputError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [pendingRemove, setPendingRemove] = useState<ConfiguredGroup | null>(
    null,
  );
  const [transports, setTransports] = useState<string[]>([]);

  // 只在输入框打开时问一次：节点在跑期间这张清单是常量（传输在 bind 时装配完毕）。
  useEffect(() => {
    if (!showInput || !isRunning) return;
    let cancelled = false;
    void initMobileCore()
      .then((core) => core.supportedTransports())
      .then((list) => {
        if (!cancelled) setTransports(list);
      })
      // 拿不到就不显示这行提示——校验本身在 Rust 侧，不受影响。
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [isRunning, showInput]);

  const { groups, discovered } = useMemo(() => {
    const rowsByPeer = new Map(
      nodeHealth.rows.map((row) => [row.link.peerId, row]),
    );
    const entries: ConfiguredEntry[] = [
      ...MOBILE_BOOTSTRAP_NODES.map((addr) => ({ addr, builtin: true })),
      ...customBootstrapNodes
        .filter((addr) => !MOBILE_BOOTSTRAP_NODES.includes(addr))
        .map((addr) => ({ addr, builtin: false })),
    ];

    const byKey = new Map<string, ConfiguredGroup>();
    for (const entry of entries) {
      // 解析不出身份的地址（历史遗留的手填串）自成一组，用地址本身当键，
      // 这样它至少还能被看见和删掉。
      const peerId = peerIdFromMultiaddr(entry.addr);
      const key = peerId ?? entry.addr;
      const group = byKey.get(key) ?? {
        key,
        peerId,
        entries: [],
        hasBuiltin: false,
        row: peerId ? rowsByPeer.get(peerId) : undefined,
      };
      group.entries.push(entry);
      group.hasBuiltin = group.hasBuiltin || entry.builtin;
      byKey.set(key, group);
    }

    return {
      groups: [...byKey.values()],
      // 运行时经 identify 学到的、或局域网发现的节点——不在配置清单里，只读。
      discovered: nodeHealth.rows.filter((row) => !byKey.has(row.link.peerId)),
    };
  }, [customBootstrapNodes, nodeHealth.rows]);

  /**
   * 把 typed 的校验错误翻成一句**能据以行动**的话。
   *
   * 七个变体逐一列出、**不留 default**：内核加一个失败原因时这里编译期红，
   * 而不是静默退化成一句「地址无效」。
   */
  const describeError = useCallback(
    (error: unknown): string => {
      if (!MobileInfraAddrError.instanceOf(error)) {
        return getErrorMessage(error);
      }
      switch (error.tag) {
        case MobileInfraAddrError_Tags.NodeNotStarted:
          return t`节点未运行，无法校验地址`;
        case MobileInfraAddrError_Tags.Malformed:
          return t`不是合法的 multiaddr：${error.inner.detail}`;
        case MobileInfraAddrError_Tags.MissingPeerId:
          return t`地址缺少 /p2p/<节点 ID> 段`;
        case MobileInfraAddrError_Tags.NoTransport:
          return t`地址里没有可拨号的传输段`;
        case MobileInfraAddrError_Tags.UnsupportedTransport:
          return t`本机没有装配 ${error.inner.transport} 传输，可用的是 ${error.inner.supported.join(" / ")}`;
        case MobileInfraAddrError_Tags.SelfAddr:
          return t`这是本机的地址`;
        case MobileInfraAddrError_Tags.Duplicate:
          return t`这条地址已经在清单里了`;
      }
    },
    [t],
  );

  /**
   * 添加：**先改内核成功、再写持久化**。
   *
   * 反过来会留下一条内核从没接受过的地址——重启前它一直躺在清单里显示不出状态，
   * 重启后才在启动路径上静默失败。
   */
  const handleAdd = useCallback(async () => {
    const addr = inputValue.trim();
    if (!addr) return;
    setSubmitting(true);
    setInputError(null);
    try {
      const core = await initMobileCore();
      await core.addInfraNode(addr);
      addBootstrapNode(addr);
      setInputValue("");
      setShowInput(false);
      toast.success(t`引导节点已添加`);
      await refreshNetworkStatus();
    } catch (error) {
      setInputError(describeError(error));
    } finally {
      setSubmitting(false);
    }
  }, [addBootstrapNode, describeError, inputValue, refreshNetworkStatus, t]);

  const confirmRemove = useCallback(async () => {
    const group = pendingRemove;
    setPendingRemove(null);
    if (!group) return;
    try {
      if (isRunning && group.peerId) {
        const core = await initMobileCore();
        await core.removeInfraNode(group.peerId);
      }
      for (const entry of group.entries) removeBootstrapNode(entry.addr);
      toast.success(t`引导节点已删除`);
      await refreshNetworkStatus();
    } catch (error) {
      toast.error(t`删除引导节点失败`, getErrorMessage(error));
    }
  }, [isRunning, pendingRemove, refreshNetworkStatus, removeBootstrapNode, t]);

  return (
    // ConfirmDialog 放在 AppScreen **外面**:它的 Root 会渲染一个真实的零高 View,
    // 留在 `gap-3` 的滚动内容里会让页面末尾凭空多出 12px 死带(Yoga 的 gap 不看子节点高度)。
    <>
      <AppScreen
        scroll
        testID="bootstrap-nodes-screen"
        header={<SettingsHeader title={t`引导节点`} />}
        contentClassName="gap-3 pt-2"
      >
        <Text className="text-[13px] text-muted-foreground px-1">
          <Trans>
            引导节点用于发现 P2P 网络。增删立即生效，不需要重启节点。
          </Trans>
        </Text>

        <View className="rounded-lg border border-border bg-card overflow-hidden">
          {groups.map((group, idx) => (
            <Fragment key={group.key}>
              <ConfiguredGroupView
                group={group}
                canRemove={
                  // 内置条目不给移除（清单是硬编码的，删了下次启动照样回来）；
                  // 其余按内核的 `removable` 门控——同一个 NodeId 也可能是别人的
                  // 局域网协助，那种情况下移除会掐断正在跑的传输。
                  !group.hasBuiltin && (group.row?.link.removable ?? true)
                }
                onRemove={() => setPendingRemove(group)}
              />
              {idx < groups.length - 1 || showInput ? (
                <View className="h-px bg-border" />
              ) : null}
            </Fragment>
          ))}

          {showInput ? (
            <View className="gap-2 p-3.5">
              <TextInput
                value={inputValue}
                onChangeText={(value) => {
                  setInputValue(value);
                  setInputError(null);
                }}
                accessibilityLabel={t`引导节点地址`}
                placeholder={t`/ip4/.../tcp/.../p2p/...`}
                placeholderTextColor={colors.mutedForeground}
                autoFocus
                autoCapitalize="none"
                autoCorrect={false}
                className="rounded-lg border border-border bg-background px-3 py-2.5 font-mono text-[13px] text-foreground"
              />
              {inputError ? (
                <Text
                  className="text-[12px] text-destructive-ink"
                  testID="bootstrap-add-error"
                >
                  {inputError}
                </Text>
              ) : transports.length > 0 ? (
                // 传输清单是**内核事实**（`Endpoint::supported_transports`），不是
                // 这里硬编码的一张表——把它先说出来，用户就不必先粘一条再被拒。
                <Text className="text-[12px] text-muted-foreground">
                  <Trans>本机支持的传输：{transports.join(" / ")}</Trans>
                </Text>
              ) : null}
              <View className="flex-row gap-2">
                <Pressable
                  onPress={() => {
                    setShowInput(false);
                    setInputValue("");
                    setInputError(null);
                  }}
                  accessibilityRole="button"
                  className="flex-1 h-10 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
                >
                  <Text className="text-[13px] text-foreground">
                    <Trans>取消</Trans>
                  </Text>
                </Pressable>
                <Pressable
                  onPress={() => void handleAdd()}
                  disabled={submitting}
                  accessibilityRole="button"
                  className="flex-1 h-10 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
                >
                  {submitting ? (
                    <ActivityIndicator color={colors.primaryForeground} />
                  ) : (
                    <Text className="text-[13px] font-semibold text-primary-foreground">
                      <Trans>添加</Trans>
                    </Text>
                  )}
                </Pressable>
              </View>
            </View>
          ) : (
            <Pressable
              onPress={() => setShowInput(true)}
              disabled={!isRunning}
              accessibilityRole="button"
              testID="bootstrap-add-custom-node-button"
              className="flex-row items-center gap-2 p-3.5 active:bg-muted disabled:opacity-50"
            >
              <Plus color={colors.mutedForeground} size={14} />
              <Text className="text-[13px] text-muted-foreground">
                <Trans>添加自定义引导节点</Trans>
              </Text>
            </Pressable>
          )}
        </View>

        {!isRunning ? (
          <Text className="px-1 text-[12px] text-muted-foreground">
            {/* 校验要问内核「本机装配了哪些传输」，节点没起来就问不出来。
                在 JS 侧再写一份形状校验只会与内核的判据漂开。 */}
            <Trans>
              启动节点后才能添加引导节点——校验需要内核确认本机支持的传输。
            </Trans>
          </Text>
        ) : null}

        {discovered.length > 0 ? (
          <>
            <Text className="px-1 pt-1 text-[13px] font-semibold text-foreground">
              <Trans>自动发现的节点</Trans>
            </Text>
            <View className="rounded-lg border border-border bg-card overflow-hidden">
              {discovered.map((row, idx) => (
                <Fragment key={row.link.peerId}>
                  <InfraLinkRowView row={row} />
                  {idx < discovered.length - 1 ? (
                    <View className="h-px bg-border" />
                  ) : null}
                </Fragment>
              ))}
            </View>
          </>
        ) : null}
      </AppScreen>
      <ConfirmDialog
        open={pendingRemove !== null}
        onOpenChange={(open) => {
          if (!open) setPendingRemove(null);
        }}
        title={<Trans>删除引导节点</Trans>}
        description={
          <Trans>
            该节点将不再作为自动发现的兜底路径，与它的连接会立即断开。之后可随时重新添加。
          </Trans>
        }
        actionLabel={<Trans>删除</Trans>}
        onAction={() => void confirmRemove()}
      />
    </>
  );
}

function ConfiguredGroupView({
  group,
  canRemove,
  onRemove,
}: {
  group: ConfiguredGroup;
  canRemove: boolean;
  onRemove: () => void;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();

  return (
    <View className="gap-1.5 p-3.5">
      <View className="flex-row items-center gap-2">
        {group.row ? (
          <InfraLinkStatusChip presentation={group.row.presentation} />
        ) : (
          // 节点没跑（或这条还没进候选表）时不编一个状态出来——空缺本身就是答案。
          <Text className="text-[13px] text-muted-foreground">
            <Trans>暂无状态</Trans>
          </Text>
        )}
        <View className="flex-1" />
        {group.hasBuiltin ? (
          <View className="rounded bg-muted px-1.5 py-0.5">
            <Text className="text-[11px] text-muted-foreground">
              <Trans>默认</Trans>
            </Text>
          </View>
        ) : null}
        {canRemove ? (
          <Pressable
            onPress={onRemove}
            hitSlop={8}
            accessibilityRole="button"
            accessibilityLabel={t`删除`}
            className="size-7 items-center justify-center"
          >
            <Trash2 color={colors.destructive} size={14} />
          </Pressable>
        ) : null}
      </View>
      {group.entries.map((entry) => (
        <Text
          key={entry.addr}
          className="font-mono text-[12px] text-muted-foreground"
          numberOfLines={1}
        >
          {truncateAddr(entry.addr)}
        </Text>
      ))}
      {group.row ? <InfraLinkDetail row={group.row} /> : null}
    </View>
  );
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
