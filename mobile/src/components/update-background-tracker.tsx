/**
 * 后台下载状态条。用户在 UpdateProgressModal 点了"后台下载"后，对话框关闭，
 * 切换到这个贴底浮动条（半透明深色卡片），不阻塞操作。SwarmDrop-RN 没有引入
 * toast 库，所以自己实现一个简化版。
 */
import { Download } from "lucide-react-native";
import { Platform, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useUpdateStore } from "@/stores/update-store";

export function UpdateBackgroundTracker() {
  const status = useUpdateStore((s) => s.status);
  const backgrounded = useUpdateStore((s) => s.backgrounded);
  const percent = useUpdateStore((s) => s.progress?.percent ?? 0);
  const insets = useSafeAreaInsets();

  if (Platform.OS !== "android") return null;
  const visible = backgrounded && status === "downloading";
  if (!visible) return null;

  return (
    <View
      style={[styles.wrap, { bottom: insets.bottom + 16 }]}
      pointerEvents="none"
    >
      <View style={styles.card}>
        <Download color="#FFFFFF" size={16} />
        <Text style={styles.text}>正在下载新版本 · {percent}%</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: {
    alignItems: "center",
    left: 0,
    position: "absolute",
    right: 0,
  },
  card: {
    alignItems: "center",
    backgroundColor: "rgba(15, 23, 42, 0.92)",
    borderRadius: 999,
    flexDirection: "row",
    gap: 8,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  text: {
    color: "#FFFFFF",
    fontSize: 13,
    fontWeight: "600",
  },
});
