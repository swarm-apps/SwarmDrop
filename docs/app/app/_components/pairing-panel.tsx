"use client";

// #77 配对面板：消费邀请（受邀方）/ 生成邀请（发起方，需先 #76 reserve）/ 入站配对请求确认。
// 隐式优先（PRODUCT.md 原则 1）：配对是一次性动作，配完即长期信任，不做成每次传输都要选的
// 模式——本面板只负责把「配对」这一次性动作走完，配对后的设备去下方「已配对设备」清单看。

import { Trans, useLingui } from "@lingui/react/macro";
import { Link2 } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { toast } from "sonner";
import { InviteShare } from "./invite-share";
import { SectionHeader, SectionShell } from "./section";
import { WebErrorCard } from "./web-error-view";
import {
  INVITE_TTL_HOURS,
  INVITE_URL_PREFIX,
  remainingLabel,
  remainingSeconds,
  extractInviteLink,
} from "../_lib/invite";
import { NAV } from "../_lib/nav";
import { getNode, refreshPairedDevices } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { selectReservation, useWebNode } from "../_lib/store";
import {
  toWebError,
  type PairInvitePreviewJson,
  type WebError,
} from "../_lib/view-types";
import { useNowSeconds } from "../_lib/use-now-seconds";
import type { InviteListItemJson, PairingOutcomeJson } from "swarmdrop-web";

