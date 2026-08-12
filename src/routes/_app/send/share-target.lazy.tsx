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
import {
  Check,
  FileStack,
  Inbox,
  Loader2,
  MonitorSmartphone,
  Send,
} from "lucide-react";
import { toast } from "sonner";
import { Trans, useLingui } from "@lingui/react/macro";
import { t } from "@lingui/core/macro";
import type { Device } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { useShareStore } from "@/stores/share-store";
import {
  useActivePrepareProgress,
  useTransferStore,
} from "@/stores/transfer-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useFileSelection } from "./-use-file-selection";
import { getErrorMessage } from "@/lib/errors";
import { deviceDisplayName } from "@/lib/device-name";
import { formatLatency } from "@/lib/format";
import { CONNECTION_LABEL } from "../devices/-components/connection-labels";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { FileBrowser } from "@swarmdrop/file-browser";
import { PrepareProgressBar } from "./-components/prepare-progress-bar";
import { SendProgressView } from "./-components/send-progress-view";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useIsWideLayout } from "@/hooks/use-media-query";
import { SlideDrawer } from "@/components/layout/master-detail-shell";
import {
  CommandDock,
  GlassPanel,
  TaskButton,
  TaskPageShell,
  TaskToolbar,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_app/send/share-target")({
  component: ShareTargetPage,
});

