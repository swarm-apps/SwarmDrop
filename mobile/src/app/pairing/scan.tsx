import { Trans, useLingui } from "@lingui/react/macro";
import {
  type BarcodeScanningResult,
  CameraView,
  useCameraPermissions,
} from "expo-camera";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import {
  ArrowLeft,
  Camera,
  ClipboardPaste,
  Flashlight,
  FlashlightOff,
} from "lucide-react-native";
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
import { cn } from "@/lib/utils";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";

/**
 * 扫码配对屏（受邀方）——用 expo-camera `CameraView` 扫另一台设备展示的邀请二维码，
 * 前缀校验通过后走 `previewInvite` 解码验签，成功即 `replace` 进确认页。
 *
 * 三态权限编排：未决 → primer（不冷启系统弹窗）；可再问 → 请求；已永久拒 → 去系统设置。
 * 扫描用 `lockRef` 一次性闸 + 失败去抖，避免同一帧连触发多次 `previewInvite`。
 *
 * 取景是「压暗四周 + 四角标记」而不是一个白框：暗场遮罩把注意力和相机对焦都推到中央，
 * 四角在浅色背景（白纸/白屏上的二维码）下也不会糊成一片。低光是扫码失败的头号原因，
 * 所以手电筒是常驻控件，不藏在二级菜单里。
 */

/** 邀请串 KIND 前缀（`crates/invite/src/invite.rs:32`）。QR 编码为大写，解码大小写不敏感。 */
const INVITE_PREFIX = "sdinvite";

/** 取景窗边长（pt）。四角标记与遮罩挖洞共用，改这一个值即可。 */
const RETICLE_SIZE = 248;

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
  const [torch, setTorch] = useState(false);

  // 回到前台时刷新权限读数：永久拒绝后引导用户去系统设置开启相机，返回 App 时
  // useCameraPermissions 不会自动重拉，不刷新就会卡在「去设置」态、明明已授权却进不去相机。
  useEffect(() => {
    const sub = AppState.addEventListener("change", (state) => {
      if (state === "active") void getPermission();
    });
    return () => sub.remove();
  }, [getPermission]);

  /** 扫码与粘贴共用的一条通路：加锁 → 验签 → 成功跳确认页 / 失败去抖解锁。 */
  const consumeInvite = useCallback(
    async (raw: string, invalidHint: string) => {
      lockRef.current = true;
      setWorking(true);
      setError(null);
      // QR 走 alphanumeric 模式整串大写，归一回小写规范形态：core 解码已大小写不敏感，
      // 这里保证 pending.invite 是规范形态，后续 consume 与之一致。
      const ok = await previewInvite(raw.toLowerCase());
      if (ok) {
        Haptics.notificationAsync(
          Haptics.NotificationFeedbackType.Success,
        ).catch(() => {});
        // replace 而非 push：扫码屏出栈卸载，CameraView 随之释放相机。
        router.replace({ pathname: "/pairing/found-device" });
        return;
      }
      setWorking(false);
      setError(invalidHint);
      // 去抖 1.5s 再解锁，避免坏码高频重触发。
      setTimeout(() => {
        lockRef.current = false;
        setError(null);
      }, 1500);
    },
    [previewInvite, router],
  );

  const handleScanned = useCallback(
    async (result: BarcodeScanningResult) => {
      if (lockRef.current) return;
      const raw = result.data.trim();
      // 前缀校验：只认 sdinvite 邀请串，其余二维码静默忽略、继续扫。
      if (!raw.toLowerCase().startsWith(INVITE_PREFIX)) return;
      await consumeInvite(
        raw,
        t`邀请无效或已过期，请对准另一台设备的邀请二维码`,
      );
    },
    [consumeInvite, t],
  );

  // 对方把邀请以文本发过来时（微信/邮件），不必回上一屏再找输入框。
  const handlePaste = useCallback(async () => {
    if (lockRef.current) return;
    const clip = (await Clipboard.getStringAsync()).trim();
    if (!clip.toLowerCase().startsWith(INVITE_PREFIX)) {
      setError(t`剪贴板里没有配对邀请`);
      setTimeout(() => setError(null), 2000);
      return;
    }
    await consumeInvite(clip, t`剪贴板里的邀请无效或已过期`);
  }, [consumeInvite, t]);

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
                扫码需要访问相机。SwarmDrop
                只用相机识别配对二维码，不拍照、不录像、不上传。
              </Trans>
            ) : (
              <Trans>
                相机权限已被关闭。请到系统设置里为 SwarmDrop
                开启相机，再回来扫码。
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
              {canAsk ? (
                <Trans>允许相机权限</Trans>
              ) : (
                <Trans>去系统设置开启</Trans>
              )}
            </Text>
          </Pressable>
          {/* 没有相机也仍然配得上：粘贴是同等一档的备用通路，不是「返回」 */}
          <Pressable
            onPress={() => void handlePaste()}
            accessibilityRole="button"
            className="min-h-11 w-full flex-row items-center justify-center gap-1.5 active:opacity-70"
          >
            <ClipboardPaste color={colors.mutedForeground} size={16} />
            <Text className="text-sm font-medium text-muted-foreground">
              <Trans>改用粘贴邀请</Trans>
            </Text>
          </Pressable>
          {error !== null ? (
            <Text className="text-center text-[13px] text-destructive-ink">
              {error}
            </Text>
          ) : null}
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
        enableTorch={torch}
        barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
        onBarcodeScanned={working ? undefined : (r) => void handleScanned(r)}
      />

      {/* 压暗四周、留出中央取景窗；纯视觉层，不吃触摸 */}
      <View style={StyleSheet.absoluteFill} pointerEvents="none">
        <View style={styles.scrim} />
        <View className="flex-row">
          <View style={styles.scrim} />
          <View style={styles.reticle}>
            <ReticleCorner corner="tl" active={working} />
            <ReticleCorner corner="tr" active={working} />
            <ReticleCorner corner="bl" active={working} />
            <ReticleCorner corner="br" active={working} />
            {working ? (
              <View className="flex-1 items-center justify-center">
                <ActivityIndicator color="#ffffff" size="large" />
              </View>
            ) : null}
          </View>
          <View style={styles.scrim} />
        </View>
        {/* 提示紧贴取景窗下沿：与遮罩同层，位置随取景窗走，不靠猜垂直间距 */}
        <View style={styles.scrim} className="items-center pt-6">
          <Text style={styles.hint}>
            {working ? (
              <Trans>正在验证邀请…</Trans>
            ) : (
              <Trans>将配对二维码放入框内</Trans>
            )}
          </Text>
          {error !== null ? <Text style={styles.error}>{error}</Text> : null}
        </View>
      </View>

      {/* 覆盖层：顶部返回/手电筒、中央提示、底部粘贴入口。相机上一律用白字 + 深色描边保证对比。 */}
      <SafeAreaView
        style={StyleSheet.absoluteFill}
        edges={["top", "bottom"]}
        pointerEvents="box-none"
      >
        <View className="flex-row items-center justify-between px-4 pt-2">
          <Pressable
            onPress={goBack}
            hitSlop={12}
            accessibilityRole="button"
            accessibilityLabel={t`返回`}
            className="size-11 items-center justify-center rounded-full bg-black/40 active:opacity-70"
          >
            <ArrowLeft color="#ffffff" size={22} />
          </Pressable>
          <Pressable
            onPress={() => setTorch((on) => !on)}
            hitSlop={12}
            accessibilityRole="button"
            accessibilityLabel={torch ? t`关闭手电筒` : t`打开手电筒`}
            accessibilityState={{ selected: torch }}
            testID="scan-torch-toggle"
            className={cn(
              "size-11 items-center justify-center rounded-full active:opacity-70",
              torch ? "bg-white" : "bg-black/40",
            )}
          >
            {torch ? (
              <Flashlight color="#0a0a0a" size={20} />
            ) : (
              <FlashlightOff color="#ffffff" size={20} />
            )}
          </Pressable>
        </View>

        <View className="flex-1" pointerEvents="none" />

        <View className="items-center px-6 pb-6">
          <Pressable
            onPress={() => void handlePaste()}
            disabled={working}
            accessibilityRole="button"
            testID="scan-paste-invite-button"
            className="min-h-11 flex-row items-center justify-center gap-1.5 rounded-full bg-black/40 px-5 active:opacity-70 disabled:opacity-50"
          >
            <ClipboardPaste color="#ffffff" size={16} />
            <Text className="text-sm font-semibold text-white">
              <Trans>粘贴邀请</Trans>
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
        className="size-11 items-center justify-center active:opacity-70"
      >
        <ArrowLeft color={foreground} size={22} />
      </Pressable>
    </View>
  );
}

