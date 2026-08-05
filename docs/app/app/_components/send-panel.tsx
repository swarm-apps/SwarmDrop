"use client";

// 发送面板：选目标 → 拖/选文件 → `send_files()` → prepare 进度实时展示。
//
// ## 目标选择器是**落点与纠错**，不是入口（DESIGN.md 的 Send Entry Contract）
//
// 正常路径是从设备卡片点「发送」进来，`?peerId=` 已经把目标带好了——那时这里呈现的是一张
// **已确认的目标卡**，而不是一个还要再选一次的下拉框。下拉框只在两种情况下出场：
// 直接访问本页（没带 peerId），或用户主动点「更换」。
//
// 此前这里是「表单优先」：进来先面对一个 `<select>`，与桌面/移动的「设备优先」心智分叉。
//
// 隐式优先 + 极客高效（PRODUCT.md 原则 1/3）：路径尽量短、不堆确认层级；
// prepare 是真实阶段而非假 loading（原则 2·状态诚实可见）。
//
// ## 读 query 必须有 Suspense 边界
//
// 静态导出下预渲染阶段拿不到 query，没边界 `next build` 直接报 CSR bailout。边界包在本文件
// 导出的 `SendPanel` 里，调用方拿到的就是安全的版本，不必记得自己套。
// **不要把 `useSearchParams()` 写在套边界的那一层**——那样边界在读取点外面才有用。

