import { useRouter } from "expo-router";
import {
  Activity,
  AlertCircle,
  Cpu,
  Download,
  FileUp,
  RefreshCw,
  Send,
  ShieldCheck,
  Smartphone,
  Upload,
  Wifi,
  WifiOff,
} from "lucide-react-native";
import { useEffect } from "react";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { CodePairingCard } from "@/components/code-pairing-card";
import { usePairingCodeGenerator } from "@/hooks/usePairingCodeGenerator";
import {
  type RuntimeState,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function DevicesScreen() {
  const router = useRouter();
  const {
    devices,
    error,
    identityStatus,
    networkStatus,
    runtimeState,
    selectedFiles,
    chooseFiles,
    initialize,
    refreshDevices,
    setError,
  } = useMobileCoreStore();
  const sessions = useTransferStore((s) => s.sessions);
  const progress = useTransferStore((s) => s.progress);
  const transferError = useTransferStore((s) => s.lastError);
  const clearTransferError = useTransferStore((s) => s.setError);
  const codeGenerator = usePairingCodeGenerator();

  useEffect(() => {
    if (runtimeState === "stopped") {
      void initialize();
    }
  }, [initialize, runtimeState]);

  const onlinePaired = devices.filter((d) => d.isPaired);
  const activeSessions = Object.values(sessions);

  const onPickFiles = async () => {
    await chooseFiles();
    if (useMobileCoreStore.getState().selectedFiles.length > 0) {
      router.push("/send/select-device" as never);
    }
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top"]}>
      <View style={styles.header}>
        <View>
          <Text style={styles.title}>SwarmDrop</Text>
          <Text style={styles.subtitle}>跨网络文件传输</Text>
        </View>
        <StatusPill state={runtimeState} />
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.identityCard}>
          <ShieldCheck size={28} color="#2563EB" />
          <View style={styles.identityText}>
            <Text style={styles.cardTitle}>设备身份</Text>
            <Text style={styles.cardSubtitle} numberOfLines={1}>
              {identityStatus}
            </Text>
          </View>
        </View>

        {error !== null ? (
          <Pressable style={styles.errorBanner} onPress={() => setError(null)}>
            <AlertCircle color="#B91C1C" size={16} />
            <Text style={styles.errorText} numberOfLines={2}>
              {error}
            </Text>
          </Pressable>
        ) : null}

        {transferError !== null ? (
          <Pressable
            style={styles.errorBanner}
            onPress={() => clearTransferError(null)}
          >
            <AlertCircle color="#B91C1C" size={16} />
            <Text style={styles.errorText} numberOfLines={2}>
              {transferError}
            </Text>
          </Pressable>
        ) : null}

        <View style={styles.section}>
          <View style={styles.sectionHeader}>
            <Text style={styles.sectionTitle}>配对</Text>
            <Pressable
              onPress={() => router.push("/pairing/input-code" as never)}
              hitSlop={8}
            >
              <Text style={styles.linkText}>输入配对码</Text>
            </Pressable>
          </View>
          <CodePairingCard
            code={codeGenerator.code}
            expiresAt={codeGenerator.expiresAt}
            loading={codeGenerator.generating}
            error={codeGenerator.error}
            onGenerate={codeGenerator.generate}
            onExpire={codeGenerator.reset}
          />
        </View>

        <View style={styles.section}>
          <View style={styles.sectionHeader}>
            <Text style={styles.sectionTitle}>发送文件</Text>
          </View>
          <View style={styles.actionsRow}>
            <Pressable style={styles.primaryAction} onPress={onPickFiles}>
              <FileUp size={20} color="#FFFFFF" />
              <Text style={styles.primaryActionText}>选择并发送</Text>
            </Pressable>
            <Pressable style={styles.secondaryAction} onPress={refreshDevices}>
              <RefreshCw size={18} color="#0F172A" />
            </Pressable>
          </View>
          {selectedFiles.length > 0 ? (
            <Text style={styles.helperText}>
              已选 {selectedFiles.length} 个文件，可去选择接收设备
            </Text>
          ) : null}
        </View>

        {activeSessions.length > 0 ? (
          <View style={styles.section}>
            <Text style={styles.sectionTitle}>正在传输</Text>
            {activeSessions.map((session) => {
              const value =
                progress[session.sessionId] ?? session.progress ?? 0;
              const percent = Math.min(100, Math.round(value * 100));
              return (
                <View key={session.sessionId} style={styles.transferRow}>
                  <View style={styles.transferIcon}>
                    {session.direction === "outgoing" ? (
                      <Upload color="#2563EB" size={16} />
                    ) : (
                      <Download color="#16A34A" size={16} />
                    )}
                  </View>
                  <View style={styles.transferInfo}>
                    <Text style={styles.transferLabel}>
                      {session.direction === "outgoing" ? "发送中" : "接收中"} ·{" "}
                      {session.peerId.slice(0, 10)}...
                    </Text>
                    <View style={styles.progressBar}>
                      <View
                        style={[styles.progressFill, { width: `${percent}%` }]}
                      />
                    </View>
                  </View>
                  <Text style={styles.progressPercent}>{percent}%</Text>
                </View>
              );
            })}
          </View>
        ) : null}

        <View style={styles.section}>
          <View style={styles.sectionHeader}>
            <Text style={styles.sectionTitle}>设备列表</Text>
            <Pressable onPress={refreshDevices} hitSlop={8}>
              <Text style={styles.linkText}>刷新</Text>
            </Pressable>
          </View>
          {onlinePaired.length === 0 ? (
            <View style={styles.empty}>
              <Smartphone color="#94A3B8" size={32} />
              <Text style={styles.emptyTitle}>暂无已配对设备</Text>
              <Text style={styles.emptyHint}>使用配对码与其他设备建立连接</Text>
            </View>
          ) : (
            onlinePaired.map((d) => (
              <View key={d.peerId} style={styles.deviceRow}>
                <View style={styles.deviceIcon}>
                  <Smartphone color="#2563EB" size={18} />
                </View>
                <View style={styles.deviceText}>
                  <Text style={styles.deviceTitle} numberOfLines={1}>
                    {d.hostname}
                  </Text>
                  <Text style={styles.deviceMeta} numberOfLines={1}>
                    {d.platform} · {d.status}
                  </Text>
                </View>
                <Pressable
                  onPress={async () => {
                    await chooseFiles();
                    if (
                      useMobileCoreStore.getState().selectedFiles.length > 0
                    ) {
                      router.push("/send/select-device" as never);
                    }
                  }}
                  hitSlop={8}
                  disabled={d.status !== "online"}
                  style={[
                    styles.sendIcon,
                    d.status !== "online" && styles.disabled,
                  ]}
                >
                  <Send color="#2563EB" size={16} />
                </Pressable>
              </View>
            ))
          )}
        </View>

        <View style={styles.section}>
          <Text style={styles.sectionTitle}>网络状态</Text>
          <View style={styles.networkGrid}>
            <NetworkCell
              icon={<Wifi color="#2563EB" size={16} />}
              label="已连接"
              value={String(networkStatus?.connectedPeers ?? 0)}
            />
            <NetworkCell
              icon={<Activity color="#2563EB" size={16} />}
              label="已发现"
              value={String(networkStatus?.discoveredPeers ?? 0)}
            />
            <NetworkCell
              icon={<Cpu color="#2563EB" size={16} />}
              label="NAT"
              value={networkStatus?.natStatus ?? "未知"}
            />
            <NetworkCell
              icon={<WifiOff color="#2563EB" size={16} />}
              label="中继"
              value={networkStatus?.relayReady ? "就绪" : "未启用"}
            />
          </View>
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function StatusPill({ state }: { state: RuntimeState }) {
  const label =
    state === "running"
      ? "运行中"
      : state === "starting"
        ? "启动中"
        : state === "error"
          ? "错误"
          : "未启动";
  const bg =
    state === "running"
      ? "#DCFCE7"
      : state === "starting"
        ? "#FEF3C7"
        : state === "error"
          ? "#FEE2E2"
          : "#F1F5F9";
  const fg =
    state === "running"
      ? "#166534"
      : state === "starting"
        ? "#854D0E"
        : state === "error"
          ? "#991B1B"
          : "#475569";
  return (
    <View style={[styles.pill, { backgroundColor: bg }]}>
      <Text style={[styles.pillText, { color: fg }]}>{label}</Text>
    </View>
  );
}

function NetworkCell({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <View style={styles.networkCell}>
      <View style={styles.networkCellHeader}>
        {icon}
        <Text style={styles.networkCellLabel}>{label}</Text>
      </View>
      <Text style={styles.networkCellValue} numberOfLines={1}>
        {value}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    backgroundColor: "#F8FAFC",
    flex: 1,
    paddingHorizontal: 16,
  },
  header: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 12,
    paddingTop: 8,
  },
  title: {
    color: "#0F172A",
    fontSize: 28,
    fontWeight: "800",
  },
  subtitle: {
    color: "#64748B",
    fontSize: 13,
    marginTop: 2,
  },
  pill: {
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  pillText: {
    fontSize: 12,
    fontWeight: "700",
  },
  content: {
    gap: 16,
    paddingBottom: 32,
  },
  identityCard: {
    alignItems: "center",
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    flexDirection: "row",
    gap: 12,
    padding: 16,
  },
  identityText: {
    flex: 1,
    gap: 2,
  },
  cardTitle: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "700",
  },
  cardSubtitle: {
    color: "#64748B",
    fontSize: 12,
  },
  errorBanner: {
    alignItems: "center",
    backgroundColor: "#FEF2F2",
    borderColor: "#FCA5A5",
    borderRadius: 10,
    borderWidth: 1,
    flexDirection: "row",
    gap: 8,
    padding: 12,
  },
  errorText: {
    color: "#991B1B",
    flex: 1,
    fontSize: 13,
  },
  section: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    gap: 12,
    padding: 16,
  },
  sectionHeader: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
  },
  sectionTitle: {
    color: "#0F172A",
    fontSize: 16,
    fontWeight: "700",
  },
  linkText: {
    color: "#2563EB",
    fontSize: 13,
    fontWeight: "600",
  },
  actionsRow: {
    flexDirection: "row",
    gap: 10,
  },
  primaryAction: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    flex: 1,
    flexDirection: "row",
    gap: 8,
    justifyContent: "center",
    minHeight: 48,
  },
  primaryActionText: {
    color: "#FFFFFF",
    fontSize: 15,
    fontWeight: "700",
  },
  secondaryAction: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    height: 48,
    justifyContent: "center",
    width: 48,
  },
  helperText: {
    color: "#475569",
    fontSize: 12,
  },
  transferRow: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
  },
  transferIcon: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 32,
    justifyContent: "center",
    width: 32,
  },
  transferInfo: {
    flex: 1,
    gap: 6,
  },
  transferLabel: {
    color: "#0F172A",
    fontSize: 12,
    fontWeight: "600",
  },
  progressBar: {
    backgroundColor: "#E2E8F0",
    borderRadius: 999,
    height: 4,
    overflow: "hidden",
  },
  progressFill: {
    backgroundColor: "#2563EB",
    height: "100%",
  },
  progressPercent: {
    color: "#475569",
    fontSize: 12,
    fontWeight: "600",
    minWidth: 36,
    textAlign: "right",
  },
  deviceRow: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
    paddingVertical: 8,
  },
  deviceIcon: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 36,
    justifyContent: "center",
    width: 36,
  },
  deviceText: {
    flex: 1,
    gap: 2,
  },
  deviceTitle: {
    color: "#0F172A",
    fontSize: 14,
    fontWeight: "700",
  },
  deviceMeta: {
    color: "#64748B",
    fontSize: 12,
  },
  sendIcon: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 32,
    justifyContent: "center",
    width: 32,
  },
  disabled: {
    opacity: 0.4,
  },
  empty: {
    alignItems: "center",
    gap: 6,
    paddingVertical: 24,
  },
  emptyTitle: {
    color: "#0F172A",
    fontSize: 14,
    fontWeight: "600",
  },
  emptyHint: {
    color: "#64748B",
    fontSize: 12,
    textAlign: "center",
  },
  networkGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
  },
  networkCell: {
    backgroundColor: "#F8FAFC",
    borderRadius: 10,
    flex: 1,
    gap: 6,
    minWidth: "45%",
    padding: 12,
  },
  networkCellHeader: {
    alignItems: "center",
    flexDirection: "row",
    gap: 6,
  },
  networkCellLabel: {
    color: "#64748B",
    fontSize: 12,
  },
  networkCellValue: {
    color: "#0F172A",
    fontSize: 14,
    fontWeight: "700",
  },
});
