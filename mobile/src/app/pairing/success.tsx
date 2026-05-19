import { Trans, useLingui } from "@lingui/react/macro";
import { useLocalSearchParams, useRouter } from "expo-router";
import { CheckCircle2 } from "lucide-react-native";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

export default function PairingSuccess() {
  const { t } = useLingui();
  const router = useRouter();
  const params = useLocalSearchParams<{
    peerId: string;
    name?: string;
    hostname: string;
    os: string;
    platform: string;
    arch: string;
  }>();
  const displayName = params.name?.trim() || params.hostname;

  const finish = () => {
    router.dismissAll();
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.body}>
        <View style={styles.iconWrap}>
          <CheckCircle2 color="#16A34A" size={56} strokeWidth={2} />
        </View>
        <Text style={styles.title}>
          <Trans>配对成功</Trans>
        </Text>
        <Text style={styles.subtitle}>
          <Trans>已与 {displayName} 建立安全连接</Trans>
        </Text>

        <View style={styles.card}>
          <Row label={t`设备名`} value={displayName} />
          {params.name && params.name !== params.hostname ? (
            <>
              <Divider />
              <Row label={t`主机名`} value={params.hostname} />
            </>
          ) : null}
          <Divider />
          <Row label={t`系统`} value={`${params.os} · ${params.arch}`} />
          <Divider />
          <Row label={t`设备 ID`} value={truncatePeerId(params.peerId)} mono />
        </View>
      </View>

      <View style={styles.footer}>
        <Pressable onPress={finish} style={styles.primaryButton}>
          <Text style={styles.primaryButtonText}>
            <Trans>完成</Trans>
          </Text>
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
  body: {
    alignItems: "center",
    flex: 1,
    gap: 12,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  iconWrap: {
    alignItems: "center",
    backgroundColor: "#DCFCE7",
    borderRadius: 999,
    height: 96,
    justifyContent: "center",
    marginBottom: 12,
    width: 96,
  },
  title: {
    color: "#0F172A",
    fontSize: 24,
    fontWeight: "800",
  },
  subtitle: {
    color: "#64748B",
    fontSize: 14,
    marginBottom: 12,
    textAlign: "center",
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
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
  footer: {
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
});
