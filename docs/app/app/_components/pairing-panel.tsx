"use client";

// #77 配对面板：消费邀请（受邀方）/ 生成邀请（发起方，需先 #76 reserve）/ 入站配对请求确认。
// 隐式优先（PRODUCT.md 原则 1）：配对是一次性动作，配完即长期信任，不做成每次传输都要选的
// 模式——本面板只负责把「配对」这一次性动作走完，配对后的设备去下方「已配对设备」清单看。

import { useState } from "react";
import { WebErrorCard } from "./web-error-view";
import { getNode } from "../_lib/node-runtime";
import { useAsyncAction } from "../_lib/use-async-action";
import { useWebNode, webNodeActions } from "../_lib/store";
import { toWebError, type WebError, type WebNode } from "../_lib/view-types";

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

  const doGenerateInvite = () => {
    const node = getNode();
    if (!node) return;
    setGenerateError(null);
    try {
      setGeneratedInvite(node.generate_invite(localOnly));
    } catch (e) {
      setGenerateError(toWebError(e));
    }
  };

  // —— 入站配对请求确认（每条请求可独立并发处理，故用 Set 而非单一 id）——
  const [respondingIds, setRespondingIds] = useState<Set<string>>(new Set());
  const [respondError, setRespondError] = useState<WebError | null>(null);

  const respond = async (pendingId: string, accept: boolean) => {
    const node = getNode();
    if (!node) return;
    setRespondingIds((prev) => new Set(prev).add(pendingId));
    setRespondError(null);
    try {
      await node.respond_pairing_request(pendingId, accept);
      webNodeActions.removePendingPairing(pendingId);
      if (accept) refreshPairedDevices(node);
    } catch (e) {
      setRespondError(toWebError(e));
    } finally {
      setRespondingIds((prev) => {
        const next = new Set(prev);
        next.delete(pendingId);
        return next;
      });
    }
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
            placeholder="sdinvite..."
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
            需先在上方「连接」区 reserve 拿到 circuit 可达地址，否则邀请里无可拨地址。
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
                    disabled={respondingIds.has(r.pendingId)}
                    className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    接受
                  </button>
                  <button
                    type="button"
                    onClick={() => respond(r.pendingId, false)}
                    disabled={respondingIds.has(r.pendingId)}
                    className="rounded-lg border border-fd-border px-2.5 py-1 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    拒绝
                  </button>
                </div>
              </li>
            ))}
          </ul>
          {respondError && <WebErrorCard error={respondError} className="mt-2 text-xs" />}
        </div>
      )}
    </div>
  );
}
