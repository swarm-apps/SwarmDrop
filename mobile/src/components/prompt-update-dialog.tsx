import { type ReactNode, useEffect } from "react";
import { View } from "react-native";
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
import { Text } from "@/components/ui/text";
import { useUpdate } from "@/hooks/use-update";
import {
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";

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
  const { status, release, progress, download, install, postpone } =
    useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const busy = isDownloading || isReady;

  // 下载完成(ready)→ 自动拉起系统安装器(install 是 fire-and-forget:engine 不离开 ready;
  // 进程会在 replace 时被杀,这里不会再收到 resolve)。
  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

  // 任意方式关闭弹窗(返回键 / 点遮罩 / Close X / 「稍后」按钮)都记一次 postpone(),避免下次回
  // 前台(AppState 'active' 复核)立刻重弹;busy(下载中 / ready)时只隐藏 UI、不 postpone。
  const handleOpenChange = (next: boolean) => {
    if (!next && !busy) void postpone();
    onOpenChange(next);
  };

  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed
    ? (progress.speed / 1024 / 1024).toFixed(1)
    : null;
  const actionLabel = isDownloading
    ? t.downloadingButton
    : isReady
      ? t.installButton
      : t.updateButton;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t.promptTitle}</DialogTitle>
          {release ? (
            <DialogDescription>
              {currentVersion
                ? t.promptDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </DialogDescription>
          ) : null}
        </DialogHeader>

        {release?.notes ? (
          <View className="bg-muted gap-2 rounded-lg p-4">
            <Text className="text-muted-foreground text-xs font-medium">
              {t.releaseNotesLabel}
            </Text>
            <ReleaseNotesView
              notes={release.notes}
              renderer={releaseNotesRenderer}
            />
          </View>
        ) : null}

        {isDownloading && progress ? (
          <View className="gap-2">
            <Progress value={percent} />
            <View className="flex-row justify-between">
              <Text className="text-muted-foreground text-xs">{percent}%</Text>
              {speedMb ? (
                <Text className="text-muted-foreground text-xs">
                  {speedMb} MB/s
                </Text>
              ) : null}
            </View>
          </View>
        ) : null}

        {isReady ? (
          <Text className="text-primary-ink text-sm">
            {t.systemConfirmHint}
          </Text>
        ) : null}

        <DialogFooter>
          <Button
            className="flex-1"
            variant="outline"
            onPress={() => handleOpenChange(false)}
            disabled={busy}
          >
            <Text>{t.laterButton}</Text>
          </Button>
          <Button
            className="flex-1"
            onPress={() => void download()}
            disabled={busy}
          >
            <Text>{actionLabel}</Text>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
