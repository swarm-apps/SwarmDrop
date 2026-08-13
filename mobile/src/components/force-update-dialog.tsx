import type { ReactNode } from "react";
import { View } from "react-native";
import { ReleaseNotesView } from "@/components/release-notes-view";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import {
  forceDialogVisible,
  progressView,
} from "@/lib/update-dialog-visibility";
import {
  readyHintText,
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";
import { cn } from "@/lib/utils";

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
  const { status, release, progress, download } = useUpdate();
  const { blockedReason, autoAttemptSpent, install } = useAutoInstall();
  const t = resolveUpdateTexts(locale, texts);

  const isDownloading = status === "downloading";
  const isReady = status === "ready";
  const open = forceDialogVisible(status, release);

  const { percent, speedMb } = progressView(status, progress);
  const actionLabel = isDownloading
    ? t.downloadingButton
    : isReady
      ? t.installButton
      : t.updateButton;
  // 本弹窗不可关、只有这一个按钮 —— ready 态若也把它禁掉,用户就彻底没有出路了
  // (比普通更新流更糟:那边至少还有个 ×)。只有下载中才禁。
  const onAction = isReady ? install : download;

  return (
    // 软强制:AlertDialog 无关闭 X、不响应点遮罩 / 返回键关闭;无 dismiss 按钮。
    <AlertDialog open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t.forceTitle}</AlertDialogTitle>
          {release ? (
            <AlertDialogDescription>
              {currentVersion
                ? t.forceDescription(release.version, currentVersion)
                : t.updateAvailable(release.version)}
            </AlertDialogDescription>
          ) : null}
        </AlertDialogHeader>

        {release?.notes ? (
          <View className="bg-muted rounded-lg p-3">
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

        {isReady ? (
          <Text className="text-primary-ink text-sm">
            {readyHintText(t, blockedReason, autoAttemptSpent)}
          </Text>
        ) : null}

        <AlertDialogFooter>
          {/* AlertDialogAction(RNR canonical)不像 Button 那样在 disabled 时自动加 opacity-50,
              故禁用时在调用处补 opacity-50,保持禁用态的视觉反馈(不改 vendored 原语)。 */}
          <AlertDialogAction
            className={cn("flex-1", isDownloading && "opacity-50")}
            onPress={() => void onAction()}
            disabled={isDownloading}
          >
            <Text>{actionLabel}</Text>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
