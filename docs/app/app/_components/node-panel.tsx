"use client";

// 首屏节点面板：本机身份 + 状态。node id 是机器真值 → mono + tabular-nums（Mono Truth Rule）。
// 扁平卡片（shadow-xs），不堆装饰。
//
// 设备名也在这里改——它与 node id 同属「本机身份」，只不过是给人看的那一半。
// **Web 端不做首启强制命名引导**：典型入口是别人发来的邀请链接，此时插一个必填步骤，用户还没
// 看到任何价值就先被要求填表；而不改名的后果只是对端列表里出现一行「Chrome」，可读、不阻断、
// 随时可补救。两者代价不对称，故默认值立刻可用，想区分的人自己来这里改。

import { Trans, useLingui } from "@lingui/react/macro";
import { Check, Copy, Fingerprint } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { renameDevice } from "../_lib/node-runtime";
import { IDENTITY_LOCATION, useWebNode, webNodeActions } from "../_lib/store";
import { DEVICE_NAME_MAX_CHARS } from "@swarmdrop/shared-view";
import { useAsyncAction } from "../_lib/use-async-action";
import { useCopyToClipboard } from "../_lib/use-copy";
import { NodeStatusPill } from "./node-status-pill";
import { SectionHeader, SectionShell } from "./section";
import { WebErrorCard } from "./web-error-view";

