/**
 * 邀请二维码组件
 *
 * SVG 由后端 `invite_qr_svg` 生成（三端统一编码规范：大写 alphanumeric + ECL::M +
 * quiet zone，见 `crates/invite/src/qr.rs`）——前端只负责渲染进白卡。
 *
 * 二维码本体固定深模块 + 白底、不随暗色主题反色（摄像头对反色 QR 识别差）。因此
 * 白卡内一切文字都固定用 neutral 深色，**不能**用 `text-muted-foreground` 这类主题
 * token（暗色主题下会变浅灰，压在白底上不可读）。
 *
 * 码位永远占同一块白卡：等待 / 过期 / 出错都以覆盖层叠在码上，不替换整块内容——
 * 状态可见且无布局跳动（PRODUCT.md「状态诚实可见」）。
 */

import { memo, useEffect, useState } from "react";
import { AlertCircle, Loader2, PowerOff, TimerOff } from "lucide-react";
import type { ReactNode } from "react";
import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import { commands } from "@/lib/bindings";
import { cn } from "@/lib/utils";

/**
 * 码面覆盖态：邀请本身无效（过期 / 出错 / 还没生成），码要留在原位但明确失效。
 *
 * `waiting`（真的在等，转圈）与 `blocked`（等你动手，静态图标）必须分开：把「请先启动
 * 网络节点」也画成转圈，等于告诉用户「正在处理」，而实际上没人在处理。
 *
 * 不带动作——恢复动作统一由页面底部的 CommandDock 承担，一屏只出现一个主动作。
 */
export interface InviteQrOverlay {
  kind: "waiting" | "blocked" | "expired" | "error";
  message: ReactNode;
}

interface InviteQrProps {
  /** 邀请串（小写规范形态）；null 时展示骨架 */
  invite: string | null;
  /** 边长（px），默认 260 */
  size?: number;
  /** 覆盖态；null 表示码有效可扫 */
  overlay?: InviteQrOverlay | null;
  className?: string;
}

type QrState =
  | { status: "loading" }
  | { status: "ok"; svg: string }
  | { status: "error" };

/** 图标按「谁在动」区分：waiting 有人在处理（转圈），blocked 等你动手（静态）。 */
const OVERLAY_ICON = {
  waiting: Loader2,
  blocked: PowerOff,
  expired: TimerOff,
  error: AlertCircle,
} as const;

export const InviteQr = memo(function InviteQr({
  invite,
  size = 260,
  overlay = null,
  className,
}: InviteQrProps) {
  const [state, setState] = useState<QrState>({ status: "loading" });

  useEffect(() => {
    if (invite === null) {
      setState({ status: "loading" });
      return;
    }
    let cancelled = false;
    setState({ status: "loading" });
    commands
      .inviteQrSvg(invite)
      .then((svg) => {
        if (!cancelled) setState({ status: "ok", svg });
      })
      .catch(() => {
        if (!cancelled) setState({ status: "error" });
      });
    return () => {
      cancelled = true;
    };
  }, [invite]);

  // 渲染失败自成一态：把它折进覆盖层，语义和过期/等待共用一套呈现
  const effectiveOverlay: InviteQrOverlay | null =
    state.status === "error"
      ? { kind: "error", message: <Trans>二维码渲染失败</Trans> }
      : overlay;

  return (
    <div
      className={cn(
        "relative isolate flex shrink-0 items-center justify-center rounded-[22px] bg-white p-3",
        "shadow-[0_12px_34px_rgb(15_23_42_/_0.10)] ring-1 ring-slate-900/[0.06]",
        "dark:shadow-[0_16px_44px_rgb(0_0_0_/_0.4)] dark:ring-white/15",
        className,
      )}
      style={{ width: size + 24, height: size + 24 }}
    >
      {state.status === "ok" ? (
        <div
          role="img"
          aria-label={t`配对邀请二维码`}
          className={cn(
            "size-full transition-opacity duration-200 [&>svg]:size-full",
            // 压到扫不动为止：失效的码若还能被读到，对方只会白跑一趟失败流程
            effectiveOverlay !== null && "opacity-[0.09]",
          )}
          // 二维码 SVG 由后端受信任生成（纯几何 path，无脚本），安全内联
          dangerouslySetInnerHTML={{ __html: state.svg }}
        />
      ) : (
        <QrSkeleton />
      )}

      {effectiveOverlay !== null && <QrOverlay {...effectiveOverlay} />}
    </div>
  );
});

/**
 * 骨架：三枚定位角轮廓 + 淡格底，示意「这里将出现一个二维码」。
 * 比居中 spinner 更贴合内容形状，也不会让白卡看起来是坏的。
 */
function QrSkeleton() {
  return (
    <div
      aria-hidden
      className="size-full animate-pulse motion-reduce:animate-none"
    >
      <div className="relative size-full rounded-[10px] bg-[image:repeating-linear-gradient(90deg,rgb(15_23_42_/_0.05)_0_6px,transparent_6px_12px),repeating-linear-gradient(0deg,rgb(15_23_42_/_0.05)_0_6px,transparent_6px_12px)]">
        <FinderCorner className="left-0 top-0" />
        <FinderCorner className="right-0 top-0" />
        <FinderCorner className="bottom-0 left-0" />
      </div>
    </div>
  );
}

function FinderCorner({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        "absolute size-[22%] rounded-[6px] border-[5px] border-slate-900/[0.09]",
        className,
      )}
    />
  );
}

function QrOverlay({ kind, message }: InviteQrOverlay) {
  const Icon = OVERLAY_ICON[kind];
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 rounded-[22px] px-6 text-center">
      <span
        className={cn(
          "flex size-11 items-center justify-center rounded-full",
          kind === "error" ? "bg-red-50 text-red-600" : "bg-slate-100 text-slate-500",
        )}
      >
        <Icon
          className={cn("size-5", kind === "waiting" && "animate-spin motion-reduce:animate-none")}
        />
      </span>
      {/* 白底固定，文字色不接主题 token */}
      <p className="text-sm font-medium text-slate-700">{message}</p>
    </div>
  );
}
