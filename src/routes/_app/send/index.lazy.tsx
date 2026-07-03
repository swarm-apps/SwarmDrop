/**
 * Send Page (Lazy)
 * 发送文件页面 — 从设备页面点击发送跳转至此
 */

import { useMemo, useState } from "react";
import {
  createLazyFileRoute,
  useNavigate,
  useRouter,
} from "@tanstack/react-router";
import { Channel } from "@tauri-apps/api/core";
import { FileStack, HardDrive, MonitorSmartphone, Send } from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import type { Device } from "@/lib/bindings";
import type { FileSource } from "@/lib/bindings";
import type { PrepareProgress } from "@/lib/types";
import { commands } from "@/lib/bindings";
import { useTransferStore } from "@/stores/transfer-store";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { useFileSelection } from "./-use-file-selection";
import { getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import { formatFileSize } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { FileDropZone } from "./-components/file-drop-zone";
import { PrepareProgressBar } from "./-components/prepare-progress-bar";
import { FileTree } from "@/components/file-tree";
import { getDeviceIcon } from "@/components/pairing/device-icon";
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

export const Route = createLazyFileRoute("/_app/send/")({
  component: SendPage,
});

function SendPage() {
  const { peerId } = Route.useSearch();
  const navigate = useNavigate();
  const router = useRouter();
  const fileSelection = useFileSelection();
  const [sending, setSending] = useState(false);
  const [prepareProgress, setPrepareProgress] = useState<PrepareProgress | null>(null);

  // 从 network-store / secret-store 查找目标设备
  const onlineDevice = useNetworkStore(
    (s) => s.devices.find((d) => d.peerId === peerId) ?? null,
  );
  const pairedDevices = useSecretStore((s) => s.pairedDevices);

  const device = useMemo<Device | null>(() => {
    if (onlineDevice) return onlineDevice;
    const stored = pairedDevices.find((p) => p.peerId === peerId);
    if (!stored) return null;
    return {
      peerId: stored.peerId,
      name: stored.name,
      hostname: stored.hostname,
      os: stored.os,
      platform: stored.platform,
      arch: stored.arch,
      capabilities: stored.capabilities ?? [],
      status: "offline" as const,
      connection: null,
      latency: null,
      isPaired: true,
      trustLevel: stored.trustLevel ?? "collaborator",
      receivePolicy: stored.receivePolicy ?? null,
      trustConfirmed: stored.trustConfirmed ?? false,
    };
  }, [onlineDevice, pairedDevices, peerId]);

  const handleSourcesSelected = async (sources: FileSource[]) => {
    try {
      await fileSelection.addSources(sources);
    } catch (err) {
      toast.error(getErrorMessage(err));
    }
  };

  const handleSend = async () => {
    if (!device || !fileSelection.hasFiles) return;

    setSending(true);
    setPrepareProgress(null);
    try {
      // 将扫描到的文件列表传给后端计算 hash
      const scannedFiles = fileSelection.getScannedFiles();
      const progressChannel = new Channel<PrepareProgress>();
      progressChannel.onmessage = setPrepareProgress;
      const prepared = await commands.prepareSend(scannedFiles, progressChannel);
      const fileIds = prepared.files.map((f) => f.fileId);
      const displayName = deviceDisplayName(device);
      const result = await commands.startSend(
        prepared.preparedId,
        device.peerId,
        displayName,
        fileIds,
      );

      await useTransferStore.getState().loadProjections();

      navigate({
        to: "/transfer/$sessionId",
        params: { sessionId: result.sessionId },
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setSending(false);
      setPrepareProgress(null);
    }
  };

  const handleBack = () => {
    if (router.history.length > 1) {
      router.history.back();
    } else {
      navigate({ to: "/devices" });
    }
  };

  if (!device) {
    return (
      <main className="flex h-full flex-col items-center justify-center gap-3">
        <p className="text-sm text-muted-foreground">
          <Trans>设备未找到</Trans>
        </p>
        <Button variant="outline" onClick={handleBack}>
          <Trans>返回</Trans>
        </Button>
      </main>
    );
  }

  return (
    <DesktopSendView
      device={device}
      fileSelection={fileSelection}
      sending={sending}
      prepareProgress={prepareProgress}
      onSourcesSelected={handleSourcesSelected}
      onSend={handleSend}
      onBack={handleBack}
    />
  );
}

/* ─────────────────── 共享 Props ─────────────────── */

interface SendViewProps {
  device: Device;
  fileSelection: ReturnType<typeof useFileSelection>;
  sending: boolean;
  prepareProgress: PrepareProgress | null;
  onSourcesSelected: (sources: FileSource[]) => void;
  onSend: () => void;
  onBack: () => void;
}

/* ─────────────────── 桌面端视图 ─────────────────── */

function DesktopSendView({
  device,
  fileSelection,
  sending,
  prepareProgress,
  onSourcesSelected,
  onSend,
  onBack,
}: SendViewProps) {
  const DeviceIcon = getDeviceIcon(device.os || device.platform || "");

  return (
    <TaskPageShell>
      <TaskToolbar
        title={<Trans>发送文件到 {deviceDisplayName(device)}</Trans>}
        onBack={onBack}
      />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[360px_minmax(0,1fr)]">
          <TaskHeroPanel
            icon={MonitorSmartphone}
            label={<Trans>目标设备</Trans>}
            title={deviceDisplayName(device)}
            description={<Trans>确认目标设备在线后，将文件或文件夹拖到右侧面板。</Trans>}
            className="min-h-[320px]"
          >
            <div className="flex h-full flex-col justify-between gap-5">
              <div className="glass-accent flex items-center gap-3 rounded-[22px] p-4">
                <span className="glass-control flex size-13 shrink-0 items-center justify-center rounded-[19px] text-brand">
                  <DeviceIcon className="size-6" />
                </span>
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-foreground">
                    {deviceDisplayName(device)}
                  </p>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground">
                    {device.hostname || device.peerId}
                  </p>
                </div>
              </div>

              <div className="grid gap-2">
                <InfoTile
                  icon={FileStack}
                  label={<Trans>已选内容</Trans>}
                  value={
                    fileSelection.hasFiles ? (
                      <Trans>
                        {fileSelection.totalCount} 项，{formatFileSize(fileSelection.totalSize)}
                      </Trans>
                    ) : (
                      <Trans>等待选择</Trans>
                    )
                  }
                />
                <InfoTile
                  icon={HardDrive}
                  label={<Trans>传输方式</Trans>}
                  value={<Trans>端到端加密直连</Trans>}
                />
              </div>
            </div>
          </TaskHeroPanel>

          <GlassPanel className="min-h-0">
            <div className="flex h-full min-h-0 flex-col gap-4 p-4 lg:p-5">
              <FileDropZone onSourcesSelected={onSourcesSelected} disabled={sending} />
              <div className="min-h-0 flex-1 overflow-hidden">
                {fileSelection.hasFiles ? (
                  <FileTree
                    mode="select"
                    dataLoader={fileSelection.dataLoader}
                    rootChildren={fileSelection.rootChildren}
                    totalCount={fileSelection.totalCount}
                    totalSize={fileSelection.totalSize}
                    onRemoveFile={fileSelection.removeFile}
                  />
                ) : (
                  <div className="flex h-full min-h-[180px] items-center justify-center rounded-[20px] bg-foreground/[0.025] text-center dark:bg-white/[0.035]">
                    <p className="max-w-[28ch] text-sm leading-6 text-muted-foreground">
                      <Trans>选择内容后，文件结构和总大小会在这里确认。</Trans>
                    </p>
                  </div>
                )}
              </div>
            </div>
          </GlassPanel>
        </div>

        {prepareProgress ? (
          <CommandDock className="justify-stretch">
            <div className="min-w-0 flex-1 px-2">
              <PrepareProgressBar progress={prepareProgress} />
            </div>
          </CommandDock>
        ) : (
          <CommandDock>
            <TaskButton variant="outline" onClick={onBack} disabled={sending}>
              <Trans>取消</Trans>
            </TaskButton>
            <TaskButton
              onClick={onSend}
              disabled={!fileSelection.hasFiles || sending}
            >
              <Send className="size-4" />
              {sending ? <Trans>发送中...</Trans> : <Trans>发送</Trans>}
            </TaskButton>
          </CommandDock>
        )}
      </TaskContent>
    </TaskPageShell>
  );
}

