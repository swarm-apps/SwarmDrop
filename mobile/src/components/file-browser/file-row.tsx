import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import {
  Check,
  CirclePause,
  RotateCcw,
  Send,
  Share2,
  Timer,
  X,
} from "lucide-react-native";
import { memo, useCallback } from "react";
import { type GestureResponderEvent, Pressable, View } from "react-native";
import { formatBytes, ProgressBar } from "@/components/transfer/shared";
import { Text } from "@/components/ui/text";
import { useThemeColors } from "@/hooks/useThemeColors";
import { cn } from "@/lib/utils";
import { fileBrowserIcon } from "./file-icon";
import type {
  FileBrowserActions,
  FileBrowserItem,
  FileBrowserStatus,
} from "./types";

interface FileRowProps {
  item: FileBrowserItem;
  depth?: number;
  actions?: FileBrowserActions;
  testID?: string;
}

const STATUS_CLASS: Record<FileBrowserStatus, string> = {
  idle: "",
  waiting: "opacity-60",
  transferring: "bg-primary/10",
  paused: "bg-warning/10",
  completed: "bg-success/5",
  cancelled: "opacity-60",
  error: "bg-destructive/5",
  missing: "bg-destructive/5 opacity-75",
};

function FileRowComponent({ item, depth = 0, actions, testID }: FileRowProps) {
  const { t } = useLingui();
  const colors = useThemeColors();
  const Icon = fileBrowserIcon(item.name);
  const openable = Boolean(actions?.openItem) && item.status !== "missing";
  const statusText = t(STATUS_TEXT[item.status]);
  const progress = Math.round(item.progress ?? 0);
  const accessibilityLabel = t`${item.name}，${formatBytes(item.size)}，${statusText}${
    item.status === "transferring" || item.status === "paused"
      ? `，${progress}%`
      : ""
  }`;

  const open = useCallback(() => {
    if (openable) actions?.openItem?.(item);
  }, [actions, item, openable]);

  const remove = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.removeItem?.(item);
    },
    [actions, item],
  );

  const retry = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.retryItem?.(item);
    },
    [actions, item],
  );

  const share = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.shareItem?.(item);
    },
    [actions, item],
  );

  const send = useCallback(
    (event: GestureResponderEvent) => {
      event.stopPropagation();
      actions?.sendItem?.(item);
    },
    [actions, item],
  );

  return (
    <View
      className={cn(
        "min-h-14 justify-center rounded-lg py-2 pr-2",
        STATUS_CLASS[item.status],
      )}
      style={{ paddingLeft: depth * 20 + 8 }}
    >
      <View className="flex-row items-center">
        <Pressable
          onPress={open}
          disabled={!openable}
          accessibilityRole={openable ? "button" : undefined}
          accessibilityLabel={accessibilityLabel}
          testID={testID}
          className="min-h-11 min-w-0 flex-1 flex-row items-center gap-2.5 active:opacity-70"
        >
          <Icon size={18} color={statusIconColor(item.status, colors)} />
          <View className="min-w-0 flex-1 gap-0.5">
            <Text
              className="text-sm font-medium text-foreground"
              numberOfLines={1}
            >
              {item.name}
            </Text>
            <Text
              className="text-[12px] text-muted-foreground"
              numberOfLines={1}
            >
              {formatBytes(item.size)} · {statusText}
            </Text>
          </View>

          {item.status === "completed" ? (
            <Check size={15} color={colors.success} />
          ) : item.status === "waiting" ? (
            <Timer size={15} color={colors.mutedForeground} />
          ) : item.status === "paused" ? (
            <CirclePause size={15} color={colors.warning} />
          ) : null}
        </Pressable>

        {item.status === "error" && actions?.retryItem ? (
          <Pressable
            onPress={retry}
            accessibilityRole="button"
            accessibilityLabel={t`重试 ${item.name}`}
            className="size-11 items-center justify-center rounded-xl active:bg-destructive/15"
          >
            <RotateCcw size={16} color={colors.destructive} />
          </Pressable>
        ) : null}

        {actions?.sendItem && item.status !== "missing" ? (
          <Pressable
            onPress={send}
            accessibilityRole="button"
            accessibilityLabel={t`发送 ${item.name} 到设备`}
            className="size-11 items-center justify-center rounded-xl active:bg-muted"
          >
            <Send size={16} color={colors.mutedForeground} />
          </Pressable>
        ) : null}

        {actions?.shareItem && item.status !== "missing" ? (
          <Pressable
            onPress={share}
            accessibilityRole="button"
            accessibilityLabel={t`分享 ${item.name}`}
            className="size-11 items-center justify-center rounded-xl active:bg-muted"
          >
            <Share2 size={16} color={colors.mutedForeground} />
          </Pressable>
        ) : null}

        {actions?.removeItem ? (
          <Pressable
            onPress={remove}
            accessibilityRole="button"
            accessibilityLabel={t`移除 ${item.name}`}
            className="size-11 items-center justify-center rounded-xl active:bg-destructive/15"
          >
            <X size={16} color={colors.mutedForeground} />
          </Pressable>
        ) : null}
      </View>

      {item.status === "transferring" || item.status === "paused" ? (
        <View className="mt-2 pl-7">
          <ProgressBar
            percent={progress}
            heightClass="h-1"
            fillClass={item.status === "paused" ? "bg-warning" : "bg-primary"}
          />
        </View>
      ) : null}
    </View>
  );
}

type ThemeColors = ReturnType<typeof useThemeColors>;

function statusIconColor(status: FileBrowserStatus, colors: ThemeColors) {
  switch (status) {
    case "completed":
      return colors.success;
    case "error":
    case "missing":
      return colors.destructive;
    case "paused":
      return colors.warning;
    case "transferring":
    case "idle":
      return colors.primary;
    default:
      return colors.mutedForeground;
  }
}

/**
 * 状态文案。
 *
 * **存 `msg` 描述符，不存字符串。** 这里此前是个把 `t` 当参数收的函数——babel 宏只认词法
 * 作用域里的 `useLingui()` 解构，`t` 一旦变成形参就展不开，于是这八条从来没被 extract 到
 * catalog 里，英文界面上一直原样显示中文。模块级 Record + 调用点 `t(...)` 是三端一致的
 * 写法（Web 的 `_lib/view-types.ts`、桌面的 `transfer-labels.ts` 都是这个形状）。
 *
 * **`idle` 不能写「已选择」**：它现在有两个来源——发送侧的已选文件，以及**入站 offer**
 * （统一模型把 offer 从 `waiting` 修成了 `idle`，理由是它还没进传输队列，挂等待图标会暗示
 * 传输已经开始）。offer 那一堆文件是对方选的，说「已选择」是替用户记了一笔他没做过的动作。
 */
const STATUS_TEXT: Record<FileBrowserStatus, MessageDescriptor> = {
  idle: msg`未开始`,
  waiting: msg`等待中`,
  transferring: msg`传输中`,
  paused: msg`已暂停`,
  completed: msg`已完成`,
  cancelled: msg`已取消`,
  error: msg`失败`,
  missing: msg`文件缺失`,
};

export const FileRow = memo(FileRowComponent);
