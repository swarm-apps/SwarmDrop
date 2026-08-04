"use client";

// 连接：浏览器怎么被别人找到。
//
// ## 浏览器不监听端口，所以「可达」要自己建
//
// 桌面/移动会 listen 本地 socket，对端直接拨得到。浏览器不行，必须先经一个中继节点建立
// circuit 可达地址，那个地址才是邀请里写给对方的东西。这块的全部内容都在服务这一件事。
//
// ## 清单来自内核，不是本地记的（2026-08-04 改）
//
// 此前这里只有一个文本框 + 三个按钮，且「撤销」用的是 `helperIdRef`——**只记手动登记的
// 那一次**。而 `web-node-bootstrap` 启动时就把配置里的全部 helper 自动 ensure 了一遍，
// 那些条目用户既看不到、也撤不掉：填错地址后后台会一直退避重试，界面上一片安静。
//
// 清单来自 store 的 `relays` 域，由 layout 的 `startRelayWatch` 单点订阅 `relays_changed()`
// 写入（两条 wasm 早就导出、前端一直零调用）。**订阅不在这个面板里**：设备页的配对区也要读
// 同一份事实（判断能不能生成邀请），绑在某一页上会让直接进设备页的用户看到一个永远禁用的
// 「生成邀请」——见 `_lib/relay-watch.ts` 的头注释。于是清单里手动与自动登记的一视同仁，
// 每条都能看到状态、失败原因，也都能撤。
//
// 连接类型（局域网/打洞/中继）是真实状态，要像安全工具一样清楚可视化（PRODUCT.md 原则 2），
// 配色沿用桌面 device-card 的语义编码（DESIGN.md：这三色是状态语义编码，在 one-accent 规则之外）。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { Radio } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getNode } from "../_lib/node-runtime";
import { NAV } from "../_lib/nav";
import { WEB_RELAY_HELPERS } from "../_lib/relay-helpers";
import { useAsyncAction } from "../_lib/use-async-action";
import { useKeyedAsyncAction } from "../_lib/use-keyed-async-action";
import { selectReservation, useWebNode, webNodeActions } from "../_lib/store";
import type { PathKindJson, RelayStateKind } from "../_lib/view-types";
import { SectionHeader, SectionShell } from "./section";
import { StatusDot } from "./status-dot";
import { WebErrorCard } from "./web-error-view";

const PATH_META: Record<PathKindJson, { label: MessageDescriptor; dot: string }> = {
  local: { label: msg`局域网直连`, dot: "bg-emerald-500" },
  direct: { label: msg`打洞直连`, dot: "bg-sky-500" },
  relayed: { label: msg`中继`, dot: "bg-amber-500" },
};

const RELAY_META: Record<RelayStateKind, { label: MessageDescriptor; dot: string }> = {
  connecting: { label: msg`连接中`, dot: "bg-amber-500" },
  active: { label: msg`可达`, dot: "bg-emerald-500" },
  failed: { label: msg`连接失败`, dot: "bg-destructive" },
};

