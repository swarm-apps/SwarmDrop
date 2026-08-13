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
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, useColorScheme, View } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { ReducedMotionConfig, ReduceMotion } from "react-native-reanimated";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { AppErrorBoundary } from "@/components/app-error-boundary";
import { PairingRequestHost } from "@/components/pairing-request-host";
import { TransferOfferHost } from "@/components/transfer-offer-host";
import { UpdateHost } from "@/components/update-host";
import { UpdateProvider } from "@/components/update-provider";
import { initMobileCore } from "@/core/mobile-core";
import { initNotifications } from "@/core/notifications";
import { useIsOnboardingComplete } from "@/core/onboarding-flow";
import { PREVIEW_REJECT_MESSAGE } from "@/core/pairing-labels";
import {
  subscribePendingInvite,
  takePendingInvite,
} from "@/core/pending-deep-link";
import { useReceiveLocationWatch } from "@/core/receive-location";
import { shareFilesToTransferFiles } from "@/core/share-intent";
import { useNavTheme } from "@/hooks/useThemeColors";
import { LinguiProvider } from "@/i18n/LinguiProvider";
import { i18n, initI18n } from "@/i18n/lingui";
import { getErrorMessage } from "@/lib/errors";
import { restoreThemePreference } from "@/lib/theme-persistence";
import { toast } from "@/lib/toast";
import { usePairingInviteStore } from "@/stores/pairing-invite-store";
import { waitForPreferencesHydration } from "@/stores/preferences-store";
import { useShareStore } from "@/stores/share-store";

SplashScreen.preventAutoHideAsync().catch(() => {});

/**
 * 全 App 的 JS 异常兜底。expo-router 用 `<Try catch={ErrorBoundary}>` 包住**本 layout 组件
 * 外面**,所以它接得住 RootLayout 子树里任何渲染期/effect 期的抛错 —— 在此之前这类异常
 * 一路冒到 RN fatal handler,release build 上就是闪退(见 knowledge/toolchain.md 的两起)。
 * 也正因为包在外面,boundary 渲染时下面这些 Provider 都还不存在,故它零 Provider 依赖。
 */
export { AppErrorBoundary as ErrorBoundary };

const BOOT_FAILED_TITLE = msg`启动失败`;

export default function RootLayout() {
  const colorScheme = useColorScheme();
  const isDark = colorScheme === "dark";
  const navTheme = useNavTheme();
  const [ready, setReady] = useState(false);
  const [bootError, setBootError] = useState<Error | null>(null);
  const [bootMessage, setBootMessage] = useState<string | null>(null);

  // 抽成具名函数是为了让启动失败屏的「重试」能直接再调一次,而不是靠一个
  // 只用来当触发器的假依赖(`bootAttempt`)—— 那种写法 effect body 里根本读不到它。
  // 各步都能安全重入:initMobileCore 只在构造成功后才缓存 promise、
  // initNotifications 有 initialized 标志、initI18n 无缓存。
  const runBoot = useCallback(async () => {
    setBootError(null);
    setBootMessage(null);
    setReady(false);
    try {
      await Promise.all([
        restoreThemePreference(),
        // 引导守卫的判据全在偏好里（设备名、接收目录），水合前它们是初始值——
        // 不等它，一个配置齐全的用户会被 `<Redirect>` 一次性扔进引导流程。
        waitForPreferencesHydration(),
        initMobileCore(),
        initI18n(),
      ]);
      // 通知系统初始化(前台服务 runner + 前后台事件 + 冷启动初始通知)。
      // 放在 core 就绪后,保证 action 事件里能安全调 getMobileCore()。
      initNotifications();
    } catch (err) {
      console.error("[boot] init failed:", err);
      // 两件都要:原始 Error 进错误屏的详情框(带 stack,供反馈截图),
      // getErrorMessage 的**本地化**文案进正文——错误屏详情框里那串是 Rust 侧写的中文
      // 技术描述,`lib/errors.ts` 明文规定它不能当用户文案(英文界面会原样露出中文)。
      setBootError(err instanceof Error ? err : new Error(String(err)));
      setBootMessage(getErrorMessage(err));
    } finally {
      setReady(true);
      SplashScreen.hideAsync().catch(() => {});
    }
  }, []);

  useEffect(() => {
    void runBoot();
  }, [runBoot]);

  // 落点失效探活：回前台时重探一次，让设置页与引导判据也能看见「目录没了」，
  // 而不是拖到下一次接受传输才发现。
  useReceiveLocationWatch();

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

  // 启动失败复用同一张错误屏(只换标题):此前这里是另一版自绘的两行文字,没有重试、
  // 没有错误详情——同一类「App 用不了了」的处境,用户却会看到两种毫不相干的界面,
  // 而且往后给错误屏加的任何改进(比如「复制详情」)都会漏掉启动这条路径。
  if (bootError !== null) {
    return (
      <AppErrorBoundary
        error={bootError}
        title={i18n._(BOOT_FAILED_TITLE)}
        description={bootMessage ?? undefined}
        retry={runBoot}
      />
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
  const onboarded = useIsOnboardingComplete();
  const setSharedFiles = useShareStore((s) => s.setSharedFiles);

  useEffect(() => {
    if (!isReady || !hasShareIntent) return;
    const files = shareFilesToTransferFiles(shareIntent.files);
    if (files.length === 0) {
      toast.info(t`暂只支持发送文件、图片和视频`);
      resetShareIntent();
      return;
    }
    if (!onboarded) {
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
    onboarded,
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
  const onboarded = useIsOnboardingComplete();
  const previewInvite = usePairingInviteStore((s) => s.previewInvite);

  useEffect(() => {
    const handle = async () => {
      const invite = takePendingInvite();
      if (invite === null) return;
      if (!onboarded) {
        toast.info(t`请先完成 SwarmDrop 设置`);
        return;
      }
      // 原样交给 core：canonical 载体整串大小写不敏感，归一统一在那侧做。
      const outcome = await previewInvite(invite);
      if (outcome === "ok") {
        router.push({ pathname: "/pairing/found-device" });
        return;
      }
      toast.error(t(PREVIEW_REJECT_MESSAGE[outcome]));
    };

    const unsubscribe = subscribePendingInvite(() => {
      void handle();
    });
    void handle();
    return unsubscribe;
  }, [onboarded, previewInvite, router, t]);

  return null;
}
