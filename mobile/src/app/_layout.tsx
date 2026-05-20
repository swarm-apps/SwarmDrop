import "../global.css";

import { BottomSheetModalProvider } from "@gorhom/bottom-sheet";
import { ThemeProvider } from "@react-navigation/native";
import { PortalHost } from "@rn-primitives/portal";
import { Stack } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState } from "react";
import { ActivityIndicator, Text, useColorScheme, View } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { NotifierRoot } from "react-native-notifier";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { PairingRequestHost } from "@/components/pairing-request-host";
import { TransferOfferHost } from "@/components/transfer-offer-host";
import { UpdateHost } from "@/components/update-host";
import { initMobileCore } from "@/core/mobile-core";
import { useNavTheme } from "@/hooks/useThemeColors";
import { LinguiProvider } from "@/i18n/LinguiProvider";
import { initI18n } from "@/i18n/lingui";
import { restoreThemePreference } from "@/lib/theme-persistence";
import { waitForOnboardingHydration } from "@/stores/onboarding-store";
import { useUpdateStore } from "@/stores/update-store";

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

  // 升级检查:启动 2s 后首次检查 + AppState 回前台时再检查
  useEffect(() => {
    if (!ready || bootError !== null) return;
    const timer = setTimeout(() => {
      void useUpdateStore.getState().checkForUpdate();
    }, 2000);
    const teardown = useUpdateStore.getState().setupAppStateListener();
    return () => {
      clearTimeout(timer);
      teardown();
    };
  }, [ready, bootError]);

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
        <Text className="text-base font-bold text-destructive">启动失败</Text>
        <Text className="text-center text-sm text-muted-foreground">
          {bootError}
        </Text>
      </View>
    );
  }

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <KeyboardProvider>
        <SafeAreaProvider>
          <ThemeProvider value={navTheme}>
            <LinguiProvider>
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
                    name="settings"
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
            </LinguiProvider>
          </ThemeProvider>
          {/* NotifierRoot 放最后,iOS 走 RNScreens overlay 让 toast 浮在 modal 之上 */}
          <NotifierRoot useRNScreensOverlay />
        </SafeAreaProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}
