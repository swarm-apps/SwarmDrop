import { Trans } from "@lingui/react/macro";
import { Tabs } from "expo-router";
import { Inbox, Settings, Smartphone } from "lucide-react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useThemeColors } from "@/hooks/useThemeColors";

export default function MainLayout() {
  const colors = useThemeColors();
  const insets = useSafeAreaInsets();

  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: colors.primary,
        tabBarInactiveTintColor: colors.mutedForeground,
        tabBarStyle: {
          backgroundColor: colors.card,
          borderTopColor: colors.border,
          // 叠加底部安全区，避免全面屏 home indicator 压住图标/文字。
          height: 64 + insets.bottom,
          paddingBottom: 8 + insets.bottom,
          paddingTop: 7,
        },
        tabBarLabelStyle: {
          fontSize: 11,
          fontWeight: "600",
        },
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: "Devices",
          tabBarLabel: () => <Trans>设备</Trans>,
          tabBarIcon: ({ color }) => <Smartphone color={color} size={20} />,
          tabBarButtonTestID: "tab-devices",
        }}
      />
      <Tabs.Screen
        name="inbox"
        options={{
          title: "Inbox",
          tabBarLabel: () => <Trans>收件箱</Trans>,
          tabBarIcon: ({ color }) => <Inbox color={color} size={20} />,
          tabBarButtonTestID: "tab-inbox",
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: "Settings",
          tabBarLabel: () => <Trans>设置</Trans>,
          tabBarIcon: ({ color }) => <Settings color={color} size={20} />,
          tabBarButtonTestID: "tab-settings",
        }}
      />
    </Tabs>
  );
}
