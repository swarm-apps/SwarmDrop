/**
 * 发起方屏——生成一次性签名邀请，展示二维码 + 复制链接 + 倒计时 + 仅本地网络开关。
 * 对方扫码/粘贴此邀请后，本机仍会弹确认请求（安全闸）。
 *
 * 码位是全屏唯一焦点：节点未启动 / 生成中 / 出错 / 过期都以覆盖层压在码面上，
 * 不替换整块内容——状态一眼可见且布局不跳（PRODUCT.md「状态诚实可见」）。
 */

import { useEffect, useState, useCallback, useMemo } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { Check, Clock, Copy, RefreshCw, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { INVITE_TTL_SECS, usePairingStore } from "@/stores/pairing-store";
import { useNetworkStore } from "@/stores/network-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { useCountdown } from "@/hooks/use-countdown";
import { copyText } from "@/lib/clipboard";
import { formatCountdown } from "@/lib/format";
import { cn } from "@/lib/utils";
import { InviteQr, type InviteQrOverlay } from "@/components/pairing/invite-qr";
import { PairingModeTabs } from "@/components/pairing/pairing-mode-tabs";
import { PairingSteps } from "@/components/pairing/pairing-steps";
import { Switch } from "@/components/ui/switch";
import {
  CommandDock,
  GlassPanel,
  TaskButton,
  TaskContent,
  TaskHeroPanel,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/pairing/generate")({
  component: PairingGeneratePage,
});

/** 倒计时进入这个区间就转告警色——「快没了」要先于「已过期」被看见。 */
const EXPIRY_WARNING_SECS = 30;

function PairingGeneratePage() {
  const navigate = useNavigate();

  const { ensureActiveInvite, generateInvite } = usePairingStore(
    useShallow((state) => ({
      ensureActiveInvite: state.ensureActiveInvite,
      generateInvite: state.generateInvite,
    })),
  );

  const activeInvite = usePairingStore((s) => s.activeInvite);
  const errorMessage = usePairingStore((s) => s.inviteError);
  const nodeStatus = useNetworkStore((s) => s.status);
  const isNodeRunning = nodeStatus === "running";
  const isNodeStarting = nodeStatus === "starting";

  const [localOnly, setLocalOnly] = useState(false);
  const [copied, setCopied] = useState(false);

  // 进入页面确保有活跃邀请；切换 localOnly 时重生成
  useEffect(() => {
    if (isNodeRunning) ensureActiveInvite(localOnly);
  }, [ensureActiveInvite, isNodeRunning, localOnly]);

  usePairingSuccess();

  const { remainingSeconds, isExpired } = useCountdown(
    activeInvite ? activeInvite.generatedAt + INVITE_TTL_SECS * 1000 : null,
  );

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const invite = activeInvite?.invite ?? null;
  const handleCopy = useCallback(async () => {
    if (invite === null) return;
    try {
      await copyText(invite);
      setCopied(true);
    } catch {
      toast.error(t`复制失败，请手动复制邀请`);
    }
  }, [invite]);

  const handleRegenerate = useCallback(
    () => generateInvite(localOnly),
    [generateInvite, localOnly],
  );

  const handleBack = () => navigate({ to: "/devices" });

  // 码面覆盖态：优先级 = 节点不可用 > 生成失败 > 已过期
  const qrOverlay = useMemo<InviteQrOverlay | null>(() => {
    if (!isNodeRunning) {
      return isNodeStarting
        ? { kind: "waiting", message: <Trans>等待节点启动</Trans> }
        : { kind: "blocked", message: <Trans>请先启动网络节点</Trans> };
    }
    if (errorMessage !== null) {
      return { kind: "error", message: errorMessage };
    }
    if (isExpired) {
      return { kind: "expired", message: <Trans>邀请已过期</Trans> };
    }
    return null;
  }, [errorMessage, isExpired, isNodeRunning, isNodeStarting]);

  const showCountdown = activeInvite !== null && qrOverlay === null;
  const isExpiringSoon = remainingSeconds <= EXPIRY_WARNING_SECS;

  return (
    <TaskPageShell>
      <TaskToolbar
        title={<Trans>添加设备</Trans>}
        onBack={handleBack}
        trailing={<PairingModeTabs />}
      />

      <TaskContent
        className="flex min-h-0 flex-col gap-5"
        footer={
          <CommandDock>
            <TaskButton variant="outline" onClick={handleBack}>
              <Trans>取消</Trans>
            </TaskButton>
            {isExpired || errorMessage ? (
              <TaskButton onClick={handleRegenerate} disabled={!isNodeRunning}>
                <RefreshCw className="size-4" />
                <Trans>重新生成邀请</Trans>
              </TaskButton>
            ) : (
              <TaskButton onClick={handleCopy} disabled={!isNodeRunning || invite === null}>
                {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
                {copied ? <Trans>已复制</Trans> : <Trans>复制邀请链接</Trans>}
              </TaskButton>
            )}
          </CommandDock>
        }
      >
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <GlassPanel className="min-h-[420px]">
            <div className="flex h-full flex-col items-center justify-center gap-6 p-6 text-center">
              <div>
                <h1 className="text-2xl font-semibold tracking-tight text-balance text-foreground">
                  <Trans>让对方扫码或粘贴此邀请</Trans>
                </h1>
                <p className="mt-2 text-sm text-muted-foreground">
                  <Trans>邀请一次性有效，过期后可立即重新生成。</Trans>
                </p>
              </div>

              <InviteQr
                invite={invite}
                size={240}
                overlay={qrOverlay}
              />

              <div className="h-9">
                {showCountdown && (
                  <div
                    aria-live="polite"
                    className={cn(
                      "glass-control flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm tabular-nums",
                      isExpiringSoon
                        ? "text-amber-700 dark:text-amber-300"
                        : "text-muted-foreground",
                    )}
                  >
                    <Clock className="size-4 shrink-0" />
                    <Trans>将在 {formatCountdown(remainingSeconds)} 后过期</Trans>
                  </div>
                )}
              </div>
            </div>
          </GlassPanel>

          <TaskHeroPanel
            icon={ShieldCheck}
            label={<Trans>安全配对</Trans>}
            title={<Trans>只建立你确认过的连接</Trans>}
            description={<Trans>对方扫码或粘贴邀请后，本机仍会弹出确认请求。</Trans>}
          >
            <div className="flex flex-col gap-5">
              <PairingSteps
                steps={[
                  <Trans key="1">在对方设备上打开 SwarmDrop，进入「添加设备」</Trans>,
                  <Trans key="2">
                    手机上点「扫码」对准这个二维码；电脑上改用「粘贴邀请」
                  </Trans>,
                  <Trans key="3">本机弹出确认请求，你同意后配对完成</Trans>,
                ]}
              />

              <div className="grid gap-1.5">
                <label className="glass-control flex items-center justify-between gap-3 rounded-[18px] px-4 py-3">
                  <div className="text-left">
                    <div className="text-sm font-medium text-foreground">
                      <Trans>仅本地网络</Trans>
                    </div>
                    <div className="text-xs text-muted-foreground">
                      <Trans>只允许同一局域网连接</Trans>
                    </div>
                  </div>
                  <Switch
                    checked={localOnly}
                    onCheckedChange={setLocalOnly}
                    disabled={!isNodeRunning}
                  />
                </label>
                <p className="px-1 text-[11px] leading-4 text-muted-foreground">
                  <Trans>切换后会立即重新生成邀请，此前展示的二维码随即失效。</Trans>
                </p>
              </div>
            </div>
          </TaskHeroPanel>
        </div>
      </TaskContent>
    </TaskPageShell>
  );
}

