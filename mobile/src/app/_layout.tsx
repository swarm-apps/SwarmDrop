import "../global.css";

import { BottomSheetModalProvider } from "@gorhom/bottom-sheet";
import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { PortalHost } from "@rn-primitives/portal";
import { Stack, useRouter } from "expo-router";
import { ThemeProvider } from "expo-router/react-navigation";
import { ShareIntentProvider, useShareIntentContext } from "expo-share-intent";
import * as SplashScreen from "expo-splash-screen";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState } from "react";
import { ActivityIndicator, Text, useColorScheme, View } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { ReducedMotionConfig, ReduceMotion } from "react-native-reanimated";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { PairingRequestHost } from "@/components/pairing-request-host";
import { TransferOfferHost } from "@/components/transfer-offer-host";
import { UpdateHost } from "@/components/update-host";
import { UpdateProvider } from "@/components/update-provider";
import { initMobileCore } from "@/core/mobile-core";
import { initNotifications } from "@/core/notifications";
import {
  subscribePendingInvite,
  takePendingInvite,
} from "@/core/pending-deep-link";
import { shareFilesToTransferFiles } from "@/core/share-intent";
import { useNavTheme } from "@/hooks/useThemeColors";
import { LinguiProvider } from "@/i18n/LinguiProvider";
import { i18n, initI18n } from "@/i18n/lingui";
import { restoreThemePreference } from "@/lib/theme-persistence";
import { toast } from "@/lib/toast";
import {
  useOnboardingStore,
  waitForOnboardingHydration,
} from "@/stores/onboarding-store";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";
import { useShareStore } from "@/stores/share-store";

SplashScreen.preventAutoHideAsync().catch(() => {});

const BOOT_FAILED_TITLE = msg`启动失败`;

export default function RootLayout() {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === "dark";
  const navTheme = useNavTheme();
  const [ready, setReady] = useState(false);
  const [bootError, setBootError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        await Promise.all([
          restoreThemePreference(),
          waitForOnboardingHydration(),
          initMobileCore(),
          initI18n(),
        ]);
        // 通知系统初始化(前台服务 runner + 前后台事件 + 冷启动初始通知)。
        // 放在 core 就绪后,保证 action 事件里能安全调 getMobileCore()。
        initNotifications();
      } catch (err) {
        console.error("[boot] init failed:", err);
        setBootError(err instanceof Error ? err.message : String(err));
      } finally {
        setReady(true);
        SplashScreen.hideAsync().catch(() => {});
      }
    })();
  }, []);

  // 节点开关由用户在 NodeControlSheet 控制,不再随 AppState 自动 shutdown/start —
  // 文件选择器等瞬间退台会反复重建 NetManager 打断传输,且 iOS/Android 后台本身
  // 就会挂起 socket。长传保活留给后续 Foreground Service / BGTask。

  // 升级检查现由 <UpdateProvider>（registry-rn / SwarmHive 引擎）负责：
  // checkOnMount 启动即查、recheckOnFocus 回前台（AppState active）再查（engine 内部节流）。

  if (!ready) {
    return (
      <View className="flex-1 items-center justify-center bg-background">
        <ActivityIndicator />
      </View>
    );
  }

  if (bootError !== null) {
    return (
      <View className="flex-1 items-center justify-center gap-3 bg-background p-6">
        <Text className="text-base font-bold text-destructive-ink">
          {i18n._(BOOT_FAILED_TITLE)}
        </Text>
        <Text className="text-center text-sm text-muted-foreground">
          {bootError}
        </Text>
      </View>
    );
  }

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      {/* 所有 Reanimated 动画尊重系统「减弱动效」设置(无障碍),显式固定为 System。 */}
      <ReducedMotionConfig mode={ReduceMotion.System} />
      <KeyboardProvider>
        <SafeAreaProvider>
          <ThemeProvider value={navTheme}>
            <LinguiProvider>
              {/* SwarmHive 更新引擎（dogfood server）；engine 装配后再渲染子树。 */}
              <UpdateProvider
                baseUrl="http://47.115.172.218:3030"
                appSlug="swarmdrop-rn"
              >
                <ShareIntentProvider options={{ debug: __DEV__ }}>
                  <BottomSheetModalProvider>
                    <StatusBar style={isDark ? "light" : "dark"} />
                    <Stack screenOptions={{ headerShown: false }}>
                      <Stack.Screen name="index" />
                      <Stack.Screen name="onboarding" />
                      <Stack.Screen name="(main)" />
                      <Stack.Screen
                        name="transfer"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="activity"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="inbox/search"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="inbox/[itemId]"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="settings"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="device/[peerId]"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="device/groups"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="pairing/scan"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="pairing/found-device"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="pairing/success"
                        options={{
                          animation: "slide_from_right",
                          gestureEnabled: false,
                        }}
                      />
                      <Stack.Screen
                        name="send/select-device"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="send/share-target"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen
                        name="send/shared-files"
                        options={{ animation: "slide_from_right" }}
                      />
                      <Stack.Screen name="e2e/file-browser" />
                    </Stack>
                    <PairingRequestHost />
                    <TransferOfferHost />
                    <UpdateHost />
                    <PortalHost />
                    {/* 入站分享(expo-share-intent):映射文件 → 选设备屏。命令式,无常驻 UI。 */}
                    <ShareIntentHandler />
                    {/* 配对深链:`+native-intent.tsx` 放下的邀请 → 确认卡。同样无常驻 UI。 */}
                    <DeepLinkInviteHandler />
                  </BottomSheetModalProvider>
                </ShareIntentProvider>
              </UpdateProvider>
            </LinguiProvider>
          </ThemeProvider>
          {/* toast 走 burnt(命令式原生:iOS SPIndicator / Android ToastAndroid),无需宿主组件 */}
        </SafeAreaProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}

