import { ChevronRight } from "lucide-react-native";
import { type PropsWithChildren, useState } from "react";
import { Pressable, Text, View } from "react-native";
import Animated, { FadeIn } from "react-native-reanimated";
import { useThemeColors } from "@/hooks/useThemeColors";

export function Collapsible({
  children,
  title,
}: PropsWithChildren & { title: string }) {
  const [isOpen, setIsOpen] = useState(false);
  const colors = useThemeColors();

  return (
    <View>
      <Pressable
        className="flex-row items-center gap-2 active:opacity-70"
        onPress={() => setIsOpen((value) => !value)}
      >
        <View className="w-8 h-8 rounded-lg bg-muted items-center justify-center">
          <ChevronRight
            size={14}
            color={colors.foreground}
            style={{ transform: [{ rotate: isOpen ? "90deg" : "0deg" }] }}
          />
        </View>
        <Text className="text-sm text-foreground">{title}</Text>
      </Pressable>
      {isOpen && (
        <Animated.View entering={FadeIn.duration(200)}>
          <View className="mt-3 ml-8 p-4 rounded-lg bg-muted">{children}</View>
        </Animated.View>
      )}
    </View>
  );
}
