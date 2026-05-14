import { Stack } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState } from "react";
import { ActivityIndicator, StyleSheet, Text, View } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { PairingRequestHost } from "@/components/pairing-request-host";
import { TransferOfferHost } from "@/components/transfer-offer-host";
import { initMobileCore } from "@/core/mobile-core";
import { waitForOnboardingHydration } from "@/stores/onboarding-store";

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