/** 取景窗的一个角：两条边 + 外圆角，命中验证时加粗提示「抓到了」。 */
function ReticleCorner({
  corner,
  active,
}: {
  corner: "tl" | "tr" | "bl" | "br";
  active: boolean;
}) {
  return (
    <View
      style={[
        CORNER_SHAPE[corner],
        { borderColor: active ? "#ffffff" : "rgba(255,255,255,0.92)" },
      ]}
    />
  );
}

/** 四个角的静态几何——模块级注册一次，避免每帧为 4 个角各建 5 个样式对象。 */
const CORNER_SHAPE = StyleSheet.create({
  tl: {
    position: "absolute",
    top: 0,
    left: 0,
    width: 34,
    height: 34,
    borderTopWidth: 4,
    borderLeftWidth: 4,
    borderTopLeftRadius: 14,
  },
  tr: {
    position: "absolute",
    top: 0,
    right: 0,
    width: 34,
    height: 34,
    borderTopWidth: 4,
    borderRightWidth: 4,
    borderTopRightRadius: 14,
  },
  bl: {
    position: "absolute",
    bottom: 0,
    left: 0,
    width: 34,
    height: 34,
    borderBottomWidth: 4,
    borderLeftWidth: 4,
    borderBottomLeftRadius: 14,
  },
  br: {
    position: "absolute",
    bottom: 0,
    right: 0,
    width: 34,
    height: 34,
    borderBottomWidth: 4,
    borderRightWidth: 4,
    borderBottomRightRadius: 14,
  },
});

const styles = StyleSheet.create({
  scrim: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.45)",
  },
  reticle: {
    width: RETICLE_SIZE,
    height: RETICLE_SIZE,
  },
  hint: {
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
