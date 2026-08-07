/**
 * ClipboardInviteBanner
 *
 * 全局剪贴板邀请提示条（非模态），挂在 `_app` 布局（openspec: pair-deep-link design D5）。
 *
 * ## 为什么是全局而不是只在配对页
 *
 * 用户从别处复制了邀请链接、随手点开 SwarmDrop，落在哪一页是不确定的。早期版本只在
 * `/pairing/input` 检测，等于要求用户先自己找到那一屏 —— 而他之所以需要提示，恰恰是因为
 * 还不知道该去哪。**注意别再在页面内挂第二份 `useClipboardInvite`**：两份实例各读一次剪贴板、
 * 各记一份「已提示过」，同一条邀请会被亮两次。
 *
 * ## 为什么是非模态
 *
 * 深链跑通之后，剪贴板感知退化成兜底路径（链接在终端/纯文本里不可点、或 scheme 注册失效）。
 * 给兜底路径配模态弹窗，打断性与它承担的份量不匹配 —— 而且用户可能只是想复制个链接转发，
 * 弹窗会挡在他和「复制完就走」之间。安全闸仍在：点击后进确认卡，用户确认才发起配对。
 *
 * 自我过滤（不对本机自己发出的邀请亮条）在 `useClipboardInvite` 里做，判据是邀请的
 * `inviter_id` 与本机身份比对。
 */

import { useMemo } from "react";
import { useNavigate } from "@tanstack/react-router";
import { Trans } from "@lingui/react/macro";
import { ClipboardCheck, X } from "lucide-react";
import { useClipboardInvite } from "@/hooks/use-clipboard-invite";
import { useNetworkStore } from "@/stores/network-store";
import { usePairingStore } from "@/stores/pairing-store";
import { useSecretStore } from "@/stores/secret-store";
import { getDeviceIcon } from "@/components/pairing/device-icon";

export function ClipboardInviteBanner() {
  const navigate = useNavigate();
  const isNodeRunning = useNetworkStore((s) => s.status === "running");
  // 本机 id 取身份而非网络状态：身份在首启就有，`networkStatus` 要等节点起来。
  // 这里恰好被 `isNodeRunning` 挡着看不出差别，但两处判据必须同源——否则哪天这个门
  // 一松，剪贴板横幅就会开始抬自己发出去的邀请。同 `pairing-store.previewInvite`。
  const selfPeerId = useSecretStore((s) => s.deviceId);
  const previewInvite = usePairingStore((s) => s.previewInvite);
  const phase = usePairingStore((s) => s.current.phase);

  // 配对流程进行中（正在看确认卡 / 正在请求）时不检测：那时用户手上已经有一条邀请在走，
  // 再抬一条只会打断他。
  //
  // `pairedDevices` 传给 hook 做「已配对过滤」：本组件对 `/pairing/*` 是条件渲染，
  // 点下去必然卸载一次，配对成功后又在 `/devices` 重新挂载 —— 那时剪贴板里那条邀请还在，
  // 没有这道过滤就会提示「刚刚配好的设备想和你配对」。
  // selector 只取稳定引用，派生放 useMemo —— 直接 `.map()` 会让每次渲染都产出新数组，
  // hook 里 `check` 的依赖随之变化 → focus listener 每渲染摘挂一次
  // （同 theme-and-styling.md 的「Zustand selector 与派生数组」）。
  const pairedDevices = useSecretStore((s) => s.pairedDevices);
  const pairedPeerIds = useMemo(
    () => pairedDevices.map((d) => d.peerId),
    [pairedDevices],
  );
  const { detected, dismiss } = useClipboardInvite(
    isNodeRunning && phase === "idle",
    selfPeerId,
    pairedPeerIds,
  );

  if (detected === null) return null;

  const { invite, preview } = detected;
  const DeviceIcon = getDeviceIcon(preview.displayPlatform);
  const deviceName = preview.displayName || preview.peerId.slice(-8);

  return (
    <div className="px-4 pt-3 md:px-5 lg:px-6">
      <button
        type="button"
        onClick={() => {
          dismiss();
          // 先跳再 preview：解码失败时 pairing-store 会 toast 报错，用户停在受邀方屏
          // 正好可以改用粘贴（与深链处理器同一考虑）。
          void navigate({ to: "/pairing/input" });
          void previewInvite(invite);
        }}
        className="flex w-full items-center gap-3 rounded-[16px] border border-brand/30 bg-brand/10 px-4 py-3 text-left text-sm text-foreground transition hover:bg-brand/15"
      >
        <ClipboardCheck className="size-5 shrink-0 text-brand" />
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <DeviceIcon className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">
            <Trans>
              <span className="font-medium">{deviceName}</span> 想和你配对，点此确认
            </Trans>
          </span>
        </span>
        <X
          className="size-4 shrink-0 text-muted-foreground hover:text-foreground"
          onClick={(e) => {
            e.stopPropagation();
            dismiss();
          }}
        />
      </button>
    </div>
  );
}
