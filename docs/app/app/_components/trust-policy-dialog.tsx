"use client";

// 信任级别与收件策略的编辑。
//
// ## 默认策略表只有一份，在内核里
//
// 切换信任级别时的默认策略经 `default_receive_policy()`（wasm 导出）向内核要，前端不算。
// 那张表的权威版本是 Rust 的 `DeviceReceivePolicy::for_trust_level`；桌面与移动此前各抄了
// 一份，两份还长出了不同的「切级别时保留哪些字段」规则，而内核那一份一个都不保留——
// 同一个产品动作三种行为。现在规则收在内核，三端各经自己的 binding 调它。
//
// `default_receive_policy` 是**纯派生**（不碰节点），所以切级别可以就地预览新的默认值，
// 用户看到什么就保存什么，一次保存搞定。
//
// ## Web 不呈现的三组字段（保留原值，不静默清零）
//
// - `allowMcpSendToDevice` / `allowMcpAcceptFromDevice`：MCP server 是桌面能力，浏览器没有。
// - `saveBehavior` / `defaultSaveLocation`：Web 落 OPFS + 收件箱，没有「保存到哪个目录」。
// - `expiresAt`：临时设备的有效期由内核在 `for_trust_level` 里给。
//
// 保存时把当前策略整份摊开再覆盖编辑过的几项——**不要只发编辑过的字段**，那会把上面这些
// 静默重置掉。

import { Trans, useLingui } from "@lingui/react/macro";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { normalizeTrustLevel, organizedDeviceName, TRUST_LEVELS, type TrustLevel } from "@swarmdrop/shared-view";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/cn";
import { defaultReceivePolicy, getNode } from "../_lib/node-runtime";
import { usePreferences } from "../_lib/preferences-store";
import { webNodeActions } from "../_lib/store";
import { useAsyncAction } from "../_lib/use-async-action";
import { toWebError, type Device, type DeviceReceivePolicy, type WebError } from "../_lib/view-types";
import { WebErrorCard } from "./web-error-view";

const BYTES_PER_MB = 1024 * 1024;

