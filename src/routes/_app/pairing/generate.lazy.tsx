/**
 * Desktop Generate Code Page (Route)
 * 桌面端生成配对码页面
 * Toolbar（← 添加新设备）+ 居中 6 位码展示 + 倒计时 + 取消/复制按钮
 */

import { useEffect, useState, useCallback } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { AlertCircle, Check, Clock, Copy, Link, Loader2, RefreshCw, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { usePairingStore } from "@/stores/pairing-store";
import { useNetworkStore } from "@/stores/network-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { useCountdown } from "@/hooks/use-countdown";
import { formatCountdown } from "@/lib/format";
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

  const { ensureActiveCode, regenerateCode } = usePairingStore(
    useShallow((state) => ({
      ensureActiveCode: state.ensureActiveCode,
      regenerateCode: state.regenerateCode,
    }))
  );

  const codeInfo = usePairingStore((s) => s.activeCode);
  const errorMessage = usePairingStore((s) => s.codeError);
  const nodeStatus = useNetworkStore((s) => s.status);
  const isNodeRunning = nodeStatus === "running";
  const isNodeStarting = nodeStatus === "starting";
  const isNodeUnavailable = !isNodeRunning && !isNodeStarting;
  const isLoading =
    isNodeStarting ||
    (isNodeRunning && codeInfo === null && errorMessage === null);

  const [copied, setCopied] = useState(false);

  // 进入页面时确保有活跃码（store 内部自带过期自动重生 + paired-device-added
  // 后 acceptRequest 触发的重生；离开页面不清状态，下次进来直接是新码）
  useEffect(() => {
    if (isNodeRunning) {
      ensureActiveCode();
    }
  }, [ensureActiveCode, isNodeRunning]);

  // 配对成功后自动跳转到设备页面
  usePairingSuccess();

  // 倒计时
  const { remainingSeconds, isExpired } = useCountdown(codeInfo?.expiresAt ?? null);

  // 复制状态自动重置
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const handleCopy = useCallback(async () => {
    if (!codeInfo) return;
    try {
      await navigator.clipboard.writeText(codeInfo.code);
      setCopied(true);
    } catch {
      toast.error(t`复制失败，请手动复制配对码`);
    }
  }, [codeInfo]);

  const handleBack = () => {
    navigate({ to: "/devices" });
  };

  const codeDigits = codeInfo?.code.split("") ?? [];

  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>添加新设备</Trans>} onBack={handleBack} />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <GlassPanel className="min-h-[420px]">
            <div className="flex h-full flex-col items-center justify-center gap-7 p-6 text-center">
              <div className="glass-control flex size-16 items-center justify-center rounded-[24px] text-blue-600 dark:text-blue-300">
                <Link className="size-7" />
              </div>

              <div>
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>让对方输入这组配对码</Trans>
                </h1>
                <p className="mt-2 text-sm text-muted-foreground">
                  <Trans>配对码只在短时间内有效，过期后可立即重新生成。</Trans>
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
                  <Trans>正在生成配对码</Trans>
                </PairingStatusMessage>
              ) : errorMessage ? (
                <PairingStatusMessage icon={AlertCircle} tone="danger">
                  {errorMessage}
                </PairingStatusMessage>
              ) : (
                <div className="flex items-center gap-2.5">
                  {codeDigits.slice(0, 3).map((digit, i) => (
                    <CodeDigit key={i} digit={digit} />
                  ))}
                  <span className="mx-1 text-2xl font-semibold text-muted-foreground">
                    -
                  </span>
                  {codeDigits.slice(3, 6).map((digit, i) => (
                    <CodeDigit key={i + 3} digit={digit} />
                  ))}
                </div>
              )}

              {codeInfo && (
                <div className="glass-control flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm text-muted-foreground">
                  <Clock className="size-4" />
                  {isExpired ? (
                    <Trans>配对码已过期</Trans>
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
            description={<Trans>另一台设备输入配对码后，本机仍会弹出确认请求。</Trans>}
          >
            <div className="grid content-end gap-2">
              <InfoTile
                label={<Trans>节点状态</Trans>}
                value={
                  isNodeRunning ? (
                    <Trans>正在运行</Trans>
                  ) : isNodeStarting ? (
                    <Trans>启动中</Trans>
                  ) : (
                    <Trans>未启动</Trans>
                  )
                }
              />
              <InfoTile
                label={<Trans>有效期</Trans>}
                value={codeInfo && !isExpired ? formatCountdown(remainingSeconds) : t`待生成`}
              />
            </div>
          </TaskHeroPanel>
        </div>

        <CommandDock>
          <TaskButton variant="outline" onClick={handleBack}>
            <Trans>取消</Trans>
          </TaskButton>
          {isExpired || errorMessage ? (
            <TaskButton onClick={() => regenerateCode()} disabled={!isNodeRunning}>
              <RefreshCw className="size-4" />
              <Trans>重新生成</Trans>
            </TaskButton>
          ) : (
            <TaskButton
              onClick={() => handleCopy()}
              disabled={!isNodeRunning || isLoading || !codeInfo}
            >
              {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
              {copied ? <Trans>已复制</Trans> : <Trans>复制配对码</Trans>}
            </TaskButton>
          )}
        </CommandDock>
      </TaskContent>
    </TaskPageShell>
  );
}

function CodeDigit({ digit }: { digit: string }) {
  return (
    <div className="glass-control flex h-18 w-14 items-center justify-center rounded-[18px] font-mono text-3xl font-semibold text-foreground shadow-[inset_0_1px_0_rgba(255,255,255,0.58)] dark:shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]">
      {digit}
    </div>
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
