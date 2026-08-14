/**
 * Send Page (Lazy)
 * 发送内容页面 — 从设备页面点击发送跳转至此
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createLazyFileRoute,
  useNavigate,
  useRouter,
} from "@tanstack/react-router";
import { ClipboardPaste, FileText, Send, ShieldCheck, Type } from "lucide-react";
import { toast } from "sonner";
import { Trans } from "@lingui/react/macro";
import type { Device, FileSource, TextDeliveryRecord } from "@/lib/bindings";
import type { PrepareProgress } from "@/lib/types";
import { commands } from "@/lib/bindings";
import {
  useActivePrepareProgress,
  useTransferStore,
} from "@/stores/transfer-store";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useFileSelection } from "./-use-file-selection";
import { getErrorMessage } from "@/lib/errors";
import { readText } from "@/lib/clipboard";
import {
  deviceGroupNames,
  deviceIdentityHint,
  formatTextDeliveryKiB,
  isTextDeliveryRetryable,
  isTextDeliveryWithinLimit,
  organizedDeviceName,
  TEXT_DELIVERY_MAX_BYTES,
  utf8ByteLength,
} from "@swarmdrop/shared-view";
import { formatFileSize } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { FileDropZone } from "./-components/file-drop-zone";
import { PrepareProgressBar } from "./-components/prepare-progress-bar";
import { SendProgressView } from "./-components/send-progress-view";
import { FileBrowser } from "@swarmdrop/file-browser";
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
  // 准备进度住在 store 而不是这里：它是广播事件，且要在用户离开本页后继续可读。
  const prepareProgress = useActivePrepareProgress();
  const clearPrepare = useTransferStore((s) => s.clearPrepare);
  const loadProjections = useTransferStore((s) => s.loadProjections);

  // 从 network-store / secret-store 查找目标设备
  const onlineDevice = useNetworkStore(
    (s) => s.devices.find((d) => d.peerId === peerId) ?? null,
  );
  const pairedDevices = useSecretStore((s) => s.pairedDevices);
  const deviceOrganization = usePreferencesStore((s) => s.deviceOrganization);

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
      connectionDetails: null,
      lanUpgradeFailed: false,
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
    // 开工先清：上一批可能是**中途失败**停在半路的（尤其 MCP 发起的准备没有前端调用点，
    // 永远轮不到 finally），而认领规则只让「已跑到 100%」的批次让位。不清的话用户会看着
    // 一条冻结的旧进度条、标着别人的文件名，直到自己这批结束。
    clearPrepare();
    try {
      // 将扫描到的文件列表传给后端读取源文件、算出校验和与验签树
      const scannedFiles = fileSelection.getScannedFiles();
      const prepared = await commands.prepareSend(scannedFiles);
      // 准备已完成：立刻收掉进度条。否则接下来的 startSend / loadProjections 期间
      // 它会停在 100% 上，而文案还在说「正在准备」——那两步不是准备。
      clearPrepare();
      const fileIds = prepared.files.map((f) => f.fileId);
      const displayName = organizedDeviceName(device, deviceOrganization);
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
      // 兜底：prepare 自己抛错时上面那次 clear 根本没跑到。**必须无条件调**——
      // 早先这里写成 `if (preparedId)`，而 `preparedId` 恰恰只在 prepare 成功后才有值，
      // 于是它声称覆盖的失败路径一次都没覆盖到，进度行会永久停在半路。
      clearPrepare();
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
      displayName={organizedDeviceName(device, deviceOrganization)}
      identityHint={deviceIdentityHint(device)}
      groupNames={deviceGroupNames(device.peerId, deviceOrganization)}
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
  displayName: string;
  identityHint: string;
  groupNames: string[];
  fileSelection: ReturnType<typeof useFileSelection>;
  sending: boolean;
  prepareProgress: PrepareProgress | null;
  onSourcesSelected: (sources: FileSource[]) => void;
  onSend: () => void;
  onBack: () => void;
}

/* ─────────────────── 桌面端视图 ─────────────────── */

