import { ActivityIndicator, View } from "react-native";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useUpdate } from "@/hooks/use-update";
import {
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
}

export function UpdateProgressDialog({
  locale,
  texts,
  open,
}: UpdateProgressDialogProps) {
  const { status, progress } = useUpdate();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? (status === "downloading" || isReady);
  const percent = progress ? Math.round(progress.percent * 100) : 0;
  const speedMb = progress?.speed
    ? (progress.speed / 1024 / 1024).toFixed(1)
    : null;

  return (
    <AlertDialog open={visible}>
      {/* 无 AlertDialogFooter/Action:这是不可关闭的纯进度视图,status 离开 downloading/ready 时
          自动隐藏(open=false),无需任何按钮。 */}
      <AlertDialogContent className="sm:max-w-sm">
        <AlertDialogHeader>
          <View className="flex-row items-center gap-2">
            {/* 下载中转圈(等价 web 的 Loader2 spinner);ready 态在等系统安装器,不转圈。 */}
            {isReady ? null : <ActivityIndicator size="small" />}
            <AlertDialogTitle>
              {isReady ? t.systemConfirmHint : t.progressTitle}
            </AlertDialogTitle>
          </View>
        </AlertDialogHeader>
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
      </AlertDialogContent>
    </AlertDialog>
  );
}
