import { Trans } from "@lingui/react/macro";
import {
  Archive,
  ChevronRight,
  ClipboardList,
  FileText,
  Image,
  Package,
  Video,
} from "lucide-react-native";
import type { ReactNode } from "react";
import { Pressable, ScrollView, View } from "react-native";
import { MobileInboxContentKind } from "react-native-swarmdrop-core";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import type { InboxPreviewItem } from "@/stores/inbox-store";

export type InboxFilter =
  | "all"
  | "files"
  | "images"
  | "videos"
  | "text"
  | "attention"
  | "archived";

export type InboxFilterCounts = Record<InboxFilter, number>;

export function getInboxFilterCounts(
  items: InboxPreviewItem[],
): InboxFilterCounts {
  return {
    all: items.length,
    files: items.filter(isFileLike).length,
    images: items.filter(isImageLike).length,
    videos: items.filter(isVideoLike).length,
    text: items.filter(isTextLike).length,
    attention: items.filter((item) => item.missing).length,
    archived: items.filter((item) => item.archivedAt != null).length,
  };
}

export function filterInboxItems(
  items: InboxPreviewItem[],
  {
    filter,
    query = "",
  }: {
    filter: InboxFilter;
    query?: string;
  },
): InboxPreviewItem[] {
  const trimmedQuery = query.trim().toLowerCase();
  return items.filter((item) => {
    const archived = item.archivedAt != null;
    const matchesFilter =
      filter === "all" ||
      (filter === "files" && isFileLike(item)) ||
      (filter === "images" && isImageLike(item)) ||
      (filter === "videos" && isVideoLike(item)) ||
      (filter === "text" && isTextLike(item)) ||
      (filter === "attention" && item.missing) ||
      (filter === "archived" && archived);
    if (!matchesFilter) return false;
    if (!trimmedQuery) return true;
    const haystack = `${item.title} ${item.sourceName}`.toLowerCase();
    return haystack.includes(trimmedQuery);
  });
}

export function FilterRail({
  value,
  counts,
  onChange,
}: {
  value: InboxFilter;
  counts: InboxFilterCounts;
  onChange: (value: InboxFilter) => void;
}) {
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      className="-mx-5 h-10 max-h-10"
      contentContainerClassName="gap-2 px-5"
      testID="inbox-filter-rail"
    >
      <FilterChip
        active={value === "all"}
        label={<Trans>全部</Trans>}
        count={counts.all}
        onPress={() => onChange("all")}
      />
      <FilterChip
        active={value === "files"}
        label={<Trans>文件</Trans>}
        count={counts.files}
        onPress={() => onChange("files")}
      />
      <FilterChip
        active={value === "images"}
        label={<Trans>图片</Trans>}
        count={counts.images}
        onPress={() => onChange("images")}
      />
      <FilterChip
        active={value === "videos"}
        label={<Trans>视频</Trans>}
        count={counts.videos}
        onPress={() => onChange("videos")}
      />
      <FilterChip
        active={value === "text"}
        label={<Trans>文本</Trans>}
        count={counts.text}
        onPress={() => onChange("text")}
      />
      <FilterChip
        active={value === "attention"}
        label={<Trans>异常</Trans>}
        count={counts.attention}
        onPress={() => onChange("attention")}
      />
      <FilterChip
        active={value === "archived"}
        label={<Trans>已归档</Trans>}
        count={counts.archived}
        onPress={() => onChange("archived")}
      />
    </ScrollView>
  );
}

function FilterChip({
  active,
  label,
  count,
  onPress,
}: {
  active: boolean;
  label: ReactNode;
  count: number;
  onPress: () => void;
}) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ selected: active }}
      className={cn(
        "h-9 min-w-16 items-center justify-center rounded-xl border px-3 active:opacity-70",
        active ? "border-primary bg-primary" : "border-border bg-card",
      )}
    >
      <Text
        className={cn(
          "text-[12px] font-semibold",
          active ? "text-primary-foreground" : "text-foreground",
        )}
        numberOfLines={1}
      >
        {label} {count}
      </Text>
    </Pressable>
  );
}

