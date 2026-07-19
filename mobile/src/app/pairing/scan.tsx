import { Trans, useLingui } from "@lingui/react/macro";
import {
  type BarcodeScanningResult,
  CameraView,
  useCameraPermissions,
} from "expo-camera";
import { useRouter } from "expo-router";
import { ArrowLeft, Camera, ClipboardPaste } from "lucide-react-native";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  AppState,
  Linking,
  Pressable,
  StyleSheet,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";

/**
 * 扫码配对屏（受邀方）——用 expo-camera `CameraView` 扫另一台设备展示的邀请二维码，
 * 前缀校验通过后走 `previewInvite` 解码验签，成功即 `replace` 进确认页。
 *
 * 三态权限编排：未决 → primer（不冷启系统弹窗）；可再问 → 请求；已永久拒 → 去系统设置。
 * 扫描用 `lockRef` 一次性闸 + 失败去抖，避免同一帧连触发多次 `previewInvite`。
 */

/** 邀请串 KIND 前缀（`crates/invite/src/invite.rs:32`）。QR 编码为大写，解码大小写不敏感。 */
const INVITE_PREFIX = "sdinvite";

export default function ScanInvite() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const [permission, requestPermission, getPermission] = useCameraPermissions();
  const previewInvite = usePairingInviteStore((s) => s.previewInvite);

  // 一次性闸：命中有效邀请后锁住，直到导航离开或失败去抖解锁——防同一二维码连触发。
  const lockRef = useRef(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 回到前台时刷新权限读数：永久拒绝后引导用户去系统设置开启相机，返回 App 时
  // useCameraPermissions 不会自动重拉，不刷新就会卡在「去设置」态、明明已授权却进不去相机。
  useEffect(() => {
    const sub = AppState.addEventListener("change", (state) => {
      if (state === "active") void getPermission();
    });
    return () => sub.remove();
  }, [getPermission]);

  const handleScanned = useCallback(
    async (result: BarcodeScanningResult) => {
      if (lockRef.current) return;
      const raw = result.data.trim();
      // 前缀校验：只认 sdinvite 邀请串，其余二维码静默忽略、继续扫。
      if (!raw.toLowerCase().startsWith(INVITE_PREFIX)) return;

      lockRef.current = true;
      setWorking(true);
      setError(null);
      // QR 走 alphanumeric 模式整串大写，归一回小写规范形态：core 解码已大小写不敏感，
      // 这里保证 pending.invite 是规范形态，后续 consume 与之一致。
      const ok = await previewInvite(raw.toLowerCase());
      if (ok) {
        // replace 而非 push：扫码屏出栈卸载，CameraView 随之释放相机。
        router.replace({ pathname: "/pairing/found-device" });
      } else {
        setWorking(false);
        setError(t`邀请无效或已过期，请对准另一台设备的邀请二维码`);
        // 去抖 1.5s 再解锁，避免坏码高频重触发。
        setTimeout(() => {
          lockRef.current = false;
          setError(null);
        }, 1500);
      }
    },
    [previewInvite, router, t],
  );

  const goBack = () => router.back();

  // ── 权限未就绪（首帧 null）：转圈 ──
  if (permission === null) {
    return (
      <SafeAreaView
        style={{ flex: 1 }}
        className="items-center justify-center bg-background"
      >
        <ActivityIndicator color={colors.primary} />
      </SafeAreaView>
    );
  }

  // ── 未授权：primer（可再问）/ 去设置（已永久拒）──
  if (!permission.granted) {
    const canAsk = permission.canAskAgain;
    return (
      <SafeAreaView
        style={{ flex: 1 }}
        className="bg-background"
        edges={["top", "bottom"]}
      >
        <ScanHeader onBack={goBack} foreground={colors.foreground} />
        <View className="flex-1 items-center justify-center gap-4 px-8">
          <View className="size-[72px] items-center justify-center rounded-full bg-primary/10">
            <Camera color={colors.primary} size={36} />
          </View>
          <Text className="text-center text-[22px] font-extrabold text-foreground">
            <Trans>开启相机以扫码</Trans>
          </Text>
          <Text className="text-center text-sm leading-6 text-muted-foreground">
            {canAsk ? (
              <Trans>
                扫码需要访问相机。SwarmDrop 只用相机识别配对二维码，不拍照、不录像、不上传。
              </Trans>
            ) : (
              <Trans>
                相机权限已被关闭。请到系统设置里为 SwarmDrop 开启相机，再回来扫码。
              </Trans>
            )}
          </Text>
          <Pressable
            onPress={() =>
              canAsk ? void requestPermission() : void Linking.openSettings()
            }
            accessibilityRole="button"
            className="mt-2 h-12 w-full items-center justify-center rounded-xl bg-primary active:opacity-70"
          >
            <Text className="text-base font-semibold text-primary-foreground">
              {canAsk ? <Trans>允许相机权限</Trans> : <Trans>去系统设置开启</Trans>}
            </Text>
          </Pressable>
          <Pressable
            onPress={goBack}
            accessibilityRole="button"
            className="h-11 w-full flex-row items-center justify-center gap-1.5 active:opacity-70"
          >
            <ClipboardPaste color={colors.mutedForeground} size={16} />
            <Text className="text-sm font-medium text-muted-foreground">
              <Trans>返回</Trans>
            </Text>
          </Pressable>
        </View>
      </SafeAreaView>
    );
  }

  // ── 已授权：相机全屏 + 取景框 + 提示 ──
  return (
    <View style={{ flex: 1, backgroundColor: "#000" }}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={working ? undefined : (r) => void handleScanned(r)}
      />

      {/* 覆盖层：顶部返回、中央取景框、底部提示。相机上一律用白字 + 深色描边保证对比。 */}
      <SafeAreaView
        style={StyleSheet.absoluteFill}
        edges={["top", "bottom"]}
        pointerEvents="box-none"
      >
        <View className="flex-row px-4 pt-2">
          <Pressable
            onPress={goBack}
            hitSlop={12}
            accessibilityRole="button"
            accessibilityLabel={t`返回`}
            className="size-10 items-center justify-center rounded-full bg-black/40 active:opacity-70"
          >
            <ArrowLeft color="#ffffff" size={22} />
          </Pressable>
        </View>

        <View className="flex-1 items-center justify-center" pointerEvents="none">
          <View style={styles.reticle} />
          <Text style={styles.hint}>
            {working ? (
              <Trans>正在验证邀请…</Trans>
            ) : (
              <Trans>将配对二维码放入框内</Trans>
            )}
          </Text>
          {error !== null ? <Text style={styles.error}>{error}</Text> : null}
        </View>

        <View className="items-center px-6 pb-6">
          <Pressable
            onPress={goBack}
            accessibilityRole="button"
            className="h-11 flex-row items-center justify-center gap-1.5 rounded-full bg-black/40 px-5 active:opacity-70"
          >
            <ClipboardPaste color="#ffffff" size={16} />
            <Text className="text-sm font-semibold text-white">
              <Trans>返回</Trans>
            </Text>
          </Pressable>
        </View>
      </SafeAreaView>
    </View>
  );
}

/** 非相机态（primer/设置）复用的顶部返回条。 */
function ScanHeader({
  onBack,
  foreground,
}: {
  onBack: () => void;
  foreground: string;
}) {
  const { t } = useLingui();
  return (
    <View className="flex-row items-center px-4 pt-2">
      <Pressable
        onPress={onBack}
        hitSlop={12}
        accessibilityRole="button"
        accessibilityLabel={t`返回`}
        className="size-10 items-center justify-center active:opacity-70"
      >
        <ArrowLeft color={foreground} size={22} />
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  reticle: {
    width: 240,
    height: 240,
    borderRadius: 24,
    borderWidth: 3,
    borderColor: "#ffffff",
    backgroundColor: "transparent",
  },
  hint: {
    marginTop: 24,
    color: "#ffffff",
    fontSize: 15,
    fontWeight: "600",
    textShadowColor: "rgba(0,0,0,0.6)",
    textShadowRadius: 4,
  },
  error: {
    marginTop: 8,
    color: "#fecaca",
    fontSize: 13,
    fontWeight: "600",
    textAlign: "center",
    paddingHorizontal: 32,
    textShadowColor: "rgba(0,0,0,0.6)",
    textShadowRadius: 4,
  },
});
