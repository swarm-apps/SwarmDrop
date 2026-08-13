import { Download, FolderOpen, LoaderCircle, LocateFixed, RotateCcw, Send, X } from "lucide-react";
import { useLingui } from "@lingui/react/macro";
import { cn } from "./cn";
import type { FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserActions, FileBrowserTarget } from "./types";

interface ActionButtonProps {
  label: string;
  onClick?: () => void;
  destructive?: boolean;
  disabled?: boolean;
  children: React.ReactNode;
}

function ActionButton({
  label,
  onClick,
  destructive,
  disabled,
  children,
}: ActionButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick?.();
      }}
      className={cn(
        "inline-flex size-7 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors",
        "hover:bg-foreground/[0.06] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
        "disabled:pointer-events-none disabled:opacity-35",
        destructive && "hover:bg-destructive/10 hover:text-destructive",
      )}
    >
      {children}
    </button>
  );
}

export function RemoveAction({
  target,
  onRemove,
}: {
  target: FileBrowserTarget;
  onRemove?: FileBrowserActions["onRemove"];
}) {
  const { t } = useLingui();
  if (!onRemove) return null;
  return (
    <ActionButton label={t`移除`} destructive onClick={() => onRemove(target)}>
      <X className="size-3.5" />
    </ActionButton>
  );
}

/**
 * 取回按钮。文件行与目录行共用——两者的差别只在 target 与「有没有 `missing` 这回事」，
 * 按钮本身（pending 转 spinner、禁用、图标、无障碍名）是同一个东西，不该有两份。
 *
 * `onDownload` 必填、由调用点判在不在：本组件调 `useLingui()`（钩子规则要求它在任何
 * 提前 return 之前），所以「渲染出来再返回 null」在桌面（不传 `onDownload`）的每一行上
 * 都是一个白挂的 fiber 加一次 context 读，而那是几百行的列表。
 */
function DownloadAction({
  target,
  pending,
  disabled,
  onDownload,
}: {
  target: FileBrowserTarget;
  pending: boolean;
  disabled?: boolean;
  onDownload: NonNullable<FileBrowserActions["onDownload"]>;
}) {
  const { t } = useLingui();
  return (
    <ActionButton
      label={pending ? t`正在准备下载` : t`下载`}
      disabled={disabled || pending}
      onClick={() => onDownload(target)}
    >
      {pending ? (
        <LoaderCircle className="size-3.5 animate-spin" />
      ) : (
        <Download className="size-3.5" />
      )}
    </ActionButton>
  );
}

/**
 * 这组动作里有没有**主动作**——有的话动作条常驻，而不是 hover 才露出。
 *
 * 主动作 = 把文件从沙盒里弄出去的那条路。浏览器端有两条：`onDownload` 取回本机，
 * `onSend` 送去另一台设备；那一端没有「打开文件」也没有「在文件夹中显示」，藏起这两个
 * 就等于藏起整个出口，而收件箱存在的理由正是「把收到的东西弄走」。桌面端的动作全是
 * 便利项（文件早就落盘了），继续 hover 才露出。
 *
 * 判据是「这组动作里有没有主动作」，**不是「跑在哪一端」**——组件仍然不认识平台，
 * 与 `FileBrowserActions` 那条「不传就不渲染」的并集约定同一个路子。
 *
 * **模块内私有**：它算的是「文件那条动作条有没有主动作」，目录那条渲染不出 `onSend`、
 * 自己判 `onDownload`。导出会让人以为这是个跨模块契约，从而不敢按需改它。
 */
function hasPrimaryAction(actions?: FileBrowserActions): boolean {
  return Boolean(actions?.onDownload || actions?.onSend);
}

/**
 * 动作条外壳：布局 + 阻断冒泡 + 「有主动作才常驻」的可见性。
 *
 * **可见性判据住在这里**，不在三个调用点各写一遍：行/卡组件不知道自己这条动作条最终
 * 渲染得出哪些按钮，而这里知道。`pointer-coarse:opacity-100` 兜住触摸屏——那里没有
 * hover，藏起来就等于整个入口不存在（Web 端唯一的取回出口就在这条动作条里）。
 *
 * 冒泡必须挡：动作条嵌在一整行/整卡可点击的区域里，不挡的话点下载会顺带折叠目录、
 * 或触发卡片的「打开」。
 */
