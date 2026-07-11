/**
 * 设备别名与分组的两个 bottom sheet —— 移动端形态。
 *
 * - `DeviceOrganizeSheet`：per-device 别名编辑 + 分组勾选 + 内联新建分组，
 *   从设备详情页唤起（点设备卡进详情，是移动端的「设备菜单」等价物）。
 * - `DeviceGroupsManageSheet`：分组的全局管理（创建 / 重命名 / 上移下移 / 删除），
 *   从设备中心首页的「管理分组」入口唤起。
 *
 * 两个 sheet 都自包含地从 `usePreferencesStore` 读组织数据与操作，只需外部传入
 * 目标设备 / 触发 present。别名与分组仅保存在本机，不同步给对端。
 */

import { BottomSheetTextInput } from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  ChevronDown,
  ChevronUp,
  Plus,
  Tags,
  Trash2,
} from "lucide-react-native";
import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { Pressable, View } from "react-native";
import type { MobileDevice as DeviceInfo } from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { Checkbox } from "@/components/ui/checkbox";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { usePreferencesStore } from "@/stores/preferences-store";

/* ─────────────── 共享输入样式 ─────────────── */

function useSheetInputStyle(): object {
  const colors = useThemeColors();
  return {
    flex: 1,
    minHeight: 44,
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.card,
    borderRadius: 12,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 14,
    color: colors.foreground,
  };
}

/* ─────────────── 别名与分组编辑 sheet ─────────────── */

export interface DeviceOrganizeSheetRef {
  present: (device: DeviceInfo) => void;
  dismiss: () => void;
}