export function NodePanel() {
  const { t } = useLingui();
  const nodeId = useWebNode((s) => s.nodeId);
  const deviceName = useWebNode((s) => s.deviceName);
  const deviceNameFallback = useWebNode((s) => s.deviceNameFallback);
  /** 决定保存后那句反馈的说法：节点在跑才谈得上「对端已经看到了」。 */
  const nodeRunning = useWebNode((s) => s.status === "running");

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  /** 保存成功的即时反馈；重新进入编辑态即清除。 */
  const [justSaved, setJustSaved] = useState(false);
  const saveAction = useAsyncAction();

  const startEdit = () => {
    setDraft(deviceName ?? "");
    setJustSaved(false);
    setEditing(true);
  };

  const save = () => {
    const trimmed = draft.trim();
    saveAction.run(
      // 空串 = 清空，回落到 UA 派生的默认名（与桌面「清空回退 hostname」同义）。
      // 返回的是内核归一化后的名字（`DeviceName::parse` 会 trim、剥控制字符与 `;`、
      // 截断到 40 个 char），展示的必须是它而不是 draft。
      () => renameDevice(trimmed || null),
      (saved) => {
        webNodeActions.setDeviceName(saved);
        setEditing(false);
        setJustSaved(true);
      },
    );
  };

  return (
    <SectionShell>
      <SectionHeader
        icon={Fingerprint}
        title={<Trans>本机节点</Trans>}
        action={<NodeStatusPill />}
      />
      <dl className="space-y-3">
        <div>
          <dt className="text-xs font-medium text-muted-foreground">
            <Trans>设备名</Trans>
          </dt>
          <dd className="mt-1 text-sm text-foreground">
            {editing ? (
              <>
                <Input
                  className="h-11 sm:h-9"
                  value={draft}
                  maxLength={DEVICE_NAME_MAX_CHARS}
                  placeholder={deviceNameFallback ?? ""}
                  aria-label={t`设备名`}
                  autoFocus
                  onChange={(e) => setDraft(e.target.value)}
                  // 行内编辑要认键盘：Enter 保存、Esc 取消。桌面那份一直有，
                  // 这里此前只能用鼠标点两个按钮。
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      save();
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      setEditing(false);
                    }
                  }}
                  disabled={saveAction.pending}
                />
                <div className="mt-2 flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    onClick={save}
                    disabled={saveAction.pending}
                                      >
                    {saveAction.pending ? <Trans>保存中…</Trans> : <Trans>保存</Trans>}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setEditing(false)}
                    disabled={saveAction.pending}
                                      >
                    <Trans>取消</Trans>
                  </Button>
                </div>
              </>
            ) : (
              <div className="flex flex-wrap items-center gap-2">
                <span className="break-all">{deviceName ?? deviceNameFallback ?? "—"}</span>
                {!deviceName && deviceNameFallback && (
                  <span className="text-xs text-muted-foreground">
                    <Trans>（浏览器默认）</Trans>
                  </span>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  onClick={startEdit}
                                  >
                  <Trans>修改</Trans>
                </Button>
              </div>
            )}
            <p className="mt-1.5 text-xs text-muted-foreground">
              {deviceNameFallback ? (
                <Trans>
                  对端在配对确认与传输请求里看到的就是这一行。未设置时用浏览器 UA 派生的默认名
                  （当前是「{deviceNameFallback}」），清空输入即回落到它。
                </Trans>
              ) : (
                <Trans>
                  对端在配对确认与传输请求里看到的就是这一行。未设置时用浏览器 UA 派生的默认名，
                  清空输入即回落到它。
                </Trans>
              )}
            </p>
            {justSaved && (
              <p className="mt-1.5 text-xs text-success-ink">
                {nodeRunning ? (
                  <Trans>
                    已保存，已连接的对端立刻就能看到新名字——不必刷新页面，连接与进行中的传输都不受影响。
                  </Trans>
                ) : (
                  <Trans>已保存。节点启动后，新名字会随本机身份一起广播出去。</Trans>
                )}
              </p>
            )}
            {saveAction.error && <WebErrorCard error={saveAction.error} className="mt-2 text-xs" />}
          </dd>
        </div>
        <div>
          <dt className="text-xs font-medium text-muted-foreground">
            <Trans>节点 ID</Trans>
          </dt>
          <dd className="mt-1 flex items-start gap-2">
            <span className="min-w-0 flex-1 font-mono text-xs tabular-nums break-all text-foreground">
              {nodeId ?? "—"}
            </span>
            {/* 它是用来跟对方核对身份、贴进 issue 的机器真值——桌面点一下就能复制，
                这里此前只能手动选中一串 52 字符的 base58。 */}
            {nodeId && <CopyNodeIdButton key={nodeId} nodeId={nodeId} />}
          </dd>
        </div>
      </dl>

      {/* 「身份存在哪儿」是**诊断**不是设置：它回答的是「我刷新后还是我吗」，
          属于排查时才需要的事实。收进折叠里，不占设置首屏。 */}
      <details className="border-t pt-3 text-xs">
        <summary className="focus-ring cursor-pointer rounded text-muted-foreground">
          <Trans>身份存在哪里？</Trans>
        </summary>
        <p className="mt-2 text-muted-foreground">
          <span className="font-mono">{t(IDENTITY_LOCATION)}</span>{" "}
          <Trans>· 刷新后保持不变。清除浏览器数据会让本机换一个新身份，已配对的设备需要重新配对。</Trans>
        </p>
      </details>
    </SectionShell>
  );
}

/**
 * 节点 ID 的复制按钮。
 *
 * **自带状态并由 `key` 换代**：复制态若住在父组件、`key` 挂在 DOM 节点上，换代什么也重置
 * 不了（知识库「复制态的换代 key 必须挂在持有状态的组件」）。这里 nodeId 几乎不变，
 * 但形态与 `connection-badge` / `invite-share` 保持一致，免得下一个人照着抄错那两处。
 */
function CopyNodeIdButton({ nodeId }: { nodeId: string }) {
  const { t } = useLingui();
  const { state, copy } = useCopyToClipboard();
  return (
    <Button
      size="sm"
      variant="ghost"
      onClick={() => void copy(nodeId)}
      aria-label={t`复制节点 ID`}
      className="shrink-0 gap-1.5 text-xs"
    >
      {state === "copied" ? (
        <Check className="size-3.5" aria-hidden />
      ) : (
        <Copy className="size-3.5" aria-hidden />
      )}
      {state === "copied" ? (
        <Trans>已复制</Trans>
      ) : state === "failed" ? (
        <Trans>复制失败</Trans>
      ) : (
        <Trans>复制</Trans>
      )}
    </Button>
  );
}
