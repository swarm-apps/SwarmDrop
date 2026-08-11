/**
 * NodeStatusSheet
 * 节点状态面（桌面端 Dialog）。**启停合并在这一个面里**——动作随状态切换。
 *
 * 拆成 StartNodeSheet / StopNodeSheet 两个组件曾造成两处硬伤：设备页只挂得起
 * 「启动」那一个（`stopSheetOpen` 从来没有被置真过，是一段死状态），且两份
 * 各写各的状态呈现，节点没跑时用户只看得到四个写死的 0。
 *
 * 分层遵守 `DESIGN.md` 的 Node Status Contract：
 * - **结论层**（常驻）：状态点+词 · 可达性后果句 · 已配对 N·在线 M · 至多一个 CTA
 * - **诊断层**（一个 details，默认折叠）：引导节点逐条 + 本机真值
 *
 * **不得按窗口高度丢信息位。** 此前这里有 `showExtra = windowHeight >= 700` 与 7 个
 * 门控点，矮窗口下中继状态、引导节点、公网地址、监听地址整片消失——而那正是用户在
 * 排查「连不上」时唯一要看的东西。现在改为折叠 + 内滚（对话框自带 `max-h`/`overflow`）。
 */

import { hostname, platform, type as osType } from "@tauri-apps/plugin-os";
import { useEffect, useState } from "react";
import { Link } from "@tanstack/react-router";
import { ChevronRight, Copy } from "lucide-react";
import { toast } from "sonner";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useShallow } from "zustand/shallow";
import { cn } from "@/lib/utils";
import { copyText } from "@/lib/clipboard";
import { formatUptime } from "@/lib/format-uptime";
import { useActiveTransferCount } from "@/hooks/use-active-transfer-count";
import { useNodeHealth } from "@/hooks/use-node-health";
import {
  deriveInfraLinkState,
  INFRA_LINK_WORD,
  resolveNodePresentation,
  toInfraLinkView,
  TONE_DOT,
} from "@/lib/node-status";
import { getDeviceIcon } from "@/components/pairing/device-icon";
import { LanHelperAddress } from "@/components/network/lan-helper-address";
import {
  InfraLinkAttribution,
  InfraLinkError,
} from "@/components/network/infra-link-parts";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { usePairedOnlineCount } from "@/hooks/use-paired-online-count";
import { useNetworkStore } from "@/stores/network-store";
import { useSecretStore } from "@/stores/secret-store";
import { usePreferencesStore } from "@/stores/preferences-store";
import { commands, type InfraLink } from "@/lib/bindings";

interface NodeStatusSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const platformLabel: Record<string, string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
  android: "Android",
  ios: "iOS",
};

/**
 * 身份文件路径，**每进程取一次**。
 *
 * 它由 `app_local_data_dir` + 固定文件名拼出，进程内恒定；而这个面板是 Dialog 的子树，
 * 关掉即卸载，不 memo 的话每开一次都发一轮 IPC（诊断层还折叠着的时候也发）。
 * 失败吞成空串——调用点据此渲染 `—`，诊断少一行不该让整个面板出错。
 */
let identityPathPromise: Promise<string> | null = null;
function identityFilePathOnce(): Promise<string> {
  // **只缓存成功的结果**。一次瞬时失败（例如弹窗在 webview 重载期间打开、Tauri handler
  // 还没就绪）如果被缓存下来，这一行就会钉死在 `—` 直到重启应用——而用户恰恰是在出问题
  // 的时候才来看它。失败就把 memo 清掉，下次开弹窗重试。
  identityPathPromise ??= commands.getIdentityFilePath().catch(() => {
    identityPathPromise = null;
    return "";
  });
  return identityPathPromise;
}

