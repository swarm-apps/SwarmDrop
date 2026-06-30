import {
  BottomSheetBackdrop,
  type BottomSheetBackdropProps,
  BottomSheetFooter,
  type BottomSheetFooterProps,
  BottomSheetModal,
  BottomSheetScrollView,
  BottomSheetTextInput,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { Directory } from "expo-file-system";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  Ban,
  Clock,
  RotateCcw,
  SendHorizontal,
  Shield,
  ShieldCheck,
  ShieldX,
  SlidersHorizontal,
  Trash2,
  UserCheck,
  Users,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
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
  AppScreen,
  BottomActionArea,
  Surface,
} from "@/components/mobile/screen";
import { SettingsHeader } from "@/components/settings-header";
import { TrustBadge, TrustLabel } from "@/components/trust-badge";
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
import { deviceDisplayName } from "@/lib/device-name";
import { devicePlatformIcon } from "@/lib/device-platform";
import { toast } from "@/lib/toast";
import { cn, errorMessage } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";

const POLICY_SHEET_SNAP_POINTS = ["72%", "90%"];

type SavingAction = "save" | "block" | "unblock" | "unpair" | null;

export default function DeviceDetailScreen() {
  const { t } = useLingui();
  const router = useRouter();
  const colors = useThemeColors();
  const { peerId } = useLocalSearchParams<{ peerId: string }>();
  const policySheetRef = useRef<BottomSheetModal>(null);
  const [draftLevel, setDraftLevel] = useState<TrustLevel>("collaborator");
  const [draftPolicy, setDraftPolicy] = useState<MobileDeviceReceivePolicy>(
    () => defaultReceivePolicy("collaborator"),
  );
  const [savingAction, setSavingAction] = useState<SavingAction>(null);
  const [unpairOpen, setUnpairOpen] = useState(false);
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

  const renderBackdrop = useCallback(
    (props: BottomSheetBackdropProps) => (
      <BottomSheetBackdrop
        {...props}
        opacity={0.4}
        appearsOnIndex={0}
        disappearsOnIndex={-1}
        pressBehavior="close"
      />
    ),
    [],
  );

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
        toast.success(t`设备策略已更新`);
        policySheetRef.current?.dismiss();
      } catch (err) {
        toast.error(t`策略保存失败`, errorMessage(err));
      } finally {
        setSavingAction(null);
      }
    },
    [device, savingAction, t, updatePairedDevicePolicy],
  );

  const handleSave = useCallback(() => {
    void savePolicy(draftLevel, draftPolicy, "save");
  }, [draftLevel, draftPolicy, savePolicy]);

  const handleBlock = useCallback(() => {
    void savePolicy("blocked", defaultReceivePolicy("blocked"), "block");
  }, [savePolicy]);

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
      setUnpairOpen(false);
      policySheetRef.current?.dismiss();
      toast.success(t`已取消配对`);
      router.back();
    } catch (err) {
      toast.error(t`取消配对失败`, errorMessage(err));
    } finally {
      setSavingAction(null);
    }
  }, [device, removePairedDevice, router, savingAction, t]);

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
          onBlock={handleBlock}
          onUnblock={handleUnblock}
          onUnpair={() => setUnpairOpen(true)}
        />
      </BottomSheetFooter>
    ),
    [
      colors.card,
      draftLevel,
      handleBlock,
      handleSave,
      handleUnblock,
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

  const displayName = deviceDisplayName(device);
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
            <View className="size-14 items-center justify-center rounded-2xl bg-muted">
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
                className="text-[12px] text-muted-foreground"
                numberOfLines={1}
              >
                {device.os} · {device.platform}
              </Text>
            </View>
            <TrustBadge level={trustLevel} confirmed={device.trustConfirmed} />
          </View>

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
          </View>
        </Surface>

        <Surface className="gap-3">
          <View className="flex-row items-center gap-2">
            <Shield color={colors.primary} size={18} />
            <Text className="text-[14px] font-semibold text-foreground">
              <Trans>信任与接收策略</Trans>
            </Text>
          </View>
          <Text className="text-[12px] text-muted-foreground">
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
            color={sendable ? colors.background : colors.mutedForeground}
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

      <BottomSheetModal
        ref={policySheetRef}
        snapPoints={POLICY_SHEET_SNAP_POINTS}
        enableDynamicSizing={false}
        enablePanDownToClose
        backdropComponent={renderBackdrop}
        footerComponent={renderPolicyFooter}
        backgroundStyle={{ backgroundColor: colors.card }}
        handleIndicatorStyle={{ backgroundColor: colors.border }}
      >
        <BottomSheetScrollView
          testID="device-policy-sheet"
          contentContainerStyle={{
            paddingHorizontal: 20,
            paddingTop: 8,
            paddingBottom: 142,
          }}
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
        </BottomSheetScrollView>
      </BottomSheetModal>

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

  const patchPolicy = (patch: Partial<MobileDeviceReceivePolicy>) => {
    onPolicyChange({ ...draftPolicy, ...patch });
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
            className="max-w-[280px] text-center text-[12px] text-muted-foreground"
            numberOfLines={2}
          >
            {deviceName}
          </Text>
        </View>
      </View>

      <View className="gap-2">
        <Text className="px-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
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

      <View className="gap-2">
        <Text className="px-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          <Trans>接收策略</Trans>
        </Text>
        <View className="overflow-hidden rounded-xl border border-border bg-card">
          <PolicySwitch
            label={<Trans>自动接收</Trans>}
            description={<Trans>本人设备可直接进入收件箱和默认保存位置</Trans>}
            checked={draftPolicy.autoAccept && !draftPolicy.requireConfirmation}
            disabled={blocked}
            testID="device-policy-auto-accept-switch"
            onCheckedChange={(checked) =>
              patchPolicy({
                autoAccept: checked,
                requireConfirmation: checked
                  ? false
                  : draftPolicy.requireConfirmation,
              })
            }
          />
          <Divider />
          <PolicySwitch
            label={<Trans>需要确认</Trans>}
            description={<Trans>收到文件时先弹出确认，不直接接收</Trans>}
            checked={draftPolicy.requireConfirmation}
            disabled={blocked}
            testID="device-policy-confirm-switch"
            onCheckedChange={(checked) =>
              patchPolicy({
                requireConfirmation: checked,
                autoAccept: checked ? false : draftPolicy.autoAccept,
              })
            }
          />
          <Divider />
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
          <Divider />
          <PolicySwitch
            label={<Trans>允许中继自动接收</Trans>}
            description={<Trans>仅在自动接收开启时生效</Trans>}
            checked={draftPolicy.allowRelayAutoAccept}
            disabled={blocked || !draftPolicy.autoAccept}
            testID="device-policy-relay-switch"
            onCheckedChange={(checked) =>
              patchPolicy({ allowRelayAutoAccept: checked })
            }
          />
        </View>
      </View>

      <View className="gap-3 rounded-xl bg-muted px-3.5 py-3">
        <View className="gap-1.5">
          <View className="flex-row items-center justify-between gap-3">
            <Text className="text-[12px] text-muted-foreground">
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
                  borderColor: sizeError ? colors.destructive : colors.border,
                  backgroundColor: colors.card,
                  borderRadius: 8,
                  paddingHorizontal: 10,
                  paddingVertical: 6,
                  textAlign: "right",
                  fontSize: 13,
                  color: colors.foreground,
                  opacity: blocked ? 0.5 : 1,
                }}
              />
              <Text className="text-[12px] text-muted-foreground">MB</Text>
            </View>
          </View>
          {sizeError ? (
            <Text
              className="text-right text-[11px] text-destructive"
              testID="device-policy-max-size-error"
            >
              <Trans>请输入大于 0 的数字，留空表示不限制</Trans>
            </Text>
          ) : null}
        </View>

        <InfoRow
          label={<Trans>有效期</Trans>}
          value={formatExpiresAt(draftPolicy.expiresAt)}
        />

        <View className="flex-row items-center justify-between gap-3">
          <Text className="text-[12px] text-muted-foreground">
            <Trans>保存位置</Trans>
          </Text>
          <View className="min-w-0 flex-1 flex-row items-center justify-end gap-2">
            <Text
              className="shrink text-right text-[12px] text-foreground"
              numberOfLines={1}
            >
              {formatSaveLocation(draftPolicy.defaultSaveLocation)}
            </Text>
            {draftPolicy.defaultSaveLocation ? (
              <Pressable
                onPress={() => patchPolicy({ defaultSaveLocation: undefined })}
                disabled={blocked}
                hitSlop={6}
                accessibilityRole="button"
                accessibilityLabel={t`恢复默认保存位置`}
                testID="device-policy-save-location-reset"
                className="rounded-full p-1 active:opacity-60 disabled:opacity-40"
              >
                <RotateCcw color={colors.mutedForeground} size={13} />
              </Pressable>
            ) : null}
            <Pressable
              onPress={onPickSaveLocation}
              disabled={blocked}
              accessibilityRole="button"
              testID="device-policy-save-location-button"
              className="rounded-lg border border-border px-2.5 py-1.5 active:opacity-70 disabled:opacity-40"
            >
              <Text className="text-[12px] font-semibold text-foreground">
                <Trans>选择</Trans>
              </Text>
            </Pressable>
          </View>
        </View>
      </View>
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
        testID="device-policy-save-button"
        disabled={savingAction !== null || saveDisabled}
        className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
      >
        {savingAction === "save" ? (
          <ActivityIndicator color={colors.background} size="small" />
        ) : (
          <ShieldCheck color={colors.background} size={17} />
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
            testID="device-policy-block-button"
            disabled={savingAction !== null}
            className="min-h-11 flex-1 flex-row items-center justify-center gap-2 rounded-xl border border-destructive/40 bg-card active:opacity-70 disabled:opacity-50"
          >
            {savingAction === "block" ? (
              <ActivityIndicator color={colors.destructive} size="small" />
            ) : (
              <Ban color={colors.destructive} size={16} />
            )}
            <Text className="text-[13px] font-semibold text-destructive">
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
        <Text className="text-[11px] text-muted-foreground" numberOfLines={2}>
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
        <Text className="text-[11px] text-muted-foreground">{description}</Text>
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
      <Text className="text-[12px] text-muted-foreground">{label}</Text>
      <Text
        className={
          mono
            ? "flex-1 text-right font-mono text-[11px] text-foreground"
            : "flex-1 text-right text-[12px] text-foreground"
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
  try {
    const decoded = decodeURIComponent(uri.replace(/\/$/, ""));
    const segments = decoded.split("/");
    return segments[segments.length - 1] || <Trans>默认位置</Trans>;
  } catch {
    return uri;
  }
}

function formatExpiresAt(expiresAt?: bigint | null): React.ReactNode {
  if (expiresAt == null) return <Trans>不限制</Trans>;
  const ms = Number(expiresAt);
  if (!Number.isFinite(ms) || ms <= 0) return <Trans>不限制</Trans>;
  return new Date(ms).toLocaleString();
}
