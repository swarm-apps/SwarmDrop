import { Copy, KeyRound, RefreshCcw } from "lucide-react-native";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";

interface Props {
  code?: string | null;
  expiresAt?: Date | null;
  loading?: boolean;
  error?: string | null;
  onGenerate?: () => void;
  onExpire?: () => void;
  onCopy?: () => void;
}

export function CodePairingCard({
  code,
  expiresAt,
  loading = false,
  error,
  onGenerate,
  onExpire,
  onCopy,
}: Props) {
  const hasCode = code !== undefined && code !== null;
  const remaining = useExpiresCountdown(
    hasCode ? (expiresAt ?? null) : null,
    onExpire,
  );

  if (!hasCode) {
    return (
      <View style={styles.card}>
        <View style={styles.headerRow}>
          <KeyRound color="#2563EB" size={18} />
          <Text style={styles.title}>分享配对码</Text>
        </View>
        <Text style={styles.hint}>
          生成 6 位配对码，让其他设备在 SwarmDrop 中输入即可建立配对
        </Text>
        <Pressable
          onPress={onGenerate}
          disabled={loading}
          style={[styles.primaryButton, loading && styles.disabled]}
        >
          {loading ? (
            <ActivityIndicator color="#FFFFFF" size="small" />
          ) : (
            <Text style={styles.primaryButtonText}>生成配对码</Text>
          )}
        </Pressable>
        {error !== null && error !== undefined ? (
          <Text style={styles.errorText}>{error}</Text>
        ) : null}
      </View>
    );
  }

  const expiryLabel =
    remaining > 0
      ? `${formatMmss(remaining)} 后过期 · 在另一台设备输入此码`
      : "已过期，请重新生成";

  return (
    <View style={[styles.card, styles.cardActive]}>
      <View style={styles.headerRow}>
        <KeyRound color="#2563EB" size={18} />
        <Text style={styles.title}>分享配对码</Text>
        <View style={styles.spacer} />
        <Pressable
          onPress={onGenerate}
          hitSlop={8}
          accessibilityLabel="重新生成"
        >
          <RefreshCcw color="#64748B" size={16} />
        </Pressable>
      </View>
      <Text style={styles.code}>{code}</Text>
      <Text style={styles.expiry}>{expiryLabel}</Text>
      <Pressable onPress={onCopy} style={styles.copyButton} hitSlop={8}>
        <Copy color="#0F172A" size={14} />
        <Text style={styles.copyText}>复制</Text>
      </Pressable>
    </View>
  );
}

function formatMmss(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    gap: 10,
    padding: 16,
  },
  cardActive: {
    borderColor: "#BFDBFE",
  },
  headerRow: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
  },
  title: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "700",
  },
  hint: {
    color: "#64748B",
    fontSize: 13,
    lineHeight: 18,
  },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 42,
    marginTop: 4,
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontSize: 14,
    fontWeight: "700",
  },
  code: {
    color: "#0F172A",
    fontSize: 32,
    fontWeight: "800",
    letterSpacing: 8,
    paddingVertical: 4,
    textAlign: "center",
  },
  expiry: {
    color: "#64748B",
    fontSize: 12,
    textAlign: "center",
  },
  copyButton: {
    alignItems: "center",
    alignSelf: "center",
    borderColor: "#E2E8F0",
    borderRadius: 8,
    borderWidth: 1,
    flexDirection: "row",
    gap: 6,
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  copyText: {
    color: "#0F172A",
    fontSize: 12,
    fontWeight: "600",
  },
  spacer: {
    flex: 1,
  },
  disabled: {
    opacity: 0.5,
  },
  errorText: {
    color: "#B91C1C",
    fontSize: 12,
  },
});
