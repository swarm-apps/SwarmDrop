import { Download, File as FileIcon, X } from "lucide-react-native";
import { useCallback, useState } from "react";
import {
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { getMobileCore } from "@/core/mobile-core";
import { getMobilePaths } from "@/core/paths";
import { useTransferStore } from "@/stores/transfer-store";

export function TransferOfferHost() {
  const current = useTransferStore((s) => s.currentOffer);
  const dismiss = useTransferStore((s) => s.dismissOffer);
  const registerSession = useTransferStore((s) => s.registerSession);
  const setError = useTransferStore((s) => s.setError);
  const [busy, setBusy] = useState<"accepting" | "rejecting" | null>(null);

  const open = current !== null;

  const accept = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("accepting");
    try {
      const { transfersInboxUri } = getMobilePaths();
      // acceptReceive 返回 void（接收会话由 core 异步启动 + EventBus 推 Progress）
      await getMobileCore().acceptReceive(current.id, transfersInboxUri);
      registerSession(current.id);
      dismiss(current.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }, [busy, current, dismiss, setError, registerSession]);

  const reject = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("rejecting");
    try {
      await getMobileCore().rejectReceive(current.id);
    } catch (err) {
      console.warn("[transfer-offer-host] reject failed:", err);
    } finally {
      dismiss(current.id);
      setBusy(null);
    }
  }, [busy, current, dismiss]);

  if (!open || !current) {
    return null;
  }

  const totalLabel = formatBytes(Number(current.offer.totalSize));

  return (
    <Modal
      animationType="fade"
      transparent
      visible={open}
      onRequestClose={reject}
    >
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <View style={styles.header}>
            <View style={styles.iconWrap}>
              <Download color="#2563EB" size={22} />
            </View>
            <View style={styles.headerText}>
              <Text style={styles.title}>新的文件传输</Text>
              <Text style={styles.meta}>
                {current.offer.deviceName} · {current.offer.files.length} 个文件
                · {totalLabel}
              </Text>
            </View>
          </View>

          <ScrollView
            style={styles.fileList}
            contentContainerStyle={styles.fileListContent}
          >
            {current.offer.files
              .slice(0, 5)
              .map((f: (typeof current.offer.files)[number]) => (
                <View key={f.fileId} style={styles.fileRow}>
                  <FileIcon color="#64748B" size={16} />
                  <Text style={styles.fileName} numberOfLines={1}>
                    {f.name}
                  </Text>
                  <Text style={styles.fileSize}>{formatBytes(Number(f.size))}</Text>
                </View>
              ))}
            {current.offer.files.length > 5 ? (
              <Text style={styles.fileMore}>
                ...还有 {current.offer.files.length - 5} 个文件
              </Text>
            ) : null}
          </ScrollView>

          <View style={styles.actions}>
            <Pressable
              onPress={accept}
              disabled={busy !== null}
              style={[styles.acceptButton, busy !== null && styles.disabled]}
            >
              <Download color="#FFFFFF" size={16} />
              <Text style={styles.acceptText}>
                {busy === "accepting" ? "接收中..." : "接收"}
              </Text>
            </Pressable>
            <Pressable
              onPress={reject}
              disabled={busy !== null}
              style={[styles.rejectButton, busy !== null && styles.disabled]}
            >
              <X color="#0F172A" size={16} />
              <Text style={styles.rejectText}>拒绝</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

const styles = StyleSheet.create({
  backdrop: {
    alignItems: "center",
    backgroundColor: "rgba(15, 23, 42, 0.55)",
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    gap: 14,
    maxHeight: "80%",
    padding: 20,
    width: "100%",
  },
  header: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
  },
  iconWrap: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 40,
    justifyContent: "center",
    width: 40,
  },
  headerText: {
    flex: 1,
    gap: 2,
  },
  title: {
    color: "#0F172A",
    fontSize: 17,
    fontWeight: "700",
  },
  meta: {
    color: "#64748B",
    fontSize: 12,
  },
  fileList: {
    backgroundColor: "#F8FAFC",
    borderRadius: 10,
    maxHeight: 220,
  },
  fileListContent: {
    gap: 6,
    padding: 12,
  },
  fileRow: {
    alignItems: "center",
    flexDirection: "row",
    gap: 10,
  },
  fileName: {
    color: "#0F172A",
    flex: 1,
    fontSize: 13,
  },
  fileSize: {
    color: "#64748B",
    fontSize: 12,
  },
  fileMore: {
    color: "#64748B",
    fontSize: 12,
    fontStyle: "italic",
    marginTop: 4,
    textAlign: "center",
  },
  actions: {
    gap: 10,
  },
  acceptButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    flexDirection: "row",
    gap: 6,
    justifyContent: "center",
    minHeight: 48,
  },
  acceptText: {
    color: "#FFFFFF",
    fontSize: 15,
    fontWeight: "700",
  },
  rejectButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    flexDirection: "row",
    gap: 6,
    justifyContent: "center",
    minHeight: 48,
  },
  rejectText: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
  disabled: {
    opacity: 0.5,
  },
});
