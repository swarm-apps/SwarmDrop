import { useContext } from "react";
import { AutoInstallContext, useUpdate } from "@/hooks/use-update";
import type { ApkInstallBlockReason } from "@/lib/ports";

/**
 * ready 态的**只读**提示信息 + 手动安装入口。
 *
 * **编排不在这里** —— 自动安装由 `<UpdateProvider>` 单点驱动(它天然是单例)。本 hook 曾经
 * 既驱动又读状态,于是四个消费者各挂一份:各注册一个 AppState 监听、各派发一次安装、
 * 各写一次状态,逼出一个模块级的手写 store 来给它们去重。把驱动挪走之后,这里剩下的就是
 * 从 context 与 engine state 读两个值。
 */
export interface AutoInstallState {
  /** 上次安装被平台门禁挡下的原因;null = 没被挡。 */
  blockedReason: ApkInstallBlockReason | null;
  /** 当前 release 的自动安装机会是否已用掉(用掉后主动权交回用户)。 */
  autoAttemptSpent: boolean;
  /** 手动触发安装,供 ready 态主按钮调用。 */
  install: () => Promise<void>;
}

export function useAutoInstall(): AutoInstallState {
  const { release, installBlocked, install } = useUpdate();
  const attemptedVersion = useContext(AutoInstallContext);

  return {
    // engine 只透传 adapter 给的字符串(它平台无关、不解释取值);这里收窄回本平台的联合类型。
    blockedReason: (installBlocked as ApkInstallBlockReason | null) ?? null,
    // 派生而非另存一份:同一个事实存两处,每条写路径都得记得同步。
    autoAttemptSpent:
      attemptedVersion !== null &&
      attemptedVersion === (release?.version ?? null),
    install,
  };
}