export const DeviceOrganizeSheet = forwardRef<DeviceOrganizeSheetRef, object>(
  function DeviceOrganizeSheet(_props, ref) {
    const { t } = useLingui();
    const colors = useThemeColors();
    const inputStyle = useSheetInputStyle();
    const sheetRef = useRef<AppBottomSheetRef>(null);

    const { organization, setDeviceAlias, setDeviceGroups, createDeviceGroup } =
      usePreferencesStore(
        useShallow((s) => ({
          organization: s.deviceOrganization,
          setDeviceAlias: s.setDeviceAlias,
          setDeviceGroups: s.setDeviceGroups,
          createDeviceGroup: s.createDeviceGroup,
        })),
      );

    const [device, setDevice] = useState<DeviceInfo | null>(null);
    const [alias, setAlias] = useState("");
    const [groupIds, setGroupIds] = useState<string[]>([]);
    const [newGroup, setNewGroup] = useState("");

    useImperativeHandle(ref, () => ({
      present: (target) => {
        const org = usePreferencesStore.getState().deviceOrganization;
        setDevice(target);
        setAlias(org.aliases[target.peerId] ?? "");
        setGroupIds(
          Object.entries(org.groupDeviceIds)
            .filter(([, peerIds]) => peerIds.includes(target.peerId))
            .map(([groupId]) => groupId),
        );
        setNewGroup("");
        sheetRef.current?.present();
      },
      dismiss: () => sheetRef.current?.dismiss(),
    }));

    const toggleGroup = useCallback((groupId: string, checked: boolean) => {
      setGroupIds((current) =>
        checked
          ? [...current, groupId]
          : current.filter((id) => id !== groupId),
      );
    }, []);

    const handleCreateGroup = useCallback(() => {
      const id = createDeviceGroup(newGroup);
      if (id) {
        setGroupIds((current) => [...current, id]);
        setNewGroup("");
      }
    }, [createDeviceGroup, newGroup]);

    const handleSave = useCallback(() => {
      if (!device) return;
      setDeviceAlias(device.peerId, alias);
      setDeviceGroups(device.peerId, groupIds);
      sheetRef.current?.dismiss();
    }, [alias, device, groupIds, setDeviceAlias, setDeviceGroups]);

    const sortedGroups = useMemo(
      () => [...organization.groups].sort((a, b) => a.sortOrder - b.sortOrder),
      [organization.groups],
    );

    return (
      <AppBottomSheet
        ref={sheetRef}
        scrollable
        contentTestID="device-organize-sheet"
        keyboardBehavior="interactive"
        keyboardBlurBehavior="restore"
      >
        <View className="gap-5 px-5 pt-2 pb-6">
          <View className="items-center gap-2">
            <View className="size-12 items-center justify-center rounded-full bg-primary/10">
              <Tags color={colors.primary} size={22} />
            </View>
            <View className="items-center gap-1">
              <Text className="text-base font-bold text-foreground">
                <Trans>别名与分组</Trans>
              </Text>
              <Text className="text-center text-[12px] leading-5 text-muted-foreground">
                <Trans>这些信息仅保存在本机，不会同步给对端</Trans>
              </Text>
            </View>
          </View>

          <View className="gap-2">
            <Text className="px-1 text-[13px] font-semibold text-foreground">
              <Trans>设备别名</Trans>
            </Text>
            <BottomSheetTextInput
              value={alias}
              onChangeText={setAlias}
              placeholder={t`留空则用对端名称`}
              placeholderTextColor={colors.mutedForeground}
              testID="device-organize-alias-input"
              style={inputStyle}
            />
          </View>

          <View className="gap-2">
            <Text className="px-1 text-[13px] font-semibold text-foreground">
              <Trans>所属分组</Trans>
            </Text>
            {sortedGroups.length === 0 ? (
              <Text className="px-1 text-[12px] text-muted-foreground">
                <Trans>还没有分组，可在下方创建</Trans>
              </Text>
            ) : (
              <View className="gap-1">
                {sortedGroups.map((group) => {
                  const checked = groupIds.includes(group.id);
                  return (
                    <Pressable
                      key={group.id}
                      onPress={() => toggleGroup(group.id, !checked)}
                      accessibilityRole="checkbox"
                      accessibilityState={{ checked }}
                      className="min-h-11 flex-row items-center gap-3 rounded-xl border border-border bg-card px-3.5 active:opacity-70"
                    >
                      <Checkbox
                        checked={checked}
                        onCheckedChange={(next) => toggleGroup(group.id, next)}
                      />
                      <Text
                        className="flex-1 text-[14px] text-foreground"
                        numberOfLines={1}
                      >
                        {group.name}
                      </Text>
                    </Pressable>
                  );
                })}
              </View>
            )}
            <View className="flex-row items-center gap-2">
              <BottomSheetTextInput
                value={newGroup}
                onChangeText={setNewGroup}
                placeholder={t`新分组名称`}
                placeholderTextColor={colors.mutedForeground}
                testID="device-organize-new-group-input"
                style={inputStyle}
                onSubmitEditing={handleCreateGroup}
                returnKeyType="done"
              />
              <Pressable
                onPress={handleCreateGroup}
                disabled={newGroup.trim().length === 0}
                accessibilityRole="button"
                accessibilityLabel={t`创建分组`}
                testID="device-organize-create-group-button"
                className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
              >
                <Plus
                  color={
                    newGroup.trim().length === 0
                      ? colors.mutedForeground
                      : colors.primaryForeground
                  }
                  size={18}
                />
              </Pressable>
            </View>
          </View>

          <View className="flex-row gap-2.5">
            <Pressable
              onPress={() => sheetRef.current?.dismiss()}
              accessibilityRole="button"
              className="min-h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
            >
              <Text className="text-[14px] font-semibold text-foreground">
                <Trans>取消</Trans>
              </Text>
            </Pressable>
            <Pressable
              onPress={handleSave}
              accessibilityRole="button"
              testID="device-organize-save-button"
              className="min-h-12 flex-1 items-center justify-center rounded-xl bg-primary active:opacity-70"
            >
              <Text className="text-[14px] font-semibold text-primary-foreground">
                <Trans>保存</Trans>
              </Text>
            </Pressable>
          </View>
        </View>
      </AppBottomSheet>
    );
  },
);

/* ─────────────── 分组管理 sheet ─────────────── */

export interface DeviceGroupsManageSheetRef {
  present: () => void;
  dismiss: () => void;
}

export const DeviceGroupsManageSheet = forwardRef<
  DeviceGroupsManageSheetRef,
  object
