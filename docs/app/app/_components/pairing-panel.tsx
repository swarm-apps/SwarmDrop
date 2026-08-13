"use client";

// #77 配对面板：消费邀请（受邀方）/ 生成邀请（发起方，需先 #76 reserve）/ 入站配对请求确认。
// 隐式优先（PRODUCT.md 原则 1）：配对是一次性动作，配完即长期信任，不做成每次传输都要选的
// 模式——本面板只负责把「配对」这一次性动作走完，配对后的设备去下方「已配对设备」清单看。

import { Trans, useLingui } from "@lingui/react/macro";
import { Link2 } from "lucide-react";
import Link from "next/link";
import { useEffect, useId, useImperativeHandle, useRef, useState, type Ref } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { toast } from "sonner";
import { InviteShare } from "./invite-share";
import { PairingConfirmDialog } from "./pairing-confirm-dialog";
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
import {
  PAIRING_READINESS_TIMEOUT_MS,
  waitForPairingReadiness,
} from "../_lib/pair-readiness";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { selectReservation, useWebNode } from "../_lib/store";
import {
  PAIRING_REFUSED_LABEL,
  toWebError,
  type PairInvitePreviewJson,
  type WebError,
} from "../_lib/view-types";
import { useNowSeconds } from "../_lib/use-now-seconds";
import type { InviteListItemJson, PairingOutcomeJson } from "swarmdrop-web";

/**
 * 外部唤起配对区的句柄——设备网格里那张「添加设备」卡片按下时调 `open()`。
 *
 * **是命令式句柄而不是受控 prop**：要表达的是一次**点击事件**，不是一个状态。
 * 做成 `open: boolean` 会撞上下面那套三层开合规则（`needsAttention` 强开、`dismissed`
 * 压过它、`override` 是用户意志），受控化等于让每个调用方都复刻这三层；做成「请求计数 +
 * useEffect」则是拿 effect 当事件处理器——为了绕开「effect 只在依赖变化时重跑」，得额外
 * 编一个只增不减的数字和一个首帧守卫，再用两处注释解释那个数字为什么存在。
 */
export interface PairingPanelHandle {
  open: () => void;
}

