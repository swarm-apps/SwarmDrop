import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { progressDialogVisible, progressView } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /**
   * 覆盖可见性。缺省只在非强制流的 downloading / ready 显示;prompt 弹窗自带内联进度,
   * 宿主应传 `open={!promptOpen && progressDialogVisible(status, release)}` 让本弹窗只在
   * 用户主动关掉 prompt 后接管——否则两个弹窗同框。
   */
  open?: boolean;
  /**
   * 用户请求收起本弹窗。**只隐藏 UI** —— 下载继续、status 不变、产物不丢。
   *
   * **必填**:可选的话就能装配出一个既关不掉又没按钮的模态框,而那正是本次要修的东西。
   * 让类型系统守住这条,比在四份拷贝里各写一句注释可靠。
   */
  onDismiss: () => void;
}

export function UpdateProgressDialog({
  locale,
  texts,
  open,
  onDismiss,
}: UpdateProgressDialogProps) {
  const { status, release, progress, install } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? progressDialogVisible(status, release);
  const { percent, speedMb } = progressView(status, progress);

  return (
    <Dialog open={visible} onOpenChange={(next) => !next && onDismiss()}>
      {/* 出口:点遮罩 / Esc / 关闭按钮都走 onOpenChange → onDismiss。 */}
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {/* 下载中转圈;ready 已经不在传输了,不转圈。 */}
            {isReady ? null : <Loader2 className="size-5 animate-spin" />}
            {isReady ? t.readyHint : t.progressTitle}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-2">
          <Progress value={percent} />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>{percent}%</span>
            {speedMb ? <span>{speedMb} MB/s</span> : null}
          </div>
        </div>
        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={onDismiss}>
            {isReady ? t.laterButton : t.backgroundButton}
          </Button>
          {isReady ? <Button onClick={() => void install()}>{t.installButton}</Button> : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
