"use client";

// #77 配对面板：消费邀请（受邀方）/ 生成邀请（发起方，需先 #76 reserve）/ 入站配对请求确认。
// 隐式优先（PRODUCT.md 原则 1）：配对是一次性动作，配完即长期信任，不做成每次传输都要选的
// 模式——本面板只负责把「配对」这一次性动作走完，配对后的设备去下方「已配对设备」清单看。

import Link from "next/link";
import { useEffect, useState } from "react";
import { InviteShare } from "./invite-share";
import { WebErrorCard } from "./web-error-view";
import {
  INVITE_TTL_HOURS,
  INVITE_URL_PREFIX,
  formatRemaining,
  remainingSeconds,
  extractInviteLink,
} from "../_lib/invite";
import { NAV } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { useWebNode, webNodeActions } from "../_lib/store";
import {
  toWebError,
  type PairInvitePreviewJson,
  type WebError,
  type WebNode,
} from "../_lib/view-types";
import { useNowSeconds } from "../_lib/use-now-seconds";
import type { InviteListItemJson } from "swarmdrop-web";

/** 配对/消费邀请成功后刷新已配对设备清单；失败不影响主流程（下一轮 state-poll 会补上）。 */
function refreshPairedDevices(node: WebNode) {
  try {
    webNodeActions.setPairedDevices(node.paired_devices());
  } catch {
    // ignore
  }
}

