import {
  BottomSheetFooter,
  type BottomSheetFooterProps,
  BottomSheetTextInput,
} from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import { organizedDeviceName } from "@swarmdrop/shared-view";
import { useLocalSearchParams, useRouter } from "expo-router";
import {
  Ban,
  ChevronDown,
  Clock,
  RotateCcw,
  Send,
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
import {
  ActivityIndicator,
  type LayoutChangeEvent,
  Pressable,
  ScrollView,
  View,
} from "react-native";
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
import { ConnectionDetailsSection } from "@/components/connection-details";
import {
  DeviceOrganizeSheet,
  type DeviceOrganizeSheetRef,
} from "@/components/device-organization-sheets";
import { EncryptionNote } from "@/components/encryption-note";
import {
  AppScreen,
  BottomActionBar,
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
  resolveTrustLevel,
  type TrustLevel,
  trustLevelToNative,
} from "@/core/device-trust";
import { pickDirectory } from "@/core/receive-location";
import {
  BOTTOM_BREATHING,
  useBottomSafePadding,
} from "@/hooks/useBottomSafePadding";
import { useThemeColors } from "@/hooks/useThemeColors";
import { devicePlatformIcon } from "@/lib/device-platform";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { cn, lastPathSegment } from "@/lib/utils";
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

  // —— 策略 sheet 的内容底距 ——
  // gorhom 的 footer 是**绝对定位**悬在滚动内容之上,内容侧不会自动为它让位,所以内容
  // 底部必须空出 footer 的高度,否则最后一项被永久盖住。这里**量测**而不是手算:
  // 此前写的是常数 142(比当时实测的 131 多 11),footer 一变高就不够,而没有任何东西会拦
  // ——footer 补上 bottom inset 之后正好高出 24~48dp,那个常数当场失效。
  // ⚠️ 若将来在 `AppBottomSheet` 里打开 gorhom 自带的 `enableFooterMarginAdjustment`
  // (它做的是同一件事:把实测 footer 高度加进 `contentContainerStyle.paddingBottom`),
  // **必须同时删掉这里的量测**,否则底距翻倍。
  const [policyFooterHeight, setPolicyFooterHeight] = useState(
    POLICY_FOOTER_FALLBACK_HEIGHT,
  );
  const handlePolicyFooterLayout = useCallback((e: LayoutChangeEvent) => {
    setPolicyFooterHeight(e.nativeEvent.layout.height);
  }, []);
  const policySheetContentStyle = useMemo(
    () => ({
      paddingHorizontal: 20,
      paddingTop: 8,
      // footer 实高 + 一段呼吸位(与 footer 内部的 pt-3 对称,和 BottomActionBar 同一个数)。
      paddingBottom: policyFooterHeight + BOTTOM_BREATHING,
    }),
    [policyFooterHeight],
  );

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
        toast.error(t`策略保存失败`, getErrorMessage(err));
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
      toast.error(t`取消配对失败`, getErrorMessage(err));
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
        // 保持 0(也是 gorhom 的默认值,写出来是为了挡住「顺手改成 insets.bottom」)。
        // 底部安全区**只能有一处吃**,这里让给 `PolicyActionFooter` 自己的 paddingBottom:
        // `bottomInset` 是把整条 footer 上移,下方会露出一条缝 —— 而 footer 绝对定位、
        // 滚动内容不为它让位,内容就会从那条缝里穿出来。写进 footer 自己的 padding 则
        // 那块不透明 `bg-card` 一直铺到屏底,与 `BottomActionBar` 同款。
        bottomInset={0}
        style={{ backgroundColor: colors.card }}
      >
        <PolicyActionFooter
          draftLevel={draftLevel}
          savingAction={savingAction}
          saveDisabled={!policyValid}
          onLayout={handlePolicyFooterLayout}
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
      handlePolicyFooterLayout,
      handleSave,
      handleUnblock,
      openBlockConfirm,
      policyValid,
      savingAction,
    ],
  );

  if (!device) {
    return (
      <AppScreen
        testID="device-detail-missing-screen"
        header={<SettingsHeader title={t`设备详情`} />}
      >
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
    // ConfirmDialog 挂在 AppScreen **外面**:它的 Root 会渲染一个真实的零高 View,
    // 留在带 gap 的内容盒里每个都会凭空撑出一段死带(Yoga 的 gap 不看子节点高度)。
    // 同款注释见 settings/bootstrap-nodes.tsx。两个 gorhom sheet 走 Portal、原地不产生
    // 节点,留在 AppScreen 里没问题。
    <>
      <AppScreen
        testID="device-detail-screen"
        header={<SettingsHeader title={t`设备详情`} />}
        bare
      >
        {/* 内容必须是滚动容器:展开「链路详情」后这一屏放不下,而底栏是流内兄弟节点、
            不悬浮 —— 没有滚动时超出的部分(包括「策略设置」入口)用户永远够不到。
            pt-4 与列表页的 LIST_CONTENT_PADDING_UNDER_HEADER 同值(导航条与首张卡的间距);
            pb-6 是滚到底时最后一张卡与底栏之间的呼吸位。 */}
        <ScrollView
          className="flex-1"
          showsVerticalScrollIndicator={false}
          contentContainerClassName="gap-4 px-5 pt-4 pb-6"
        >
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
              <TrustBadge
                level={trustLevel}
                confirmed={device.trustConfirmed}
              />
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
                        transport={device.connectionDetails?.transport}
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
              {device.connectionDetails ? (
                <ConnectionDetailsSection
                  details={device.connectionDetails}
                  lanUpgradeFailed={device.lanUpgradeFailed}
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
        </ScrollView>

        <BottomActionBar testID="device-detail-action-bar">
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
            // flex-1 不可省:BottomActionBar 是 flex-row,单个子节点不撑满会缩成文字宽度。
            className="min-h-12 flex-1 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
          >
            <Send
              color={
                sendable ? colors.primaryForeground : colors.mutedForeground
              }
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
        </BottomActionBar>

        <AppBottomSheet
          ref={policySheetRef}
          scrollable
          contentTestID="device-policy-sheet"
          contentContainerStyle={policySheetContentStyle}
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
              // 带上当前草稿：用户显式设过的落点由内核带过去（`blocked` 除外）。
              // 在 updater 外算：`defaultReceivePolicy` 会走一次 FFI，而 updater 在
              // StrictMode 下可能被调用两次。它是同步的（uniffi 自由函数），没有竞态。
              setDraftPolicy(defaultReceivePolicy(level, draftPolicy));
            }}
            onPolicyChange={setDraftPolicy}
            onValidityChange={setPolicyValid}
          />
        </AppBottomSheet>

        <DeviceOrganizeSheet ref={organizeSheetRef} />
      </AppScreen>

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
    </>
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
    // 选目录 + 探活走 `receive-location` 的共享判据——此前这里、本次接收的覆盖、以及
    // 全局落点各写一份，连把探活从 `list()` 换成 `exists` 都得改三个文件。
    const picked = await pickDirectory();
    if (picked.outcome === "unusable") toast.error(t`此目录不可读`);
    else if (picked.outcome === "picked") {
      patchPolicy({ defaultSaveLocation: picked.uri });
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

/**
 * footer 尚未 `onLayout` 时的回退高度(dp)——**只影响 present 后的第一帧**,量到真值立刻覆盖。
 *
 * 取「最矮的一档」:pt-3(12) + 保存按钮 48 + gap-2.5(10) + 次级行 44 + 呼吸位 12,
 * 即 `insets.bottom == 0`(Android 全屏隐藏导航)时的高度。宁可小不可大——小了首帧少留一点、
 * 下一帧补上;大了会先撑出一段空白再缩回去,肉眼看得见。
 * **它不是内容底距的事实源**,事实源是量测值;这里不需要跟着 footer 内容精确同步。
 */
const POLICY_FOOTER_FALLBACK_HEIGHT = 126;

function PolicyActionFooter({
  draftLevel,
  savingAction,
  saveDisabled,
  onLayout,
  onSave,
  onBlock,
  onUnblock,
  onUnpair,
}: {
  draftLevel: TrustLevel;
  savingAction: SavingAction;
  saveDisabled?: boolean;
  /** 量测实高,供滚动内容让位用(见 `policySheetContentStyle`)。 */
  onLayout: (event: LayoutChangeEvent) => void;
  onSave: () => void;
  onBlock: () => void;
  onUnblock: () => void;
  onUnpair: () => void;
}) {
  const colors = useThemeColors();
  const blocked = draftLevel === "blocked";
  // 底距 = 系统占用 + 呼吸位,与 `BottomActionBar` 同一个公式(相加,不是取大)。
  // 原先是硬编码 `pb-4`(16dp):这条 footer 的底边就是屏底(sheet 挂在根 Stack 之外的
  // `BottomSheetModalProvider` 里,没有任何安全区 padding),于是 Android 三键导航(48dp)下
  // 「阻止设备 / 取消配对」那 44dp 高的一行有 32dp 落到系统导航栏之下,只剩约 12dp 可点
  // ——远低于 48dp 最小触控目标,而「取消配对」是移动端解除配对的**唯一**入口。
  //
  // 已知取舍:键盘弹起时(高级里的「最大大小」输入框)footer 浮到键盘之上,这段系统占用
  // 就成了多余的空隙。用 `bottomInset` 能让 gorhom 在键盘态自动抹掉它,但代价更大 ——
  // 见 `renderPolicyFooter` 那条注释。空一点可以忍,按钮点不到不行。
  const paddingBottom = useBottomSafePadding();

  return (
    <View
      className="gap-2.5 border-t border-border bg-card px-5 pt-3"
      style={{ paddingBottom }}
      onLayout={onLayout}
    >
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

/**
 * 按设备的保存位置覆盖。**未设置不等于没有落点**——`withHostSaveLocation` 会在这一项为空时
 * 补上全局接收位置（`@/core/receive-location`），所以这里说的是「跟随」而不是某个具体目录。
 *
 * 此前这里写「收件箱」，那是治本前的说法：那时的默认落点是应用私有目录，用户在文件管理器里
 * 根本找不到它，「收件箱」指的是应用内那个列表。现在默认落点本身就是用户可见目录了。
 */
function formatSaveLocation(uri?: string | null): React.ReactNode {
  if (!uri) return <Trans>跟随默认接收位置</Trans>;
  return lastPathSegment(uri) || <Trans>默认位置</Trans>;
}

function formatExpiresAt(expiresAt?: bigint | null): React.ReactNode {
  if (expiresAt == null) return <Trans>不限制</Trans>;
  const ms = Number(expiresAt);
  if (!Number.isFinite(ms) || ms <= 0) return <Trans>不限制</Trans>;
  return new Date(ms).toLocaleString();
}

// 屏级错误兜底:异常只换掉本屏内容,导航栈与 tab 栏保持可用(见 components/app-error-boundary.tsx)
export { AppErrorBoundary as ErrorBoundary } from "@/components/app-error-boundary";
