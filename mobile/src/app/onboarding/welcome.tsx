import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Hexagon, Lock, Radio, ShieldCheck } from "lucide-react-native";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useOnboardingStore } from "@/stores/onboarding-store";

export default function Welcome() {
  const { t } = useLingui();
  const router = useRouter();
  const nextStep = useOnboardingStore((s) => s.nextStep);

  const features = [
    { Icon: ShieldCheck, text: t`端到端加密,数据不经服务器` },
    { Icon: Radio, text: t`P2P 直连,跨网络也能传输` },
    { Icon: Lock, text: t`设备身份本地保存,无需账号` },
  ];

  const onNext = () => {
    nextStep();
    router.push("/onboarding/setup" as never);
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.content}>
        <View style={styles.hero}>
          <Hexagon color="#2563EB" size={80} strokeWidth={1.5} />
          <Text style={styles.title}>SwarmDrop</Text>
          <Text style={styles.subtitle}>
            <Trans>去中心化跨网络文件传输</Trans>
          </Text>
        </View>

        <View style={styles.featureList}>
          {features.map(({ Icon, text }) => (
            <View key={text} style={styles.feature}>
              <Icon color="#2563EB" size={22} strokeWidth={2} />
              <Text style={styles.featureText}>{text}</Text>
            </View>
          ))}
        </View>
      </View>

      <View style={styles.footer}>
        <Pressable
          onPress={onNext}
          style={styles.primaryButton}
          accessibilityRole="button"
        >
          <Text style={styles.primaryButtonText}>
            <Trans>开始使用</Trans>
          </Text>
        </Pressable>
        <View style={styles.dots}>
          <View style={[styles.dot, styles.dotActive]} />
          <View style={styles.dot} />
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
    gap: 40,
    justifyContent: "center",
  },
  hero: {
    alignItems: "center",
    gap: 12,
  },
  title: {
    color: "#0F172A",
    fontSize: 34,
    fontWeight: "800",
  },
  subtitle: {
    color: "#64748B",
    fontSize: 15,
  },
  featureList: {
    gap: 18,
    paddingHorizontal: 8,
  },
  feature: {
    alignItems: "center",
    flexDirection: "row",
    gap: 14,
  },
  featureText: {
    color: "#0F172A",
    fontSize: 15,
    flex: 1,
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