export function DesktopSendView({
  device,
  displayName,
  identityHint,
  groupNames,
  fileSelection,
  sending,
  prepareProgress,
  onSourcesSelected,
  onSend,
  onBack,
}: SendViewProps) {
  const [mode, setMode] = useState<"files" | "text">("files");
  const [textBody, setTextBody] = useState("");
  const [sendingText, setSendingText] = useState(false);
  const [outbox, setOutbox] = useState<TextDeliveryRecord[]>([]);
  const [outboxLoading, setOutboxLoading] = useState(false);
  const DeviceIcon = getDeviceIcon(device.os || device.platform || "");
  // `removeTarget` 本身已是稳定引用（useCallback），包一层 memo 让 actions 对象也稳。
  const removeActions = useMemo(
    () => ({ onRemove: fileSelection.removeTarget }),
    [fileSelection.removeTarget],
  );
  const view = usePreferencesStore((state) => state.fileBrowserViews.send);
  const setFileBrowserView = usePreferencesStore((state) => state.setFileBrowserView);
  const textBytes = utf8ByteLength(textBody);
  const textValid = isTextDeliveryWithinLimit(textBody);

  const refreshOutbox = useCallback(async () => {
    setOutboxLoading(true);
    try {
      setOutbox(await commands.listTextOutbox(device.peerId));
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setOutboxLoading(false);
    }
  }, [device.peerId]);

  useEffect(() => {
    if (mode === "text") void refreshOutbox();
  }, [mode, refreshOutbox]);

  const pasteText = async () => {
    try {
      const clipboardText = await readText();
      if (clipboardText.length === 0) {
        toast.error(<Trans>剪贴板中没有可用文本</Trans>);
        return;
      }
      setTextBody(clipboardText);
    } catch (error) {
      toast.error(getErrorMessage(error));
    }
  };

  const sendText = async () => {
    if (!textValid || sendingText) return;
    setSendingText(true);
    try {
      await commands.sendTextDelivery(device.peerId, displayName, textBody);
      setTextBody("");
      toast.success(<Trans>文本已投递</Trans>);
      await refreshOutbox();
    } catch (error) {
      toast.error(getErrorMessage(error));
    } finally {
      setSendingText(false);
    }
  };

  return (
    <TaskPageShell data-testid="send-page">
      <TaskToolbar
        title={<Trans>发送到 {displayName}</Trans>}
        onBack={onBack}
      />

      <TaskContent
        data-testid="send-content"
        className="flex min-h-0 flex-col gap-4"
        footer={
          // 准备进度**叠在按钮之上**，不再整条把它们顶掉。此前替换式的写法让准备期间
          // 界面上一个可交互元素都不剩，而准备大目录可以是分钟级的。
          <div className="flex flex-col gap-2">
            {mode === "files" && prepareProgress && (
              <div className="min-w-0 px-2">
                <PrepareProgressBar progress={prepareProgress} />
              </div>
            )}
            <CommandDock>
              <TaskButton
                variant="outline"
                onClick={onBack}
                disabled={sending || sendingText}
                data-testid="send-cancel-action"
              >
                <Trans>取消</Trans>
              </TaskButton>
              <TaskButton
                onClick={mode === "files" ? onSend : sendText}
                disabled={
                  mode === "files"
                    ? !fileSelection.hasFiles || sending || fileSelection.isScanning
                    : !textValid || sendingText
                }
                data-testid="send-confirm-action"
              >
                {mode === "files" && <Send className="size-4" />}
                {mode === "text" ? (
                  sendingText ? <Trans>投递中...</Trans> : <Trans>发送文本</Trans>
                ) : fileSelection.isScanning ? (
                  <Trans>正在读取所选内容…</Trans>
                ) : sending ? (
                  <Trans>发送中...</Trans>
                ) : (
                  <Trans>发送</Trans>
                )}
              </TaskButton>
            </CommandDock>
          </div>
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
              {displayName}
            </p>
            <p className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-muted-foreground">
              <ShieldCheck className="size-3 shrink-0 text-brand/80" />
              <span className="truncate">
                {groupNames.length > 0 ? groupNames.join(" · ") : identityHint}
              </span>
              <span className="shrink-0 text-muted-foreground/70">·</span>
              <span className="shrink-0"><Trans>端到端加密直连</Trans></span>
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

        <div
          className="inline-flex w-fit rounded-xl border border-border/70 bg-muted/45 p-1"
          role="tablist"
          aria-label="发送内容类型"
        >
          <Button
            type="button"
            size="sm"
            variant={mode === "files" ? "secondary" : "ghost"}
            className="min-h-11 w-28"
            onClick={() => setMode("files")}
            role="tab"
            aria-selected={mode === "files"}
          >
            <FileText className="size-4" />
            <Trans>文件</Trans>
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === "text" ? "secondary" : "ghost"}
            className="min-h-11 w-28"
            onClick={() => setMode("text")}
            role="tab"
            aria-selected={mode === "text"}
          >
            <Type className="size-4" />
            <Trans>文本</Trans>
          </Button>
        </div>

        {/* 文件选择：空态只保留一个明确的投放区；有内容后再展开补充入口与文件浏览器。 */}
        {mode === "files" ? <GlassPanel
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
                  actions={removeActions}
                />
              </div>
            ) : null}
          </div>
        </GlassPanel> : (
          <GlassPanel className="min-h-0 flex-1" data-testid="send-text-panel">
            <div className="flex h-full min-h-0 flex-col gap-3 p-4 lg:p-5">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold"><Trans>要发送的文本</Trans></p>
                  <p className="mt-1 text-xs text-muted-foreground"><Trans>仅在你点击发送后传输，不会自动同步剪贴板。</Trans></p>
                </div>
                <div className="flex shrink-0 gap-2">
                  <Button type="button" variant="ghost" size="sm" onClick={() => setTextBody("")} disabled={sendingText || textBody.length === 0}>
                    <Trans>清空</Trans>
                  </Button>
                  <Button type="button" variant="outline" size="sm" onClick={pasteText} disabled={sendingText}>
                    <ClipboardPaste className="size-4" />
                    <Trans>从剪贴板粘贴</Trans>
                  </Button>
                </div>
              </div>
              <textarea
                value={textBody}
                onChange={(event) => setTextBody(event.target.value)}
                placeholder="输入或粘贴文本"
                aria-label="要发送的文本"
                className="min-h-52 flex-1 resize-none rounded-2xl border border-border bg-background/70 p-4 text-sm leading-6 outline-none transition focus-visible:ring-2 focus-visible:ring-ring"
              />
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span><Trans>支持 UTF-8 文本，最长 64 KiB</Trans></span>
                <span className={textBytes > TEXT_DELIVERY_MAX_BYTES ? "text-destructive" : undefined}>
                  {formatTextDeliveryKiB(textBytes)} / {formatTextDeliveryKiB(TEXT_DELIVERY_MAX_BYTES)}
                </span>
              </div>
              {textBody.length > 0 && !textValid ? (
                <p className="text-xs text-destructive">
                  <Trans>文本超过 64 KiB，请缩短后发送。</Trans>
                </p>
              ) : null}
              <TextOutbox
                records={outbox}
                loading={outboxLoading}
                onRetry={async (deliveryId) => {
                  try {
                    await commands.retryTextDelivery(deliveryId);
                    await refreshOutbox();
                  } catch (error) {
                    toast.error(getErrorMessage(error));
                  }
                }}
                onEdit={(body) => setTextBody(body)}
                onDelete={async (deliveryId) => {
                  try {
                    await commands.deleteTextOutboxRecord(deliveryId);
                    await refreshOutbox();
                  } catch (error) {
                    toast.error(getErrorMessage(error));
                  }
                }}
              />
            </div>
          </GlassPanel>
        )}

      </TaskContent>
    </TaskPageShell>
  );
}

