import { useLocalSearchParams, useRouter } from "expo-router";
import { ArrowLeft, MonitorSmartphone } from "lucide-react-native";
import { useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { getMobileCore } from "@/core/mobile-core";

export default function FoundDevice() {
  const router = useRouter();
  const params = useLocalSearchParams<{
    peerId: string;
    code: string;
    hostname: string;
    os: string;
    platform: string;
    arch: string;
  }>();

  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onConfirm = async () => {
    setError(null);
    setConfirming(true);
    try {
      const result = await getMobileCore().requestPairing(
        params.peerId,
        params.code,
        [],
      );
      if (!result.accepted) {
        setError(`配对被拒绝${result.reason ? `：${result.reason}` : ""}`);
        return;
      }
      router.replace({
        pathname: "/pairing/success" as never,
        params: {
          peerId: params.peerId,
          hostname: params.hostname,
          os: params.os,
          platform: params.platform,
          arch: params.arch,
        },
      } as never);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setConfirming(false);
    }
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.header}>
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          disabled={confirming}
          style={styles.backButton}
        >
          <ArrowLeft color="#0F172A" size={22} />
        </Pressable>
        <Text style={styles.headerTitle}>确认设备</Text>
        <View style={styles.backButton} />
      </View>

      <View style={styles.body}>
        <View style={styles.heroIcon}>
          <MonitorSmartphone color="#2563EB" size={36} />
        </View>
        <Text style={styles.title}>找到设备</Text>
        <Text style={styles.subtitle}>确认这是你要配对的设备？</Text>

        <View style={styles.card}>
          <Row label="主机名" value={params.hostname} />
          <Divider />
          <Row label="系统" value={`${params.os} · ${params.arch}`} />
          <Divider />
          <Row label="平台" value={params.platform} />
          <Divider />
          <Row label="设备 ID" value={truncatePeerId(params.peerId)} mono />
        </View>

        {error !== null ? <Text style={styles.error}>{error}</Text> : null}
      </View>

      <View style={styles.footer}>
        <Pressable
          onPress={onConfirm}
          disabled={confirming}
          style={[styles.primaryButton, confirming && styles.disabled]}
        >
          {confirming ? (
            <ActivityIndicator color="#FFFFFF" />
          ) : (
            <Text style={styles.primaryButtonText}>确认配对</Text>
          )}
        </Pressable>
        <Pressable
          onPress={() => router.back()}
          disabled={confirming}
          style={styles.secondaryButton}
        >
          <Text style={styles.secondaryButtonText}>取消</Text>
        </Pressable>
      </View>
    </SafeAreaView>
  );
}

function Row({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <View style={styles.row}>
      <Text style={styles.rowLabel}>{label}</Text>
      <Text style={[styles.rowValue, mono && styles.mono]} numberOfLines={1}>
        {value}
      </Text>
    </View>
  );
}

function Divider() {
  return <View style={styles.divider} />;
}

function truncatePeerId(peerId: string): string {
  if (peerId.length <= 16) return peerId;
  return `${peerId.slice(0, 8)}...${peerId.slice(-6)}`;
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
  body: {
    alignItems: "center",
    flex: 1,
    gap: 12,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  heroIcon: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 72,
    justifyContent: "center",
    marginBottom: 8,
    width: 72,
  },
  title: {
    color: "#0F172A",
    fontSize: 22,
    fontWeight: "800",
  },
  subtitle: {
    color: "#64748B",
    fontSize: 14,
    marginBottom: 8,
    textAlign: "center",
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    gap: 0,
    padding: 16,
    width: "100%",
  },
  row: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
    paddingVertical: 10,
  },
  rowLabel: {
    color: "#64748B",
    fontSize: 13,
  },
  rowValue: {
    color: "#0F172A",
    fontSize: 13,
    fontWeight: "600",
    maxWidth: "60%",
  },
  mono: {
    fontFamily: "monospace",
  },
  divider: {
    backgroundColor: "#E2E8F0",
    height: 1,
  },
  error: {
    color: "#B91C1C",
    fontSize: 13,
    marginTop: 12,
    textAlign: "center",
  },
  footer: {
    gap: 10,
    paddingBottom: 24,
    paddingHorizontal: 24,
  },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 12,
    justifyContent: "center",
    minHeight: 52,
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontSize: 17,
    fontWeight: "700",
  },
  secondaryButton: {
    alignItems: "center",
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    justifyContent: "center",
    minHeight: 52,
  },
  secondaryButtonText: {
    color: "#0F172A",
    fontSize: 16,
    fontWeight: "600",
  },
  disabled: {
    opacity: 0.6,
  },
});