export function InboxRow({
  item,
  index,
  onPress,
}: {
  item: InboxPreviewItem;
  index: number;
  onPress: (itemId: string) => void;
}) {
  const colors = useThemeColors();
  const Icon = contentIcon(item);
  const archived = item.archivedAt != null;
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => onPress(item.id)}
      testID={`inbox-row-${index}`}
      className="min-h-20 flex-row items-center gap-3 rounded-lg border border-border bg-card p-3.5 active:bg-muted/50"
    >
      <View className="size-12 items-center justify-center rounded-xl bg-primary/10">
        <Icon color={colors.primary} size={20} />
      </View>
      <View className="min-w-0 flex-1 gap-1">
        <View className="flex-row items-center gap-2">
          <Text
            className="min-w-0 flex-1 text-[14px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {item.title}
          </Text>
          {item.missing || archived ? (
            <StatePill missing={item.missing} archived={archived} />
          ) : null}
        </View>
        <View className="flex-row items-center gap-1.5">
          <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
            {contentLabel(item)}
          </Text>
          <Text className="text-[11px] text-muted-foreground">·</Text>
          <Text
            className="min-w-0 flex-1 text-[11px] text-muted-foreground"
            numberOfLines={1}
          >
            {item.sourceName}
          </Text>
        </View>
        <View className="flex-row items-center gap-1.5">
          <Text className="text-[11px] text-muted-foreground">
            {item.itemCount} <Trans>项</Trans>
          </Text>
          <Text className="text-[11px] text-muted-foreground">·</Text>
          <Text className="text-[11px] text-muted-foreground">
            {formatBytes(item.totalSize)}
          </Text>
          <Text className="text-[11px] text-muted-foreground">·</Text>
          <Text className="text-[11px] text-muted-foreground">
            {formatRelativeTime(item.receivedAt)}
          </Text>
        </View>
      </View>
      <ChevronRight color={colors.mutedForeground} size={17} />
    </Pressable>
  );
}

function StatePill({
  missing,
  archived,
}: {
  missing: boolean;
  archived: boolean;
}) {
  return (
    <View
      className={
        missing
          ? "rounded-full bg-destructive/15 px-2 py-0.5"
          : "rounded-full bg-muted px-2 py-0.5"
      }
    >
      <Text
        className={
          missing
            ? "text-[10px] font-medium text-destructive"
            : "text-[10px] font-medium text-muted-foreground"
        }
      >
        {missing ? (
          <Trans>缺失</Trans>
        ) : archived ? (
          <Trans>已归档</Trans>
        ) : null}
      </Text>
    </View>
  );
}

function isFileLike(item: InboxPreviewItem): boolean {
  return (
    item.contentKind === MobileInboxContentKind.Files ||
    item.contentKind === MobileInboxContentKind.Bundle
  );
}

function isTextLike(item: InboxPreviewItem): boolean {
  return (
    item.contentKind === MobileInboxContentKind.Text ||
    item.contentKind === MobileInboxContentKind.Clipboard
  );
}

function isImageLike(item: InboxPreviewItem): boolean {
  return hasAnyExtension(item.title, IMAGE_EXTENSIONS);
}

function isVideoLike(item: InboxPreviewItem): boolean {
  return hasAnyExtension(item.title, VIDEO_EXTENSIONS);
}

const IMAGE_EXTENSIONS = [
  ".jpg",
  ".jpeg",
  ".png",
  ".gif",
  ".webp",
  ".heic",
  ".heif",
  ".bmp",
  ".tiff",
  ".avif",
];

const VIDEO_EXTENSIONS = [
  ".mp4",
  ".mov",
  ".m4v",
  ".webm",
  ".mkv",
  ".avi",
  ".wmv",
  ".flv",
  ".3gp",
];

function hasAnyExtension(title: string, extensions: string[]): boolean {
  const lower = title.toLowerCase();
  return extensions.some((extension) => lower.endsWith(extension));
}

function contentIcon(item: InboxPreviewItem) {
  if (isImageLike(item)) return Image;
  if (isVideoLike(item)) return Video;
  switch (item.contentKind) {
    case MobileInboxContentKind.Text:
      return FileText;
    case MobileInboxContentKind.Clipboard:
      return ClipboardList;
    case MobileInboxContentKind.Bundle:
      return Package;
    default:
      return Archive;
  }
}

function contentLabel(item: InboxPreviewItem) {
  if (isImageLike(item)) return <Trans>图片</Trans>;
  if (isVideoLike(item)) return <Trans>视频</Trans>;
  switch (item.contentKind) {
    case MobileInboxContentKind.Text:
      return <Trans>文本</Trans>;
    case MobileInboxContentKind.Clipboard:
      return <Trans>剪贴板</Trans>;
    case MobileInboxContentKind.Bundle:
      return <Trans>合集</Trans>;
    default:
      return <Trans>文件</Trans>;
  }
}
