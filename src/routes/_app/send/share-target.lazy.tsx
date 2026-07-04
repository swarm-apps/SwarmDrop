/**
 * Share Target Page (Lazy)
 *
 * 反向发送屏 —— 文件已定、只需选设备。入口：外部「用 SwarmDrop 打开」文件/文件夹
 * → 根级 ExternalOpenHandler 包装成 FileSource[] 塞进 share-store → 跳到这里。
 * 镜像 `/send`（设备优先）的双栏，但角色对调：左=待发文件汇总，右=在线可发送设备单选。
 * 复用 scan_sources → prepare_send → start_send 链（后端零改动）。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { createLazyFileRoute, useNavigate, useRouter } from "@tanstack/react-router";
import { Channel } from "@tauri-apps/api/core";
import {
  Check,
  FileStack,
  HardDrive,
  Inbox,
  Loader2,
  MonitorSmartphone,
  Send,
} from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import type { Device } from "@/lib/bindings";
import type { PrepareProgress } from "@/lib/types";
import { commands } from "@/lib/bindings";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { useShareStore } from "@/stores/share-store";
import { useTransferStore } from "@/stores/transfer-store";
import { useFileSelection } from "./-use-file-selection";
import { getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import { formatFileSize } from "@/lib/format";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { FileTree } from "@/components/file-tree";
import { PrepareProgressBar } from "./-components/prepare-progress-bar";
import { SendProgressView } from "./-components/send-progress-view";
import { cn } from "@/lib/utils";
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

export const Route = createLazyFileRoute("/_app/send/share-target")({
  component: ShareTargetPage,
});

function ShareTargetPage() {
  const navigate = useNavigate();
  const router = useRouter();
  const fileSelection = useFileSelection();
  const { addSources } = fileSelection;

  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [prepareProgress, setPrepareProgress] = useState<PrepareProgress | null>(null);
  // startSend 成功后就地转进度视图（右键快捷发送全程单界面，发完即可关窗）
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);

  const status = useNetworkStore((s) => s.status);
  const devices = useNetworkStore((s) => s.devices);
  const pairedDevices = useSecretStore((s) => s.pairedDevices);

  // 消费在途来源：mount 时一次性 consume（取走即清空）→ 扫描。addSources 稳定
  // （useCallback []），consume 清空后即便重入也自然 no-op，无需额外 guard。
  useEffect(() => {
    const sources = useShareStore.getState().consume();
    if (sources.length > 0) {
      void addSources(sources).catch((err) => toast.error(getErrorMessage(err)));
    }
  }, [addSources]);

  // 节点未启动时自动启动一次（外部打开常处于冷启动、节点还没起）。
  const startedRef = useRef(false);
  useEffect(() => {
    if (!startedRef.current && status === "stopped") {
      startedRef.current = true;
      void useNetworkStore.getState().startNetwork();
    }
  }, [status]);

  const nodeRunning = status === "running";

  // 在线且已配对（可发送）的设备。信任/接收策略只影响「接收」，故发送目标 = 在线 + 已配对；
  // DeviceOption 与发送只读 os/connection/latency/peerId/displayName，无需归并 trust 等字段。
  const targetDevices = useMemo<Device[]>(() => {
    const pairedIds = new Set(pairedDevices.map((d) => d.peerId));
    return devices
      .filter((d) => d.status === "online" && (d.isPaired || pairedIds.has(d.peerId)))
      .sort((a, b) => deviceDisplayName(a).localeCompare(deviceDisplayName(b)));
  }, [devices, pairedDevices]);

  // 选中设备掉线时无需显式重置：selectedDevice 由下面的 find 派生，设备不在列表即为 null，
  // dock 自动回落「选择一个设备」、canSend 自动为 false。
  const selectedDevice = targetDevices.find((d) => d.peerId === selectedPeerId) ?? null;
  const canSend = !sending && fileSelection.hasFiles && selectedDevice !== null;

  const handleSend = async () => {
    if (!selectedDevice || sending || !fileSelection.hasFiles) return;
    setSending(true);
    setPrepareProgress(null);
    try {
      const scannedFiles = fileSelection.getScannedFiles();
      const progressChannel = new Channel<PrepareProgress>();
      progressChannel.onmessage = setPrepareProgress;
      const prepared = await commands.prepareSend(scannedFiles, progressChannel);
      const fileIds = prepared.files.map((f) => f.fileId);
      const result = await commands.startSend(
        prepared.preparedId,
        selectedDevice.peerId,
        deviceDisplayName(selectedDevice),
        fileIds,
      );
      await useTransferStore.getState().loadProjections();
      setActiveSessionId(result.sessionId);
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

  if (activeSessionId) {
    return (
      <SendProgressView
        sessionId={activeSessionId}
        onBack={handleBack}
        onSessionChange={setActiveSessionId}
      />
    );
  }

  return (
    <TaskPageShell>
      <TaskToolbar
        title={
          fileSelection.hasFiles ? (
            <Trans>发送 {fileSelection.totalCount} 个文件</Trans>
          ) : (
            <Trans>快捷发送</Trans>
          )
        }
        onBack={handleBack}
      />

      <TaskContent className="flex min-h-0 flex-col gap-5">
        <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[360px_minmax(0,1fr)]">
          {/* 左：待发文件 */}
          <TaskHeroPanel
            icon={FileStack}
            label={<Trans>待发送</Trans>}
            title={
              fileSelection.hasFiles ? (
                <Trans>{fileSelection.totalCount} 项内容</Trans>
              ) : (
                <Trans>准备中</Trans>
              )
            }
            description={<Trans>选择右侧一台在线设备，端到端加密直接送达。</Trans>}
            className="min-h-[320px]"
          >
            <div className="flex h-full min-h-0 flex-col gap-4">
              <InfoTile
                icon={HardDrive}
                label={<Trans>合计</Trans>}
                value={
                  fileSelection.hasFiles ? (
                    <span className="font-mono tabular-nums">
                      {formatFileSize(fileSelection.totalSize)}
                    </span>
                  ) : (
                    <Trans>—</Trans>
                  )
                }
              />
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
                  <div className="flex h-full min-h-[160px] items-center justify-center rounded-[20px] bg-foreground/[0.025] px-4 text-center dark:bg-white/[0.035]">
                    <p className="max-w-[26ch] text-sm leading-6 text-muted-foreground">
                      <Trans>文件已全部移除，返回重新分享。</Trans>
                    </p>
                  </div>
                )}
              </div>
            </div>
          </TaskHeroPanel>

          {/* 右：选设备 */}
          <GlassPanel className="min-h-0">
            <div className="flex h-full min-h-0 flex-col gap-4 p-4 lg:p-5">
              <div className="flex items-center gap-2 text-[13px] font-semibold text-muted-foreground">
                <MonitorSmartphone className="size-4" />
                <Trans>选择设备</Trans>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                {!nodeRunning ? (
                  <StartingNodePlaceholder />
                ) : targetDevices.length === 0 ? (
                  <EmptyDevices />
                ) : (
                  <div className="flex flex-col gap-2">
                    {targetDevices.map((device) => (
                      <DeviceOption
                        key={device.peerId}
                        device={device}
                        selected={device.peerId === selectedPeerId}
                        onSelect={() => setSelectedPeerId(device.peerId)}
                      />
                    ))}
                  </div>
                )}
              </div>
            </div>
          </GlassPanel>
        </div>

        {/* 底部命令栏 */}
        {sending && prepareProgress ? (
          <CommandDock className="justify-stretch">
            <div className="min-w-0 flex-1 px-2">
              <PrepareProgressBar progress={prepareProgress} />
            </div>
          </CommandDock>
        ) : (
          <CommandDock>
            <TaskButton variant="outline" onClick={handleBack} disabled={sending}>
              <Trans>取消</Trans>
            </TaskButton>
            <TaskButton onClick={handleSend} disabled={!canSend}>
              <Send className="size-4" />
              {sending ? (
                <Trans>准备中…</Trans>
              ) : selectedDevice ? (
                <Trans>发送给 {deviceDisplayName(selectedDevice)}</Trans>
              ) : (
                <Trans>选择一个设备</Trans>
              )}
            </TaskButton>
          </CommandDock>
        )}
      </TaskContent>
    </TaskPageShell>
  );
}