/** 配对/消费邀请成功后刷新已配对设备清单；失败不影响主流程（下一轮 state-poll 会补上）。 */
export function PairingPanel() {
  const { t, i18n } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  // 可达地址从 relay 清单派生（`selectReservation`）——它由 layout 单点订阅写入，
  // 所以不进设置页也是对的。此前它是设置页写、这里读，用户直接进设备页时恒为 null。
  const reservation = useWebNode(selectReservation);
  const pendingPairings = useWebNode((s) => s.pendingPairings);
  const ready = nodeStatus === "running";
  /** 本面板所有剩余有效期共读这一个时钟：码上的倒计时与列表里每一行不会各走各的。 */
  const now = useNowSeconds();

  // —— 消费邀请（受邀方）——
  const [inviteInput, setInviteInput] = useState("");
  /**
   * 消费邀请成功后的结果。
   *
   * `persisted === false` 是一种「一半成功」：对端已经把本机加进它的已配对列表、本页面
   * 也能立刻用，只是这条记录没写进 IndexedDB，刷新后会不见。既不能报成失败（那会和
   * 对端的认知分叉），也不能当普通成功——所以两条信息都要显示。
   */
  const [consumeSuccess, setConsumeSuccess] = useState<PairingOutcomeJson | null>(
    null,
  );
  const consumeAction = useAsyncAction();
  /** 这条是剪贴板感知填进来的（#105）——要说一句，否则输入框会莫名其妙自己有了内容。 */
  const [pastedFromClipboard, setPastedFromClipboard] = useState(false);

  /**
   * 邀请预览（#98）：确认卡数据 / 解码失败 / 本地拦下的成因，三者互斥，任一时刻至多一个非空。
   *
   * **刻意不进 store**：预览态只有本面板一个消费者，进 store 只是把一份局部状态摊到全局，
   * 换不来任何共享。
   */
  const [preview, setPreview] = useState<PairInvitePreviewJson | null>(null);
  const [previewError, setPreviewError] = useState<WebError | null>(null);
  /** 解码成功但本地就该拦下的（自己发的邀请 / 已配过的设备）：说一句话，不亮确认卡。 */
  const [previewNotice, setPreviewNotice] = useState<string | null>(null);
  /** 三格永远一起变——收口成一个函数，免得某条路径漏清其中一格留下前一次的残影。 */
  const resetPreview = () => {
    setPreview(null);
    setPreviewError(null);
    setPreviewNotice(null);
  };
  /** 已过期的邀请不给「配对」按钮：点了也只是白跑一趟发起端的 TTL 校验。 */
  const previewExpired = preview !== null && remainingSeconds(preview.expiresAt, now) <= 0;

  /**
   * 把一条邀请串放进消费框的**唯一入口**。手打 / 剪贴板感知 / `/p/` 落地页 handoff
   * 三条来路全部收口在这里，于是「解码 → 确认卡 → 用户点确认」这道闸没有旁路——
   * #98 的硬约束原文：不要因为「用户点了链接」就当作已确认。
   *
   * 解码是**纯本地同步计算**（不拨号、不查 DHT、不碰 IndexedDB），所以敢挂在每一次输入
   * 变化上；「确认卡出现之前零出网」也正是靠这一点成立。
   */
  const setInviteAndPreview = (link: string) => {
    setInviteInput(link);
    setConsumeSuccess(null);
    resetPreview();
    const node = getNode();
    const trimmed = link.trim();
    // 节点还没起来时先只把串收进框里，解码交给下面那个补偿 effect。
    if (!node || !trimmed) return;

    let decoded: PairInvitePreviewJson;
    try {
      decoded = node.decode_invite_preview(trimmed);
    } catch (e) {
      // 「前缀不认 / 编码损坏 / 验签不过」的分情况文案由 wasm 侧按 `InviteParseError` 给出，
      // 这里不再拍成一句「邀请无效」。**注意它是 invalidInput 而非 network**——
      // 确认卡这一步压根没出网。
      setPreviewError(toWebError(e));
      return;
    }

    // 两条本地过滤都发生在**解码之后、出网之前**：`node_id()` 与 `paired_devices()`
    // 都是同步的本地读，不破坏「确认卡出现前零出网」。
    if (decoded.peerId === node.node_id()) {
      // 判据是签名覆盖范围内的 `inviter_id`，伪造不了。比对「当前生成的那一串」则漏得掉
      // 历史生成的、隔壁标签页生成的、以及刷新之后的（那时本地根本没有那串可比）。
      setPreviewNotice(t`这是本机生成的邀请——把它发给对方，不是自己用。`);
      return;
    }
    if (node.paired_devices().some((d) => d.peerId === decoded.peerId)) {
      setPreviewNotice(
        t`已经和「${decoded.displayName || t`对方设备`}」配过对了，去「发送」页直接选它就行。`,
      );
      return;
    }
    setPreview(decoded);
  };

  /**
   * 清空消费框与预览态。**不调任何后端**：邀请只在 `connect_invite` 走通 capability 握手
   * 时才被邀请方 CAS 消费，用户在确认卡上取消时它一个字节都没出网，那串仍然可以再用。
   */
  const clearInvite = () => {
    setInviteInput("");
    resetPreview();
    setPastedFromClipboard(false);
  };

  // 从配对落地页（/p/）过来时把邀请接过来预填。
  //
  // 主路径是 sessionStorage（落地页存完整 canonical 链接，capability 不进本页地址栏）；
  // 隐私模式下 storage 不可用，落地页会退回把 payload 挂在 fragment 上，故两处都读。
  // **读完立刻清掉**：capability 是一次性信任凭证，不该留在 storage 或地址栏里被刷新、
  // 分享、截图二次带走。接过来的串走 `setInviteAndPreview` 这道闸，与手打、剪贴板同一条路：
  // 先解码成确认卡，由用户看清对方是谁再按「配对」——「用户点了链接」不等于已确认（#98）。
  useEffect(() => {
    const KEY = "swarmdrop:pending-invite";
    let handoff: string | null = null;
    try {
      handoff = sessionStorage.getItem(KEY);
      if (handoff !== null) sessionStorage.removeItem(KEY);
    } catch {
      // storage 被禁用，走 fragment 兜底
    }
    if (handoff === null) {
      const payload = location.hash.slice(1);
      if (!/^[A-Za-z2-7]+$/.test(payload)) return;
      // 兜底路径拿到的是裸 payload，拼回 canonical 链接再交给后端（唯一解析入口）。
      // 前缀取 `_lib/invite.ts` 的单一副本（为什么不能用 `location.origin` 见那里）。
      handoff = `${INVITE_URL_PREFIX}${payload}`;
      // **只有这条路径需要清 URL**（payload 挂在 fragment 上）。主路径走 sessionStorage，
      // 地址栏里本来什么都没有，清它零收益。
      //
      // 必须把 `history.state` 原样传回：Next 的 app-router 在 `useInsertionEffect` 里往
      // state 塞内部字段，而给 `replaceState` 打补丁是在普通 passive effect 里 —— 子先父后，
      // 所以本 effect 跑在补丁安装之前拿到的是原生实现。传 `null` 会抹掉那些字段，
      // `onPopState` 里的 `if (!event.state) return` 随即让这个 history entry 失活：
      // 表现为按浏览器后退键地址栏变了、页面不动。
      history.replaceState(history.state, "", location.pathname + location.search);
    }
    setInviteAndPreview(handoff);
  }, []);

  // 解码要 `getNode()`，而本面板挂载远早于 wasm spawn 完成——落地页 handoff 恰好落在这个
  // 窗口里。就绪后补解码一次，否则那条邀请会停在「框里有串、却没有确认卡」，而「配对」按钮
  // 只长在确认卡上：从落地页点进来的人正好走进一条死路。
  //
  // 条件里的「三格都空」是幂等闸：正常路径下 `setInviteAndPreview` 已经给出结论，这里不重跑。
  useEffect(() => {
    if (!ready || !inviteInput.trim() || preview || previewError || previewNotice) return;
    setInviteAndPreview(inviteInput);
  }, [ready, inviteInput]);

  const doConsumeInvite = () => {
    const node = getNode();
    // 只有确认卡在场（解码验签过、不是自己的、也没配过）才允许出网。
    if (!node || preview === null) return;
    setConsumeSuccess(null);
    consumeAction.run(
      () => node.connect_invite(inviteInput.trim()),
      (outcome) => {
        setConsumeSuccess(outcome);
        clearInvite();
        refreshPairedDevices(node);
        if (!outcome.persisted) {
          // **必须是 toast，不能只靠下面那行内联提示** —— 同 `doRevokeInvite` 的理由：
          // `consumeSuccess` 是本组件的 state，而本组件只挂在 `/app/devices`。用户配完对
          // 最自然的下一步就是去 `/app/send` 发文件，一切页组件卸载，这条**需要他记住并
          // 采取行动**的信息就没了，切回来也不再显示。文案与 `pairing-request-host.tsx`
          // 共用同一组 msgid。
          toast.warning(t`配对成功，但这条记录没能存进浏览器`, {
            description: t`刷新页面后需要重新配对。`,
          });
        }
      },
    );
  };

  // —— 生成邀请（发起方 / browser-as-inviter）——
  const [localOnly, setLocalOnly] = useState(false);
  const generateAction = useAsyncAction();
  const [generatedInvite, setGeneratedInvite] = useState<string | null>(null);
  /**
   * 当前展示中那个码对应的注册表条目 id（capability 哈希 hex）与失效时刻——码面覆盖层
   * 靠它们判断这条码是不是已经死了（#101）。
   *
   * 为什么不取 `invites[0]`：那依赖「最近生成的在前」这条排序约定，而 `list_active` 的
   * 排序键是**秒级** `created_at`，同一秒生成的两条谁在前是任意的。这里改用**生成前后的
   * id 差集**定位，是结构性判据。
   *
   * **`expiresAt` 必须 latch 住，不能每次从列表现查**：`list_active` 只返回「未过期且未
   * 撤销」的条目，所以一条邀请到期后会从列表里消失——现查的话 `expiresAt` 随之变 null，
   * 过期判定失效，覆盖层只剩「已撤销」可说，于是对着一条自然到期的邀请断言用户撤销过它。
   */
  const [activeInviteId, setActiveInviteId] = useState<string | null>(null);
  const [activeExpiresAt, setActiveExpiresAt] = useState<string | null>(null);

  // —— 已发出的邀请（TTL 24h + 跨刷新存活 → 必须能看见和撤销）——
  //
  // 邀请现在活 24 小时且跨刷新存活（openspec: invite-persistence），「我有几条邀请在外面
  // 飘」不再是可以忽略的问题。列表里**没有邀请串本身**：capability 明文不落盘也不出注册表，
  // 所以刷新后只能显示元数据 + 提供撤销，想再分享就生成新的。
  const [invites, setInvites] = useState<InviteListItemJson[]>([]);
  /** 纯读注册表；读不到返回 null（区别于「真的空了」——后者不该清掉已显示的列表）。 */
  const readInvites = (): InviteListItemJson[] | null => {
    try {
      return getNode()?.list_invites() ?? null;
    } catch {
      // 列表是辅助信息，读失败不打扰主流程
      return null;
    }
  };
  const refreshInvites = () => {
    const list = readInvites();
    if (list !== null) setInvites(list);
  };

  useEffect(refreshInvites, [nodeStatus]);

  // 入站配对请求被决策后，那条邀请可能已被消费（CAS）——码面要跟着更新，否则它会一直
  // 亮着让人以为还能再扫一台（#101 的 consumed 态靠这次刷新拿到数据）。
  //
  // **决策本身已经搬去全局宿主** `pairing-request-host.tsx`（挂 layout，任何路由都能弹），
  // 所以这里改成观察队列长度的变化，而不是在决策回调里顺手刷。拒绝时也会触发，代价只是
  // 多读一次 localStorage。
  // biome-ignore lint/correctness/useExhaustiveDependencies: 刷的是外部存储，依赖的是「队列变了」这个事实
  useEffect(refreshInvites, [pendingPairings.length]);

  // generate / revoke 在 invite-persistence 里变成了 async（要写穿 IndexedDB，否则刷新后
  // 本机就不认识刚发出的邀请了）。revoke 保持 fire-and-forget：后端幂等，失败也不影响
  // 调用方要的终态。
  const doGenerateInvite = () => {
    const node = getNode();
    if (!node) return;
    // 旧邀请立即作废——邀请是一次性信任凭证，界面上被顶掉了就不该还能用到 TTL 到点。
    // 同时立刻撤下旧码：它此刻已经失效，还挂在那儿只会让人扫完白跑一趟失败流程。
    //
    // 撤销落盘后**自己刷一次列表**，不搭生成那条链：撤销是无条件发出的，若只在生成成功
    // 时刷新，一旦生成失败，「已发出的邀请」里会一直挂着那条刚被撤掉的。
    //
    // 已知边界：若两轮生成重叠，`useAsyncAction` 的 seq 会丢弃先返回的那条，它没进
    // `generatedInvite` 也就逃过了这里的撤销，会活满 TTL。pending 守着按钮，人手点不出
    // 重叠；真出现了它也还在下方「已发出的邀请」里可手动撤销，是可见的而非静默泄漏。
    if (generatedInvite !== null) void node.revoke_invite(generatedInvite).then(refreshInvites);
    setGeneratedInvite(null);
    setActiveInviteId(null);
    setActiveExpiresAt(null);

    // 生成前的 id 集合，用来在生成后认出「新出现的那条就是我的」（见 activeInviteId 注释）。
    const before = new Set(readInvites()?.map((i) => i.id) ?? []);
    generateAction.run(
      () => node.generate_invite(localOnly),
      (invite) => {
        setGeneratedInvite(invite);
        const after = readInvites();
        if (after !== null) setInvites(after);
        const mine = after?.find((i) => !before.has(i.id));
        setActiveInviteId(mine?.id ?? null);
        setActiveExpiresAt(mine?.expiresAt ?? null);
      },
    );
  };

  /**
   * 当前码的注册表条目。命中说明它还在「未过期且未撤销」的集合里；未命中则两种成因都有
   * 可能，**由 `InviteShare` 拿 latch 住的 `expiresAt` 先判过期**，剩下的才是撤销。
   *
   * `activeInviteId` 为 null 表示**定位失败**，那时不谎报撤销：宁可不显示覆盖层，
   * 也不要对着一个好码说它废了。
   */
  const activeInvite =
    activeInviteId === null ? undefined : invites.find((i) => i.id === activeInviteId);

  // —— 剪贴板邀请感知（#105）——
  //
  // 它服务的是上面那个「消费邀请」输入框。
  //
  // **监听 paste 而不是主动读剪贴板**：`navigator.clipboard.readText()` 会弹权限提示，
  // 页面一加载就读等于一进来就弹一个没有上下文的权限框。paste 事件是用户手势的直接产物，
  // 零权限、零新 UI（隐式优先，PRODUCT.md 原则 1）。非安全上下文下它照样能用，
  // 这也顺带绕开了 `navigator.clipboard` 在那种环境下压根是 undefined 的坑。
  //
  // 自我过滤与已配对过滤都不在这里做——它们要先解码，统一在 `setInviteAndPreview` 里判，
  // 于是三条来路一视同仁，不会有哪条漏掉。这里仍然只**预填**、不弹横幅、不自动发起：
  // 在能看清对方是谁之前，越安静越好。
  //
  // 依赖为空即可：`setInviteAndPreview` 只用稳定的 setter 与 `getNode()` 现读，
  // 捕获到哪一次渲染的实例都等价。
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      // 从整段文本里**提取**链接，而不是要求整段就是链接：IM 里复制常常连着说明文字
      // （「配对链接：https://…」），后端解码本来也是在任意文本里定位前缀的。
      const link = extractInviteLink(e.clipboardData?.getData("text") ?? "");
      if (link === null) return;
      // 用户正往一个能接收输入的控件里粘贴时不插手，原生粘贴会做同样的事。
      // **`readonly` 不算**——码面右侧那个只读的「邀请链接」框接不住原生粘贴，
      // 在它上面早退等于让粘贴键看起来是坏的。
      if (
        e.target instanceof Element &&
        e.target.matches("input:not([readonly]), textarea, [contenteditable]")
      ) {
        return;
      }
      setInviteAndPreview(link);
      setPastedFromClipboard(true);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, []);

  const revokeAction = useKeyedAsyncAction();
  const doRevokeInvite = (id: string) => {
    const node = getNode();
    if (!node) return;
    void revokeAction.run(id, async () => {
      // 返回值是「有没有写进 IndexedDB」。没写进去的话撤销只在本次会话内生效 ——
      // 刷新页面后那条邀请会复活，必须说出来。
      const persisted = await node.revoke_invite_by_id(id);
      refreshInvites();
      if (persisted) {
        toast.success(t`邀请已撤销`);
      } else {
        // **必须是 toast，不能是列表里的一行内联提示。** 这条告警此前渲染在
        // `invites.length > 0` 的块内，于是撤销掉**最后一条**（最常见的场景）时列表清空、
        // 整块卸载，这句话永远看不到——用户以为撤干净了，而外面还飘着一条能用满 24 小时的
        // 一次性凭证。toast 的生命周期与列表无关，从结构上免疫这个问题。
        toast.warning(t`邀请已撤销，但没能保存`, {
          description: t`刷新页面后它可能恢复可用，建议稍后再撤销一次。`,
        });
      }
    });
  };

  const generateLabel = generateAction.pending
    ? t`生成中…`
    : generatedInvite
      ? t`重新生成`
      : t`生成邀请`;

  return (
    <SectionShell>
      <SectionHeader
        icon={Link2}
        title={<Trans>配对</Trans>}
        description={<Trans>与另一台设备互换一次邀请即可，之后长期有效。</Trans>}
      />

      <div>
        <p className="text-xs font-medium text-muted-foreground">
          <Trans>粘贴对方给你的邀请</Trans>
        </p>
        {/* 没有「配对」按钮：串一进框就地解码，动作长在下面那张确认卡上（#98）。 */}
        <Input
          className="mt-2 h-11 font-mono text-xs sm:h-9"
          placeholder={`${INVITE_URL_PREFIX}...`}
          aria-label={t`对方的邀请链接`}
          value={inviteInput}
          onChange={(e) => {
            setInviteAndPreview(e.target.value);
            setPastedFromClipboard(false);
          }}
          disabled={!ready}
        />
        {/* 输入框自己有了内容总得有个交代，否则像是页面在替用户做主。 */}
        {pastedFromClipboard && !consumeAction.error && (
          <p className="mt-2 text-xs text-muted-foreground" aria-live="polite">
            <Trans>已从剪贴板识别到一条邀请。</Trans>
          </p>
        )}
        {previewNotice && (
          <p className="mt-2 text-xs text-muted-foreground" aria-live="polite">
            {previewNotice}
          </p>
        )}
        {previewError && <WebErrorCard error={previewError} className="mt-2 text-xs" />}
        {/* 确认卡：出网之前先把「对方是谁、还有效多久、是不是仅局域网」摆出来。 */}
        {preview && (
          <div className="mt-2 rounded-lg border bg-background px-3 py-2.5">
            <p className="text-xs text-foreground">
              <span className="font-medium">{preview.displayName || t`对方设备`}</span>
              <span className="ml-2 text-muted-foreground">{preview.displayPlatform}</span>
            </p>
            {/* 只露末 8 位：够用来跟对方核一句，不必铺满一行 */}
            <p className="mt-0.5 font-mono text-xs text-muted-foreground">
              {preview.peerId.slice(-8)}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t(remainingLabel(preview.expiresAt, now, i18n.locale))}
              {preview.localOnly && <> · <Trans>仅同一网络内可用</Trans></>}
            </p>
            {previewExpired ? (
              <p className="mt-2 text-xs text-warning-ink">
                <Trans>这条邀请已过期，让对方重新生成一条。</Trans>
              </p>
            ) : (
              <p className="mt-2 text-xs text-muted-foreground">
                <Trans>配对后双方可以互相发送文件。确认是这台设备再继续。</Trans>
              </p>
            )}
            <div className="mt-2 flex gap-2">
              {!previewExpired && (
                <Button
                  size="sm"
                  onClick={doConsumeInvite}
                  disabled={!ready || consumeAction.pending}
                                  >
                  {consumeAction.pending ? <Trans>配对中…</Trans> : <Trans>确认配对</Trans>}
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={clearInvite}
                disabled={consumeAction.pending}
                              >
                <Trans>取消</Trans>
              </Button>
            </div>
          </div>
        )}
        {consumeAction.error && (
          <>
            <WebErrorCard error={consumeAction.error} className="mt-2 text-xs" />
            {/* 「已撤销」在受邀方本地判不出来——撤销状态只在邀请方的注册表里，那条邀请一个
                字节都没传播过来（要判就得出网，与「确认卡前零出网」冲突）。所以它只能在这一步
                现形：邀请方拒绝之后，把可能的成因说成人话，而不是让人对着一句「配对未成功」猜。 */}
            <p className="mt-1 text-xs text-muted-foreground">
              <Trans>邀请是一次性的：若对方已撤销它、或它已被别的设备用掉，就会走到这里——让对方重新生成一条。</Trans>
            </p>
          </>
        )}
        {consumeSuccess && (
          <>
            <p className="mt-2 text-xs text-success-ink">
              <Trans>
                已配对：<span className="font-mono">{consumeSuccess.peerId}</span>
              </Trans>
            </p>
            {!consumeSuccess.persisted && (
              <p className="mt-1 text-xs text-warning-ink">
                <Trans>但这条记录没能存进浏览器，刷新页面后需要重新配对。</Trans>
              </p>
            )}
          </>
        )}
      </div>


      <div className="border-t pt-4">
        <p className="text-xs font-medium text-muted-foreground">
          <Trans>生成一条邀请发给对方</Trans>
        </p>
        {!reservation && (
          <p className="mt-1 text-xs text-warning-ink">
            {/* 原文是「需先在设置页的「连接」区建立可达（circuit），否则邀请里无可拨地址」——
                circuit / reserve / helper 都是内核词汇，对着它用户无从判断自己该做什么。
                这里说清楚**为什么**（浏览器不监听端口）与**该做什么**（去连一个中继）。 */}
            <Trans>
              浏览器不能被直接拨号，需要先连上一个中继，对方才找得到你。到{" "}
              <Link href={NAV.settings.href} className="font-medium underline underline-offset-2">
                设置
              </Link>{" "}
              页连接后再生成邀请。
            </Trans>
          </p>
        )}

        <div className="mt-3 flex items-start justify-between gap-3">
          <div className="min-w-0">
            <Label htmlFor="pairing-local-only" className="text-xs font-medium text-foreground">
              <Trans>仅同一网络内可用</Trans>
            </Label>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              <Trans>限制这条邀请只能被同一网络里的设备使用。通过公网中继连接时请保持关闭，否则邀请里可能没有对方拨得通的地址。</Trans>
            </p>
          </div>
          {/* 它改变的是邀请的可达范围，属于「安全边界」级的开关——此前是个 13px 的裸
              checkbox，全页最小的可点目标却承担着最重的语义。 */}
          <Switch
            id="pairing-local-only"
            checked={localOnly}
            onCheckedChange={setLocalOnly}
            disabled={!ready}
            className="mt-0.5 shrink-0"
          />
        </div>

        <Button
          size="sm"
          onClick={doGenerateInvite}
          disabled={!ready || !reservation || generateAction.pending}
          className="mt-3"
        >
          {generateLabel}
        </Button>
        {generateAction.error && (
          <WebErrorCard error={generateAction.error} className="mt-2 text-xs" />
        )}
        {/* 生成中也渲染（invite=null）：白卡留在原位显示占位，重新生成时不塌一下再长回来。 */}
        {(generatedInvite || generateAction.pending) && (
          <InviteShare
            invite={generatedInvite}
            expiresAt={activeExpiresAt}
            revoked={activeInviteId !== null && activeInvite === undefined}
            // 一次性凭证，被对方用掉之后这个码就不再有第二次机会——而它恰恰是最常见的死法。
            consumed={activeInvite?.consumed === true}
            // 用户在设置页撤销了可达之后，邀请里那条 circuit 地址就拨不回来了。
            reachable={reservation !== null}
            now={now}
          />
        )}
      </div>

      {/*
        「已发出的邀请」贴着生成入口，不进设置页——邀请是一次性信任凭证，「刚发错人了」
        是它最需要被撤回的时刻，而那一刻用户就站在这一屏上。三端同此位置。
      */}
      <div className="border-t pt-4">
        <p className="text-xs font-medium text-muted-foreground">
          <Trans>已发出的邀请（未过期）</Trans>
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          <Trans>
            邀请有效期 {INVITE_TTL_HOURS} 小时且跨刷新保留。这里列不出原始链接（凭证明文不留存），
            不想让它继续可用就撤销，需要再分享请重新生成。
          </Trans>
        </p>

        {/* 「撤销了但没保存」走 toast（见 `doRevokeInvite`）——它必须活过列表的卸载。
            这里只留出网失败的错误卡，那个错误发生时列表还在。 */}
        {revokeAction.latestError && (
          <WebErrorCard error={revokeAction.latestError} className="mt-2 text-xs" />
        )}

        {invites.length === 0 ? (
          <p className="mt-2 text-xs text-muted-foreground">
            <Trans>当前没有在外面等待使用的邀请。</Trans>
          </p>
        ) : (
          <ul className="mt-2 space-y-2">
            {invites.map((invite) => (
              <li
                key={invite.id}
                className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="text-xs text-foreground">
                    {invite.consumed ? <Trans>已被使用</Trans> : <Trans>等待对方使用</Trans>}
                    <span className="ml-2 text-muted-foreground">
                      {t(remainingLabel(invite.expiresAt, now, i18n.locale))}
                    </span>
                  </p>
                  {/* 只露前 8 位：它是 capability 的哈希，够用来区分两条邀请，不必铺满整行 */}
                  <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
                    {invite.id.slice(0, 8)}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => doRevokeInvite(invite.id)}
                  disabled={revokeAction.isPending(invite.id)}
                  className="shrink-0 text-xs"
                >
                  {revokeAction.isPending(invite.id) ? <Trans>撤销中…</Trans> : <Trans>撤销</Trans>}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </SectionShell>
  );
}
