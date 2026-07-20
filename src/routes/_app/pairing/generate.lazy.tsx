/**
 * 发起方屏——生成一次性签名邀请，展示二维码 + 复制链接 + 倒计时 + 仅本地网络开关。
 * 对方扫码/粘贴此邀请后，本机仍会弹确认请求（安全闸）。
 */

import { useEffect, useState, useCallback, useMemo } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { AlertCircle, Check, Clock, Copy, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
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
import { InviteQr } from "@/components/pairing/invite-qr";
import { Switch } from "@/components/ui/switch";
import {
  CommandDock,
  GlassPanel,
  InfoTile,
  TaskButton,
  TaskContent,
  TaskHeroPanel,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/pairing/generate")({
  component: PairingGeneratePage,
});

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
  const isNodeUnavailable = !isNodeRunning && !isNodeStarting;
  const isLoading =
    isNodeStarting ||
    (isNodeRunning && activeInvite === null && errorMessage === null);

  const [localOnly, setLocalOnly] = useState(false);
  const [copied, setCopied] = useState(false);

  // 进入页面确保有活跃邀请；切换 localOnly 时重生成
  useEffect(() => {
    if (isNodeRunning) ensureActiveInvite(localOnly);
  }, [ensureActiveInvite, isNodeRunning, localOnly]);

  usePairingSuccess();

  // 倒计时：generatedAt + TTL → ISO 字符串喂 useCountdown
  const expiresAtIso = useMemo(
    () =>
      activeInvite
        ? new Date(activeInvite.generatedAt + INVITE_TTL_SECS * 1000).toISOString()
        : null,
    [activeInvite],
  );
  const { remainingSeconds, isExpired } = useCountdown(expiresAtIso);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const handleCopy = useCallback(async () => {
    if (!activeInvite) return;
    try {
      await copyText(activeInvite.invite);
      setCopied(true);
    } catch {
      toast.error(t`复制失败，请手动复制邀请`);
    }
  }, [activeInvite]);

  const handleBack = () => navigate({ to: "/devices" });

  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>添加新设备</Trans>} onBack={handleBack} />

      <TaskContent
        className="flex min-h-0 flex-col gap-5"
        footer={
          <CommandDock>
            <TaskButton variant="outline" onClick={handleBack}>
              <Trans>取消</Trans>
            </TaskButton>
            {isExpired || errorMessage ? (
              <TaskButton onClick={() => generateInvite(localOnly)} disabled={!isNodeRunning}>
                <RefreshCw className="size-4" />
                <Trans>重新生成邀请</Trans>
              </TaskButton>
            ) : (
              <TaskButton
                onClick={() => handleCopy()}
                disabled={!isNodeRunning || isLoading || !activeInvite}
              >
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
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>让对方扫码或粘贴此邀请</Trans>
                </h1>
                <p className="mt-2 text-sm text-muted-foreground">
                  <Trans>邀请一次性有效，过期后可立即重新生成。</Trans>
                </p>
              </div>

              {isNodeUnavailable ? (
                <PairingStatusMessage icon={AlertCircle}>
                  <Trans>请先启动网络节点</Trans>
                </PairingStatusMessage>
              ) : isNodeStarting ? (
                <PairingStatusMessage icon={Loader2} spinning>
                  <Trans>等待节点启动</Trans>
                </PairingStatusMessage>
              ) : isLoading ? (
                <PairingStatusMessage icon={Loader2} spinning>
                  <Trans>正在生成邀请</Trans>
                </PairingStatusMessage>
              ) : errorMessage ? (
                <PairingStatusMessage icon={AlertCircle} tone="danger">
                  {errorMessage}
                </PairingStatusMessage>
              ) : (
                <InviteQr invite={isExpired ? null : (activeInvite?.invite ?? null)} size={240} />
              )}

              {activeInvite && !isLoading && !errorMessage && (
                <div className="glass-control flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm text-muted-foreground">
                  <Clock className="size-4" />
                  {isExpired ? (
                    <Trans>邀请已过期</Trans>
                  ) : (
                    <Trans>将在 {formatCountdown(remainingSeconds)} 后过期</Trans>
                  )}
                </div>
              )}
            </div>
          </GlassPanel>

          <TaskHeroPanel
            icon={ShieldCheck}
            label={<Trans>安全配对</Trans>}
            title={<Trans>只建立你确认过的连接</Trans>}
            description={<Trans>对方扫码或粘贴邀请后，本机仍会弹出确认请求。</Trans>}
          >
            <div className="grid content-end gap-2">
              <label className="glass-control flex items-center justify-between gap-3 rounded-[18px] px-4 py-3">
                <div className="text-left">
                  <div className="text-sm font-medium text-foreground">
                    <Trans>仅本地网络</Trans>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    <Trans>只允许同一局域网连接</Trans>
                  </div>
                </div>
                <Switch checked={localOnly} onCheckedChange={setLocalOnly} disabled={!isNodeRunning} />
              </label>
              <InfoTile
                label={<Trans>有效期</Trans>}
                value={activeInvite && !isExpired ? formatCountdown(remainingSeconds) : t`待生成`}
              />
            </div>
          </TaskHeroPanel>
        </div>
      </TaskContent>
    </TaskPageShell>
  );
}

function PairingStatusMessage({
  icon: Icon,
  children,
  spinning,
  tone = "muted",
}: {
  icon: React.ComponentType<{ className?: string }>;
  children: React.ReactNode;
  spinning?: boolean;
  tone?: "muted" | "danger";
}) {
  return (
    <div
      className={
        tone === "danger"
          ? "flex h-18 items-center gap-2 rounded-[18px] px-4 text-sm text-destructive"
          : "flex h-18 items-center gap-2 rounded-[18px] px-4 text-sm text-muted-foreground"
      }
    >
      <Icon className={spinning ? "size-5 animate-spin" : "size-5"} />
      {children}
    </div>
  );
}