export function ActionBar({
  primary,
  className,
  children,
}: {
  primary: boolean;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-0.5",
        !primary &&
          "opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100",
        className,
      )}
      onClick={(event) => event.stopPropagation()}
    >
      {children}
    </div>
  );
}

export function FileItemActions({
  item,
  actions,
  className,
}: {
  item: FileBrowserItem;
  actions?: FileBrowserActions;
  className?: string;
}) {
  const { t } = useLingui();
  const isMissing = item.status === "missing";
  const isPending = actions?.pendingIds?.has(item.id) ?? false;
  if (!actions) return null;

  return (
    <ActionBar primary={hasPrimaryAction(actions)} className={className}>
      {actions.onDownload && (
        <DownloadAction
          target={{ type: "file", item }}
          pending={isPending}
          disabled={isMissing}
          onDownload={actions.onDownload}
        />
      )}
      {actions.onSend && (
        <ActionButton
          label={t`发送到设备`}
          disabled={isMissing}
          onClick={() => actions.onSend?.(item)}
        >
          <Send className="size-3.5" />
        </ActionButton>
      )}
      {actions.onOpen && (
        <ActionButton
          label={t`打开文件`}
          disabled={isMissing}
          onClick={() => actions.onOpen?.(item)}
        >
          <FolderOpen className="size-3.5" />
        </ActionButton>
      )}
      {actions.onReveal && (
        <ActionButton
          label={t`在文件夹中显示`}
          disabled={isMissing}
          onClick={() => actions.onReveal?.(item)}
        >
          <LocateFixed className="size-3.5" />
        </ActionButton>
      )}
      {actions.onRetry && item.status === "error" && (
        <ActionButton label={t`重试`} onClick={() => actions.onRetry?.(item)}>
          <RotateCcw className="size-3.5" />
        </ActionButton>
      )}
      {actions.onRemove && (
        <RemoveAction
          target={{ type: "file", item }}
          onRemove={actions.onRemove}
        />
      )}
    </ActionBar>
  );
}

/**
 * 目录行的动作条。
 *
 * 只有取回与移除两项：`onOpen` / `onReveal` / `onRetry` / `onSend` 都是绑在单个文件上的
 * 概念，目录没有对应物（「打开一个目录」在浏览器里不成立，转发整个目录则要先决定发哪些
 * 文件——那是调用方的业务判断，不是这条行的动作）。
 *
 * **收 `{ id, relativePath }` 而不是整个树节点**：本组件只需要这两个字段，而树节点在 L1 是
 * 嵌套形态、在 `headless-tree-adapter` 里又多一个虚拟根变体，写死任一种都会逼另一种做断言。
 */
export function FolderItemActions({
  relativePath,
  actions,
  className,
}: {
  /** 目录的相对路径（带尾斜杠）。它同时是 target 的身份与 `pendingIds` 的查询键。 */
  relativePath: string;
  actions?: FileBrowserActions;
  className?: string;
}) {
  if (!actions) return null;
  const target: FileBrowserTarget = { type: "directory", relativePath };

  return (
    // **`primary` 只看 `onDownload`**，不用 `hasPrimaryAction`：那个判据含 `onSend`，
    // 而本组件渲染不出 send——只传 `onSend` 的调用方会得到一条常驻的、里面只剩「移除」
    // 的动作条，而常驻是为主动作服务的。判据要贴着「本条动作条实际给得出什么」。
    <ActionBar primary={Boolean(actions.onDownload)} className={className}>
      {actions.onDownload && (
        <DownloadAction
          target={target}
          pending={actions.pendingIds?.has(relativePath) ?? false}
          onDownload={actions.onDownload}
        />
      )}
      {/* 与上面一致由调用点判在不在：`RemoveAction` 同样先调 `useLingui()` 再可能
          return null，而 Web 收件箱不传 `onRemove`——每个目录行都会白挂一个 fiber。 */}
      {actions.onRemove && (
        <RemoveAction target={target} onRemove={actions.onRemove} />
      )}
    </ActionBar>
  );
}
