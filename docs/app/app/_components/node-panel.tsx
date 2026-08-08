"use client";

// 设置页的「设备信息」卡 —— **对齐移动端 `mobile/src/components/device-info-card.tsx`**，
// 那是这张卡三端里最新的一版，它自己的注释写明是「对齐桌面端 DeviceInfoSection」并改了两处：
//
//   · **不用英雄头像**（桌面是品牌色圆角块 + 姓名首字母）。平台图标本身即身份，
//     再套一层带色的头像只是装饰。
//   · **底部指标降权成一行小字**，不做大数字仪表盘。桌面那版是「大数字 + 小标签 + 三格分隔」，
//     正是 impeccable 明令禁止的 hero-metric 模板。
//
// 形态：图标 chip（带在线点）+ 设备名（点铅笔就地改）+ PeerId（点一下复制）/ 一行指标 / 诊断折叠。
//
// ## 指标三项与另外两端不同，因为浏览器没有那两个概念
//
// 桌面/移动是「已连接 / 配对 / NAT」。前两项 Web 现在也有了；第三项换掉：
//
//   已连接  `connected_peers()` —— **2026-08-05 新加的 wasm 绑定**，与桌面/移动的
//           `NetworkStatus.connected_peers` 是同一个函数（core 的 `connected_count()`，
//           按 `is_swarmdrop_agent` 过滤，bootstrap/relay 不算在内）。此前 Web 没有这个
//           导出，只能拿「已配对设备里在线的台数」凑数，而那是 presence 快照，
//           未配对的对端不在里面。
//   已配对  总台数
//   可达    有没有 circuit 预留 —— **这是浏览器版的 NAT 状态**：对方能不能拨到我
//
// **NAT 那项没有搬，也搬不了**：`Endpoint::watch_nat()` 的唯一写入点是 autonat 事件，
// 而 autonat 是 native-only（`crates/net/src/actor.rs` 的 `WatchSenders::nat` 上就挂着
// `cfg_attr(wasm_browser, expect(dead_code))`），wasm 下它恒为 `Unknown`。
// 这与「浏览器有没有 WebRTC」无关——浏览器压根不监听端口，「我公网可拨吗」这个问题
// 对它不成立。摆一个永远不变的「NAT: 未知」是拿占位冒充状态。
//
// ## 设备名的那段说明只在编辑态出现
//
// 另外两端没有这段说明（只有一支铅笔）。但「对端看到的就是这一行」这件事对 Web 用户更需要说
// ——他们多半是点开一条邀请链接才第一次见到这个应用。折中：静息态与另外两端一样干净，
// 说明在**按下铅笔那一刻**出现，那正是它有用的时刻。

