import { Trans, useLingui } from "@lingui/react/macro";
import { useRouter } from "expo-router";
import { Bot, Download, FolderOpen } from "lucide-react-native";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Platform,
  Pressable,
  useWindowDimensions,
  View,
} from "react-native";
import {
  type MobileDevice,
  MobileTransferOrigin_Tags,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import { EncryptionNote } from "@/components/encryption-note";
import {
  FileBrowser,
  type FileBrowserItem,
  type FileBrowserListContext,
  fromOfferFiles,
} from "@/components/file-browser";
import { formatBytes } from "@/components/transfer/shared";
import { TrustBadge } from "@/components/trust-badge";
import { AlertDialog, AlertDialogContent } from "@/components/ui/alert-dialog";
import {
  AppBottomSheet,
  type AppBottomSheetRef,
} from "@/components/ui/app-bottom-sheet";
import { Text } from "@/components/ui/text";
import { resolveTrustLevel } from "@/core/device-trust";
import { getMobileCore } from "@/core/mobile-core";
import {
  pickDirectory,
  pickReceiveLocation,
  probeReceiveLocation,
  type ReceiveLocation,
  useReceiveLocation,
} from "@/core/receive-location";
import {
  policyNoteOf,
  type TransferOfferQueueItem,
} from "@/core/transfer-types";
import {
  BOTTOM_BREATHING,
  useBottomSafePadding,
} from "@/hooks/useBottomSafePadding";
import { useThemeColors } from "@/hooks/useThemeColors";
import { getErrorMessage } from "@/lib/errors";
import { toast } from "@/lib/toast";
import { cn, lastPathSegment } from "@/lib/utils";
import {
  summariesToOfflineDevices,
  useMobileCoreStore,
} from "@/stores/mobile-core-store";
import { useTransferStore } from "@/stores/transfer-store";

export function TransferOfferHost() {
  const { t } = useLingui();
  const router = useRouter();
  const { width } = useWindowDimensions();
  const sheetRef = useRef<AppBottomSheetRef>(null);
  const current = useTransferStore((state) => state.currentOffer);
  const dismiss = useTransferStore((state) => state.dismissOffer);
  const loadProjection = useTransferStore((state) => state.loadProjection);
  const setError = useTransferStore((state) => state.setError);
  const { devices, pairedDevicesCache } = useMobileCoreStore(
    useShallow((state) => ({
      devices: state.devices,
      pairedDevicesCache: state.pairedDevicesCache,
    })),
  );
  const [busy, setBusy] = useState<"accepting" | "rejecting" | null>(null);
  const [saveDir, setSaveDir] = useState<string | null>(null);
  /**
   * 接受时探活发现**已配置的**落点访问不了。
   *
   * 只存一个布尔而不是整个 `ReceiveLocation`：那是 store 的一份副本，用户在设置里换了目录
   * 而这张面板还开着时它就陈旧了。「哪个目录」由 `SaveDestinationRow` 自己订阅
   * `useReceiveLocation()` 取当下的值；这里只回答「刚才那次探活过了吗」。
   */
  const [blockedByLocation, setBlockedByLocation] = useState(false);
  const configuredLocation = useReceiveLocation();
  /**
   * 落点此刻能不能用。**判据是当下配置 + 上次探活，不是「有没有被拦下过」**——后者在
   * 「还没点接受、但落点本就没配」时是 false，而那正是最需要把用户领去选目录的时刻。
   */
  const locationUnusable =
    configuredLocation.status !== "ready" || blockedByLocation;
  const isTablet = width >= 768;

  useEffect(() => {
    setSaveDir(null);
    setBlockedByLocation(false);
    setBusy(null);
    if (current && !isTablet) sheetRef.current?.present();
    else sheetRef.current?.dismiss();
  }, [current, isTablet]);

  const pairedDevice = useMemo(() => {
    if (!current) return null;
    return (
      devices.find((device) => device.peerId === current.offer.peerId) ??
      summariesToOfflineDevices(pairedDevicesCache).find(
        (device) => device.peerId === current.offer.peerId,
      ) ??
      null
    );
  }, [current, devices, pairedDevicesCache]);

  const items = useMemo(
    () => (current ? fromOfferFiles(current.id, current.offer.files) : []),
    [current],
  );

  const acceptInto = useCallback(
    async (target: string) => {
      if (!current || busy !== null) return;
      setBusy("accepting");
      try {
        await getMobileCore().acceptReceive(current.id, target);
        const sessionId = current.id;
        await loadProjection(sessionId);
        dismiss(sessionId);
        router.push({
          pathname: "/transfer/[sessionId]",
          params: { sessionId },
        });
      } catch (error) {
        setError(getErrorMessage(error));
      } finally {
        setBusy(null);
      }
    },
    [busy, current, dismiss, loadProjection, router, setError],
  );

  /**
   * 落点缺失/失效时的选择：选的是**全局落点**，因为那正是缺的东西——只作用于本次的话，
   * 下一个 offer 还会被拦在同一处。选中即继续被中断的接受流程。
   */
  const chooseGlobalLocationAndAccept = useCallback(async () => {
    const picked = await pickReceiveLocation();
    if (picked.outcome === "unusable") {
      toast.error(t`此目录不可用`);
      return;
    }
    if (picked.outcome !== "picked") return; // 取消：留在原地，警示行还在
    setBlockedByLocation(false);
    await acceptInto(picked.uri);
  }, [acceptInto, t]);

  // 落点在**接受之前**探活。此前这里是 `saveDir ?? resolveReceiveLocation()`——后者
  // 保证有返回值（回退应用私有目录），于是「目录没配 / 授权失效」只能在写入时才暴露，
  // 表现为接受之后的静默失败。
  const accept = useCallback(async () => {
    if (!current || busy !== null) return;
    // 本次覆盖优先：用户刚在这个面板里挑过目录，就不必再问全局落点。
    if (saveDir) {
      await acceptInto(saveDir);
      return;
    }
    const location = probeReceiveLocation();
    if (location.status !== "ready") {
      setBlockedByLocation(true);
      // 直接把用户送进目录选择——再点一次「接受」本来是最自然的重试动作，而它此前
      // 只会对一个已经是 true 的 state 再 set 一次：React 直接 bail out，界面纹丝不动。
      // 契约要的是「恢复后继续被中断的接受」，不是让用户自己去发现角落里那颗小按钮。
      await chooseGlobalLocationAndAccept();
      return;
    }
    setBlockedByLocation(false);
    await acceptInto(location.uri);
  }, [acceptInto, busy, chooseGlobalLocationAndAccept, current, saveDir]);

  /**
   * 「更改 / 选择」保存位置。同一个按钮，两种语义，由**落点当前能不能用**区分——而不是
   * 由「有没有被拦下过」：那两件事在「还没点过接受、但落点本就没配」时并不一致，而行上
   * 写着的正是「选择」，点下去却只设了本次覆盖，全局落点一直是空的。
   */
  const pickSaveDir = useCallback(async () => {
    if (locationUnusable) {
      await chooseGlobalLocationAndAccept();
      return;
    }
    const picked = await pickDirectory();
    if (picked.outcome === "unusable") toast.error(t`此目录不可读`);
    else if (picked.outcome === "picked") setSaveDir(picked.uri);
  }, [chooseGlobalLocationAndAccept, locationUnusable, t]);

  const reject = useCallback(async () => {
    if (!current || busy !== null) return;
    setBusy("rejecting");
    try {
      await getMobileCore().rejectReceive(current.id);
    } catch (error) {
      console.warn("[transfer-offer-host] reject failed:", error);
      toast.error(t`拒绝接收失败`, error);
    } finally {
      dismiss(current.id);
      setBusy(null);
    }
  }, [busy, current, dismiss, t]);

  if (!current) return null;

  const contentProps: Omit<OfferContentProps, "listContext"> = {
    current,
    pairedDevice,
    items,
    saveDir,
    blockedByLocation,
    busy,
    onPickSaveDir: pickSaveDir,
    onAccept: accept,
    onReject: reject,
    onDismiss: () => dismiss(current.id),
  };

  if (!isTablet) {
    return (
      <AppBottomSheet
        ref={sheetRef}
        virtualized
        snapPoints={["92%"]}
        enablePanDownToClose={false}
        contentTestID="transfer-offer-dialog"
      >
        <OfferContent {...contentProps} listContext="bottom-sheet" />
      </AppBottomSheet>
    );
  }

  return (
    <AlertDialog open>
      <AlertDialogContent
        testID="transfer-offer-dialog"
        style={[
          { borderRadius: 20, borderWidth: 0, height: "82%", width: 680 },
          Platform.OS === "android"
            ? { elevation: 24 }
            : {
                shadowColor: "#000",
                shadowOffset: { width: 0, height: 16 },
                shadowOpacity: 0.2,
                shadowRadius: 40,
              },
        ]}
        className="max-w-none gap-0 bg-card p-0 sm:max-w-none"
      >
        <OfferContent {...contentProps} listContext="screen" />
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** 接收落点里「不能用」的那两态——`ready` 不需要引导，也就不进这个 prop。 */
type UnusableLocation = Exclude<ReceiveLocation, { status: "ready" }>;

interface OfferContentProps {
  current: TransferOfferQueueItem;
  pairedDevice: MobileDevice | null;
  items: FileBrowserItem[];
  saveDir: string | null;
  /** 接受被落点问题拦下过（见 `blockedByLocation` 的定义处）。 */
  blockedByLocation: boolean;
  busy: "accepting" | "rejecting" | null;
  listContext: FileBrowserListContext;
  onPickSaveDir: () => void;
  onAccept: () => void;
  onReject: () => void;
  onDismiss: () => void;
}

function OfferContent({
  current,
  pairedDevice,
  items,
  saveDir,
  blockedByLocation,
  busy,
  listContext,
  onPickSaveDir,
  onAccept,
  onReject,
  onDismiss,
}: OfferContentProps) {
  const rejectedByPolicy = current.offer.policyAction === "reject";

  return (
    <View className="flex-1">
      <OfferHeader
        current={current}
        pairedDevice={pairedDevice}
        fileCount={items.length}
        saveDir={saveDir}
        blockedByLocation={blockedByLocation}
        onPickSaveDir={onPickSaveDir}
      />
      <FileBrowser
        items={items}
        scope="transfer"
        listContext={listContext}
        resetKey={current.id}
        testID="transfer-offer-file-browser"
        title={<Trans>文件</Trans>}
        contentFooter={
          !rejectedByPolicy ? (
            <EncryptionNote className="justify-center py-4">
              <Trans>全程端到端加密，路上没人能看到内容</Trans>
            </EncryptionNote>
          ) : null
        }
      />
      <OfferActions
        rejectedByPolicy={rejectedByPolicy}
        busy={busy}
        safeArea={listContext === "bottom-sheet"}
        onAccept={onAccept}
        onReject={onReject}
        onDismiss={onDismiss}
      />
    </View>
  );
}

function OfferHeader({
  current,
  pairedDevice,
  fileCount,
  saveDir,
  blockedByLocation,
  onPickSaveDir,
}: Pick<
  OfferContentProps,
  "current" | "pairedDevice" | "saveDir" | "blockedByLocation" | "onPickSaveDir"
> & { fileCount: number }) {
  const colors = useThemeColors();
  const rejectedByPolicy = current.offer.policyAction === "reject";
  const trustLevel = pairedDevice ? resolveTrustLevel(pairedDevice) : null;
  const policyNote = policyNoteOf(
    current.offer.policyAction,
    current.offer.policyReason,
  );

  return (
    <View className="gap-4 px-5 pt-4">
      <View className="flex-row items-center gap-3">
        <View className="size-11 items-center justify-center rounded-full bg-primary/10">
          <Download color={colors.primary} size={22} />
        </View>
        <View className="min-w-0 flex-1 gap-0.5">
          <Text className="text-base font-bold text-foreground">
            {rejectedByPolicy ? (
              <Trans>已拒绝文件</Trans>
            ) : (
              <Trans>收到文件</Trans>
            )}
          </Text>
          <Text className="text-[13px] text-muted-foreground" numberOfLines={1}>
            {current.offer.deviceName} · {fileCount} <Trans>个文件</Trans> ·{" "}
            {formatBytes(current.offer.totalSize)}
          </Text>
          {current.offer.origin.tag === MobileTransferOrigin_Tags.Mcp ? (
            <View className="mt-1 flex-row items-center gap-1 self-start rounded-full bg-primary/10 px-2 py-0.5">
              <Bot color={colors.primary} size={12} />
              <Text className="text-[12px] font-medium text-primary-ink">
                {current.offer.origin.inner.client ? (
                  <Trans>
                    由 AI 代理发起（{current.offer.origin.inner.client}）
                  </Trans>
                ) : (
                  <Trans>由 AI 代理发起</Trans>
                )}
              </Text>
            </View>
          ) : null}
        </View>
      </View>

      <View className="gap-2 rounded-xl bg-muted px-3.5 py-3">
        <View className="flex-row items-center justify-between gap-3">
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>设备策略</Trans>
          </Text>
          {trustLevel ? (
            <TrustBadge level={trustLevel} compact />
          ) : (
            <Text className="text-[12px] text-muted-foreground">
              <Trans>未配对</Trans>
            </Text>
          )}
        </View>
        <Text className="text-[13px] text-muted-foreground">
          {policyNote ?? <Trans>此设备需要手动确认后才会开始接收。</Trans>}
        </Text>
      </View>

      {!rejectedByPolicy ? (
        <SaveDestinationRow
          saveDir={saveDir}
          blockedByLocation={blockedByLocation}
          onPick={onPickSaveDir}
        />
      ) : null}
    </View>
  );
}

interface Destination {
  label: React.ReactNode;
  hint: React.ReactNode | null;
  action: React.ReactNode;
}

/** 一个可用落点的呈现。没有 hint——目录名本身就说完了。 */
function readyDestination(uri: string): Destination {
  return {
    label: lastPathSegment(uri),
    hint: null,
    action: <Trans>更改</Trans>,
  };
}

/**
 * 落点不可用时的两种呈现。
 *
 * 无 `default` 分支是刻意的：`ReceiveLocation` 新增状态变体时，这里会在编译期报「并非
 * 所有代码路径都返回值」，而不是悄悄漏掉一种情况让用户对着一个空白的提示发呆。
 */
function issuePresentation(issue: UnusableLocation): Destination {
  switch (issue.status) {
    case "unconfigured":
      return {
        label: <Trans>还没有设置接收位置</Trans>,
        hint: (
          <Trans>收到的文件需要一个你能在系统文件管理器里找到的位置。</Trans>
        ),
        action: <Trans>选择</Trans>,
      };
    case "revoked":
      return {
        label: <Trans>原接收位置不可用</Trans>,
        hint: (
          <Trans>
            可能是目录被删除或授权被撤销。原位置：
            {lastPathSegment(issue.previousUri)}
          </Trans>
        ),
        action: <Trans>重选</Trans>,
      };
  }
}

/**
 * 此刻该报的落点问题；`null` = 没问题。
 *
 * 本次覆盖优先——用户刚在这张面板里挑过一个能用的目录，全局落点是什么已经不影响这次接收。
 */
function locationIssueOf(
  configured: ReceiveLocation,
  blocked: boolean,
  override: string | null,
): UnusableLocation | null {
  if (override) return null;
  if (configured.status !== "ready") return configured;
  // 配置在、探活却没过：那就是「你选的那个目录没了」。`previousUri` 取当下订阅到的值，
  // 而不是探活当时的快照——用户可能已经在设置里换过了。
  return blocked ? { status: "revoked", previousUri: configured.uri } : null;
}

/** 「保存到」区块。 */
function SaveDestinationRow({
  saveDir,
  blockedByLocation,
  onPick,
}: {
  saveDir: string | null;
  /** 接受被落点问题拦下过。具体是哪种问题由本组件结合当下配置判定。 */
  blockedByLocation: boolean;
  onPick: () => void;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const configured = useReceiveLocation();

  const issue = locationIssueOf(configured, blockedByLocation, saveDir);
  // `issue === null` 蕴含「有本次覆盖，或配置就绪」，两者必有其一给得出目录。
  const destination = issue
    ? issuePresentation(issue)
    : readyDestination(
        saveDir ?? (configured.status === "ready" ? configured.uri : ""),
      );
  const warn = issue !== null;

  return (
    <View
      className={cn(
        "flex-row items-center justify-between gap-3 rounded-xl px-3.5 py-3",
        warn ? "bg-destructive/10" : "bg-muted",
      )}
    >
      <View className="min-w-0 flex-1 flex-row items-center gap-2">
        <FolderOpen
          color={warn ? colors.destructive : colors.mutedForeground}
          size={15}
        />
        <View className="min-w-0 flex-1">
          <Text className="text-[12px] text-muted-foreground">
            <Trans>保存到</Trans>
          </Text>
          <Text
            className={cn(
              "text-[13px]",
              warn ? "text-destructive" : "text-foreground",
            )}
            numberOfLines={1}
            testID="transfer-offer-save-destination"
          >
            {destination.label}
          </Text>
          {destination.hint ? (
            <Text
              className="text-[12px] text-muted-foreground"
              numberOfLines={2}
            >
              {destination.hint}
            </Text>
          ) : null}
        </View>
      </View>
      <Pressable
        onPress={onPick}
        accessibilityRole="button"
        accessibilityLabel={t`更改保存位置`}
        testID="transfer-offer-change-save-dir"
        className="min-h-11 items-center justify-center rounded-lg border border-border px-3.5 active:opacity-70"
      >
        <Text className="text-[13px] font-semibold text-foreground">
          {destination.action}
        </Text>
      </Pressable>
    </View>
  );
}

function OfferActions({
  rejectedByPolicy,
  busy,
  safeArea,
  onAccept,
  onReject,
  onDismiss,
}: Pick<OfferContentProps, "busy" | "onAccept" | "onReject" | "onDismiss"> & {
  rejectedByPolicy: boolean;
  safeArea: boolean;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  // 手机走 bottom sheet、直接坐在屏幕底缘 → 底距 = 系统占用 + 呼吸位(相加,见 hook 的注释);
  // 平板走居中浮层,底缘不碰系统条 → 只要呼吸位。两者同一个呼吸位、都与 pt-3 对称。
  const safeAreaPadding = useBottomSafePadding();

  return (
    <View
      className="flex-row gap-2.5 border-t border-border bg-card px-5 pt-3"
      style={{ paddingBottom: safeArea ? safeAreaPadding : BOTTOM_BREATHING }}
    >
      {rejectedByPolicy ? (
        <Pressable
          onPress={onDismiss}
          accessibilityRole="button"
          accessibilityLabel={t`知道了`}
          testID="transfer-offer-dismiss-button"
          className="h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70"
        >
          <Text className="text-base font-medium text-foreground">
            <Trans>知道了</Trans>
          </Text>
        </Pressable>
      ) : (
        <>
          <Pressable
            onPress={onReject}
            disabled={busy !== null}
            accessibilityRole="button"
            accessibilityLabel={t`拒绝`}
            testID="transfer-offer-reject-button"
            className="h-12 flex-1 items-center justify-center rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
          >
            <Text className="text-base font-medium text-foreground">
              <Trans>拒绝</Trans>
            </Text>
          </Pressable>
          <Pressable
            onPress={onAccept}
            disabled={busy !== null}
            accessibilityRole="button"
            accessibilityLabel={t`接收`}
            testID="transfer-offer-accept-button"
            className="h-12 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
          >
            {busy === "accepting" ? (
              <ActivityIndicator color={colors.primaryForeground} />
            ) : (
              <Download color={colors.primaryForeground} size={16} />
            )}
            <Text className="text-base font-semibold text-primary-foreground">
              {busy === "accepting" ? (
                <Trans>接收中...</Trans>
              ) : (
                <Trans>接收</Trans>
              )}
            </Text>
          </Pressable>
        </>
      )}
    </View>
  );
}
