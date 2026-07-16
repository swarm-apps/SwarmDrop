import { Loader2 } from "lucide-react";
import { type ReactNode, useEffect } from "react";
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
import { forceDialogVisible } from "@/lib/update-dialog-visibility";
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

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const percent = progress ? Math.round(progress.percent * 100) : 0;

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
              {progress.speed ? (
                <span>{(progress.speed / 1024 / 1024).toFixed(1)} MB/s</span>
              ) : null}
            </div>
          </div>
        )}

        <Button
          className="w-full"
          onClick={() => void download()}
          disabled={isDownloading || isReady}
        >
          {isDownloading ? (
            <>
              <Loader2 className="mr-2 size-4 animate-spin" />
              {t.downloadingButton}
            </>
          ) : isReady ? (
            t.restartingButton
          ) : (
            t.updateButton
          )}
        </Button>
      </DialogContent>
    </Dialog>
  );
}
