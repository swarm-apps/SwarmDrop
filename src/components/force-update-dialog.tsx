import { Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { forceDialogVisible, progressView } from "@/lib/update-dialog-visibility";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface ForceUpdateDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
}

export function ForceUpdateDialog({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
}: ForceUpdateDialogProps) {
  const { status, release, progress, download, install } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const open = forceDialogVisible(status, release);

  const { percent, speedMb } = progressView(status, progress);

  return (
    <Dialog open={open}>
      <DialogContent
        className="sm:max-w-md"
        onPointerDownOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t.forceTitle}</DialogTitle>
          {release && (
            <DialogDescription>
              {currentVersion
                ? t.forceDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </DialogDescription>
          )}
        </DialogHeader>

        {release?.notes && (
          <div className="max-h-48 overflow-y-auto rounded-lg bg-muted p-3">
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

        {/* 本弹窗不可关、只有这一个按钮 —— ready 态若也禁掉,用户就彻底没有出路了
            (安装被 UAC 取消时 status 会停在 ready)。只有下载中才禁。 */}
        <Button
          className="w-full"
          onClick={() => void (isReady ? install() : download())}
          disabled={isDownloading}
        >
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
      </DialogContent>
    </Dialog>
  );
}
