import { ActivityIndicator, View } from "react-native";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Progress } from "@/components/ui/progress";
import { Text } from "@/components/ui/text";
import { useAutoInstall } from "@/hooks/use-auto-install";
import { useUpdate } from "@/hooks/use-update";
import {
  progressDialogVisible,
  progressView,
} from "@/lib/update-dialog-visibility";
import {
  readyHintText,
  resolveUpdateTexts,
  type UpdateLocale,
  type UpdateTexts,
} from "@/lib/update-texts";

export interface UpdateProgressDialogProps {
  locale?: UpdateLocale;
  texts?: Partial<UpdateTexts>;
  /** 覆盖可见性;缺省按 status(downloading / ready)自动显示。 */
  open?: boolean;
  /**
   * 用户请求收起本弹窗。**只隐藏 UI** —— 下载继续、status 不变、产物不丢。
   *
   * **必填**:可选的话就能装配出一个既关不掉又没按钮的模态框(AlertDialog 本身不响应
   * 返回键与遮罩),而那正是本次要修的东西。让类型系统守住这条 No Dead End 规则。
   */
  onDismiss: () => void;
}

export function UpdateProgressDialog({
  locale,
  texts,
  open,
  onDismiss,
}: UpdateProgressDialogProps) {
  const { status, release, progress } = useUpdate();
  const { blockedReason, autoAttemptSpent, install } = useAutoInstall();
  const t = resolveUpdateTexts(locale, texts);

  const isReady = status === "ready";
  const visible = open ?? progressDialogVisible(status, release);
  const { percent, speedMb } = progressView(status, progress);

  return (
    <AlertDialog open={visible}>
      <AlertDialogContent className="sm:max-w-sm">
        <AlertDialogHeader>
          <View className="flex-row items-center gap-2">
            {/* 下载中转圈(等价 web 的 Loader2 spinner);ready 已经不在传输了,不转圈。 */}
            {isReady ? null : <ActivityIndicator size="small" />}
            <AlertDialogTitle>
              {isReady
                ? readyHintText(t, blockedReason, autoAttemptSpent)
                : t.progressTitle}
            </AlertDialogTitle>
          </View>
        </AlertDialogHeader>
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
        {/* `flex-1`：footer 是 flex-row 且不替调用方分配宽度。这里两种形态都要对——
            ready 时双键各占一半，否则单键占满整行。 */}
        <AlertDialogFooter>
          <AlertDialogCancel className="flex-1" onPress={onDismiss}>
            <Text>{isReady ? t.laterButton : t.backgroundButton}</Text>
          </AlertDialogCancel>
          {isReady ? (
            <AlertDialogAction
              className="flex-1"
              onPress={() => void install()}
            >
              <Text>{t.installButton}</Text>
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