export function PairingPanel() {
  const nodeStatus = useWebNode((s) => s.status);
  const reservation = useWebNode((s) => s.reservation);
  const pendingPairings = useWebNode((s) => s.pendingPairings);
  const ready = nodeStatus === "running";
  /** 本面板所有剩余有效期共读这一个时钟：码上的倒计时与列表里每一行不会各走各的。 */
  const now = useNowSeconds();

  // —— 消费邀请（受邀方）——
  const [inviteInput, setInviteInput] = useState("");
  const [consumeSuccess, setConsumeSuccess] = useState<string | null>(null);
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
      setPreviewNotice("这是本机生成的邀请——把它发给对方，不是自己用。");
      return;
    }
    if (node.paired_devices().some((d) => d.peerId === decoded.peerId)) {
      setPreviewNotice(
        `已经和「${decoded.displayName || "对方设备"}」配过对了，去「发送」页直接选它就行。`,
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
      (peerId) => {
        setConsumeSuccess(peerId);
        clearInvite();
        refreshPairedDevices(node);
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
  const [revokeUnsaved, setRevokeUnsaved] = useState(false);
  const doRevokeInvite = (id: string) => {
    const node = getNode();
    if (!node) return;
    void revokeAction.run(id, async () => {
      // 返回值是「有没有写进 IndexedDB」。没写进去的话撤销只在本次会话内生效 ——
      // 刷新页面后那条邀请会复活，必须说出来。
      setRevokeUnsaved(!(await node.revoke_invite_by_id(id)));
      refreshInvites();
    });
  };

  // —— 入站配对请求确认（每条请求可独立并发处理，故按 pendingId 分键而非单一 id）——
  const respondAction = useKeyedAsyncAction();

  const respond = (pendingId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    void respondAction.run(pendingId, async () => {
      await node.respond_pairing_request(pendingId, accept);
      webNodeActions.removePendingPairing(pendingId);
      if (accept) refreshPairedDevices(node);
      // 接受即消费掉那条邀请（CAS）。不刷新的话码面不会知道自己已经用掉了，
      // 会一直亮着让人以为还能再扫一台（#101 的 consumed 态靠这次刷新拿到数据）。
      refreshInvites();
    });
  };

  const generateLabel = generateAction.pending
    ? "生成中…"
    : generatedInvite
      ? "重新生成"
      : "生成邀请";

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">配对</h2>

      <div className="mt-4">
        <p className="text-xs font-medium text-fd-muted-foreground">
          消费邀请（连接桌面 / 移动生成的邀请）
        </p>
        {/* 没有「配对」按钮：串一进框就地解码，动作长在下面那张确认卡上（#98）。 */}
        <input
          className="mt-2 w-full rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground placeholder:text-fd-muted-foreground"
          placeholder={`${INVITE_URL_PREFIX}...`}
          value={inviteInput}
          onChange={(e) => {
            setInviteAndPreview(e.target.value);
            setPastedFromClipboard(false);
          }}
          disabled={!ready}
        />
        {/* 输入框自己有了内容总得有个交代，否则像是页面在替用户做主。 */}
        {pastedFromClipboard && !consumeAction.error && (
          <p className="mt-2 text-xs text-fd-muted-foreground" aria-live="polite">
            已从剪贴板识别到一条邀请。
          </p>
        )}
        {previewNotice && (
          <p className="mt-2 text-xs text-fd-muted-foreground" aria-live="polite">
            {previewNotice}
          </p>
        )}
        {previewError && <WebErrorCard error={previewError} className="mt-2 text-xs" />}
        {/* 确认卡：出网之前先把「对方是谁、还有效多久、是不是仅局域网」摆出来。 */}
        {preview && (
          <div className="mt-2 rounded-lg border border-fd-border bg-fd-background px-3 py-2.5">
            <p className="text-xs text-fd-foreground">
              <span className="font-medium">{preview.displayName || "对方设备"}</span>
              <span className="ml-2 text-fd-muted-foreground">{preview.displayPlatform}</span>
            </p>
            {/* 只露末 8 位：够用来跟对方核一句，不必铺满一行 */}
            <p className="mt-0.5 font-mono text-xs text-fd-muted-foreground">
              {preview.peerId.slice(-8)}
            </p>
            <p className="mt-1 text-xs text-fd-muted-foreground">
              {formatRemaining(preview.expiresAt, now)}
              {preview.localOnly && " · 仅局域网可见（LocalOnly）"}
            </p>
            {previewExpired ? (
              <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
                这条邀请已过期，让对方重新生成一条。
              </p>
            ) : (
              <p className="mt-2 text-xs text-fd-muted-foreground">
                配对后双方可以互相发送文件。确认是这台设备再继续。
              </p>
            )}
            <div className="mt-2 flex gap-2">
              {!previewExpired && (
                <button
                  type="button"
                  onClick={doConsumeInvite}
                  disabled={!ready || consumeAction.pending}
                  className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  {consumeAction.pending ? "配对中…" : "确认配对"}
                </button>
              )}
              <button
                type="button"
                onClick={clearInvite}
                disabled={consumeAction.pending}
                className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
              >
                取消
              </button>
            </div>
          </div>
        )}
        {consumeAction.error && (
          <>
            <WebErrorCard error={consumeAction.error} className="mt-2 text-xs" />
            {/* 「已撤销」在受邀方本地判不出来——撤销状态只在邀请方的注册表里，那条邀请一个
                字节都没传播过来（要判就得出网，与「确认卡前零出网」冲突）。所以它只能在这一步
                现形：邀请方拒绝之后，把可能的成因说成人话，而不是让人对着一句「配对未成功」猜。 */}
            <p className="mt-1 text-xs text-fd-muted-foreground">
              邀请是一次性的：若对方已撤销它、或它已被别的设备用掉，就会走到这里——让对方重新生成一条。
            </p>
          </>
        )}
        {consumeSuccess && (
          <p className="mt-2 text-xs text-emerald-600 dark:text-emerald-400">
            已配对：<span className="font-mono">{consumeSuccess}</span>
          </p>
        )}
      </div>


      <div className="mt-5 border-t border-fd-border pt-4">
        <p className="text-xs font-medium text-fd-muted-foreground">
          生成邀请（让桌面 / 移动扫码或粘贴来配对本机）
        </p>
        {!reservation && (
          <p className="mt-1 text-xs text-fd-muted-foreground">
            需先在{" "}
            <Link href={NAV.settings.href} className="font-medium text-fd-foreground underline underline-offset-2">
              设置
            </Link>{" "}
            页的「连接」区建立可达（circuit），否则邀请里无可拨地址。
          </p>
        )}
        <label className="mt-2 flex items-center gap-1.5 text-xs text-fd-muted-foreground">
          <input
            type="checkbox"
            checked={localOnly}
            onChange={(e) => setLocalOnly(e.target.checked)}
            disabled={!ready}
          />
          仅局域网可见（LocalOnly）——若 reserve 用的是公网 helper，保持不勾选，否则邀请可能不含可用地址
        </label>
        <button
          type="button"
          onClick={doGenerateInvite}
          disabled={!ready || !reservation || generateAction.pending}
          className="mt-2 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
        >
          {generateLabel}
        </button>
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

      {invites.length > 0 && (
        <div className="mt-5 border-t border-fd-border pt-4">
          <p className="text-xs font-medium text-fd-muted-foreground">
            已发出的邀请（未过期）
          </p>
          <p className="mt-1 text-xs text-fd-muted-foreground">
            邀请有效期 {INVITE_TTL_HOURS} 小时且跨刷新保留。这里列不出原始链接（凭证明文不留存），
            不想让它继续可用就撤销，需要再分享请重新生成。
          </p>
          <ul className="mt-2 space-y-2">
            {invites.map((invite) => (
              <li
                key={invite.id}
                className="flex items-center justify-between gap-3 rounded-lg border border-fd-border bg-fd-background px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="text-xs text-fd-foreground">
                    {invite.consumed ? "已被使用" : "等待对方使用"}
                    <span className="ml-2 text-fd-muted-foreground">
                      {formatRemaining(invite.expiresAt, now)}
                    </span>
                  </p>
                  {/* 只露前 8 位：它是 capability 的哈希，够用来区分两条邀请，不必铺满整行 */}
                  <p className="mt-0.5 truncate font-mono text-xs text-fd-muted-foreground">
                    {invite.id.slice(0, 8)}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => doRevokeInvite(invite.id)}
                  disabled={revokeAction.isPending(invite.id)}
                  className="shrink-0 rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                >
                  {revokeAction.isPending(invite.id) ? "撤销中…" : "撤销"}
                </button>
              </li>
            ))}
          </ul>
          {revokeUnsaved && (
            <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
              撤销已生效，但没能写入本地存储 —— 刷新页面后它可能恢复可用，建议稍后再撤销一次。
            </p>
          )}
          {revokeAction.latestError && (
            <WebErrorCard error={revokeAction.latestError} className="mt-2 text-xs" />
          )}
        </div>
      )}

      {pendingPairings.length > 0 && (
        <div className="mt-5 border-t border-fd-border pt-4">
          <p className="text-xs font-medium text-fd-muted-foreground">入站配对请求</p>
          <ul className="mt-2 space-y-2">
            {pendingPairings.map((r) => (
              <li key={r.pendingId} className="rounded-lg border border-fd-border bg-fd-background px-3 py-2">
                <p className="text-xs text-fd-foreground">
                  <span className="font-medium">{r.deviceName}</span> 请求配对
                </p>
                <p className="mt-0.5 truncate font-mono text-xs text-fd-muted-foreground">{r.peerId}</p>
                <div className="mt-2 flex gap-2">
                  <button
                    type="button"
                    onClick={() => respond(r.pendingId, true)}
                    disabled={respondAction.isPending(r.pendingId)}
                    className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    接受
                  </button>
                  <button
                    type="button"
                    onClick={() => respond(r.pendingId, false)}
                    disabled={respondAction.isPending(r.pendingId)}
                    className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    拒绝
                  </button>
                </div>
              </li>
            ))}
          </ul>
          {respondAction.latestError && <WebErrorCard error={respondAction.latestError} className="mt-2 text-xs" />}
        </div>
      )}
    </div>
  );
}