/* ─────────────────── 设备选项行 ─────────────────── */

function DeviceOption({
  device,
  selected,
  onSelect,
}: {
  device: Device;
  selected: boolean;
  onSelect: () => void;
}) {
  const DeviceIcon = getDeviceIcon(device.os);
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      className={cn(
        "flex min-h-[60px] items-center gap-3 rounded-[18px] p-3 text-left transition-[background-color,box-shadow,transform] duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] active:scale-[0.99]",
        selected
          ? "glass-accent shadow-[0_10px_24px_rgba(219,163,65,0.12)]"
          : "glass-card hover:border-primary/20",
      )}
    >
      <span
        className={cn(
          "glass-control flex size-10 shrink-0 items-center justify-center rounded-[15px]",
          selected ? "text-brand" : "text-muted-foreground",
        )}
      >
        <DeviceIcon className="size-5" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-foreground">
          {deviceDisplayName(device)}
        </p>
        <div className="mt-0.5 flex items-center gap-1.5">
          <span className="size-1.5 rounded-full bg-green-500" />
          <span className="text-[11px] text-green-500">
            <Trans>在线</Trans>
          </span>
          <ConnectionHint device={device} />
        </div>
      </div>
      <span
        className={cn(
          "flex size-6 shrink-0 items-center justify-center rounded-full border transition-colors",
          selected ? "border-primary bg-primary text-primary-foreground" : "border-border",
        )}
      >
        {selected ? <Check className="size-3.5" /> : null}
      </span>
    </button>
  );
}

