"use client";

// SwarmDrop Web 端到端测试页（/try）。
//
// 节点跑在**主线程 Window**——不是 Web Worker：webrtc-websys 拨号要碰 `window`，在 Worker 里
// 会 panic（见 crates/web/src/node.rs spawn 注释）。既然要支持 webrtc-direct，就必须主线程。
//
// wasm 模块（swarmdrop-web，workspace 包）是浏览器专属，**动态 import**——绝不在模块顶层静态
// import，否则 `next build` 预渲染阶段会尝试在 Node 里加载 wasm 而失败。

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  ConnectionJson,
  OfferJson,
  PendingPairingJson,
  WebTransferEvent,
} from "swarmdrop-web";
import { WEB_RELAY_HELPERS } from "./relay-helpers";

// WebNode 的运行时形状（swarmdrop-web 的 .d.ts 提供，这里仅取用到的方法）。
type WebNode = {
  node_id(): string;
  connect(addr: string, signal?: AbortSignal): Promise<ConnectionJson>;
  // 真配对握手（pair_with_invite）→ 成功返回已配对对端 NodeId（base58）。
  connect_invite(invite: string): Promise<string>;
  // relay 意图（声明式）：ensure 登记（同步、返回 helper 的 NodeId）→
  // until_active 等首次建立（failed 立即 reject）。id 由 ensure 返回值串联。
  relays_ensure(helperAddr: string): string;
  relays_until_active(helperId: string, signal?: AbortSignal): Promise<string>;
  relays_drop(helperId: string): Promise<void>;
  // browser-as-inviter：生成邀请串（供桌面扫码/粘贴消费）。需先建 reservation 才有可达地址。
  generate_invite(localOnly: boolean): string;
  pending_pairing_requests(): PendingPairingJson[];
  respond_pairing_request(pendingId: string, accept: boolean): Promise<void>;
  send_files(to: string, files: File[]): Promise<string>;
  pending_offers(): OfferJson[];
  accept_offer(sessionId: string): Promise<void>;
  reject_offer(sessionId: string): Promise<void>;
  download_url(relativePath: string): Promise<string>;
  events(): ReadableStream<WebTransferEvent>;
  close(): Promise<void>;
};

type OfferView = {
  sessionId: string;
  who: string;
  totalSize: number;
  files: { relativePath: string; name: string }[];
};

type XferView = {
  sessionId: string;
  pct: number;
  status: string;
  downloads: { name: string; url: string }[];
};

