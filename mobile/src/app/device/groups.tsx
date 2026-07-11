import { Plural, Trans, useLingui } from "@lingui/react/macro";
import * as Haptics from "expo-haptics";
import { GripVertical, Plus, Tags, Trash2 } from "lucide-react-native";
import { useCallback, useMemo, useRef, useState } from "react";
import { Pressable, View } from "react-native";
import { Swipeable } from "react-native-gesture-handler";
import { KeyboardStickyView } from "react-native-keyboard-controller";
import ReorderableList, {
  type ReorderableListReorderEvent,
  reorderItems,
  useIsActive,
  useReorderableDrag,
} from "react-native-reorderable-list";
import {
  SafeAreaView,
  useSafeAreaInsets,
} from "react-native-safe-area-context";
import { useShallow } from "zustand/react/shallow";
import { EmptyState } from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import {
  type DeviceGroup,
  sortedDeviceGroups,
} from "@/lib/device-organization";
import { useMobileCoreStore } from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 设备分组管理 —— 独立页面(替代此前的 bottom sheet)。
 *
 * - 拖拽排序:长按右侧手柄拖动(react-native-reorderable-list,Reanimated 驱动)。
 * - 重命名:点分组名就地变输入框(受控),切到别的行 / 收键盘时提交。编辑时手柄禁用,
 *   避免拖拽 remount 丢失未提交的名字。
 * - 删除:左滑露出删除,二次确认(不解除组内设备的配对)。
 * - 新建:底部固定输入栏,支持连续创建;键盘弹出时随 KeyboardStickyView 贴合键盘。
 *
 * 组内成员仍在「设备详情 → 别名与分组」里勾选;本页只负责分组本身的 CRUD + 排序,
 * 每行右侧展示只读的设备数(与实际配对设备取交集,和首页分组筛选一致)。
 * 别名与分组仅存本机,不同步给对端。
 */
