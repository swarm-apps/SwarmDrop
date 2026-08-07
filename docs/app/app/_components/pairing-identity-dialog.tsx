"use client";

// 配对身份确认的共用外壳 —— 出站（我拿着对方的邀请找过去）与入站（对方拿着我的邀请找上门）
// 共用这一份。
//
// **两半必须长一样**（DESIGN.md 的「Pairing」一节），但「必须长一样」如果只靠两份逐字相同的
// JSX 维持，改一边漏一边不会有任何提示——文案就已经先分叉过一次（『…再接受』/『…再继续』）。
// 骨架收在这里，两个调用点只给内容与按钮语义：
//
// | | 图标 | 描述 | NodeId 下的补充 | 页脚 |
// |---|---|---|---|---|
// | 入站 `PairingRequestHost` | `ShieldCheck` | 「X 想与这台设备配对」 | 无 | 拒绝 / 接受 |
// | 出站 `PairingConfirmDialog` | 对方平台的设备图标 | 「名称 · 平台」 | 有效期 · 仅局域网 | 取消 / 确认配对 |

import { Trans } from "@lingui/react/macro";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { WebErrorCard } from "./web-error-view";
import type { WebError } from "../_lib/view-types";

export function PairingIdentityDialog({
  open,
  onOpenChange,
  icon: Icon,
  title,
  description,
  peerId,
  meta,
  hint,
  error,
  footer,
  trailing,
}: {
  open: boolean;
  /** 关闭意图（Esc / 点遮罩）。**两个方向的语义不同**，所以由调用方决定它意味着什么。 */
  onOpenChange: (open: boolean) => void;
  icon: LucideIcon;
  title: ReactNode;
  description: ReactNode;
  /**
   * 对方的**完整** NodeId。核对身份是这一屏唯一的作用，所以不截断——52 个字符的连续
   * base58 没有断点，靠 `break-all` 换行。
   */
  peerId: string | undefined;
  /** NodeId 下方的补充事实（有效期、仅局域网……）。入站没有，不传即不渲染。 */
  meta?: ReactNode;
  hint: ReactNode;
  error: WebError | null;
  /** 两颗对半分的按钮。语义按方向不同，由调用方给。 */
  footer: ReactNode;
  /** 页脚之后的附加信息（如入站的「还有 N 个请求等待处理」）。 */
  trailing?: ReactNode;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader className="items-center gap-2 text-center sm:text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-[var(--brand-solid)]/12">
            <Icon className="size-6 text-brand" aria-hidden />
          </div>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="rounded-lg bg-muted px-3 py-2.5">
          <p className="text-xs text-muted-foreground">
            <Trans>对方节点 ID</Trans>
          </p>
          <p className="mt-0.5 break-all font-mono text-xs text-foreground">{peerId}</p>
        </div>

        {meta}

        <p className="text-xs text-muted-foreground">{hint}</p>

        {error && <WebErrorCard error={error} className="text-xs" />}

        <DialogFooter className="gap-2 sm:gap-2">{footer}</DialogFooter>

        {trailing}
      </DialogContent>
    </Dialog>
  );
}
