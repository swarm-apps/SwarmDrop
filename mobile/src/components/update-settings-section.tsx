import { type ReactNode, useEffect } from "react";
import { View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useUpdate } from "@/hooks/use-update";
import {
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";
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
  const { status, release, progress, error, check, download, install, retry } =
    useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isChecking = status === "checking";
  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const hasUpdate = status === "available" || status === "force-required";
  const busy = isDownloading || isReady;

  useEffect(() => {
    if (status === "ready") void install();
  }, [status, install]);

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
    <View className={cn("gap-4", className)}>
      <View className="flex-row items-center justify-between gap-4">
        <View className="flex-1 gap-0.5">
          <Text className="text-sm font-medium">{t.settingsTitle}</Text>
          {currentVersion ? (
            <Text className="text-muted-foreground text-[13px]">
              {t.currentVersionLabel(currentVersion)}
            </Text>
          ) : null}
        </View>
        {hasUpdate || busy ? (
          <Button onPress={() => void download()} disabled={busy}>
            <Text>{actionLabel}</Text>
          </Button>
        ) : (
          <Button
            variant="outline"
            onPress={() => void check(true)}
            disabled={isChecking}
          >
            <Text>{isChecking ? t.checkingButton : t.checkButton}</Text>
          </Button>
        )}
      </View>

      {status === "up-to-date" ? (
        <Text className="text-muted-foreground text-sm">{t.upToDate}</Text>
      ) : null}
      {hasUpdate && release ? (
        <Text className="text-sm">{t.updateAvailable(release.version)}</Text>
      ) : null}
      {hasUpdate && release?.notes ? (
        <View className="bg-muted rounded-lg p-3">
          <ReleaseNotesView
            notes={release.notes}
            renderer={releaseNotesRenderer}
          />
        </View>
      ) : null}

      {isReady ? (
        <Text className="text-primary-ink text-sm">{t.systemConfirmHint}</Text>
      ) : null}

      {isDownloading && progress ? (
        <View className="gap-2">
          <Progress value={percent} />
          <View className="flex-row justify-between">
            <Text className="text-muted-foreground text-[13px]">
              {percent}%
            </Text>
            {speedMb ? (
              <Text className="text-muted-foreground text-[13px]">
                {speedMb} MB/s
              </Text>
            ) : null}
          </View>
        </View>
      ) : null}

      {status === "error" ? (
        <View className="border-destructive/40 flex-row items-center justify-between gap-3 rounded-lg border p-3">
          <Text className="text-destructive-ink flex-1 text-sm">
            {error?.message ?? t.checkFailed}
          </Text>
          <Button variant="outline" size="sm" onPress={() => void retry()}>
            <Text>{t.retryButton}</Text>
          </Button>
        </View>
      ) : null}
    </View>
  );
}
