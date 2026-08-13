"use client";

// 出站配对的身份确认 —— 「这串邀请解出来是谁，要不要跟它配」。
//
// **它是 `pairing-request-host.tsx` 的镜像**：那个是对方拿着我的邀请找上门（入站），这个是
// 我拿着对方的邀请找过去（出站）。两者共用 `PairingIdentityDialog` 的骨架，本文件只给
// 出站那一半的内容与按钮语义。此前出站是设备页侧栏里一张 `rounded-lg border` 小卡：
// 360px 宽、字号 xs、NodeId 只露末 8 位，而它承担的判断与入站那个对话框一模一样。
//
// ## 为什么是 Dialog，而 DESIGN.md 说配对不用 Dialog
//
// DESIGN.md「Pairing stays in the page」管的是**配对面板整体**（发码 + 粘贴 + 已发出的邀请），
// 理由是用户在发/收两面之间来回移动时，overlay 会反复盖住他正盯着看结果的已配对设备清单。
// 那条理由在**这一个瞬间**不成立：用户此刻要看的就是「对方是谁」，不是设备清单；确认完
// 对话框即关，新设备正好出现在它身后的清单里。面板本身仍然留在页内，一行没动。
//
// **已知边界**：两台设备互相交换邀请时，本对话框可能与挂在 layout 的 `PairingRequestHost`
// 同时打开，叠成两层模态（Radix 把下面那层置 inert）。不做互斥，因为两者都是需要答复的
// 决策、先后处理并无损失；而 Esc 在这一层只是收起（见 `onDismiss`），被盖住也不会丢东西。
// 真要处理，该做的是一个统一的配对决策队列，而不是在这里加 `open &&` 判断。

import { Trans, useLingui } from "@lingui/react/macro";
import { useRef } from "react";
import { Button } from "@/components/ui/button";
import { deviceIcon } from "../_lib/device-presentation";
import { remainingLabel, remainingSeconds } from "../_lib/invite";
import { PairingIdentityDialog } from "./pairing-identity-dialog";
import type { WebError } from "../_lib/view-types";
import type { PairInvitePreviewJson } from "swarmdrop-web";