function ConnectionHint({ device }: { device: Device }) {
  if (!device.connection || device.latency == null) return null;
  const label =
    device.connection === "lan" ? (
      <Trans>局域网</Trans>
    ) : device.connection === "dcutr" ? (
      <Trans>打洞</Trans>
    ) : (
      <Trans>中继</Trans>
    );
  return (
    <span className="flex items-center gap-1 text-[11px] text-muted-foreground">
      <span aria-hidden>·</span>
      {label}
      <span className="font-mono tabular-nums">{device.latency}ms</span>
    </span>
  );
}

/* ─────────────────── 设备区空/加载态 ─────────────────── */

function StartingNodePlaceholder() {
  return (
    <div className="flex h-full min-h-32 flex-col items-center justify-center gap-3 rounded-[18px] bg-foreground/[0.025] py-8 dark:bg-white/[0.035]">
      <Loader2 className="size-5 animate-spin text-muted-foreground" />
      <div className="text-center">
        <p className="text-[13px] font-semibold text-foreground">
          <Trans>正在启动节点…</Trans>
        </p>
        <p className="mt-0.5 text-[11px] text-muted-foreground">
          <Trans>启动后显示可发送的设备</Trans>
        </p>
      </div>
    </div>
  );
}

function EmptyDevices() {
  return (
    <div className="flex h-full min-h-32 flex-col items-center justify-center gap-3 rounded-[18px] bg-foreground/[0.025] px-6 py-8 text-center dark:bg-white/[0.035]">
      <span className="glass-control flex size-11 items-center justify-center rounded-[16px] text-muted-foreground">
        <Inbox className="size-5" />
      </span>
      <div>
        <p className="text-[13px] font-semibold text-foreground">
          <Trans>没有在线设备</Trans>
        </p>
        <p className="mt-1 max-w-[30ch] text-[11px] leading-5 text-muted-foreground">
          <Trans>让目标设备打开 SwarmDrop 并保持在线，或先配对一台设备。</Trans>
        </p>
      </div>
    </div>
  );
}

