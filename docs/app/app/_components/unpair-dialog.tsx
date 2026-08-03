"use client";

// 取消配对的确认与执行。
//
// ## 为什么这里的确认是「等异步结果」的
//
// 应用区有两个确认原语，这是刻意的：`ConfirmAction`（清空历史 / 取消传输 / 删除记录）**不等**
// 异步结果，点确认的同一拍就复位；这一份**等**。判据是**动作失败后用户还想不想留在
// 「我要删这个」的状态里**。
//
// 取消配对是一次可失败的网络操作（节点没起来、内核报错都可能），失败后让用户重新走
// 「触发 → 确认」两步才能重试是惩罚。所以这里保持对话框开着、错误就地显示，重试只差一次点击。
//
// 这条判据原本记在 `device-list.tsx` 的顶部注释里；那个文件已被本卡片体系取代，判据搬到这里。
//
// 解除本身走内核的 `remove_paired_device()`（core 的原子 unpair：先落盘 → 再删共享内存表 →
// 再发事件），所以失败时设备**还在**清单里、错误也说得出来，不会出现「点完就没了、刷新又回来」。

import { Trans } from "@lingui/react/macro";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { getNode } from "../_lib/node-runtime";
import { preferencesActions } from "../_lib/preferences-store";
import { useAsyncAction } from "../_lib/use-async-action";
import { webNodeActions } from "../_lib/store";
import { WebErrorCard } from "./web-error-view";

export function UnpairDialog({
  open,
  onOpenChange,
  peerId,
  deviceName,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  peerId: string;
  deviceName: string;
}) {
  const action = useAsyncAction();

  const doUnpair = () => {
    const node = getNode();
    if (!node) return;
    action.run(
      () => node.remove_paired_device(peerId),
      () => {
        // 内核那边这台设备已经从共享表里摘掉了，这里立刻取一份新快照——等下一轮 state-poll
        // 的话，点完之后它还会在列表里挂最多 1.5 秒，看着像是没生效。
        webNodeActions.setPairedDevices(node.paired_devices());
        // 别名与分组是本机偏好，内核不知道它们的存在——不在这里清就会留下幽灵条目，
        // 将来这个 PeerId 再次配对时还会顶着上一段关系的名字回来。
        preferencesActions.forgetDevice(peerId);
        onOpenChange(false);
      },
    );
  };

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Trans>取消配对</Trans>
          </AlertDialogTitle>
          <AlertDialogDescription>
            <Trans>
              确定要取消与「{deviceName}」的配对吗？取消后需要重新配对才能传输文件。
            </Trans>
          </AlertDialogDescription>
        </AlertDialogHeader>

        {action.error && <WebErrorCard error={action.error} className="text-xs" />}

        <AlertDialogFooter>
          <AlertDialogCancel disabled={action.pending} data-testid="device-unpair-cancel">
            <Trans>取消</Trans>
          </AlertDialogCancel>
          <AlertDialogAction
            // 不自动关闭：失败时对话框要留在原位、错误就地显示（见文件头部的判据）。
            onClick={(event) => {
              event.preventDefault();
              doUnpair();
            }}
            disabled={action.pending}
            data-testid="device-unpair-confirm"
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            {action.pending ? <Trans>取消配对中…</Trans> : <Trans>确认取消配对</Trans>}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