function ShareTargetPage() {
  const { session: activeSessionId } = Route.useSearch() as {
    session?: string;
  };
  const navigate = useNavigate();
  const router = useRouter();
  const fileSelection = useFileSelection();
  const { addSources, clear } = fileSelection;

  const [selectedPeerId, setSelectedPeerId] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  // 准备进度住在 store：广播事件，且要在用户离开本页后继续可读。
  const prepareProgress = useActivePrepareProgress();
  const clearPrepare = useTransferStore((s) => s.clearPrepare);
  // 主任务是选设备（文件已定）。窄屏（<920）选设备占主屏，「待发文件」收进左抽屉。
  const isWide = useIsWideLayout();
  const [filesDrawerOpen, setFilesDrawerOpen] = useState(false);

  const status = useNetworkStore((s) => s.status);
  const startNetwork = useNetworkStore((s) => s.startNetwork);
  const devices = useNetworkStore((s) => s.devices);
  const pairedDevices = useSecretStore((s) => s.pairedDevices);
  const consumeShareSources = useShareStore((s) => s.consume);
  const loadProjections = useTransferStore((s) => s.loadProjections);
  // `removeTarget` 本身已是稳定引用（useCallback），包一层 memo 让 actions 对象也稳。
  const removeActions = useMemo(
    () => ({ onRemove: fileSelection.removeTarget }),
    [fileSelection.removeTarget],
  );
  const fileView = usePreferencesStore((state) => state.fileBrowserViews.send);
  const setFileBrowserView = usePreferencesStore((state) => state.setFileBrowserView);

  // 消费在途来源：订阅 store 而非只在 mount 时 consume——页面已挂载时用户再次
  // 「用 SwarmDrop 打开」不会 remount（navigate 同路由 no-op），必须靠订阅感知新批次。
  // 新一批 = 用户最新意图：覆盖旧选择（clear 而非追加），并退出可能停留的进度视图。
  // consume 取走即清空 → sources 归空触发的下一轮 effect 拿到空数组自然 no-op。
  const pendingSources = useShareStore((s) => s.sources);
  useEffect(() => {
    if (pendingSources.length === 0) return;
    const { sources, presetPeerId } = consumeShareSources();
    if (sources.length === 0) return;
    void navigate({
      to: "/send/share-target",
      search: {},
      replace: true,
    });
    clear();
    // 「重新发送」携带原目标设备；设备当前离线时 selectedDevice 派生为 null，自动回落选设备
    if (presetPeerId) setSelectedPeerId(presetPeerId);
    void addSources(sources).catch((err) => toast.error(getErrorMessage(err)));
  }, [pendingSources, consumeShareSources, navigate, addSources, clear]);

  // 节点未启动时自动启动一次（外部打开常处于冷启动、节点还没起）。
  const startedRef = useRef(false);
  useEffect(() => {
    if (!startedRef.current && status === "stopped") {
      startedRef.current = true;
      void startNetwork();
    }
  }, [status, startNetwork]);

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
  const canSend =
    !sending &&
    !fileSelection.isScanning &&
    fileSelection.hasFiles &&
    selectedDevice !== null;

  const handleSend = async () => {
    if (!selectedDevice || sending || !fileSelection.hasFiles) return;
    setSending(true);
    // 开工先清：上一批可能是**中途失败**停在半路的（尤其 MCP 发起的准备没有前端调用点，
    // 永远轮不到 finally），而认领规则只让「已跑到 100%」的批次让位。不清的话用户会看着
    // 一条冻结的旧进度条、标着别人的文件名，直到自己这批结束。
    clearPrepare();
    try {
      const scannedFiles = fileSelection.getScannedFiles();
      const prepared = await commands.prepareSend(scannedFiles);
      // 准备已完成：立刻收掉进度条，别让它在随后的 startSend / loadProjections 期间
      // 停在 100% 却还说着「正在准备」。
      clearPrepare();
      const fileIds = prepared.files.map((f) => f.fileId);
      const result = await commands.startSend(
        prepared.preparedId,
        selectedDevice.peerId,
        deviceDisplayName(selectedDevice),
        fileIds,
      );
      await loadProjections();
      void navigate({
        to: "/send/share-target",
        search: { session: result.sessionId },
        replace: true,
      });
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setSending(false);
      // 兜底：prepare 自己抛错时上面那次 clear 根本没跑到（见 /send 那页的同一条注释）。
      clearPrepare();
    }
  };

  const setActiveSessionId = (sessionId: string) => {
    void navigate({
      to: "/send/share-target",
      search: { session: sessionId },
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

  if (activeSessionId) {
    return (
      <SendProgressView
        sessionId={activeSessionId}
        onBack={handleBack}
        onSessionChange={setActiveSessionId}
      />
    );
  }

  // 待发文件面板内容（宽屏左栏 / 窄屏抽屉共用）
  const filesBody = (
    <FileBrowser
      items={fileSelection.items}
      title={<Trans>待发文件</Trans>}
      view={fileView}
      onViewChange={(nextView) => setFileBrowserView("send", nextView)}
      actions={removeActions}
      emptyState={{
        title: <Trans>文件已全部移除</Trans>,
        description: <Trans>返回后重新选择要分享的文件。</Trans>,
      }}
      className="h-full"
    />
  );

  // 选设备面板内容（主任务）。窄屏传入 openFiles 渲染「查看待发文件」按钮。
  const devicesBody = (openFiles: (() => void) | null) => (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2 text-[13px] font-semibold text-muted-foreground">
          <MonitorSmartphone className="size-4" />
          <Trans>选择设备</Trans>
        </span>
        {openFiles && (
          <Button
            variant="ghost"
            size="sm"
            onClick={openFiles}
            className="h-7 gap-1.5 rounded-full px-2.5 text-xs text-muted-foreground hover:text-foreground"
          >
            <FileStack className="size-3.5" />
            <span className="font-mono tabular-nums">{fileSelection.totalCount}</span>
            <Trans>项</Trans>
          </Button>
        )}
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
  );

  // 准备进度**叠在按钮之上**（与 /send 同一形态），不替换它们：准备大目录可以是分钟级，
  // 此前替换式的写法让那段时间界面上一个可交互元素都不剩。两页此前的显示门也不一致
  // （这里是 `sending && prepareProgress`，那边是 `prepareProgress`），一并统一。
  const prepareBar = prepareProgress ? (
    <div className="min-w-0 px-2">
      <PrepareProgressBar progress={prepareProgress} />
    </div>
  ) : null;

  const commandDock =
    !fileSelection.hasFiles ? (
      // 空载荷：发送无从谈起，只留一个明确的出口
      <CommandDock>
        <TaskButton onClick={handleBack}>
          <Trans>返回</Trans>
        </TaskButton>
      </CommandDock>
    ) : (
      <CommandDock>
        <TaskButton variant="outline" onClick={handleBack} disabled={sending}>
          <Trans>取消</Trans>
        </TaskButton>
        <TaskButton onClick={handleSend} disabled={!canSend}>
          <Send className="size-4" />
          {fileSelection.isScanning ? (
            <Trans>正在读取所选内容…</Trans>
          ) : sending ? (
            <Trans>准备中…</Trans>
          ) : selectedDevice ? (
            <Trans>发送给 {deviceDisplayName(selectedDevice)}</Trans>
          ) : (
            <Trans>选择一个设备</Trans>
          )}
        </TaskButton>
      </CommandDock>
    );

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

      {/* relative 容器：作为待发文件抽屉的定位上下文（仅窄屏） */}
      <div className="relative flex min-h-0 flex-1 flex-col">
        <div className="mx-auto flex min-h-0 w-full max-w-[1180px] flex-1 flex-col gap-5 p-5 lg:p-7">
          {isWide ? (
            <div
              className="grid min-h-0 flex-1 gap-5"
              style={{ gridTemplateColumns: "360px minmax(0, 1fr)" }}
            >
              <GlassPanel className="min-h-0">
                <div className="h-full p-4 lg:p-5">{filesBody}</div>
              </GlassPanel>
              <GlassPanel className="min-h-0">
                <div className="h-full p-4 lg:p-5">{devicesBody(null)}</div>
              </GlassPanel>
            </div>
          ) : (
            <GlassPanel className="min-h-0 flex-1">
              <div className="h-full p-4 lg:p-5">
                {devicesBody(
                  fileSelection.hasFiles
                    ? () => setFilesDrawerOpen(true)
                    : null,
                )}
              </div>
            </GlassPanel>
          )}

          {prepareBar}
          {commandDock}
        </div>

        {!isWide && (
          <SlideDrawer
            open={filesDrawerOpen}
            onClose={() => setFilesDrawerOpen(false)}
            label={t`待发文件`}
          >
            <div className="flex h-full min-h-0 flex-col p-4">{filesBody}</div>
          </SlideDrawer>
        )}
      </div>
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
          <span className="size-1.5 rounded-full bg-success" />
          <span className="text-[11px] text-success-ink">
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
  const { t: translate } = useLingui();
  const { connection } = device;
  if (!connection || device.latency == null) return null;
  // 查表而非三元链：连接方式此前写成 `lan ? … : dcutr ? … : 中继`，于是新增的
  // `direct` 落进了最后那个"其余"分支，一条直连被显示成「中继」。
  // `CONNECTION_LABEL` 是 `Record<ConnectionType, …>`，漏一个变体编译期就报。
  const latency = formatLatency(device.latency);
  return (
    <span className="flex items-center gap-1 text-[11px] text-muted-foreground">
      <span aria-hidden>·</span>
      {translate(CONNECTION_LABEL[connection])}
      {latency && <span className="font-mono tabular-nums">{latency}</span>}
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