>(function DeviceGroupsManageSheet(_props, ref) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const inputStyle = useSheetInputStyle();
  const sheetRef = useRef<AppBottomSheetRef>(null);

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

  const [newGroup, setNewGroup] = useState("");
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  useImperativeHandle(ref, () => ({
    present: () => sheetRef.current?.present(),
    dismiss: () => sheetRef.current?.dismiss(),
  }));

  const groups = useMemo(
    () => [...organization.groups].sort((a, b) => a.sortOrder - b.sortOrder),
    [organization.groups],
  );

  const move = useCallback(
    (groupId: string, offset: number) => {
      const ids = groups.map((group) => group.id);
      const from = ids.indexOf(groupId);
      const to = from + offset;
      if (from < 0 || to < 0 || to >= ids.length) return;
      [ids[from], ids[to]] = [ids[to], ids[from]];
      reorderDeviceGroups(ids);
    },
    [groups, reorderDeviceGroups],
  );

  const handleCreate = useCallback(() => {
    if (createDeviceGroup(newGroup)) setNewGroup("");
  }, [createDeviceGroup, newGroup]);

  return (
    <>
      <AppBottomSheet
        ref={sheetRef}
        scrollable
        contentTestID="device-groups-manage-sheet"
        keyboardBehavior="interactive"
        keyboardBlurBehavior="restore"
      >
        <View className="gap-4 px-5 pt-2 pb-6">
          <View className="items-center gap-1">
            <Text className="text-base font-bold text-foreground">
              <Trans>管理设备分组</Trans>
            </Text>
            <Text className="text-center text-[12px] leading-5 text-muted-foreground">
              <Trans>删除分组不会取消其中设备的配对</Trans>
            </Text>
          </View>

          {groups.length === 0 ? (
            <Text className="px-1 text-center text-[12px] text-muted-foreground">
              <Trans>还没有分组，在下方创建第一个</Trans>
            </Text>
          ) : (
            <View className="gap-2">
              {groups.map((group, index) => (
                <View key={group.id} className="flex-row items-center gap-1.5">
                  <BottomSheetTextInput
                    defaultValue={group.name}
                    onEndEditing={(event) =>
                      renameDeviceGroup(group.id, event.nativeEvent.text)
                    }
                    placeholder={t`分组名称`}
                    placeholderTextColor={colors.mutedForeground}
                    style={inputStyle}
                  />
                  <Pressable
                    onPress={() => move(group.id, -1)}
                    disabled={index === 0}
                    accessibilityRole="button"
                    accessibilityLabel={t`上移`}
                    className="size-9 items-center justify-center rounded-lg active:opacity-70 disabled:opacity-30"
                  >
                    <ChevronUp color={colors.mutedForeground} size={18} />
                  </Pressable>
                  <Pressable
                    onPress={() => move(group.id, 1)}
                    disabled={index === groups.length - 1}
                    accessibilityRole="button"
                    accessibilityLabel={t`下移`}
                    className="size-9 items-center justify-center rounded-lg active:opacity-70 disabled:opacity-30"
                  >
                    <ChevronDown color={colors.mutedForeground} size={18} />
                  </Pressable>
                  <Pressable
                    onPress={() => setPendingDelete(group.id)}
                    accessibilityRole="button"
                    accessibilityLabel={t`删除分组`}
                    className="size-9 items-center justify-center rounded-lg active:opacity-70"
                  >
                    <Trash2 color={colors.destructive} size={17} />
                  </Pressable>
                </View>
              ))}
            </View>
          )}

          <View className="flex-row items-center gap-2">
            <BottomSheetTextInput
              value={newGroup}
              onChangeText={setNewGroup}
              placeholder={t`新分组名称`}
              placeholderTextColor={colors.mutedForeground}
              testID="device-groups-new-group-input"
              style={inputStyle}
              onSubmitEditing={handleCreate}
              returnKeyType="done"
            />
            <Pressable
              onPress={handleCreate}
              disabled={newGroup.trim().length === 0}
              accessibilityRole="button"
              accessibilityLabel={t`创建分组`}
              testID="device-groups-create-button"
              className="size-11 items-center justify-center rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
            >
              <Plus
                color={
                  newGroup.trim().length === 0
                    ? colors.mutedForeground
                    : colors.primaryForeground
                }
                size={18}
              />
            </Pressable>
          </View>
        </View>
      </AppBottomSheet>

      <ConfirmDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
        title={<Trans>删除分组</Trans>}
        description={
          <Trans>分组内设备将保留为已配对状态，并显示在未分组列表中。</Trans>
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
    </>
  );
});
