import { Download, FileText, Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { progressView } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface PromptUpdateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** release notes 渲染器(如接 Markdown);缺省纯文本。 */
  releaseNotesRenderer?: (notes: string) => ReactNode;
  /** 当前版本,用于描述文案;缺省只显示新版本。 */
  currentVersion?: string;
}

export function PromptUpdateDialog({
  open,
  onOpenChange,
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: PromptUpdateDialogProps) {
  const { status, release, progress, download, install, postpone } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";

  // 任意方式关闭弹窗(Esc / 点遮罩 / Close X / 「稍后」按钮)都记一次 postpone(),避免下次 window
  // focus 复核时立刻重弹;下载中 / ready 时只隐藏 UI、不 postpone。
  const handleOpenChange = (next: boolean) => {
    if (!next && !isDownloading && !isReady) void postpone();
    onOpenChange(next);
  };

  // **ready 的主按钮必须可点**(No Dead End 规则):安装失败或被取消后,它是用户唯一的
  // 重试入口。从前它与 downloading 共用一个 disabled 判据,于是一旦停在 ready 就没救了。
  const onAction = isReady ? install : download;

  const { percent, speedMb } = progressView(status, progress);

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download className="size-5" />
            {t.promptTitle}
          </DialogTitle>
          {release && (
            <DialogDescription>
              {currentVersion
                ? t.promptDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </DialogDescription>
          )}
        </DialogHeader>

        {release?.notes && (
          <div className="rounded-lg bg-muted p-4">
            <div className="mb-2 flex items-center gap-2 text-xs font-medium text-muted-foreground">
              <FileText className="size-4" />
              {t.releaseNotesLabel}
            </div>
            <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
          </div>
        )}

        {isDownloading && progress && (
          <div className="space-y-2">
            <Progress value={percent} />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{percent}%</span>
              {speedMb ? <span>{speedMb} MB/s</span> : null}
            </div>
          </div>
        )}

        <DialogFooter className="gap-2">
          <Button
            variant="outline"
            onClick={() => handleOpenChange(false)}
            disabled={isDownloading}
          >
            {t.laterButton}
          </Button>
          <Button onClick={() => void onAction()} disabled={isDownloading}>
            {isDownloading ? (
              <>
                <Loader2 className="mr-2 size-4 animate-spin" />
                {t.downloadingButton}
              </>
            ) : isReady ? (
              t.installButton
            ) : (
              t.updateButton
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
