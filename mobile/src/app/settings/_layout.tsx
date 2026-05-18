import { Stack } from "expo-router";

export default function SettingsLayout() {
  return (
    <Stack
      screenOptions={{ headerShown: false, animation: "slide_from_right" }}
    >
      <Stack.Screen name="index" options={{ animation: "none" }} />
      <Stack.Screen name="general" />
      <Stack.Screen name="language" />
      <Stack.Screen name="theme" />
      <Stack.Screen name="network" />
      <Stack.Screen name="bootstrap-nodes" />
      <Stack.Screen name="about" />
    </Stack>
  );
}
