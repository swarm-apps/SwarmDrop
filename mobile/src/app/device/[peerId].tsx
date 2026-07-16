import {
  BottomSheetFooter,
  type BottomSheetFooterProps,
  BottomSheetTextInput,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { Directory } from "expo-file-system";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  Ban,
  ChevronDown,
  Clock,
  RotateCcw,
  SendHorizontal,
  Shield,
  ShieldCheck,
  ShieldX,
  SlidersHorizontal,
  Tags,
  Trash2,
  UserCheck,
  Users,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import Animated, { FadeIn } from "react-native-reanimated";
import type {
  MobileDevice as DeviceInfo,
  MobileDeviceReceivePolicy,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  ConnectionBadge,
  normalizeConnectionKind,
} from "@/components/connection-badge";
import {
  DeviceOrganizeSheet,
  type DeviceOrganizeSheetRef,
} from "@/components/device-organization-sheets";
import { EncryptionNote } from "@/components/encryption-note";
import {
  AppScreen,
  BottomActionArea,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { TrustBadge, TrustLabel } from "@/components/trust-badge";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Switch } from "@/components/ui/switch";
import { Text } from "@/components/ui/text";
import {
  canSendToDevice,
  defaultReceivePolicy,
  normalizePolicyForTrustLevel,
  type PolicyNote,
  policyForDevice,
  policySummaryForDevice,
  policyWithTrustDefaults,
  resolveTrustLevel,
  type TrustLevel,
  trustLevelToNative,
} from "@/core/device-trust";
import { useThemeColors } from "@/hooks/useThemeColors";
import { organizedDeviceName } from "@/lib/device-organization";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { cn, errorMessage, lastPathSegment } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { usePreferencesStore } from "@/stores/preferences-store";

type SavingAction = "save" | "block" | "unblock" | "unpair" | null;

export default function DeviceDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { peerId } = useLocalSearchParams<{ peerId: string }>();
  const policySheetRef = useRef<AppBottomSheetRef>(null);
  const organizeSheetRef = useRef<DeviceOrganizeSheetRef>(null);
  const [draftLevel, setDraftLevel] = useState<TrustLevel>("collaborator");
  const [draftPolicy, setDraftPolicy] = useState<MobileDeviceReceivePolicy>(
    () => defaultReceivePolicy("collaborator"),
  );
  const [savingAction, setSavingAction] = useState<SavingAction>(null);
  const [unpairOpen, setUnpairOpen] = useState(false);
  const [blockOpen, setBlockOpen] = useState(false);
  // 策略草稿是否合法(大小上限输入校验);非法时禁用保存按钮。
  const [policyValid, setPolicyValid] = useState(true);

  const {
    devices,
    pairedDevicesCache,
    updatePairedDevicePolicy,
    removePairedDevice,
  } = useMobileCoreStore(
    useShallow((s) => ({
      devices: s.devices,
      pairedDevicesCache: s.pairedDevicesCache,
      updatePairedDevicePolicy: s.updatePairedDevicePolicy,
      removePairedDevice: s.removePairedDevice,
    })),
  );
  const deviceOrganization = usePreferencesStore((s) => s.deviceOrganization);
  const clearDeviceOrganization = usePreferencesStore(
    (s) => s.clearDeviceOrganization,
  );

  const device = useMemo<DeviceInfo | null>(() => {
    if (!peerId) return null;
    return (
      devices.find((item) => item.peerId === peerId) ??
      summariesToOfflineDevices(pairedDevicesCache).find(
        (item) => item.peerId === peerId,
      ) ??
      null
    );
  }, [peerId, devices, pairedDevicesCache]);

  // 仅在首次加载或切换 peerId 时初始化草稿；后台 DevicesChanged 刷新会换出新的 device
  // 引用，但只要还是同一台设备就不重置，避免抹掉用户正在编辑的未保存策略。
  const seededPeerIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!device || seededPeerIdRef.current === device.peerId) return;
    seededPeerIdRef.current = device.peerId;
    setDraftLevel(resolveTrustLevel(device));
    setDraftPolicy(policyForDevice(device));
  }, [device]);

  const openPolicySheet = useCallback(() => {
    if (!device) return;
    setDraftLevel(resolveTrustLevel(device));
    setDraftPolicy(policyForDevice(device));
    setPolicyValid(true);
    policySheetRef.current?.present();
  }, [device]);

  const savePolicy = useCallback(
    async (
      nextLevel: TrustLevel,
      nextPolicy: MobileDeviceReceivePolicy,
      action: Exclude<SavingAction, null>,
    ) => {
      if (!device || savingAction !== null) return;
      setSavingAction(action);
      try {
        const normalizedPolicy = normalizePolicyForTrustLevel(
          nextLevel,
          nextPolicy,
        );
        await updatePairedDevicePolicy(
          device.peerId,
          trustLevelToNative(nextLevel),
          normalizedPolicy,
        );
        setDraftLevel(nextLevel);
        setDraftPolicy(normalizedPolicy);
        // 针对性反馈:阻止/解除有专属文案,别一律"设备策略已更新"含糊带过。
        toast.success(
          action === "block"
            ? t`已阻止 ${organizedDeviceName(device, deviceOrganization)}`
            : action === "unblock"
              ? t`已解除阻止`
              : t`设备策略已更新`,
        );
        policySheetRef.current?.dismiss();
      } catch (err) {
        toast.error(t`策略保存失败`, errorMessage(err));
      } finally {
        setSavingAction(null);
      }
    },
    [device, deviceOrganization, savingAction, t, updatePairedDevicePolicy],
  );

  const handleSave = useCallback(() => {
    void savePolicy(draftLevel, draftPolicy, "save");
  }, [draftLevel, draftPolicy, savePolicy]);

  const handleBlock = useCallback(() => {
    void savePolicy("blocked", defaultReceivePolicy("blocked"), "block");
  }, [savePolicy]);

  // 阻止是敏感信任动作(断对方发送 + 关自动接收),与"取消配对"同级,补二次确认。
  const openBlockConfirm = useCallback(() => setBlockOpen(true), []);

  const handleUnblock = useCallback(() => {
    void savePolicy(
      "collaborator",
      defaultReceivePolicy("collaborator"),
      "unblock",
    );
  }, [savePolicy]);

  const handleUnpair = useCallback(async () => {
    if (!device || savingAction !== null) return;
    setSavingAction("unpair");
    try {
      await removePairedDevice(device.peerId);
      // 取消配对同时清理该 PeerId 的本机别名与全部分组成员关系。
      clearDeviceOrganization(device.peerId);
      setUnpairOpen(false);
      policySheetRef.current?.dismiss();
      toast.success(t`已取消配对`);
      router.back();
    } catch (err) {
      toast.error(t`取消配对失败`, errorMessage(err));
    } finally {
      setSavingAction(null);
    }
  }, [
    device,
    clearDeviceOrganization,
    removePairedDevice,
    router,
    savingAction,
    t,
  ]);

  const renderPolicyFooter = useCallback(
    (props: BottomSheetFooterProps) => (
      <BottomSheetFooter
        {...props}
        bottomInset={0}
        style={{ backgroundColor: colors.card }}
      >
        <PolicyActionFooter
          draftLevel={draftLevel}
          savingAction={savingAction}
          saveDisabled={!policyValid}
          onSave={handleSave}
          onBlock={openBlockConfirm}
          onUnblock={handleUnblock}
          onUnpair={() => setUnpairOpen(true)}
        />
      </BottomSheetFooter>
    ),
    [
      colors.card,
      draftLevel,
      handleSave,
      handleUnblock,
      openBlockConfirm,
      policyValid,
      savingAction,
    ],
  );

  if (!device) {
    return (
      <AppScreen testID="device-detail-missing-screen">
        <SettingsHeader title={t`设备详情`} />
        <View className="flex-1 items-center justify-center gap-3">
          <Text className="text-[13px] text-muted-foreground">
            <Trans>设备未找到</Trans>
          </Text>
          <Pressable
            onPress={() => router.back()}
            accessibilityRole="button"
            className="min-h-11 items-center justify-center rounded-xl bg-primary px-4 active:opacity-70"
          >
            <Text className="text-[13px] font-semibold text-primary-foreground">
              <Trans>返回</Trans>
            </Text>
          </Pressable>
        </View>
      </AppScreen>
    );
  }

  const displayName = organizedDeviceName(device, deviceOrganization);
  const Icon = devicePlatformIcon(`${device.os} ${device.platform}`);
  const trustLevel = resolveTrustLevel(device);
  const policy = policySummaryForDevice(device);
  const sendable = canSendToDevice(device);

  return (
    <AppScreen testID="device-detail-screen" contentClassName="gap-4 px-0 pb-0">
      <SettingsHeader title={t`设备详情`} />

      <View className="flex-1 gap-4 px-5">
        <Surface className="gap-4">
          <View className="flex-row items-center gap-3">
            <View className="size-14 items-center justify-center rounded-full bg-muted">
              <Icon color={colors.foreground} size={25} />
            </View>
            <View className="min-w-0 flex-1 gap-1">
              <Text
                className="text-[18px] font-semibold text-foreground"
                numberOfLines={1}
              >
                {displayName}
              </Text>
              <Text
                className="text-[13px] text-muted-foreground"
                numberOfLines={1}
              >
                {device.os} · {device.platform}
              </Text>
            </View>
            <TrustBadge level={trustLevel} confirmed={device.trustConfirmed} />
          </View>

          <Pressable
            onPress={() => organizeSheetRef.current?.present(device)}
            accessibilityRole="button"
            testID="device-organize-entry"
            className="min-h-11 flex-row items-center justify-center gap-2 rounded-xl border border-border active:opacity-70"
          >
            <Tags color={colors.foreground} size={16} />
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>别名与分组</Trans>
            </Text>
          </Pressable>

          <View className="gap-2">
            <InfoRow
              label={<Trans>连接状态</Trans>}
              value={
                device.status === "online" ? (
                  <Trans>在线</Trans>
                ) : (
                  <Trans>离线</Trans>
                )
              }
            />
            <InfoRow
              label={<Trans>连接路径</Trans>}
              value={
                normalizeConnectionKind(device.connection) ? (
                  <View className="flex-row justify-end">
                    <ConnectionBadge
                      connection={device.connection}
                      latencyMs={device.latencyMs}
                    />
                  </View>
                ) : (
                  <Trans>等待发现</Trans>
                )
              }
            />
            {device.latencyMs != null ? (
              <InfoRow
                label={<Trans>延迟</Trans>}
                value={`${Number(device.latencyMs)}ms`}
              />
            ) : null}
            <InfoRow
              label={<Trans>Peer ID</Trans>}
              value={device.peerId}
              mono
            />
            <EncryptionNote>
              <Trans>这串 ID 由它的加密密钥生成，像指纹一样独一无二</Trans>
            </EncryptionNote>
          </View>
        </Surface>

        <Surface className="gap-3">
          <View className="flex-row items-center gap-2">
            <Shield color={colors.primary} size={18} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>信任与接收策略</Trans>
            </Text>
          </View>
          <Text className="text-[13px] text-muted-foreground">
            <PolicyHeadline note={policy.note} />
          </Text>
          <View className="gap-2 rounded-lg bg-muted px-3.5 py-3">
            <InfoRow
              label={<Trans>接收方式</Trans>}
              value={<PolicyModeLabel note={policy.note} />}
            />
            <InfoRow
              label={<Trans>文件夹</Trans>}
              value={
                policy.policy.allowDirectories ? (
                  <Trans>允许</Trans>
                ) : (
                  <Trans>不允许</Trans>
                )
              }
            />
            <InfoRow
              label={<Trans>保存位置</Trans>}
              value={formatSaveLocation(policy.policy.defaultSaveLocation)}
            />
          </View>
          <Pressable
            onPress={openPolicySheet}
            accessibilityRole="button"
            testID="device-policy-entry"
            className="min-h-11 flex-row items-center justify-center gap-2 rounded-xl border border-border active:opacity-70"
          >
            <SlidersHorizontal color={colors.foreground} size={16} />
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>策略设置</Trans>
            </Text>
          </Pressable>
        </Surface>
      </View>

      <BottomActionArea>
        <Pressable
          onPress={() => {
            router.push({
              pathname: "/send/select-device",
              params: { peerId: device.peerId },
            } as never);
          }}
          accessibilityRole="button"
          testID="device-detail-send-button"
          disabled={!sendable}
          className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          <SendHorizontal
            color={sendable ? colors.primaryForeground : colors.mutedForeground}
            size={17}
          />
          <Text
            className={
              sendable
                ? "text-[14px] font-semibold text-primary-foreground"
                : "text-[14px] font-semibold text-muted-foreground"
            }
          >
            <Trans>发送文件</Trans>
          </Text>
        </Pressable>
      </BottomActionArea>

      <AppBottomSheet
        ref={policySheetRef}
        scrollable
        contentTestID="device-policy-sheet"
        contentContainerStyle={{
          paddingHorizontal: 20,
          paddingTop: 8,
          paddingBottom: 142,
        }}
        footerComponent={renderPolicyFooter}
        keyboardBehavior="interactive"
        keyboardBlurBehavior="restore"
      >
        <PolicyEditor
          deviceName={displayName}
          draftLevel={draftLevel}
          draftPolicy={draftPolicy}
          onLevelChange={(level) => {
            setDraftLevel(level);
            setDraftPolicy((current) =>
              policyWithTrustDefaults(level, current),
            );
          }}
          onPolicyChange={setDraftPolicy}
          onValidityChange={setPolicyValid}
        />
      </AppBottomSheet>

      <DeviceOrganizeSheet ref={organizeSheetRef} />

      <ConfirmDialog
        open={unpairOpen}
        onOpenChange={setUnpairOpen}
        title={<Trans>取消配对</Trans>}
        description={
          <Trans>取消后需要重新配对，才能再次向这台设备发送或接收文件。</Trans>
        }
        actionLabel={<Trans>取消配对</Trans>}
        destructive
        onAction={handleUnpair}
        contentTestID="device-unpair-dialog"
        actionTestID="device-unpair-confirm-button"
      />

      <ConfirmDialog
        open={blockOpen}
        onOpenChange={setBlockOpen}
        title={<Trans>阻止这台设备</Trans>}
        description={
          <Trans>
            阻止后对方无法向你发送文件,自动接收也会关闭。你可以随时解除阻止。
          </Trans>
        }
        actionLabel={<Trans>阻止设备</Trans>}
        destructive
        onAction={() => {
          setBlockOpen(false);
          handleBlock();
        }}
        contentTestID="device-block-dialog"
        actionTestID="device-block-confirm-button"
      />
    </AppScreen>
  );
}

