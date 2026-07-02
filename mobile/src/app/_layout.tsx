import "../global.css";

import { BottomSheetModalProvider } from "@gorhom/bottom-sheet";
import { PortalHost } from "@rn-primitives/portal";
import { Stack } from "expo-router";
import { ThemeProvider } from "expo-router/react-navigation";
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
import { useNavTheme } from "@/hooks/useThemeColors";
import { LinguiProvider } from "@/i18n/LinguiProvider";
import { initI18n } from "@/i18n/lingui";
import { restoreThemePreference } from "@/lib/theme-persistence";
import { waitForOnboardingHydration } from "@/stores/onboarding-store";

SplashScreen.preventAutoHideAsync().catch(() => {});

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
          启动失败
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
                  </Stack>
                  <PairingRequestHost />
                  <TransferOfferHost />
                  <UpdateHost />
                  <PortalHost />
                </BottomSheetModalProvider>
              </UpdateProvider>
            </LinguiProvider>
          </ThemeProvider>
          {/* toast 走 burnt(命令式原生:iOS SPIndicator / Android ToastAndroid),无需宿主组件 */}
        </SafeAreaProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}
