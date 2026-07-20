/**
 * 受邀方屏——粘贴/剪贴板感知邀请串 → 本地解码验签展示确认卡 → 确认后发起配对。
 *
 * 桌面无相机，输入靠粘贴 + 剪贴板感知（窗口 focus 静默读，命中 `sdinvite` 前缀亮一键条，
 * 用户点击才 preview——非全自动，邀请是信任凭证）。
 */

import { useState, useEffect } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { ClipboardCheck, Link, Loader2, ShieldCheck, X } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { usePairingStore } from "@/stores/pairing-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { useClipboardInvite } from "@/hooks/use-clipboard-invite";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { useNetworkStore } from "@/stores/network-store";
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

export const Route = createLazyFileRoute("/_app/pairing/input")({
  component: PairingInputPage,
});

function PairingInputPage() {
  const navigate = useNavigate();

  const { previewInvite, confirmInvite, reset } = usePairingStore(
    useShallow((state) => ({
      previewInvite: state.previewInvite,
      confirmInvite: state.confirmInvite,
      reset: state.reset,
    })),
  );

  const current = usePairingStore((s) => s.current);
  const isNodeRunning = useNetworkStore((s) => s.status === "running");
  const [text, setText] = useState("");

  const { detected, dismiss } = useClipboardInvite(
    isNodeRunning && current.phase === "idle",
  );

  useEffect(() => () => reset(), [reset]);
  usePairingSuccess();

  const handleBack = () => navigate({ to: "/devices" });
  const handleSubmit = (invite: string) => {
    const v = invite.trim();
    if (v.length > 0) previewInvite(v);
  };

  // ─── 确认卡（解码验签后展示对端设备） ───
  if (current.phase === "previewing" || current.phase === "requesting") {
    const preview =
      current.phase === "previewing" ? current.preview : null;
    const isRequesting = current.phase === "requesting";
    const DeviceIcon = getDeviceIcon(preview?.displayPlatform ?? "unknown");
    return (
      <TaskPageShell>
        <TaskToolbar title={<Trans>确认设备</Trans>} onBack={reset} />
        <TaskContent className="flex items-center justify-center">
          <GlassPanel className="w-full max-w-md">
            <div className="flex flex-col items-center gap-6 p-8 text-center">
              <div className="glass-control flex size-20 items-center justify-center rounded-[28px] text-brand">
                <DeviceIcon className="size-9" />
              </div>
              <div>
                <h1 className="text-xl font-semibold text-foreground">
                  {preview?.displayName || <Trans>对方设备</Trans>}
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  {preview?.displayPlatform}
                  {preview && (
                    <>
                      {" · "}
                      <span className="font-mono">{preview.peerId.slice(-8)}</span>
                    </>
                  )}
                </p>
              </div>
              <p className="text-sm text-muted-foreground">
                <Trans>配对后，双方可以互相发送文件。确认发起配对？</Trans>
              </p>
              <div className="flex w-full gap-3">
                <TaskButton variant="outline" className="flex-1" onClick={reset} disabled={isRequesting}>
                  <Trans>取消</Trans>
                </TaskButton>
                <TaskButton className="flex-1" onClick={() => confirmInvite()} disabled={isRequesting}>
                  {isRequesting && <Loader2 className="size-4 animate-spin" />}
                  {isRequesting ? <Trans>配对中...</Trans> : <Trans>确认配对</Trans>}
                </TaskButton>
              </div>
            </div>
          </GlassPanel>
        </TaskContent>
      </TaskPageShell>
    );
  }

  // ─── 粘贴邀请视图 ───
  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>连接已有设备</Trans>} onBack={handleBack} />

      <TaskContent
        className="flex min-h-0 flex-col gap-5"
        footer={
          <CommandDock>
            <TaskButton variant="outline" onClick={handleBack}>
              <Trans>取消</Trans>
            </TaskButton>
            <TaskButton onClick={() => handleSubmit(text)} disabled={text.trim().length === 0}>
              <Trans>继续</Trans>
            </TaskButton>
          </CommandDock>
        }
      >
        {/* 剪贴板感知一键条 */}
        {detected && (
          <button
            type="button"
            onClick={() => {
              handleSubmit(detected);
              dismiss();
            }}
            className="flex items-center gap-2 rounded-[16px] border border-brand/30 bg-brand/10 px-4 py-3 text-left text-sm text-foreground transition hover:bg-brand/15"
          >
            <ClipboardCheck className="size-5 shrink-0 text-brand" />
            <span className="flex-1">
              <Trans>检测到剪贴板中的配对邀请，点此继续</Trans>
            </span>
            <X
              className="size-4 shrink-0 text-muted-foreground hover:text-foreground"
              onClick={(e) => {
                e.stopPropagation();
                dismiss();
              }}
            />
          </button>
        )}

        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <GlassPanel className="min-h-[420px]">
            <div className="flex h-full flex-col items-center justify-center gap-7 p-6 text-center">
              <div className="glass-control flex size-16 items-center justify-center rounded-[24px] text-brand">
                <Link className="size-7" />
              </div>

              <div>
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>粘贴对方的配对邀请</Trans>
                </h1>
                <p className="mt-2 text-sm text-muted-foreground">
                  <Trans>粘贴后本机会验证并显示对端设备，确认后再发起配对。</Trans>
                </p>
              </div>

              <textarea
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="sdinvite..."
                spellCheck={false}
                autoFocus
                className="glass-control h-32 w-full max-w-md resize-none rounded-[18px] p-4 font-mono text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
            </div>
          </GlassPanel>

          <TaskHeroPanel
            icon={ShieldCheck}
            label={<Trans>配对确认</Trans>}
            title={<Trans>先验证，再确认</Trans>}
            description={<Trans>邀请经本地签名验证，真正建立信任仍需要双方确认。</Trans>}
          >
            <div className="grid content-end gap-2">
              <InfoTile
                label={<Trans>输入状态</Trans>}
                value={text.trim().length > 0 ? <Trans>已粘贴</Trans> : <Trans>等待粘贴</Trans>}
              />
            </div>
          </TaskHeroPanel>
        </div>
      </TaskContent>
    </TaskPageShell>
  );
}