export function PairingConfirmDialog({
  preview,
  now,
  pending,
  awaitingNetwork,
  error,
  onConfirm,
  onCancel,
  onDismiss,
}: {
  /** 解码验签通过、且尚未被收起的邀请预览；`null` 表示对话框关闭。 */
  preview: PairInvitePreviewJson | null;
  /** 当前 Unix 秒，与面板其余倒计时共用同一个时钟（见 `useNowSeconds`）。 */
  now: number;
  pending: boolean;
  /**
   * `pending` 的第一个子阶段：还在等本机连上中继，握手尚未发出（见 `pair-readiness.ts`）。
   *
   * **必须与 `pending` 分开显示**，不能笼统一句「配对中…」：这一段可以持续十几秒，
   * 而它与对方毫无关系 —— 说成「配对中」会让用户以为对方那边迟迟不点确认，
   * 于是去催一个根本还没收到请求的人。
   */
  awaitingNetwork: boolean;
  error: WebError | null;
  onConfirm: () => void;
  /**
   * **放弃**这条邀请：清空输入框与预览。不出网——邀请要到 `connect_invite` 走通握手才被
   * CAS 消费，所以这串仍然可以再用（前提是用户手上还有它）。
   */
  onCancel: () => void;
  /**
   * **只收起**对话框，邀请串留着。
   *
   * 与 `onCancel` 分开是必须的，不是洁癖：`/p/` 落地页那条来路已经
   * `sessionStorage.removeItem` 并 `history.replaceState` 抹掉了 fragment，React state 里那串
   * 是它在世上**唯一**的副本。把 Esc / 点遮罩接到 `onCancel` 上，等于让一次误触不可恢复地
   * 删掉它——`pairing-panel.tsx` 的折叠逻辑里记着同一个教训（那里的 `dismissed` 就是为它
   * 存在的），这里不能再犯一次。
   */
  onDismiss: () => void;
}) {
  const { t, i18n } = useLingui();
  /**
   * 关闭动画期间内容不能塌。`preview` 一变 null 对话框还要淡出约 150ms，直接读它会让整张卡
   * 在退场途中变成一具空壳（标题在、设备名和 ID 全没了——`PairingRequestHost` 的旧写法
   * 实测就是这个样子）。latch 住最后一次非空值即可：渲染期写 ref 是幂等的，StrictMode 下
   * 重跑写的是同一个值。
   */
  const lastRef = useRef<PairInvitePreviewJson | null>(null);
  if (preview !== null) lastRef.current = preview;
  const shown = preview ?? lastRef.current;

  // 已过期的不给「确认配对」：点了也只是白跑一趟发起端的 TTL 校验。
  const expired = shown !== null && remainingSeconds(shown.expiresAt, now) <= 0;

  return (
    <PairingIdentityDialog
      open={preview !== null}
      // Esc / 点遮罩 = 收起，**不是**放弃（见 `onDismiss`）。在途请求不打断：一次误触不该让
      // 「已发出的 connect_invite」和随后的放弃抢跑。
      //
      // ⚠️ 等中继那一段要**放行**，与下面「取消」按钮同一个判据。少了它，这段长达二十秒的
      // 等待里唯一可用的控件就只剩破坏性的那个（`onCancel` 会连邀请串一起清掉，而
      // `/p/` 落地页那条来路里它是世上唯一的副本）——把安全出口锁上、只留危险出口，
      // 比两个都锁上更糟。收起后等待照常跑完，那正是「收起 ≠ 放弃」的意思。
      onOpenChange={(open) => {
        if (!open && (!pending || awaitingNetwork)) onDismiss();
      }}
      icon={deviceIcon(shown?.displayPlatform ?? "")}
      title={<Trans>确认配对</Trans>}
      description={
        <>
          <span className="font-medium text-foreground">
            {shown?.displayName || t`对方设备`}
          </span>
          {shown?.displayPlatform && (
            <>
              {" · "}
              {shown.displayPlatform}
            </>
          )}
        </>
      }
      peerId={shown?.peerId}
      meta={
        <p className="text-xs text-muted-foreground">
          {shown && t(remainingLabel(shown.expiresAt, now, i18n.locale))}
          {shown?.localOnly && (
            <>
              {" · "}
              <Trans>仅同一网络内可用</Trans>
            </>
          )}
        </p>
      }
      hint={
        expired ? (
          <span className="text-warning-ink">
            <Trans>这条邀请已过期，让对方重新生成一条。</Trans>
          </span>
        ) : awaitingNetwork ? (
          // 说清楚在等谁：等的是**本机**的网络，不是对方的回应。用户此刻最想知道的
          // 就是「该催对方，还是该等我自己」——文案不回答这个，他只能瞎猜。
          <Trans>浏览器要先连上中继才拨得到对方，通常几秒。连上后会自动继续。</Trans>
        ) : (
          <Trans>配对之后双方都能互发文件。确认这是你认识的设备再继续。</Trans>
        )
      }
      // 握手失败时对话框留在原地：邀请没被消费，用户可以直接再点一次。
      error={error}
      footer={
        <>
          {/*
            等中继那一段**照样能取消**：那时一个字节都还没出网，而它可能长达二十秒。
            只有握手真的发出去之后才锁住 —— 那时对端可能已经 CAS 消费掉邀请，「取消」
            就只是本地忘掉结果，给不出用户以为的那个撤回。

            ⚠️ 「没出网」说的是**协议层**：那串邀请在对端眼里仍然没被用过。但 `onCancel`
            会连本地那份副本一起清掉（`clearInvite`），而 `/p/` 落地页那条来路里它是世上
            唯一的副本 —— 所以这颗按钮在等待期是**破坏性**的，Esc / 点遮罩那条非破坏性的
            出口必须同时可用（见上面的 `onOpenChange`）。两个出口，用户挑一个。
          */}
          <Button
            variant="outline"
            onClick={onCancel}
            disabled={pending && !awaitingNetwork}
            className="flex-1"
          >
            <Trans>取消</Trans>
          </Button>
          {!expired && (
            <Button
              onClick={onConfirm}
              disabled={pending}
              className="flex-1"
              data-testid="pairing-confirm"
            >
              {awaitingNetwork ? (
                <Trans>正在连接中继…</Trans>
              ) : pending ? (
                <Trans>配对中…</Trans>
              ) : (
                <Trans>确认配对</Trans>
              )}
            </Button>
          )}
        </>
      }
    />
  );
}
