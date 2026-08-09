/**
 * PrepareProgressBar — 发送准备阶段（一遍流式读产出校验和 + 验签树）的进度条。
 *
 * 两个发送入口（/send/select-device 与 /send/share-target）共用这一份。此前它们各写各
 * 的：一个说「正在计算校验和」、另一个说「正在准备」，显示门也不一致（一个 `progress ?`、
 * 另一个 `sending && progress`），share-target 那份还不显示当前文件名。同一件事在同一个
 * 应用里说两种话，没有理由。
 *
 * 文案取「正在准备」：用户不关心校验和是什么，只关心还要等多久。
 */

import { Trans } from "@lingui/react/macro";
import { View } from "react-native";

import { Text } from "@/components/ui/text";
import type { MobilePrepareProgress } from "react-native-swarmdrop-core";

import {
  calcPercent,
  formatBytes,
  ProgressBar,
} from "@/components/transfer/shared";

export function PrepareProgressBar({
  progress,
}: {
  progress: MobilePrepareProgress;
}) {
  const total = Number(progress.totalBytes);
  const hashed = Number(progress.bytesHashed);
  return (
    <View className="flex-1 gap-2">
      <View className="flex-row items-center justify-between gap-3">
        <Text
          className="flex-1 text-[13px] text-muted-foreground"
          numberOfLines={1}
        >
          <Trans>
            正在准备 ({progress.completedFiles.toString()}/
            {progress.totalFiles.toString()})
          </Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {formatBytes(hashed)} / {formatBytes(total)}
        </Text>
      </View>
      <ProgressBar percent={calcPercent(hashed, total)} heightClass="h-1.5" />
      {/* 收尾事件**保留**最后一个文件名（见 `flow/prepare.rs::emit_final`），所以正常
          路径上这行不会消失、操作栏高度也不跳。判空只是防御，别据此以为置空还在发生。 */}
      {progress.currentFile ? (
        <Text className="text-[12px] text-muted-foreground" numberOfLines={1}>
          {progress.currentFile}
        </Text>
      ) : null}
    </View>
  );
}
