import { useRouter } from "expo-router";
import { ArrowLeft } from "lucide-react-native";
import { useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { getMobileCore } from "@/core/mobile-core";

const CODE_LENGTH = 6;
const SLOT_KEYS = ["s0", "s1", "s2", "s3", "s4", "s5"] as const;

export default function InputCode() {
  const router = useRouter();
  const inputRef = useRef<TextInput>(null);
  const [code, setCode] = useState("");
  const [looking, setLooking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onLookup = async (filled: string) => {
    if (looking) return;
    if (filled.length !== CODE_LENGTH) return;
    setError(null);
    setLooking(true);
    try {
      const remote = await getMobileCore().lookupDeviceByCode(filled);
      router.push({
        pathname: "/pairing/found-device" as never,
        params: {
          peerId: remote.peerId,
          code: filled,
          hostname: remote.hostname,
          os: remote.os,
          platform: remote.platform,
          arch: remote.arch,
        },
      } as never);
    } catch (_err) {
      setError("配对码无效或已过期");
      setCode("");
    } finally {
      setLooking(false);
    }
  };

  const handleChange = (value: string) => {
    const digits = value.replace(/\D/g, "").slice(0, CODE_LENGTH);
    setCode(digits);
    if (digits.length === CODE_LENGTH) {
      void onLookup(digits);
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
        <Text style={styles.headerTitle}>输入配对码</Text>
        <View style={styles.backButton} />
      </View>

      <Pressable
        style={styles.body}
        onPress={() => inputRef.current?.focus()}
        accessibilityRole="button"
      >
        <Text style={styles.hint}>输入对方设备显示的 6 位配对码</Text>
        <View style={styles.slots}>
          {SLOT_KEYS.map((slotKey, i) => {
            const char = code[i] ?? "";
            const active = i === code.length;
            return (
              <View
                key={slotKey}
                style={[styles.slot, active && styles.slotActive]}
              >
                <Text style={styles.slotText}>{char}</Text>
              </View>
            );
          })}
        </View>
        {error !== null ? (
          <Text style={styles.error}>{error}</Text>
        ) : looking ? (
          <ActivityIndicator color="#2563EB" />
        ) : null}
        <TextInput
          ref={inputRef}
          autoFocus
          keyboardType="number-pad"
          value={code}
          onChangeText={handleChange}
          maxLength={CODE_LENGTH}
          style={styles.hiddenInput}
          caretHidden
        />
      </Pressable>

      <View style={styles.footer}>
        <Pressable
          onPress={() => onLookup(code)}
          disabled={code.length !== CODE_LENGTH || looking}
          style={[
            styles.primaryButton,
            (code.length !== CODE_LENGTH || looking) &&
              styles.primaryButtonDisabled,
          ]}
        >
          {looking ? (
            <ActivityIndicator color="#FFFFFF" />
          ) : (
            <Text style={styles.primaryButtonText}>连接</Text>
          )}
        </Pressable>
      </View>
    </SafeAreaView>
  );
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
    gap: 24,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  hint: {
    color: "#64748B",
    fontSize: 14,
    textAlign: "center",
  },
  slots: {
    flexDirection: "row",
    gap: 10,
  },
  slot: {
    alignItems: "center",
    backgroundColor: "#FFFFFF",
    borderColor: "#E2E8F0",
    borderRadius: 10,
    borderWidth: 1,
    height: 60,
    justifyContent: "center",
    width: 48,
  },
  slotActive: {
    borderColor: "#2563EB",
    borderWidth: 2,
  },
  slotText: {
    color: "#0F172A",
    fontSize: 28,
    fontWeight: "700",
  },
  error: {
    color: "#B91C1C",
    fontSize: 13,
  },
  hiddenInput: {
    height: 1,
    opacity: 0,
    position: "absolute",
    width: 1,
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
  primaryButtonDisabled: {
    backgroundColor: "#94A3B8",
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontSize: 17,
    fontWeight: "700",
  },
});
