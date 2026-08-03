"use client";

// 就地两态确认：默认是一个普通按钮，点一下换成「警示文案 + 红色确认 + 返回」。
//
// 不用 window.confirm——它阻塞整页，而这里要拦的只是一次手滑。三处调用点（清空传输历史 /
// 取消传输 / 删除记录）此前各写一份 `useState(false)` 加同构的三段 JSX，只有图标、文案与
// 回调不同；confirming 收在这里之后，调用方连「有没有确认态」都不必知道。
//
// **应用区有第二个确认原语，那是刻意的**（#109 的结论）：`device-list.tsx` 的「取消配对」
// 失败后要留在确认态，而本组件的核心设计是「点确认的同一拍就复位、不等异步结果」——两者在
// 同一个轴上取相反的值，合并需要本组件同时管 error 的渲染位置，与现有三个调用点的布局冲突。
// 完整推导写在 `device-list.tsx` 的文件头注释里，别再纠结第二遍。

import { Trans } from "@lingui/react/macro";
import type { LucideIcon } from "lucide-react";
import { type ReactNode, useState } from "react";

export type ConfirmActionOptions = {
  /** 触发按钮与确认按钮共用同一个图标——同一个动作换了个说法，换图标反而像换了件事。 */
  icon: LucideIcon;
  /** 默认态按钮文案。 */
  label: string;
  /** 动作在途时顶掉 `label`。确认态此时已经收起，这是在途的唯一提示。 */
  pendingLabel: string;
  /** 确认态里那个红色按钮的文案。 */
  confirmLabel: string;
  /** 确认态的一句警示，说清「没掉的到底是什么」——尤其是「哪些东西不会没」。 */
  warning: string;
  /** 外部前置条件（如节点未就绪）。与 `pending` 一起禁用触发/确认按钮；「返回」永不禁用。 */
  disabled: boolean;
  pending: boolean;
  onConfirm: () => void;
  /**
   * `inline`：确认态就地顶掉触发按钮，渲染成片段跟随外层 flex 行（列表项的动作行）。
   * `banner`：确认态是独立一条灰底横幅，与触发按钮**分处两地**（面板头部按钮 + 头部下方横幅）。
   */
  layout?: "inline" | "banner";
};

/**
 * 列表项动作行里那些**不需要确认**的按钮（续传 / 归档 / 跳转链接）的皮肤。
 *
 * 与 `SKIN.inline.trigger` 是同一串，导出它是为了让同一行里「要确认的」与「不要确认的」
 * 长得一样——此前那串 class 在仓库里被手抄了 4 份，`SKIN` 改一次要靠人肉找齐。
 * `disabled:opacity-50` 留在这里，`<Link>` 这类没有 disabled 态的用它也无害。
 */
export const INLINE_ACTION_CLASS =
  "inline-flex items-center gap-1.5 rounded-lg border border-fd-border px-2.5 py-1 font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50";

// 两套皮肤，差异都是既有的、用户看得见的，不是可以顺手抹平的：
// - `banner` 用在面板层，自带 `text-xs`（外层没有），并与面板内其它按钮一样带 `cursor-pointer`；
// - `inline` 用在列表项动作行，父级已是 `text-xs`，且同行的「续传」按钮也不带 `cursor-pointer`。
const SKIN = {
  inline: {
    trigger: INLINE_ACTION_CLASS,
    confirm:
      "inline-flex items-center gap-1.5 rounded-lg border border-red-500/40 px-2.5 py-1 font-medium text-red-600 hover:bg-red-500/10 disabled:opacity-50 dark:text-red-400",
    back: "rounded-lg px-2 py-1 text-fd-muted-foreground hover:text-fd-foreground",
    warning: "text-[11px]",
  },
  banner: {
    trigger:
      "inline-flex cursor-pointer items-center gap-1.5 rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent hover:text-fd-foreground disabled:opacity-50",
    confirm:
      "inline-flex cursor-pointer items-center gap-1.5 rounded-lg border border-red-500/40 px-2.5 py-1 font-medium text-red-600 hover:bg-red-500/10 disabled:opacity-50 dark:text-red-400",
    back: "cursor-pointer rounded-lg px-2 py-1 text-fd-muted-foreground hover:text-fd-foreground",
    warning: "text-fd-muted-foreground",
  },
} as const;

/**
 * 双出口形态：触发按钮与确认区**不在同一个 DOM 位置**时用它。
 *
 * 「清空记录」就是这种：按钮在面板头部与计数并排，确认横幅在头部下方撑满整张卡。单挂载点的
 * 组件表达不了这个关系（放头部里横幅会被挤进那一行，放头部外按钮又离开了计数旁边），所以把
 * 两段节点交回调用方各自摆放——confirming 仍然自持在这里，调用方拿到的只是「现在该渲染哪一段」。
 *
 * 同一位置替换的场景直接用下面的 `ConfirmAction`。
 */
export function useConfirmAction({
  icon: Icon,
  label,
  pendingLabel,
  confirmLabel,
  warning,
  disabled,
  pending,
  onConfirm,
  layout = "inline",
}: ConfirmActionOptions): { trigger: ReactNode; panel: ReactNode } {
  const [confirming, setConfirming] = useState(false);
  const skin = SKIN[layout];

  if (!confirming) {
    return {
      trigger: (
        <button
          type="button"
          onClick={() => setConfirming(true)}
          disabled={disabled || pending}
          className={skin.trigger}
        >
          <Icon className="size-3" aria-hidden="true" />
          {pending ? pendingLabel : label}
        </button>
      ),
      panel: null,
    };
  }

  // 复位时机：点「确认」的同一拍就退出确认态，**不等异步结果**。
  //
  // 三处原本都是这么做的，收口时保持不变——在途由调用方的 `pending` 表达（触发按钮变成
  // 「清空中 / 取消中 / 删除中」），失败由调用方在它自己的错误位渲染。若改成「成功后才复位」，
  // 确认横幅会在整个请求期间赖着不走，反而多出一个此前没有的中间态。
  const confirmButton = (
    <button
      type="button"
      onClick={() => {
        setConfirming(false);
        onConfirm();
      }}
      disabled={disabled || pending}
      className={skin.confirm}
    >
      <Icon className="size-3" aria-hidden="true" />
      {confirmLabel}
    </button>
  );

  const backButton = (
    <button type="button" onClick={() => setConfirming(false)} className={skin.back}>
      <Trans>返回</Trans>
    </button>
  );

  const panel =
    layout === "banner" ? (
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-fd-border bg-fd-muted/40 px-3 py-2 text-xs">
        <span className={skin.warning}>{warning}</span>
        <div className="flex items-center gap-2">
          {confirmButton}
          {backButton}
        </div>
      </div>
    ) : (
      <>
        <span className={skin.warning}>{warning}</span>
        {confirmButton}
        {backButton}
      </>
    );

  return { trigger: null, panel };
}

/** 单挂载点形态：确认态就地顶掉触发按钮。绝大多数场景用这个。 */
export function ConfirmAction(options: ConfirmActionOptions) {
  const { trigger, panel } = useConfirmAction(options);
  return <>{panel ?? trigger}</>;
}
