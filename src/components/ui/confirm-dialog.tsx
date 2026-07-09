/**
 * ConfirmDialog
 * 破坏性/需确认操作的通用确认弹窗，收口此前散落各处的 AlertDialog 骨架
 * （标题 + 说明 + 取消/确认 + 危险按钮样式）。调用方只提供内容与 onConfirm。
 *
 * 用法（配合条件挂载，避免在高频重渲染的行里常驻子树）：
 *   {open && (
 *     <ConfirmDialog open onOpenChange={setOpen} title={...} description={...}
 *       confirmLabel={...} onConfirm={...} />
 *   )}
 */

import type { ReactNode } from "react";
import { Trans } from "@lingui/react/macro";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  onConfirm,
  /** 危险操作（默认 true）：确认按钮走 destructive 样式。 */
  destructive = true,
  /** 嵌在可点击行内时阻止冒泡，避免点弹窗触发行的 onClick。 */
  stopPropagation = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description: ReactNode;
  confirmLabel: ReactNode;
  onConfirm: () => void;
  destructive?: boolean;
  stopPropagation?: boolean;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onClick={stopPropagation ? (e) => e.stopPropagation() : undefined}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            <Trans>取消</Trans>
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            className={cn(
              destructive &&
                "bg-destructive text-destructive-foreground hover:bg-destructive/90",
            )}
          >
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
