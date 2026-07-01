import { Trans } from "@lingui/react/macro";
import {
  Archive,
  Bot,
  ChevronRight,
  ClipboardList,
  FileText,
  Image,
  Package,
  SlidersHorizontal,
  Video,
} from "lucide-react-native";
import { type ReactNode, useState } from "react";
import { Pressable, ScrollView, View } from "react-native";
import {
  MobileInboxContentKind,
  MobileInboxSourceKind,
} from "react-native-swarmdrop-core";
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

/** 单条记录是否满足某个筛选维度,供列表过滤与 FTS 命中结果的客户端二次筛选共用。 */
export function matchesInboxFilter(
  item: InboxPreviewItem,
  filter: InboxFilter,
): boolean {
  const archived = item.archivedAt != null;
  return (
    filter === "all" ||
    (filter === "files" && isFileLike(item)) ||
    (filter === "images" && isImageLike(item)) ||
    (filter === "videos" && isVideoLike(item)) ||
    (filter === "text" && isTextLike(item)) ||
    (filter === "attention" && item.missing) ||
    (filter === "archived" && archived)
  );
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
    if (!matchesInboxFilter(item, filter)) return false;
    if (!trimmedQuery) return true;
    const haystack = `${item.title} ${item.sourceName}`.toLowerCase();
    return haystack.includes(trimmedQuery);
  });
}

/** 主行最多展示的筛选 chip(≤4 个),其余次级筛选收进"更多筛选"入口。 */
const PRIMARY_FILTERS: readonly InboxFilter[] = [
  "all",
  "files",
  "images",
  "text",
];
/** 收进"更多筛选"的次级筛选:异常/已归档是救援性筛选,视频是文件的子集,三者使用频率都更低。 */
const SECONDARY_FILTERS: readonly InboxFilter[] = [
  "videos",
  "attention",
  "archived",
];

const FILTER_LABELS: Record<InboxFilter, ReactNode> = {
  all: <Trans>全部</Trans>,
  files: <Trans>文件</Trans>,
  images: <Trans>图片</Trans>,
  videos: <Trans>视频</Trans>,
  text: <Trans>文本</Trans>,
  attention: <Trans>异常</Trans>,
  archived: <Trans>已归档</Trans>,
};

export function FilterRail({
  value,
  counts,
  onChange,
}: {
  value: InboxFilter;
  counts: InboxFilterCounts;
  onChange: (value: InboxFilter) => void;
}) {
  const [showMore, setShowMore] = useState(false);
  const isSecondaryActive = SECONDARY_FILTERS.includes(value);
  // 若当前选中的是一个次级筛选(如通过搜索页带入),即使未手动展开也要保持可见,
  // 否则用户会看到一个"生效中但找不到"的筛选。
  const expanded = showMore || isSecondaryActive;

  const toggleMore = () => {
    if (expanded) {
      setShowMore(false);
      if (isSecondaryActive) onChange("all");
    } else {
      setShowMore(true);
    }
  };

  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      className="-mx-5 h-10 max-h-10"
      contentContainerClassName="gap-2 px-5"
      testID="inbox-filter-rail"
    >
      {PRIMARY_FILTERS.map((filter) => (
        <FilterChip
          key={filter}
          active={value === filter}
          label={FILTER_LABELS[filter]}
          count={counts[filter]}
          onPress={() => onChange(filter)}
        />
      ))}
      {expanded
        ? SECONDARY_FILTERS.map((filter) => (
            <FilterChip
              key={filter}
              active={value === filter}
              label={FILTER_LABELS[filter]}
              count={counts[filter]}
              onPress={() => onChange(filter)}
            />
          ))
        : null}
      <MoreFiltersChip expanded={expanded} onPress={toggleMore} />
    </ScrollView>
  );
}

function MoreFiltersChip({
  expanded,
  onPress,
}: {
  expanded: boolean;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ expanded }}
      testID="inbox-filter-more-button"
      className="h-9 min-w-16 flex-row items-center justify-center gap-1.5 rounded-xl border border-border bg-card px-3 active:opacity-70"
    >
      <SlidersHorizontal color={colors.mutedForeground} size={13} />
      <Text
        className="text-[12px] font-semibold text-foreground"
        numberOfLines={1}
      >
        {expanded ? <Trans>收起</Trans> : <Trans>更多筛选</Trans>}
      </Text>
    </Pressable>
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
  highlight,
}: {
  item: InboxPreviewItem;
  index: number;
  onPress: (itemId: string) => void;
  /** 命中关键词:非空时在标题/来源里高亮匹配子串(大小写不敏感)。 */
  highlight?: string;
}) {
  const colors = useThemeColors();
  const Icon = contentIcon(item);
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
          <HighlightedText
            text={item.title}
            query={highlight}
            className="min-w-0 flex-1 text-[14px] font-semibold text-foreground"
            numberOfLines={1}
          />
          <InboxStatusBadges item={item} />
        </View>
        <View className="flex-row items-center gap-1.5">
          <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
            {contentLabel(item)}
          </Text>
          <Text className="text-[11px] text-muted-foreground">·</Text>
          <HighlightedText
            text={item.sourceName}
            query={highlight}
            className="min-w-0 flex-1 text-[11px] text-muted-foreground"
            numberOfLines={1}
          />
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

/**
 * 缺失/已归档/AI 代理来源徽标——浏览态 InboxRow 与搜索命中态 InboxHitRow 共用同一份逻辑,
 * 保证同一条记录在两处渲染的状态一致(镜像 Requirement: Filter Consistency Across Search)。
 */
export function InboxStatusBadges({ item }: { item: InboxPreviewItem }) {
  const colors = useThemeColors();
  const archived = item.archivedAt != null;
  return (
    <>
      {item.missing || archived ? (
        <StatePill missing={item.missing} archived={archived} />
      ) : null}
      {item.sourceKind === MobileInboxSourceKind.Mcp ? (
        <View className="flex-row items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5">
          <Bot color={colors.primary} size={11} />
          <Text className="text-[10px] font-medium text-primary">
            <Trans>AI 代理</Trans>
          </Text>
        </View>
      ) : null}
    </>
  );
}

/** 在 text 中高亮匹配 query 的子串(大小写不敏感),query 为空时退化为普通文本。 */
export function HighlightedText({
  text,
  query,
  className,
  numberOfLines,
}: {
  text: string;
  query?: string;
  className?: string;
  numberOfLines?: number;
}) {
  const needle = query?.trim().toLowerCase();
  if (!needle) {
    return (
      <Text className={className} numberOfLines={numberOfLines}>
        {text}
      </Text>
    );
  }
  const lower = text.toLowerCase();
  const parts: ReactNode[] = [];
  let cursor = 0;
  let idx = lower.indexOf(needle);
  while (idx !== -1) {
    if (idx > cursor) parts.push(text.slice(cursor, idx));
    parts.push(
      <Text
        key={`${cursor}-${idx}`}
        className="rounded-sm bg-primary/25 font-semibold text-foreground"
      >
        {text.slice(idx, idx + needle.length)}
      </Text>,
    );
    cursor = idx + needle.length;
    idx = lower.indexOf(needle, cursor);
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return (
    <Text className={className} numberOfLines={numberOfLines}>
      {parts}
    </Text>
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