export function NodeStatusSheet({ open, onOpenChange }: NodeStatusSheetProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="node-status-sheet"
        className="max-h-[85vh] overflow-y-auto p-0! sm:max-w-[min(90vw,32rem)]"
      >
        <NodeStatusContent onClose={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}

function NodeStatusContent({ onClose }: { onClose: () => void }) {
  const { t } = useLingui();
  const { summary, lifecycle, links, networkStatus, nowMs } = useNodeHealth();
  const { startNetwork, stopNetwork, startedAt } = useNetworkStore(
    useShallow((s) => ({
      startNetwork: s.startNetwork,
      stopNetwork: s.stopNetwork,
      startedAt: s.startedAt,
    })),
  );
  // 与设置页的「已连接设备」同源——见 `usePairedOnlineCount` 的文档。
  const onlineDeviceCount = usePairedOnlineCount();
  const pairedCount = useSecretStore((s) => s.pairedDevices.length);
  const deviceId = useSecretStore((s) => s.deviceId);
  const deviceName = usePreferencesStore((s) => s.deviceName);
  // 停止节点会断开全部连接，在途传输当场中断——这是这个面唯一会造成数据损失的后果。
  const activeTransferCount = useActiveTransferCount();

  const [systemHostname, setSystemHostname] = useState("");
  useEffect(() => {
    hostname().then((name) => setSystemHostname(name ?? ""));
  }, []);

  // 身份文件路径。取不到就留空，`CopyableValue` 那侧渲染成 `—`——诊断少一行信息
  // 不该让整个面板出错。
  const [identityPath, setIdentityPath] = useState("");
  useEffect(() => {
    void identityFilePathOnce().then(setIdentityPath);
  }, []);

  // 诊断层受控：`openDiagnostics` 这个 CTA 要能把它展开，`<details>` 自管开合就做不到。
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

  const presentation = resolveNodePresentation(lifecycle, summary);
  const isRunning = lifecycle === "running";
  const isStarting = lifecycle === "starting";

  const displayName = deviceName || systemHostname || "SwarmDrop";
  const avatarInitials = displayName.slice(0, 2).toUpperCase();
  const currentPlatform = platform();
  const DeviceIcon = getDeviceIcon(osType());

  const handleStart = async () => {
    const ok = await startNetwork();
    // 失败时保持面板打开，让用户看到错误状态与 toast
    if (ok) onClose();
  };
  const handleStop = async () => {
    await stopNetwork();
    onClose();
  };

  return (
    <div className="flex flex-col gap-4 p-6">
      <DialogHeader className="items-center text-center">
        <div className="relative">
          <div className="flex size-16 items-center justify-center rounded-2xl bg-primary/10 dark:bg-primary/12">
            <span className="text-xl font-bold tracking-tight text-brand">
              {avatarInitials}
            </span>
          </div>
          <div className="absolute -bottom-1 -right-1 flex size-6 items-center justify-center rounded-lg border border-border bg-background shadow-sm">
            <DeviceIcon className="size-3.5 text-muted-foreground" />
          </div>
        </div>
        <div>
          <DialogTitle>{displayName}</DialogTitle>
          <DialogDescription>
            {platformLabel[currentPlatform] ?? currentPlatform}
          </DialogDescription>
        </div>
      </DialogHeader>

      {/* ── 结论层 ── */}
      <section
        data-testid="node-status-conclusion"
        className="flex flex-col gap-3 rounded-xl border border-border p-4"
      >
        <div className="flex items-center gap-2">
          <span
            className={cn("size-2.5 rounded-full", TONE_DOT[presentation.tone])}
          />
          <span className="text-sm font-semibold text-foreground">
            {t(presentation.word)}
          </span>
        </div>
        <p className="text-sm leading-6 text-muted-foreground">
          {t(presentation.sentence)}
        </p>

        <Link
          to="/devices"
          onClick={onClose}
          className="flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <Trans>
            已配对 {pairedCount} · 在线 {onlineDeviceCount}
          </Trans>
          <ChevronRight className="size-3.5" />
        </Link>

        {/* 至多一个 CTA：`startNode` 与页脚主按钮是同一件事，不在这里重复一遍。 */}
        {presentation.cta === "openSettings" && (
          <Button asChild variant="outline" size="sm" className="self-start">
            <Link to="/settings" onClick={onClose}>
              <Trans>去设置</Trans>
            </Link>
          </Button>
        )}
        {presentation.cta === "openDiagnostics" && (
          <Button
            variant="outline"
            size="sm"
            className="self-start"
            onClick={() => setDiagnosticsOpen(true)}
          >
            <Trans>查看诊断信息</Trans>
          </Button>
        )}
      </section>

      {/* ── 诊断层 ── */}
      <details
        open={diagnosticsOpen}
        onToggle={(e) => setDiagnosticsOpen(e.currentTarget.open)}
        className="rounded-xl border border-border"
      >
        <summary className="cursor-pointer px-4 py-2.5 text-sm text-muted-foreground hover:text-foreground">
          <Trans>诊断信息</Trans>
        </summary>

        <div className="flex flex-col gap-3 border-t border-border p-4">
          <InfraLinkList links={links} nowMs={nowMs} />

          <div className="overflow-hidden rounded-lg border border-border">
            <DiagnosticRow label={<Trans>节点 ID</Trans>}>
              <CopyableValue
                value={deviceId ?? ""}
                display={deviceId ? truncateMiddle(deviceId, 4, 5) : "—"}
                toastLabel={t`节点 ID 已复制`}
              />
            </DiagnosticRow>
            <DiagnosticRow label={<Trans>公网地址</Trans>}>
              <CopyableValue
                value={networkStatus?.publicAddr ?? ""}
                display={
                  networkStatus?.publicAddr
                    ? truncateMiddle(networkStatus.publicAddr, 18, 12)
                    : "—"
                }
                toastLabel={t`公网地址已复制`}
              />
            </DiagnosticRow>
            <DiagnosticRow label={<Trans>NAT 状态</Trans>}>
              <Badge
                variant="outline"
                className={cn(
                  "border-transparent text-xs",
                  networkStatus?.natStatus === "public"
                    ? "bg-primary/10 text-brand dark:bg-primary/15"
                    : "bg-muted text-muted-foreground",
                )}
              >
                {networkStatus?.natStatus === "public" ? t`映射成功` : t`未知`}
              </Badge>
            </DiagnosticRow>
            <DiagnosticRow label={<Trans>已发现节点</Trans>}>
              <span className="text-sm tabular-nums text-foreground">
                {networkStatus?.discoveredPeers ?? 0}
              </span>
            </DiagnosticRow>
            {/* 身份自 2026-08 由系统钥匙串改为本机明文文件（0600）。这一行因此从
                「一句不必关心的实现细节」变成用户**需要**据以行动的信息：备份、换机
                迁移、自己收紧权限都要先找得到它，所以给的是可复制的完整路径。 */}
            <DiagnosticRow label={<Trans>身份文件</Trans>}>
              <CopyableValue
                value={identityPath}
                display={truncateMiddle(identityPath, 8, 23)}
                toastLabel={t`已复制身份文件路径`}
              />
            </DiagnosticRow>
            {/* 运行时长住诊断层：它回答不了「别人能不能连到我」这个问题。 */}
            <DiagnosticRow label={<Trans>运行时长</Trans>}>
              <span className="text-sm text-foreground">
                {startedAt ? formatUptime(startedAt) : "—"}
              </span>
            </DiagnosticRow>
          </div>

          <ListenAddrs addrs={networkStatus?.listenAddrs ?? []} />
          <LanHelperAddress />
        </div>
      </details>

      <DialogFooter className="flex flex-col gap-3">
        {isRunning ? (
          <>
            <p className="text-center text-xs text-destructive-ink">
              <Trans>停止后将断开所有连接，其他设备将无法发现你</Trans>
              {/* 「断开所有连接」说的是可达性，用户读不出「我正在传的文件会没」。 */}
              {activeTransferCount > 0 && (
                <>
                  {" "}
                  {/* `<Plural>` 而不是插值：源 locale 只有一种复数形式，直接插值会让
                      英文 catalog 被迫写成 "1 transfer(s)"——而这条提示最常见的取值
                      恰恰就是 1。 */}
                  <Plural
                    value={activeTransferCount}
                    one="正在进行的 # 个传输会被中断。"
                    other="正在进行的 # 个传输会被中断。"
                  />
                </>
              )}
            </p>
            <div className="flex gap-2">
              <Button variant="outline" onClick={onClose} className="flex-1">
                <Trans>取消</Trans>
              </Button>
              <Button
                variant="destructive"
                onClick={handleStop}
                className="flex-1"
                data-testid="stop-node-confirm-action"
              >
                <Trans>停止节点</Trans>
              </Button>
            </div>
          </>
        ) : (
          <Button
            onClick={handleStart}
            disabled={isStarting}
            data-testid="start-node-confirm-action"
          >
            {isStarting ? <Trans>启动中...</Trans> : <Trans>启动节点</Trans>}
          </Button>
        )}
      </DialogFooter>
    </div>
  );
}

/** 引导节点逐条：状态点+词 · 归因 · 原样 `lastError` + 复制。 */
function InfraLinkList({ links, nowMs }: { links: InfraLink[]; nowMs: number }) {
  const { t } = useLingui();

  if (links.length === 0) {
    return (
      <p
        data-testid="infra-links-empty"
        className="rounded-lg border border-dashed border-border px-3 py-4 text-xs leading-5 text-muted-foreground"
      >
        <Trans>还没有引导节点。设置页可以添加。</Trans>
      </p>
    );
  }

  return (
    <div
      data-testid="infra-links"
      className="overflow-hidden rounded-lg border border-border"
    >
      {links.map((link) => {
        const state = deriveInfraLinkState(toInfraLinkView(link), nowMs);
        return (
          <div
            key={link.peerId}
            className="flex flex-col gap-1.5 border-b border-border p-3 last:border-b-0"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="flex min-w-0 items-center gap-1.5">
                <span
                  className={cn(
                    "size-2 shrink-0 rounded-full",
                    TONE_DOT[state.tone],
                  )}
                />
                <span className="truncate text-xs font-medium text-foreground">
                  {t(INFRA_LINK_WORD[state.state])}
                </span>
              </span>
              <code
                title={link.peerId}
                className="shrink-0 font-mono text-[11px] text-muted-foreground"
              >
                {truncateMiddle(link.peerId, 8, 6)}
              </code>
            </div>
            {/* 归因：来源 · 范围 · 角色。三项缺一都会让两条状态相同的行变得无法区分。 */}
            <InfraLinkAttribution link={link} />
            {/* `lastError` 原样不翻译：排查时用户要贴的就是这一句。 */}
            {state.detail && <InfraLinkError detail={state.detail} />}
          </div>
        );
      })}
    </div>
  );
}

function ListenAddrs({ addrs }: { addrs: string[] }) {
  if (addrs.length === 0) return null;
  return (
    <details className="rounded-lg border border-border">
      <summary className="flex cursor-pointer items-center justify-between px-3 py-2 text-xs text-muted-foreground hover:text-foreground">
        <Trans>监听地址</Trans>
        <span className="tabular-nums">{addrs.length}</span>
      </summary>
      <div className="max-h-32 overflow-y-auto border-t border-border px-3 py-2">
        <div className="flex flex-col gap-1">
          {addrs.map((addr) => (
            <code
              key={addr}
              className="break-all font-mono text-[11px] leading-relaxed text-muted-foreground"
            >
              {addr}
            </code>
          ))}
        </div>
      </div>
    </details>
  );
}

function DiagnosticRow({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border px-3 py-2.5 last:border-b-0">
      <span className="shrink-0 text-sm text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

/**
 * 可复制的机器值。**复制的永远是完整值**，屏幕上那串是被中间省略过的——
 * 贴出去的截断串是一条写错的地址／一个不存在的节点。`title` 同理给全值。
 */
function CopyableValue({
  value,
  display,
  toastLabel,
}: {
  value: string;
  display: string;
  toastLabel: string;
}) {
  const { t } = useLingui();
  if (!value) {
    return <code className="font-mono text-sm text-muted-foreground">—</code>;
  }
  return (
    <button
      type="button"
      title={value}
      onClick={() => {
        copyText(value).then(
          () => toast.success(toastLabel),
          () => toast.error(t`复制失败`),
        );
      }}
      className="group flex min-w-0 items-center gap-1.5 text-right"
    >
      <code className="truncate font-mono text-sm text-foreground">
        {display}
      </code>
      <Copy className="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
    </button>
  );
}

/** 中间省略。两端各留几位，切口处必须看得见省略号（否则像一条完整但写错的串）。 */
function truncateMiddle(value: string, head: number, tail: number): string {
  if (value.length <= head + tail + 1) return value;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}
