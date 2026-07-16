/**
 * 设备别名与分组编辑 sheet —— 移动端形态。
 *
 * `DeviceOrganizeSheet`:per-device 别名编辑 + 分组勾选 + 内联新建分组,从设备详情页
 * 唤起(点设备卡进详情,是移动端的「设备菜单」等价物)。
 *
 * 分组的全局管理(创建 / 重命名 / 拖拽排序 / 删除)已迁到独立页面
 * `app/device/groups.tsx`。别名与分组仅保存在本机,不同步给对端。
 */

import { BottomSheetTextInput } from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { Plus, Tags } from "lucide-react-native";
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
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { sortedDeviceGroups } from "@/lib/device-organization";
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
    // Android: 去字体额外 padding + 垂直居中,避免 CJK 文字被裁(同 ui/input 基线)。
    includeFontPadding: false,
    textAlignVertical: "center",
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
      () => sortedDeviceGroups(organization),
      [organization],
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
              <Text className="text-center text-[13px] leading-5 text-muted-foreground">
                <Trans>这些信息仅保存在本机,不会同步给对端</Trans>
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
              <Text className="px-1 text-[13px] text-muted-foreground">
                <Trans>还没有分组,可在下方创建</Trans>
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
