/**
 * Desktop Input Code Page (Route)
 * 桌面端输入配对码页面
 * - 输入阶段：Toolbar（← 连接已有设备）+ 居中 OTP 输入 + 取消/确认
 * - 设备详情：Toolbar（← 设备详情）+ 居中设备信息卡片 + 取消/发送配对请求
 */

import { useState, useEffect } from "react";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { Keyboard, Link, Loader2, ShieldCheck } from "lucide-react";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
  InputOTPSeparator,
} from "@/components/ui/input-otp";
import { Trans } from "@lingui/react/macro";
import { useShallow } from "zustand/react/shallow";
import { usePairingStore } from "@/stores/pairing-store";
import { usePairingSuccess } from "@/hooks/use-pairing-success";
import {
  DesktopDeviceFoundContent,
  useDeviceFoundState,
} from "@/routes/_app/pairing/-device-found-view";
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

  const { searchDevice, sendPairingRequest, openInput, reset } =
    usePairingStore(
      useShallow((state) => ({
        searchDevice: state.searchDevice,
        sendPairingRequest: state.sendPairingRequest,
        openInput: state.openInput,
        reset: state.reset,
      }))
    );

  const [code, setCode] = useState("");

  const { showDeviceFound, deviceInfo, isRequesting } = useDeviceFoundState();

  // 进入页面时初始化输入状态
  useEffect(() => {
    openInput();
    return () => {
      reset();
    };
  }, [openInput, reset]);

  // 配对成功后自动跳转到设备页面
  usePairingSuccess();

  const isSearching = usePairingStore((s) => s.current.phase === "searching");

  const handleCodeComplete = (value: string) => {
    if (value.length === 6) {
      searchDevice(value);
    }
  };

  const handleConfirm = () => {
    if (code.length === 6) {
      searchDevice(code);
    }
  };

  const handleBack = () => {
    navigate({ to: "/devices" });
  };

  // ─── 设备详情视图 ───
  if (showDeviceFound && deviceInfo) {
    return (
      <TaskPageShell>
        <TaskToolbar title={<Trans>设备详情</Trans>} onBack={reset} />
        <TaskContent className="flex items-center justify-center">
          <GlassPanel className="w-full max-w-md">
            <div className="p-6">
              <DesktopDeviceFoundContent
                deviceInfo={deviceInfo}
                isRequesting={isRequesting}
                onSendRequest={() => sendPairingRequest()}
                onCancel={reset}
              />
            </div>
          </GlassPanel>
        </TaskContent>
      </TaskPageShell>
    );
  }

  // ─── 输入配对码视图 ───
  return (
    <TaskPageShell>
      <TaskToolbar title={<Trans>连接已有设备</Trans>} onBack={handleBack} />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <GlassPanel className="min-h-[420px]">
            <div className="flex h-full flex-col items-center justify-center gap-7 p-6 text-center">
              <div className="glass-control flex size-16 items-center justify-center rounded-[24px] text-brand">
                <Link className="size-7" />
              </div>

              <div>
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>输入另一台设备的配对码</Trans>
                </h1>
                <p className="mt-2 text-sm text-muted-foreground">
                  <Trans>找到设备后，你可以确认系统信息再发送配对请求。</Trans>
                </p>
              </div>

              <InputOTP
                maxLength={6}
                value={code}
                onChange={setCode}
                onComplete={handleCodeComplete}
                disabled={isSearching}
                autoFocus
              >
                <InputOTPGroup>
                  <InputOTPSlot index={0} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                  <InputOTPSlot index={1} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                  <InputOTPSlot index={2} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                </InputOTPGroup>
                <InputOTPSeparator />
                <InputOTPGroup>
                  <InputOTPSlot index={3} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                  <InputOTPSlot index={4} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                  <InputOTPSlot index={5} className="h-16 w-13 rounded-[16px] text-2xl font-semibold" />
                </InputOTPGroup>
              </InputOTP>

              {isSearching && (
                <div className="glass-control flex items-center gap-2 rounded-full px-3 py-1.5 text-sm text-muted-foreground">
                  <Loader2 className="size-4 animate-spin" />
                  <Trans>正在查找设备...</Trans>
                </div>
              )}
            </div>
          </GlassPanel>

          <TaskHeroPanel
            icon={ShieldCheck}
            label={<Trans>配对确认</Trans>}
            title={<Trans>先查找，再确认</Trans>}
            description={<Trans>配对码只用于定位设备，真正建立信任仍需要双方确认。</Trans>}
          >
            <div className="grid content-end gap-2">
              <InfoTile
                icon={Keyboard}
                label={<Trans>输入进度</Trans>}
                value={<Trans>{code.length}/6 位</Trans>}
              />
              <InfoTile
                label={<Trans>当前状态</Trans>}
                value={isSearching ? <Trans>查找中</Trans> : <Trans>等待输入</Trans>}
              />
            </div>
          </TaskHeroPanel>
        </div>

        <CommandDock>
          <TaskButton variant="outline" onClick={handleBack}>
            <Trans>取消</Trans>
          </TaskButton>
          <TaskButton
            onClick={handleConfirm}
            disabled={code.length < 6 || isSearching}
          >
            <Trans>确认</Trans>
          </TaskButton>
        </CommandDock>
      </TaskContent>
    </TaskPageShell>
  );
}
