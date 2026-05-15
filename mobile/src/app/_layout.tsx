import { Stack } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { StatusBar } from "expo-status-bar";
import { useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  AppState,
  type AppStateStatus,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { PairingRequestHost } from "@/components/pairing-request-host";
import { TransferOfferHost } from "@/components/transfer-offer-host";
import { initMobileCore } from "@/core/mobile-core";
import { waitForOnboardingHydration } from "@/stores/onboarding-store";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

SplashScreen.preventAutoHideAsync().catch(() => {});

export default function RootLayout() {
  const [ready, setReady] = useState(false);
  const [bootError, setBootError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        await Promise.all([waitForOnboardingHydration(), initMobileCore()]);
      } catch (err) {
        console.error("[boot] init failed:", err);
        setBootError(err instanceof Error ? err.message : String(err));
      } finally {
        setReady(true);
        SplashScreen.hideAsync().catch(() => {});
      }
    })();
  }, []);

  // AppState lifecycle：进后台关 NetManager，回前台重启
  // 避免后台 → 前台后留下僵尸 NetManager / socket。
  // 仅在 ready 后挂载（boot 期 NetManager 还没建好就监听会乱）。
  const wasRunningBeforeBackgroundRef = useRef(false);
  useEffect(() => {
    if (!ready || bootError !== null) return;
    const sub = AppState.addEventListener("change", (next: AppStateStatus) => {
      const { runtimeState, shutdownNode, startNode } = useMobileCoreStore.getState();
      if (next === "background" || next === "inactive") {
        if (runtimeState === "running") {
          wasRunningBeforeBackgroundRef.current = true;
          shutdownNode().catch((err) =>
            console.warn("[lifecycle] shutdownNode on background failed:", err),
          );
        }
      } else if (next === "active") {
        if (wasRunningBeforeBackgroundRef.current && runtimeState === "stopped") {
          wasRunningBeforeBackgroundRef.current = false;
          startNode().catch((err) =>
            console.warn("[lifecycle] startNode on foreground failed:", err),
          );
        }
      }
    });
    return () => sub.remove();
  }, [ready, bootError]);

  if (!ready) {
    return (
      <View style={styles.splash}>
        <ActivityIndicator color="#2563EB" size="large" />
      </View>
    );
  }

  if (bootError !== null) {
    return (
      <View style={styles.splash}>
        <Text style={styles.errorTitle}>启动失败</Text>
        <Text style={styles.errorBody}>{bootError}</Text>
      </View>
    );
  }

  return (
    <GestureHandlerRootView style={styles.flex}>
      <SafeAreaProvider>
        <StatusBar style="auto" />
        <Stack screenOptions={{ headerShown: false }}>
          <Stack.Screen name="index" />
          <Stack.Screen name="onboarding" />
          <Stack.Screen name="(main)" />
          <Stack.Screen
            name="pairing/input-code"
            options={{ animation: "slide_from_right" }}
          />
          <Stack.Screen
            name="pairing/found-device"
            options={{ animation: "slide_from_right" }}
          />
          <Stack.Screen
            name="pairing/success"
            options={{ animation: "slide_from_right", gestureEnabled: false }}
          />
          <Stack.Screen
            name="send/select-device"
            options={{ animation: "slide_from_right" }}
          />
        </Stack>
        <PairingRequestHost />
        <TransferOfferHost />
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}

const styles = StyleSheet.create({
  flex: {
    flex: 1,
  },
  splash: {
    alignItems: "center",
    backgroundColor: "#F8FAFC",
    flex: 1,
    gap: 12,
    justifyContent: "center",
    padding: 24,
  },
  errorTitle: {
    color: "#B91C1C",
    fontSize: 18,
    fontWeight: "700",
  },
  errorBody: {
    color: "#475569",
    fontSize: 13,
    textAlign: "center",
  },
});
