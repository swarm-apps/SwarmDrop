/**
 * 邀请交换区——「添加设备」sheet 里的配对主体。
 *
 * 两个方向（展示本机邀请 / 扫对方的码）是同一件事的两面，用一个分段控件在**同一个
 * sheet 内**切换；此前「粘贴邀请」要先收起 A sheet 再弹 B sheet，多一次开合动画、
 * 还得靠 onDismiss 串接，用户也丢失了「我刚才在配对」的上下文。
 *
 * 相机屏是全屏 push（`/pairing/scan`），push 前必须先 dismiss sheet：sheet 仍 present
 * 时 push 新屏，其返回键监听器会跨屏残留、吞掉新屏第一次返回。
 */

import { BottomSheetTextInput } from "@gorhom/bottom-sheet";
import { Trans, useLingui } from "@lingui/react/macro";
import * as Clipboard from "expo-clipboard";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";
import {
  ClipboardPaste,
  Copy,
  QrCode,
  RefreshCcw,
  ScanLine,
} from "lucide-react-native";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, View } from "react-native";
import { SegmentedControl } from "@/components/mobile/screen";
import { InviteQr, type InviteQrOverlay } from "@/components/pairing/invite-qr";
import { Text } from "@/components/ui/text";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { useThemeColors } from "@/hooks/useThemeColors";
import { toast } from "@/lib/toast";
import { cn } from "@/lib/utils";
import {
  INVITE_TTL_SECS,
  usePairingInviteStore,
} from "@/stores/pairing-invite-store";

/** 倒计时进入这个区间就转告警色——「快没了」要先于「已过期」被看见。 */
const EXPIRY_WARNING_SECS = 30;

type InviteMode = "show" | "scan";

export function InviteExchange({
  onBeforeLeaveSheet,
}: {
  /** 离开 sheet（push 相机屏 / 邀请验签通过跳确认页）前收起 sheet。 */
  onBeforeLeaveSheet: () => void;
}) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const router = useRouter();
  const [mode, setMode] = useState<InviteMode>("show");

  const openScanner = useCallback(() => {
    onBeforeLeaveSheet();
    router.push({ pathname: "/pairing/scan" });
  }, [onBeforeLeaveSheet, router]);

  return (
    <View className="gap-3">
      <SegmentedControl<InviteMode>
        variant="tabs"
        value={mode}
        onChange={setMode}
        testID="invite-mode-control"
        options={[
          {
            value: "show",
            label: t`展示我的邀请`,
            icon: QrCode,
            testID: "invite-mode-show",
          },
          {
            value: "scan",
            label: t`扫码或粘贴`,
            icon: ScanLine,
            testID: "invite-mode-scan",
          },
        ]}
      />

      {mode === "show" ? (
        <InviteCard />
      ) : (
        <View className="gap-3">
          <Pressable
            onPress={openScanner}
            accessibilityRole="button"
            testID="devices-open-scanner-button"
            className="min-h-12 flex-row items-center justify-center gap-2 rounded-xl bg-primary active:opacity-70"
          >
            <ScanLine color={colors.primaryForeground} size={18} />
            <Text className="text-base font-semibold text-primary-foreground">
              <Trans>扫描对方的二维码</Trans>
            </Text>
          </Pressable>

          <View className="flex-row items-center gap-3">
            <View className="h-px flex-1 bg-border" />
            <Text className="text-xs text-muted-foreground">
              <Trans>或</Trans>
            </Text>
            <View className="h-px flex-1 bg-border" />
          </View>

          <PasteInviteInput onResolved={onBeforeLeaveSheet} />
        </View>
      )}
    </View>
  );
}