export default function TryPage() {
  const nodeRef = useRef<WebNode | null>(null);
  const [nodeId, setNodeId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [secure, setSecure] = useState(true);
  const [logLines, setLogLines] = useState<string[]>([]);
  const [offers, setOffers] = useState<Record<string, OfferView>>({});
  const [xfers, setXfers] = useState<Record<string, XferView>>({});
  // browser-as-inviter：本机生成的邀请串 + 挂起的入站配对请求（桌面消费本机 invite 后到达）。
  const [myInvite, setMyInvite] = useState("");
  const [reserving, setReserving] = useState(false);
  // 最近一次 ensure 的 helper NodeId（relays_ensure 返回值，drop / until_active 串联用）。
  const helperIdRef = useRef<string | null>(null);
  const [pairingReqs, setPairingReqs] = useState<PendingPairingJson[]>([]);

  // 表单
  const [addr, setAddr] = useState(() => WEB_RELAY_HELPERS[0] ?? "");
  const [invite, setInvite] = useState("");
  const [peer, setPeer] = useState("");
  const filesRef = useRef<HTMLInputElement | null>(null);

  // 会话 → 文件（完成后生成下载链接）。
  const offerFilesRef = useRef<Map<string, { relativePath: string; name: string }[]>>(new Map());

  const log = useCallback((m: string) => {
    setLogLines((prev) => [...prev.slice(-300), m]);
  }, []);

  useEffect(() => {
    setSecure(typeof window !== "undefined" ? window.isSecureContext : true);
  }, []);

  const consumeEvents = useCallback(
    async (node: WebNode) => {
      const reader = node.events().getReader();
      for (;;) {
        let ev: WebTransferEvent | undefined;
        let done: boolean;
        try {
          ({ value: ev, done } = await reader.read());
        } catch (e) {
          log("❌ reader.read 抛错: " + String(e));
          break;
        }
        if (done) break;
        if (!ev) continue;
        try {
          handleEvent(node, ev);
        } catch (e) {
          log(`❌ 处理事件 ${ev.type} 抛错(已跳过): ${String(e)}`);
        }
      }
    },
    [log],
  );

  const handleEvent = useCallback(
    (node: WebNode, ev: WebTransferEvent) => {
      switch (ev.type) {
        case "transferOfferReceived": {
          const o = ev.offer;
          const files = o.files.map((f) => ({ relativePath: f.relativePath, name: f.name }));
          offerFilesRef.current.set(o.sessionId, files);
          setOffers((prev) => ({
            ...prev,
            [o.sessionId]: {
              sessionId: o.sessionId,
              who: o.deviceName || o.peerId.slice(0, 8),
              totalSize: o.totalSize,
              files,
            },
          }));
          break;
        }
        case "transferProgress": {
          const e = ev.event;
          const total = Number(e.totalBytes);
          const got = Number(e.transferredBytes);
          const pct = total ? Math.floor((100 * got) / total) : 0;
          const speed = e.speed ? (Number(e.speed) / 1024 / 1024).toFixed(2) : "?";
          setXfers((prev) => ({
            ...prev,
            [e.sessionId]: {
              ...(prev[e.sessionId] ?? { downloads: [] }),
              sessionId: e.sessionId,
              pct,
              status: `${e.direction} ${pct}% (${got}/${total}) ${speed} MB/s`,
            },
          }));
          break;
        }
        case "transferCompleted": {
          const e = ev.event;
          setXfers((prev) => ({
            ...prev,
            [e.sessionId]: {
              ...(prev[e.sessionId] ?? { downloads: [] }),
              sessionId: e.sessionId,
              pct: 100,
              status: `✅ 完成（${e.direction}）`,
            },
          }));
          if (e.direction === "receive") void addDownloads(node, e.sessionId);
          break;
        }
        case "transferFailed": {
          const e = ev.event;
          setXfers((prev) => ({
            ...prev,
            [e.sessionId]: {
              ...(prev[e.sessionId] ?? { downloads: [] }),
              sessionId: e.sessionId,
              pct: prev[e.sessionId]?.pct ?? 0,
              status: "❌ 失败: " + e.error,
            },
          }));
          break;
        }
        case "transferPaused": {
          const sid = ev.event.sessionId;
          setXfers((prev) => ({
            ...prev,
            [sid]: { ...(prev[sid] ?? { downloads: [], pct: 0 }), sessionId: sid, status: "⏸ 暂停" } as XferView,
          }));
          break;
        }
        case "transferProjection": {
          // data channel 建立失败、对端断开等恢复型错误只会以 projection
          // （suspended / Interrupted）上报，并不会再额外发 transferFailed。
          // try 页必须把它展开，否则表面上会像“已接受后卡住”。
          const p = ev.projection;
          const reason =
            p.errorMessage ?? p.suspendedReason ?? p.terminalReason ?? "";
          const status = [p.direction, p.phase, reason].filter(Boolean).join(" · ");
          setXfers((prev) => ({
            ...prev,
            [p.sessionId]: {
              ...(prev[p.sessionId] ?? { downloads: [], pct: 0 }),
              sessionId: p.sessionId,
              pct: p.totalSize
                ? Math.floor(
                    (100 * Number(p.transferredBytes)) / Number(p.totalSize),
                  )
                : 0,
              status,
            },
          }));
          log(`projection: session=${p.sessionId} ${status}`);
          break;
        }
        default:
          log("event: " + ev.type);
      }
    },
    [log],
  );

  const addDownloads = useCallback(async (node: WebNode, sessionId: string) => {
    const files = offerFilesRef.current.get(sessionId) ?? [];
    const dls: { name: string; url: string }[] = [];
    for (const f of files) {
      try {
        dls.push({ name: f.name, url: await node.download_url(f.relativePath) });
      } catch (e) {
        log("❌ download_url " + String(e));
      }
    }
    setXfers((prev) => ({
      ...prev,
      [sessionId]: { ...(prev[sessionId] ?? { pct: 100, status: "" }), sessionId, downloads: dls } as XferView,
    }));
  }, [log]);

  const start = useCallback(async () => {
    if (nodeRef.current || starting) return;
    setStarting(true);
    try {
      // 动态 import：wasm 浏览器专属。default 是 wasm-pack --target web 的 init（拉 .wasm）。
      const wasm = await import("swarmdrop-web");
      await (wasm as unknown as { default: () => Promise<unknown> }).default();
      const node = (await (wasm as unknown as { WebNode: { spawn(): Promise<WebNode> } }).WebNode.spawn()) as WebNode;
      nodeRef.current = node;
      const id = node.node_id();
      setNodeId(id);
      log("node spawned: " + id);
      void consumeEvents(node);
    } catch (e) {
      log("❌ spawn 失败: " + String(e));
    } finally {
      setStarting(false);
    }
  }, [starting, log, consumeEvents]);

  const doConnect = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      const c = await node.connect(addr.trim());
      log(`connected: path=${c.path} addr=${c.addr}`);
    } catch (e) {
      log("❌ connect " + errStr(e));
    }
  }, [addr, log]);

  const doReserve = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      // ensure 返回 helper 的 NodeId，直接串联 until_active / drop——JS 侧不解析 multiaddr
      const helperId = node.relays_ensure(addr.trim());
      helperIdRef.current = helperId;
      log("relay 意图已登记，等待 reservation…");
      log("reserve → " + (await node.relays_until_active(helperId)));
    } catch (e) {
      log("❌ reserve " + errStr(e));
    }
  }, [addr, log]);

  // 撤销 relay 意图（真撤销：停止后台收敛重试、断开连接）。
  const doDropRelay = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    const helperId = helperIdRef.current;
    if (!helperId) return log("先 reserve 过才有可撤销的 relay 意图");
    try {
      await node.relays_drop(helperId);
      helperIdRef.current = null;
      log("relay 意图已撤销（后台不再重试）");
    } catch (e) {
      log("❌ drop " + errStr(e));
    }
  }, [log]);

  const doInvite = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      const peerId = await node.connect_invite(invite.trim());
      log(`✅ 已配对: ${peerId}（现在可与该设备互传，无需再连）`);
    } catch (e) {
      log("❌ connect-invite " + errStr(e));
    }
  }, [invite, log]);

  const doSend = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    const files = Array.from(filesRef.current?.files ?? []);
    if (!files.length) return log("先选文件");
    try {
      const sid = await node.send_files(peer.trim(), files);
      log("offer 已发出: session=" + sid);
    } catch (e) {
      log("❌ send " + errStr(e));
    }
  }, [peer, log]);

  const acceptOffer = useCallback(async (sid: string) => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      await node.accept_offer(sid);
      setOffers((prev) => { const n = { ...prev }; delete n[sid]; return n; });
    } catch (e) {
      log("❌ accept " + errStr(e));
    }
  }, [log]);

  const rejectOffer = useCallback(async (sid: string) => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      await node.reject_offer(sid);
      setOffers((prev) => { const n = { ...prev }; delete n[sid]; return n; });
    } catch (e) {
      log("❌ reject " + errStr(e));
    }
  }, [log]);

  // browser-as-inviter：本机经一个中继建 reservation（拿可达 circuit 地址）后生成邀请。
  const reserveHelper = useCallback(async () => {
    const node = nodeRef.current;
    if (!node) return;
    if (!addr.trim()) return log("先在①填 helper 的 ws 地址再 reserve");
    setReserving(true);
    try {
      const helperId = node.relays_ensure(addr.trim());
      helperIdRef.current = helperId;
      const circuit = await node.relays_until_active(helperId);
      log("✅ reserve ok（本机现可被拨）→ " + circuit);
    } catch (e) {
      log("❌ reserve " + errStr(e));
    } finally {
      setReserving(false);
    }
  }, [addr, log]);

  const showInvite = useCallback(() => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      const inv = node.generate_invite(false);
      setMyInvite(inv);
      log("已生成邀请（Auto）——桌面用「粘贴邀请配对」消费它");
    } catch (e) {
      log("❌ generate_invite " + errStr(e));
    }
  }, [log]);

  const respondPairing = useCallback(async (pendingId: string, accept: boolean) => {
    const node = nodeRef.current;
    if (!node) return;
    try {
      await node.respond_pairing_request(pendingId, accept);
      setPairingReqs((prev) => prev.filter((r) => r.pendingId !== pendingId));
      log(accept ? "✅ 已接受配对" : "已拒绝配对");
    } catch (e) {
      log("❌ respond " + errStr(e));
    }
  }, [log]);

  // 轮询入站配对请求（桌面消费本机 invite 后到达 → 本机弹确认）。
  // 仅在已生成邀请后开轮询——入站配对请求只可能在 browser-as-inviter（本机 generate_invite
  // 被桌面消费）后到达，未生成邀请前轮询全程空转。
  useEffect(() => {
    if (!nodeId || !myInvite) return;
    const iv = setInterval(() => {
      const node = nodeRef.current;
      if (!node) return;
      try {
        const reqs = node.pending_pairing_requests();
        if (reqs.length) setPairingReqs((prev) => [...prev, ...reqs]);
      } catch {
        /* ignore */
      }
    }, 1500);
    return () => clearInterval(iv);
  }, [nodeId, myInvite]);

  const ready = !!nodeId;
  const offerList = Object.values(offers);
  const xferList = Object.values(xfers);

  return (
    <main className="mx-auto max-w-3xl px-4 py-8 font-mono text-sm">
      <h1 className="text-xl font-bold">SwarmDrop Web — 端到端测试</h1>
      <p className="mt-1 text-fd-muted-foreground">
        浏览器传输端：offer / accept / 续传 / bao 逐块验证全量复用内核。身份存 localStorage，收到的文件落 OPFS。
        节点跑主线程（webrtc 需 window）。
      </p>

      {!secure && (
        <div className="mt-3 rounded border border-red-400 bg-red-100 p-2 font-bold text-red-800">
          ⚠ 当前非 secure context：navigator.storage / crypto.subtle 不可用，接收方落盘会失败。
          请用 https 或 http://localhost / http://127.0.0.1 访问，而非 http 私网 IP。
        </div>
      )}

      <div className="mt-4 flex items-center gap-3">
        <button
          onClick={start}
          disabled={ready || starting}
          className="rounded bg-[var(--brand-solid,#2563eb)] px-4 py-2 font-semibold text-white disabled:opacity-50"
        >
          {ready ? "已启动" : starting ? "启动中…" : "启动节点"}
        </button>
        <span>
          本机 node id：<b>{nodeId ?? "（未启动）"}</b>
        </span>
      </div>

      <Section title="① 连接（拨桌面的 ws / webrtc-direct 地址）">
        <input
          className="w-full rounded border border-fd-border bg-transparent p-2"
          placeholder="/ip4/192.168.x.x/tcp/xxxx/ws/p2p/12D3Koo... 或 .../webrtc-direct/certhash/.../p2p/..."
          value={addr}
          onChange={(e) => setAddr(e.target.value)}
        />
        <div className="mt-2 flex gap-2">
          <Btn onClick={doConnect} disabled={!ready}>connect</Btn>
          <Btn onClick={doReserve} disabled={!ready}>reserve（circuit listen）</Btn>
          <Btn onClick={doDropRelay} disabled={!ready}>drop relay</Btn>
        </div>
      </Section>

      <Section title="② 粘贴邀请（受邀方：连桌面/移动生成的配对邀请）">
        <div className="flex gap-2">
          <input
            className="flex-1 rounded border border-fd-border bg-transparent p-2"
            placeholder="sdinvite..."
            value={invite}
            onChange={(e) => setInvite(e.target.value)}
          />
          <Btn onClick={doInvite} disabled={!ready}>配对（消费邀请）</Btn>
        </div>
      </Section>

      <Section title="②′ 展示邀请（发起方：让桌面扫码/粘贴来配对本机）">
        <p className="text-xs text-fd-muted-foreground">
          浏览器不 listen 本地 socket，桌面要拨得到本机需先在①填桌面 ws 地址并 reserve（拿 circuit 可达地址），再生成邀请。
        </p>
        <div className="mt-2 flex gap-2">
          <Btn onClick={reserveHelper} disabled={!ready || reserving}>
            {reserving ? "reserve 中…" : "① 先 reserve（用①的地址）"}
          </Btn>
          <Btn onClick={showInvite} disabled={!ready}>② 生成邀请</Btn>
        </div>
        {myInvite && (
          <textarea
            readOnly
            value={myInvite}
            onFocus={(e) => e.currentTarget.select()}
            className="mt-2 h-24 w-full break-all rounded border border-fd-border bg-fd-card/50 p-2 text-xs"
          />
        )}
      </Section>

      {pairingReqs.length > 0 && (
        <Section title="⚠ 配对请求（桌面消费了本机邀请，等你确认）">
          {pairingReqs.map((r) => (
            <div key={r.pendingId} className="my-1 rounded border border-fd-border bg-fd-card/50 p-2">
              <b>{r.deviceName}</b> 请求与本机配对
              <div className="text-xs text-fd-muted-foreground">{r.peerId}</div>
              <div className="mt-1 flex gap-2">
                <Btn onClick={() => respondPairing(r.pendingId, true)}>接受配对</Btn>
                <Btn onClick={() => respondPairing(r.pendingId, false)}>拒绝</Btn>
              </div>
            </div>
          ))}
        </Section>
      )}

      <Section title="③ 发送文件">
        <input
          className="w-full rounded border border-fd-border bg-transparent p-2"
          placeholder="对端 node id（base58）"
          value={peer}
          onChange={(e) => setPeer(e.target.value)}
        />
        <input ref={filesRef} type="file" multiple className="mt-2 block w-full" />
        <div className="mt-2">
          <Btn onClick={doSend} disabled={!ready}>send</Btn>
        </div>
      </Section>

      <Section title="④ 收到的 Offer">
        {offerList.length === 0 && <p className="text-fd-muted-foreground">（暂无）</p>}
        {offerList.map((o) => (
          <div key={o.sessionId} className="my-1 rounded border border-fd-border bg-fd-card/50 p-2">
            <b>{o.who}</b> 发来 {o.files.length} 个文件（{o.totalSize} 字节）
            <div className="text-xs text-fd-muted-foreground">{o.files.map((f) => f.relativePath).join(", ")}</div>
            <div className="mt-1 flex gap-2">
              <Btn onClick={() => acceptOffer(o.sessionId)}>接受</Btn>
              <Btn onClick={() => rejectOffer(o.sessionId)}>拒绝</Btn>
            </div>
          </div>
        ))}
      </Section>

      <Section title="⑤ 传输">
        {xferList.length === 0 && <p className="text-fd-muted-foreground">（暂无）</p>}
        {xferList.map((x) => (
          <div key={x.sessionId} className="my-1 rounded border border-fd-border bg-fd-card/50 p-2">
            <div className="text-xs">{x.sessionId}</div>
            <div className="mt-1 h-1.5 overflow-hidden rounded bg-fd-border">
              <div className="h-full bg-[var(--brand-solid,#2563eb)]" style={{ width: `${x.pct}%` }} />
            </div>
            <div className="mt-1 text-xs">{x.status}</div>
            {x.downloads.map((d) => (
              <a key={d.name} href={d.url} download={d.name} className="mt-1 block text-[var(--brand,#2563eb)] underline">
                下载 {d.name}
              </a>
            ))}
          </div>
        ))}
      </Section>

      <h2 className="mt-6 border-b border-fd-border pb-1 font-semibold">日志</h2>
      <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded bg-fd-card/50 p-3 text-xs">
        {logLines.join("\n")}
      </pre>
    </main>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-5">
      <h2 className="mb-1 border-b border-fd-border pb-1 font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function Btn({ onClick, disabled, children }: { onClick: () => void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="rounded border border-fd-border px-3 py-1.5 hover:bg-fd-accent disabled:opacity-50"
    >
      {children}
    </button>
  );
}

function errStr(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return `[${(e as { kind?: string }).kind ?? "err"}] ${(e as { message?: string }).message}`;
  }
  return String(e);
}
