"use client";

// 把已接收的文件转发给另一台设备。
//
// ## 为什么需要它
//
// 收件箱里的文件躺在 OPFS 里——浏览器的配额存储，规范上就定义为对用户不可见。用户没有
// 「在文件管理器里选中它再发出去」这条路（那正是桌面端习以为常的做法），所以转发必须由
// 应用自己提供入口。后端不缺能力：`send_inbox_files` 只是把 OPFS 句柄取成 `File`，
// 之后与用户选文件发送完全同路。
//
// ## 形态上与移动端有意分叉
//
// 三端共用「文件已定 → 挑设备」这条反向流，但呈现允许按平台走各自合适的那种：移动端
// 推一屏（触摸设备上弹层会盖掉半个屏幕），Web 用对话框（有指针，浮层不占版面）。
// 这与 Node Status Contract 允许 popover / 就地展开分叉是同一条道理。
//
// ## 不做多选
//
// 入口有两级：整条记录（收件箱详情的头部动作）与单个文件（文件行的动作）。「选其中三个」
// 是第三种，需要给 FileBrowser 引入选择态——那是一笔独立的 UI 投资，收益却只覆盖前两者
// 之间的窄缝。

import { Trans, useLingui } from "@lingui/react/macro";
import { LoaderCircle, Send } from "lucide-react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { useMemo, useState } from "react";
import { canSendToDevice, organizedDeviceName } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/cn";
import { deviceIcon } from "../_lib/device-presentation";
import { transferSessionHref } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { usePreferences } from "../_lib/preferences-store";
import { useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import type { InboxItemFileEntry } from "../_lib/view-types";
import { CenteredEmptyState } from "./empty-state";
import { PrepareProgressRow } from "./prepare-progress";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

export function ForwardToDeviceDialog({
  open,
  onOpenChange,
  files,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 要转发的文件。空数组时不渲染主体——见 `shown`。 */
  files: InboxItemFileEntry[];
}) {
  const { t } = useLingui();
  const router = useRouter();
  const devices = useWebNode((s) => s.pairedDevices);
  const organization = usePreferences((s) => s.deviceOrganization);
  const action = useAsyncAction();
  /**
   * 正在发往哪台设备。`useAsyncAction` 只知道「有动作在跑」，不知道是哪一行——不带这个
   * 的话单次发送会让所有设备行一起转圈。
   *
   * 不需要在失败后清掉它：spinner 的条件里带着 `action.pending`，而失败时 `useAsyncAction`
   * 已经把它置回 false，那一行自然停转。
   */
  const [sendingTo, setSendingTo] = useState<string | null>(null);

  // 离线设备不列：转发是一次即时发送，摆一个必然失败的目标只会让用户点完才知道。
  const sendable = useMemo(
    () => devices.filter((device) => canSendToDevice(device)),
    [devices],
  );

  /**
   * 关闭即作废在途结果与错误。`useAsyncAction` 的 `run` 只处理「新一轮顶掉旧一轮」，
   * **不处理「转入无动作态」**——不调 `cancel`，下次为另一个文件打开对话框时，上一次
   * 失败的错误卡片还挂在那儿。
   */
  const close = () => {
    action.cancel();
    setSendingTo(null);
    onOpenChange(false);
  };

  const send = (peerId: string) => {
    const node = getNode();
    if (!node || files.length === 0) return;
    setSendingTo(peerId);
    webNodeActions.clearPrepare();
    action.run(
      () =>
        node.send_inbox_files(
          peerId,
          files.map((file) => file.relativePath),
        ),
      (sessionId) => {
        // 取不到的条目被内核跳过而不是让整批失败（OPFS 是配额存储，条目可能被驱逐）。
        // 跳了什么必须说——否则用户以为七个文件都发出去了，实际只走了五个。
        const skipped = node.take_skipped_forward_paths();
        if (skipped.length > 0) {
          toast.warning(t`已跳过 ${skipped.length} 个文件`, {
            description: t`它们已不在浏览器存储里。`,
          });
        }
        close();
        router.push(transferSessionHref(sessionId));
      },
      // 收尾挂 settle 而不是 success：转发失败时也要收掉进度行。
      webNodeActions.clearPrepare,
    );
  };


  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) onOpenChange(true);
        else close();
      }}
    >
      {/* 主体挂在 `files.length > 0` 上：父层一关就把待转发列表置空，而 Radix 的
          `DialogContent` 还要播 ~150ms 退场动画——期间渲染空数组会闪出
          「把这 0 个文件发给…」。 */}
      <DialogContent className="sm:max-w-md">
        {files.length === 0 ? null : (
        <>
        <DialogHeader>
          <DialogTitle>
            <Trans>发送到设备</Trans>
          </DialogTitle>
          <DialogDescription>
            {files.length === 1 ? (
              <Trans>把「{files[0].name}」发给另一台已配对的设备。</Trans>
            ) : (
              <Trans>
                把这 {files.length} 个文件发给另一台已配对的设备。
              </Trans>
            )}
          </DialogDescription>
        </DialogHeader>

        {sendable.length === 0 ? (
          <CenteredEmptyState
            icon={Send}
            title={<Trans>没有可发送的设备</Trans>}
            description={
              <Trans>已配对的设备都不在线，等对方上线后再试。</Trans>
            }
          />
        ) : (
          <ul className="flex flex-col gap-1.5">
            {sendable.map((device) => {
              const Icon = deviceIcon(device.os);
              const name = organizedDeviceName(device, organization);
              return (
                <li key={device.peerId}>
                  <button
                    type="button"
                    onClick={() => send(device.peerId)}
                    disabled={action.pending}
                    aria-label={t`发送到 ${name}`}
                    className={cn(
                      "flex w-full items-center gap-3 rounded-lg border bg-background p-3 text-left",
                      "transition-colors hover:bg-muted disabled:opacity-50",
                    )}
                  >
                    <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--brand-solid)]/12 text-brand">
                      <Icon className="size-5" aria-hidden />
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-sm font-medium text-foreground">
                        {name}
                      </span>
                      <span className="flex items-center gap-1.5 text-xs">
                        <StatusDot colorClass="bg-success" />
                        <span className="text-success-ink">
                          <Trans>在线</Trans>
                        </span>
                      </span>
                    </span>
                    {sendingTo === device.peerId && action.pending ? (
                      <LoaderCircle
                        className="size-4 shrink-0 animate-spin text-muted-foreground"
                        aria-hidden
                      />
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}

        {/* 转发走的是与发送面板同一个 `send_files()`，准备阶段一样可能是分钟级。
            此前这里只有设备行末尾一个转圈——用户完全不知道在等什么。 */}
        <PrepareProgressRow />

        {action.error ? <WebErrorCard error={action.error} /> : null}

        <Button
          type="button"
          variant="ghost"
          onClick={close}
          disabled={action.pending}
          className="self-end"
        >
          <Trans>取消</Trans>
        </Button>
        </>
        )}
      </DialogContent>
    </Dialog>
  );
}
