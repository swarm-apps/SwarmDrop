import { useMemo } from "react";
import {
  getReceiveLocation,
  requiresUserChoice,
  useReceiveLocation,
} from "@/core/receive-location";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 引导流程 —— 一组有序步骤，每步带一个可判定的满足条件。
 *
 * **完成状态完全由真实状态派生，一个持久位都不存。** 此前它是 `onboarding-store` 里一个
 * 持久化的 `hasOnboarded` 布尔，代价在新增步骤时才显形：存量用户那一位已经是 `true`，于是
 * 新步骤对他们永远不会出现——接收目录这一步要是这么加，他们会卡在「没有落点、又不许回退
 * 私有目录」的死角里，且没有任何界面能把他们领出来。
 *
 * 中间还有过一版「介绍性步骤共用一个持久位」，那仍然留着一份会与真实状态漂移的副本，
 * 而且换个字段名就等于把所有存量用户扔回欢迎页。现在连它也没有了：**欢迎页的判据就是
 * 设备名有没有填过**——名字填过就说明这个人走过引导，没有必要再给他看一遍卖点。
 *
 * 于是：新增一个步骤 = 新增一个判据，存量用户自动补跑且只补新的那一步，零迁移代码；
 * 「标记说完成了但配置其实没做」这种漂移在结构上不可能发生。
 */
export type OnboardingStepId = "welcome" | "device-name" | "receive-folder";

/** 判据的输入。抽成一个值，是为了让快照版与订阅版共用**同一份**判据实现。 */
interface FlowState {
  deviceName: string;
  receiveReady: boolean;
}

interface OnboardingStep {
  id: OnboardingStepId;
  route: string;
  /** 本平台是否需要这一步。只看平台，故在模块加载时就求值（见 `ACTIVE_STEPS`）。 */
  applies: boolean;
  isSatisfied: (state: FlowState) => boolean;
}

const hasDeviceName = (state: FlowState) => state.deviceName.trim().length > 0;

const STEPS: readonly OnboardingStep[] = [
  {
    // 纯介绍屏，没有属于自己的状态——借设备名作判据：填过名字的人已经走过这里。
    id: "welcome",
    route: "/onboarding/welcome",
    applies: true,
    isSatisfied: hasDeviceName,
  },
  {
    id: "device-name",
    route: "/onboarding/device-name",
    applies: true,
    isSatisfied: hasDeviceName,
  },
  {
    // iOS 恒有落点（`Documents`），这一步整个不适用——步骤指示器的总数也随之少一格。
    id: "receive-folder",
    route: "/onboarding/receive-folder",
    applies: requiresUserChoice(),
    isSatisfied: (s) => s.receiveReady,
  },
];

/** 本平台实际要走的步骤。`Platform.OS` 不会变，所以算一次就够。 */
const ACTIVE_STEPS: readonly OnboardingStep[] = STEPS.filter(
  (step) => step.applies,
);

/**
 * 引导的终点屏（身份就绪 + 通知授权）。
 *
 * **它不是一个步骤**：没有可判定的完成条件，而借用别的步骤的判据会让存量用户也被拉来看
 * 一遍。所以它由最后一个配置步骤显式导航过去，而守卫（`useOnboardingRoute`）看不见它
 * ——配置齐全的人直接进主界面。
 */
export const ONBOARDING_DONE_ROUTE = "/onboarding/setup";

function firstUnsatisfied(
  state: FlowState,
  from = 0,
): OnboardingStep | undefined {
  return ACTIVE_STEPS.slice(from).find((step) => !step.isSatisfied(state));
}

function snapshot(): FlowState {
  return {
    deviceName: usePreferencesStore.getState().deviceName,
    receiveReady: getReceiveLocation().status === "ready",
  };
}

function useFlowState(): FlowState {
  const deviceName = usePreferencesStore((s) => s.deviceName);
  const location = useReceiveLocation();
  const receiveReady = location.status === "ready";
  return useMemo(
    () => ({ deviceName, receiveReady }),
    [deviceName, receiveReady],
  );
}

/** 第一个未满足步骤的路由；全满足时为 `null`（可以进主界面了）。 */
export function useOnboardingRoute(): string | null {
  const state = useFlowState();
  return useMemo(() => firstUnsatisfied(state)?.route ?? null, [state]);
}

export function useIsOnboardingComplete(): boolean {
  return useOnboardingRoute() === null;
}

/**
 * 某一步点「继续」之后该去哪：它后面第一个未满足的步骤，全都满足则去终点屏。
 *
 * 跳过已满足的步骤是这里的重点——存量用户在欢迎页点继续后，设备名那步会被直接跨过去，
 * 不会再问一遍他早就填过的东西。
 */
export function nextRouteAfter(id: OnboardingStepId): string {
  const index = ACTIVE_STEPS.findIndex((step) => step.id === id);
  const from = index < 0 ? 0 : index + 1;
  return firstUnsatisfied(snapshot(), from)?.route ?? ONBOARDING_DONE_ROUTE;
}

/** 步骤指示器用：本步在**当前平台实际步骤**中的位置。 */
export function useStepPosition(id: OnboardingStepId): {
  index: number;
  total: number;
} {
  return useMemo(
    () => ({
      index: Math.max(
        0,
        ACTIVE_STEPS.findIndex((step) => step.id === id),
      ),
      total: ACTIVE_STEPS.length,
    }),
    [id],
  );
}
