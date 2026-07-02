import { Trans, useLingui } from "@lingui/react/macro";
import { Plus, RotateCw, Trash2 } from "lucide-react-native";
import { Fragment, useCallback, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useShallow } from "zustand/react/shallow";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

/** 默认引导节点(与后端 BOOTSTRAP_NODES 对应,只读展示) */
const DEFAULT_BOOTSTRAP_NODES = [
  "/ip4/47.115.172.218/tcp/4001/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
  "/ip4/47.115.172.218/udp/4001/quic-v1/p2p/12D3KooWCq8xgrSap7VZZHpW7EYXw8zFmNEgru9D7cGHGW3bMASX",
];

function isValidMultiaddr(addr: string): boolean {
  return addr.startsWith("/") && addr.includes("/p2p/");
}

function truncateAddr(addr: string): string {
  if (addr.length <= 50) return addr;
  const p2pIdx = addr.indexOf("/p2p/");
  if (p2pIdx === -1) return `${addr.slice(0, 25)}...${addr.slice(-15)}`;
  const prefix = addr.slice(0, Math.min(p2pIdx, 25));
  const peerId = addr.slice(p2pIdx + 5);
  const shortPeerId =
    peerId.length > 12 ? `${peerId.slice(0, 6)}...${peerId.slice(-6)}` : peerId;
  return `${prefix}/p2p/${shortPeerId}`;
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
  const { runtimeState, shutdownNode, startNode } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      shutdownNode: s.shutdownNode,
      startNode: s.startNode,
    })),
  );

  const [inputValue, setInputValue] = useState("");
  const [showInput, setShowInput] = useState(false);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [pendingRemove, setPendingRemove] = useState<string | null>(null);

  const handleAdd = () => {
    const addr = inputValue.trim();
    if (!addr) return;
    if (!isValidMultiaddr(addr)) {
      toast.error(t`地址格式无效,需以 / 开头且包含 /p2p/`);
      return;
    }
    if (
      customBootstrapNodes.includes(addr) ||
      DEFAULT_BOOTSTRAP_NODES.includes(addr)
    ) {
      toast.error(t`该节点已存在`);
      return;
    }
    addBootstrapNode(addr);
    setInputValue("");
    setShowInput(false);
    if (runtimeState === "running") setNeedsRestart(true);
  };

  const handleRemove = (addr: string) => {
    removeBootstrapNode(addr);
    if (runtimeState === "running") setNeedsRestart(true);
    toast.success(t`引导节点已删除`);
  };

  const confirmRemove = () => {
    if (pendingRemove) handleRemove(pendingRemove);
    setPendingRemove(null);
  };

  const handleRestart = useCallback(async () => {
    setRestarting(true);
    try {
      await shutdownNode();
      await startNode();
      setNeedsRestart(false);
      toast.success(t`节点已重启`);
    } catch (err) {
      toast.error(t`重启节点失败`, err);
    } finally {
      setRestarting(false);
    }
  }, [shutdownNode, startNode, t]);

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top"]}
      testID="bootstrap-nodes-screen"
    >
      <SettingsHeader title={t`引导节点`} />
      <ScrollView
        contentContainerClassName="gap-3 px-5 pt-2 pb-8"
        showsVerticalScrollIndicator={false}
      >
        <Text className="text-[12px] text-muted-foreground px-1">
          <Trans>引导节点用于发现 P2P 网络。修改后需重启节点生效。</Trans>
        </Text>

        <View className="rounded-lg border border-border bg-card overflow-hidden">
          {DEFAULT_BOOTSTRAP_NODES.map((addr, idx) => (
            <Fragment key={addr}>
              <View className="flex-row items-center justify-between gap-2 p-3.5">
                <Text
                  className="flex-1 font-mono text-[11px] text-muted-foreground"
                  numberOfLines={1}
                >
                  {truncateAddr(addr)}
                </Text>
                <View className="rounded bg-muted px-1.5 py-0.5">
                  <Text className="text-[10px] text-muted-foreground">
                    <Trans>默认</Trans>
                  </Text>
                </View>
              </View>
              {idx < DEFAULT_BOOTSTRAP_NODES.length - 1 ||
              customBootstrapNodes.length > 0 ||
              showInput ? (
                <View className="h-px bg-border" />
              ) : null}
            </Fragment>
          ))}

          {customBootstrapNodes.map((addr, idx) => (
            <Fragment key={addr}>
              <View className="flex-row items-center justify-between gap-2 p-3.5">
                <Text
                  className="flex-1 font-mono text-[11px] text-foreground"
                  numberOfLines={1}
                >
                  {truncateAddr(addr)}
                </Text>
                <Pressable
                  onPress={() => setPendingRemove(addr)}
                  hitSlop={8}
                  accessibilityRole="button"
                  accessibilityLabel={t`删除`}
                  className="size-7 items-center justify-center"
                >
                  <Trash2 color={colors.destructive} size={14} />
                </Pressable>
              </View>
              {idx < customBootstrapNodes.length - 1 || showInput ? (
                <View className="h-px bg-border" />
              ) : null}
            </Fragment>
          ))}

          {showInput ? (
            <View className="gap-2 p-3.5">
              <TextInput
                value={inputValue}
                onChangeText={setInputValue}
                accessibilityLabel={t`引导节点地址`}
                placeholder={t`/ip4/.../tcp/.../p2p/...`}
                placeholderTextColor={colors.mutedForeground}
                autoFocus
                autoCapitalize="none"
                autoCorrect={false}
                className="h-10 rounded-lg border border-border bg-background px-3 font-mono text-[12px] text-foreground"
              />
              <View className="flex-row gap-2">
                <Pressable
                  onPress={() => {
                    setShowInput(false);
                    setInputValue("");
                  }}
                  accessibilityRole="button"
                  className="flex-1 h-10 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
                >
                  <Text className="text-[13px] text-foreground">
                    <Trans>取消</Trans>
                  </Text>
                </Pressable>
                <Pressable
                  onPress={handleAdd}
                  accessibilityRole="button"
                  className="flex-1 h-10 items-center justify-center rounded-xl bg-primary active:opacity-70"
                >
                  <Text className="text-[13px] font-semibold text-primary-foreground">
                    <Trans>添加</Trans>
                  </Text>
                </Pressable>
              </View>
            </View>
          ) : (
            <Pressable
              onPress={() => setShowInput(true)}
              accessibilityRole="button"
              testID="bootstrap-add-custom-node-button"
              className="flex-row items-center gap-2 p-3.5 active:bg-muted"
            >
              <Plus color={colors.mutedForeground} size={14} />
              <Text className="text-[13px] text-muted-foreground">
                <Trans>添加自定义引导节点</Trans>
              </Text>
            </Pressable>
          )}
        </View>

        {needsRestart && runtimeState === "running" ? (
          <View className="flex-row items-center justify-between gap-2 rounded-xl border border-warning/30 bg-warning/10 p-3">
            <Text className="flex-1 text-[12px] text-warning-ink">
              <Trans>引导节点已变更,需重启节点生效</Trans>
            </Text>
            <Pressable
              onPress={handleRestart}
              disabled={restarting}
              accessibilityRole="button"
              className="h-8 flex-row items-center gap-1 rounded-lg border border-border bg-card px-3 active:opacity-70 disabled:opacity-50"
            >
              {restarting ? (
                <ActivityIndicator color={colors.foreground} size="small" />
              ) : (
                <RotateCw color={colors.foreground} size={12} />
              )}
              <Text className="text-[12px] font-medium text-foreground">
                {restarting ? <Trans>重启中</Trans> : <Trans>重启节点</Trans>}
              </Text>
            </Pressable>
          </View>
        ) : null}
      </ScrollView>

      <ConfirmDialog
        open={pendingRemove !== null}
        onOpenChange={(open) => {
          if (!open) setPendingRemove(null);
        }}
        title={<Trans>删除引导节点</Trans>}
        description={
          <Trans>
            该节点将不再作为自动发现的兜底路径，可能影响连通性。之后可随时重新添加。
          </Trans>
        }
        actionLabel={<Trans>删除</Trans>}
        onAction={confirmRemove}
      />
    </SafeAreaView>
  );
}
