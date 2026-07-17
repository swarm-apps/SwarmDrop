import { useLingui } from "@lingui/react/macro";
import { Image } from "expo-image";
import { Check, Play, RotateCcw, Share2, X } from "lucide-react-native";
import { memo, useCallback, useState } from "react";
import { type GestureResponderEvent, Pressable, View } from "react-native";
import { formatBytes, ProgressBar } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import { fileBrowserIcon } from "./file-icon";
import { isVideoFile } from "./media-type";
import type { FileBrowserActions, FileBrowserItem } from "./types";
import { useFileThumbnail } from "./use-file-thumbnail";

interface FileCardProps {
  item: FileBrowserItem;
  actions?: FileBrowserActions;
  testID?: string;
}

// expo-image 的 <Image> 默认不吃 nativewind className,直接用 style 铺满 cell。
const FILL_STYLE = { width: "100%", height: "100%" } as const;

function FileCardComponent({ item, actions, testID }: FileCardProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const Icon = fileBrowserIcon(item.name);
  const thumbnailUri = useFileThumbnail(item);
  const [failedUri, setFailedUri] = useState<string | null>(null);
  const showPreview = Boolean(thumbnailUri) && failedUri !== thumbnailUri;
  const showVideoBadge = showPreview && isVideoFile(item.name);
  const openable = Boolean(actions?.openItem) && item.status !== "missing";

  const open = useCallback(() => {
    if (openable) actions?.openItem?.(item);
  }, [actions, item, openable]);
  const remove = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.removeItem?.(item);
    },
    [actions, item],
  );
  const retry = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.retryItem?.(item);
    },
    [actions, item],
  );
  const share = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.shareItem?.(item);
    },
    [actions, item],
  );

  const progress = Math.round(item.progress ?? 0);
  return (
    <View className="m-1 min-h-40 flex-1 overflow-hidden rounded-lg border border-border bg-card">
      <Pressable
        onPress={open}
        disabled={!openable}
        accessibilityRole={openable ? "button" : undefined}
        accessibilityLabel={t`${item.name}，${formatBytes(item.size)}`}
        testID={testID}
        className="flex-1 active:opacity-70"
      >
        <View className="h-24 items-center justify-center bg-muted">
          {showPreview ? (
            <Image
              source={{ uri: thumbnailUri }}
              style={FILL_STYLE}
              contentFit="cover"
              recyclingKey={item.id}
              cachePolicy="memory-disk"
              transition={0}
              onError={() => setFailedUri(thumbnailUri ?? null)}
            />
          ) : (
            <Icon size={32} color={colors.mutedForeground} />
          )}
          {showVideoBadge ? (
            <View className="absolute inset-0 items-center justify-center">
              <View className="size-8 items-center justify-center rounded-full bg-black/45">
                <Play size={16} color="#FFFFFF" fill="#FFFFFF" />
              </View>
            </View>
          ) : null}
          {item.status === "completed" ? (
            <View className="absolute right-2 top-2 rounded-full bg-card/90 p-1.5">
              <Check size={14} color={colors.success} />
            </View>
          ) : null}
        </View>
        <View className="gap-1 p-3">
          <Text className="text-[13px] font-semibold" numberOfLines={2}>
            {item.name}
          </Text>
          <Text className="text-[12px] text-muted-foreground">
            {formatBytes(item.size)}
          </Text>
          {item.status === "transferring" || item.status === "paused" ? (
            <ProgressBar
              percent={progress}
              heightClass="h-1"
              fillClass={item.status === "paused" ? "bg-warning" : "bg-primary"}
            />
          ) : null}
        </View>
      </Pressable>
      <View className="absolute right-1 top-24 flex-row">
        {actions?.shareItem && item.status !== "missing" ? (
          <Pressable
            onPress={share}
            accessibilityRole="button"
            accessibilityLabel={t`分享 ${item.name}`}
            className="size-11 items-center justify-center rounded-xl"
          >
            <Share2 size={15} color={colors.mutedForeground} />
          </Pressable>
        ) : null}
        {item.status === "error" && actions?.retryItem ? (
          <Pressable
            onPress={retry}
            accessibilityRole="button"
            accessibilityLabel={t`重试 ${item.name}`}
            className="size-11 items-center justify-center rounded-xl"
          >
            <RotateCcw size={15} color={colors.destructive} />
          </Pressable>
        ) : null}
        {actions?.removeItem ? (
          <Pressable
            onPress={remove}
            accessibilityRole="button"
            accessibilityLabel={t`移除 ${item.name}`}
            className={cn(
              "size-11 items-center justify-center rounded-xl",
              item.status === "error" ? "ml-0" : "ml-auto",
            )}
          >
            <X size={15} color={colors.mutedForeground} />
          </Pressable>
        ) : null}
      </View>
    </View>
  );
}

export const FileCard = memo(FileCardComponent);
