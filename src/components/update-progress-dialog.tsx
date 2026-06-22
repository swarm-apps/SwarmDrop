import { Loader2 } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { resolveUpdateTexts, type UpdateLocale, type UpdateTexts } from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
}

export function UpdateProgressDialog({ locale, texts, open }: UpdateProgressDialogProps) {
  const { status, progress } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const visible = open ?? (status === "downloading" || status === "ready");
  const percent = progress ? Math.round(progress.percent * 100) : 0;

  return (
    <Dialog open={visible}>
      <DialogContent
        className="sm:max-w-sm"
        onPointerDownOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Loader2 className="size-5 animate-spin" />
            {t.progressTitle}
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-2">
          <Progress value={percent} />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>{percent}%</span>
            {progress?.speed ? <span>{(progress.speed / 1024 / 1024).toFixed(1)} MB/s</span> : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