function PolicyEditor({
  deviceName,
  draftLevel,
  draftPolicy,
  onLevelChange,
  onPolicyChange,
  onValidityChange,
}: {
  deviceName: string;
  draftLevel: TrustLevel;
  draftPolicy: MobileDeviceReceivePolicy;
  onLevelChange: (level: TrustLevel) => void;
  onPolicyChange: (policy: MobileDeviceReceivePolicy) => void;
  onValidityChange: (valid: boolean) => void;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const blocked = draftLevel === "blocked";
  const [showAdvanced, setShowAdvanced] = useState(false);

  const patchPolicy = (patch: Partial<MobileDeviceReceivePolicy>) => {
    onPolicyChange({ ...draftPolicy, ...patch });
  };

  // 接收方式只有两个真实状态:自动接收(autoAccept) vs 需要确认。用二选一分段显式表达,
  // 取代原先两个会互相静默关闭的开关(both-false 只属于 blocked,不作为用户可选项)。
  const autoMode = draftPolicy.autoAccept && !draftPolicy.requireConfirmation;
  const setReceiveMode = (mode: "auto" | "confirm") => {
    if (mode === "auto") {
      patchPolicy({ autoAccept: true, requireConfirmation: false });
    } else {
      patchPolicy({
        autoAccept: false,
        requireConfirmation: true,
        allowRelayAutoAccept: false,
      });
    }
  };

  // 大小上限以 MB 文本编辑;非法输入时不回写 draftPolicy(保留上次有效值)并标记草稿无效。
  const [sizeText, setSizeText] = useState(() =>
    bytesToMbText(draftPolicy.maxTransferBytes),
  );
  const [sizeError, setSizeError] = useState(false);

  // 当 maxTransferBytes 变化(切换信任级别会带动默认值)时,把输入框重新校准回有效值。
  useEffect(() => {
    setSizeText(bytesToMbText(draftPolicy.maxTransferBytes));
    setSizeError(false);
    onValidityChange(true);
  }, [draftPolicy.maxTransferBytes, onValidityChange]);

  const onSizeChange = (text: string) => {
    setSizeText(text);
    const trimmed = text.trim();
    if (trimmed === "") {
      setSizeError(false);
      onValidityChange(true);
      patchPolicy({ maxTransferBytes: undefined });
      return;
    }
    const mb = Number(trimmed);
    if (!Number.isFinite(mb) || mb <= 0) {
      setSizeError(true);
      onValidityChange(false);
      return;
    }
    setSizeError(false);
    onValidityChange(true);
    patchPolicy({
      maxTransferBytes: BigInt(Math.floor(mb)) * 1024n * 1024n,
    });
  };

  const onPickSaveLocation = async () => {
    try {
      const dir = await Directory.pickDirectoryAsync();
      try {
        dir.list();
      } catch (probeErr) {
        toast.error(t`此目录不可读`, errorMessage(probeErr));
        return;
      }
      patchPolicy({ defaultSaveLocation: dir.uri });
    } catch (err) {
      toast.error(t`选择失败`, errorMessage(err));
    }
  };

  return (
    <View className="gap-5">
      <View className="items-center gap-2">
        <View className="size-12 items-center justify-center rounded-full bg-primary/10">
          <Shield color={colors.primary} size={23} />
        </View>
        <View className="items-center gap-1">
          <Text className="text-[16px] font-semibold text-foreground">
            <Trans>设备策略</Trans>
          </Text>
          <Text
            className="max-w-[280px] text-center text-[13px] text-muted-foreground"
            numberOfLines={2}
          >
            {deviceName}
          </Text>
        </View>
      </View>

      <View className="gap-2">
        <Text className="px-1 text-[13px] font-semibold text-foreground">
          <Trans>信任级别</Trans>
        </Text>
        <View className="gap-2">
          {(["owned", "collaborator", "temporary", "blocked"] as const).map(
            (level) => (
              <TrustOption
                key={level}
                level={level}
                selected={draftLevel === level}
                onPress={() => onLevelChange(level)}
              />
            ),
          )}
        </View>
      </View>

      {/* 接收方式:二选一分段,取代两个会静默互斥的开关 —— 选择本身即可见、可互斥 */}
      <View className="gap-2">
        <Text className="px-1 text-[13px] font-semibold text-foreground">
          <Trans>接收方式</Trans>
        </Text>
        <ReceiveModeSegment
          mode={autoMode ? "auto" : "confirm"}
          disabled={blocked}
          onChange={setReceiveMode}
        />
        <Text className="px-1 text-[12px] text-muted-foreground">
          {autoMode ? (
            <Trans>文件直接进入收件箱和默认保存位置</Trans>
          ) : (
            <Trans>收到文件时先弹出确认,不直接接收</Trans>
          )}
        </Text>
      </View>

      {/* 允许文件夹 —— 基础项 */}
      <View className="overflow-hidden rounded-xl border border-border bg-card">
        <PolicySwitch
          label={<Trans>允许文件夹</Trans>}
          description={<Trans>关闭后只接收单个文件或文件集合</Trans>}
          checked={draftPolicy.allowDirectories}
          disabled={blocked}
          testID="device-policy-directories-switch"
          onCheckedChange={(checked) =>
            patchPolicy({ allowDirectories: checked })
          }
        />
      </View>

      {/* 保存位置 —— 基础项 */}
      <View className="rounded-xl bg-muted px-3.5 py-3">
        <View className="flex-row items-center justify-between gap-3">
          <Text className="text-[13px] text-muted-foreground">
            <Trans>保存位置</Trans>
          </Text>
          <View className="min-w-0 flex-1 flex-row items-center justify-end gap-2">
            <Text
              className="shrink text-right text-[13px] text-foreground"
              numberOfLines={1}
            >
              {formatSaveLocation(draftPolicy.defaultSaveLocation)}
            </Text>
            {draftPolicy.defaultSaveLocation ? (
              <Pressable
                onPress={() => patchPolicy({ defaultSaveLocation: undefined })}
                disabled={blocked}
                hitSlop={10}
                accessibilityRole="button"
                accessibilityLabel={t`恢复默认保存位置`}
                testID="device-policy-save-location-reset"
                className="min-h-11 min-w-11 items-center justify-center rounded-full active:opacity-60 disabled:opacity-40"
              >
                <RotateCcw color={colors.mutedForeground} size={15} />
              </Pressable>
            ) : null}
            <Pressable
              onPress={onPickSaveLocation}
              disabled={blocked}
              accessibilityRole="button"
              testID="device-policy-save-location-button"
              className="min-h-11 items-center justify-center rounded-lg border border-border px-3.5 active:opacity-70 disabled:opacity-40"
            >
              <Text className="text-[13px] font-semibold text-foreground">
                <Trans>选择</Trans>
              </Text>
            </Pressable>
          </View>
        </View>
      </View>

      {/* 高级:渐进披露,默认收起 —— 中继/大小上限/有效期这些限制项留给需要的人 */}
      <View className="overflow-hidden rounded-xl border border-border bg-card">
        <Pressable
          onPress={() => setShowAdvanced((v) => !v)}
          accessibilityRole="button"
          accessibilityLabel={t`高级`}
          accessibilityState={{ expanded: showAdvanced }}
          testID="device-policy-advanced-toggle"
          className="min-h-11 flex-row items-center justify-between px-3.5 py-2.5 active:opacity-70"
        >
          <Text className="text-[13px] font-medium text-foreground">
            <Trans>高级</Trans>
          </Text>
          <ChevronDown
            size={16}
            color={colors.mutedForeground}
            style={{
              transform: [{ rotate: showAdvanced ? "180deg" : "0deg" }],
            }}
          />
        </Pressable>
        {showAdvanced ? (
          <Animated.View entering={FadeIn.duration(160)}>
            <Divider />
            <PolicySwitch
              label={<Trans>允许中继自动接收</Trans>}
              description={<Trans>仅在自动接收开启时生效</Trans>}
              checked={draftPolicy.allowRelayAutoAccept}
              disabled={blocked || !autoMode}
              testID="device-policy-relay-switch"
              onCheckedChange={(checked) =>
                patchPolicy({ allowRelayAutoAccept: checked })
              }
            />
            <Divider />
            <View className="gap-1.5 px-3.5 py-3">
              <View className="flex-row items-center justify-between gap-3">
                <Text className="text-[13px] text-muted-foreground">
                  <Trans>最大大小</Trans>
                </Text>
                <View className="flex-row items-center gap-2">
                  <BottomSheetTextInput
                    value={sizeText}
                    onChangeText={onSizeChange}
                    editable={!blocked}
                    keyboardType="number-pad"
                    placeholder={t`不限制`}
                    placeholderTextColor={colors.mutedForeground}
                    testID="device-policy-max-size-input"
                    style={{
                      minWidth: 96,
                      borderWidth: 1,
                      borderColor: sizeError
                        ? colors.destructive
                        : colors.border,
                      backgroundColor: colors.card,
                      borderRadius: 8,
                      paddingHorizontal: 10,
                      paddingVertical: 8,
                      textAlign: "right",
                      fontSize: 13,
                      color: colors.foreground,
                      opacity: blocked ? 0.5 : 1,
                    }}
                  />
                  <Text className="text-[13px] text-muted-foreground">MB</Text>
                </View>
              </View>
              {sizeError ? (
                <Text
                  className="text-right text-[12px] text-destructive-ink"
                  testID="device-policy-max-size-error"
                >
                  <Trans>请输入大于 0 的数字，留空表示不限制</Trans>
                </Text>
              ) : null}
            </View>
            <Divider />
            <View className="px-3.5 py-3">
              <InfoRow
                label={<Trans>有效期</Trans>}
                value={formatExpiresAt(draftPolicy.expiresAt)}
              />
            </View>
          </Animated.View>
        ) : null}
      </View>
    </View>
  );
}

function ReceiveModeSegment({
  mode,
  disabled,
  onChange,
}: {
  mode: "auto" | "confirm";
  disabled?: boolean;
  onChange: (mode: "auto" | "confirm") => void;
}) {
  const options = [
    { key: "auto", label: <Trans>自动接收</Trans> },
    { key: "confirm", label: <Trans>需要确认</Trans> },
  ] as const;
  return (
    <View
      className={cn(
        "flex-row gap-1 rounded-xl border border-border bg-muted p-1",
        disabled && "opacity-50",
      )}
    >
      {options.map((opt) => {
        const active = mode === opt.key;
        return (
          <Pressable
            key={opt.key}
            onPress={() => onChange(opt.key)}
            disabled={disabled}
            accessibilityRole="button"
            accessibilityState={{ selected: active }}
            testID={`device-policy-mode-${opt.key}`}
            className={cn(
              "min-h-11 flex-1 items-center justify-center rounded-lg active:opacity-70",
              active && "bg-card",
            )}
          >
            <Text
              className={cn(
                "text-[13px]",
                active
                  ? "font-semibold text-foreground"
                  : "text-muted-foreground",
              )}
            >
              {opt.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/** 把 maxTransferBytes(字节,bigint)转成可编辑的 MB 文本;null/未设 → 空串(不限制)。 */
function bytesToMbText(bytes?: bigint | null): string {
  if (bytes == null) return "";
  const mb = Math.ceil(Number(bytes) / (1024 * 1024));
  return mb > 0 ? String(mb) : "";
}

function PolicyActionFooter({
  draftLevel,
  savingAction,
  saveDisabled,
  onSave,
  onBlock,
  onUnblock,
  onUnpair,
}: {
  draftLevel: TrustLevel;
  savingAction: SavingAction;
  saveDisabled?: boolean;
  onSave: () => void;
  onBlock: () => void;
  onUnblock: () => void;
  onUnpair: () => void;
}) {
  const colors = useThemeColors();
  const blocked = draftLevel === "blocked";

  return (
    <View className="gap-2.5 border-t border-border bg-card px-5 pt-3 pb-4">
      <Pressable
        onPress={onSave}
        accessibilityRole="button"
        accessibilityState={{
          busy: savingAction === "save",
          disabled: savingAction !== null || saveDisabled,
        }}
        testID="device-policy-save-button"
        disabled={savingAction !== null || saveDisabled}
        className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
      >
        {savingAction === "save" ? (
          <ActivityIndicator color={colors.primaryForeground} size="small" />
        ) : (
          <ShieldCheck color={colors.primaryForeground} size={17} />
        )}
        <Text className="text-[14px] font-semibold text-primary-foreground">
          <Trans>保存策略</Trans>
        </Text>
      </Pressable>

      <View className="flex-row gap-2.5">
        {blocked ? (
          <Pressable
            onPress={onUnblock}
            accessibilityRole="button"
            accessibilityState={{
              busy: savingAction === "unblock",
              disabled: savingAction !== null,
            }}
            testID="device-policy-unblock-button"
            disabled={savingAction !== null}
            className="min-h-11 flex-1 flex-row items-center justify-center gap-2 rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
          >
            {savingAction === "unblock" ? (
              <ActivityIndicator color={colors.foreground} size="small" />
            ) : (
              <Users color={colors.foreground} size={16} />
            )}
            <Text className="text-[13px] font-semibold text-foreground">
              <Trans>解除阻止</Trans>
            </Text>
          </Pressable>
        ) : (
          <Pressable
            onPress={onBlock}
            accessibilityRole="button"
            accessibilityState={{
              busy: savingAction === "block",
              disabled: savingAction !== null,
            }}
            testID="device-policy-block-button"
            disabled={savingAction !== null}
            className="min-h-11 flex-1 flex-row items-center justify-center gap-2 rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
          >
            {savingAction === "block" ? (
              <ActivityIndicator color={colors.destructive} size="small" />
            ) : (
              <Ban color={colors.destructive} size={16} />
            )}
            <Text className="text-[13px] font-semibold text-destructive-ink">
              <Trans>阻止设备</Trans>
            </Text>
          </Pressable>
        )}

        <Pressable
          onPress={onUnpair}
          accessibilityRole="button"
          testID="device-policy-unpair-button"
          disabled={savingAction !== null}
          className="min-h-11 flex-1 flex-row items-center justify-center gap-2 rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <Trash2 color={colors.mutedForeground} size={16} />
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>取消配对</Trans>
          </Text>
        </Pressable>
      </View>
    </View>
  );
}

function TrustOption({
  level,
  selected,
  onPress,
}: {
  level: TrustLevel;
  selected: boolean;
  onPress: () => void;
}) {
  const colors = useThemeColors();
  const Icon = trustIcon(level);
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      testID={`device-policy-trust-${level}`}
      className={cn(
        "min-h-14 flex-row items-center gap-3 rounded-xl border px-3.5 py-3 active:opacity-70",
        selected ? "border-primary bg-primary/10" : "border-border bg-card",
      )}
    >
      <View
        className={cn(
          "size-9 items-center justify-center rounded-full",
          selected ? "bg-primary/15" : "bg-muted",
        )}
      >
        <Icon
          color={selected ? colors.primary : colors.mutedForeground}
          size={18}
        />
      </View>
      <View className="min-w-0 flex-1 gap-0.5">
        <Text className="text-[14px] font-semibold text-foreground">
          <TrustLabel level={level} />
        </Text>
        <Text className="text-[12px] text-muted-foreground" numberOfLines={2}>
          <TrustDescription level={level} />
        </Text>
      </View>
      {selected ? <ShieldCheck color={colors.primary} size={18} /> : null}
    </Pressable>
  );
}

function PolicySwitch({
  label,
  description,
  checked,
  disabled,
  testID,
  onCheckedChange,
}: {
  label: React.ReactNode;
  description: React.ReactNode;
  checked: boolean;
  disabled?: boolean;
  testID: string;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <View className="min-h-16 flex-row items-center gap-3 px-3.5 py-3">
      <View className="flex-1 gap-0.5">
        <Text className="text-[14px] text-foreground">{label}</Text>
        <Text className="text-[12px] text-muted-foreground">{description}</Text>
      </View>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
        testID={testID}
      />
    </View>
  );
}

function PolicyHeadline({ note }: { note: PolicyNote }) {
  switch (note) {
    case "blocked":
      return <Trans>该设备已被阻止，发送入口和自动接收都会关闭。</Trans>;
    case "temporary":
      return <Trans>临时设备默认需要确认，并限制文件夹和自动接收。</Trans>;
    case "auto_accept":
      return <Trans>该设备会自动接收，并保存到收件箱和默认位置。</Trans>;
    default:
      return <Trans>收到文件前需要手动确认。</Trans>;
  }
}

function PolicyModeLabel({ note }: { note: PolicyNote }) {
  switch (note) {
    case "auto_accept":
      return <Trans>自动接收</Trans>;
    case "temporary":
      return <Trans>临时确认</Trans>;
    case "blocked":
      return <Trans>已阻止</Trans>;
    default:
      return <Trans>手动确认</Trans>;
  }
}

function TrustDescription({ level }: { level: TrustLevel }) {
  switch (level) {
    case "owned":
      return <Trans>自己的设备，可自动接收入站文件。</Trans>;
    case "temporary":
      return <Trans>短期授权，默认一天后过期。</Trans>;
    case "blocked":
      return <Trans>阻止发送和入站接收。</Trans>;
    default:
      return <Trans>默认级别，收到文件前需要确认。</Trans>;
  }
}

function trustIcon(level: TrustLevel) {
  switch (level) {
    case "owned":
      return UserCheck;
    case "temporary":
      return Clock;
    case "blocked":
      return ShieldX;
    default:
      return Users;
  }
}

function InfoRow({
  label,
  value,
  mono,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  mono?: boolean;
}) {
  return (
    <View className="flex-row items-center justify-between gap-3">
      <Text className="text-[13px] text-muted-foreground">{label}</Text>
      <Text
        className={
          mono
            ? "flex-1 text-right font-mono text-[12px] text-foreground"
            : "flex-1 text-right text-[13px] text-foreground"
        }
        numberOfLines={1}
      >
        {value}
      </Text>
    </View>
  );
}

function Divider() {
  return <View className="h-px bg-border" />;
}

function formatSaveLocation(uri?: string | null): React.ReactNode {
  if (!uri) return <Trans>收件箱</Trans>;
  return lastPathSegment(uri) || <Trans>默认位置</Trans>;
}

function formatExpiresAt(expiresAt?: bigint | null): React.ReactNode {
  if (expiresAt == null) return <Trans>不限制</Trans>;
  const ms = Number(expiresAt);
  if (!Number.isFinite(ms) || ms <= 0) return <Trans>不限制</Trans>;
  return new Date(ms).toLocaleString();
}
