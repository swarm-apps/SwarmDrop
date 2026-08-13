import { Trans, useLingui } from "@lingui/react/macro";
import { lanHelperAddresses } from "@swarmdrop/shared-view";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { Copy, RadioTower } from "lucide-react-native";
import { Pressable, View } from "react-native";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";

export function LanHelperAddresses({
  addresses,
  peerId,
}: {
  addresses: string[];
  peerId?: string;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  // 筛选与排序全在 shared-view：此前这里是一份与桌面逐行同构的白名单三元链，
  // WebTransport 上线后被静默丢掉。返回顺序即推荐顺序，第一条最快。
  const dialAddresses = lanHelperAddresses(addresses, peerId);

  if (dialAddresses.length === 0) return null;

  const copyAddress = async (address: string) => {
    await Clipboard.setStringAsync(address);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`协助地址已复制`);
  };

  return (
    <View className="gap-2">
      <Text className="px-1 text-[12px] font-semibold uppercase tracking-wider text-muted-foreground">
        <Trans>局域网协助地址</Trans>
      </Text>
      <View className="overflow-hidden rounded-lg border border-border bg-card">
        <View className="flex-row items-start gap-3 border-b border-border px-3.5 py-3">
          <View className="size-8 items-center justify-center rounded-lg bg-primary/10">
            <RadioTower color={colors.primary} size={16} />
          </View>
          <View className="min-w-0 flex-1 gap-0.5">
            <Text className="text-[14px] font-medium text-foreground">
              <Trans>本机可拨号地址</Trans>
            </Text>
            <Text className="text-[12px] leading-4 text-muted-foreground">
              <Trans>复制后可供浏览器端快速连接本机。</Trans>
            </Text>
          </View>
        </View>
        {dialAddresses.map(({ address, transport }, index) => (
          <Pressable
            key={address}
            onPress={() => void copyAddress(address)}
            accessibilityRole="button"
            accessibilityLabel={t`复制协助地址`}
            // 传输名是带空格的专有名词（"WebRTC Direct"），testID 里转成 slug。
            testID={`lan-helper-address-${transport.toLowerCase().replace(/\s+/g, "-")}`}
            className={
              index === 0
                ? "flex-row items-center gap-3 px-3.5 py-3 active:bg-muted"
                : "flex-row items-center gap-3 border-t border-border px-3.5 py-3 active:bg-muted"
            }
          >
            <View className="min-w-0 flex-1 gap-1">
              <View className="self-start rounded-md bg-muted px-1.5 py-0.5">
                <Text className="text-[10px] font-medium text-muted-foreground">
                  {transport}
                </Text>
              </View>
              <Text className="font-mono text-[11px] leading-4 text-muted-foreground">
                {address}
              </Text>
            </View>
            <Copy color={colors.mutedForeground} size={15} />
          </Pressable>
        ))}
      </View>
    </View>
  );
}