export default function DeviceGroupsScreen() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const insets = useSafeAreaInsets();

  const {
    organization,
    createDeviceGroup,
    renameDeviceGroup,
    deleteDeviceGroup,
    reorderDeviceGroups,
  } = usePreferencesStore(
    useShallow((s) => ({
      organization: s.deviceOrganization,
      createDeviceGroup: s.createDeviceGroup,
      renameDeviceGroup: s.renameDeviceGroup,
      deleteDeviceGroup: s.deleteDeviceGroup,
      reorderDeviceGroups: s.reorderDeviceGroups,
    })),
  );
  const pairedDevicesCache = useMobileCoreStore((s) => s.pairedDevicesCache);

  const [newGroup, setNewGroup] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  const groups = useMemo(
    () => sortedDeviceGroups(organization),
    [organization],
  );

  // 设备数与实际配对设备取交集(残留 membership 不计入),和首页分组筛选口径一致。
  const pairedPeerIds = useMemo(
    () => new Set(pairedDevicesCache.map((summary) => summary.peerId)),
    [pairedDevicesCache],
  );
  const deviceCountOf = useCallback(
    (groupId: string) =>
      (organization.groupDeviceIds[groupId] ?? []).filter((id) =>
        pairedPeerIds.has(id),
      ).length,
    [organization.groupDeviceIds, pairedPeerIds],
  );

  const handleReorder = useCallback(
    ({ from, to }: ReorderableListReorderEvent) => {
      if (from === to) return;
      reorderDeviceGroups(
        reorderItems(
          groups.map((group) => group.id),
          from,
          to,
        ),
      );
    },
    [groups, reorderDeviceGroups],
  );

  const handleCreate = useCallback(() => {
    if (createDeviceGroup(newGroup)) setNewGroup("");
  }, [createDeviceGroup, newGroup]);

  // 受控内联重命名:切到别的行前先提交当前编辑,避免焦点 TextInput unmount 时
  // onEndEditing 不可靠导致丢失。draft 存在页面级 editingName,不依赖行的生命周期。
  const startEdit = useCallback(
    (id: string, name: string) => {
      setEditingId((current) => {
        if (current && current !== id) renameDeviceGroup(current, editingName);
        return id;
      });
      setEditingName(name);
    },
    [editingName, renameDeviceGroup],
  );
  const commitEdit = useCallback(() => {
    setEditingId((current) => {
      if (current) renameDeviceGroup(current, editingName);
      return null;
    });
  }, [editingName, renameDeviceGroup]);

  const pendingDeleteName = pendingDelete
    ? (organization.groups.find((group) => group.id === pendingDelete)?.name ??
      "")
    : "";

  const renderItem = useCallback(
    ({ item }: { item: DeviceGroup }) => (
      <GroupRow
        group={item}
        deviceCount={deviceCountOf(item.id)}
        editing={editingId === item.id}
        editingName={editingName}
        onChangeEditingName={setEditingName}
        onStartEdit={() => startEdit(item.id, item.name)}
        onCommitEdit={commitEdit}
        onRequestDelete={() => setPendingDelete(item.id)}
      />
    ),
    [deviceCountOf, editingId, editingName, startEdit, commitEdit],
  );

  const trimmedNew = newGroup.trim();

  return (
    <SafeAreaView edges={["top"]} style={{ flex: 1 }} className="bg-background">
      <SettingsHeader title={t`设备分组`} />

      <View style={{ flex: 1 }}>
        {groups.length === 0 ? (
          <View className="flex-1 justify-center px-5">
            <EmptyState
              icon={Tags}
              title={<Trans>还没有分组</Trans>}
              description={
                <Trans>创建分组来整理你的设备,例如「家里」「工作」</Trans>
              }
              testID="device-groups-empty"
            />
          </View>
        ) : (
          <ReorderableList
            data={groups}
            onReorder={handleReorder}
            shouldUpdateActiveItem
            automaticallyAdjustKeyboardInsets
            renderItem={renderItem}
            keyExtractor={(item) => item.id}
            ListHeaderComponent={
              <Text className="px-1 pb-2 text-[12px] text-muted-foreground">
                <Trans>长按手柄拖动排序 · 左滑删除</Trans>
              </Text>
            }
            contentContainerStyle={{
              paddingHorizontal: 20,
              paddingTop: 8,
              paddingBottom: 24,
            }}
            keyboardShouldPersistTaps="handled"
          />
        )}
      </View>

      {/* 底部固定新建栏 —— 键盘弹出时随 KeyboardStickyView 贴合键盘;键盘收起时留安全区。 */}
      <KeyboardStickyView>
        <View
          className="border-t border-border bg-background px-5 pt-3"
          style={{ paddingBottom: Math.max(insets.bottom, 12) }}
        >
          <View className="flex-row items-center gap-2">
            <Input
              value={newGroup}
              onChangeText={setNewGroup}
              placeholder={t`新分组名称`}
              placeholderTextColor={colors.mutedForeground}
              testID="device-groups-new-group-input"
              onSubmitEditing={handleCreate}
              returnKeyType="done"
              className="h-11 flex-1 rounded-lg bg-card px-3.5 text-[14px]"
            />
            <Pressable
              onPress={handleCreate}
              disabled={trimmedNew.length === 0}
              accessibilityRole="button"
              accessibilityLabel={t`创建分组`}
              testID="device-groups-create-button"
              className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
            >
              <Plus
                color={
                  trimmedNew.length === 0
                    ? colors.mutedForeground
                    : colors.primaryForeground
                }
                size={18}
              />
            </Pressable>
          </View>
        </View>
      </KeyboardStickyView>

      <ConfirmDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
        title={<Trans>删除分组</Trans>}
        description={
          pendingDeleteName ? (
            <Trans>
              删除「{pendingDeleteName}
              」后,组内设备将保留为已配对状态,并显示在未分组列表中。
            </Trans>
          ) : (
            <Trans>分组内设备将保留为已配对状态,并显示在未分组列表中。</Trans>
          )
        }
        actionLabel={<Trans>删除</Trans>}
        destructive
        onAction={() => {
          if (pendingDelete) deleteDeviceGroup(pendingDelete);
          setPendingDelete(null);
        }}
        contentTestID="device-group-delete-dialog"
        actionTestID="device-group-delete-confirm-button"
      />
    </SafeAreaView>
  );
}

