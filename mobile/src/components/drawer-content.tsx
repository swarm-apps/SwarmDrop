import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
// SDK 56：@react-navigation/drawer 的 DrawerContentComponentProps 通过 expo-router
// 的 drawer 子模块重新暴露；expo-router 没用 package.json "exports"，deep path 可访问。
import type { DrawerContentComponentProps } from "expo-router/build/react-navigation/drawer";
import { DrawerActions } from "expo-router/react-navigation";
import {
  History,
  Info,
  type LucideIcon,
  Settings as SettingsIcon,
} from "lucide-react-native";
import { useRef } from "react";
import { Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useShallow } from "zustand/react/shallow";
import {
  NodeControlSheet,
  type NodeControlSheetRef,
} from "@/components/node-control-sheet";
import { StatusPill } from "@/components/status-pill";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { truncateMiddle } from "@/lib/utils";
import { useMobileCoreStore } from "@/stores/mobile-core-store";

interface NavItem {
  href: string;
  icon: LucideIcon;
  label: React.ReactNode;
}

/**
 * Drawer 只在主屏拉出来,所有项都 router.push 到 root Stack 的同级路由。
 * 不在 Drawer 内做 active 切换 —— 模型是"主屏 + 全局入口面板",不是 tab-like 切换。
 */
const NAV_ITEMS = (): NavItem[] => [
  { href: "/transfer", icon: History, label: <Trans>传输历史</Trans> },
  { href: "/settings", icon: SettingsIcon, label: <Trans>设置</Trans> },
  { href: "/settings/about", icon: Info, label: <Trans>关于</Trans> },
];

export function DrawerContent(props: DrawerContentComponentProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const router = useRouter();
  const { runtimeState, peerId, identityStatus } = useMobileCoreStore(
    useShallow((s) => ({
      runtimeState: s.runtimeState,
      peerId: s.peerId,
      identityStatus: s.identityStatus,
    })),
  );

  const identityLabel =
    identityStatus === "loading"
      ? t`正在初始化身份...`
      : identityStatus === "failed"
        ? t`身份加载失败`
        : identityStatus === "ready" && peerId
          ? t`已加载身份 ${peerId.slice(0, 12)}...`
          : t`等待初始化`;

  const nodeSheetRef = useRef<NodeControlSheetRef>(null);

  const closeDrawer = () =>
    props.navigation.dispatch(DrawerActions.closeDrawer());

  const handleStatusPress = () => nodeSheetRef.current?.present();

  const handleCopyPeerId = async () => {
    if (!peerId) return;
    await Clipboard.setStringAsync(peerId);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制 PeerID`);
  };

  const handleNavPress = (item: NavItem) => {
    closeDrawer();
    router.push(item.href as never);
  };

  const navItems = NAV_ITEMS();

  return (
    <SafeAreaView
      style={{ flex: 1 }}
      className="bg-background"
      edges={["top", "bottom"]}
    >
      <View className="px-5 pt-2 pb-4">
        <StatusPill
          state={runtimeState}
          onPress={handleStatusPress}
          size="md"
        />
        <Text className="mt-2 text-xs text-muted-foreground" numberOfLines={1}>
          {identityLabel}
        </Text>
      </View>

      <View className="h-px bg-border" />

      <View className="flex-1 px-2 pt-2">
        {navItems.map((item) => {
          const Icon = item.icon;
          return (
            <Pressable
              key={item.href}
              onPress={() => handleNavPress(item)}
              accessibilityRole="button"
              className="flex-row items-center gap-3 rounded-lg px-3 py-2.5 active:bg-muted"
            >
              <Icon color={colors.foreground} size={18} />
              <Text className="text-[15px] text-foreground">{item.label}</Text>
            </Pressable>
          );
        })}
      </View>

      <View className="h-px bg-border" />

      <View className="px-5 py-4 gap-1">
        <Text className="text-[11px] text-muted-foreground">
          <Trans>本机</Trans>
        </Text>
        <Pressable
          onLongPress={handleCopyPeerId}
          hitSlop={4}
          accessibilityRole="button"
        >
          <Text className="text-xs font-mono text-foreground" numberOfLines={1}>
            {peerId ? truncateMiddle(peerId, 12, 8) : "—"}
          </Text>
        </Pressable>
      </View>

      <NodeControlSheet ref={nodeSheetRef} />
    </SafeAreaView>
  );
}
