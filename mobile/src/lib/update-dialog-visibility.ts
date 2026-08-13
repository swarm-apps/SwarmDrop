import type { Progress, ReleaseInfo, UpdateStatus } from "@swarm-hive/sdk";
import type { ApkInstallBlockReason } from "./ports";

/**
 * 是否强制升级流。**唯一真相源是 `release.upgradeType`**,不是 `status`。
 *
 * engine 判 forced(`release.upgradeType === "force"`)、从 error 恢复态用的都是它,且它在
 * 整个生命周期稳定;`status` 则会在进入 downloading / ready 后把「从哪来」抹平——据它推导
 * 就会把普通更新的下载误判成强制流。
 *
 * ⚠️ **本文件由 `@swarmhive-rn` registry 分发,上游在 SwarmHive
 * `packages/registry-rn/registry/rn/lib/update-dialog-visibility.ts`。要改请改上游再重新拉取**
 * —— 就地改会在下次拉取时被覆盖,且改动不会回流给其它 app。互斥不变量(任何 status ×
 * 任何 upgradeType 下至多一个弹窗承载进度)由上游 test/update-dialog-visibility.test.ts 守护;
 * 本文件是纯函数正是为了让它可测——渲染层的可见性测不了(registry 无 RN render 设施)。
 */
export function isForcedFlow(release: ReleaseInfo | null | undefined): boolean {
  return release?.upgradeType === "force";
}

/** 下载中 / 待安装 —— 需要向用户呈现进度的两个态。宿主判「该不该收起进度 UI」也用它。 */
export function isBusy(status: UpdateStatus): boolean {
  return status === "downloading" || status === "ready";
}

/**
 * force-update-dialog 是否可见。仅强制流;普通更新走 prompt-update-dialog。
 *
 * 本弹窗不可关、无「稍后」,错弹会把用户锁到下载结束,故强制流判据必须严格。
 */
export function forceDialogVisible(
  status: UpdateStatus,
  release: ReleaseInfo | null | undefined,
): boolean {
  return (
    isForcedFlow(release) && (status === "force-required" || isBusy(status))
  );
}

/**
 * update-progress-dialog 的缺省可见性(`open` prop 未覆盖时)。
 *
 * 强制流下 force-update-dialog 常驻且自带内联进度,本弹窗让位。非强制流下 prompt 由宿主在
 * downloading 时收起(交接),由本弹窗接管进度。
 */
export function progressDialogVisible(
  status: UpdateStatus,
  release: ReleaseInfo | null | undefined,
): boolean {
  return !isForcedFlow(release) && isBusy(status);
}

/** 设置区主按钮此刻代表的动作。 */
export type UpdateActionKind =
  | "check"
  | "checking"
  | "download"
  | "downloading"
  | "install";

/**
 * 由 status **穷尽**推出主按钮的动作。
 *
 * 这里刻意写成穷尽 switch 而不是「特判几个 + else 兜底」:后者会让新增或改语义的状态
 * 静默落进兜底分支去说谎 —— `ready` 就曾这样掉进「已是最新」,于是设置页写着已是最新、
 * 同时前面压着一个说有新版本要装的弹窗。`never` 断言让漏掉的状态在编译期就报错。
 */
export function updateActionKind(status: UpdateStatus): UpdateActionKind {
  switch (status) {
    case "checking":
      return "checking";
    case "available":
    case "force-required":
      return "download";
    case "downloading":
      return "downloading";
    case "ready":
      return "install";
    case "idle":
    case "up-to-date":
    case "error":
      return "check";
    default: {
      const exhaustive: never = status;
      return exhaustive;
    }
  }
}

/** ready 态该说哪句话。文案本身归 update-texts,这里只定判据。 */
export type ReadyHintKind = "ready" | "background" | "canceled";

/**
 * ready 态提示的选择判据。
 *
 * `ready` 是持久静止态,停在这里的原因不止一种,而它们对用户意味着完全不同的下一步:
 * 在后台要回到应用、点了取消只需再试一次、没试过就等他点。三种一律显示同一句
 * 「等待系统确认」既没用也不诚实 —— 系统那边可能什么都没发生。
 *
 * `autoAttemptSpent` 是「本进程为当前 release 的自动安装机会已经用掉」。没被门禁拦、
 * 却仍停在 ready,说明 intent 发出去了、系统框也弹了,是用户自己取消的。
 */
export function readyHintKind(
  blockedReason: ApkInstallBlockReason | null,
  autoAttemptSpent: boolean,
): ReadyHintKind {
  if (blockedReason === "background") return "background";
  return autoAttemptSpent ? "canceled" : "ready";
}

/** 进度呈现:百分比 + 速度(MB/s,算不出来给 null)。 */
export interface ProgressView {
  percent: number;
  speedMb: string | null;
}

/**
 * 由 status + progress 推出该怎么显示进度。**四个组件共用同一条规则**,否则同一个 `ready`
 * 会在一个弹窗里显示「100%、无速度」、在另一个里显示「0%、上一帧的残留速度」。
 *
 * `ready` 恒为 100 且不报速度:产物已就绪就是「下完了」,而跨进程恢复的 ready 根本没走过
 * 下载(`reconcile` 直接进 ready,progress 为 null),照 progress 算会显示一根 0% 的进度条。
 */
export function progressView(
  status: UpdateStatus,
  progress: Progress | null | undefined,
): ProgressView {
  const isReady = status === "ready";
  return {
    percent: isReady ? 100 : progress ? Math.round(progress.percent * 100) : 0,
    speedMb:
      !isReady && progress?.speed
        ? (progress.speed / 1024 / 1024).toFixed(1)
        : null,
  };
}
