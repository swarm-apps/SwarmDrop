import { Trans } from "@lingui/react/macro";
import { Archive, FileArchive, FolderOpen } from "lucide-react-native";
import { useCallback, useEffect } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { AppHeader, AppScreen, EmptyState } from "@/components/mobile/screen";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { type InboxPreviewItem, useInboxStore } from "@/stores/inbox-store";

export default function InboxScreen() {
  const colors = useThemeColors();
  const loading = useInboxStore((s) => s.loading);
  const items = useInboxStore((s) => s.items);
  const refresh = useInboxStore((s) => s.refresh);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleRefresh = useCallback(() => {
    void refresh();
  }, [refresh]);

  return (
    <AppScreen scroll testID="inbox-screen" contentClassName="gap-5 pt-1">
      <AppHeader
        title={<Trans>收件箱</Trans>}
        subtitle={<Trans>已接收内容会在这里归档</Trans>}
        testID="inbox-header"
      />

      {loading ? (
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
          onAction={handleRefresh}
          testID="inbox-empty-state"
        />
      ) : (
        <View className="gap-2" testID="inbox-list">
          {items.map((item) => (
            <InboxRow key={item.id} item={item} />
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

function InboxRow({ item }: { item: InboxPreviewItem }) {
  const colors = useThemeColors();
  const Icon = item.kind === "folder" ? FolderOpen : Archive;
  return (
    <Pressable
      accessibilityRole="button"
      className="min-h-16 flex-row items-center gap-3 rounded-lg border border-border bg-card p-3 active:opacity-70"
    >
      <View className="size-10 items-center justify-center rounded-xl bg-muted">
        <Icon color={colors.foreground} size={18} />
      </View>
      <View className="min-w-0 flex-1 gap-0.5">
        <Text
          className="text-[14px] font-semibold text-foreground"
          numberOfLines={1}
        >
          {item.title}
        </Text>
        <Text className="text-[11px] text-muted-foreground" numberOfLines={1}>
          {item.sourceName}
        </Text>
      </View>
      {item.missing || item.archived ? (
        <Text className="text-[11px] text-muted-foreground">
          {item.missing ? <Trans>缺失</Trans> : <Trans>已归档</Trans>}
        </Text>
      ) : null}
    </Pressable>
  );
}
