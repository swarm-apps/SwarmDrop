/**
 * 更新 UI 入口（仅 Android）。
 *
 * 单文件聚合四种状态对话框：available / force-required / downloading / error。
 * 后台下载状态的 Toast 浮动条由同目录 update-background-tracker.tsx 单独承载，
 * 因为它不是 Modal，需要持续挂在屏幕上。
 *
 * iOS 平台直接返回 null，UpdateHost 整体不挂载，store 也是 no-op，
 * 完整的 iOS 升级路径走 TestFlight / App Store。
 */
import { Trans, useLingui } from "@lingui/react/macro";
import { Download, ExternalLink, RefreshCw } from "lucide-react-native";
import {
  Linking,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { useUpdateStore } from "@/stores/update-store";
import { UpdateBackgroundTracker } from "./update-background-tracker";

const RELEASES_URL = "https://github.com/swarm-apps/SwarmDrop/releases";

export function UpdateHost() {
  if (Platform.OS !== "android") return null;
  return (
    <>
      <UpdateAvailableModal />
      <UpdateProgressModal />
      <UpdateErrorModal />
      <UpdateBackgroundTracker />
    </>
  );
}

function UpdateAvailableModal() {
  const { t } = useLingui();
  const status = useUpdateStore((s) => s.status);
  const latest = useUpdateStore((s) => s.latestVersion);
  const current = useUpdateStore((s) => s.currentVersion);
  const releaseNotes = useUpdateStore((s) => s.releaseNotes);
  const dismiss = useUpdateStore((s) => s.dismiss);
  const executeUpdate = useUpdateStore((s) => s.executeUpdate);

  const open = status === "available" || status === "force-required";
  const isForce = status === "force-required";
  const trimmedNotes = releaseNotes?.trim() ?? "";
  const hasNotes = trimmedNotes.length > 0;

  const openReleasePage = () => {
    const target = latest ? `${RELEASES_URL}/tag/v${latest}` : RELEASES_URL;
    Linking.openURL(target).catch((err) =>
      console.warn("[update] open release page failed:", err),
    );
  };

  if (!open) return null;

  return (
    <Modal
      animationType="fade"
      transparent
      visible={open}
      onRequestClose={isForce ? undefined : () => void dismiss()}
    >
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={styles.title}>
            {isForce ? t`必须更新` : t`发现新版本`}
          </Text>
          <Text style={styles.versionLine}>
            {isForce
              ? t`当前 v${current ?? ""} 已不再支持,请升级到 v${latest ?? ""}`
              : `v${latest ?? ""} · ${t`当前`} v${current ?? ""}`}
          </Text>

          {hasNotes ? (
            <ScrollView
              style={styles.notesScroll}
              contentContainerStyle={styles.notesContent}
              showsVerticalScrollIndicator
            >
              <Text style={styles.notesText}>{trimmedNotes}</Text>
            </ScrollView>
          ) : (
            <Pressable onPress={openReleasePage} style={styles.notesLink}>
              <Text style={styles.notesLinkText}>
                <Trans>查看更新内容</Trans>
              </Text>
              <ExternalLink color="#2563EB" size={14} />
            </Pressable>
          )}

          <View style={styles.actions}>
            <Pressable
              onPress={() => void executeUpdate()}
              style={styles.primaryButton}
            >
              <Download color="#FFFFFF" size={16} />
              <Text style={styles.primaryText}>
                <Trans>立即更新</Trans>
              </Text>
            </Pressable>
            {!isForce ? (
              <Pressable
                onPress={() => void dismiss()}
                style={styles.secondaryButton}
              >
                <Text style={styles.secondaryText}>
                  <Trans>稍后</Trans>
                </Text>
              </Pressable>
            ) : null}
          </View>
        </View>
      </View>
    </Modal>
  );
}

function UpdateProgressModal() {
  const status = useUpdateStore((s) => s.status);
  const progress = useUpdateStore((s) => s.progress);
  const backgrounded = useUpdateStore((s) => s.backgrounded);
  const backgroundDownload = useUpdateStore((s) => s.backgroundDownload);

  const open = status === "downloading" && !backgrounded;
  if (!open) return null;

  const percent = progress?.percent ?? 0;
  const downloaded = formatMB(progress?.downloaded ?? 0);
  const total = formatMB(progress?.total ?? 0);

  return (
    <Modal animationType="fade" transparent visible={open}>
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={styles.title}>
            <Trans>正在下载新版本</Trans>
          </Text>
          <Text style={styles.versionLine}>
            <Trans>下载完成后系统会引导你完成安装</Trans>
          </Text>

          <View style={styles.progressTrack}>
            <View style={[styles.progressFill, { width: `${percent}%` }]} />
          </View>
          <View style={styles.progressMeta}>
            <Text style={styles.progressMetaText}>{percent}%</Text>
            <Text style={styles.progressMetaText}>
              {downloaded} / {total} MB
            </Text>
          </View>

          <View style={styles.actions}>
            <Pressable
              onPress={backgroundDownload}
              style={styles.primaryButton}
            >
              <Text style={styles.primaryText}>
                <Trans>后台下载</Trans>
              </Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

function UpdateErrorModal() {
  const { t } = useLingui();
  const status = useUpdateStore((s) => s.status);
  const error = useUpdateStore((s) => s.error);
  const acknowledgeError = useUpdateStore((s) => s.acknowledgeError);
  const executeUpdate = useUpdateStore((s) => s.executeUpdate);

  const open = status === "error";
  if (!open) return null;

  const handleRetry = () => {
    acknowledgeError();
    void executeUpdate();
  };

  return (
    <Modal
      animationType="fade"
      transparent
      visible={open}
      onRequestClose={acknowledgeError}
    >
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <Text style={[styles.title, styles.errorTitle]}>
            <Trans>更新失败</Trans>
          </Text>
          <Text style={styles.versionLine}>
            {error ?? t`下载或安装过程中出错,请稍后重试。`}
          </Text>
          <View style={styles.actions}>
            <Pressable onPress={handleRetry} style={styles.primaryButton}>
              <RefreshCw color="#FFFFFF" size={16} />
              <Text style={styles.primaryText}>
                <Trans>重试</Trans>
              </Text>
            </Pressable>
            <Pressable
              onPress={acknowledgeError}
              style={styles.secondaryButton}
            >
              <Text style={styles.secondaryText}>
                <Trans>关闭</Trans>
              </Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

function formatMB(bytes: number): string {
  return (bytes / 1024 / 1024).toFixed(1);
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
  title: {
    color: "#0F172A",
    fontSize: 17,
    fontWeight: "700",
  },
  errorTitle: {
    color: "#B91C1C",
  },
  versionLine: {
    color: "#475569",
    fontSize: 13,
    lineHeight: 19,
  },
  notesScroll: {
    backgroundColor: "#F8FAFC",
    borderRadius: 10,
    maxHeight: 220,
  },
  notesContent: {
    padding: 12,
  },
  notesText: {
    color: "#0F172A",
    fontSize: 13,
    lineHeight: 19,
  },
  notesLink: {
    alignItems: "center",
    flexDirection: "row",
    gap: 6,
  },
  notesLinkText: {
    color: "#2563EB",
    fontSize: 13,
    textDecorationLine: "underline",
  },
  progressTrack: {
    backgroundColor: "#E2E8F0",
    borderRadius: 6,
    height: 8,
    overflow: "hidden",
  },
  progressFill: {
    backgroundColor: "#2563EB",
    height: "100%",
  },
  progressMeta: {
    flexDirection: "row",
    justifyContent: "space-between",
  },
  progressMetaText: {
    color: "#64748B",
    fontSize: 12,
  },
  actions: {
    gap: 10,
  },
  primaryButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    flexDirection: "row",
    gap: 6,
    justifyContent: "center",
    minHeight: 48,
  },
  primaryText: {
    color: "#FFFFFF",
    fontSize: 15,
    fontWeight: "700",
  },
  secondaryButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 48,
  },
  secondaryText: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
});
