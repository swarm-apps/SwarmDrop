import { Loader2 } from "lucide-react";
import { type ReactNode, useEffect } from "react";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";
import { cn } from "@/lib/utils";

export interface UpdateSettingsSectionProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  releaseNotesRenderer?: (notes: string) => ReactNode;
  currentVersion?: string;
  className?: string;
}

export function UpdateSettingsSection({
  locale,
  texts,
  releaseNotesRenderer,
  currentVersion,
  className,
}: UpdateSettingsSectionProps) {
  const { status, release, progress, error, check, download, install, retry } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isChecking = status === "checking";
  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const hasUpdate = status === "available" || status === "force-required";

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  const percent = progress ? Math.round(progress.percent * 100) : 0;

  return (
    <div className={cn("space-y-4", className)}>
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm font-medium">{t.settingsTitle}</p>
          {currentVersion && (
            <p className="text-xs text-muted-foreground">{t.currentVersionLabel(currentVersion)}</p>
          )}
        </div>
        {hasUpdate ? (
          <Button onClick={() => void download()} disabled={isDownloading || isReady}>
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
        ) : (
          <Button variant="outline" onClick={() => void check(true)} disabled={isChecking}>
            {isChecking ? (
              <>
                <Loader2 className="mr-2 size-4 animate-spin" />
                {t.checkingButton}
              </>
            ) : (
              t.checkButton
            )}
          </Button>
        )}
      </div>

      {status === "up-to-date" && <p className="text-sm text-muted-foreground">{t.upToDate}</p>}
      {hasUpdate && release && <p className="text-sm">{t.updateAvailable(release.version)}</p>}
      {hasUpdate && release?.notes && (
        <div className="rounded-lg bg-muted p-3">
          <ReleaseNotesView notes={release.notes} renderer={releaseNotesRenderer} />
        </div>
      )}

      {isDownloading && progress && (
        <div className="space-y-2">
          <Progress value={percent} />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>{percent}%</span>
            {progress.speed ? <span>{(progress.speed / 1024 / 1024).toFixed(1)} MB/s</span> : null}
          </div>
        </div>
      )}

      {status === "error" && (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-destructive/40 p-3 text-sm text-destructive">
          <span>{error?.message ?? t.checkFailed}</span>
          <Button variant="outline" size="sm" onClick={() => void retry()}>
            {t.retryButton}
          </Button>
        </div>
      )}
    </div>
  );
}
