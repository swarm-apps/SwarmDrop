/**
 * PrepareProgressBar — 发送准备阶段（一遍流式读产出校验和 + 验签树）的进度条。
 *
 * 两个发送入口（/send/select-device 与 /send/share-target）共用这一份。此前它们各写各
 * 的：一个说「正在计算校验和」、另一个说「正在准备」，显示门也不一致（一个 `progress ?`、
 * 另一个 `sending && progress`），share-target 那份还不显示当前文件名。同一件事在同一个
 * 应用里说两种话，没有理由。
 *
 * 文案取「正在准备」：用户不关心校验和是什么，只关心还要等多久。
 *
 * **视觉上必须与传输进度条区分开。** 两者背靠背出现、共用同一个 `ProgressBar` 原语：
 * 准备条走完归零、传输条才从 0 起步，在 1.99 GB / 60 s 的量级下会被读成「续传又从头来了」。
 * 语义定死并写进 DESIGN.md 的 `Transfer Progress Contract`：
 * **灰（`bg-muted-foreground`）= 本机在准备，还没上网；teal（`bg-primary`）= 真的在传。**
 * 百分比显式写在文案里，让「这条走到 100% 只代表准备完了」不必靠颜色单独承担
 * ——一条不报数的条子只能靠颜色区分，而颜色正是被记混的那一维。
 * 三端同此：桌面 `src/routes/_app/send/-components/prepare-progress-bar.tsx`、
 * Web `docs/app/app/_components/prepare-progress.tsx`。
 *
 * 灰档用满不透明度而不是 `/60`：填充与轨道（`bg-muted`）的对比度是这条子唯一能被看出
 * 「走到哪了」的线索，`/60` 在亮色下只有 2.2:1，够不上 WCAG 2.2 SC 1.4.11 的 3:1；
 * 满不透明度是 4.3:1（暗色 5.6:1）。
 *
 * 轨道不用跟着改：这里的 `ProgressBar` 轨道本就是中性的 `bg-muted`（桌面那份是
 * `bg-primary/20`，所以它必须连轨道一起换，否则会变成「灰填充 + 品牌色底槽」）。
 *
 * 阶段以具名 `tone` 传入、颜色在 `shared.tsx` 的 `TONE_FILL` 里查表——三端同一形状
 * （桌面 `ui/progress.tsx`、Web `progress-bar.tsx`）。加第三种阶段会在查表处编译期报缺项，
 * 而不是变成某个页面忘了改的颜色。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { View } from "react-native";
import type { MobilePrepareProgress } from "react-native-swarmdrop-core";
import {
  calcPercent,
  formatBytes,
  ProgressBar,
} from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";

export function PrepareProgressBar({
  progress,
}: {
  progress: MobilePrepareProgress;
}) {
  const { t } = useLingui();
  const total = Number(progress.totalBytes);
  const hashed = Number(progress.bytesHashed);
  const percent = calcPercent(hashed, total);
  return (
    // 这里**不能**有 flex-1:两个调用点都把它放在纵向容器里,flexBasis:0% 会作用于
    // **高度**,整条塌掉、内容溢出底栏 content box(见 send/share-target 的同款注释)。
    <View className="gap-2">
      <View className="flex-row items-center justify-between gap-3">
        <Text
          className="flex-1 text-[13px] text-muted-foreground"
          numberOfLines={1}
        >
          <Trans>
            正在准备 {percent}%（{progress.completedFiles.toString()}/
            {progress.totalFiles.toString()}）
          </Trans>
        </Text>
        <Text className="text-[12px] text-muted-foreground">
          {formatBytes(hashed)} / {formatBytes(total)}
        </Text>
      </View>
      {/* 无障碍语义挂在这层包裹 View 上：`ProgressBar` 只画两个没有文本的 <View>，
          在读屏里**根本不存在**（不是「弱一点」），而灰/青绿这层区分又完全靠颜色——
          于是两个阶段对读屏用户是同一片空白。名字 + `accessibilityValue` 一起给才补得上
          「哪一条、走到几成」。三端同一句文案（Web 传 `label`、桌面传 `aria-label`）。 */}
      <View
        accessible
        accessibilityRole="progressbar"
        accessibilityLabel={t`准备文件的进度`}
        accessibilityValue={{ min: 0, max: 100, now: percent }}
      >
        <ProgressBar percent={percent} heightClass="h-1.5" tone="local" />
      </View>
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
