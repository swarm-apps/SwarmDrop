"use client";

// 首屏节点面板：本机身份 + 状态。node id 是机器真值 → mono + tabular-nums（Mono Truth Rule）。
// 扁平卡片（shadow-xs），不堆装饰。
//
// 设备名也在这里改——它与 node id 同属「本机身份」，只不过是给人看的那一半。
// **Web 端不做首启强制命名引导**：典型入口是别人发来的邀请链接，此时插一个必填步骤，用户还没
// 看到任何价值就先被要求填表；而不改名的后果只是对端列表里出现一行「Chrome」，可读、不阻断、
// 随时可补救。两者代价不对称，故默认值立刻可用，想区分的人自己来这里改。

import { Trans, useLingui } from "@lingui/react/macro";
import { useState } from "react";
import { renameDevice } from "../_lib/node-runtime";
import { IDENTITY_LOCATION, useWebNode, webNodeActions } from "../_lib/store";
import { DEVICE_NAME_MAX_CHARS } from "@swarmdrop/shared-view";
import { useAsyncAction } from "../_lib/use-async-action";
import { NodeStatusPill } from "./node-status-pill";
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
    <div className="rounded-xl border border-fd-border bg-fd-card p-6 shadow-xs">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>本机节点</Trans>
        </h2>
        <NodeStatusPill />
      </div>
      <dl className="mt-4 space-y-3">
        <div>
          <dt className="text-xs font-medium text-muted-foreground">
            <Trans>设备名</Trans>
          </dt>
          <dd className="mt-1 text-sm text-fd-foreground">
            {editing ? (
              <>
                <input
                  className="w-full rounded-lg border border-fd-border bg-fd-background px-3 py-2 text-sm text-fd-foreground placeholder:text-fd-muted-foreground"
                  value={draft}
                  maxLength={DEVICE_NAME_MAX_CHARS}
                  placeholder={deviceNameFallback ?? ""}
                  onChange={(e) => setDraft(e.target.value)}
                  disabled={saveAction.pending}
                />
                <div className="mt-2 flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={save}
                    disabled={saveAction.pending}
                    className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    {saveAction.pending ? <Trans>保存中…</Trans> : <Trans>保存</Trans>}
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditing(false)}
                    disabled={saveAction.pending}
                    className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-muted-foreground hover:bg-fd-accent disabled:opacity-50"
                  >
                    <Trans>取消</Trans>
                  </button>
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
                <button
                  type="button"
                  onClick={startEdit}
                  className="rounded-lg border border-fd-border px-3 py-1.5 text-xs font-medium text-fd-foreground hover:bg-fd-accent"
                >
                  <Trans>修改</Trans>
                </button>
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
              <p className="mt-1.5 text-xs text-emerald-600 dark:text-emerald-400">
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
          <dd className="mt-1 font-mono text-xs tabular-nums break-all text-fd-foreground">
            {nodeId ?? "—"}
          </dd>
        </div>
        <div>
          <dt className="text-xs font-medium text-muted-foreground">
            <Trans>身份持久化</Trans>
          </dt>
          <dd className="mt-1 text-sm text-fd-foreground">
            <span className="font-mono">{t(IDENTITY_LOCATION)}</span>{" "}
            <span className="text-muted-foreground">
              · <Trans>刷新后保持不变</Trans>
            </span>
          </dd>
        </div>
      </dl>
    </div>
  );
}