/** 配对/消费邀请成功后刷新已配对设备清单；失败不影响主流程（下一轮 state-poll 会补上）。 */
export function PairingPanel({
  ref,
  defaultOpen = false,
}: {
  ref?: Ref<PairingPanelHandle>;
  /**
   * 用户还没手动开合过时，这块是不是展开的。**由版式决定，所以由调用方给**——
   * 宽屏下它独占一条侧栏，收起只会留下一栏空玻璃；窄屏下它排在设备网格底下，
   * 展开就会把页面拉长半屏（那正是下面那段折叠注释里的原始理由）。
   *
   * 它只参与**默认态**：`override`（用户手动开合）与 `needsAttention`（有待决邀请）
   * 都排在它前面，见下方 `open` 的推导。
   */
  defaultOpen?: boolean;
}) {
  const { t, i18n } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  // 可达地址从 relay 清单派生（`selectReservation`）——它由 layout 单点订阅写入，
  // 所以不进设置页也是对的。此前它是设置页写、这里读，用户直接进设备页时恒为 null。
  const reservation = useWebNode(selectReservation);
  const pendingPairings = useWebNode((s) => s.pendingPairings);
  // 只要数量，用来推导折叠默认态（见下方 `open`）。selector 返回原始值，符合规则 B。
  const pairedCount = useWebNode((s) => s.pairedDevices.length);
  const ready = nodeStatus === "running";
  /** 本面板所有剩余有效期共读这一个时钟：码上的倒计时与列表里每一行不会各走各的。 */
  const now = useNowSeconds();

  /**
   * 折叠面板的用户覆盖值。`null` = 还没手动开合过，用推导的默认态（见下方 `open`）。
   *
   * 声明在这里而不是贴着 `open` 一起写：`doGenerateInvite` 要用 `setOverride`，
   * 那个函数在文件里更靠前，声明在后会踩 use-before-declaration。
   */
  const [override, setOverride] = useState<boolean | null>(null);
  /**
   * 「这一条我看过了，先收起来」。压过 `needsAttention`，**但不碰内容**（见下方 `open`）。
   *
   * 与 `override` 一样声明在这里而不是贴着 `open`：`setInviteAndPreview` 要用它，
   * 那个函数在文件里更靠前。
   */
  const [dismissed, setDismissed] = useState(false);

  // —— 消费邀请（受邀方）——
  const [inviteInput, setInviteInput] = useState("");
  /**
   * 消费邀请的结果 —— **成功与「对方拒绝」共用这一个 state**，因为后者不是错误。
   *
   * `refused !== null`：对方在确认卡上点了拒绝，或那条邀请已被撤销/用掉。网络一切正常，
   * 所以它不能走 `WebErrorCard`（那会显示标题「网络错误」）。
   *
   * `persisted === false` 是一种「一半成功」：对端已经把本机加进它的已配对列表、本页面
   * 也能立刻用，只是这条记录没写进 IndexedDB，刷新后会不见。既不能报成失败（那会和
   * 对端的认知分叉），也不能当普通成功——所以两条信息都要显示。
   */
  const [consumeOutcome, setConsumeOutcome] = useState<PairingOutcomeJson | null>(
    null,
  );
  const consumeAction = useAsyncAction();
  /**
   * 握手还没发出，正在等本机连上中继（`pair-readiness.ts`）。
   *
   * **不进 `useAsyncAction`**：那个 hook 表达的是「有没有一个动作在跑」，而这是那个动作
   * **内部**的第一个子阶段。塞进去就得给它加一个通用的「阶段」概念，而另外六个调用点
   * 一个都用不上。
   */
  const [awaitingNetwork, setAwaitingNetwork] = useState(false);
  /**
   * 中断在途的那次配对尝试。**只在等待中继那一段真的能中断** —— 握手一旦发出，对端就
   * 可能已经 CAS 消费掉邀请，此时「取消」只能是本地忘掉结果，不是撤回请求。
   */
  const consumeAbort = useRef<AbortController | null>(null);
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
  /**
   * 确认对话框被**收起**（Esc / 点遮罩）而不是被放弃（点「取消」）。
   *
   * 两者必须分开：`/p/` 落地页那条来路已经 `sessionStorage.removeItem` 并
   * `history.replaceState` 抹掉了 fragment，`inviteInput` 是那串邀请在世上唯一的副本，
   * 而 Esc 是最容易误触的键。收起只是收起——`preview` 留着，下面那行入口能把它请回来。
   * 同一条教训在本文件的折叠逻辑里已经记过一次（见 `dismissed`）。
   */
  const [confirmDismissed, setConfirmDismissed] = useState(false);

  /**
   * **握手失败要把确认卡请回来。** 「收起」说的是「这一条我看过了」，不是「出了错也别
   * 告诉我」——与「新邀请压过收起」是同一条规则。
   *
   * 这条路径不是假想的：等中继那最长二十秒里 Esc 是**放行**的（见对话框的
   * `onOpenChange`，那是刻意的：收起 ≠ 放弃），于是失败很可能落在卡片已经收起之后。
   * 而 `consumeAction.error` 的唯一消费者就是那张卡：不请回来，用户看到的是「什么都
   * 没发生」，而一次性邀请此刻可能已经被对端消费掉了。内联入口那行只写「已识别到
   * 「X」的邀请 · 查看」，一个字都不提出过错。
   */
  useEffect(() => {
    if (consumeAction.error) setConfirmDismissed(false);
  }, [consumeAction.error]);

  /** 三格永远一起变——收口成一个函数，免得某条路径漏清其中一格留下前一次的残影。 */
  const resetPreview = () => {
    setPreview(null);
    setPreviewError(null);
    setPreviewNotice(null);
  };
  /**
   * 把一条邀请串放进消费框的**唯一入口**。手打 / 剪贴板感知 / `/p/` 落地页 handoff
   * 三条来路全部收口在这里，于是「解码 → 确认卡 → 用户点确认」这道闸没有旁路——
   * #98 的硬约束原文：不要因为「用户点了链接」就当作已确认。
   *
   * 解码是**纯本地同步计算**（不拨号、不查 DHT、不碰 IndexedDB），所以敢挂在每一次输入
   * 变化上；「确认卡出现之前零出网」也正是靠这一点成立。
   */
  const setInviteAndPreview = (link: string) => {
    // **握手在途时不换邀请。** `connect_invite` 一旦发出，对端就可能已经 CAS 消费掉那条
    // 邀请；此时换串会走到下面的 `cancel()`，把 seq 推进一格，于是真结果回来时
    // `useAsyncAction` 判它过期直接丢弃——不设 `consumeOutcome`、不刷设备清单、也不报
    // 「一半成功」。用户看到的是「什么都没发生」，而邀请已经用掉了，再点只会拿到
    // 「已被使用」的拒绝。
    //
    // 这条路径不是假想的：确认对话框打开时 `document` 上的 paste 监听照常工作（事件目标是
    // 对话框里的按钮，不匹配那个 `input/textarea/contenteditable` 早退条件），而握手走 relay
    // 时界面上好几秒没有动静，再按一次 Cmd+V 是很自然的动作。
    //
    // **但等中继那一段不适用**：那时 `pending` 已经为真而握手还没发出，上面那个理由
    // 一条都不成立。照旧拦下的话，用户在长达二十秒里换不掉一条粘错的邀请——输入框看着
    // 还能打字（它只按 `ready` 禁用），敲进去的字却一个都不生效。
    if (consumeAction.pending && !awaitingNetwork) return;
    // 走到这里说明要换邀请了：把在途那次等待掐掉，否则它会照常走完并发出握手，
    // 用一条**用户已经换掉**的邀请去配对（而邀请是一次性的）。
    consumeAbort.current?.abort();
    setInviteInput(link);
    setConsumeOutcome(null);
    resetPreview();
    // 新邀请要重新亮出确认卡，压过上一条的「收起」。
    setConfirmDismissed(false);
    // 上一条邀请的握手错误属于上一条。不清掉的话，「A 配对失败 → 不关对话框、直接粘贴 B」
    // 会让 B 的确认卡顶着 A 的错误开场。
    consumeAction.cancel();
    // 新到的邀请要压过之前的「收起」——`dismissed` 说的是「这一条我看过了」，
    // 不是「以后都别弹」。三条来路里有两条不经过点击，藏起来等于用户不知道它来了。
    setDismissed(false);
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
    setConfirmDismissed(false);
    // 同 `setInviteAndPreview`：错误跟着它所属的那条邀请一起走。
    consumeAction.cancel();
    // 还在等中继时按下取消：真的把它掐掉。少了这行，那次等待会照常走完 20 秒然后
    // **发出握手**，把一条用户已经放弃的邀请消费掉——而邀请是一次性的。
    consumeAbort.current?.abort();
    setAwaitingNetwork(false);
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
    setConsumeOutcome(null);
    const link = inviteInput.trim();
    // `preview` 在 await 之后不能再读闭包外的那个 state（用户可能已经换了一条），
    // 但这一趟要用的两件事此刻就定了：串本身与它是不是 LocalOnly。
    // 名字带 `invite` 前缀是必须的——本组件里另有一个**生成**侧的 `localOnly` state
    // （「我要发的这条限同网」），两者都是布尔、主语相反，撞名不会有任何编译错误。
    const inviteIsLocalOnly = preview.localOnly;
    const inviteExpiresAt = preview.expiresAt;
    const abort = new AbortController();
    consumeAbort.current = abort;
    consumeAction.run(
      async () => {
        // **LocalOnly 邀请里没有 circuit 地址**（`select_invite_addrs` 只放私网那一桶），
        // 等中继纯属白等；何况这类场景常常压根连不上公网。
        if (!inviteIsLocalOnly) {
          setAwaitingNetwork(true);
          try {
            // 等不到也照常握手 —— 它只推迟，不否决，理由见 `pair-readiness.ts` 文件头。
            await waitForPairingReadiness(PAIRING_READINESS_TIMEOUT_MS, abort.signal);
          } finally {
            setAwaitingNetwork(false);
          }
        }
        // 等待期间用户按了取消：此刻一个字节都还没出网，邀请仍然可用，就此打住。
        // 抛出去而不是安静返回 —— `consumeAction.cancel()` 已经推过 seq，这个 reject
        // 会被判过期直接丢弃，不会渲染成一张错误卡。
        abort.signal.throwIfAborted();
        // **等完再查一次 TTL。** 上面那段最长等二十秒，而邀请可能只剩几秒 —— 界面上
        // 确认按钮已经因为 `expired` 重算而自己消失了（用户唯一的反馈），握手却照发，
        // 拿一条过期凭证去撞对端的 TTL 检查。让结果与 UI 已经声称的事情一致。
        if (remainingSeconds(inviteExpiresAt, Math.floor(Date.now() / 1000)) <= 0) {
          throw {
            kind: "invalidInput",
            message: t`这条邀请已经过期，请让对方重新生成。`,
          } satisfies WebError;
        }
        // **句柄要重新取一次，不能用闭包里那个。** 等待可以长达二十秒，其间用户完全
        // 可能在节点状态弹窗里按下停止 —— `closeNode()` 会 `close(self)` **释放 wasm
        // 侧的指针**，再拿旧句柄调方法得到的是一句 `null pointer passed to rust`，
        // 用户看不懂，日志里也指不回真正的原因。
        //
        // 比对实例而不只是判空：重启后 `spawnNode()` 给的是**另一个**节点，它没有这条
        // 邀请所属的那套上下文，照样不该拿来续这一趟。
        const live = getNode();
        // 抛 `WebError` 而不是裸 `Error`：`toWebError` 的兜底分支一律给 `kind: "network"`，
        // 于是这句会顶着一张标题写「网络错误」的卡片出现 —— 把用户指向他的网络，
        // 而实际原因是他刚按下的那个停止按钮。
        if (live !== node) {
          throw {
            kind: "aborted",
            message: t`节点已停止，重新启动后再试一次。`,
          } satisfies WebError;
        }
        return node.connect_invite(link);
      },
      (outcome) => {
        setConsumeOutcome(outcome);
        clearInvite();
        if (outcome.refused) return;
        refreshPairedDevices(node);
        if (!outcome.persisted) {
          // **必须是 toast，不能只靠下面那行内联提示** —— 同 `doRevokeInvite` 的理由：
          // `consumeOutcome` 是本组件的 state，而本组件只挂在 `/app/devices`。用户配完对
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
    // 生成是明确的「我要看这个码」，把面板钉开（见 `forceOpen` 上方那段：码本身不进
    // forceOpen，靠这里的显式 override 撑开，于是用户随时能再关掉它）。
    setOverride(true);

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
  // ⚠️ **必须经 ref 转发，不能让空依赖的 effect 直接闭包住 `setInviteAndPreview`。**
  // 它现在读渲染态（`consumeAction.pending` / `awaitingNetwork`），而空依赖的 effect 只在
  // mount 跑一次、捕获的是**首帧**那份——那一份里 `pending` 恒为 `false`，于是
  // 「握手在途时不换邀请」这条守卫在 paste 这条路径上**完全不生效**，而它的注释点名
  // 保护的正是这条路径（对话框上再按一次 Cmd+V）。失效的后果是真结果被 `cancel()` 丢弃：
  // 邀请已被对端消费，用户却什么都看不到，重试只会拿到「已被使用」。
  //
  // 用 latest-ref 而不是把函数放进依赖数组：后者每次渲染都会重新注册 document 监听。
  const setInviteAndPreviewRef = useRef(setInviteAndPreview);
  useEffect(() => {
    setInviteAndPreviewRef.current = setInviteAndPreview;
  });

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
      setInviteAndPreviewRef.current(link);
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

  // —— 折叠：配对是一次性动作，不该常年占着设备页的半屏 ——
  //
  // 本文件开头那句「配对是一次性动作，配完即长期信任」此前只写在注释里，版面没有体现：
  // 这个面板（消费邀请 + 生成邀请 + 已发出的邀请三段）是**竖排版式下**设备页最高的一块，
  // 而它的使用频率比上面的设备清单低两个数量级。
  //
  // ⚠️ 上面那句限定语是后加的，别再把它去掉：≥1280px 时这块独占一条 360px 侧栏
  // （`devices-section.tsx`），既不与设备清单争版面，收起反而只剩一栏空玻璃——
  // 那一档由调用方传 `defaultOpen` 翻转过来。**折叠的理由是版式，不是这块内容本身。**
  //
  // 默认态**由状态推导，不落 state**：
  //   · 一台设备都没有 → 配对就是这一页唯一能做的事，展开。
  //   · 已经有设备 → 看版式（`defaultOpen`）：分栏展开、竖排收起成一行。
  // 用户手动开合后 `override` 接管，此后不再被推导值改写。
  //
  // `forceOpen` 压过一切，收的是**用户没主动要求看、却必须看见**的两类东西：
  //
  //   1. 一条待决的邀请。三条来路里有两条不经过点击（剪贴板感知、`/p/` 落地页 handoff），
  //      确认卡就长在这块面板里——收起等于用户看不见自己刚粘进来的东西。同「导航徽标是
  //      补偿不是装饰」那条理由：有时效的入站信号不能被藏起来。
  //   2. `consumeOutcome`。**这条不是可有可无的**：配对成功那一刻 `pairedCount` 从 0 变 1，
  //      推导出的默认态随即翻成「收起」——不压住的话，成功提示会在出现的同一帧被折走。
  //
  // 生成出来的码**不在**这份清单里：那是用户自己按出来的，看不看由他决定。改成生成时
  // 显式 `setOverride(true)`，于是它开着是因为用户开了它，随时能关。
  const needsAttention =
    inviteInput.trim() !== "" ||
    preview !== null ||
    previewError !== null ||
    previewNotice !== null ||
    consumeOutcome !== null;
  // `dismissed` 压过 `needsAttention`，**但不碰内容**。
  //
  // 此前 `onToggle` 在强开态下调 `clearInvite()` 才能关掉面板，那是一次**不可恢复的删除**：
  // `/p/` 落地页 handoff 那条路径已经 `sessionStorage.removeItem` 并 `history.replaceState`
  // 抹掉了 fragment，React state 是那条邀请在世上唯一的副本；剪贴板感知那条同理
  // （用户压根没主动输入过，更不会想到点一下标题会删东西）。而收起态整行可点，
  // 恰恰是这一屏最像「关掉它」的东西。
  //
  // 现在收起只是收起，再点开内容还在。真要放弃那条邀请，出口是确认卡自己的「取消」——
  // 用户按下它时知道自己在放弃什么。
  const open =
    (needsAttention && !dismissed)
    || (override ?? (defaultOpen || (ready && pairedCount === 0)));
  const bodyId = useId();

  // 「添加设备」卡片按下 → 开面板并滚到它。
  //
  // 只置 `override`，**不动 `dismissed`**：`override === true` 时上面那个 `||` 的右操作数
  // 直接为真，`dismissed` 根本不参与求值，清它是死代码。
  //
  // 滚动而不只是展开：卡片在设备网格里，面板在网格**下方**，设备一多就在视口外——
  // 点了卡片却什么都没动，看起来像按钮坏了。
  //
  // ## 两条都必须在下一帧做，不能跟 `setOverride` 挤在同一个同步块里
  //
  // `setOverride(true)` 是批处理的，DOM 要到事件处理器返回之后才更新。同步调
  // `scrollIntoView` 量到的是**收起态**那 ~100px 的外壳，`block: "nearest"` 据此判定
  // 「已经完全可见」就一动不动——展开后的表单全部落在折线以下，正是上面那句注释要避免的
  // 现象。`behavior: "smooth"` 还会把这个失准固化：滚动目标在调用时刻就算死了。
  //
  // ## 聚焦输入框是这里唯一在所有路径上都成立的反馈
  //
  // 空态（`pairedCount === 0`）时面板**本来就是展开的**（见下面 `open` 的推导），点「添加
  // 设备」既没有状态变化也没有滚动——只有焦点落进输入框这一件事是可见的。它同时也是
  // 对键盘用户唯一有意义的响应：光标已经在该打字的地方。
  //
  // `preventScroll: true` 让滚动只由上面那次 `scrollIntoView` 负责，不叠加浏览器
  // 自己的聚焦滚动（两者节奏不同，叠起来会抖一下）。
  const panelRef = useRef<HTMLElement>(null);
  const inviteInputRef = useRef<HTMLInputElement>(null);
  useImperativeHandle(ref, () => ({
    open() {
      setOverride(true);
      requestAnimationFrame(() => {
        panelRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
        inviteInputRef.current?.focus({ preventScroll: true });
      });
    },
  }), []);

  return (
    <SectionShell ref={panelRef}>
      <SectionHeader
        icon={Link2}
        title={<Trans>配对</Trans>}
        description={<Trans>与另一台设备互换一次邀请即可，之后长期有效。</Trans>}
        disclosure={{
          open,
          controls: bodyId,
          // 关：`dismissed` 压住 `needsAttention`（内容留着）。开：清掉 dismissed 再置 override。
          onToggle: () => {
            setDismissed(open);
            setOverride(!open);
          },
        }}
      />

      {open && (
        <div id={bodyId} className="flex flex-col gap-[var(--space-in-panel)]">
      <div>
        <p className="text-xs font-medium text-muted-foreground">
          <Trans>粘贴对方给你的邀请</Trans>
        </p>
        {/* 没有「配对」按钮：串一进框就地解码，动作长在下面那张确认卡上（#98）。 */}
        <Input
          ref={inviteInputRef}
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
        {/* 确认卡被收起后把它请回来的入口。**没有它，Esc 就成了一次静默的功能失效**：
            邀请串还在（这正是收起与放弃的分别），但屏幕上没有任何东西说它还在、
            也没有任何地方能再走到「确认配对」——而那是这条路径唯一的出口。 */}
        {preview && confirmDismissed && (
          <button
            type="button"
            onClick={() => setConfirmDismissed(false)}
            className="focus-ring mt-2 flex min-h-11 w-full items-center justify-between gap-2 rounded-lg border px-3 text-left text-xs transition-colors hover:bg-accent sm:min-h-9"
          >
            <span className="min-w-0 truncate text-muted-foreground">
              <Trans>
                已识别到「{preview.displayName || t`对方设备`}」的邀请
              </Trans>
            </span>
            <span className="shrink-0 font-medium text-foreground">
              <Trans>查看</Trans>
            </span>
          </button>
        )}
        {previewError && <WebErrorCard error={previewError} className="mt-2 text-xs" />}
        {/*
          确认卡走 Dialog（`PairingConfirmDialog`），不再内联在这一栏里。
          它与入站的 `PairingRequestHost` 是同一件事的两个方向，本该长一样；而这一栏在
          ≥1280 档只有 360px 宽，装不下让人核对身份该有的信息密度（完整 NodeId 就摆不开）。
          理由与 DESIGN.md 的关系写在那个组件的文件头。

          「出网之前先把对方是谁摆出来」这道闸没有变——`preview` 仍然是 `connect_invite`
          的唯一前置（见 `doConsumeInvite`），只是它现在长在对话框上。

          握手失败（`consumeAction.error`）也留在对话框里：那时邀请还没被消费，用户可以
          直接再点一次「确认配对」，把错误显示在这里反而要求他先关掉对话框才看得见。
        */}
        {consumeOutcome?.refused && (
          <>
            {/* 对方拒绝**不是错误**：不走 WebErrorCard（那会给出「网络错误」这个标题）。 */}
            <p className="mt-2 text-xs text-warning-ink">
              {t(PAIRING_REFUSED_LABEL[consumeOutcome.refused.type])}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              <Trans>邀请是一次性的：若对方已撤销它、或它已被别的设备用掉，也会走到这里——让对方重新生成一条。</Trans>
            </p>
          </>
        )}
        {consumeOutcome && !consumeOutcome.refused && (
          <>
            {/* `break-all` 不能省：PeerId 是 52 个字符的连续 base58，没有断点。
                这一栏在 xl 档只有 360px，一条不换行的等宽串会把**整个页面**顶出一条
                横向滚动条（实测于 1400px 视口：`documentElement.scrollWidth` 2800 → 页面
                整体可横向拖动）。溢出的是 flex 子项，父容器的 `overflow-hidden` 拦不住它，
                只有让字符串自己允许断行才行。 */}
            <p className="mt-2 text-xs text-success-ink">
              <Trans>
                已配对：<span className="font-mono break-all">{consumeOutcome.peerId}</span>
              </Trans>
            </p>
            {!consumeOutcome.persisted && (
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
                这里说清楚**为什么**（浏览器不监听端口）与**该做什么**（去连一个引导节点）。 */}
            <Trans>
              浏览器不能被直接拨号，需要先连上一个引导节点，对方才找得到你。到{" "}
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
        </div>
      )}

      {/*
        出站确认对话框。**必须挂在 `open &&` 之外**：邀请有三条来路，其中剪贴板感知与
        `/p/` 落地页 handoff 都不经过用户点击，而面板可能正处在收起态（`dismissed`）——
        挂在里面等于那两条路径静默失效。
      */}
      <PairingConfirmDialog
        preview={confirmDismissed ? null : preview}
        now={now}
        pending={consumeAction.pending}
        awaitingNetwork={awaitingNetwork}
        error={consumeAction.error}
        onConfirm={doConsumeInvite}
        onCancel={clearInvite}
        onDismiss={() => setConfirmDismissed(true)}
      />
    </SectionShell>
  );
}