import { Trans, useLingui } from "@lingui/react/macro";
import {
  ArrowLeftRight,
  Check,
  MonitorSmartphone,
  Paperclip,
  Send,
  X,
} from "lucide-react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  calcPercent,
  canSendToDevice,
  formatFileSize,
  organizedDeviceName,
} from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/cn";
import { PANEL_SURFACE } from "./section";
import { CenteredEmptyState } from "./empty-state";
import { NodeNotReadyState } from "./node-not-ready-state";
import { deviceIcon } from "../_lib/device-presentation";
import { transferSample } from "../_lib/format";
import { NAV, PARAM, transferSessionHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { usePreferences } from "../_lib/preferences-store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useWebNode } from "../_lib/store";
import {
  OFFER_REJECT_REASON_LABEL,
  failureCodeLabel,
  type Device,
  type TransferProgressEvent,
  type TransferProjection,
} from "../_lib/view-types";
import { PanelFallback } from "./panel-fallback";
import { ProgressBar } from "./progress-bar";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

/** 待发送文件项——配 id 而非数组下标做 key，移除文件时不会因下标前移而错位复用。 */
interface PendingFile {
  id: number;
  file: File;
}

let nextFileId = 0;

export function SendPanel() {
  return (
    <Suspense fallback={<PanelFallback>
          <Trans>正在准备发送…</Trans>
        </PanelFallback>}>
      <SendPanelInner />
    </Suspense>
  );
}

function SendPanelInner() {
  const { t } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  const devices = useWebNode((s) => s.pairedDevices);
  const organization = usePreferences((s) => s.deviceOrganization);
  const prepareProgress = useWebNode((s) => s.latestPrepareProgress);
  const ready = nodeStatus === "running";

  // 同一路由内 `?peerId=` 变化（如从设备页改点另一台设备的「发送」）不会重挂本组件，
  // 故初始值之外还要跟随 query 同步。
  const requestedPeerId = useSearchParams().get(PARAM.peerId) ?? "";
  const [peerId, setPeerId] = useState(requestedPeerId);
  /** 用户主动点「更换」后才展开选择器；带着有效 peerId 进来时它是收起的。 */
  const [pickerOpen, setPickerOpen] = useState(false);
  useEffect(() => {
    if (!requestedPeerId) return;
    setPeerId(requestedPeerId);
    setPickerOpen(false);
  }, [requestedPeerId]);

  const [files, setFiles] = useState<PendingFile[]>([]);
  const [dragOver, setDragOver] = useState(false);
  const [sentSessionId, setSentSessionId] = useState<string | null>(null);
  const sendAction = useAsyncAction();
  // 对方拒绝了刚发出的 offer（尤其未配对硬拒 NotPaired）——不能让「已发出」成功态永久悬空。
  const rejection = useWebNode((s) => (sentSessionId ? s.rejections[sentSessionId] : undefined));
  // 刚发出那条会话的实时投影。**「已发出」不是终点**：此前无论对方是接受了、传完了还是中断了，
  // 这里都永远停在「等待对方接受」，用户必须跳到传输页才知道后来怎么样了——一个说谎的成功态
  // 比没有状态更糟（PRODUCT.md 原则 2·状态诚实可见）。
  const sentProjection = useWebNode((s) => (sentSessionId ? s.projections[sentSessionId] : undefined));
  /** 实时字节采样。projection 只在状态转换时重发，进度条要跟手就得读这一路。 */
  const sentProgress = useWebNode((s) => (sentSessionId ? s.progress[sentSessionId] : undefined));
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const folderInputRef = useRef<HTMLInputElement | null>(null);

  // selector 只返回 store 内的稳定引用，派生放这里（`pnpm check:zustand-access` 规则 B）。
  const target = useMemo(
    () => devices.find((device) => device.peerId === peerId) ?? null,
    [devices, peerId],
  );
  // `?peerId=` 是**外部输入**：链接可能来自已解除配对、已下线的设备，也可能是手输的。
  // 只判 `!!peerId` 会让按钮在必然失败的目标上亮着，用户点完只收到一条内核报错。
  // 同时兜住「选好设备后对方转离线」这个时间窗。
  const targetValid = target !== null && canSendToDevice(target);
  const totalBytes = useMemo(() => files.reduce((sum, f) => sum + f.file.size, 0), [files]);
  const canSend = ready && targetValid && files.length > 0 && !sendAction.pending;

  const addFiles = (list: FileList | null) => {
    if (!list || list.length === 0) return;
    setFiles((prev) => [...prev, ...Array.from(list, (file) => ({ id: nextFileId++, file }))]);
    setSentSessionId(null);
  };

  const doSend = () => {
    const node = getNode();
    if (!node || !peerId || files.length === 0) return;
    sendAction.run(
      () => node.send_files(peerId, files.map((f) => f.file)),
      (sessionId) => {
        setSentSessionId(sessionId);
        setFiles([]);
      },
    );
  };

  if (devices.length === 0) {
    // **「节点还没起来」必须与「你没有设备」分开**：`pairedDevices` 初值是空数组，
    // 而状态轮询要等 wasm 拉完 `_bg.wasm` 才第一次 tick——此前这个早返回跑在任何
    // `ready` 判断之前，于是每次刷新，已配对的老用户都先看到一句「还没有可发送的设备」
    // 外加一条去配对的教学。那是在断言一件当时并不成立的事。
    return (
      // 保留 `flex-1` 撑满高度：此刻这一栏的全部内容就是这个空态，缩起来只会在下面
      // 留一片无主的空白。宽度同样由页面的 `column="form"` 决定，两种状态才不会在
      // 「配对完第一台设备」的瞬间横向跳一下版。
      <div className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", PANEL_SURFACE)}>
        {ready ? (
          <CenteredEmptyState
            icon={MonitorSmartphone}
            title={<Trans>还没有可发送的设备</Trans>}
            description={
              <Trans>发送需要先与对方配对。配对是一次性动作，配完即长期信任，之后可以直接从设备卡片发送。</Trans>
            }
            action={
              <Button asChild size="sm">
                <Link href={NAV.devices.href}>
                  <MonitorSmartphone className="size-4" aria-hidden />
                  <Trans>去配对设备</Trans>
                </Link>
              </Button>
            }
          />
        ) : (
          <NodeNotReadyState
            description={<Trans>节点起来后，已配对的设备会出现在这里。</Trans>}
          />
        )}
      </div>
    );
  }

  return (
    // **投放目标是整张卡片，不只是那个虚线框。** 拖到框外一点点松手，此前会走浏览器默认行为
    // ——直接导航去打开那个文件，整个 SPA 连同正在跑的 P2P 节点一起没了。目标越大越难失手，
    // 而高亮仍然只画在虚线框上，用户不必知道边界在哪。
    // （落在应用区其它位置的误投由 layout 的 `WindowDropGuard` 兜底。）
    // 列宽由页面给（`send/page.tsx` 的 `column="form"`），这里不自己 `mx-auto max-w-*`
    // ——面板自己缩会让页头仍是满宽、两者左边缘对不齐。
    <div
      className="flex flex-col gap-[var(--space-in-panel)] rounded-xl border bg-card p-4 shadow-xs sm:p-6"
      onDragOver={(event) => {
        // 只接文件。从页面里拖一段选中的文字或一个链接过来不该点亮投放区，更不该被当成
        // 一次空投放（`dataTransfer.files` 为空，addFiles 会静默什么也不做）。
        if (!event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
        setDragOver(true);
      }}
      onDragLeave={(event) => {
        // dragleave 从子元素冒泡上来：拖着文件从虚线框移到下面的文件列表，浏览器先发一条
        // 「离开子元素」。只有 relatedTarget 真的落在卡片之外才算离开——否则高亮会在整个
        // 拖动过程里随鼠标经过每个子元素闪烁。
        if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
        setDragOver(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        setDragOver(false);
        addFiles(event.dataTransfer.files);
      }}
    >
      <TargetSection
        target={target}
        targetValid={targetValid}
        requestedPeerId={requestedPeerId}
        pickerOpen={pickerOpen || target === null}
        devices={devices}
        organization={organization}
        disabled={!ready || sendAction.pending}
        onPick={(id) => {
          setPeerId(id);
          setPickerOpen(false);
        }}
        onChangeRequest={() => setPickerOpen(true)}
      />

      <div
        className={cn(
          "rounded-lg border-2 border-dashed px-4 py-8 text-center transition-colors",
          dragOver ? "border-[var(--brand-solid)] bg-accent" : "border-border",
        )}
      >
        <Paperclip className="mx-auto size-5 text-muted-foreground" aria-hidden />
        {/* 手机浏览器没有拖拽，所以按钮是独立的首要入口，不是「或者」后面的补充。 */}
        <p className="mt-2 text-xs text-muted-foreground">
          <Trans>把文件拖到这里</Trans>
        </p>
        <div className="mt-2 flex flex-wrap items-center justify-center gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => fileInputRef.current?.click()}
                      >
            <Trans>选择文件</Trans>
          </Button>
          {/* 选文件夹：桌面早就有（后端递归枚举），浏览器靠 `webkitdirectory`——非标准但
              Chrome / Safari / Firefox 全支持，是这件事在 Web 上唯一的做法。
              目录结构由 `webkitRelativePath` 带过来，见 `addFiles`。 */}
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => folderInputRef.current?.click()}
                      >
            <Trans>选择文件夹</Trans>
          </Button>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(event) => {
            addFiles(event.target.files);
            event.target.value = "";
          }}
        />
        <input
          ref={folderInputRef}
          type="file"
          multiple
          className="hidden"
          // React 不认这两个非标准属性的驼峰写法，要用 ref 在 effect 里设——但直接写成
          // 小写字符串属性同样有效且更短，TS 需要一次断言。
          {...({ webkitdirectory: "", directory: "" } as Record<string, string>)}
          onChange={(event) => {
            addFiles(event.target.files);
            event.target.value = "";
          }}
        />
      </div>

      {files.length > 0 && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-xs text-muted-foreground">
              <Trans>
                已选 {files.length} 个文件 · 共{" "}
                <span className="font-mono tabular-nums">{formatFileSize(totalBytes)}</span>
              </Trans>
            </p>
            {/* 逐个点 X 清十几个文件是桌面端早就避免了的事（它有 `clear()`）。 */}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => setFiles([])}
              disabled={sendAction.pending}
              className="shrink-0 text-xs"
            >
              <Trans>清空</Trans>
            </Button>
          </div>
          <ul className="flex flex-col gap-1.5">
            {files.map(({ id, file }) => (
              <li
                key={id}
                className="flex items-center justify-between gap-2 rounded-lg border bg-background px-3 py-2 text-xs"
              >
                <span className="truncate text-foreground">{file.name}</span>
                <span className="flex shrink-0 items-center gap-2">
                  <span className="font-mono tabular-nums text-muted-foreground">
                    {formatFileSize(file.size)}
                  </span>
                  <button
                    type="button"
                    onClick={() => setFiles((prev) => prev.filter((f) => f.id !== id))}
                    disabled={sendAction.pending}
                    aria-label={t`移除 ${file.name}`}
                    className="-m-2 flex size-9 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
                  >
                    <X className="size-4" aria-hidden />
                  </button>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/*
        表单页脚，不是横幅。此前这颗按钮直接躺在 `flex-col` 里——flex 默认 `align-items:
        stretch`，于是它被拉成整块面板那么宽（1240px 限宽下实测 1100px），一条满饱和度的
        青绿长条成了全页最重的东西，而它多数时候还是 disabled 的。
        （product register 明写：非激活态不用满饱和度重色。）

        `w-full sm:w-auto`：窄屏保留整宽（拇指够得着是硬要求），指针端收回自然宽度并右对齐
        ——右下角是表单主动作的常规落点，用户不必重新找。
      */}
      <div className="flex sm:justify-end">
        <Button
          onClick={doSend}
          disabled={!canSend}
          className="w-full gap-1.5 sm:w-auto sm:min-w-32"
        >
          <Send className="size-4" aria-hidden />
          {sendAction.pending ? <Trans>发送中…</Trans> : <Trans>发送</Trans>}
        </Button>
      </div>

      {sendAction.pending && prepareProgress && (
        <div className="flex flex-col gap-1">
          <p className="text-xs text-muted-foreground">
            <Trans>
              正在准备 {prepareProgress.currentFile}（{prepareProgress.completedFiles}/
              {prepareProgress.totalFiles} 文件 · {formatFileSize(prepareProgress.bytesHashed)}/
              {formatFileSize(prepareProgress.totalBytes)}）
            </Trans>
          </p>
          <ProgressBar
            percent={calcPercent(prepareProgress.bytesHashed, prepareProgress.totalBytes)}
            label={t`准备文件的进度`}
          />
        </div>
      )}

      {sendAction.error && <WebErrorCard error={sendAction.error} className="text-xs" />}

      {sentSessionId && !rejection && (
        <SentSessionCard
          sessionId={sentSessionId}
          projection={sentProjection}
          progress={sentProgress}
        />
      )}

      {rejection?.reason && (
        // 语义状态色走 `-ink` 变体：`text-amber-600` 在白底只有 3.87:1，而这句话恰恰是
        // 最需要被读到的（对方为什么拒了）。见 global.css 的 State Ink 说明。
        <p className="text-xs text-warning-ink">
          {t(OFFER_REJECT_REASON_LABEL[rejection.reason.type])}
        </p>
      )}
    </div>
  );
}

/**
 * 刚发出那条会话的**去向**。
 *
 * 此前这里是一行固定的「已发出，等待对方接受。」——它在会话被接受、传完、中断、失败之后
 * 全都一字不改，而发送页恰恰是用户点完发送后会停留几秒的地方。真实状态就在 store 里
 * （投影 + 进度采样都按 sessionId 索引），不读它而摆一句永不更新的话，是自己制造了一个
 * 说谎的成功态（PRODUCT.md 原则 2）。
 *
 * **判据统一、文案分叉**：阶段判定读的是与传输页同一个 `projection.phase`，但这里说的是
 * 发送者视角的第一人称（「对方已接受」而不是中性的「传输中」）。分叉的是措辞，不是状态机。
 *
 * 完整的逐文件明细仍归传输页，这里只给一条链接——落地即选中该条会话。
 */
function SentSessionCard({
  sessionId,
  projection,
  progress,
}: {
  sessionId: string;
  /** 事件还没回流时为空：那一刻「已发出」就是全部已知事实。 */
  projection?: TransferProjection;
  progress?: TransferProgressEvent;
}) {
  const { t } = useLingui();
  const phase = projection?.phase;
  const ended = phase === "terminal";
  // 「终态以 projection 为准」的取舍由 `transferSample` 统一承担（见 `_lib/format.ts`）。
  const sample = projection ? transferSample(projection, progress) : null;
  const completed = ended && projection?.terminalReason === "completed";
  const failed = ended && projection?.terminalReason === "fatal_error";
  const failureLabel = failureCodeLabel(projection?.failure ?? null);

  return (
    <div
      className={cn(
        "flex flex-col gap-2 rounded-lg border px-3 py-2.5 text-xs",
        completed
          ? "border-success/30 bg-success/5 text-success-ink"
          : failed
            ? "border-destructive/30 bg-destructive/5 text-destructive-ink"
            : "bg-background text-muted-foreground",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          {completed && <Check className="size-3.5 shrink-0" aria-hidden />}
          <span className="truncate">
            {completed ? (
              <Trans>已送达</Trans>
            ) : failed ? (
              <Trans>发送失败</Trans>
            ) : ended ? (
              // 取消与拒绝都落在这里。拒绝另有一条带具体原因的提示（`rejections` 域），
              // 由调用方渲染，本卡片此时根本不出场。
              <Trans>已结束</Trans>
            ) : phase === "active" ? (
              <Trans>对方已接受，正在传输</Trans>
            ) : phase === "suspended" ? (
              // 自己在传输页按的暂停要说「已暂停」——说成「中断」会让用户以为出了故障，
              // 转头去查网络。其余几种中断（对方暂停 / 连接断 / 对方离线）对发送者而言
              // 都是「传到一半停了」，不必在这张卡上逐一区分，详情侧有完整原因。
              projection?.suspendedReason === "local_paused" ? (
                <Trans>已暂停</Trans>
              ) : (
                <Trans>传输已中断</Trans>
              )
            ) : (
              <Trans>已发出，等待对方接受</Trans>
            )}
          </span>
        </span>
        <Link
          href={transferSessionHref(sessionId)}
          className="shrink-0 font-medium underline underline-offset-2"
        >
          <Trans>查看传输</Trans>
        </Link>
      </div>
      {phase === "active" && sample && <ProgressBar percent={sample.percent} label={t`发送进度`} />}
      {/* 失败原因就在投影上，不必让用户再跳一次页面才看得到。 */}
      {failed && failureLabel && <p className="break-words">{t(failureLabel)}</p>}
    </div>
  );
}

/**
 * 目标区。两种形态：
 *
 * - **已确认**（带着有效 `?peerId=` 进来）：一张只读的目标卡 + 一个「更换」入口。
 *   用户已经在设备页做过选择，这里再摆一个下拉框等于让他确认自己刚做过的事。
 * - **待选**（直接访问本页，或点了「更换」）：出下拉框，并同时给一条回设备页的路——
 *   那才是契约里的主路径。
 */
function TargetSection({
  target,
  targetValid,
  requestedPeerId,
  pickerOpen,
  devices,
  organization,
  disabled,
  onPick,
  onChangeRequest,
}: {
  target: Device | null;
  targetValid: boolean;
  /** 链接带来的目标（`?peerId=`）。用来区分「没选」与「选了但那台设备已经不在了」。 */
  requestedPeerId: string;
  pickerOpen: boolean;
  devices: Device[];
  organization: Parameters<typeof organizedDeviceName>[1];
  disabled: boolean;
  onPick: (peerId: string) => void;
  onChangeRequest: () => void;
}) {
  const { t } = useLingui();

  if (target && !pickerOpen) {
    const Icon = deviceIcon(target.os);
    const online = target.status === "online";
    return (
      <div className="flex items-center gap-3 rounded-lg border bg-background p-3">
        <div
          className={cn(
            "flex size-10 shrink-0 items-center justify-center rounded-lg",
            online ? "bg-[var(--brand-solid)]/12 text-brand" : "bg-muted text-muted-foreground",
          )}
        >
          <Icon className="size-5" aria-hidden />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <p className="truncate text-sm font-medium text-foreground">
            {organizedDeviceName(target, organization)}
          </p>
          <span className="flex items-center gap-1.5 text-xs">
            <StatusDot colorClass={online ? "bg-emerald-500" : "bg-muted-foreground"} />
            <span
              className={online ? "text-success-ink" : "text-muted-foreground"}
            >
              {online ? <Trans>在线</Trans> : <Trans>离线</Trans>}
            </span>
          </span>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onChangeRequest}
          disabled={disabled}
          className="shrink-0 gap-1.5"
        >
          <ArrowLeftRight className="size-4" aria-hidden />
          <Trans>更换</Trans>
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-col gap-1.5">
        <span id="send-target-label" className="text-xs font-medium text-foreground">
          <Trans>目标设备</Trans>
        </span>
        {/* shadcn `Select` 而不是裸 `<select>`：同屏的设备卡、信任策略、解除配对都已经是
            shadcn 底座，一个原生下拉框在里面是明显的异物（焦点环、圆角、高度、暗色表现全
            不一样）。product register 那条「同一套组件词汇」说的就是这个。 */}
        <Select
          value={target?.peerId ?? ""}
          onValueChange={onPick}
          disabled={disabled}
        >
          <SelectTrigger
            aria-labelledby="send-target-label"
            className="h-11 w-full sm:h-9"
            data-testid="send-target-select"
          >
            <SelectValue placeholder={t`选择设备…`} />
          </SelectTrigger>
          <SelectContent>
            {devices.map((device) => (
              <SelectItem
                key={device.peerId}
                value={device.peerId}
                disabled={device.status !== "online"}
              >
                {organizedDeviceName(device, organization)}
                {device.status !== "online" ? t`（离线）` : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* 选了目标却发不出去时，把原因说出来——否则「发送」按钮只是灰着，而灰按钮不解释自己
          （PRODUCT.md 原则 2·状态诚实可见）。
          **两种情况要分开说**：设备还在但离线/被阻止，与设备已经不在配对列表里了。
          后者此前一句话都没有——守卫写的是 `target && !targetValid`，而设备被解除配对后
          `target === null`，条件根本不成立，用户点着一条别人给的 `?peerId=` 链接进来，
          看到的只是一个空下拉框。DESIGN.md 的 Send Entry Contract 第 4 条点名要求「say so in place」。 */}
      {target && !targetValid && (
        <p className="text-xs text-warning-ink">
          <Trans>这台设备当前离线或已被阻止，换一台再试。</Trans>
        </p>
      )}
      {!target && requestedPeerId && (
        <p className="text-xs text-warning-ink">
          <Trans>链接指向的设备不在已配对列表里（可能已解除配对）。从下面挑一台，或回设备页重新配对。</Trans>
        </p>
      )}

      <p className="text-xs text-muted-foreground">
        <Trans>
          也可以直接从{" "}
          <Link href={NAV.devices.href} className="font-medium text-brand underline underline-offset-2">
            设备
          </Link>{" "}
          页点某台设备的「发送」，目标会自动带过来。
        </Trans>
      </p>
    </div>
  );
}
