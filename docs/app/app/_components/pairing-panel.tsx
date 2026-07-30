"use client";

// #77 配对面板：消费邀请（受邀方）/ 生成邀请（发起方，需先 #76 reserve）/ 入站配对请求确认。
// 隐式优先（PRODUCT.md 原则 1）：配对是一次性动作，配完即长期信任，不做成每次传输都要选的
// 模式——本面板只负责把「配对」这一次性动作走完，配对后的设备去下方「已配对设备」清单看。

import Link from "next/link";
import { useEffect, useState } from "react";
import { WebErrorCard } from "./web-error-view";
import { NAV } from "../_lib/nav";
import { getNode } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { useWebNode, webNodeActions } from "../_lib/store";
import { toWebError, type WebError, type WebNode } from "../_lib/view-types";
import { formatDuration } from "../_lib/format";
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

  // —— 消费邀请（受邀方）——
  const [inviteInput, setInviteInput] = useState("");
  const [consumeSuccess, setConsumeSuccess] = useState<string | null>(null);
  const consumeAction = useAsyncAction();

  // 从配对落地页（/p/）过来时把邀请接过来预填。
  //
  // 主路径是 sessionStorage（落地页存完整 canonical 链接，capability 不进本页地址栏）；
  // 隐私模式下 storage 不可用，落地页会退回把 payload 挂在 fragment 上，故两处都读。
  // **读完立刻清掉**：capability 是一次性信任凭证，不该留在 storage 或地址栏里被刷新、
  // 分享、截图二次带走。不自动发起配对——由用户按下「配对」，与桌面端的安全闸一致。
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
      //
      // **前缀必须硬编码，不能用 `location.origin`**：解码器只认受理列表里的前缀，别的
      // 域名一律拒（`invite.rs` 里 `https://evil.example/P/#…` 那条负例钉着），而站点跑在
      // 预览域名或 localhost 时 `location.origin` 拼出来的串会被拒掉。
      //
      // 写哪个受理前缀都行、**域名迁移时不必跟着改**：`ACCEPTED_URL_PREFIXES` 同时受理
      // Pages 与 swarmapp.cn 两个前缀，所以这行不是迁移时的landmine。
      handoff = `https://swarm-apps.github.io/SwarmDrop/p/#${payload}`;
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
    setInviteInput(handoff);
  }, []);

  const doConsumeInvite = () => {
    const node = getNode();
    if (!node || !inviteInput.trim()) return;
    setConsumeSuccess(null);
    consumeAction.run(
      () => node.connect_invite(inviteInput.trim()),
      (peerId) => {
        setConsumeSuccess(peerId);
        setInviteInput("");
        refreshPairedDevices(node);
      },
    );
  };

  // —— 生成邀请（发起方 / browser-as-inviter）——
  const [localOnly, setLocalOnly] = useState(false);
  const [generateError, setGenerateError] = useState<WebError | null>(null);
  const [generatedInvite, setGeneratedInvite] = useState<string | null>(null);

  // —— 已发出的邀请（TTL 24h + 跨刷新存活 → 必须能看见和撤销）——
  //
  // 邀请现在活 24 小时且跨刷新存活（openspec: invite-persistence），「我有几条邀请在外面
  // 飘」不再是可以忽略的问题。列表里**没有邀请串本身**：capability 明文不落盘也不出注册表，
  // 所以刷新后只能显示元数据 + 提供撤销，想再分享就生成新的。
  const [invites, setInvites] = useState<InviteListItemJson[]>([]);
  const refreshInvites = () => {
    const node = getNode();
    if (!node) return;
    try {
      setInvites(node.list_invites());
    } catch {
      // 列表是辅助信息，读失败不打扰主流程
    }
  };

  useEffect(refreshInvites, [nodeStatus]);

  // generate / revoke 在 invite-persistence 里变成了 async（要写穿 IndexedDB，否则刷新后
  // 本机就不认识刚发出的邀请了）。revoke 保持 fire-and-forget：后端幂等，失败也不影响
  // 调用方要的终态。
  const doGenerateInvite = async () => {
    const node = getNode();
    if (!node) return;
    setGenerateError(null);
    // 旧邀请立即作废——邀请是一次性信任凭证，界面上被顶掉了就不该还能用到 TTL 到点。
    if (generatedInvite !== null) void node.revoke_invite(generatedInvite);
    try {
      setGeneratedInvite(await node.generate_invite(localOnly));
    } catch (e) {
      setGenerateError(toWebError(e));
    }
    refreshInvites();
  };

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
    });
  };

  return (
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <h2 className="text-sm font-semibold text-fd-foreground">配对</h2>

      <div className="mt-4">
        <p className="text-xs font-medium text-fd-muted-foreground">
          消费邀请（连接桌面 / 移动生成的邀请）
        </p>
        <div className="mt-2 flex gap-2">
          <input
            className="flex-1 rounded-lg border border-fd-border bg-fd-background px-3 py-2 font-mono text-xs text-fd-foreground placeholder:text-fd-muted-foreground"
            placeholder="https://swarm-apps.github.io/SwarmDrop/p/#..."
            value={inviteInput}
            onChange={(e) => setInviteInput(e.target.value)}
            disabled={!ready}
          />
          <button
            type="button"
            onClick={doConsumeInvite}
            disabled={!ready || !inviteInput.trim() || consumeAction.pending}
            className="shrink-0 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
          >
            {consumeAction.pending ? "配对中…" : "配对"}
          </button>
        </div>
        {consumeAction.error && <WebErrorCard error={consumeAction.error} className="mt-2 text-xs" />}
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
          onClick={() => void doGenerateInvite()}
          disabled={!ready || !reservation}
          className="mt-2 rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
        >
          生成邀请
        </button>
        {generateError && <WebErrorCard error={generateError} className="mt-2 text-xs" />}
        {generatedInvite && (
          <textarea
            readOnly
            value={generatedInvite}
            onFocus={(e) => e.currentTarget.select()}
            className="mt-2 h-20 w-full break-all rounded-lg border border-fd-border bg-fd-background p-2 font-mono text-xs text-fd-foreground"
          />
        )}
      </div>

      {invites.length > 0 && (
        <div className="mt-5 border-t border-fd-border pt-4">
          <p className="text-xs font-medium text-fd-muted-foreground">
            已发出的邀请（未过期）
          </p>
          <p className="mt-1 text-xs text-fd-muted-foreground">
            邀请有效期 24 小时且跨刷新保留。这里列不出原始链接（凭证明文不留存），
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
                      {formatRemaining(invite.expiresAt)}
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

/**
 * 邀请剩余有效期文案。
 *
 * `expiresAt` 是字符串承载的 Unix 秒（wasm 侧避开 u64 → BigInt 的取回麻烦）。
 * 已过期的条目本不该出现在列表里（后端按 now 过滤），但时钟跳变或列表未刷新时会撞上，
 * 所以这里显式给一句而不是显示负数。
 */
function formatRemaining(expiresAt: string): string {
  const seconds = Number(expiresAt) - Math.floor(Date.now() / 1000);
  return seconds <= 0 ? "已过期" : `${formatDuration(seconds)}后失效`;
}
