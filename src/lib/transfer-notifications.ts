/**
 * 传输通知副作用
 *
 * 订阅传输生命周期里「纯 toast 提示」的事件（failed / paused / rejected / dbError），
 * 与 store 的 projection / progress / offer 状态同步订阅解耦。返回聚合后的 unlisten。
 */

import { events } from "@/lib/bindings";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";

export async function setupTransferNotifications(): Promise<() => void> {
  const fns = await Promise.all([
    events.transferFailed.listen((event) => {
      const { error } = event.payload;
      if (error.startsWith("对方取消")) {
        toast.info(t`对方已取消传输`);
      } else {
        toast.error(error || t`传输失败`);
      }
    }),

    events.transferPaused.listen(() => {
      toast.info(t`对方已暂停传输`);
    }),

    events.transferRejected.listen((event) => {
      const { reason } = event.payload;
      if (reason?.type === "not_paired") {
        toast.error(t`设备已取消配对`);
      } else {
        toast.error(t`对方拒绝了请求`);
      }
    }),

    events.transferDbError.listen((event) => {
      toast.error(event.payload.message);
    }),
  ]);

  return () => {
    for (const unlisten of fns) {
      unlisten();
    }
  };
}
