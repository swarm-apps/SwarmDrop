/**
 * 受邀方屏——粘贴/剪贴板感知邀请串 → 本地解码验签展示确认卡 → 确认后发起配对。
 *
 * 桌面无相机，输入靠粘贴。剪贴板感知已提到 `_app` 布局的 `ClipboardInviteBanner`（全局，
 * 且会过滤掉本机自己发出的邀请）——本页只保留手动粘贴这条路。
 */

import { useState, useEffect } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { Link, Loader2, ShieldCheck } from "lucide-react";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { usePairingStore } from "@/stores/pairing-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { PairingModeTabs } from "@/components/pairing/pairing-mode-tabs";
import { PairingSteps } from "@/components/pairing/pairing-steps";
import {
  CommandDock,
  GlassPanel,
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
  const [text, setText] = useState("");

  // 剪贴板感知不在本页挂：它已提到 `_app` 布局的 `ClipboardInviteBanner`（全局）。
  // 这里再挂一份会让两个 hook 实例各读一次剪贴板、各记一份「已提示过」，同一条邀请亮两次。

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
      <TaskToolbar
        title={<Trans>添加设备</Trans>}
        onBack={handleBack}
        trailing={<PairingModeTabs />}
      />

      <TaskContent
        className="flex flex-col gap-5"
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
        {/* 断点与列序同 generate 页（理由写在那边）：920 分栏、说明在左、输入在右；
            窄屏堆叠时输入框在上，先给用户可操作的那块。 */}
        <div className="grid min-h-0 flex-1 gap-5 min-[920px]:grid-cols-[360px_minmax(0,1fr)] lg:grid-cols-[380px_minmax(0,1fr)]">
          <GlassPanel className="min-h-[420px] min-[920px]:order-2">
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
                placeholder="https://swarm-apps.github.io/SwarmDrop/p/#..."
                spellCheck={false}
                autoFocus
                className="glass-control h-32 w-full max-w-md resize-none rounded-[18px] p-4 font-mono text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
            </div>
          </GlassPanel>

          <TaskHeroPanel
            className="min-[920px]:order-1"
            icon={ShieldCheck}
            label={<Trans>配对确认</Trans>}
            title={<Trans>先验证，再确认</Trans>}
            description={<Trans>邀请经本地签名验证，真正建立信任仍需要双方确认。</Trans>}
          >
            <div className="flex flex-col gap-4">
              <PairingSteps
                steps={[
                  <Trans key="1">
                    在对方设备上打开 SwarmDrop，进入「添加设备 → 展示邀请」
                  </Trans>,
                  <Trans key="2">
                    让对方点「复制邀请链接」，把那条链接发给你
                  </Trans>,
                  <Trans key="3">粘贴到左侧，验证通过后确认发起配对</Trans>,
                ]}
              />
              <p className="px-1 text-[11px] leading-4 text-muted-foreground">
                <Trans>
                  手机上装了 SwarmDrop 的话，直接用手机扫对方的二维码更快。
                </Trans>
              </p>
            </div>
          </TaskHeroPanel>
        </div>
      </TaskContent>
    </TaskPageShell>
  );
}