export function ConnectionPanel() {
  const { t } = useLingui();
  const nodeStatus = useWebNode((s) => s.status);
  const connection = useWebNode((s) => s.connection);
  // relay 清单由 layout 的 `startRelayWatch` 单点订阅并写进 store——**订阅不在这个面板里**，
  // 因为设备页的配对区也要读它（见 relay-watch.ts 的头注释）。这里只是它的一个视图。
  const relays = useWebNode((s) => s.relays);
  const activeCircuit = useWebNode(selectReservation);

  const [addr, setAddr] = useState("");
  const connectAction = useAsyncAction();
  const addAction = useAsyncAction();
  const dropAction = useKeyedAsyncAction();

  const ready = nodeStatus === "running";


  const doConnect = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    connectAction.run(
      () => node.connect(addr.trim()),
      (conn) => webNodeActions.setConnection(conn),
    );
  };

  const doAdd = () => {
    const node = getNode();
    if (!node || !addr.trim()) return;
    addAction.run(
      async () => {
        const helperId = node.relays_ensure(addr.trim());
        // 不 await `until_active`：登记是**常驻意图**，后台会持续收敛。挂在这里等 30 秒
        // 只会让按钮转半天，而清单里那一条已经在显示「连接中」了。
        return helperId;
      },
      () => {
        setAddr("");
        toast.success(t`已添加中继，正在连接…`);
      },
    );
  };

  const doDrop = (helperId: string) => {
    const node = getNode();
    if (!node) return;
    void dropAction.run(helperId, async () => {
      await node.relays_drop(helperId);
      toast.success(t`已移除中继`);
    });
  };

  return (
    <SectionShell>
      <SectionHeader
        icon={Radio}
        title={<Trans>连接</Trans>}
        description={
          <Trans>浏览器不能被直接拨号，需要经中继节点建立一个可达地址，对方才找得到你。</Trans>
        }
      />

      <div className="flex flex-col gap-2">
        <p className="text-sm font-medium text-foreground">
          <Trans>中继节点</Trans>
        </p>

        {relays.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            {ready ? (
              <Trans>还没有中继。默认会自动连接内置的中继，也可以在下面添加自己的。</Trans>
            ) : (
              <Trans>节点启动后会自动连接中继。</Trans>
            )}
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {relays.map((relay) => (
              <li
                key={relay.id}
                className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2"
              >
                <div className="min-w-0 flex-1">
                  <p className="flex items-center gap-1.5 text-xs font-medium text-foreground">
                    <StatusDot
                      colorClass={RELAY_META[relay.state].dot}
                      pulse={relay.state === "connecting"}
                    />
                    {t(RELAY_META[relay.state].label)}
                  </p>
                  {/* 中继身份是机器真值 → mono（Mono Truth Rule）。只露末 8 位：
                      够用来区分两条，完整值在失败诊断里没有额外价值。 */}
                  <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                    …{relay.id.slice(-8)}
                  </p>
                  {relay.lastError && (
                    <p className="mt-0.5 break-words text-[11px] text-destructive-ink">
                      {relay.lastError}
                    </p>
                  )}
                </div>
                {/* 每条都能撤——包括启动时自动登记的那些。此前只有手动登记的那一条撤得掉，
                    自动的既看不见也停不下来，填错地址后后台会一直退避重试。 */}
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => doDrop(relay.id)}
                  disabled={!ready || dropAction.isPending(relay.id)}
                  className="shrink-0 text-xs"
                >
                  {dropAction.isPending(relay.id) ? <Trans>移除中…</Trans> : <Trans>移除</Trans>}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {dropAction.latestError && (
          <WebErrorCard error={dropAction.latestError} className="text-xs" />
        )}
      </div>

      <div className="flex flex-col gap-2 border-t pt-4">
        <p className="text-sm font-medium text-foreground">
          <Trans>添加中继</Trans>
        </p>
        <p className="text-xs text-muted-foreground">
          <Trans>
            粘贴一个中继节点的地址。自建节点的地址形如{" "}
            <code className="font-mono text-[11px]">/ip4/…/udp/…/webrtc-direct/certhash/…/p2p/12D3Koo…</code>
          </Trans>
        </p>
        <Input
          className="h-11 font-mono text-xs sm:h-9"
          placeholder={WEB_RELAY_HELPERS[0] ?? "/ip4/…/p2p/12D3Koo…"}
          aria-label={t`中继节点地址`}
          value={addr}
          onChange={(e) => setAddr(e.target.value)}
          disabled={!ready}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            onClick={doAdd}
            disabled={!ready || !addr.trim() || addAction.pending}
                      >
            {addAction.pending ? <Trans>添加中…</Trans> : <Trans>添加</Trans>}
          </Button>
          {/* 「只连一次看看通不通」是排查动作，与「登记为常驻中继」不是一回事——
              此前两者是并排的两个按钮，标签还一个中文一个裸英文 "connect"。 */}
          <Button
            size="sm"
            variant="outline"
            onClick={doConnect}
            disabled={!ready || !addr.trim() || connectAction.pending}
                      >
            {connectAction.pending ? <Trans>测试中…</Trans> : <Trans>测试连通性</Trans>}
          </Button>
        </div>
        {addAction.error && <WebErrorCard error={addAction.error} className="text-xs" />}
        {connectAction.error && <WebErrorCard error={connectAction.error} className="text-xs" />}
        {connection && (
          <div className="flex items-center gap-2 rounded-lg border bg-background px-3 py-2">
            <StatusDot colorClass={PATH_META[connection.path].dot} />
            <span className="text-xs font-medium text-foreground">
              {t(PATH_META[connection.path].label)}
            </span>
            <span className="truncate font-mono text-xs text-muted-foreground">
              {connection.addr}
            </span>
          </div>
        )}
      </div>

      {activeCircuit && (
        <div className="border-t pt-4">
          <p className="text-xs font-medium text-muted-foreground">
            <Trans>
              你的可达地址（
              <Link href={NAV.devices.href} className="underline underline-offset-2">
                生成邀请
              </Link>
              时会写进去）
            </Trans>
          </p>
          <p className="mt-1 break-all rounded-lg border bg-background px-3 py-2 font-mono text-xs text-foreground">
            {activeCircuit}
          </p>
        </div>
      )}
    </SectionShell>
  );
}
