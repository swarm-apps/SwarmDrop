import { toast as burntToast } from "burnt";
import * as Haptics from "expo-haptics";
import { Platform } from "react-native";
import { errorMessage } from "./utils";

const DEFAULT_DURATION_S = 4;
const isIOS = Platform.OS === "ios";

interface BaseOptions {
  description?: string;
  duration?: number;
}

interface PromiseOptions<T> {
  loading: string;
  success: (result: T) => string;
  error: ((err: unknown) => string) | string;
  duration?: number;
}

function errorHaptic() {
  Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error).catch(
    () => {},
  );
}

function durationS(ms?: number): number {
  return ms && ms > 0 ? ms / 1000 : DEFAULT_DURATION_S;
}

// iOS SPIndicator 惯例在顶部;Android 系统 toast 惯例在底部。
const FROM = isIOS ? "top" : "bottom";

/**
 * 统一弹出 —— 收敛 title/message 折叠、时长换算与平台位置。
 *
 * Android 的系统 toast(ToastAndroid,burnt 底层)只显示一行 `title`、丢弃 `message`,
 * 所以把描述折进 title;iOS 的 SPIndicator 支持 title + message 两行,分开传更好看。
 */
function show(
  preset: "done" | "error" | "none",
  title: string,
  description?: string,
  durationMs?: number,
) {
  const text =
    description && !isIOS
      ? { title: `${title}\n${description}` }
      : { title, message: description };
  burntToast({ ...text, preset, duration: durationS(durationMs), from: FROM });
}

/**
 * 全 App toast 门面 —— 底层 `burnt`,走各平台**原生**机制:iOS = SPIndicator 顶部胶囊,
 * Android = 系统 `ToastAndroid`(底部小条)。API 稳定,调用点无需感知底层库。
 *
 * 注意:burnt 的 iOS 侧是原生模块(SPIndicator),新增/改动后需**重编原生**才生效;
 * Android 侧是纯 JS(ToastAndroid),无需重编。
 */
export const toast = {
  success(message: string, opts?: BaseOptions) {
    show("done", message, opts?.description, opts?.duration);
  },

  info(message: string, opts?: BaseOptions) {
    show("none", message, opts?.description, opts?.duration);
  },

  /** Pass `err` to expand `errorMessage(err)` into the description. */
  error(message: string, err?: unknown) {
    errorHaptic();
    show("error", message, err === undefined ? undefined : errorMessage(err));
  },

  /**
   * burnt 无「持久 + 原地更新」的 loading toast(iOS SPIndicator / Android ToastAndroid
   * 都是自动消失的短提示),这里退化为一条较长的普通提示。当前无调用点。
   */
  loading(message: string, opts?: BaseOptions) {
    show("none", message, opts?.description, opts?.duration ?? 10_000);
  },

  /** 系统 toast 自动消失,无编程式关闭(仅 burnt.alert 有 dismiss)。保留以兼容 API。 */
  dismiss() {},

  promise<T>(promise: Promise<T>, opts: PromiseOptions<T>) {
    this.info(opts.loading);
    promise.then(
      (result) =>
        this.success(
          opts.success(result),
          opts.duration ? { duration: opts.duration } : undefined,
        ),
      (err: unknown) =>
        this.error(
          typeof opts.error === "function" ? opts.error(err) : opts.error,
          err,
        ),
    );
    return promise;
  },
};
