import * as Haptics from "expo-haptics";
import { Notifier } from "react-native-notifier";
import { IosToast } from "@/components/ios-toast";
import { errorMessage } from "./utils";

const DEFAULT_DURATION_MS = 4000;
const LONG_DURATION_MS = 60 * 60 * 1000;

type AlertKind = "success" | "info" | "error" | "warn";

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

function show(alertType: AlertKind, title: string, opts: BaseOptions = {}) {
  Notifier.showNotification({
    title,
    description: opts.description,
    duration: opts.duration ?? DEFAULT_DURATION_MS,
    Component: IosToast,
    componentProps: { alertType },
  });
}

function errorHaptic() {
  Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error).catch(
    () => {},
  );
}

export const toast = {
  success(message: string, opts?: BaseOptions) {
    show("success", message, opts);
  },

  info(message: string, opts?: BaseOptions) {
    show("info", message, opts);
  },

  /** Pass `err` to expand `errorMessage(err)` into the description. */
  error(message: string, err?: unknown) {
    errorHaptic();
    show("error", message, {
      description: err === undefined ? undefined : errorMessage(err),
    });
  },

  /** Persistent notification. Re-call to update text in place — Notifier's
   *  default `queueMode: 'reset'` replaces the visible toast. */
  loading(message: string, opts?: BaseOptions) {
    show("info", message, {
      ...opts,
      duration: opts?.duration ?? LONG_DURATION_MS,
    });
  },

  dismiss() {
    Notifier.hideNotification();
  },

  promise<T>(promise: Promise<T>, opts: PromiseOptions<T>) {
    show("info", opts.loading, { duration: LONG_DURATION_MS });
    promise.then(
      (result) =>
        show("success", opts.success(result), { duration: opts.duration }),
      (err: unknown) => {
        errorHaptic();
        show(
          "error",
          typeof opts.error === "function" ? opts.error(err) : opts.error,
          {
            description: errorMessage(err),
          },
        );
      },
    );
    return promise;
  },
};