import { Trans, useLingui } from "@lingui/react/macro";
import { Activity, Check, Copy, Globe, MonitorSmartphone, Pencil, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { DEVICE_NAME_MAX_CHARS, shortPeerId } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { renameDevice } from "../_lib/node-runtime";
import { IDENTITY_LOCATION, selectReservation, useWebNode, webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { useCopyToClipboard } from "../_lib/use-copy";
import { Disclosure } from "./disclosure";
import { NodeStatusDialog } from "./node-status-dialog";
import { SettingsCard, SettingsSection } from "./settings-primitives";
import { WebErrorCard } from "./web-error-view";

export function NodePanel() {
  const { t } = useLingui();
  const nodeId = useWebNode((s) => s.nodeId);
  const deviceName = useWebNode((s) => s.deviceName);
  const deviceNameFallback = useWebNode((s) => s.deviceNameFallback);
  const nodeRunning = useWebNode((s) => s.status === "running");
  const pairedCount = useWebNode((s) => s.pairedDevices.length);
  const connectedPeers = useWebNode((s) => s.connectedPeers);
  const reachable = useWebNode(selectReservation) !== null;

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const saveAction = useAsyncAction();
  const { state: copyState, copy } = useCopyToClipboard();

  const displayName = deviceName ?? deviceNameFallback ?? "—";

  const startEdit = () => {
    setDraft(deviceName ?? "");
    setEditing(true);
  };

  const save = () => {
    // 两道闸，缺一条都会出事：
    //
    // 1. **重入**。`save()` 置 pending → 重渲染让 `<Input disabled>` 生效，而它此刻正持有
    //    焦点；HTML 的 focus fixup 会 unfocus 并派发 blur → `onBlur={save}` 再跑一遍。
    //    `useAsyncAction.run` 的 seq 只丢弃**回调**，不拦调用本身，所以 `rename_device`
    //    会实打实发两次。按 Enter 必中。
    // 2. **脏检查**。没改名字也走一次完整往返——内核会把新 `agent_version` 逐连接下发给
    //    每一个已连接对端——然后弹一句「设备名称已更新」。什么都没变却告诉用户变了。
    //    它同时是触摸端的安全网：手机软键盘没有 Esc，误触铅笔后唯一的出路就是失焦，
    //    有这道闸那次失焦才是无害的。
    if (saveAction.pending) return;
    const trimmed = draft.trim();
    if (trimmed === (deviceName ?? "")) {
      setEditing(false);
      return;
    }
    saveAction.run(
      // 空串 = 清空，回落到 UA 派生的默认名（与桌面「清空回退 hostname」同义）。
      // 返回的是内核归一化后的名字（`DeviceName::parse` 会 trim、剥控制字符与 `;`、
      // 截断到 40 个 char），展示的必须是它而不是 draft。
      () => renameDevice(trimmed || null),
      (saved) => {
        webNodeActions.setDeviceName(saved);
        setEditing(false);
        // 走 toast 而不是卡内一行绿字，与另外两端一致，也免疫「切页后这条反馈就没了」
        // ——用户改完名最自然的下一步是回设备页。
        toast.success(t`设备名称已更新`, {
          description: nodeRunning
            ? t`已连接的对端立刻就能看到新名字，不必刷新页面。`
            : t`节点启动后，新名字会随本机身份一起广播出去。`,
        });
      },
    );
  };

  return (
    <SettingsSection
      icon={MonitorSmartphone}
      title={<Trans>设备信息</Trans>}
      aside={<NodeStatusDialog />}
    >
      <SettingsCard>
        <div className="flex items-center gap-4 p-4">
          {/* 中性平台图标 chip + 在线点（同移动端：不用带色的英雄头像）。
              Web 的「平台」就是浏览器，所以是 `Globe` 而不是某个操作系统图标。 */}
          <div className="relative shrink-0">
            <span
              className={`absolute -top-0.5 -left-0.5 z-10 size-3.5 rounded-full border-2 border-background ${
                nodeRunning ? "bg-success" : "bg-muted-foreground"
              }`}
              aria-hidden
            />
            <div className="flex size-14 items-center justify-center rounded-full bg-muted">
              <Globe className="size-6 text-foreground" aria-hidden />
            </div>
          </div>

          <div className="flex min-w-0 flex-1 flex-col gap-1">
            {editing ? (
              <div className="flex items-center gap-1">
                <Input
                  className="h-11 px-1 text-base font-bold sm:h-9"
                  value={draft}
                  maxLength={DEVICE_NAME_MAX_CHARS}
                  placeholder={deviceNameFallback ?? ""}
                  aria-label={t`设备名`}
                  autoFocus
                  onChange={(e) => setDraft(e.target.value)}
                  // 失焦即保存（同另外两端），Esc 放弃。`save()` 自带脏检查与重入闸，
                  // 所以「点开又什么都没改」的失焦是无害的 no-op。
                  onBlur={save}
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
                {/*
                  取消按钮：**手机软键盘没有 Esc**，而这一端的基线视口就是手机——
                  改了一半想反悔，没有这颗按钮就只剩「失焦 = 保存」一条路。

                  `onPointerDown` 里 `preventDefault()` 是关键：不拦的话点它会先让 input
                  失焦、`onBlur` 抢先把草稿存下去，取消就永远执行不到。
                */}
                <Button
                  size="icon-sm"
                  variant="ghost"
                  aria-label={t`取消重命名`}
                  onPointerDown={(e) => e.preventDefault()}
                  onClick={() => setEditing(false)}
                  disabled={saveAction.pending}
                  className="shrink-0 text-muted-foreground"
                >
                  <X className="size-4" aria-hidden />
                </Button>
              </div>
            ) : (
              <button
                type="button"
                onClick={startEdit}
                // `aria-label` 里带上名字本身：光写「编辑设备名」会把按钮内容盖掉，
                // 而这张卡上再没有第二处显示设备名——读屏用户将永远听不到它。
                aria-label={t`编辑设备名：${displayName}`}
                // `-my-2 py-2`：命中区撑到 ≥44px 而不改变视觉位置。裸 <button> 绕开了
                // `components/ui/button.tsx` 里那条「每一档 min-h-11 起」的移动基线，
                // 这里得自己补上。
                className="focus-ring -my-2 flex min-w-0 items-center gap-1.5 self-start rounded py-2 text-left"
              >
                <span className="truncate text-base font-bold text-foreground">{displayName}</span>
                {!deviceName && deviceNameFallback && (
                  <span className="shrink-0 text-xs font-normal text-muted-foreground">
                    <Trans>（浏览器默认）</Trans>
                  </span>
                )}
                {/* 铅笔常驻可见，不做 hover 显隐——这一端的基线视口是手机，没有 hover。 */}
                <Pencil className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
              </button>
            )}

            {/* PeerId：点一下复制。短形式取共享包的 `shortPeerId`（`12D3…UvWxYz`），
                不在这里再写一份截断规则。 */}
            <button
              type="button"
              onClick={() => nodeId && void copy(nodeId)}
              disabled={!nodeId}
              // 同上：带上值本身，否则读屏只听得到「复制节点 ID」，听不到 ID 是什么。
              aria-label={nodeId ? t`复制节点 ID：${nodeId}` : t`复制节点 ID`}
              className="focus-ring -my-2 flex min-w-0 items-center gap-1.5 self-start rounded py-2 text-left text-muted-foreground transition-colors hover:text-brand disabled:hover:text-muted-foreground"
            >
              <Activity className="size-3.5 shrink-0" aria-hidden />
              <span className="truncate font-mono text-[13px] tabular-nums">
                {nodeId ? shortPeerId(nodeId) : "—"}
              </span>
              {/* 失败态也要有说法：非安全上下文下 `navigator.clipboard` 是 undefined，
                  按钮不能一动不动地骗人（见 `use-copy.ts`）。
                  **播报走按钮外的 aria-live**（见下方）——渲染在按钮内部会被 aria-label 盖掉。 */}
              {copyState === "copied" ? (
                <Check className="size-3.5 shrink-0 text-success-ink" aria-hidden />
              ) : copyState === "failed" ? (
                <span className="shrink-0 text-[11px] text-destructive-ink" aria-hidden>
                  <Trans>复制失败</Trans>
                </span>
              ) : (
                <Copy className="size-3.5 shrink-0" aria-hidden />
              )}
            </button>

            {/* 复制结果的播报点。**必须在按钮外面**：按钮带 `aria-label`，它会盖掉全部
                后代文本，渲染在里面的「已复制 / 复制失败」读屏永远听不到。 */}
            <span className="sr-only" role="status" aria-live="polite">
              {copyState === "copied" ? t`已复制节点 ID` : copyState === "failed" ? t`复制失败` : ""}
            </span>
          </div>
        </div>

        {/* 编辑态才出现的说明：静息时这张卡与另外两端一样干净，见文件头。 */}
        {editing && (
          <p className="border-t px-4 py-2.5 text-xs leading-5 text-muted-foreground">
            {deviceNameFallback ? (
              <Trans>
                对端在配对确认与传输请求里看到的就是这一行。清空即回落到浏览器 UA 派生的默认名
                （当前是「{deviceNameFallback}」）。
              </Trans>
            ) : (
              <Trans>
                对端在配对确认与传输请求里看到的就是这一行。清空即回落到浏览器 UA 派生的默认名。
              </Trans>
            )}
          </p>
        )}
        {saveAction.error && (
          <div className="border-t px-4 py-2.5">
            <WebErrorCard error={saveAction.error} className="text-xs" />
          </div>
        )}

        {/* 底部指标：一行小字，不是仪表盘。三项的选取理由见文件头。 */}
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t px-4 py-2.5">
          <Metric label={<Trans>已连接</Trans>} value={String(connectedPeers)} />
          <Metric label={<Trans>已配对</Trans>} value={String(pairedCount)} />
          <Metric
            label={<Trans>可达</Trans>}
            value={reachable ? t`是` : t`否`}
          />
        </div>

        {/* 「身份存在哪儿」是**诊断**不是设置：它回答的是「我刷新后还是我吗」。
            这条是 Web 独有的（另外两端的身份在系统钥匙串里，没有这个疑问）。

            两处与此前不同，都不是为了好看：

            · **标题是名词短语，答案摆在收起态的右边。** 原来它是一句问句
              「身份存在哪里？」——这张卡上唯一的疑问式标签——而答案只是一个 API 名，
              却非点开不可。折叠该藏的是**后果**（清掉浏览器数据会换身份），
              那才是需要一段话的部分；一个词不该收门票。
            · **走 `Disclosure` 而不是裸 `<details>`。** 裸的那版顶着浏览器默认的 ▶，
              命中区只有那行 12px 文字高，且 `py-2.5` 比上面的指标条还窄，
              整张卡的下缘看着像少写了点什么。理由见 disclosure.tsx 的文件头。 */}
        <Disclosure className="border-t" label={<Trans>身份存储</Trans>} value={IDENTITY_LOCATION}>
          <p className="text-xs leading-5 text-muted-foreground">
            <Trans>刷新后保持不变。清除浏览器数据会让本机换一个新身份，已配对的设备需要重新配对。</Trans>
          </p>
        </Disclosure>
      </SettingsCard>
    </SettingsSection>
  );
}

/** 底部指标项：标签在前、值加粗，同移动端 `Metric`。 */
function Metric({ label, value }: { label: React.ReactNode; value: string }) {
  return (
    <span className="text-[13px] text-muted-foreground">
      {label} <span className="font-semibold tabular-nums text-foreground">{value}</span>
    </span>
  );
}
