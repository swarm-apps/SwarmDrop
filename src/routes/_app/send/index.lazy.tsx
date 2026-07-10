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
import { Send, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import type { Device } from "@/lib/bindings";
import type { FileSource } from "@/lib/bindings";
import type { PrepareProgress } from "@/lib/types";
import { commands } from "@/lib/bindings";
import { useTransferStore } from "@/stores/transfer-store";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useFileSelection } from "./-use-file-selection";
import { getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import { formatFileSize } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { FileDropZone } from "./-components/file-drop-zone";
import { PrepareProgressBar } from "./-components/prepare-progress-bar";
import { SendProgressView } from "./-components/send-progress-view";
import { FileBrowser } from "@/components/file-browser";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import {
  CommandDock,
  GlassPanel,
  TaskButton,
  TaskContent,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/send/")({
  component: SendPage,
});

function SendPage() {
  const { peerId, session: activeSessionId } = Route.useSearch();
  const navigate = useNavigate();
  const router = useRouter();
  const fileSelection = useFileSelection();
  const [sending, setSending] = useState(false);
  const [prepareProgress, setPrepareProgress] = useState<PrepareProgress | null>(null);
  const loadProjections = useTransferStore((s) => s.loadProjections);

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

      await loadProjections();
      void navigate({
        to: "/send",
        search: { peerId: device.peerId, session: result.sessionId },
        replace: true,
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setSending(false);
      setPrepareProgress(null);
    }
  };

  const setActiveSessionId = (sessionId: string | null) => {
    void navigate({
      to: "/send",
      search: {
        peerId,
        ...(sessionId ? { session: sessionId } : {}),
      },
      replace: true,
    });
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
      <main
        data-testid="send-device-missing"
        className="flex h-full flex-col items-center justify-center gap-3"
      >
        <p className="text-sm text-muted-foreground">
          <Trans>设备未找到</Trans>
        </p>
        <Button variant="outline" onClick={handleBack}>
          <Trans>返回</Trans>
        </Button>
      </main>
    );
  }

  if (activeSessionId) {
    return (
      <SendProgressView
        sessionId={activeSessionId}
        onBack={handleBack}
        onSessionChange={setActiveSessionId}
        onSendMore={() => {
          fileSelection.clear();
          setActiveSessionId(null);
        }}
      />
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
  const view = usePreferencesStore((state) => state.fileBrowserViews.send);
  const setFileBrowserView = usePreferencesStore((state) => state.setFileBrowserView);

  return (
    <TaskPageShell data-testid="send-page">
      <TaskToolbar
        title={<Trans>发送文件到 {deviceDisplayName(device)}</Trans>}
        onBack={onBack}
      />

      <TaskContent
        data-testid="send-content"
        className="flex min-h-0 flex-col gap-4"
        footer={
          prepareProgress ? (
            <CommandDock className="justify-stretch">
              <div className="min-w-0 flex-1 px-2">
                <PrepareProgressBar progress={prepareProgress} />
              </div>
            </CommandDock>
          ) : (
            <CommandDock>
              <TaskButton
                variant="outline"
                onClick={onBack}
                disabled={sending}
                data-testid="send-cancel-action"
              >
                <Trans>取消</Trans>
              </TaskButton>
              <TaskButton
                onClick={onSend}
                disabled={!fileSelection.hasFiles || sending}
                data-testid="send-confirm-action"
              >
                <Send className="size-4" />
                {sending ? <Trans>发送中...</Trans> : <Trans>发送</Trans>}
              </TaskButton>
            </CommandDock>
          )
        }
      >
        {/* 目标设备 mini 摘要条：设备只是信息，让位给文件选择这个主任务 */}
        <div
          data-testid="send-target-summary"
          className="glass-panel flex shrink-0 items-center gap-3 rounded-[20px] px-4 py-3"
        >
          <span className="glass-control flex size-11 shrink-0 items-center justify-center rounded-[16px] text-brand">
            <DeviceIcon className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold text-foreground">
              {deviceDisplayName(device)}
            </p>
            <p className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-muted-foreground">
              <ShieldCheck className="size-3 shrink-0 text-brand/80" />
              <Trans>端到端加密直连</Trans>
            </p>
          </div>
          <div className="shrink-0 text-right">
            <p className="text-[11px] text-muted-foreground">
              <Trans>已选内容</Trans>
            </p>
            <p className="mt-0.5 text-sm font-semibold text-foreground">
              {fileSelection.hasFiles ? (
                <span className="font-mono tabular-nums">
                  {fileSelection.totalCount} 项 · {formatFileSize(fileSelection.totalSize)}
                </span>
              ) : (
                <span className="text-muted-foreground"><Trans>等待选择</Trans></span>
              )}
            </p>
          </div>
        </div>

        {/* 文件选择：空态只保留一个明确的投放区；有内容后再展开补充入口与文件浏览器。 */}
        <GlassPanel
          data-testid="send-file-selection-panel"
          className="min-h-0 flex-1"
        >
          <div className="flex h-full min-h-0 flex-col gap-4 p-4 lg:p-5">
            {fileSelection.hasFiles ? (
              <FileDropZone
                onSourcesSelected={onSourcesSelected}
                disabled={sending}
                compact
                className="shrink-0"
              />
            ) : (
              <div
                data-testid="send-empty-selection"
                className="flex min-h-0 flex-1"
              >
                <FileDropZone
                  onSourcesSelected={onSourcesSelected}
                  disabled={sending}
                  className="flex-1"
                />
              </div>
            )}
            {fileSelection.hasFiles ? (
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                <FileBrowser
                  items={fileSelection.items}
                  title={<Trans>已选文件</Trans>}
                  view={view}
                  onViewChange={(nextView) => setFileBrowserView("send", nextView)}
                  actions={{
                    onRemove: (target) => fileSelection.removeFile(
                      target.type === "directory"
                        ? target.relativePath
                        : target.item.relativePath,
                    ),
                  }}
                />
              </div>
            ) : null}
          </div>
        </GlassPanel>

      </TaskContent>
    </TaskPageShell>
  );
}