function TextOutbox({
  records,
  loading,
  onRetry,
  onEdit,
  onDelete,
}: {
  records: TextDeliveryRecord[];
  loading: boolean;
  onRetry: (deliveryId: string) => void;
  onEdit: (body: string) => void;
  onDelete: (deliveryId: string) => void;
}) {
  if (!loading && records.length === 0) return null;
  return (
    <section className="mt-2 border-t border-border/70 pt-3" aria-label="最近发送的文本">
      <div className="mb-2 flex items-center justify-between">
        <p className="text-xs font-semibold text-muted-foreground"><Trans>最近发送</Trans></p>
        {loading ? <span className="text-xs text-muted-foreground"><Trans>加载中…</Trans></span> : null}
      </div>
      <div className="space-y-2">
        {records.slice(0, 5).map((record) => (
          <div key={record.deliveryId} className="rounded-xl border border-border/70 bg-muted/25 p-3">
            <div className="flex items-center justify-between gap-2">
              <p className="min-w-0 flex-1 truncate text-xs text-foreground">{record.body}</p>
              <span className="shrink-0 text-[11px] text-muted-foreground">{textDeliveryStatusLabel(record.status)}</span>
            </div>
            <p className="mt-1 text-[11px] text-muted-foreground"><Trans>更新于</Trans> {new Date(record.updatedAt).toLocaleString()}</p>
            <div className="mt-2 flex justify-end gap-2">
              <Button type="button" size="sm" variant="ghost" onClick={() => onEdit(record.body)}><Trans>编辑后重发</Trans></Button>
              {isRetryableTextStatus(record.status) ? (
                <Button type="button" size="sm" variant="outline" onClick={() => onRetry(record.deliveryId)}><Trans>重试</Trans></Button>
              ) : null}
              <Button type="button" size="sm" variant="ghost" onClick={() => onDelete(record.deliveryId)}><Trans>删除</Trans></Button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function isRetryableTextStatus(status: TextDeliveryRecord["status"]) {
  return isTextDeliveryRetryable(status);
}

function textDeliveryStatusLabel(status: TextDeliveryRecord["status"]) {
  const labels: Record<TextDeliveryRecord["status"], React.ReactNode> = {
    sending: <Trans>发送中</Trans>,
    waiting_confirmation: <Trans>等待确认</Trans>,
    delivered: <Trans>已送达</Trans>,
    rejected: <Trans>已拒绝</Trans>,
    retryable: <Trans>可重试，送达状态未知</Trans>,
    expired: <Trans>已过期</Trans>,
    cancelled: <Trans>已取消</Trans>,
  };
  return labels[status];
}