/**
 * 入站分享处理:收到系统分享 → 映射成 TransferFile[] → 塞进 share-store → 跳选设备屏。
 * - 无文件(纯文本 / URL 分享)→ 提示 v1 只支持文件,放弃本次。
 * - 未过引导 → 提示先完成设置,放弃本次(v1 不暂存)。
 * 仅在 App ready 后渲染(RootLayout 的 !ready 早返回),故此处不再重复 ready 门控。
 */
function ShareIntentHandler() {
  const { isReady, hasShareIntent, shareIntent, resetShareIntent } =
    useShareIntentContext();
  const router = useRouter();
  const { t } = useLingui();
  const hasOnboarded = useOnboardingStore((s) => s.hasOnboarded);
  const setSharedFiles = useShareStore((s) => s.setSharedFiles);

  useEffect(() => {
    if (!isReady || !hasShareIntent) return;
    const files = shareFilesToTransferFiles(shareIntent.files);
    if (files.length === 0) {
      toast.info(t`暂只支持发送文件、图片和视频`);
      resetShareIntent();
      return;
    }
    if (!hasOnboarded) {
      toast.info(t`请先完成 SwarmDrop 设置`);
      resetShareIntent();
      return;
    }
    setSharedFiles(files);
    router.push("/send/share-target" as never);
    resetShareIntent();
  }, [
    isReady,
    hasShareIntent,
    shareIntent,
    hasOnboarded,
    router,
    setSharedFiles,
    resetShareIntent,
    t,
  ]);

  return null;
}

/**
 * 配对深链处理：`+native-intent.tsx` 放下的邀请 → 解码验签 → 确认卡。
 *
 * 与扫码/粘贴走的是**同一条**安全闸（`previewInvite` 成功才进 `/pairing/found-device`，
 * 由用户看着对端设备名与指纹确认）—— 深链只是又一个入口，不是一条捷径。
 *
 * 冷启动与热启动都要覆盖，所以是「先订阅、再取一次」：
 * - 冷启动时 `redirectSystemPath` 早在 React 之前就放下了负载，mount 后 take 得到；
 * - 热启动时它在 handler 已 mount 之后放下，只能靠订阅拿到。
 * 反过来（先 take 再订阅）会在两者之间漏掉一条。
 *
 * 未过引导时不能进配对流（还没有设备身份），提示后**丢弃**——与分享一致，v1 不暂存。
 */
function DeepLinkInviteHandler() {
  const router = useRouter();
  const { t } = useLingui();
  const hasOnboarded = useOnboardingStore((s) => s.hasOnboarded);
  const previewInvite = usePairingInviteStore((s) => s.previewInvite);

  useEffect(() => {
    const handle = async () => {
      const invite = takePendingInvite();
      if (invite === null) return;
      if (!hasOnboarded) {
        toast.info(t`请先完成 SwarmDrop 设置`);
        return;
      }
      // 原样交给 core：canonical 载体整串大小写不敏感，归一统一在那侧做。
      const ok = await previewInvite(invite);
      if (ok) {
        router.push({ pathname: "/pairing/found-device" });
        return;
      }
      const reject = usePairingInviteStore.getState().previewReject;
      toast.error(
        reject === "self"
          ? t`这是你自己的邀请`
          : reject === "expired"
            ? t`邀请已过期，请让对方重新生成`
            : t`邀请无效或已被使用`,
      );
    };

    const unsubscribe = subscribePendingInvite(() => {
      void handle();
    });
    void handle();
    return unsubscribe;
  }, [hasOnboarded, previewInvite, router, t]);

  return null;
}
