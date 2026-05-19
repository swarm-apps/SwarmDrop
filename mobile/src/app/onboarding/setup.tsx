import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { CheckCircle2, KeyRound } from "lucide-react-native";
import { useEffect } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { useOnboardingStore } from "@/stores/onboarding-store";

export default function Setup() {
  const { t } = useLingui();
  const router = useRouter();
  const markCompleted = useOnboardingStore((s) => s.markCompleted);
  const { loadIdentity, peerId, error } = useMobileCoreStore();

  useEffect(() => {
    void loadIdentity();
  }, [loadIdentity]);

  const ready = peerId !== null && error === null;
  const failed = error !== null;

  const onEnter = () => {
    markCompleted();
    router.replace("/(main)" as never);
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.content}>
        <View style={styles.iconWrap}>
          {ready ? (
            <CheckCircle2 color="#16A34A" size={64} strokeWidth={2} />
          ) : (
            <KeyRound color="#2563EB" size={56} strokeWidth={2} />
          )}
        </View>

        <Text style={styles.title}>
          {ready ? t`一切就绪` : failed ? t`初始化失败` : t`正在准备你的设备`}
        </Text>
        <Text style={styles.subtitle}>
          {ready
            ? t`设备身份已就绪,可以开始配对和传输文件了`
            : failed
              ? t`请稍后重试,或检查存储权限`
              : t`正在生成本地身份...`}
        </Text>

        {!ready && !failed ? (
          <ActivityIndicator
            color="#2563EB"
            size="large"
            style={styles.spinner}
          />
        ) : null}

        {peerId !== null ? (
          <View style={styles.infoCard}>
            <Text style={styles.infoLabel}>
              <Trans>设备 ID</Trans>
            </Text>
            <Text style={styles.infoValue} numberOfLines={1}>
              {`${peerId.slice(0, 16)}...${peerId.slice(-6)}`}
            </Text>
          </View>
        ) : null}

        {error !== null ? <Text style={styles.errorText}>{error}</Text> : null}
      </View>

      <View style={styles.footer}>
        {failed ? (
          <Pressable
            onPress={() => loadIdentity()}
            style={styles.primaryButton}
          >
            <Text style={styles.primaryButtonText}>
              <Trans>重试</Trans>
            </Text>
          </Pressable>
        ) : (
          <Pressable
            onPress={onEnter}
            disabled={!ready}
            style={[
              styles.primaryButton,
              !ready && styles.primaryButtonDisabled,
            ]}
          >
            <Text style={styles.primaryButtonText}>
              <Trans>进入 SwarmDrop</Trans>
            </Text>
          </Pressable>
        )}
        <View style={styles.dots}>
          <View style={styles.dot} />
          <View style={styles.dot} />
          <View style={[styles.dot, styles.dotActive]} />
        </View>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  screen: {
    backgroundColor: "#F8FAFC",
    flex: 1,
    paddingHorizontal: 24,
    paddingVertical: 16,
  },
  content: {
    flex: 1,
    gap: 16,
    justifyContent: "center",
    alignItems: "center",
  },
  iconWrap: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 96,
    justifyContent: "center",
    marginBottom: 12,
    width: 96,
  },
  title: {
    color: "#0F172A",
    fontSize: 26,
    fontWeight: "800",
    textAlign: "center",
  },
  subtitle: {
    color: "#475569",
    fontSize: 15,
    lineHeight: 22,
    textAlign: "center",
  },
  spinner: {
    marginVertical: 16,
  },
  infoCard: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 10,
    borderWidth: 1,
    marginTop: 12,
    padding: 14,
    width: "100%",
  },
  infoLabel: {
    color: "#64748B",
    fontSize: 12,
    marginBottom: 4,
  },
  infoValue: {
    color: "#0F172A",
    fontFamily: "monospace",
    fontSize: 13,
  },
  errorText: {
    color: "#B91C1C",
    fontSize: 13,
    textAlign: "center",
  },
  footer: {
    gap: 16,
  },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 12,
    justifyContent: "center",
    minHeight: 52,
  },
  primaryButtonDisabled: {
    backgroundColor: "#94A3B8",
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontSize: 17,
    fontWeight: "700",
  },
  dots: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
    justifyContent: "center",
  },
  dot: {
    backgroundColor: "#CBD5E1",
    borderRadius: 999,
    height: 8,
    width: 8,
  },
  dotActive: {
    backgroundColor: "#2563EB",
    height: 10,
    width: 10,
  },
});
