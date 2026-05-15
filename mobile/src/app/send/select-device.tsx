import { useRouter } from "expo-router";
import { ArrowLeft, Send, Smartphone, WifiOff } from "lucide-react-native";
import { useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { getMobileCore } from "@/core/mobile-core";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export default function SelectDevice() {
  const router = useRouter();
  const devices = useMobileCoreStore((s) => s.devices);
  const selectedFiles = useMobileCoreStore((s) => s.selectedFiles);
  const clearSelectedFiles = useMobileCoreStore((s) => s.clearSelectedFiles);
  const registerSession = useTransferStore((s) => s.registerSession);

  const onlinePaired = devices.filter(
    (d) => d.isPaired && d.status === "online",
  );
  const [sendingTo, setSendingTo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // file.size 来自 Rust u64 → bigint,累加用 bigint 字面量初值
  const totalSize = selectedFiles.reduce((s, f) => s + f.size, 0n);

  const onSend = async (peerId: string, peerName: string) => {
    if (sendingTo !== null || selectedFiles.length === 0) return;
    setError(null);
    setSendingTo(peerId);
    try {
      const prepared = await getMobileCore().prepareSend(selectedFiles);
      // 第四个参数 file_ids 为空数组时, 后端会发所有 prepared 文件
      const result = await getMobileCore().sendPrepared(
        prepared.preparedId,
        peerId,
        peerName,
        [],
      );
      registerSession(result.sessionId);
      clearSelectedFiles();
      router.back();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSendingTo(null);
    }
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.header}>
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          style={styles.backButton}
        >
          <ArrowLeft color="#0F172A" size={22} />
        </Pressable>
        <Text style={styles.headerTitle}>选择接收设备</Text>
        <View style={styles.backButton} />
      </View>

      <View style={styles.summary}>
        <Text style={styles.summaryTitle}>{selectedFiles.length} 个文件</Text>
        <Text style={styles.summaryMeta}>{formatBytes(totalSize)}</Text>
      </View>

      {error !== null ? <Text style={styles.errorText}>{error}</Text> : null}

      <ScrollView contentContainerStyle={styles.list}>
        {onlinePaired.length === 0 ? (
          <View style={styles.empty}>
            <WifiOff color="#94A3B8" size={36} />
            <Text style={styles.emptyTitle}>暂无在线的已配对设备</Text>
            <Text style={styles.emptyHint}>
              请确保对方设备在线，或先在主页完成配对
            </Text>
          </View>
        ) : (
          onlinePaired.map((d) => {
            const sending = sendingTo === d.peerId;
            const disabled = sendingTo !== null;
            return (
              <Pressable
                key={d.peerId}
                onPress={() => onSend(d.peerId, d.hostname)}
                disabled={disabled}
                style={[styles.row, disabled && styles.disabled]}
              >
                <View style={styles.rowIcon}>
                  <Smartphone color="#2563EB" size={20} />
                </View>
                <View style={styles.rowText}>
                  <Text style={styles.rowTitle} numberOfLines={1}>
                    {d.hostname}
                  </Text>
                  <Text style={styles.rowMeta} numberOfLines={1}>
                    {d.platform} · {d.connection ?? "在线"}
                  </Text>
                </View>
                {sending ? (
                  <ActivityIndicator color="#2563EB" />
                ) : (
                  <Send color="#2563EB" size={18} />
                )}
              </Pressable>
            );
          })
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

// bigint 输入(来自 Rust u64),内部转 number 走原有比较;TB+ 级别本机不可能命中
function formatBytes(bytes: bigint): string {
  const n = Number(bytes);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

const styles = StyleSheet.create({
  screen: {
    backgroundColor: "#F8FAFC",
    flex: 1,
  },
  header: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
    paddingHorizontal: 16,
    paddingTop: 8,
  },
  backButton: {
    alignItems: "center",
    height: 40,
    justifyContent: "center",
    width: 40,
  },
  headerTitle: {
    color: "#0F172A",
    fontSize: 17,
    fontWeight: "700",
  },
  summary: {
    backgroundColor: "#EFF6FF",
    borderRadius: 12,
    marginHorizontal: 16,
    marginTop: 12,
    padding: 14,
  },
  summaryTitle: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "700",
  },
  summaryMeta: {
    color: "#64748B",
    fontSize: 13,
    marginTop: 2,
  },
  errorText: {
    color: "#B91C1C",
    fontSize: 13,
    marginHorizontal: 16,
    marginTop: 8,
  },
  list: {
    gap: 8,
    padding: 16,
  },
  row: {
    alignItems: "center",
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    flexDirection: "row",
    gap: 12,
    padding: 14,
  },
  rowIcon: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 36,
    justifyContent: "center",
    width: 36,
  },
  rowText: {
    flex: 1,
    gap: 2,
  },
  rowTitle: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "700",
  },
  rowMeta: {
    color: "#64748B",
    fontSize: 12,
  },
  empty: {
    alignItems: "center",
    gap: 8,
    paddingVertical: 60,
  },
  emptyTitle: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
  emptyHint: {
    color: "#64748B",
    fontSize: 13,
    textAlign: "center",
  },
  disabled: {
    opacity: 0.5,
  },
});