/* ─────────────── 单个分组行 ─────────────── */

interface GroupRowProps {
  group: DeviceGroup;
  deviceCount: number;
  editing: boolean;
  editingName: string;
  onChangeEditingName: (name: string) => void;
  onStartEdit: () => void;
  onCommitEdit: () => void;
  onRequestDelete: () => void;
}

function GroupRow({
  group,
  deviceCount,
  editing,
  editingName,
  onChangeEditingName,
  onStartEdit,
  onCommitEdit,
  onRequestDelete,
}: GroupRowProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const drag = useReorderableDrag();
  const isActive = useIsActive();
  const swipeableRef = useRef<Swipeable>(null);

  // 长按手柄:触觉反馈(放 JS 线程,避开 worklet 限制)+ 激活拖拽。
  // active 抬起用 className 切换(shouldUpdateActiveItem 驱动),不引入自定义 worklet。
  const handleDrag = useCallback(() => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    drag();
  }, [drag]);

  const renderRightActions = useCallback(
    () => (
      <Pressable
        onPress={() => {
          swipeableRef.current?.close();
          onRequestDelete();
        }}
        accessibilityRole="button"
        accessibilityLabel={t`删除分组`}
        className="mb-2.5 ml-2 w-20 items-center justify-center rounded-lg bg-destructive active:opacity-70"
      >
        <Trash2 color={colors.destructiveForeground} size={20} />
      </Pressable>
    ),
    [colors.destructiveForeground, onRequestDelete, t],
  );

  return (
    <Swipeable
      ref={swipeableRef}
      renderRightActions={renderRightActions}
      overshootRight={false}
      rightThreshold={40}
      enabled={!editing && !isActive}
    >
      <View className={cnRow(isActive)}>
        {editing ? (
          <Input
            value={editingName}
            onChangeText={onChangeEditingName}
            autoFocus
            selectTextOnFocus
            onEndEditing={onCommitEdit}
            returnKeyType="done"
            placeholder={t`分组名称`}
            placeholderTextColor={colors.mutedForeground}
            testID={`device-group-name-input-${group.id}`}
            className="flex-1 px-2.5 text-[15px] font-semibold"
          />
        ) : (
          <Pressable
            onPress={onStartEdit}
            accessibilityRole="button"
            accessibilityLabel={t`重命名 ${group.name}`}
            className="min-w-0 flex-1 active:opacity-70"
          >
            <Text
              className="text-[15px] font-semibold text-foreground"
              numberOfLines={1}
            >
              {group.name}
            </Text>
          </Pressable>
        )}

        <Text className="text-[11px] text-muted-foreground">
          {deviceCount > 0 ? (
            <Plural value={deviceCount} one="# 台设备" other="# 台设备" />
          ) : (
            <Trans>暂无设备</Trans>
          )}
        </Text>

        <Pressable
          onLongPress={editing ? undefined : handleDrag}
          delayLongPress={120}
          disabled={editing}
          hitSlop={8}
          accessibilityRole="button"
          accessibilityLabel={t`拖动排序`}
          className="size-9 items-center justify-center rounded-lg active:opacity-70 disabled:opacity-30"
        >
          <GripVertical color={colors.mutedForeground} size={20} />
        </Pressable>
      </View>
    </Swipeable>
  );
}

/** 分组行容器 className —— active(拖拽中)加强阴影浮起。 */
function cnRow(active: boolean): string {
  return [
    "mb-2.5 h-14 flex-row items-center gap-3 rounded-lg border border-border bg-card px-3.5",
    active ? "shadow-lg shadow-black/15" : "shadow-sm shadow-black/5",
  ].join(" ");
}
