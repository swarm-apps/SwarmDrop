import { Trans, useLingui } from "@lingui/react/macro";
import { useFocusEffect, useRouter } from "expo-router";
import {
  Archive,
  ArchiveRestore,
  FileArchive,
  FolderOpen,
  RefreshCw,
} from "lucide-react-native";
import { useCallback } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { MobileInboxContentKind } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  AppHeader,
  AppScreen,
  EmptyState,
  HeaderIconButton,
  Surface,
} from "@/components/mobile/screen";
import { formatBytes, formatRelativeTime } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { type InboxPreviewItem, useInboxStore } from "@/stores/inbox-store";

export default function InboxScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const {
    loading,
    includeArchived,
    items,
    action,
    refresh,
    repairMissingItems,
    setIncludeArchived,
  } = useInboxStore(
    useShallow((s) => ({
      loading: s.loading,
      includeArchived: s.includeArchived,
      items: s.items,
      action: s.action,
      refresh: s.refresh,
      repairMissingItems: s.repairMissingItems,
      setIncludeArchived: s.setIncludeArchived,
    })),
  );

  useFocusEffect(
    useCallback(() => {
      void refresh();
    }, [refresh]),
  );

  const openDetail = useCallback(
    (itemId: string) => {
      router.push({
        pathname: "/inbox/[itemId]",
        params: { itemId },
      } as never);
    },
    [router],
  );

  const repair = useCallback(async () => {
    try {
      const count = await repairMissingItems();
      toast.success(
        count > 0 ? t`已修复 ${count} 条收件箱记录` : t`收件箱已是最新`,
      );
    } catch (err) {
      toast.error(t`修复收件箱失败`, err);
    }
  }, [repairMissingItems, t]);

  return (
    <AppScreen scroll testID="inbox-screen" contentClassName="gap-5 pt-1">
      <AppHeader
        title={<Trans>收件箱</Trans>}
        subtitle={<Trans>已接收内容会在这里归档</Trans>}
        testID="inbox-header"
        right={
          <View className="flex-row gap-2">
            <HeaderIconButton
              icon={ArchiveRestore}
              label={t`修复收件箱`}
              onPress={repair}
              testID="inbox-repair-button"
            />
            <HeaderIconButton
              icon={RefreshCw}
              label={t`刷新`}
              onPress={refresh}
              testID="inbox-refresh-button"
            />
          </View>
        }
      />

      <Surface className="gap-3" testID="inbox-summary">
        <View className="flex-row items-center justify-between gap-3">
          <View className="min-w-0 flex-1">
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>内容索引</Trans>
            </Text>
            <Text className="mt-0.5 text-[11px] text-muted-foreground">
              <Trans>与传输活动分开保存</Trans>
            </Text>
          </View>
          {loading || action === "repair" ? (
            <ActivityIndicator color={colors.mutedForeground} />
          ) : (
            <Text className="text-[20px] font-semibold tabular-nums text-foreground">
              {items.length}
            </Text>
          )}
        </View>

        <Pressable
          accessibilityRole="switch"
          accessibilityState={{ checked: includeArchived }}
          onPress={() => setIncludeArchived(!includeArchived)}
          testID="inbox-include-archived-toggle"
          className="min-h-11 flex-row items-center justify-between gap-3 rounded-lg bg-muted px-3 active:opacity-70"
        >
          <View className="min-w-0 flex-1">
            <Text className="text-[12px] font-medium text-foreground">
              <Trans>显示已归档</Trans>
            </Text>
            <Text className="text-[11px] text-muted-foreground">
              <Trans>归档内容默认从主列表隐藏</Trans>
            </Text>
          </View>
          <View
            className={`h-6 w-11 justify-center rounded-full px-0.5 ${
              includeArchived ? "items-end bg-primary" : "items-start bg-border"
            }`}
          >
            <View className="size-5 rounded-full bg-background" />
          </View>
        </Pressable>
      </Surface>

      {loading && items.length === 0 ? (
        <View
          className="items-center justify-center gap-3 rounded-lg border border-border bg-card py-12"
          testID="inbox-loading-state"
        >
          <ActivityIndicator color={colors.mutedForeground} />
          <Text className="text-[12px] text-muted-foreground">
            <Trans>正在刷新收件箱</Trans>
          </Text>
        </View>
      ) : items.length === 0 ? (
        <EmptyState
          icon={FileArchive}
          title={<Trans>收件箱还是空的</Trans>}
          description={
            <Trans>
              完成的接收内容会在这里出现；传输过程和恢复入口在活动页查看。
            </Trans>
          }
          actionLabel={<Trans>刷新</Trans>}
          onAction={refresh}
          testID="inbox-empty-state"
        />
      ) : (
        <View className="gap-2" testID="inbox-list">
          {items.map((item, index) => (
            <InboxRow
              key={item.id}
              item={item}
              index={index}
              onPress={openDetail}
            />
          ))}
        </View>
      )}

      <View className="gap-2 rounded-lg bg-muted px-3.5 py-3">
        <Text className="text-[13px] font-semibold text-foreground">
          <Trans>内容边界</Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          <Trans>
            收件箱负责回答“我收到了什么”；活动页负责回答“传输发生了什么”。
          </Trans>
        </Text>
      </View>
    </AppScreen>
  );
}

function InboxRow({
  item,
  index,
  onPress,
}: {
  item: InboxPreviewItem;
  index: number;
  onPress: (itemId: string) => void;
}) {
  const colors = useThemeColors();
  const Icon =
    item.contentKind === MobileInboxContentKind.Files ? Archive : FolderOpen;
  const archived = item.archivedAt != null;
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => onPress(item.id)}
      testID={`inbox-row-${index}`}
      className="min-h-20 flex-row items-center gap-3 rounded-lg border border-border bg-card p-3 active:opacity-70"
    >
      <View className="size-11 items-center justify-center rounded-xl bg-muted">
        <Icon color={colors.foreground} size={19} />
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
        <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
          {item.sourceName}
          {" · "}
          {item.itemCount} <Trans>个文件</Trans>
          {" · "}
          {formatBytes(item.totalSize)}
        </Text>
        <Text className="text-[11px] text-muted-foreground">
          {formatRelativeTime(item.receivedAt)}
        </Text>
      </View>
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