export function TrustPolicyDialog({
  device,
  onOpenChange,
}: {
  /** null = 关闭。用「哪台设备」而不是 open 布尔量驱动，省掉一个必须与它同步的状态。 */
  device: Device | null;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useLingui();
  const organization = usePreferences((s) => s.deviceOrganization);
  const action = useAsyncAction();

  const currentLevel = normalizeTrustLevel(device?.trustLevel);
  const [level, setLevel] = useState<TrustLevel>(currentLevel);
  const [policy, setPolicy] = useState<DeviceReceivePolicy | null>(device?.receivePolicy ?? null);
  /** 上一次内核派生失败的原因；成功时清空。 */
  const [deriveError, setDeriveError] = useState<WebError | null>(null);
  /** 派生的轮次，用来丢弃过期结果（见 `changeLevel`）。 */
  const deriveSeq = useRef(0);
  /** 大小上限用 MB 输入。空串 = 不限制；这里存字符串而非数字，才区分得出「空」与「0」。 */
  const [limitMb, setLimitMb] = useState("");

  const peerId = device?.peerId;
  // 每次换设备（或重新打开）都从设备快照重新灌一次草稿，否则会带着上一台的编辑进来。
  useEffect(() => {
    if (!device) return;
    setLevel(normalizeTrustLevel(device.trustLevel));
    setPolicy(device.receivePolicy ?? null);
    setLimitMb(
      device.receivePolicy?.maxTransferBytes
        ? String(Math.ceil(device.receivePolicy.maxTransferBytes / BYTES_PER_MB))
        : "",
    );
    // 换设备属于「转入无动作态」——`run` 的 seq 只覆盖「新一轮顶掉旧一轮」，
    // 不显式作废的话，上一台设备在途的失败会把错误卡带到这一台身上。
    setDeriveError(null);
    action.cancel();
    // 只跟 peerId 走：把 device 整个放进依赖会让每一轮 state-poll 都冲掉用户正在编的草稿。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [peerId]);

  if (!device) return null;

  /**
   * 换信任级别：向内核要该级别的默认策略，**级别与策略一起提交**。
   *
   * **带上当前草稿**——用户显式设过的落点由内核带过去（`blocked` 除外）。不带等于替用户
   * 把落点清掉，而落点为空时自动接收会静默失效（内核那一侧一律退回手动确认）。
   *
   * 两条与「先 setLevel 再异步补策略」的差别，都是实打实的：
   *
   * 1. **派生成功之前不动 level**。否则失败时会留下「级别已变、策略还是旧的」这一对，
   *    而用户可以就这么点保存——`update_policy` 对传入的策略不做钳制，于是存下一台
   *    「已阻止但开关还写着自动接收」的设备。（拦得住：`evaluate_receive_policy` 先判
   *    `trust_level == Blocked` 再看策略，所以那是显示不一致而不是安全缺口。）
   * 2. **序号丢弃过期结果**。连点两个级别时，先发的那次可能后 resolve，把 A 的默认值
   *    盖到 B 上。
   *
   * 派生本身是纯计算、不碰节点；异步只是因为 wasm 模块动态加载（见 `node-runtime.ts`
   * 顶部那条「禁止顶层静态 import wasm」），模块早已就绪时它几乎同 tick resolve。
   */
  const changeLevel = (next: TrustLevel) => {
    const seq = ++deriveSeq.current;
    setDeriveError(null);
    void defaultReceivePolicy(next, policy ?? undefined).then(
      (derived) => {
        if (seq !== deriveSeq.current) return;
        setLevel(next);
        setPolicy(derived);
      },
      (e: unknown) => {
        if (seq !== deriveSeq.current) return;
        setDeriveError(toWebError(e));
      },
    );
  };
  // 上限：非空但非正数/非法算非法输入，**不静默兜底成 0**——0 在内核表示「拒收一切」。
  const trimmedLimit = limitMb.trim();
  const parsedLimit = Number(trimmedLimit);
  const limitInvalid = trimmedLimit !== "" && (!Number.isFinite(parsedLimit) || parsedLimit <= 0);
  // 阻止级别下策略由内核强制，开关全部无意义。
  const policyDisabled = level === "blocked";

  const save = () => {
    const node = getNode();
    if (!node || limitInvalid) return;
    action.run(
      () =>
        node.update_paired_device_policy(
          device.peerId,
          level,
          // 草稿里已经是「内核给的默认值 + 用户的微调」，整份发过去。
          // 传 `undefined` 会让内核再派生一次，把用户刚在这个对话框里调的开关抹掉。
          policy === null
            ? undefined
            : {
                ...policy,
                maxTransferBytes:
                  trimmedLimit === "" ? null : Math.floor(parsedLimit) * BYTES_PER_MB,
              },
        ),
      () => {
        // 内核这条路径不发事件（core 里写明了理由），所以必须自己重取一份快照。
        webNodeActions.setPairedDevices(node.paired_devices());
        onOpenChange(false);
      },
    );
  };

  const setSwitch = (key: "autoAccept" | "requireConfirmation" | "allowDirectories" | "allowRelayAutoAccept") =>
    (value: boolean) =>
      setPolicy((current) => (current ? { ...current, [key]: value } : current));

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            <Trans>信任策略</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>
              「{organizedDeviceName(device, organization)}」发来的文件按这里的设定处置。
              策略只保存在本机，对方看不到。
            </Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-foreground">
              <Trans>信任级别</Trans>
            </span>
            <div role="radiogroup" aria-label={t`信任级别`} className="flex flex-wrap gap-2">
              {TRUST_LEVELS.map((candidate) => (
                <button
                  key={candidate}
                  type="button"
                  role="radio"
                  aria-checked={level === candidate}
                  onClick={() => changeLevel(candidate)}
                  disabled={action.pending}
                  className={cn(
                    "focus-ring min-h-11 rounded-full border px-4 text-xs transition-colors disabled:opacity-50 sm:min-h-9",
                    level === candidate
                      ? "border-transparent bg-[var(--brand-solid)] text-[var(--brand-ink)]"
                      : "text-muted-foreground hover:bg-accent",
                  )}
                >
                  <TrustLevelLabel level={candidate} />
                </button>
              ))}
            </div>
            {deriveError && <WebErrorCard error={deriveError} className="text-xs" />}
          </div>

          {policy && (
            <div className="flex flex-col gap-3">
              <PolicySwitch
                id="auto-accept"
                checked={policy.autoAccept}
                onCheckedChange={setSwitch("autoAccept")}
                disabled={policyDisabled || action.pending}
                label={<Trans>自动接收</Trans>}
                hint={<Trans>无需逐次确认，对方发来即开始接收。</Trans>}
              />
              <PolicySwitch
                id="require-confirmation"
                checked={policy.requireConfirmation}
                onCheckedChange={setSwitch("requireConfirmation")}
                disabled={policyDisabled || action.pending}
                label={<Trans>每次确认</Trans>}
                hint={<Trans>开启后会顶掉「自动接收」——两者同时开时以本项为准。</Trans>}
              />
              <PolicySwitch
                id="allow-directories"
                checked={policy.allowDirectories}
                onCheckedChange={setSwitch("allowDirectories")}
                disabled={policyDisabled || action.pending}
                label={<Trans>允许文件夹</Trans>}
                hint={<Trans>关闭后只接收单个文件，不接收整个目录。</Trans>}
              />
              <PolicySwitch
                id="allow-relay-auto-accept"
                checked={policy.allowRelayAutoAccept}
                onCheckedChange={setSwitch("allowRelayAutoAccept")}
                disabled={policyDisabled || action.pending}
                label={<Trans>中继连接也自动接收</Trans>}
                hint={<Trans>关闭后经中继来的传输一律要确认，即使开了自动接收。</Trans>}
              />

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="max-transfer" className="text-xs font-medium">
                  <Trans>单次上限（MB）</Trans>
                </Label>
                <Input
                  id="max-transfer"
                  inputMode="numeric"
                  value={limitMb}
                  onChange={(event) => setLimitMb(event.target.value)}
                  disabled={policyDisabled || action.pending}
                  placeholder={t`留空表示不限制`}
                  className="h-11 sm:h-9"
                  aria-invalid={limitInvalid}
                />
                {limitInvalid && (
                  <p className="text-xs text-warning-ink">
                    <Trans>请填一个大于 0 的数字，或留空表示不限制。</Trans>
                  </p>
                )}
              </div>
            </div>
          )}

          {action.error && <WebErrorCard error={action.error} className="text-xs" />}
        </div>

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={action.pending}
                      >
            <Trans>取消</Trans>
          </Button>
          <Button onClick={save} disabled={limitInvalid || action.pending}>
            {action.pending ? <Trans>保存中…</Trans> : <Trans>保存</Trans>}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PolicySwitch({
  id,
  checked,
  onCheckedChange,
  disabled,
  label,
  hint,
}: {
  id: string;
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled: boolean;
  label: ReactNode;
  hint: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <Label htmlFor={id} className="text-xs font-medium">
          {label}
        </Label>
        <p className="mt-0.5 text-[11px] text-muted-foreground">{hint}</p>
      </div>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
    </div>
  );
}

/** 与 `device-card.tsx` 的 `TrustLabel` 同一组文案——两处都按枚举 switch 出 `<Trans>`。 */
function TrustLevelLabel({ level }: { level: TrustLevel }) {
  switch (level) {
    case "owned":
      return <Trans>本人设备</Trans>;
    case "temporary":
      return <Trans>临时设备</Trans>;
    case "blocked":
      return <Trans>已阻止</Trans>;
    default:
      return <Trans>协作者</Trans>;
  }
}
