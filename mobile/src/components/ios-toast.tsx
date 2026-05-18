import { BlurView } from "expo-blur";
import { Check, CircleAlert, CircleX, Info } from "lucide-react-native";
import { useColorScheme, View, type ViewStyle } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Text } from "@/components/ui/text";

type AlertType = "success" | "info" | "error" | "warn";

interface Props {
  title?: string;
  description?: string;
  /** Spread from `showNotification({ componentProps })` by react-native-notifier. */
  alertType?: AlertType;
}

const ICON: Record<AlertType, { Icon: typeof Check; color: string }> = {
  success: { Icon: Check, color: "#34C759" },
  info: { Icon: Info, color: "#0A84FF" },
  error: { Icon: CircleX, color: "#FF3B30" },
  warn: { Icon: CircleAlert, color: "#FF9500" },
};

const SHADOW: ViewStyle = {
  alignSelf: "center",
  maxWidth: "85%",
  borderRadius: 999,
  shadowColor: "#000",
  shadowOpacity: 0.18,
  shadowOffset: { width: 0, height: 4 },
  shadowRadius: 16,
  elevation: 8,
};

// BlurView clips its blur to the borderRadius only when overflow is hidden;
// the shadow lives on the outer View so it isn't clipped along with the blur.
const PILL: ViewStyle = {
  flexDirection: "row",
  alignItems: "center",
  gap: 12,
  borderRadius: 999,
  overflow: "hidden",
  paddingHorizontal: 20,
  paddingVertical: 12,
};

/**
 * SPIndicator-style capsule toast for react-native-notifier. iOS gets real
 * UIKit blur via `BlurView`; Android falls back to BlurView's translucent
 * tint (close enough to the SPIndicator look without paying for
 * `experimentalBlurMethod` runtime cost).
 *
 * Layout + alpha background go via inline style — react-native-css under
 * NativeWind v5 preview silently drops alpha colors and arbitrary layout
 * utilities. See dev-notes/knowledge/theme-and-styling.md.
 */
export function IosToast({ title, description, alertType = "info" }: Props) {
  const isDark = useColorScheme() === "dark";
  const { top } = useSafeAreaInsets();
  const { Icon, color } = ICON[alertType];

  return (
    <View style={[SHADOW, { marginTop: top + 8 }]}>
      <BlurView intensity={80} tint={isDark ? "dark" : "light"} style={PILL}>
        <Icon color={color} size={22} />
        <View style={{ flexShrink: 1 }}>
          {title ? (
            <Text
              className="text-foreground text-sm font-semibold"
              numberOfLines={1}
            >
              {title}
            </Text>
          ) : null}
          {description ? (
            <Text className="text-muted-foreground text-xs" numberOfLines={2}>
              {description}
            </Text>
          ) : null}
        </View>
      </BlurView>
    </View>
  );
}