/** 本机邀请：码面 + 有效期 + 单一主动作（有效时复制，失效时重新生成）。 */
function InviteCard() {
  const { t } = useLingui();
  const colors = useThemeColors();
  const activeInvite = usePairingInviteStore((s) => s.activeInvite);
  const generating = usePairingInviteStore((s) => s.generating);
  const error = usePairingInviteStore((s) => s.error);
  const ensureActiveInvite = usePairingInviteStore((s) => s.ensureActiveInvite);
  const generateInvite = usePairingInviteStore((s) => s.generateInvite);

  useEffect(() => {
    void ensureActiveInvite();
  }, [ensureActiveInvite]);

  const remaining = useExpiresCountdown(
    activeInvite ? activeInvite.generatedAt + INVITE_TTL_SECS * 1000 : null,
  );
  const invite = activeInvite?.invite ?? null;
  const isExpired = invite !== null && remaining <= 0;

  const localOnly = activeInvite?.localOnly ?? false;
  const regenerate = useCallback(
    () => void generateInvite(localOnly),
    [localOnly, generateInvite],
  );

  const handleCopy = async () => {
    if (!invite) return;
    await Clipboard.setStringAsync(invite);
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(() => {});
    toast.success(t`已复制邀请链接`);
  };

  // 码面覆盖态：优先级 = 生成失败 > 已过期 > 还没有邀请。动作一律交给下方主按钮
  // （拇指区），码面只负责说清「为什么现在扫不了」。
  const overlay = useMemo<InviteQrOverlay | null>(() => {
    if (generating) return null;
    if (error !== null) return { kind: "error", message: error };
    if (isExpired) return { kind: "expired", message: t`邀请已过期` };
    if (invite === null) return { kind: "blocked", message: t`尚未生成邀请` };
    return null;
  }, [generating, error, isExpired, invite, t]);

  const isUsable = !generating && overlay === null;

  return (
    <View
      className="items-center gap-3 rounded-lg bg-primary/5 p-3.5"
      testID="devices-local-code"
    >
      <View className="w-full flex-row items-start justify-between gap-3">
        <View className="min-w-0 flex-1">
          <Text className="text-[13px] font-semibold text-foreground">
            <Trans>本机配对邀请</Trans>
          </Text>
          <Text className="mt-0.5 text-[12px] text-muted-foreground">
            <Trans>让另一台设备扫码或粘贴此邀请</Trans>
          </Text>
        </View>
        {isUsable ? (
          <Text
            className={cn(
              "rounded-full bg-card px-2 py-1 text-[12px] tabular-nums",
              remaining <= EXPIRY_WARNING_SECS
                ? "text-warning-ink"
                : "text-muted-foreground",
            )}
          >
            {t`${formatMmss(remaining)} 后过期`}
          </Text>
        ) : null}
      </View>

      <InviteQr
        invite={generating ? null : invite}
        size={220}
        overlay={overlay}
      />

      <View className="w-full flex-row gap-2">
        {isUsable ? (
          <>
            <Pressable
              onPress={regenerate}
              accessibilityRole="button"
              className="min-h-11 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl border border-border bg-card active:opacity-70"
            >
              <RefreshCcw color={colors.foreground} size={14} />
              <Text className="text-[13px] font-semibold text-foreground">
                <Trans>刷新</Trans>
              </Text>
            </Pressable>
            <Pressable
              onPress={handleCopy}
              accessibilityRole="button"
              className="min-h-11 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70"
            >
              <Copy color={colors.primaryForeground} size={14} />
              <Text className="text-[13px] font-semibold text-primary-foreground">
                <Trans>复制链接</Trans>
              </Text>
            </Pressable>
          </>
        ) : (
          <Pressable
            onPress={regenerate}
            disabled={generating}
            accessibilityRole="button"
            testID="devices-regenerate-invite-button"
            className="min-h-11 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:opacity-50"
          >
            {generating ? (
              <ActivityIndicator color={colors.primaryForeground} />
            ) : (
              <>
                <RefreshCcw color={colors.primaryForeground} size={14} />
                <Text className="text-[13px] font-semibold text-primary-foreground">
                  {invite === null ? (
                    <Trans>生成邀请</Trans>
                  ) : (
                    <Trans>重新生成邀请</Trans>
                  )}
                </Text>
              </>
            )}
          </Pressable>
        )}
      </View>
    </View>
  );
}

function PasteInviteInput({ onResolved }: { onResolved: () => void }) {
  const router = useRouter();
  const { t } = useLingui();
  const colors = useThemeColors();
  const previewInvite = usePairingInviteStore((s) => s.previewInvite);
  const [text, setText] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasClip, setHasClip] = useState(false);

  // 剪贴板感知：sheet 打开即挂载（gorhom modal present 时挂 children），用
  // hasStringAsync 探测——只问「有没有字符串」，不读内容、不触发 iOS 粘贴横幅，
  // 有内容就亮一枚 chip 引导一键粘贴。
  useEffect(() => {
    let alive = true;
    void Clipboard.hasStringAsync().then((has) => {
      if (alive) setHasClip(has);
    });
    return () => {
      alive = false;
    };
  }, []);

  const submit = async (raw: string) => {
    const v = raw.trim();
    if (working || v.length === 0) return;
    setError(null);
    setWorking(true);
    const ok = await previewInvite(v);
    setWorking(false);
    if (ok) {
      onResolved();
      router.push({ pathname: "/pairing/found-device" });
    } else {
      setError(t`邀请无效或已过期`);
    }
  };

  const pasteFromClipboard = async () => {
    const clip = (await Clipboard.getStringAsync()).trim();
    if (clip.length > 0) {
      setText(clip);
      await submit(clip);
    }
  };

  return (
    <View className="gap-3">
      {hasClip && text.length === 0 ? (
        <Pressable
          onPress={pasteFromClipboard}
          disabled={working}
          accessibilityRole="button"
          className="min-h-9 flex-row items-center gap-1.5 self-start rounded-full border border-primary/30 bg-primary/10 px-3 active:opacity-70 disabled:opacity-50"
        >
          <ClipboardPaste color={colors.primary} size={14} />
          <Text className="text-[13px] font-medium text-primary-ink">
            <Trans>剪贴板里有内容，点此粘贴</Trans>
          </Text>
        </Pressable>
      ) : null}
      <BottomSheetTextInput
        value={text}
        onChangeText={setText}
        placeholder="sdinvite..."
        placeholderTextColor={colors.mutedForeground}
        autoCapitalize="none"
        autoCorrect={false}
        multiline
        accessibilityLabel={t`粘贴配对邀请`}
        className="min-h-24 rounded-lg border border-border bg-card p-3 font-mono text-[13px] text-foreground"
      />
      {error !== null ? (
        <Text className="text-[13px] text-destructive-ink">{error}</Text>
      ) : null}
      <View className="flex-row gap-2">
        <Pressable
          onPress={pasteFromClipboard}
          disabled={working}
          accessibilityRole="button"
          className="min-h-11 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl border border-border bg-card active:opacity-70 disabled:opacity-50"
        >
          <ClipboardPaste color={colors.foreground} size={16} />
          <Text className="text-[14px] font-semibold text-foreground">
            <Trans>粘贴</Trans>
          </Text>
        </Pressable>
        <Pressable
          onPress={() => void submit(text)}
          disabled={working || text.trim().length === 0}
          accessibilityRole="button"
          className="min-h-11 flex-1 flex-row items-center justify-center gap-1.5 rounded-xl bg-primary active:opacity-70 disabled:bg-muted"
        >
          {working ? (
            <ActivityIndicator color={colors.primaryForeground} />
          ) : (
            <Text className="text-[14px] font-semibold text-primary-foreground">
              <Trans>继续</Trans>
            </Text>
          )}
        </Pressable>
      </View>
    </View>
  );
}

function formatMmss(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
