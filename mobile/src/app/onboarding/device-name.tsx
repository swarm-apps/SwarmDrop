import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { ArrowLeft, Smartphone } from "lucide-react-native";
import { useEffect, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { applyDeviceName, suggestedDeviceName } from "@/lib/device-name";
import { useOnboardingStore } from "@/stores/onboarding-store";
import { usePreferencesStore } from "@/stores/preferences-store";

export default function DeviceName() {
  const { t } = useLingui();
  const router = useRouter();
  const nextStep = useOnboardingStore((s) => s.nextStep);
  const existing = usePreferencesStore((s) => s.deviceName);

  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 首次进入：用 expo-device 给一个合理的默认值（之前可能已经存过名字就用旧的）
  useEffect(() => {
    setName(existing?.trim() || suggestedDeviceName());
  }, [existing]);

  const trimmed = name.trim();
  const disabled = saving || trimmed.length === 0;

  const onNext = async () => {
    setSaving(true);
    setError(null);
    try {
      await applyDeviceName(trimmed);
      nextStep();
      router.push("/onboarding/setup" as never);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <SafeAreaView style={styles.screen} edges={["top", "bottom"]}>
      <View style={styles.content}>
        <Pressable
          onPress={() => router.back()}
          hitSlop={12}
          accessibilityLabel={t`返回`}
          style={styles.backButton}
        >
          <ArrowLeft color="#0F172A" size={24} />
        </Pressable>

        <View style={styles.iconWrap}>
          <Smartphone color="#2563EB" size={48} strokeWidth={1.5} />
        </View>

        <View style={styles.titleBlock}>
          <Text style={styles.title}>
            <Trans>给设备取个名字</Trans>
          </Text>
          <Text style={styles.subtitle}>
            <Trans>
              其他设备配对时会看到这个名称{"\n"}你可以随时在设置里修改
            </Trans>
          </Text>
        </View>

        <View style={styles.inputBlock}>
          <Text style={styles.label}>
            <Trans>设备名称</Trans>
          </Text>
          <TextInput
            value={name}
            onChangeText={setName}
            autoFocus
            maxLength={40}
            placeholder={t`我的 iPhone`}
            placeholderTextColor="#94A3B8"
            style={styles.input}
          />
          {error !== null ? <Text style={styles.errorText}>{error}</Text> : null}
        </View>
      </View>

      <View style={styles.footer}>
        <Pressable
          onPress={onNext}
          disabled={disabled}
          style={[styles.primaryButton, disabled && styles.primaryButtonDisabled]}
          accessibilityRole="button"
          accessibilityLabel={t`继续`}
        >
          {saving ? (
            <ActivityIndicator color="#FFFFFF" />
          ) : (
            <Text style={styles.primaryButtonText}>
              <Trans>继续</Trans>
            </Text>
          )}
        </Pressable>
        <View style={styles.dots}>
          <View style={styles.dot} />
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
    gap: 24,
  },
  backButton: {
    height: 44,
    justifyContent: "center",
    marginLeft: -8,
    width: 44,
  },
  iconWrap: {
    alignItems: "center",
    alignSelf: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 96,
    justifyContent: "center",
    marginTop: 8,
    width: 96,
  },
  titleBlock: {
    alignItems: "center",
    gap: 10,
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
  inputBlock: {
    gap: 8,
    marginTop: 12,
  },
  label: {
    color: "#0F172A",
    fontSize: 14,
    fontWeight: "600",
  },
  input: {
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 12,
    borderWidth: 1,
    color: "#0F172A",
    fontSize: 16,
    height: 50,
    paddingHorizontal: 14,
  },
  errorText: {
    color: "#B91C1C",
    fontSize: 13,
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
